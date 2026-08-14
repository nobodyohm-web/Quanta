# AUDIT QUANTA — CHAÎNE D'APPROVISIONNEMENT, BUILD, CI, SECRETS, QUALITÉ DE LA PREUVE

**Cible** : `/Users/alex/Desktop/Quanta` — Quanta v3.15.1, protocole TORUS v9
**Périmètre** : `Cargo.toml` / `Cargo.lock` (787 crates), `deny.toml`, `package.json` /
`package-lock.json`, `.github/workflows/*`, `.gitignore`, `src-tauri/fuzz/`,
`src-tauri/tests/`, `SECURITY.md`, `CONTRIBUTING.md`, `docs/audit/*`, artefacts de release
**Date** : 2026-08-13 · **Outils** : cargo 1.95.0, cargo-deny 0.19.8, cargo-audit 0.22.2
**Aucun fichier du dépôt n'a été modifié.** Les bancs d'essai sont dans `/tmp/quanta_probe` et
`/tmp/ovf`.

---

## 1) Résumé exécutif

1. **La porte anti-vulnérabilités de la CI est un tampon vide.** `cargo deny check` sort en
   **exit 0 / « advisories ok »** au moment exact où `cargo audit` sort **« error: 4
   vulnerabilities found! »** : les 4 sont dans la liste `ignore` de `deny.toml`. Pire, un
   *use-after-free* (`lru` 0.16.4, RUSTSEC-2026-0253) n'est **même pas** dans la liste et passe
   quand même, parce que `deny.toml` ne relève jamais `unsound`/`unmaintained` au rang d'erreur.
   La CI ne peut structurellement pas échouer sur une advisory.
2. **La clé qui contrôle tous les binaires est confiée à des actions non épinglées.**
   `release.yml` passe `TAURI_SIGNING_PRIVATE_KEY` — la clé minisign que chaque client Quanta
   installé considère comme racine de confiance — à `tauri-apps/tauri-action@v0`, un **tag
   mutable**, pas un SHA. 0 action sur 8 est épinglée, dont `dtolnay/rust-toolchain@stable` qui
   est une *branche*. Un seul compromis en amont = mise à jour malveillante signée, installée
   et exécutée chez tous les utilisateurs.
3. **Le binaire que télécharge un utilisateur aujourd'hui date de mai 2026, n'est pas signé, et
   n'a rien à voir avec le code.** Prouvé : `codesign` → « code object is not signed at all »,
   `spctl` → « rejected — source=no usable signature », version du bundle **1.0.1** (le dépôt
   est en 3.15.1), identifiant `com.sovereign.webengine` (la config actuelle dit
   `com.quanta.protocol`).

En contrepartie : **aucun secret** n'a jamais été committé (178 commits vérifiés), le compte
« 513 tests + 1 » est **exact**, la vérification de signature de l'updater est **réelle et
anti-downgrade**, et les 4 vulnérabilités de `cargo audit` sont, après traçage des appels,
**réellement inatteignables** — mais pour des raisons que `deny.toml` n'a pas trouvées.

---

## 2) Tableau des constats

| id | sévérité | ancre | résumé |
|---|---|---|---|
| SC-01 | **CRITIQUE** | `.github/workflows/release.yml:77` + `:81` | La clé de signature de l'updater est exposée à `tauri-action@v0`, tag mutable non épinglé |
| SC-02 | **CRITIQUE** | `deny.toml:19-59` + `.github/workflows/ci.yml:48-52` | `cargo deny check` = exit 0 alors que 4 vulns + 1 UAF sont dans l'arbre ; porte inopérante |
| SC-03 | **HAUT** | release GitHub `v1.0.1` / `tauri.conf.json:40` | Binaire publié non signé, non notarisé, vieux de 3 mois, version et bundle-id divergents |
| SC-04 | **HAUT** | `tauri.conf.json:46` → `latest.json` | Le manifeste de mise à jour pointe vers un **autre dépôt** (`Torus`), survit par redirection |
| SC-05 | **HAUT** | `SECURITY.md:125` ↔ `src/lib.rs:187` | « aucun client HTTP sortant » est **faux** — et cette phrase justifie 4 `ignore` de `deny.toml` |
| SC-06 | **HAUT** | `src-tauri/fuzz/fuzz_targets/gossip_envelope.rs:15` | 100 % des entrées de fuzz meurent au mur d'authentification ; zéro couverture après signature |
| SC-07 | **HAUT** | `src-tauri/Cargo.toml` (absence de `[profile.release]`) | `overflow-checks` OFF en release, ON en test : la suite valide une autre arithmétique que le binaire |
| SC-08 | **MOYEN** | `.github/workflows/ci.yml:54-66` | 3 vulnérabilités npm (1 haute) et aucun `npm audit` en CI |
| SC-09 | **MOYEN** | `src-tauri/capabilities/default.json:7` ↔ `src/lib/Settings.svelte:35` | Permissions `updater:*` / `process:*` non accordées : l'updater in-app est probablement mort par ACL |
| SC-10 | **MOYEN** | `.github/workflows/claude-review.yml:5` + `:47` | `issue_comment` + secrets + `issues: write` + action non épinglée = surface d'injection de prompt |
| SC-11 | **MOYEN** | `.github/workflows/ci.yml:34,36,38` | Aucun `--locked` : la CI n'atteste pas que le lockfile revu est celui compilé |
| SC-12 | **MOYEN** | `.github/workflows/release.yml:64-71` | Le binaire signé est construit depuis un `target/` restauré du cache ; ni reproductible, ni attesté |
| SC-13 | **MOYEN** | `deny.toml:82-87` | `deny = []` sous un commentaire qui promet l'inverse ; 60 crates dupliquées en `warn` |
| SC-14 | **BAS** | `src-tauri/src/rpc.rs:177-182` | Cookie RPC écrit en umask par défaut puis `chmod 0600` (fenêtre TOCTOU), Unix seulement |
| SC-15 | **BAS** | `.gitignore` | Ni `*.key`, `*.pem`, `*.p12`, `*.db`, ni `fuzz/{corpus,artifacts}` ignorés |
| SC-16 | *info* | `Cargo.lock` (rand 0.7.3) | RUSTSEC-2026-0097 : build-dep, non exploitable — bruit, à documenter comme tel |

---

## 3) Développement, constat par constat

---

### SC-01 — CRITIQUE — La clé de signature de l'updater passe par des actions non épinglées

**Ancres** : `.github/workflows/release.yml:77` (`uses: tauri-apps/tauri-action@v0`),
`:81-82` (`TAURI_SIGNING_PRIVATE_KEY`), `:22-23` (`permissions: contents: write`), `:20`
(`workflow_dispatch`), `src-tauri/tauri.conf.json:44` (clé publique épinglée dans l'app).

**Ce qui est faux.** La racine de confiance de tout le parc installé est une clé minisign
unique. Sa moitié publique est gravée dans l'application :

```
tauri.conf.json:44  "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEUyNDExMzA2QkU5MjBDRTcK..."
→ décodé :  untrusted comment: minisign public key: E2411306BE920CE7
            RWTnDJK+BhNB4kUPcSKhQPVrul496s91c51DtfNk+c2BCKRmqy5nHHyB
```

Sa moitié privée vit dans le secret GitHub `TAURI_SIGNING_PRIVATE_KEY` et est injectée dans
l'environnement d'une action tierce référencée par un **tag mutable** :

```
release.yml:76-82
      - name: Build & release
        uses: tauri-apps/tauri-action@v0        ← v0 = tag mobile, réécrit par l'amont
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
```

Inventaire exhaustif de l'épinglage sur les trois workflows :

| action | référence | type |
|---|---|---|
| `actions/checkout` | `@v4` | tag mobile |
| `actions/setup-node` | `@v4` | tag mobile |
| `actions/cache` | `@v4` | tag mobile |
| `dtolnay/rust-toolchain` | `@stable` | **branche** — la pire des références |
| `Swatinem/rust-cache` | `@v2` | tag mobile |
| `EmbarkStudios/cargo-deny-action` | `@v2` | tag mobile |
| `anthropics/claude-code-action` | `@v1` | tag mobile |
| `tauri-apps/tauri-action` | `@v0` | tag mobile — **et il reçoit la clé** |

**0 action sur 8 n'est épinglée par SHA.**

**Chemin d'exploitation concret.** L'attaquant compromet (ou obtient un commit dans) l'un de
ces dépôts d'action — le plus faible étant `dtolnay/rust-toolchain@stable`, une branche qu'un
seul commit suffit à déplacer. Au prochain `git push origin v3.16.0` :

1. le step malveillant lit `$TAURI_SIGNING_PRIVATE_KEY` dans l'environnement du job et
   l'exfiltre (DNS, HTTP, ou simplement en l'inscrivant dans un artefact) ;
2. l'attaquant construit son propre `Quanta.app.tar.gz`, le signe avec la clé volée et publie
   un `latest.json` ;
3. `tauri-plugin-updater` vérifie la signature contre la clé publique gravée
   (`updater.rs:712`), elle est **valide**, l'archive est installée puis exécutée.

**Impact chiffré.** Exécution de code arbitraire sur 100 % du parc installé qui accepte une
mise à jour, avec les privilèges de l'utilisateur — donc accès direct au vault
`~/Library/Application Support/quanta-protocol/quanta.db` et à la phrase BIP39 de 24 mots qui
contrôle tous les fonds. Il n'y a **ni rotation de clé, ni seuil M-sur-N, ni révocation** : une
seule fuite est définitive et silencieuse.

Deux aggravants sur le même fichier :
- `permissions: contents: write` est déclaré **au niveau du workflow** (`:22-23`) : tous les
  steps l'héritent, y compris les actions tierces.
- `workflow_dispatch` (`:20`) sans `environment:` d'approbation : tout détenteur du droit
  d'écriture peut lancer une release **signée** depuis n'importe quelle branche, sans second
  regard.

**Correction exacte.**
1. Épingler par SHA complet : `uses: tauri-apps/tauri-action@<sha40>  # v0.5.x`, idem pour les
   sept autres. Activer Dependabot sur l'écosystème `github-actions` pour bouger les SHA sous
   revue.
2. `environment: release` avec *required reviewers*, et restreindre le secret à cet
   environnement.
3. Mieux : sortir la clé de GitHub — signer hors-ligne sur une machine dédiée et ne publier que
   `latest.json` + `.sig`.
4. Ajouter `actions/attest-build-provenance` pour lier l'artefact au commit et au workflow.

*Prouvé* : l'inventaire d'épinglage et le passage du secret (lecture des workflows) ; la
vérification effective de la signature côté client (lecture de
`tauri-plugin-updater-2.10.1/src/updater.rs:712` et `:1453-1462`). *Non prouvé* : que le secret
soit effectivement configuré aujourd'hui (l'API secrets exige un droit admin).

---

### SC-02 — CRITIQUE — `cargo deny check` passe au vert avec 4 vulnérabilités et un use-after-free

**Ancres** : `deny.toml:19` (`[advisories]`), `:23` (`yanked = "deny"`), `:24-59`
(`ignore = [...]`), `:82-87` (`[bans]`), `.github/workflows/ci.yml:40-52`, `CONTRIBUTING.md:19`.

**Preuve exécutée**, les deux commandes lancées sur le dépôt, dans cet ordre :

```
$ cargo deny --manifest-path src-tauri/Cargo.toml check
advisories ok, bans ok, licenses ok, sources ok
DENY EXIT CODE = 0

$ cargo audit                                   (dans src-tauri/)
error: 4 vulnerabilities found!
warning: 22 allowed warnings found
```

Détail machine (`cargo audit --json`) :

| advisory | crate | version | CVSS | corrigé en |
|---|---|---|---|---|
| RUSTSEC-2026-0120 | hickory-net | 0.26.0-beta.4 | — | ≥ 0.26.1 |
| RUSTSEC-2026-0119 | hickory-proto | 0.26.0-beta.4 | — | ≥ 0.26.1 |
| RUSTSEC-2026-0195 | quick-xml | 0.38.4 | 7.5 (AV:N/AC:L/PR:N/A:H) | ≥ 0.41.0 |
| RUSTSEC-2026-0194 | quick-xml | 0.38.4 | 7.5 (AV:N/AC:L/PR:N/A:H) | ≥ 0.41.0 |

**Trois défauts distincts et cumulatifs.**

**(a) Les 4 vulnérabilités sont explicitement neutralisées.** `deny.toml:50-51` et `:57-58`
listent les quatre identifiants. La CI ne les verra jamais. `CONTRIBUTING.md:19` présente
pourtant `cargo deny check` comme l'une des « trois portes » que « chaque PR doit passer ». La
porte est ouverte, et cadenassée en position ouverte.

**(b) Les advisories `unsound` passent même sans être ignorées.** C'est le point le plus grave,
parce qu'il vaut pour *toute vulnérabilité future*. `cargo audit` remonte :

```
RUSTSEC-2026-0253  lru 0.16.4  unsound — use-after-free / double-free dans LruCache::pop()
RUSTSEC-2024-0429  glib 0.18.5 unsound
RUSTSEC-2026-0097  rand 0.7.3  unsound
```

Aucun de ces trois n'est dans `ignore`. Et pourtant `cargo deny check` répond `advisories ok`.
Raison : `[advisories]` ne déclare que `yanked = "deny"` (`:23`) ; il ne relève ni `unsound` ni
`unmaintained` au rang d'erreur, et cargo-deny 0.19 les traite en avertissement par défaut.
**Une corruption mémoire (CWE-416) classée `unsound` traverse donc la CI en silence,
aujourd'hui, sans que personne l'ait décidé.**

**(c) Six `ignore` sont périmés et pourrissent la liste.** cargo-deny le dit lui-même :

```
warning[advisory-not-detected]: advisory was not encountered
  deny.toml:43  RUSTSEC-2026-0049   no crate matched advisory criteria
  deny.toml:44  RUSTSEC-2026-0098   no crate matched advisory criteria
  deny.toml:45  RUSTSEC-2026-0099   no crate matched advisory criteria
  deny.toml:46  RUSTSEC-2026-0104   no crate matched advisory criteria
  deny.toml:36  RUSTSEC-2025-0134   no crate matched advisory criteria
  deny.toml:36  RUSTSEC-2025-0141   no crate matched advisory criteria
```

Les quatre `rustls-webpki` sont sortis de l'arbre quand `libsql` est passé en
`default-features = false` (`Cargo.toml:102`) — bonne décision — mais l'`ignore` est resté. Une
liste d'exceptions qui ne se nettoie pas devient une liste dans laquelle on ne relit plus rien.

**Datation et justification des exceptions.** Les 12 exceptions « unmaintained » (`:27-36`)
sont de **simples chaînes**, sans `reason` ni date. Les 10 autres ont un `reason` bien écrit, et
deux dates apparaissent en commentaire — « Triage du 2026-05-31 » (`:22`) et « Triage du
2026-07-12 » (`:56`). **Aucune n'est une expiration** : cargo-deny ne connaît pas de champ
d'expiration, et rien en CI ne fait échouer un triage vieux de six mois.

**Chemin d'exploitation.** Indirect mais réel : la CI donne au mainteneur la certitude fausse
qu'une dépendance vulnérable serait bloquée. La prochaine vulnérabilité `unsound` — la
catégorie qui contient les corruptions mémoire — entrera dans `main` sans un seul signal rouge.

**Correction exacte.**

```toml
[advisories]
yanked = "deny"
unmaintained = "workspace"   # 0.19 : n'échoue que sur les deps directes, sans noyer le signal
```

et, dans `ci.yml`, séparer les portes pour que l'ignore d'advisory ne masque plus rien :

```yaml
      - uses: EmbarkStudios/cargo-deny-action@<sha40>
        with: { manifest-path: src-tauri/Cargo.toml, command: check }
      - name: cargo-audit (sans liste d'ignore — visibilité brute)
        run: cargo install cargo-audit --locked && cargo audit --file src-tauri/Cargo.lock
      - name: Les ignores doivent porter une date de moins de 90 jours
        run: python3 .github/scripts/check_ignore_expiry.py deny.toml
```

Immédiatement : retirer les six `ignore` périmés et traiter `RUSTSEC-2026-0253` (`lru`), qui
n'a fait l'objet d'aucun triage.

---

### SC-03 — HAUT — Le binaire publié : non signé, non notarisé, vieux de trois mois, mal identifié

**Ancres** : release GitHub `v1.0.1` de `nobodyohm-web/Quanta`, `README.md:39`,
`SECURITY.md:114-115`, `src-tauri/tauri.conf.json:5`.

**Preuve exécutée.** L'unique release publiée (API GitHub) :

```
releases: 1
  tag=v1.0.1  draft=False  prerelease=False  published=2026-05-06T16:16:05Z
  assets=['latest.json', 'Quanta.app.tar.gz', 'Quanta.dmg']
```

Téléchargement du DMG servi par `releases/latest/download/Quanta.dmg` :

```
14 199 095 octets
sha256 = c695cd51706a98d043f93e45e9156e54de2304340562b877e2d8ae8b0610db12

$ codesign -dv --verbose=4 /tmp/Quanta_v101.dmg
/tmp/Quanta_v101.dmg: code object is not signed at all

$ spctl -a -t open --context context:primary-signature -v /tmp/Quanta_v101.dmg
/tmp/Quanta_v101.dmg: rejected
source=no usable signature
```

Puis, image montée en lecture seule, l'application elle-même :

```
CFBundleShortVersionString = 1.0.1              (le dépôt est en 3.15.1)
CFBundleIdentifier         = com.sovereign.webengine
                              (tauri.conf.json:5 déclare aujourd'hui com.quanta.protocol)

$ codesign -dv --verbose=4 Quanta.app
Identifier=quanta_protocol-6e1ffd78e1ec17e4
CodeDirectory v=20400 flags=0x20002(adhoc,linker-signed)     ← signature ad-hoc du linker

$ spctl -a -t exec -vv Quanta.app
Quanta.app: code has no resources but signature indicates they must be present
```

`flags=0x20002(adhoc,linker-signed)` est la signature que le linker appose d'office sur arm64
pour que le binaire soit simplement *exécutable*. Ce n'est **ni** un Developer ID, **ni** un
hardened runtime, **ni** un ticket de notarisation. Et il n'y a **aucun** `SHA256SUMS` ni
signature détachée parmi les assets.

**Chemin d'exploitation concret — pour un utilisateur qui télécharge un binaire.**

1. *Conditionnement.* Gatekeeper refuse le lancement. La seule voie est clic-droit → Ouvrir, ou
   `xattr -d com.apple.quarantine`. Le projet apprend donc à ses utilisateurs à contourner
   Gatekeeper pour un logiciel qui détient leurs clés privées. Un attaquant qui distribue un
   faux « Quanta.dmg » par n'importe quel canal (résultat de recherche, forum, faux miroir)
   bénéficie d'utilisateurs déjà entraînés à ignorer l'avertissement — et d'aucun point de
   comparaison, puisque le vrai binaire n'est pas signé non plus. **Il n'existe aujourd'hui
   aucun moyen, pour un utilisateur, de distinguer le vrai binaire d'un faux.**
2. *Aucune révocation possible.* Sans Developer ID ni notarisation, Apple n'a **rien** à
   révoquer si un binaire malveillant circule un jour sous ce nom ; le seul recours serait de
   dépublier la release.
3. *Périmètre de sécurité déplacé.* Le changement de `CFBundleIdentifier`
   (`com.sovereign.webengine` → `com.quanta.protocol`) fait que macOS considère les deux builds
   comme **deux applications distinctes** : répertoires de données séparés
   (`~/Library/Application Support/<id>`) et, surtout, ACL Keychain distincts — or
   `security-framework` sert au déverrouillage rapide par Touch ID (`Cargo.toml:155`).
   L'utilisateur de la v1.0.1 qui installe un build actuel se retrouve devant un nœud « neuf »,
   vault et entrée Keychain restant orphelins sous l'ancien identifiant. Aucun chemin de
   migration n'existe.
4. *Décalage protocolaire.* v1.0.1 est antérieure à neuf ruptures de protocole ; le binaire
   téléchargeable est structurellement incapable de parler au réseau que le code décrit.

Le README (`:39`) et SECURITY.md (`:114-115`) admettent honnêtement le fait ; ce constat en
chiffre les conséquences.

**Correction exacte.** (a) Dépublier v1.0.1 ou la marquer explicitement « incompatible, ne pas
installer » ; (b) publier au minimum un `SHA256SUMS` accompagné d'une signature hors-ligne —
coût nul, et cela donne enfin un ancrage vérifiable ; (c) Developer ID + notarisation +
hardened runtime : les six secrets `APPLE_*` sont **déjà câblés** en `release.yml:84-89`, il ne
manque que le certificat ; (d) figer `CFBundleIdentifier` et ne plus jamais le changer.

---

### SC-04 — HAUT — Le manifeste de mise à jour pointe vers un autre dépôt

**Ancres** : `src-tauri/tauri.conf.json:45-47`, contenu servi par cet endpoint.

**Preuve exécutée.** L'endpoint configuré répond **HTTP 200** et sert :

```json
{
  "version": "1.0.1",
  "pub_date": "2026-05-06T16:15:44Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkK...",
      "url": "https://github.com/nobodyohm-web/Torus/releases/download/v1.0.1/Quanta.app.tar.gz"
    }
  }
}
```

Trois anomalies vérifiées :

1. **L'URL de l'archive désigne `nobodyohm-web/Torus`**, l'ancien nom du projet, alors que
   l'endpoint qui sert ce manifeste est `nobodyohm-web/Quanta`. Test :
   `api.github.com/repos/nobodyohm-web/Torus` → **HTTP 301 Moved Permanently**. L'archive ne se
   télécharge donc que grâce à la redirection de renommage GitHub (14 586 076 octets
   effectivement servis). Cette redirection **cesse d'exister à la seconde où un dépôt nommé
   `Torus` est recréé sous ce compte** — y compris par inadvertance par le propriétaire.
2. **Une seule plateforme** : `darwin-aarch64`. Aucune entrée `darwin-x86_64`, `linux-x86_64`
   ni `windows-x86_64`, alors que `release.yml:29-38` construit les quatre. Les utilisateurs
   Intel, Linux et Windows n'ont **aucune** voie de mise à jour.
3. **La version annoncée est 1.0.1**, inférieure à la version courante 3.15.1.

**Ce qui tient malgré tout — et il faut le dire.** Un attaquant qui parviendrait à occuper
l'URL `Torus` **n'obtient pas d'exécution de code**. La signature minisign est portée par
`latest.json` (servi par le dépôt `Quanta`, légitime) et vérifiée sur les octets téléchargés
**avant** toute installation :

```
tauri-plugin-updater-2.10.1/src/updater.rs:712
        verify_signature(&buffer, &self.signature, &self.config.pubkey)?;
        Ok(buffer)                                        ← install() n'est appelé qu'après
updater.rs:1453-1462
        let public_key = PublicKey::decode(&pub_key_decoded)?;
        public_key.verify(data, &signature, true)?;
```

De même, le rejeu d'un manifeste ancien ne permet pas de *downgrade* :

```
updater.rs:530-533
        let should_update = match self.version_comparator.as_ref() {
            Some(comparator) => comparator(self.current_version.clone(), release.clone()),
            None => release.version > self.current_version,      ← strictement supérieur
        };
```

**Impact réel.** Déni de service sur le canal de mise à jour, pas RCE. Sévérité **HAUT** parce
que ce canal est le seul mécanisme de correction d'urgence d'un logiciel qui détient des fonds :
il est aujourd'hui inopérant sur trois plateformes sur quatre, et fragile sur la quatrième.

**Correction exacte.** Régénérer `latest.json` à chaque release — `tauri-action` le fait
correctement, le fichier publié est simplement resté figé à mai 2026 — et vérifier en CI que
toutes les cibles de la matrice apparaissent dans `platforms`.

---

### SC-05 — HAUT — Une affirmation fausse de SECURITY.md sert de justification à quatre exceptions de sécurité

**Ancres** : `SECURITY.md:125`, `deny.toml:39-46`, `src-tauri/src/lib.rs:187`,
`src-tauri/Cargo.toml:146`.

**L'affirmation.**

```
SECURITY.md:125   - L'application n'embarque aucun client HTTP sortant.
```

**La réalité, vérifiée par l'arbre de dépendances :**

```
$ cargo tree -i reqwest -e normal
reqwest v0.13.2
├── iroh v0.98.1
│   └── quanta-protocol v3.15.1
├── iroh-relay v0.98.0
└── tauri-plugin-updater v2.10.1
    └── quanta-protocol v3.15.1        ← dépendance normale directe
```

et par le code :

```
Cargo.toml:146    tauri-plugin-updater = "2.10.1"
lib.rs:187        .plugin(tauri_plugin_updater::Builder::new().build())
tauri-plugin-updater-2.10.1/src/updater.rs:451, :663
                  let mut request = ClientBuilder::new().user_agent(UPDATER_USER_AGENT);
```

L'application **embarque** un client HTTP (`reqwest` 0.13.2), **par deux chemins**, et l'un
d'eux effectue des requêtes HTTPS sortantes vers `github.com`.

**Pourquoi c'est un constat de sécurité et pas une coquille.** Cette phrase est citée
*textuellement* comme argument de non-exposition dans le fichier qui décide ce que la CI laisse
passer :

```
deny.toml:39-42
  # rustls-webpki 0.102.8 via libsql → rustls 0.22. libsql est utilisé EN LOCAL
  # uniquement (l'app ne fait AUCUNE requête HTTP sortante, cf. SECURITY.md) →
  # le chemin TLS hyper-rustls/webpki n'est jamais exercé.
```

Quatre exceptions de sécurité (RUSTSEC-2026-0049 / -0098 / -0099 / -0104) ont été accordées sur
la foi d'une prémisse fausse. Que ces quatre-là soient devenues sans objet par ailleurs (SC-02c)
est une **coïncidence heureuse**, pas le résultat du raisonnement — et c'est exactement le même
raisonnement qui a produit les exceptions hickory et quick-xml, toujours actives.

**Nuance, à l'honneur du projet.** L'appel `check()` n'est **pas** automatique au démarrage : il
n'apparaît que dans `src/lib/Settings.svelte:35`, sur action de l'utilisateur. Il n'y a donc pas
de « phone home » silencieux à chaque lancement — juste une phrase de politique de sécurité qui
n'est pas vraie.

**Correction exacte.** Réécrire `SECURITY.md:125` en : « Le seul trafic HTTP sortant est la
vérification de mise à jour (`tauri-plugin-updater`, `reqwest`), déclenchée manuellement depuis
Réglages ; le transport P2P utilise QUIC via iroh. » Puis **re-justifier chaque `ignore` de
`deny.toml` au niveau de l'API appelée**, pas au niveau du réseau — c'est faisable et beaucoup
plus solide, comme le montre la section suivante.

---

### SC-P1 — Question 1 : les 4 vulnérabilités sont-elles réellement atteignables ? (traçage complet)

Je n'ai pas recopié les avis : j'ai lu leur texte intégral dans `~/.cargo/advisory-db`, relevé
la condition exacte d'atteignabilité de chacun, puis vérifié cette condition dans l'arbre de
features résolu et dans le code amont. Verdict : **les quatre sont inatteignables**, et pour
trois d'entre elles la démonstration est bien plus forte que celle de `deny.toml`.

#### RUSTSEC-2026-0120 — hickory-net, boucle NSEC3 non bornée — **NON ATTEIGNABLE (prouvé)**

L'avis est explicite sur sa condition : *« The bug is reachable by any caller of
`DnssecDnsHandle` … **when built with the `dnssec-ring` or `dnssec-aws-lc-rs` feature** and
configured to perform DNSSEC validation. »*

Features réellement activées, relevées par `cargo tree -e features` sur l'arbre du projet :

```
hickory-net feature "__https"
hickory-net feature "__tls"
hickory-net feature "https-aws-lc-rs"
hickory-net feature "tls-aws-lc-rs"
hickory-net feature "tokio"
hickory-net feature "tokio-rustls"
```

Ni `dnssec-ring`, ni `dnssec-aws-lc-rs`, ni `__dnssec`. Or le module entier est compilé
conditionnellement :

```
hickory-net-0.26.0-beta.4/src/lib.rs:10-11
    #[cfg(feature = "__dnssec")]
    pub mod dnssec;
```

et c'est exactement là que vit le code fautif (`src/dnssec/mod.rs:62 pub struct DnssecDnsHandle`).
**Le code vulnérable n'existe pas dans le binaire.** Ce n'est pas une question d'exposition
réseau : il n'est pas compilé. C'est le choix `iroh = { default-features = false, features = [...] }`
(`Cargo.toml:48-58`) qui produit ce résultat — une bonne décision qui rapporte ici.

*La justification de `deny.toml:51` (« DoS découverte DNS ») est donc inexacte : elle laisse
croire à un risque accepté, alors que le risque est nul.*

#### RUSTSEC-2026-0119 — hickory-proto, compression de noms en O(n²) — **NON AMPLIFIABLE (prouvé)**

L'avis vise l'**encodage** : *« During message **encoding**, `hickory-proto`'s `BinEncoder`
stores pointers to labels … The name compression logic then searches for matches with a linear
scan. A malicious message with many records can both introduce many candidate labels, and
invoke this linear scan many times. »* Le code est bien présent :

```
hickory-proto-0.26.0-beta.4/src/serialize/binary/encoder.rs:99    name_pointers: Vec<(usize, Vec<u8>)>
                                                          :280    for (match_start, matcher) in &self.name_pointers {
```

La question qui décide de tout est donc : **ce programme ré-encode-t-il jamais un message
reçu ?** J'ai listé tous les sites d'encodage hors tests de la pile réellement compilée :

```
hickory-net/src/udp/udp_client_stream.rs:178    let request_bytes = match request.to_vec()
hickory-net/src/h2.rs:121                       let bytes = match request.to_vec()
hickory-net/src/h3/h3_client_stream.rs:241      let bytes = match request.to_vec()
hickory-net/src/quic/quic_stream.rs:144         let bytes = Bytes::from(message.to_vec()?)
hickory-net/src/xfer/dns_multiplexer.rs:248     match request.to_vec()
```

**Tous** encodent une `request` — une requête que le résolveur a lui-même construite : une
question, zéro enregistrement. Les réponses ne font jamais le chemin inverse : elles passent
exclusivement par `DnsResponse::from_buffer(...)` (`h2.rs:401`, `h3_client_stream.rs:177`,
`quic_stream.rs:187`), c'est-à-dire du **décodage**. Le seul `response.to_vec()` du dépôt amont
est dans un serveur factice `#[tokio::test]` (`hickory-resolver/src/name_server.rs:984`,
fonction `case_randomization_query_preserved`), donc absent du binaire.

Un attaquant réseau contrôle ce que le résolveur **décode**, jamais ce qu'il **encode**. Le
`Vec` de pointeurs reste à quelques entrées. **Pas d'amplification possible.**

*Nuance honnête* : le chemin de **décodage** DNS, lui, est bien exposé —
`Endpoint::builder(iroh::endpoint::presets::N0)` (`p2p/willow_node.rs:551`) active la découverte
n0 qui interroge le DNS, et un réseau local hostile ou un résolveur malveillant contrôle
intégralement les réponses. Aucune advisory ne frappe ce chemin aujourd'hui ; c'est une surface
à surveiller, pas un défaut actuel.

#### RUSTSEC-2026-0194 et -0195 — quick-xml — **NON ATTEIGNABLES (prouvé, deux fois)**

Les deux avis délimitent précisément l'API fautive :
- **-0194** : *« `BytesStart::attributes()` / `Attributes` iterated with checks enabled … and
  `BytesStart::try_get_attribute`. `NsReader`, which resolves namespaces … »*
- **-0195** : *« Consumers using `NsReader` … **A plain `Reader` that does not perform namespace
  resolution is not affected.** »*

Ce que `plist` — le seul consommateur de quick-xml dans l'arbre — utilise réellement :

```
plist-1.8.0/src/stream/xml_reader.rs:2
  use quick_xml::{escape::resolve_xml_entity, events::Event as XmlEvent,
                  Error as XmlReaderError, Reader as EventReader};     ← Reader nu
plist-1.8.0/src/stream/xml_reader.rs:28
  let mut xml_reader = EventReader::from_reader(reader);
```

et recherche exhaustive dans tout `plist/src` de `.attributes` / `try_get_attribute` /
`NsReader` : **aucune occurrence**. Le format plist XML n'a ni attributs signifiants ni espaces
de noms ; `plist` n'appelle donc littéralement jamais les deux fonctions vulnérables. **Les deux
advisories sont hors d'atteinte quelle que soit l'entrée**, y compris une entrée 100 % contrôlée
par un attaquant.

Second verrou, indépendant : l'entrée n'est jamais distante.

```
netdev-0.42.0/src/os/macos/sc.rs:10
  const SC_NWIF_PATH: &str = "/Library/Preferences/SystemConfiguration/NetworkInterfaces.plist";
tauri-2.10.3/src/process.rs:112
  plist::from_file::<_, plist::Dictionary>(contents_directory.join("Info.plist"))
```

Un fichier système appartenant à root (macOS uniquement) et l'`Info.plist` du propre bundle de
l'application. La justification de `deny.toml:57-58` est donc **correcte, mais pour la plus
faible des deux raisons** : elle argumente sur la provenance de l'entrée, alors qu'il suffisait
de constater que l'API vulnérable n'est jamais appelée.

*Note* : `plist` est atteint par **deux** chemins, `netdev → netwatch → iroh` **et
`tauri` en dépendance normale directe** — ce second chemin n'est pas mentionné dans le triage de
`deny.toml`, qui ne parle que de « plist/iroh ». Sans conséquence ici, mais le triage était
incomplet.

#### RUSTSEC-2026-0097 — `rand` 0.7.3 — **NON EXPLOITABLE, franchement**

```
$ cargo tree -i rand@0.7.3 -e normal,build
rand v0.7.3
└── phf_generator v0.8.0 → phf_codegen v0.8.0 → [build-dependencies] selectors v0.24.0
    → kuchikiki → tauri-utils v2.8.3 → tauri-build v2.5.6 → [build-dependencies] quanta-protocol
```

C'est une **build-dependency** : elle n'entre pas dans le binaire livré. Et l'avis exige quatre
conditions simultanées — features `log` + `thread_rng`, **un logger `log` personnalisé**, ce
logger appelant `rand::rng()`, et un reseed pendant cet appel. `phf_generator` génère des tables
de hachage parfaites à la compilation et n'installe aucun logger. **Zéro impact.**
Recommandation : l'ajouter à `ignore` avec cette justification écrite, précisément pour que
personne ne reperde une heure dessus.

#### Corrections proposées — et pourquoi les corrections « évidentes » ne marchent pas

- `[patch.crates-io] hickory-proto = "0.26.1"` : **ne fonctionne pas.** `iroh` 0.98.1 épingle
  `hickory-resolver = "=0.26.0-beta.4"` ; un `patch` doit satisfaire la contrainte d'origine, et
  `=0.26.0-beta.4` n'accepte pas `0.26.1`. Le seul `patch` viable serait un fork git conservant
  le numéro `0.26.0-beta.4` avec les deux correctifs rétroportés — coût disproportionné pour un
  risque nul.
- `[patch.crates-io] quick-xml = "0.41"` : **ne fonctionne pas non plus.** `plist` 1.8.0 requiert
  `quick-xml ^0.38`. Il faut attendre un `plist` amont sur 0.41+.
- **Correction réellement applicable** : (a) bumper `iroh` dès 0.99, ou dès que 0.98.x relâche
  l'épinglage — c'est le levier unique pour les quatre ; (b) en attendant, **conserver les
  `ignore` mais réécrire chaque `reason`** avec les preuves ci-dessus (feature `__dnssec`
  absente ; encodeur jamais alimenté par du distant ; `NsReader`/`attributes()` jamais appelés),
  ce qui transforme une exception subie en décision documentée et re-vérifiable ; (c) ajouter une
  expiration à 90 jours contrôlée en CI ; (d) traiter en priorité **RUSTSEC-2026-0253 (`lru`,
  use-after-free)** qui, lui, n'a fait l'objet d'aucun triage.

**Angle mort du triage.** Le vrai parseur exposé à des octets d'attaquant dans ce programme n'est
aucun des quatre : c'est `simple-dns 0.9.3` (via `iroh-dns`) et `mainline 6.2.0`, qui décodent
les enregistrements **pkarr signés récupérés sur la DHT BitTorrent publique** activée par
`address-lookup-pkarr-dht` (`Cargo.toml:57`) et par `DhtAddressLookup::builder()`
(`p2p/willow_node.rs:549`). Ces octets viennent de n'importe qui sur Internet, sans
authentification préalable. Ni advisory, ni triage, ni cible de fuzz.

---

### SC-06 — HAUT — Le fuzzing ne franchit jamais le mur d'authentification

**Ancres** : `src-tauri/fuzz/fuzz_targets/gossip_envelope.rs:14-16`, `src-tauri/src/lib.rs:48`,
`src-tauri/src/p2p/dispatcher.rs:959-961`, `:976-999`, `:128-133`,
`src-tauri/fuzz/Cargo.toml:13`.

**Ce qui existe — à dire d'abord.** Il y a une cible, et elle est branchée sur le **vrai**
validateur, pas sur une fonction triviale :

```
gossip_envelope.rs:14-16   fuzz_target!(|data: &[u8]| { let _ = quanta_lib::fuzz_parse_gossip(data); });
lib.rs:48                  pub use p2p::dispatcher::try_process_raw_gossip as fuzz_parse_gossip;
dispatcher.rs:959-961      pub fn try_process_raw_gossip(data: &[u8]) -> Result<(), String> {
                               validate_envelope_at(data, now_epoch_secs() as i64).map(|_| ())
                           }
dispatcher.rs:976-999      pub fn validate_envelope_at(data, now_secs) -> Result<GossipEnvelope, String>
                               ① taille > MAX_RAW_ENVELOPE_BYTES (10 Mo)  → Err
                               ② serde_json::from_slice::<GossipEnvelope> → Err
                               ③ GossipRouter::is_fresh_at(±90 s)         → Err
                               ④ verify_envelope_signature (ML-DSA-65)    → Err
```

**Ce qui est faux.** L'étape ④ est un mur infranchissable pour un fuzzer : produire une
signature ML-DSA-65 valide par mutation aléatoire est un événement de probabilité nulle. Tout ce
qui vit après ④ — donc **toute la logique métier** — reçoit exactement zéro entrée de fuzz.

**Preuve — mesure, pas raisonnement.** Banc `/tmp/quanta_probe`, crate externe dépendant du
projet par `path` (aucun fichier du dépôt touché), instrumentant l'entrée de fuzz et classant
l'étape d'arrêt de chaque entrée :

```
running 2 tests
test fuzz_entrypoint_cannot_reach_any_handler ...
    [WELL-FORMED 50000 envelopes] {"5-ML-DSA-verify": 50000}
    stopped at the authentication wall: 50000/50000
ok
test fuzz_entrypoint_random_bytes_depth ...
    [RANDOM 200000 inputs] {"2-json": 200000}
ok

test result: ok. 2 passed; 0 failed; 0 ignored
```

- **200 000 entrées aléatoires** → **200 000 / 200 000 (100 %)** arrêtées à l'étape ② (décodeur
  JSON). Aucune n'atteint même le contrôle de fraîcheur.
- **50 000 enveloppes syntaxiquement parfaites** — champs bien formés, `sender` de 1 952 octets
  et `signature` de 3 309 octets aux longueurs exactes d'ML-DSA-65, horodatage frais, soit l'état
  le plus profond qu'un fuzzer guidé par couverture puisse espérer synthétiser → **50 000 /
  50 000 (100 %)** arrêtées à l'étape ④. **Aucune n'a jamais atteint un handler.**

**Surface réellement fuzzée** : plafond de taille, désérialisation `serde_json` de
`GossipEnvelope` **et de toutes les variantes de `GossipMessage`** — c'est réel et non
négligeable, l'énumération est typée et décodée dès l'étape ② (`gossip.rs:124-137`) —, parsing
RFC3339, décodage hex, et le chemin de **rejet** d'ML-DSA.

**Surface NON fuzzée**, c'est-à-dire tout ce qui traite une donnée après l'avoir crue :
`decompress_blocks` (gzip venu d'un pair, `gossip.rs:307-327` — la fonction la plus « fuzzable »
du dépôt, un décompresseur alimenté par le réseau), l'intégration de bloc, l'application de
transaction, le registre de pseudos, les segments de chaîne, le calcul de Shapley, les votes de
finalité. Et hors gossip : les enregistrements **pkarr/BEP-44 de la DHT publique** (cf. SC-P1),
les tickets de nœud, le parseur d'adresse bech32m, le vault chiffré, la phrase BIP39.

**Deux défauts aggravants, tous deux vérifiés.**

1. **L'entrée de fuzz lit l'horloge murale.**
   ```
   dispatcher.rs:128-133   fn now_epoch_secs() -> u64 { SystemTime::now()... }
   gossip.rs:527-528       let drift = (now_secs - ts.timestamp()).unsigned_abs(); drift <= 90
   ```
   Un crash sauvegardé par libFuzzer n'est donc **reproductible que dans une fenêtre de
   180 secondes**. C'est d'autant plus dommage que la version pure existe déjà et est publique :
   `validate_envelope_at(data, now_secs)` (`dispatcher.rs:976`). La cible devrait appeler
   celle-là avec une constante.
2. **La CI ne compile même pas la cible de fuzz.** `fuzz/Cargo.toml:13` déclare un
   `[workspace]` vide, dans l'intention explicite (commentaire `:10-12`) que « `cargo test` ne
   la construise jamais et que la CI reste verte sans toolchain nightly ». Résultat : aucun des
   trois workflows ne touche `fuzz/`, et un renommage de `fuzz_parse_gossip` casserait la cible
   sans qu'un seul test rougisse. *J'ai vérifié qu'elle compile encore aujourd'hui* (`cargo
   check` dans `fuzz/` → `Finished`, exit 0) : c'est de la chance, pas un dispositif. Enfin,
   `ls fuzz/` ne montre **ni `corpus/`, ni `artifacts/`, ni `Cargo.lock`** — chaque exécution
   repartirait de zéro, et rien n'indique que le fuzzer ait jamais tourné.

**Correction exacte.**
1. Faire pointer la cible existante sur la fonction pure et déterministe :
   `fuzz_target!(|data: &[u8]| { let _ = quanta_lib::fuzz_validate_envelope_at(data, 1_780_000_000); });`
2. Exposer, en `#[doc(hidden)] pub use` comme l'a été `fuzz_parse_gossip`, les parseurs
   post-authentification et ajouter **quatre cibles**, par ordre de rentabilité :
   `decompress_blocks` (bombe gzip), le décodeur de bloc, le décodeur de transaction, le parseur
   d'adresse bech32m. Une cinquième pour les enregistrements pkarr/DHT.
3. Committer un `corpus/` de graines (une enveloppe valide par variante de `GossipMessage`) et
   ignorer `fuzz/{artifacts,corpus/*,target}` dans `.gitignore`.
4. Ajouter un job CI `cargo +nightly fuzz run <cible> -- -max_total_time=120` par cible sur
   `main`, et un `cargo fuzz build` sur chaque PR pour interdire le pourrissement.

---

### SC-07 — HAUT — Le binaire livré n'a pas les vérifications de débordement que la suite de tests suppose

**Ancre** : `src-tauri/Cargo.toml` — **absence** de toute section `[profile.*]` (vérifié :
`grep -n "profile\|overflow-checks\|panic =\|lto\|rust-version" Cargo.toml` ne renvoie rien).

**Ce qui est faux.** Sans `[profile.release]`, Cargo applique ses défauts :
`overflow-checks = false` et `debug-assertions = false` en release, `true` en dev. Or
`cargo test` compile en **dev**. La suite de 513 tests s'exécute donc sous une sémantique
arithmétique **différente de celle du binaire livré**.

**Preuve exécutée** (`/tmp/ovf`, même code, deux profils) :

```
######## profil dev  (ce qu'utilise `cargo test`) ########
thread 'main' panicked at src/main.rs:4:33:
attempt to add with overflow
u64::MAX + 1 -> PANIC

######## profil release (ce que `tauri build` produit, ce que l'utilisateur exécute) ########
debug_assertions = false
u64::MAX + 1 = 0  <-- SILENT WRAP, no panic
```

**Conséquences concrètes pour une monnaie.**

1. Tout test qui « prouve » qu'un dépassement est refusé **en observant une panique** prouve une
   propriété qui n'existe pas en production. `CONTRIBUTING.md:33` impose « tous les montants en
   `u64` µQTA » et `SECURITY.md:75` revendique l'invariant de conservation
   `Σ(dépensable + staké + en déverrouillage) + brûlé == miné` : cet invariant est vérifié en
   simulation, en dev, avec les paniques d'overflow actives. En release, un `a + b` non
   `checked_` produit silencieusement une valeur fausse au lieu de s'arrêter.
2. Le second filet — la panique — est retiré exactement là où il compte. Un `checked_add` oublié
   devient, en dev, un test rouge ; en release, de la monnaie créée ou détruite sans trace.
3. Symétriquement, un `debug_assert!` de garde n'existe pas dans le binaire. L'avis
   RUSTSEC-2026-0120 illustre parfaitement le motif : *« A `debug_assert_ne!` guards the loop
   body, so debug builds abort … Release builds compile the assertion out and run the loop
   unbounded. »*

Je **n'ai pas** audité l'arithmétique du grand livre — c'est le périmètre d'un autre auditeur.
Mon constat porte sur la **valeur probante de la suite** : elle ne teste pas l'artefact.

**Correction exacte**, deux lignes qui suppriment toute la classe de problème :

```toml
[profile.release]
overflow-checks  = true     # obligatoire pour une monnaie : arrêt net plutôt que solde faux
debug-assertions = false
panic = "abort"             # optionnel ; supprime aussi la précondition d'exploitation de
                            # RUSTSEC-2026-0253 (lru), qui exige un déroulement de panique

[profile.release.package."*"]
overflow-checks = true      # inclut les dépendances : le ledger traverse des crates tierces
```

Coût de l'ordre de 1 à 3 % de CPU, négligeable pour un nœud qui signe du ML-DSA-65. Ajouter en
complément un `rust-toolchain.toml` (aucun n'existe : `ls rust-toolchain*` → absent) et un
`rust-version` dans `[package]`, sans quoi `dtolnay/rust-toolchain@stable` fait varier le
compilateur de la release au fil des semaines.

---

### SC-08 — MOYEN — Trois vulnérabilités npm, dont une haute, et aucun `npm audit` en CI

**Ancres** : `.github/workflows/ci.yml:54-66` (job `frontend`), `package.json:12-15`.

**Preuve exécutée** :

```
$ npm audit --audit-level=low
nanoid  <3.3.18                     Severity: high
  custom generators can loop indefinitely when size is zero   GHSA-2v37-7h3g-55p8
@sveltejs/kit  <=2.70.2             Severity: moderate
  ReDoS (O(n^2)) in content negotiation — unauthenticated DoS via the Accept header
                                                              GHSA-29g2-3rmr-qm68
cookie  <0.7.0                      Severity: low
  cookie accepts cookie name, path, and domain with out of bounds characters
3 vulnerabilities (1 low, 1 moderate, 1 high)
```

Le job `frontend` lance `npm ci`, `npm run check`, `npm run build` — **jamais** `npm audit`. Le
script `ai:audit` de `package.json:14` ne couvre que `cargo audit`. Côté Rust la porte existe
(même si SC-02 montre qu'elle est ouverte) ; côté npm **il n'y a aucune porte**.

**Atteignabilité, honnêtement.** Le projet utilise `@sveltejs/adapter-static` : SvelteKit produit
des fichiers statiques à la compilation et **aucun serveur SvelteKit ne tourne à l'exécution**.
La ReDoS sur l'en-tête `Accept` et le bug `cookie` ne sont donc pas atteignables dans le produit
livré ; `nanoid` est consommé par la chaîne vite/rollup à la compilation. **Les trois sont des
vulnérabilités de la chaîne de build, pas du binaire.** Sévérité MOYEN, portée sur le **manque de
porte**, pas sur ces trois-là : c'est la machine de build du mainteneur — celle-là même qui
détient la clé de signature (SC-01) — qui est exposée, et la prochaine vulnérabilité npm ne sera
pas forcément aussi inoffensive.

**Correction exacte** — trois lignes dans `ci.yml`, job `frontend` :

```yaml
      - name: npm audit
        run: npm audit --audit-level=high
```

plus `npm audit fix` immédiat (les trois correctifs sont non cassants d'après npm) et Dependabot
sur l'écosystème `npm`, en plus de `cargo` et `github-actions`.

---

### SC-09 — MOYEN — L'updater in-app est très probablement mort par ACL (non prouvé à l'exécution)

**Ancres** : `src-tauri/capabilities/default.json:6-8`, `src/lib/Settings.svelte:2-3`, `:35`, `:64`.

La capability unique du projet n'accorde que le socle :

```
capabilities/default.json:6-8
  "permissions": [
    "core:default"
  ]
```

Le frontend appelle pourtant deux commandes de plugins :

```
Settings.svelte:2   import { check } from "@tauri-apps/plugin-updater";
Settings.svelte:3   import { relaunch } from "@tauri-apps/plugin-process";
Settings.svelte:35        const update = await check();
Settings.svelte:64        await relaunch();
```

Or Tauri v2 n'accorde **pas** automatiquement les jeux de permissions par défaut d'un plugin : il
faut les lister dans une capability. Les identifiants existent bien côté plugins —
`tauri-plugin-updater-2.10.1/permissions/default.toml` définit
`["allow-check","allow-download","allow-install","allow-download-and-install"]` et
`tauri-plugin-process-2.3.1/permissions/default.toml` définit `["allow-exit","allow-restart"]` —
mais `updater:default` et `process:default` sont **absents** de `capabilities/default.json`.

**Conséquence attendue** : `check()` et `relaunch()` sont refusés par l'ACL à l'exécution, et le
bouton « Vérifier les mises à jour » ne peut pas fonctionner. Combiné à SC-04 (le manifeste
annonce 1.0.1 < 3.15.1, donc `should_update == false` de toute façon), **le canal de mise à jour
est mort deux fois**.

**Marqué « non prouvé »** : je n'ai pas lancé l'interface graphique pour observer le refus. La
chaîne de preuve est documentaire — capability lue, permissions des plugins lues, appels frontend
lus — il manque l'observation runtime.

**Correction exacte** : ajouter `"updater:default"` et `"process:default"` à
`capabilities/default.json`, puis vérifier le comportement réel avec `tauri dev`.

---

### SC-10 — MOYEN — `claude-review.yml` : secrets, droits d'écriture et contenu d'attaquant

**Ancres** : `.github/workflows/claude-review.yml:3-6`, `:15-19`, `:24-27`, `:29-31`, `:47`,
`:53-76`.

**À l'actif du projet, et c'est notable** : la « pwn request » classique a déjà été fermée, et le
commentaire `:10-14` explique exactement pourquoi. Le déclencheur `issue_comment` est restreint
par `author_association`, et le workflow utilise `pull_request` (pas `pull_request_target`) —
vérifié : aucun `pull_request_target`, `workflow_run` ni `repository_dispatch` dans les trois
workflows. Sur une PR issue d'un fork, GitHub ne fournit aucun secret, donc le garde-fou `:34-44`
fait tomber le job proprement. C'est correct.

**Ce qui reste ouvert.**

1. **Injection de prompt avec secrets en main.** Sur `issue_comment`, le job s'exécute dans le
   contexte du dépôt de base, **avec** les secrets et avec `pull-requests: write` +
   `issues: write` (`:24-27`). Le contenu que l'agent lit — diff de la PR d'un fork, corps des
   commentaires — est intégralement contrôlé par un attaquant. Scénario : l'attaquant ouvre une
   PR anodine dont un fichier contient des instructions destinées à l'agent ; un mainteneur
   commente « @claude » ; l'agent s'exécute avec `ANTHROPIC_API_KEY` en environnement, un droit
   d'écriture sur les issues et les PR, et un accès réseau. Le prompt (`:53-76`) ne contient
   aucune consigne de méfiance vis-à-vis du contenu analysé.
2. **`COLLABORATOR` est trop large.** Cette association couvre toute personne invitée sur le
   dépôt, y compris en accès *read* ou *triage*. Le garde-fou `:18` n'exige donc pas un droit
   d'écriture, contrairement à ce qu'annonce le commentaire `:13-14` (« people who already have
   write access »).
3. **Action non épinglée** : `anthropics/claude-code-action@v1` (`:47`), tag mutable, cf. SC-01.
4. **Pas de `permissions:` au niveau du workflow** — seul des trois dans ce cas. Les permissions
   sont posées au niveau du job, ce qui fonctionne, mais laisse le défaut du dépôt s'appliquer à
   tout job ajouté plus tard.
5. **Bug fonctionnel** : sur `issue_comment`, `actions/checkout@v4` (`:29-31`) sans `ref:`
   récupère la branche par défaut, pas la tête de la PR — la « revue » ne lit alors pas le code à
   relire. *Non prouvé* : `claude-code-action` peut récupérer la PR lui-même.

**Correction exacte** : remplacer le test `author_association` par une vérification d'API
(`gh api repos/{owner}/{repo}/collaborators/{user}/permission` → `admin|write`) ; épingler
l'action par SHA ; ajouter au prompt une instruction explicite de traiter le diff comme une
donnée non fiable ; ajouter `ref: refs/pull/${{ github.event.issue.number }}/merge` sur le chemin
`issue_comment`.

---

### SC-11 — MOYEN — La CI ne vérifie jamais que le lockfile revu est celui qui est compilé

**Ancres** : `.github/workflows/ci.yml:34`, `:36`, `:38`.

```
ci.yml:34        run: cargo check  --manifest-path src-tauri/Cargo.toml
ci.yml:36        run: cargo test   --manifest-path src-tauri/Cargo.toml
ci.yml:38        run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Aucun `--locked`. Si `Cargo.lock` est désynchronisé de `Cargo.toml`, Cargo **le réécrit
silencieusement** sur le runner et compile ce qu'il vient de résoudre. La revue humaine porte
alors sur un lockfile qui n'est pas celui du build. C'est aussi ce qui rend possible une dérive :
un intervalle `^` dans une dépendance transitive peut faire entrer une version publiée depuis,
sans qu'aucun diff n'apparaisse dans la PR.

*Vérification* : `cargo metadata --locked` réussit aujourd'hui — le lockfile **est** en phase. Le
défaut est l'absence de contrainte, pas un écart actuel. Noter aussi qu'il n'existe ni
`.cargo/config.toml`, ni `rust-toolchain.toml` : rien ne fige le compilateur.

**Correction exacte** : `--locked` sur les trois commandes, `--frozen` sur le job de release. Le
job frontend fait déjà bien les choses (`npm ci`, qui respecte le lockfile ; `package-lock.json`
en `lockfileVersion 3`, 126 dépendances, **126 avec `integrity`**, **0 source hors
`registry.npmjs.org`**).

---

### SC-12 — MOYEN — La release signée est construite depuis un cache, sans attestation

**Ancres** : `.github/workflows/release.yml:64-71`.

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            src-tauri/target        ← le répertoire de build est restauré, pas reconstruit
          key: ${{ matrix.platform }}-cargo-${{ hashFiles('src-tauri/Cargo.lock') }}
```

Mettre `src-tauri/target` en cache dans un job **qui signe** casse trois propriétés d'un coup :
(a) le binaire publié n'est pas issu d'un build propre — il hérite de tout objet resté dans le
cache ; (b) la construction n'est pas reproductible, donc personne ne peut la refaire pour
comparer ; (c) toute entrée capable d'écrire ce cache — par exemple une exécution de
`workflow_dispatch` sur une branche, cf. SC-01 — influence un artefact signé. La clé ne dépend
que de `Cargo.lock` : ni du commit, ni de la version du compilateur, alors que
`dtolnay/rust-toolchain@stable` peut avoir changé de compilateur entre deux entrées de même clé.

Complément : ni SBOM, ni `actions/attest-build-provenance`, ni hachages publiés. **Rien ne relie
le `Quanta.dmg` téléchargé à un commit du dépôt.**

**Correction exacte** : ne mettre en cache que `~/.cargo/registry` et `~/.cargo/git` (jamais
`target/`) dans le job de release ; inclure `${{ github.sha }}` et la version du toolchain dans
la clé ; ajouter `actions/attest-build-provenance` et publier un `SHA256SUMS`.

---

### SC-13 — MOYEN — `deny.toml` promet un verrou crypto qui n'existe pas

**Ancres** : `deny.toml:82-87`.

```toml
[bans]
# Les doublons de version augmentent la surface — signalés (warn), non bloquants.
multiple-versions = "warn"
wildcards = "warn"
# Verrou de sécurité : interdire explicitement toute crate crypto obsolète/risquée.
deny = []
```

Le commentaire annonce un verrou ; la liste est **vide**. Aucun `openssl`, aucune ancienne
`ring`, aucune crate crypto abandonnée n'est interdite. C'est un commentaire qui décrit une
intention et se lit comme un contrôle.

Et `multiple-versions = "warn"` laisse passer un arbre très dupliqué. Extrait du rapport
(60 crates dupliquées relevées), en ne gardant que ce qui touche la sécurité :

```
warning[duplicate]: found 2 duplicate entries for crate 'ed25519-dalek'
warning[duplicate]: found 2 duplicate entries for crate 'curve25519-dalek'
warning[duplicate]: found 2 duplicate entries for crate 'ed25519'
warning[duplicate]: found 3 duplicate entries for crate 'rand'
warning[duplicate]: found 3 duplicate entries for crate 'rand_core'
warning[duplicate]: found 4 duplicate entries for crate 'getrandom'
warning[duplicate]: found 2 duplicate entries for crate 'sha2'
warning[duplicate]: found 2 duplicate entries for crate 'signature'
```

Deux `ed25519-dalek` et deux `curve25519-dalek` dans un même binaire, c'est deux implémentations
de signature à auditer, deux jeux de correctifs à suivre, et une advisory qui peut ne s'appliquer
qu'à l'une des deux. Quatre `getrandom` signifient quatre chemins vers l'entropie du système.

**Correction exacte** : remplir `deny = []` (au minimum `openssl`, `openssl-sys`, `rustls` avant
0.23, `time` avant 0.3.36) ; ajouter `[bans] skip-tree` pour les doublons acceptés et passer
`multiple-versions = "deny"` sur les crates cryptographiques, afin qu'un troisième
`ed25519-dalek` déclenche une décision explicite.

---

### SC-14 — BAS — Fenêtre TOCTOU sur le cookie d'authentification RPC

**Ancre** : `src-tauri/src/rpc.rs:165-184`.

```rust
rpc.rs:173-182
    let mut raw = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);   // ✔ 32 octets, OsRng
    let token = hex::encode(raw);
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(&path, &token)?;                                // ← créé selon l'umask (0644)
    #[cfg(unix)]
    {
        std::fs::set_permissions(&path, Permissions::from_mode(0o600))?;   // ← restreint après
    }
```

Le jeton — qui autorise les méthodes RPC déplaçant des fonds (`SECURITY.md:92-94`) — est écrit
d'abord avec les permissions par défaut de l'umask (typiquement `0644`, lisible par tous les
utilisateurs locaux) et n'est restreint qu'ensuite. Entre les deux appels, tout processus local
peut le lire. Sur Windows, le bloc étant `#[cfg(unix)]`, **aucune** restriction n'est appliquée.

**Chemin d'exploitation** : attaquant local non privilégié sur une machine multi-utilisateurs, ou
processus tiers compromis, qui surveille
`~/Library/Application Support/quanta-protocol/.cookie` et le lit dans la fenêtre de course ;
il peut ensuite appeler `sendtoaddress`. Portée limitée (accès local requis), d'où **BAS** — mais
le correctif est trivial.

**Correction exacte** : créer le fichier avec le mode voulu dès l'ouverture —
`OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)` sous `cfg(unix)` — et
poser une ACL explicite sous Windows.

---

### SC-15 — BAS — `.gitignore` : rien n'a fuité, mais rien n'empêche une fuite

**Ancre** : `.gitignore` (24 lignes, lu intégralement).

**Ce qui est absent** : `*.key`, `*.pem`, `*.p12`, `*.pfx`, `*.jks`, `*.der` (matière de
signature) ; `*.db`, `*.sqlite*` (le vault) ; `*.sig`, `*.minisig` ; `src-tauri/fuzz/target/`,
`src-tauri/fuzz/corpus/`, `src-tauri/fuzz/artifacts/` ; `/target` à la racine ; `*.log`. `.env` et
`.env.*` sont bien couverts (`:7-8`), et `src-tauri/target/` aussi (`:14`).

Le risque concret est faible parce que `default_data_dir()` place la base **hors du dépôt** —
vérifié :

```
node_runtime.rs:60-70
    pub fn default_data_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("QUANTA_DATA_DIR") { ... }
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("quanta-protocol")
    }
```

— sauf si `dirs::data_dir()` échoue, auquel cas le repli est `./quanta-protocol/quanta.db` : nœud
lancé depuis le dépôt, le vault devient un fichier non ignoré, et un `git add .` l'envoie en clair
dans un commit public. `CONTRIBUTING.md:70` interdit la pratique (« Never commit secrets, keys,
vaults or `node_key` files ») ; c'est une règle sociale là où deux lignes de `.gitignore`
seraient un contrôle. À noter également : **la CI n'a aucun scan de secrets.**

**Correction exacte** : ajouter les motifs ci-dessus, et un hook `pre-commit` (gitleaks ou
`detect-secrets`) doublé d'un job CI.

---
