# AUDIT DE SÉCURITÉ QUANTA — PÉRIMÈTRE CRYPTOGRAPHIE & GESTION DES CLÉS

Cible : `/Users/alex/Desktop/Quanta`, `quanta-protocol` v3.15.1, protocole TORUS v9.
Rust 1.95. Tous les constats marqués **[PROUVÉ]** sont adossés à un test écrit et
exécuté (14 tests, tous verts). Le code source n'a **pas** été modifié : les tests
ont tourné sur une copie hors dépôt (`/tmp/qaudit`, `CARGO_TARGET_DIR=/tmp/qtarget_crypto`).
Le fichier de preuve est archivé en `/tmp/QUANTA_AUDIT_CRYPTO_POC.rs`.
`git status` du dépôt : propre.

---

## 1) Résumé exécutif

La primitive post-quantique est correcte, mais **ce qu'elle signe ne l'est pas**.

1. **CRIT-1 — La préimage d'autorité de transaction n'est pas injective.**
   `format!("{}:{}:{}:{}:{}:{:?}:{}:{}")` sans préfixe de longueur, avec trois
   champs libres (`id`, `to`, `timestamp`) qui peuvent contenir `:`. J'ai construit
   deux transactions **sémantiquement différentes** (destinataire et montant
   différents) partageant **la même préimage**, donc la **même signature ML-DSA** et
   le **même `tx.hash`**. Test exécuté : deux nœuds partis de la même genèse
   finissent avec **la même chaîne, les mêmes hashs de bloc, et des soldes
   différents** — Bob crédité de 100 QUANTA sur l'un, de 0 sur l'autre. Divergence
   de consensus silencieuse, déclenchable par un pair non privilégié.
2. **HAUT-1 — Déni de service à amplification 10⁴ sur le chemin multisig.**
   `verify_multisig` fait `K × S` vérifications ML-DSA sans aucune borne sur `K`
   ni `S`. Mesuré en **release** : 139,7 µs par vérification ⇒ une enveloppe gossip
   de 10 Mo (le plafond du projet) coûte **≈ 135 s de CPU** au destinataire.
3. **HAUT-2 — La clé qui contrôle les fonds est écrasable/supprimable sans
   authentification.** Le blob ML-DSA vit sous une clé KV unique
   (`INSERT OR REPLACE`), et si la ligne disparaît, `unlock_wallet` **fabrique
   silencieusement une nouvelle identité de fonds** (TOFU) : les fonds deviennent
   irrécupérables sans la phrase de 24 mots.

Ce qui tient : la liaison intrinsèque adresse↔clé, la racine PQ réellement
indépendante d'Ed25519, Argon2id (64 MiB/t=3/p=4, 88 ms/essai mesurés), le
Bech32m, la séparation de domaine des votes de finalité, et `OsRng` partout.

---

## 2) Tableau des constats

| id | sévérité | ancre | une ligne |
|----|----------|-------|-----------|
| **CRIT-1** | CRITIQUE | `p2p/ledger/mod.rs:1481` | Préimage de tx non injective : une signature autorise deux transactions différentes → divergence de soldes à chaîne identique. **[PROUVÉ]** |
| **HAUT-1** | HAUT | `p2p/ledger/validation.rs:178` | `verify_multisig` : `K×S` vérifications ML-DSA non bornées, 10 Mo ⇒ 135 s CPU. **[PROUVÉ]** |
| **HAUT-2** | HAUT | `commands/identity.rs:125` | Blob d'identité PQ absent ⇒ nouvelle clé de fonds fabriquée en silence (TOFU). **[PROUVÉ au niveau DB]** |
| **HAUT-3** | HAUT | `commands/identity.rs:56` / `storage/db.rs:89` | `create_identity`/`restore_from_phrase` écrasent la clé de fonds sans ré-auth ni sauvegarde. **[PROUVÉ]** |
| **MOY-1** | MOYEN | `p2p/ledger/mod.rs:1287` | Feuille Merkle (`\|label=`) porte la même classe d'ambiguïté ; sauve le hash de bloc par accident, pas par conception. **[PROUVÉ]** |
| **MOY-2** | MOYEN | `security/pq_vault.rs:67`,`:215` | Sel Argon2id **non aléatoire**, dérivé de la clé publique ⇒ précalcul ciblé possible avant le vol du fichier, inchangé après changement de mot de passe. **[PROUVÉ]** |
| **MOY-3** | MOYEN | `security/hybrid_crypto.rs:124` | `ctx` FIPS-204 toujours `&[]` : la séparation de domaine native du standard est inutilisée sur les 4 usages. **[lecture]** |
| **MOY-4** | MOYEN | `security/pq_vault.rs:119` | Aucune liaison entre le blob de vault et l'identité attendue : substitution intégrale acceptée sans alerte. **[PROUVÉ]** |
| **MOY-5** | MOYEN | `commands/identity.rs:178`,`:475` | La phrase de 24 mots et la clé Ed25519 sortent en `String` nues (heap + IPC JSON), hors zeroize. **[lecture]** |
| **MOY-6** | MOYEN | `p2p/ledger/validation.rs:73` | `tx.signature` est exigée non vide mais **jamais vérifiée** : champ malléable lié au hash de bloc. **[lecture]** |
| **BAS-1** | BAS | `security/address.rs:213` | `parse()` accepte l'hex brut sans checksum : la protection Bech32m est contournée par tout intégrateur qui colle de l'hex. **[lecture]** |
| **BAS-2** | BAS | `commands/identity.rs:28` | Mot de passe minimum 8 caractères ; Argon2id compense mais un mot de passe de dictionnaire tombe en ~1 jour/cœur. **[PROUVÉ, mesuré]** |
| **BAS-3** | BAS | `security/cipher.rs:16` | Paramètres Argon2id figés en dur, sans champ de version persisté : pas de rétrogradation possible, mais pas d'évolution possible non plus. **[lecture]** |

---

## 3) Développement par constat

### CRIT-1 — La préimage d'autorité de transaction n'est pas injective

**Ce qui est faux.** `p2p/ledger/mod.rs:1471-1482` :

```rust
fn tx_signing_preimage(id, from, to, amount, ts, tx_type, nonce, pq_pk) -> String {
    format!("{}:{}:{}:{}:{}:{:?}:{}:{}", id, from, to, amount, ts, tx_type, nonce, pq_pk)
}
```

Cette chaîne est (a) la donnée signée en ML-DSA-65, (b) la préimage du `tx.hash`
(`mod.rs:1518`), (c) recalculée à l'identique par `verify_tx`
(`validation.rs:102-122`). Aucun préfixe de longueur, aucune étiquette de domaine.

Cinq des huit champs sont contraints — `from` est forcé à 32 octets hex par la
liaison (`validation.rs:95`), `amount` et `nonce` sont des `u64` en décimal
canonique, `tx_type` est un nom d'énuméré, `pq_pk` est l'hex de la clé ML-DSA
(3904 caractères). Mais **`id`, `to` et `timestamp` sont des `String` arbitraires
venant du JSON de gossip**, et peuvent contenir `:`. Le nombre de deux-points
absorbés par ces trois champs est le seul degré de liberté nécessaire : on déplace
les frontières de champs sans changer un seul octet.

**Chemin d'exploitation concret.**

*Qui.* N'importe quel pair du réseau. Il lui faut seulement une identité gossip
(gratuite : `generate_pq_identity`) et voir passer la transaction de la victime —
ce que le gossip garantit par construction.

*Comment.* La victime signe un virement de 100 QUANTA vers Bob. La préimage est

```
tx_42:<from>:<bob>:100000000:2026-07-25T10:51:00.123456789+00:00:Transfer:7:<pk>
```

L'attaquant **ne casse rien** : il ré-étiquette. Il pose `to =
"<bob>:100000000:2026-07-25T10"`, `amount = 51`, `timestamp =
"00.123456789+00:00"`. La chaîne concaténée est **identique octet pour octet**.
Donc la signature ML-DSA de la victime la valide, `tx.hash` est le même, et
`verify_tx` renvoie `Ok(true)` sur les deux.

*Ce qu'il gagne.* Le `to` mutant n'est pas une adresse valide (il contient des
`:`), donc personne ne pourra jamais le dépenser — `verify_tx` exige `from` = 64
hex. L'attaquant ne **vole** pas : il **détruit et divise**. Deux effets, tous
deux prouvés :

1. **Censure définitive du paiement.** `seen_tx_hashes` est indexé sur `tx.hash`
   (`mod.rs:1158`). La variante qui arrive la première consomme le slot ; l'autre
   est rejetée comme doublon **pour toujours**. Le bénéficiaire légitime n'est
   jamais crédité, et le payeur ne peut pas rejouer sa transaction (même hash).
2. **Divergence de consensus à chaîne identique.** `integrate_remote_block`
   (`reorg.rs:345-359`) apparie le mempool et le bloc **par `tx.hash`** : le nœud
   qui détenait la mutation la retire de `pending`, puis **saute**
   `cache_apply_tx` pour la transaction honnête du bloc
   (`if !pending_tx_hashes.contains(&tx.hash)`). Son `balance_cache` conserve
   l'effet de la mutation, alors que sa chaîne contient la transaction honnête.
   Les deux nœuds ont le même tip, le même hash de bloc, et des soldes différents.

**Preuve (exécutée).**

```
test zz_audit_crypto::c01_tx_preimage_collision_one_signature_two_transactions ... ok
test zz_audit_crypto::c01b_mutant_censors_the_honest_payment_forever ... ok
test zz_audit_crypto::c07_identical_chains_divergent_balances ... ok
[C-07] integrate_remote_block => Ok(true)
[C-07] A tip = 57f79582ddd3d657c7d3fd871cd77fca939946fc5c60efea16b04ad67c171be7
[C-07] bob: A=100000000 C=0 | trou noir: A=0 C=51
```

`c07` construit deux `Ledger` sur la **même** genèse, applique la transaction
honnête à l'un et la mutation à l'autre, fait sceller un bloc par le premier et
l'intègre chez le second : `integrate_remote_block` renvoie `Ok(true)` — pas un
avertissement, pas une divergence détectée — et les soldes divergent
définitivement. Chiffré : 100,000000 QUANTA volatilisés par transaction attaquée,
et un état de compte irréconciliable jusqu'au prochain `rebuild_cache`
(c'est-à-dire un redémarrage complet).

**Portée exacte.** Cette classe de défaut touche identiquement :
- l'autorité de transaction mono-clé (`validation.rs:102`) ;
- l'autorité **multisig** (`validation.rs:164`, où le champ `pq_pk` vaut le tag
  `"msig1"` — la preuve `c09(f)` construit la collision) ;
- le hash de tx, donc l'anti-rejeu, donc l'appariement mempool↔bloc.

**Correctif.** Encodage **injectif** : étiquette de domaine + préfixe de longueur
`u64` par champ, exactement comme `sm/finality_vote.rs:256-266` le fait déjà
correctement. Second verrou indépendant : valider `to` à l'admission (64 hex ou
`BURN`/`STAKE`/`NETWORK`/`ESCROW`) — aujourd'hui `cache_apply_tx`
(`mod.rs:553-554`) crédite **n'importe quelle chaîne de caractères**.

---

### HAUT-1 — DoS à amplification sur `verify_multisig`

**Ce qui est faux.** `p2p/ledger/validation.rs:178-185` :

```rust
let valid_signers = keys.iter()
    .filter(|pk| sigs.iter().any(|sig| CryptoEngine::verify_pq(pk, payload.as_bytes(), sig)))
    .count();
```

`keys` vient de `auth.pubkeys` et `sigs` de `auth.signatures`, tous deux
désérialisés depuis le JSON attaquant porté par `tx.pq_signature`. **Aucune borne**
sur leur cardinalité. Avec des signatures invalides de longueur correcte, le
`any()` ne court-circuite jamais : le coût est exactement `K × S` vérifications
ML-DSA-65.

**Chemin d'exploitation concret.** L'attaquant choisit lui-même la politique
`{keys, threshold}` — l'adresse `from` en est une fonction pure
(`security/mod.rs:116`), donc il passe trivialement les étapes (1) et (2) de
`verify_multisig` et atteint la boucle. Il emballe la transaction dans un
`GossipMessage::BroadcastTx`. Le plafond d'enveloppe est de 10 Mo
(`dispatcher.rs:100`). Une clé ML-DSA-65 fait 3904 caractères hex, une signature
6618 : l'optimum `K·3904 = S·6618 = 5 Mo` donne **K ≈ 1281, S ≈ 756**, soit
**967 617 vérifications**.

**Preuve (exécutée, profil release).**

```
[C-05] coût moyen d'une vérification ML-DSA-65 invalide : 139.7 µs
[C-05] pire cas dans 10 Mo : 1281 clés × 756 signatures = 967617 vérifications ≈ 135 s CPU
```

**Impact chiffré.** 10 Mo émis ⇒ **135 s de CPU** consommées chez chaque
destinataire : facteur d'amplification ≈ 10⁴ en temps·octet. Le rate-limit gossip
plafonne à 120 messages/fenêtre (`dispatcher.rs:37`) : un seul attaquant peut donc
réclamer jusqu'à **4,5 heures de CPU par fenêtre et par victime**. Le message est
authentifié (identité gossip auto-générée), donc le ban par réputation n'arrive
qu'après le premier message — déjà suffisant pour figer la tâche de dispatch une
minute.

**Correctif.** Borner `auth.pubkeys.len()` et `auth.signatures.len()` (par ex. ≤ 16
et ≤ 16) **avant** toute vérification, et refuser `signatures.len() > keys.len()`.
Un quorum honnête n'a jamais besoin de plus de signatures que de clés.

---

### HAUT-2 — Rotation silencieuse de la clé de fonds (TOFU)

**Ce qui est faux.** `commands/identity.rs:114-133` :

```rust
match dbref.load_state(PQ_IDENTITY_KEY).await? {
    Some(json) => { ... PQVault::unlock_pq_identity(...)? }
    None => {
        let (pq_pk, pq_enc, pq_nonce) = PQVault::create_pq_identity(&mut engine, password)?;
        dbref.save_state(PQ_IDENTITY_KEY, &pq_identity_blob(...)).await?;
        invalidate_biometric_wrap(dbref).await;
    }
}
```

Si la ligne `pq_identity_v1` de `state_snapshots` est absente, le déverrouillage
**génère une identité ML-DSA neuve** et l'installe comme identité de fonds. Aucun
avertissement, aucune comparaison avec une valeur attendue, aucune trace on-chain.
L'utilisateur voit simplement une nouvelle adresse de réception.

**Chemin d'exploitation concret.** *Qui* : tout processus disposant d'un accès en
écriture au fichier `~/…/quanta.db` — un autre utilisateur local, un logiciel
malveillant sans privilèges, une synchronisation cloud qui restaure un snapshot
antérieur, ou simplement une corruption. La base est ouverte via
`libsql::Builder::new_local` (`storage/db.rs:27`) : **aucun chiffrement au niveau
fichier**, seuls les deux blobs de secret sont chiffrés. *Ce qu'il gagne* : une
destruction irréversible. `DELETE FROM state_snapshots WHERE key='pq_identity_v1'`
suffit ; au prochain déverrouillage réussi, la victime perd l'autorité sur
**tout** son solde on-chain. Seule la phrase de 24 mots la sauve — et elle n'est
présentée qu'à l'onboarding.

Noter que le mot de passe reste exigé : l'attaquant ne **prend pas** les fonds, il
les **annule**. C'est un vecteur de rançon/sabotage, pas de vol.

**Preuve.** [PROUVÉ au niveau DB] (`c12`) : `save_state` est un
`INSERT OR REPLACE` (`storage/db.rs:89`) et `load_state` renvoie `None` sur ligne
absente ; la branche `None` de `unlock_wallet` est une lecture directe. La
composition complète (via `AppState`/Tauri) n'a pas été exécutée — **non prouvé
de bout en bout**.

**Correctif.** Persister l'adresse ML-DSA attendue à côté du keypair Ed25519 et
**échouer** si le blob PQ manque ou ne correspond pas, au lieu de fabriquer une
identité. Un vault qui « répare » silencieusement l'absence de la clé maîtresse
n'est pas un vault.

---

### HAUT-3 — Écrasement de la clé de fonds sans ré-authentification

**Ce qui est faux.** `create_wallet` (`identity.rs:37-58`) et `restore_wallet`
(`identity.rs:197-238`) écrivent tous deux `save_state(PQ_IDENTITY_KEY, …)`
(lignes 56 et 231) sur la **même clé KV unique**, en `INSERT OR REPLACE`. Ni l'un
ni l'autre :
- ne vérifie qu'une identité existe déjà (`check_identity` existe mais n'est pas
  appelée) ;
- n'exige le mot de passe **de l'identité en place** ;
- ne passe par `state.unlock_guard` (contrairement à `unlock_identity`,
  `identity.rs:79`).

**Chemin d'exploitation concret.** Les deux fonctions sont exposées comme
commandes Tauri (`create_identity`, `restore_from_phrase`). Tout ce qui peut
émettre un `invoke` — la webview, donc toute injection dans le frontend Svelte,
ou tout script pilotant l'IPC — détruit définitivement la clé d'autorité du
portefeuille en un appel, sans que l'utilisateur ait à saisir son mot de passe
actuel. Impact : 100 % du solde, irrécupérable sans la phrase.

**Preuve (exécutée).** `c12` : deux `save_state` successifs sur `pq_identity_v1`,
le second écrase le premier sans erreur ; la table `keypairs` accumule au
contraire les lignes et `get_active_keypair` (`db.rs:67`) retourne la plus
récente — l'ancienne identité Ed25519 survit donc, orpheline de sa clé de fonds.

**Correctif.** Exiger le mot de passe courant (ou un déverrouillage préalable)
pour toute opération destructive, et archiver l'ancien blob sous une clé
horodatée au lieu de l'écraser.

---

### MOY-1 — La feuille Merkle porte la même ambiguïté (et sauve le bloc par accident)

`p2p/ledger/mod.rs:1285-1289` :

```rust
format!("from={}|to={}|amount={}|nonce={}|type={:?}|ts={}", …)
```

Même défaut de conception : séparateur `|` non échappé, avec `to` et `ts` libres.
On construit sans peine deux transactions distinctes de même feuille (`to =
"A|amount=1|nonce=0|type=Transfer|ts=T"` absorbe les étiquettes suivantes).

**Mais ce format sauve le hash de bloc contre l'attaque CRIT-1**, et je peux le
démontrer : soit deux triplets `(to, amount, ts)` distincts collisionnant dans la
préimage de signature. Ils imposent `|to₁| ≠ |to₂|` ; supposons `|to₂| > |to₁|`.
La préimage de signature impose alors `to₂[|to₁|] = ':'`, tandis que la feuille
Merkle impose `to₂[|to₁|] = '|'`. Contradiction. **Aucune paire ne peut
collisionner dans les deux encodages à la fois.** Vérifié empiriquement par `c01c`
sur la paire d'attaque réelle.

**Sévérité MOYEN et non BAS** parce que c'est une coïncidence de format, pas un
invariant : changer un séparateur, ajouter un champ ou aligner les deux encodages
transformerait CRIT-1 en fork de chaîne à hash identique. Le code ne documente
nulle part que cette propriété est ce qui le sauve.

---

### MOY-2 — Sel Argon2id non aléatoire, dérivé de données publiques

`security/pq_vault.rs:67` (vault Ed25519) : `salt = BLAKE3(pk_hex)[..16]`.
`security/pq_vault.rs:173-175` et `:215-217` (vault ML-DSA) :
`salt = BLAKE3("QUANTA-PQ-MIG-1-vault-v1:" + pq_pk_hex)[..16]`.

**Ce qui est faux.** Le sel n'est pas tiré d'`OsRng` : c'est une fonction pure de
la clé publique, elle-même publique (elle est sur la chaîne, dans chaque
transaction, dans chaque enveloppe gossip). Trois conséquences :

1. **Précalcul ciblé.** Un attaquant qui connaît l'adresse d'une victime peut
   lancer sa campagne de dictionnaire **avant** d'avoir volé le fichier de vault.
   Un sel aléatoire rendrait ce travail impossible par construction.
2. **Aucune re-randomisation au changement de mot de passe.** Le sel dépend de la
   clé, pas de l'époque : la table précalculée reste valide après le changement.
3. **Le sel PQ dépend de la clé ML-DSA**, donc le simple fait de publier son
   adresse suffit — puisque `pq_public_key` est révélée dans chaque transaction
   signée (`Transaction.pq_public_key`).

**Preuve (exécutée).** `c02` reconstruit les deux clés de vault à partir de la
seule clé publique et du mot de passe, et vérifie l'égalité avec
`PQVault::derive_ed_vault_key` / `derive_pq_vault_key`.

**Ce qui atténue.** Le sel reste **unique par identité** (la clé publique l'est),
donc pas de table arc-en-ciel inter-utilisateurs, et le coût par essai reste celui
d'Argon2id (mesuré 88 ms, cf. BAS-2). C'est un affaiblissement de la *fenêtre*
d'attaque, pas de son *coût unitaire* — d'où MOYEN et non HAUT.

**Correctif.** 16 octets d'`OsRng` stockés en clair à côté du ciphertext.

---

### MOY-3 — Le contexte FIPS-204 (`ctx`) est systématiquement vide

`security/hybrid_crypto.rs:124` : `pk.verify(msg, &sig_arr, &[])`.
Côté signature : `security/mod.rs:236`, `:365`, `:400`, tous en `…, &[])`.

FIPS 204 fournit un paramètre `ctx` (jusqu'à 255 octets) dont l'objet **est**
la séparation de domaine : deux usages avec des `ctx` distincts ne peuvent pas
voir leurs signatures interchangées, quel que soit le contenu du message. Quanta
partage un vérificateur unique entre quatre domaines — autorité de transaction,
enveloppes gossip, votes de finalité, records de pseudo — et laisse `ctx` vide
dans les quatre.

**Ce que j'ai vérifié.** Le rejeu inter-domaines n'est **pas** exploitable
aujourd'hui, pour des raisons structurelles et non par conception :

| domaine | préimage | commence par | finit par |
|---|---|---|---|
| transaction | `id:from:to:…:pq_pk` (`ledger/mod.rs:1481`) | champ libre | **caractère hexadécimal** |
| enveloppe gossip | `["sender",nonce,"ts",{payload}]` (`gossip.rs:614`) | `["` | **`]`** |
| vote de finalité | `QUANTA-FINALITY-VOTE-v1‖…` (`finality_vote.rs:89`) | domaine | hex validateur |
| pseudo | `QUSER\|…` (`username.rs:132`) | domaine | chiffres |

Une signature d'enveloppe (que le nœud produit automatiquement, en continu, avec
**la même clé ML-DSA que celle qui autorise les fonds** — cf. `wallet.rs:81`)
ne peut pas être rejouée comme autorité de transaction : la préimage de tx se
termine toujours par l'hex de la clé révélée, jamais par `]`. **Preuve
exécutée** : `c08`. Le vote et le pseudo portent, eux, une vraie étiquette de
domaine.

**Sévérité MOYEN** : la défense repose sur un accident de format, exactement comme
MOY-1, et sur le fait que la clé de gossip **est** la clé des fonds — un choix qui
ne laisse aucune marge. Ajouter `ctx = b"QUANTA-TX-V1"` / `b"QUANTA-ENVELOPE-V1"`
etc. coûte une ligne par site et rend la propriété inconditionnelle.

**Correction d'une hypothèse de la mission** : la préimage d'enveloppe gossip
(`gossip.rs:606-616`) est du JSON produit par `serde_json`, donc **injective** —
les guillemets et antislashs des chaînes sont échappés, aucune concaténation ne
peut franchir une frontière de champ. `c08` le vérifie sur un `sender` adversarial
contenant `",1,"`. Ce n'est pas le point faible ; la préimage de transaction l'est.

---

### MOY-4 — Aucune liaison entre le blob de vault et l'identité attendue

`PQVault::unlock_identity_with_key` (`pq_vault.rs:119-151`) déchiffre, reconstruit
le keypair et **renvoie l'identité obtenue**, sans jamais comparer la clé publique
reconstruite à la `public_key` qui a servi à dériver le sel. Idem côté ML-DSA
(`:234-251`).

**Chemin d'exploitation.** Un attaquant ayant l'écriture sur `quanta.db` remplace
le triplet `(public_key, encrypted_secret_key, nonce)` **et** le blob PQ par les
siens, chiffrés sous un mot de passe qu'il choisit. Il ne peut pas faire ouvrir ce
vault par la victime (elle a un autre mot de passe), mais s'il contrôle aussi
l'affichage — ou si la victime restaure une sauvegarde falsifiée — l'application
présente une adresse de réception attaquante sans le moindre signal.

**Preuve (exécutée).** `c04` : substitution intégrale du triplet, déverrouillage
`Ok`, identité publique différente de l'originale, aucune erreur.

**Correctif.** Stocker et vérifier l'adresse attendue ; échouer si elle diffère.

---

### MOY-5 — Le mnémonique et la clé de récupération échappent au zeroize

Le projet a fait un travail sérieux de zeroize (`Zeroizing` sur les exports,
`encrypt_and_wipe`, garde de compilation `security/mod.rs:489-495`, feature
`zeroize` activée sur `bip39` et `ed25519-dalek`). Deux fuites subsistent, aux
deux endroits qui manipulent le secret le plus critique :

- `commands/identity.rs:176-178` :
  ```rust
  let mnemonic = bip39::Mnemonic::from_entropy(&seed[..])…;
  Ok(mnemonic.to_string())
  ```
  `Mnemonic` est bien zeroize-on-drop, mais `to_string()` produit une `String`
  **nue** contenant les 24 mots. Elle est ensuite sérialisée en JSON par Tauri
  pour franchir l'IPC, ce qui la recopie encore. Aucune de ces copies n'est
  effacée : elles restent dans le tas jusqu'à réutilisation, donc dans un core
  dump, un swap, ou une capture mémoire.
- `commands/identity.rs:470-475` : `hex` est bien `Zeroizing`, mais
  `formatted.join("-")` reconstruit une `String` nue avec la totalité du secret
  Ed25519, retournée telle quelle.

**Impact.** Le premier est le secret qui contrôle **tous les fonds** ; le second
est l'identité de transport iroh (usurpation de nœud). Sévérité MOYEN parce que
l'exploitation suppose une lecture mémoire ou un dump — mais c'est précisément le
modèle de menace contre lequel tout le reste du zeroize a été écrit. **Non
prouvé** : je n'ai pas fait de dump mémoire ; le constat est une lecture de type.

**Correctif.** `Zeroizing<String>` de bout en bout, et rendre le secret au
frontend par un canal qui ne le recopie pas indéfiniment (ou l'afficher côté Rust).

---

### MOY-6 — `tx.signature` : exigée, jamais vérifiée

`validation.rs:73-75` refuse une transaction dont `signature` est vide, mais plus
aucune ligne ne la vérifie : le co-facteur Ed25519 a été retiré du chemin
d'autorité (commentaire `validation.rs:114-120`, confirmé par grep —
`CryptoEngine::verify` n'a **aucun appelant en production**). Le champ est donc
une chaîne libre non vide.

Conséquence : il est **malléable** — un relais peut le réécrire sans invalider
`verify_tx` ni changer `tx.hash`. Il est en revanche lié à la feuille Merkle
(`mod.rs:1334`), donc deux nœuds détenant des variantes différentes de la même
transaction en mempool ne produiront pas le même bloc s'ils scellent tous deux.
Combiné à CRIT-1 c'est du bruit ; seul, c'est une incohérence de conception :
soit on vérifie la signature, soit on supprime le champ et son test de non-vacuité.

---

### BAS-1 — `address::parse` contourne le checksum Bech32m

`security/address.rs:213-222` : si le décodage Bech32m échoue, on retombe sur
`hex::decode`. Le module existe précisément parce que « a raw 64-hex string has no
checksum » (`address.rs:13-14`). Or `ledger_transfer` (`commands/wallet.rs:67`) et
le RPC utilisent `parse`, donc un intégrateur (ou un utilisateur) qui colle de
l'hex n'a **aucune** protection contre la faute de frappe — l'argument commercial
du module est annulé pour la moitié de ses appelants. Le reste du module est
correct : checksum vérifié (`:174`), HRP contraint à `qta` (`:195`), longueur
exacte (`:198-202`), vecteurs BIP-350 officiels épinglés (tests `:236-266`),
constante Bech32m distincte de Bech32.

---

### BAS-2 — Politique de mot de passe

`commands/identity.rs:28` : `password.len() < 8` ⇒ refus. Mesuré en release :
Argon2id(64 MiB, t=3, p=4) = **88 ms/essai**, soit 11,3 essais/s/cœur.

- Espace 8 caractères alphanumériques minuscules (2,8·10¹²) : **79 ans** sur 100
  cœurs. Le KDF fait son travail.
- Mot de passe issu d'une liste de 10⁶ : **~1 jour** sur un cœur, **~15 min** sur
  100. C'est le cas réel, et il n'est mitigé par rien (pas de vérification de
  robustesse, pas de blocage de mots de passe courants).

Le `UnlockGuard` (`lib.rs:57-96`, backoff exponentiel plafonné à 60 s, en mémoire)
ne protège que le chemin en ligne ; l'attaque est hors ligne sur le fichier volé.

---

### BAS-3 — Paramètres KDF non versionnés

`security/cipher.rs:14-22` fige `(64 MiB, 3, 4)` en dur. Rien n'est persisté :
**il n'existe donc aucun paramètre à rétrograder** — la question posée dans la
mission se referme par la négative, il n'y a pas de vecteur de downgrade. La
contrepartie est qu'aucune montée en dureté n'est possible sans casser tous les
vaults existants. Un champ `kdf_version` dans le blob (déjà JSON), authentifié par
le tag GCM du blob lui-même, réglerait les deux.

---

## 4) Ce qui est solide

Ce n'est pas une liste de politesse : chacun de ces points a été vérifié.

1. **Liaison intrinsèque adresse↔clé — le meilleur choix de conception du
   projet.** `from == BLAKE3(ADDR_DOMAIN ‖ pk_ML-DSA)` est exigé à
   `validation.rs:95` avant toute vérification de signature. Un attaquant ne peut
   pas attacher sa clé à un compte étranger : une autre clé donne une autre
   adresse. C'est stateless, sans registre, sans fenêtre de course. **Prouvé**
   (`c06 a/b`) : signature valide de l'attaquant sur le `from` de la victime ⇒
   `Ok(false)`, et un `from` qui n'est pas 32 octets ⇒ `Ok(false)`. Le même
   contrôle est appliqué aux records de pseudo (`username.rs:145`) — donc à
   **tous** les chemins qui déplacent ou revendiquent une identité de valeur que
   j'ai lus.
2. **La racine ML-DSA est réellement indépendante d'Ed25519.**
   `generate_pq_identity` (`security/mod.rs:193-199`) tire 32 octets d'`OsRng`,
   sans aucun lien avec la graine Ed25519. `sign_tx_authority` (`:359-368`) lie
   **le primaire**, jamais la couche héritée. **Prouvé** (`c10`) : deux moteurs de
   même graine Ed25519 ⇒ adresses de fonds différentes, alors que la couche
   héritée `ml_dsa`, elle, est bien identique. Réponse nette à la question 2 :
   **casser Ed25519 ne donne aucune autorité sur les fonds**. Ed25519 ne subsiste
   que comme identité de transport iroh, et `CryptoEngine::verify` (Ed25519) n'a
   aucun appelant en production. La dette est correctement déclarée dans le CBOM
   (`crypto_agility.rs:69-75`, `pq_migration_count() == 1`).
3. **Argon2id au-dessus des recommandations OWASP 2024.** 64 MiB / t=3 / p=4
   contre un minimum recommandé de 19 MiB / t=2 / p=1. 88 ms mesurés par essai.
4. **Nonce AES-GCM tiré d'`OsRng` à chaque chiffrement** (`cipher.rs:27`), jamais
   dérivé, jamais compteur. **Prouvé** (`c03`) : 64 chiffrements du même clair
   avec la même clé, 64 nonces distincts. Le risque de rejeu par restauration de
   fichier est nul côté nonce : c'est le sel (MOY-2) qui est statique, pas le
   nonce.
5. **`OsRng` partout où il faut.** Revue exhaustive : aucun `thread_rng`, aucun
   RNG graine-horloge, aucun `SmallRng`/`StdRng` sur un chemin de sécurité. Le
   `Blake3Rng` de `hybrid_crypto.rs:39-73` est un XOF de dérivation (usage
   correct), et le signeur déterministe `ml_dsa_sign_deterministic` est
   `#[cfg(test)]` — **physiquement absent du binaire release**, y compris son
   appelant `build_signed_tx_at` (`ledger/mod.rs:1606-1619`). La signature de
   production est bien hedgée.
6. **Bech32m correct.** Implémentation maison mais épinglée sur les vecteurs
   officiels BIP-350, y compris le rejet d'une chaîne Bech32 valide sous Bech32m.
   Checksum vérifié, HRP contraint, casse mixte refusée, longueur bornée, aucun
   `unwrap`. **Prouvé** (`c06 c`) : une faute d'un caractère est rattrapée.
7. **Multisig M-of-N : la logique de quorum est juste.** `canonicalize_msig_keys`
   (`security/mod.rs:90-106`) décode les clés, impose `PK_LEN`, ré-encode en
   minuscules, trie et dédoublonne — **la même** fonction sert à dériver l'adresse
   et à compter le quorum, donc liaison et comptage ne peuvent pas diverger.
   **Prouvé** (`c09`) : `M=0` refusé, `M>N` refusé, double comptage d'un même
   signataire impossible, rejeu des signatures d'un message sur un autre refusé,
   même clé en deux casses ⇒ un seul slot. L'adresse multisig est
   domaine-séparée (`MSIG_DOMAIN`) et encodée avec préfixes de longueur
   (`security/mod.rs:116-127`) — c'est-à-dire **correctement**, contrairement à la
   préimage de transaction.
8. **Séparation de domaine des votes de finalité.** `VOTE_DOMAIN` +
   `push_bytes`/`push_checkpoint` avec préfixe `u64`
   (`finality_vote.rs:87-95`, `:256-266`) : encodage injectif. C'est le modèle que
   le reste du projet aurait dû suivre. Le certificat refuse les liens mixtes et
   les votants dupliqués (`:213-231`), et le quorum est en `u128` entier sans
   flottant (`:239-241`).
9. **Préimage d'enveloppe gossip injective** grâce au JSON `serde`
   (`gossip.rs:614`), et pipeline de dispatch bien ordonné : taille → JSON → ban →
   id canonique → dedup en lecture seule → fraîcheur → **signature** → insertion
   LRU → rate-limit → nonce (`dispatcher.rs:426-611`). Les mutations d'état
   par-sender sont toutes **après** la vérification de signature — l'attaque
   d'inflation de map par `sender` usurpé est fermée.
10. **Hygiène défensive constante** : erreurs opaques et identiques (`cipher.rs`
    tests `failures_are_indistinguishable`), longueur du nonce vérifiée avant
    `Nonce::from_slice` pour éviter la panique (`cipher.rs:41-43`), `verify_ml_dsa`
    ne panique sur aucune entrée malformée (`hybrid_crypto.rs:119-127`), fonction
    `short()` sûre aux frontières UTF-8 (`ledger/mod.rs:28`), comparaison de token
    RPC en temps constant (`rpc.rs:228-234`), Touch ID qui enveloppe les clés
    **dérivées** et jamais le mot de passe (`biometric.rs`, `identity.rs:298-333`).
11. **La suite de tests du projet est réelle** : `cargo test --lib` ⇒ **527 tests,
    0 échec**, incluant un méta-test de déterminisme sur 128 exécutions et des
    tests de propriété de conservation monétaire.

---

## 5) Ce que je n'ai PAS pu vérifier, et pourquoi

1. **La composition complète du TOFU (HAUT-2)** : `unlock_wallet` prend un
   `Arc<AppState>` qui embarque un `WillowNode` et un `tauri::AppHandle`. Monter
   cet état hors d'une application Tauri dépassait le budget ; j'ai prouvé les
   deux moitiés (branche `None` par lecture, sémantique DB par test) mais pas la
   chaîne complète.
2. **Le comportement réel du Keychain macOS** (`biometric.rs`) : `store_kek` /
   `read_kek` opèrent sur le trousseau vivant. Le tester aurait détruit le KEK
   Touch ID de la machine. Revue de code uniquement — et le code lui-même
   documente cette limite (`identity.rs:150-156`). En particulier, je n'ai **pas**
   pu vérifier que `.BIOMETRY_CURRENT_SET` invalide bien l'élément lors d'un
   changement d'empreintes.
3. **Le temps constant de `fips204`** : le projet affirme « constant-time, sans
   `unsafe` » (Cargo.toml). Je n'ai fait aucune mesure de canal auxiliaire, ni sur
   ML-DSA, ni sur AES-GCM (`aes-gcm` en Rust logiciel — pas d'AES-NI garanti selon
   la cible, donc une implémentation potentiellement vulnérable au timing cache
   n'est pas exclue). **Non vérifié.**
4. **Les permissions du fichier `quanta.db`** : `std::fs::create_dir_all` sans
   `mode`, `libsql::Builder::new_local` sans option de permission
   (`storage/db.rs:23-33`, `node_runtime.rs:74-85`). Le `umask` du système décide.
   Je n'ai pas lancé l'application pour observer les permissions effectives.
   À vérifier : un `0644` exposerait les blobs chiffrés et le fichier `.cookie` du
   RPC à tout utilisateur local (le `.cookie`, lui, est explicitement en `0600`,
   `rpc.rs:178-182`).
5. **Le frontend Svelte** : hors périmètre. Je n'ai donc pas pu établir si
   `create_identity` est protégée côté UI quand une identité existe déjà (HAUT-3).
   Rien ne la protège côté Rust.
6. **Un vrai réseau multi-nœuds** : CRIT-1 est prouvé au niveau des structures
   `Ledger` (deux instances, même genèse, bloc scellé et intégré). La course de
   propagation réelle sur iroh-gossip — qui décide *quelle* variante gagne — n'a
   pas été simulée. L'attaquant a un avantage structurel (il diffuse dès qu'il
   voit la transaction, sans attendre) mais je ne peux pas chiffrer son taux de
   succès.
7. **Le chemin `restore_from_phrase` avec une phrase de longueur non standard** :
   `identity.rs:214` exige 32 octets d'entropie, donc 24 mots. Une phrase de 12
   mots est refusée — correct — mais je n'ai pas testé les variantes de
   normalisation Unicode acceptées par `parse_normalized`.

---

### Reproduction

```bash
rsync -a --exclude target /Users/alex/Desktop/Quanta/src-tauri/ /tmp/qaudit/src-tauri/
cp /tmp/QUANTA_AUDIT_CRYPTO_POC.rs /tmp/qaudit/src-tauri/src/zz_audit_crypto.rs
# ajouter `#[cfg(test)] mod zz_audit_crypto;` à src/lib.rs de la COPIE
cd /tmp/qaudit/src-tauri
CARGO_TARGET_DIR=/tmp/qtarget_crypto cargo test --lib zz_audit -- --nocapture --test-threads=1
CARGO_TARGET_DIR=/tmp/qtarget_crypto cargo test --release --lib c05_ -- --nocapture   # chiffres DoS
```

14 tests, 0 échec. Le dépôt d'origine n'a été ni modifié, ni compilé, ni committé.
