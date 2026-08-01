# Quanta — Monnaie souveraine pair-à-pair

> **A sovereign peer-to-peer currency. No server, no bank, no mint authority.**
> Minez la monnaie du réseau, gardez-la avec vos clés, échangez de la valeur — entre pairs, sans tiers de confiance.

<p>
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg">
  <img alt="Version" src="https://img.shields.io/badge/version-3.15.1-informational">
  <img alt="Protocole" src="https://img.shields.io/badge/protocole-TORUS%20v9-lightgrey">
  <img alt="Backend" src="https://img.shields.io/badge/backend-Rust%20%2B%20Tauri%202-orange">
  <img alt="Frontend" src="https://img.shields.io/badge/frontend-Svelte%205-ff3e00">
  <img alt="Tests" src="https://img.shields.io/badge/tests-513%20%2B%201-success">
  <img alt="Status" src="https://img.shields.io/badge/status-alpha%20non%20audit%C3%A9-yellow">
</p>

**Quanta** est une application de bureau (macOS · Windows · Linux) et un **nœud headless**
(`quanta-node`) qui font tourner un réseau pair-à-pair souverain. La pièce native est
**QUANTA** (plus petite unité : **µQTA**, 1 QUANTA = 1 000 000 µQTA) — **rare** (plafond dur
100M, émission décroissante) et **post-quantique**. Identité, wallet et clés restent locaux et
chiffrés. Le réseau remplace la banque.

> 📖 **Pour comprendre le système en profondeur** — conception, invariants, et les bugs qui ont
> façonné le design — lisez [**`docs/ARCHITECTURE.md`**](docs/ARCHITECTURE.md) *(en anglais,
> écrit pour être lu d'un bout à l'autre)*. C'est le point d'entrée recommandé pour un auditeur
> comme pour un ingénieur.

---

## ⚠️ État réel du projet — à lire avant tout

Quanta est un **logiciel alpha de recherche**. Rien ici n'est survendu ; ce qui suit est
vérifiable dans le code et dans la suite de tests.

| ✅ Implémenté et testé | 🧪 Testé en simulation / à petite échelle | ❌ Pas fait |
|---|---|---|
| Ledger µQTA déterministe : burn-and-mint, reorg, plafond 100M vérifié au consensus | Convergence multi-nœuds (simulation déterministe seedée, fautes réseau + byzantines) | **Audit de sécurité indépendant** |
| Identité = adresse ML-DSA + vault Argon2id / AES-256-GCM + phrase BIP39 24 mots | NAT traversal et découverte de pairs à grande échelle | **Testnet public** (aucun pair d'amorçage public, aucun réseau ouvert) |
| Autorité de transaction **ML-DSA-65 pur** (FIPS 204) | Oracle d'énergie — signal d'**affichage** seulement, hors du chemin monétaire depuis v8 | **Notarisation / signature OS** des binaires |
| Enveloppes gossip signées **ML-DSA-65** + KEX de transport **X25519MLKEM768** | Anti-sybil (PoC : réputation + poids de stake) | **Release binaire à jour** (la dernière release GitHub date de mai 2026 et ne correspond plus au code) |
| Gadget de finalité Casper-FFG **câblé en vivant** : votes, certificat ⅔, plancher irréversible, slashing STAKE→BURN, réconciliation de fork | Multisig M-of-N ML-DSA (on-chain, sans UX multi-partie ni test d'intégration) | **Valeur de marché** — QUANTA n'est coté nulle part, l'app n'affiche aucun prix |
| Récompense de bloc **recalculée** par chaque nœud et **partagée** avec les participants récents (v9) | | Vrai VRF cryptographique + VDF (ADR-004 ouverte) |
| Nœud headless + JSON-RPC (17 méthodes, authentifié) + explorateur web | | |
| 513 tests + 1 test d'intégration, `clippy -D warnings` propre, svelte-check 0/0 | | |

Les points durs, dits franchement :

- **Le réseau n'a jamais tourné au-delà de deux nœuds.** Pire : entre le fork v4 (18/07) et le
  01/08/2026, le nœud était **muet sur tout réseau réel** — `iroh-gossip` plafonne un message à
  4 096 octets alors qu'une enveloppe signée ML-DSA en pèse ~15 000, et l'émission échouait en
  silence. La suite était verte, le test d'intégration deux nœuds passait, et le jalon « vérifié
  entre deux machines » datait de *deux mois avant* la régression. Corrigé et vérifié en vivant ;
  l'histoire complète est dans [`docs/ARCHITECTURE.md` §7](docs/ARCHITECTURE.md#7-three-bugs-that-shaped-the-design).
  **L'échelle réelle reste non éprouvée** — deux daemons sur une même machine ne prouvent pas la
  traversée de NAT.
- **Le protocole a rompu neuf fois** (`TORUS_PROTOCOL_VERSION = 9`). Tout binaire, snapshot ou
  chaîne antérieurs sont incompatibles ; la dernière genèse a été rejouée le 18/07/2026.
- **Aucun audit externe.** Un [audit interne adversarial](docs/audit/AUDIT-INTERNE-2026-07-25.md)
  du 25/07/2026 a ouvert 4 critiques, 8 hauts et 4 moyens — tous corrigés derrière le fork v7 —
  mais un audit interne n'est pas un audit tiers. Le dossier de consultation (threat model,
  périmètre, RFQ) est prêt dans [`docs/audit/`](docs/audit/).
- **Cryptographie et consensus expérimentaux** : n'y stockez aucune valeur réelle.
- **Un seul bloc `unsafe`** dans tout le backend (interop AppKit pour l'état d'occlusion de la
  fenêtre, `src-tauri/src/guardian.rs`).

---

## Trois capacités

1. **Miner** — Gardez un nœud en ligne : produisez un bloc et vous touchez la moitié de sa
   récompense, l'autre moitié étant partagée entre les nœuds ayant produit récemment. Le montant
   est une **fonction pure de la chaîne** que chaque nœud recalcule — aucune mesure locale
   (watts déclarés, réputation) ne touche la monnaie. Émission décroissante vers un plafond dur
   de 100M (~120 QUANTA/h à la genèse), **zéro premine, zéro autorité d'émission**.
2. **Garder** — Votre identité est une paire de clés ; on vous joint par un court **`@pseudo`**
   ou par une adresse **`qta1…`** (Bech32m à checksum). La clé privée ne quitte jamais
   l'appareil (vault Argon2id + AES-256-GCM) ; une **phrase de récupération de 24 mots (BIP39)**
   restaure le compte. Aucun KYC, aucun tiers.
3. **Échanger** — Transférez des QUANTA entre wallets, autorisés par une signature **ML-DSA-65**
   liée à l'adresse de l'expéditeur, avec un **burn de 1 %** à chaque transfert. Stakez pour
   sécuriser le réseau, ou verrouillez des fonds derrière un **multisig M-of-N** post-quantique.

---

## Consensus — élection PoS + finalité Casper-FFG

**Nommage honnête** : l'élection du proposeur est **déterministe et publiquement vérifiable**,
ce n'est **pas** un VRF cryptographique. Aucune clé secrète n'entre dans le tirage, donc le
leader d'un slot est publiquement prévisible ; un beacon *enterré* (hash d'un bloc situé
plusieurs slots derrière le tip) empêche seulement le grinding immédiat. Un vrai VRF
(imprévisibilité) et un VDF (anti-grinding) sont au roadmap — [ADR-004](docs/decisions/), ouverte.

```
slot = hauteur de chaîne
beacon = BLAKE3(domaine ‖ hash du bloc enterré ‖ slot)
seed   = BLAKE3(domaine ‖ beacon ‖ slot ‖ round)
seed % stake total pondéré → leader déterministe
```

Le poids est **l'enjeu inscrit sur la chaîne** (tx `Stake`/`Unstake` scellées), donc une
fonction pure de la chaîne : identique sur un nœud vivant, restauré ou synchronisé. Minimum
1 QUANTA, fallback 30 s vers le suivant, bootstrap permissionless tant que personne n'a staké.

**Un bloc sur 16 est un slot ouvert**, proposable par n'importe quelle adresse, bondée ou non.
Sans lui le réseau se refermerait définitivement au premier staker : sans faucet, airdrop ni
premine, un nouvel arrivant a besoin d'un enjeu pour proposer et de proposer pour gagner. Le
prix est nommé et borné — le *trilemme de vulnérabilité Sybil* ([Platt, Platt & McBurney,
2024](https://doi.org/10.1080/17445760.2024.2352740)) démontre
qu'on ne peut être à la fois sans-permission, résistant aux Sybils et gratuit ; on relâche
« gratuit » d'exactement un seizième, et comme les slots sont cadencés **par la hauteur**, une
ferme d'un million de fausses identités n'en capte pas davantage qu'une seule.

Au-dessus, un **gadget de finalité de type Casper-FFG** rend l'histoire irréversible :
checkpoints par époque (E = 32 blocs), votes signés ML-DSA-65, certificat à **⅔ du stake**
(quorum gravé), règle justify/finalize, **accountable safety** (double-vote et surround
détectés, preuve non-répudiable, enjeu détruit STAKE→BURN — y compris l'enjeu en cours de
déverrouillage), fork-choice LMD-GHOST pondéré par le stake et ancré à la finalité. Le cœur est
écrit **sans IO et déterministe** (`src-tauri/src/sm/`) et éprouvé par un harnais de simulation
seedé (partitions, réordres, crashs, nœuds byzantins, invariants de sûreté / conservation /
émission vérifiés à chaque pas). Le câblage vivant (LIVE-1→4) fait circuler les votes en gossip,
maintient un plancher de finalité persisté qu'aucun fork ne peut franchir, applique le slashing
sur le ledger réel et réconcilie deux partitions qui ont scellé chacune plusieurs blocs.

---

## 🔐 Post-quantique — inventaire exact

Ce tableau dit ce qui est PQ **et ce qui ne l'est pas**.

| Surface | Primitive | État |
|---|---|---|
| Autorité de transaction (l'argent) | ML-DSA-65 (FIPS 204, crate `fips204`) | ✅ PQ pur — aucun repli classique |
| Votes de finalité (l'irréversibilité) | ML-DSA-65 | ✅ PQ pur |
| Enveloppes gossip (authentification des messages) | ML-DSA-65 | ✅ PQ depuis le fork v4 |
| Confidentialité du transport (QUIC / TLS 1.3) | X25519MLKEM768 (rustls + aws-lc-rs) | ✅ hybride PQ — défense *harvest-now-decrypt-later* |
| Identité de nœud (NodeId Iroh) | Ed25519 | ❌ **classique** — dette *upstream*, Iroh attend un consensus d'industrie sur la signature PQ des EndpointIds |
| Chiffrement au repos (vault) | AES-256-GCM + Argon2id (64 Mio, 3 itér., p=4) | ✅ symétrique, résistant Grover |

La clé ML-DSA est **dérivée de la graine** (XOF BLAKE3) : aucun secret supplémentaire persisté.
Les secrets sont `zeroize`és après usage. Une adresse est `BLAKE3(ADDR_DOMAIN ‖ clé publique
ML-DSA)`, présentée en Bech32m (`qta1…`).

---

## 🛡️ Sécurité du gossip

Chaque message entrant traverse ce pipeline, dans cet ordre :

```
taille (≤ 10 Mo) → désérialisation → ban du pair → identifiant d'enveloppe canonique
(id == BLAKE3 de la pré-image signée, sinon rejet) → sonde de déduplication en lecture
→ fraîcheur du timestamp (±90 s) → vérification de signature ML-DSA-65
→ insertion en déduplication (APRÈS authentification) → rate-limit adaptatif + nonce
anti-replay monotone → handler
```

L'ordre n'est pas cosmétique : dédupliquer **avant** d'authentifier permettait à n'importe qui
d'empoisonner le cache avec des identifiants forgés et de censurer gratuitement la
synchronisation d'un pair (corrigé en v7). S'y ajoutent : bannissement (3 signalements → 1 h),
rate-limit adaptatif `sqrt(pairs/4) × 30 msg/min` borné [15, 120], garde DoS (10 Mo par
enveloppe, 50 blocs par segment de chaîne), heuristique anti-éclipse, et des cartes par pair
bornées en mémoire. Les erreurs de déchiffrement sont opaques ; aucune clé privée n'apparaît
dans les logs, les erreurs ou les réponses JSON.

Protocole gossip : 11 messages (`Hello`, `RequestChain`, `ChainSegment`, `NewBlock`,
`BroadcastTx`, `PublishUsername`, `FinalityVote`, `FinalityFault`, `Ping`, `Pong`,
`ReportPeer`).

---

## 💎 Économie QUANTA

| Paramètre | Valeur |
|---|---|
| Plafond | **100 000 000 QUANTA**, dur, vérifié au consensus |
| Émission | décroissante : `(plafond − miné) / 50 000 000` µQTA par minute (~120 QUANTA/h à la genèse) |
| Récompense d'un bloc | `emission_for_block(offre minée avant ce bloc)` — **fonction pure de la chaîne**, frappée par le producteur et **recalculée par chaque récepteur** |
| Partage | **moitié au producteur, moitié à parts égales** entre les adresses ayant produit un bloc dans les 32 derniers — recalculé et **imposé** par chaque nœud, pas suggéré |
| Premine / autorité d'émission | **aucun** |
| Unité | 1 QUANTA = 1 000 000 µQTA — arithmétique entière, zéro flottant sur les soldes |
| Burn | **1 % détruit par transfert** (arithmétique entière, `amount / 100`) |
| Conservation | `Σ(dépensable + staké + en déverrouillage) + brûlé == miné`, vérifiée à chaque pas de simulation |
| Déverrouillage du stake | 10 080 blocs (~2 semaines), ≥ fenêtre de slashing (contrainte gravée) |

Chaque tick libère une fraction fixe de l'offre **restante** : le rythme baisse à l'approche du
plafond et le total tend vers 100M sans jamais l'atteindre. En absolu c'est lent — ≈1 % la
première année, la moitié du plafond vers ~66 ans (calendrier dans le
[whitepaper](WHITEPAPER_FR.md)). Miner coûte de l'électricité réelle, mais **un coût de
production n'est pas un prix** : QUANTA n'est coté nulle part.

---

## 🌐 Écosystème de nœud

- **`quanta-node`** — daemon headless (même cœur que l'app), wallet persistant, mine / détient /
  envoie.
- **JSON-RPC** — 17 méthodes (`getinfo`, `getblock`, `getbalance`, `validateaddress`,
  `getfinalityinfo`, `getvalidators`, `getmempool`, `listtransactions`, `sendrawtransaction`,
  `sendtoaddress`, `getmultisigaddress`…), montants en µQTA entiers. Les méthodes qui touchent
  à l'argent sont **authentifiées par jeton cookie** avec garde `Origin` / `Content-Type` —
  sans quoi un simple `fetch()` depuis une page web atteignait `sendtoaddress` (critique C4,
  corrigée). La surface de **lecture** reste volontairement ouverte.
- **Explorateur web** autonome servi par le nœud, plus un mode `--public` en lecture seule.
- **Multisig** — adresses M-of-N qui commettent à leur politique
  `BLAKE3(MSIG_DOMAIN ‖ clés triées ‖ seuil)`, vérification *rebind-proof*, sans nouveau champ
  wire. Première custody à quorum post-quantique du projet — l'UX multi-partie et le test
  d'intégration restent à faire.

---

## 🚀 Démarrage

Prérequis : Node 18+, Rust stable, toolchain Tauri 2.

```bash
npm install
npm run tauri dev                                      # app de bureau, hot-reload

cargo test --manifest-path src-tauri/Cargo.toml        # 513 tests + 1 intégration
cd src-tauri && cargo clippy --all-targets -- -D warnings
npm run check                                          # svelte-check (0 erreur / 0 warning)

npx tauri build                                        # bundle de production
```

Sur macOS le binaire **n'est pas notarisé** : `xattr -cr /Applications/Quanta.app`, puis clic
droit → Ouvrir. La notarisation est au roadmap.

---

## 🗂️ Structure

```
src-tauri/src/
├── lib.rs              ← run() / AppState / invoke_handler
├── commands/           ← commandes Tauri par domaine (identity, wallet, network, chain, diagnostics)
├── views.rs            ← view-models purs partagés app Tauri ↔ JSON-RPC
├── rpc.rs              ← serveur JSON-RPC (17 méthodes, authentifié)
├── node_runtime.rs     ← bootstrap partagé app + daemon
├── bin/                ← quanta-node (daemon headless)
├── sm/                 ← cœur sans-IO déterministe : finalité, slashing, fork-choice, simulation
├── p2p/
│   ├── pos_consensus.rs    ← élection de leader (déterministe, beacon enterré)
│   ├── ledger/             ← blockchain : état, validation, stake, slash, reorg
│   ├── gossip.rs           ← protocole (11 messages)
│   ├── dispatcher.rs       ← pipeline de réception
│   ├── finality_live.rs    ← câblage vivant du gadget (LIVE-1→3B)
│   ├── fork_heal.rs        ← réconciliation de fork profonde (LIVE-4)
│   └── …                   ← mining_loop, willow_node, shapley, username, state_persistence
├── security/           ← CryptoEngine, vault PQ, ML-DSA-65
└── storage/            ← persistance libSQL

src/                    ← frontend Svelte 5 (Wallet, Réseau, Minage, Contacts, Profil…)
```

---

## 🗺️ Feuille de route

Par ordre de ce qui bloque réellement :

1. **Deux machines physiques derrière deux NAT** — la seule épreuve qui décide de la traversée
   de NAT ; deux daemons sur un même hôte partagent une IP publique et ne percent rien.
   Procédure scriptée : [`docs/ops/two-machines.sh`](docs/ops/two-machines.sh).
2. **Audit de sécurité indépendant** — dossier prêt ([`docs/audit/`](docs/audit/)), pas encore commandé.
3. **Testnet multi-nœuds durable** sur la genèse actuelle — l'échelle réelle n'est pas éprouvée.
4. **Notarisation macOS** + pipeline de release signé, et une release à jour.
5. **Aléa d'élection** — vrai VRF + VDF (ADR-004).
6. **Multisig** — UX multi-partie et test d'intégration.
7. **Vision** — finalité BFT sous-seconde ([design DAG-BFT](docs/DESIGN-CONSENSUS-DAG-BFT.md)),
   identité de nœud post-quantique le jour où Iroh la livre.

---

## 📄 Documents

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — **commencez par là** : la visite guidée du
  protocole, la table des invariants avec leur lieu d'application, et les bugs qui ont façonné
  le design *(anglais)*.
- [`WHITEPAPER.md`](WHITEPAPER.md) · [`WHITEPAPER_FR.md`](WHITEPAPER_FR.md) — whitepaper.
- [`docs/ops/QUICKSTART.md`](docs/ops/QUICKSTART.md) — lancer l'app, le nœud headless, le RPC,
  et le test scripté à deux machines.
- [`docs/audit/`](docs/audit/) — audit interne du 25/07/2026, threat model, périmètre, RFQ.
- [`docs/decisions/`](docs/decisions/) — registre des ADR (validator set, fork-choice, slashing,
  gouvernance, portée du post-quantique…) : chaque décision avec l'alternative écartée.
- [`docs/economy/DOCTRINE.md`](docs/economy/DOCTRINE.md) — la doctrine économique, y compris les
  mécanismes envisagés puis refusés.
- [`SECURITY.md`](SECURITY.md) — divulgation de vulnérabilités.
- [`CLAUDE.md`](CLAUDE.md) — référence technique interne (architecture, invariants, historique
  détaillé de chaque fork).

---

## Licence

Distribué sous licence **Apache-2.0** — voir [`LICENSE`](LICENSE) et [`NOTICE`](NOTICE).

> **Avertissement** : logiciel expérimental fourni « en l'état », sans garantie d'aucune sorte.
> La cryptographie, le consensus et le réseau n'ont **pas** fait l'objet d'un audit indépendant.
> N'y stockez aucune valeur que vous ne pouvez pas vous permettre de perdre.

<p align="center"><strong>◈ Quanta ◈</strong></p>
