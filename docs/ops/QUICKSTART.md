# Quanta — Démarrer & tester

Guide concret pour lancer et éprouver ce qui a été construit : l'app de bureau, le
nœud headless `quanta-node`, son API JSON-RPC, l'explorateur web, le wallet
persistant et le multisig post-quantique.

> Statut : **alpha, non audité par un tiers**. QUANTA n'a **aucun prix de marché**.
> Les coins d'un réseau local n'ont aucune valeur — c'est fait pour expérimenter.

---

## 1. L'application de bureau (le plus simple)

```bash
npm install
npm run tauri dev
```
Tu obtiens le wallet : créer une identité, **Recevoir** (adresse `qta1…` + QR),
**Envoyer** (accepte `@pseudo`, `qta1…` ou hex — checksum vérifié), **Staker**,
l'écran **Minage** et la vue **Réseau** 3D. Thème clair.

---

## 2. Le nœud headless `quanta-node` + JSON-RPC

Construire une fois :
```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin quanta-node
BIN=src-tauri/target/debug/quanta-node
```

### a) Nœud watch/relais (sans clés)
```bash
$BIN --rpc-addr 127.0.0.1:8645
```
Il synchronise la chaîne et sert le RPC. Aucune clé, ne peut rien dépenser.

### b) Nœud-wallet souverain (détient, mine, envoie)
```bash
QUANTA_WALLET_PASSWORD='ton-mot-de-passe-8+' $BIN --mine
```
Au 1er lancement il **crée** un vault chiffré dans le data-dir ; aux suivants il le
**déverrouille** (même mot de passe → même adresse). `--mine` produit des blocs.

### c) Explorateur web public (lecture seule, exposable)
```bash
$BIN --rpc-addr 0.0.0.0:8645 --public
```
Ouvre **http://127.0.0.1:8645/** dans un navigateur → l'explorateur (hauteur,
finalité, **supply prouvée** vs plafond 100M, validateurs, derniers blocs,
recherche par hauteur/adresse/tx). En mode `--public`, les méthodes wallet/broadcast
sont **désactivées** (sûr à exposer).

Options : `--data-dir <chemin>` · `--rpc-addr <ip:port>` · `--mine` · `--public` ·
`-h`. Journalisation via `RUST_LOG=info`.

---

## 3. Exercer le RPC (curl)

Toutes les méthodes : `POST /` JSON-RPC 2.0. Montants en **µQTA entiers**
(1 QUANTA = 1 000 000 µQTA).

```bash
RPC=http://127.0.0.1:8645
q() { curl -s -X POST "$RPC" -H 'Content-Type: application/json' -d "$1"; echo; }

q '{"method":"listmethods"}'                                   # les 17 méthodes
q '{"method":"getinfo"}'                                       # hauteur, supply, protocole (6)
q '{"method":"getfinalityinfo"}'                               # finalité Casper-FFG (époque, quorum ⅔)
q '{"method":"getvalidators"}'                                 # set de validateurs bondés
q '{"method":"getblock","params":{"height":0}}'               # la genèse
q '{"method":"validateaddress","params":{"address":"qta1…"}}' # valide un checksum
q '{"method":"getbalance","params":{"address":"qta1…"}}'
q '{"method":"listtransactions","params":{"address":"qta1…"}}' # scan de dépôts
```

Nœud-wallet uniquement (pas en `--public`). Ces méthodes touchent des clés ou
déplacent de l'argent : elles exigent le **jeton du cookie**, un `Content-Type:
application/json` et une requête de même origine. Le jeton est écrit au démarrage
dans `<data_dir>/.cookie` (permissions `0600`) et son chemin est affiché dans le
journal du nœud.

```bash
COOKIE=$(cat "$HOME/Library/Application Support/quanta-protocol/.cookie")
qa() { curl -s -X POST "$RPC" \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $COOKIE" -d "$1"; echo; }

qa '{"method":"getwalletinfo"}'
qa '{"method":"getnewaddress"}'
qa '{"method":"sendtoaddress","params":{"address":"qta1…","amount_uqta":1000000}}'
```

> Sans ce garde, n'importe quelle page web ouverte sur la même machine pouvait
> atteindre `sendtoaddress` par un simple `fetch()` — le bind sur `127.0.0.1`
> écarte l'internet, pas le navigateur, qui est déjà local (C4, audit 2026-07-25).

---

## 4. Tester le multisig post-quantique (MSIG-1)

Calculer l'adresse d'un compte **M-of-N** (ici 2-of-3) — pur, sans clés :
```bash
q '{"method":"getmultisigaddress","params":{"pubkeys":["cleA","cleB","cleC"],"threshold":2}}'
```
→ une adresse `qta1…`. Vérifie qu'elle est **ordre-indépendante** (inverse les clés →
même adresse) et qu'une politique invalide (seuil > nombre de clés) est refusée.

> Le **flux de signature multi-parties** côté wallet (chaque détenteur signe
> hors-ligne, signatures combinées) arrive ensuite. La **vérification d'autorité**
> on-chain (quorum de N clés ML-DSA indépendantes) est déjà vivante et testée — voir
> `src-tauri/src/p2p/ledger.rs::verify_multisig`.

---

## 5. La boucle complète des tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml          # 437 tests + 1 intégration
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check                                            # svelte-check (0/0)
npm run build
```

---

## 6. Deux nœuds qui se parlent (P2P)

Lance deux daemons sur des data-dirs et ports différents ; l'un affiche son ticket
au démarrage (`RUST_LOG=info`), et `connect_peer` (ou l'auto-discovery) les relie.
Ils convergent sur la même chaîne. *(Le bootstrap par seed-nodes/mDNS est un
chantier à venir ; pour l'instant on relie via ticket.)*
