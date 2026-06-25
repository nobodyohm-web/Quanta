---
type: audit-report
id: HARDEN-AUDIT-1
status: livré (lecture seule)
date: 2026-06-22
mode: ultra-effort, fan-out 8 surfaces + vérification adverse par trouvaille
liens: [[QUANTA_HARDENING_AUDIT]] · [[QUANTA_AGENT_CONSTITUTION]] (§3, §4) · [[AUDIT_QUANTA_2_PROGRESS]] · [[QUANTA_SIGN_DET_VERIFY]] · [[ADR-001 — Fork-choice]] · [[ADR-002 — Validator set & comité BFT]] · [[ADR-003 — Slashing (accountable safety)]] · [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]]
---

# HARDEN-AUDIT-1 — Rapport d'audit de durcissement, Phase 0

> **Lecture seule.** Aucun code (prod ou test) n'a été modifié pour produire ce rapport.
> Chaque correctif est une **esquisse non appliquée**. Chaque trouvaille réelle devient
> ensuite **son propre spec chirurgical**, revu et gated individuellement.
> Méthode : 8 auditeurs en éventail (un par surface 1.1–1.8), puis **un vérificateur adverse
> par trouvaille** qui relit le `fichier:ligne` cité lui-même et tente de la **réfuter**
> (filtre décisif : *atteignable depuis un chemin consensus/réseau dans un build release,
> ou seulement test/sim ?*). 53 agents, ~2,6 M tokens. Voir §6 Provenance.

---

## 0. Synthèse

### 0.1 Verdict global

Le **cœur de consensus `sm/` est solide** : sans-IO vérifié (horloge par `Event::Tick`,
aléa par `&mut dyn Rng`), aucun `HashMap`-iteration sur un chemin de décision, aucun flottant
ni horloge murale sur un chemin de validation, déterminisme prouvé empiriquement (meta-test
128 runs byte-identiques). Le **socle monétaire entier** (u64 µQTA, burn `amount/100`,
`emission_for_tick` saturant, poids d'élection plafonné) est correct. SIGN-DET **non régressé**
(signature prod toujours `OsRng`-hedged, signeur déterministe `#[cfg(test)]`). La frontière
horloge **bord-réseau / cœur** est intacte (±90 s sur l'**enveloppe**, jamais sur le cœur).

La faiblesse est **concentrée au bord adversarial réseau** et sur **un second chemin de
validation qui diverge du premier** : le **plafond dur de 100 M est contournable via le
chemin de fork-reorg** ; **dizaines de slices d'octets** sur des `String` contrôlées par
l'attaquant peuvent rendre un nœud sourd au gossip ; le **nonce de tx n'est pas signé** et
ouvre un hang à distance ; des **maps par-expéditeur** sont alimentées **avant** la
vérification de signature.

### 0.2 Ce qui est solide (à préserver)

- **`sm/` sans-IO** : déterminisme transitif prouvé ; `sim.rs`/`p2p/simulation.rs` `#[cfg(test)]`,
  absents du release.
- **Arithmétique µQTA** : pas de f64 sur solde/burn/transfert ; pré-check du gross (AUDIT-TX-3) ;
  accumulateur d'élection en `u128` (overflow injoignable, test « whale »).
- **Liaison de contenu des blocs** : merkle root RFC-6962 lie `tx.signature` **et** `tx.nonce`
  (via `tx_content_bytes`) ; le hash de bloc est re-dérivé et re-vérifié (BLK-HASH-1).
- **Collections bornées bien faites** : `seen_messages` LRU 100K persistée ordonnée ; mempool
  `pending` pruné (TTL + cap 1000) avec retrait corrélé de `seen_tx_hashes` (ferme le replay
  gossip) ; `pending_pings` cap 256.
- **Zeroize des secrets détenus** : `SigningKey` / clé ML-DSA-65 `ZeroizeOnDrop`, gardes
  de compilation valides.
- **DST harness à dents** : `check_invariants` calcule conservation sur des sommes
  indépendantes (non tautologique), prouvé par scénarios plantés (phantom money, double-mint,
  split-brain).

### 0.3 Ce qui est faible

- **Asymétrie d'application du plafond** : le chemin de remplacement fork-reorg saute
  `validate_block_emission` → **inflation au-delà de 100 M par un adversaire réseau** (critique).
- **Slicing d'octets char-unsafe systémique** sur des champs `String` télécommandés → panic →
  nœud sourd au gossip (critique × 2 + haute × 2, **une seule classe**).
- **Champs de tx non authentifiés** (`nonce`, `hash` hors du préimage signé) → hang à distance
  ~2⁶⁴ sous le lock ledger + censure de compte (critique).
- **État par-expéditeur muté avant la signature** (maps de nonce/rate non bornées, slice de
  `sender`) → OOM distant + panic pré-auth (critique).
- **Acceptation de bloc « leaderless »** : l'acceptation ne vérifie pas que le proposeur était
  le leader élu — autorité PoS décorative côté réception (haute, **décision**).
- **Ledger CRDT fantôme** non borné + boucle CPU O(montant) par tx (moyenne × 2).

### 0.4 Top-N backlog (correctifs chirurgicaux, ordre = sévérité puis effort)

| # | id | sévérité | effort | trouvailles couvertes |
|---|----|----------|--------|------------------------|
| **1** | `FORK-CAP` | 🔴 critique | **XS** | C-1.2 = H-1.6 (réorg saute le plafond d'émission) |
| **2** | `SLICE-CLASS` | 🔴 critique | **S** | C-1.3-fork, C-1.3-sender, H-1.3-ledger, H-1.3-peerid (1 helper char-safe + valider hex au bord) |
| **3** | `PRESIG-ORDER` | 🔴 critique | **S** | H-1.5 (maps avant sig) + borne les maps ; retire aussi la joignabilité de C-1.3-sender |
| **4** | `TX-AUTH-NONCE` | 🔴 critique | **S** | C-1.6 (signer `nonce`+`hash`, recalculer le hash en réception, remplacer la boucle high-water par un set direct) |
| **5** | `CRDT-BOUND` | 🟠 moyenne | **M** | M-1.5-balances + M-1.5-cpu (gate sur l'admission, delta O(1), ou **retirer** le CRDT en mode crypto-only) |
| **6** | `ZEROIZE-SWEEP` | 🟠 moyenne | **S** | M-1.4-recovery (+ get-secret / argon2 / unlock-clone : `Zeroizing` sur les copies d'export/KDF) |
| **7** | `REGISTRY-CAP` | 🟡 basse | **S** | M-1.5-username (cap par `owner_pk`) |
| **8** | `COLL-CAPS` | 🟡 basse | **S** | peer_country (valider ISO), known_peers (GC), peer_info (cap global) |
| **9** | `TEST-TEETH` | 🟡 basse | **M** | sweep 1.7 (s3-replay, sim-emission-amount, int2-symétrique, shapley pré-norm, s5/p1, burn-boundary, thread-sleep) |

> `FORK-CAP` est **prioritaire absolu** : invariant central du projet, confirmé par **deux
> surfaces indépendantes**, correctif d'une ligne + un test de régression sur la branche fork.

### 0.5 Décisions d'Alexandre (à **lister**, jamais résoudre — §4, ADRs)

| id | sujet | ADR |
|----|-------|-----|
| `DEC-acceptance-leader` | Lier l'élection PoS à l'**acceptation** des blocs (aujourd'hui leaderless) | [[ADR-002 — Validator set & comité BFT]] |
| `DEC-rep-weight` | La réputation peut-elle pondérer l'élection, et si oui dérivée de la chaîne (pas mesurée localement) ? | (consensus) |
| `DEC-fork-choice` | Tie-break fork pondéré-stake vs hash lexicographique grindable | [[ADR-001 — Fork-choice]] |
| `DEC-tx-timestamp` | Borne temporelle des tx/blocs (median-time-past / skew) | (ouvert) |
| `DEC-slashing` | Pénalité d'équivocation (détection sim-only aujourd'hui) | [[ADR-003 — Slashing (accountable safety)]] |
| `DEC-finality` | Gadget de finalité / agrégation de votes BLS vs PQ / réconciliation multi-blocs | [[ADR-001]] · [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]] |

---

## 1. Classes racines (causes, pas symptômes)

L'audit ne s'est pas arrêté aux symptômes : quatre **classes** expliquent la majorité des
trouvailles. Corriger la classe ferme tous ses frères d'un coup.

- **CLASSE A — slicing d'octets char-unsafe pour les logs.** Des dizaines de sites font
  `&attacker_string[..N]` (dur `[..12]`, ou `[..len().min(N)]` qui borne la **fin** mais pas la
  **frontière UTF-8**). Tout champ télécommandé (`env.sender`, `block.hash`, `prev_hash`,
  `tx.id`, `block.miner`, `ReportPeer.peer_id`, `Hello.known_peer_ids`) panique sur un
  multioctet à cheval sur l'octet N. **Un helper `short()` char-safe + validation hex au bord**
  ferme C-1.3-fork (🔴), C-1.3-sender (🔴), H-1.3-ledger (🟠), H-1.3-peerid (🟠).
- **CLASSE B — état muté/traité avant la vérification de signature**, indexé par un
  `env.sender` non authentifié. Les maps `last_nonces`/`rate_counters` sont écrites aux étapes
  6–7 (rate, nonce), **avant** la vérif Ed25519 (étape 8). D'où à la fois l'OOM non borné
  (H-1.5 🔴) **et** la joignabilité du panic de slice `sender` (C-1.3-sender 🔴). **Réordonner
  la signature avant toute écriture par-expéditeur** ferme les deux ; **borner les maps** est le
  filet robuste (un porteur de clé peut encore les gonfler).
- **CLASSE C — deuxième chemin de validation (fork-reorg) qui diverge du premier.** La branche
  fork (`ledger.rs:1086`) ne fait que `validate_block_against_prev` ; le plafond
  (`validate_block_emission`) ne vit que dans `validate_remote_block` (chemin happy). D'où
  `FORK-CAP` (🔴) **et** le fait que le slice dur fork (C-1.3-fork) vit dans cette même branche.
  L'acceptation leaderless (H-1.6) est le même motif : **le chemin de réception sous-valide par
  rapport au producteur.**
- **CLASSE D — champs de tx hors du préimage signé.** `verify_tx` ne signe que
  `id:from:to:amount:timestamp:tx_type` ; `nonce` et `hash` sont **non authentifiés**. D'où le
  hang high-water (C-1.6 🔴) **et** le fait qu'un `hash` frais rejoue contre le dedup
  `seen_tx_hashes` (facilite `FORK-CAP`). **Lier `nonce`+`hash` à la signature et recalculer le
  hash en réception.**

---

## 2. Trouvailles classées

Format : `id` · surface · sévérité (auditeur → ajustée par le vérificateur) · joignable en
release · décision d'Alexandre. Preuve `fichier:ligne`. Correctif = **esquisse, non appliqué**.

### 2.1 🔴 Critique

#### `FORK-CAP` — la branche fork-reorg saute le plafond d'émission
- **Surface** 1.2 + 1.6 (C-1.2-fork-reorg-skips-emission-cap = H-1.6-reorg-skips-emission-cap, haute→**critique**). **Joignable : oui. Décision : non — bug d'omission.**
- **Confirmé deux fois indépendamment** (arithmétique et soundness de validation).
- **Preuve** : `p2p/ledger.rs:1086` (branche fork, prise quand `block.index == tip.index && block.hash > tip.hash`) n'appelle **que** `validate_block_against_prev`. Le plafond vit uniquement dans `validate_block_emission` (`ledger.rs:845` hard cap `MAX_SUPPLY_MICRO`, `:864` borne par bloc), invoqué seulement depuis `validate_remote_block:824` (chemin happy, `:1016`). `validate_block_against_prev:879-918` ne borne que le **nombre** de tx mining (`≤1`) et la forme du coinbase, **jamais le montant**. Le commentaire `ledger.rs:887-893` prétend faussement garder « les deux chemins ». `verify_tx:583-585` retourne `Ok(true)` pour `from=="NETWORK"` **sans signature ni montant**. Tests de régression `:1670`/`:1722` ne pilotent **que** le chemin happy (`index = tip.index+1`).
- **Scénario adverse** : l'attaquant forge un bloc à `index == tip.index`, `prev_hash` = `chain[tip.index-1].hash`, un seul mining `NETWORK→attaquant` de 50 M QUANTA, merkle/hash corrects, grind du timestamp jusqu'à `block.hash > tip.hash`. `integrate_remote_block` prend la branche fork, crédite le montant (`:1127-1130`), **le plafond 100 M est dépassé en silence** sur tout nœud qui adopte le fork → inflation par mint non signé **et split de consensus** (un nœud arrivé par le happy path rejette le même bloc).
- **Correctif (esquisse)** : dans la branche fork, après `validate_block_against_prev`, appeler aussi `self.validate_block_emission(&block)?` **avant** toute mutation, en évaluant `total_mined` contre la chaîne **post-pop** (ne pas double-compter la récompense du tip retiré). Ajouter un test de régression qui pilote la **branche fork** (même hauteur, hash supérieur), miroir de `trust_remote_block_cannot_exceed_hard_cap`.

#### `SLICE-FORK` — slice dur `&block.hash[..12]` dans la résolution de fork
- **Surface** 1.3 (C-1.3-fork-hash-hard-slice-no-min). **Joignable : oui. Décision : non.** Classe A + C.
- **Preuve** : `p2p/ledger.rs:1065-1066` et `1137-1138` — `&block.hash[..12]` / `&tip.hash[..12]` **durs**, sans `.len().min()` ni `is_char_boundary`. `block.hash` est un `String` libre (`ledger_types.rs:50`), payload non authentifié. La **même fonction** utilise le motif sûr `&block.miner[..block.miner.len().min(12)]` à `:1051` → omission incohérente.
- **Scénario adverse** : bloc via `NewBlock`/`ChainSegment` après la signature de l'**enveloppe** (l'attaquant signe la sienne) ; `block.hash = "\u{ff}\u{ff}"` (4 octets, lexicographiquement > un hash hex ASCII). Le dedup/happy ratent, la branche fork matche, `block.hash > tip.hash` vrai → `&block.hash[..12]` panique **avant** toute validation. `dispatch_incoming` tourne dans une **unique** tâche `tokio::spawn` (`gossip_tasks.rs:60-66`, lancée une fois `lib.rs:764`) **sans `catch_unwind`** → la boucle meurt, le nœud devient **sourd à tout le gossip** (pas de `panic=abort` → unwind, le process survit mais l'ingest réseau est mort). Le gossip propage le bloc → DoS réseau.
- **Correctif (esquisse)** : helper char-safe `fn short(s:&str)->&str { ... is_char_boundary ... }` aux deux sites, **ou** valider hex `block.hash`/`prev_hash` à la désérialisation. Ne pas changer la sémantique du fork-choice. → fusionner dans `SLICE-CLASS`.

#### `SLICE-SENDER` — slice de `env.sender` char-unsafe sur le chemin pré-signature
- **Surface** 1.3 (C-1.3-dispatcher-sender-multibyte-preverify). **Joignable : oui. Décision : non.** Classe A + B.
- **Preuve** : `p2p/dispatcher.rs:339-340` (log signature invalide), idem `:307` (rate), `:327-328` (nonce) : `&env.sender[..env.sender.len().min(12)]`. `env.sender` est un `String` (`gossip.rs:40`) désérialisé sans validation hex. `.min(12)` borne la fin mais pas la frontière UTF-8.
- **Scénario adverse** : une enveloppe forgée d'un pair frais passe ban/dedup/freshness/rate/nonce, **échoue** la vérif de signature (sender non-hex), atteint `:340` → `&env.sender[..12]` panique sur un multioctet **avant** le `broadcast(ReportPeer)` à `:344`. Un seul paquet non sollicité, **sans clé**. Même DoS « nœud sourd ».
- **Correctif (esquisse)** : `fn sender_tag(s:&str)->&str` char-safe à chaque site `&env.sender[..*.min(N)]` ; **mieux** : valider hex `env.sender` juste après le parse JSON et droper sinon (rend tous les slices aval ASCII). La cause profonde (Classe B) est de réordonner la signature avant ces logs → voir `PRESIG-ORDER`.

#### `NONCE-DOS` — `tx.nonce` non signé + boucle high-water ~2⁶⁴ sous le lock ledger
- **Surface** 1.6 (C-1.6-unsigned-nonce-highwater-dos). **Joignable : oui. Décision : non.** Classe D.
- **Preuve** : `p2p/ledger.rs:592-595` signe `format!("{}:{}:{}:{}:{}:{:?}", id, from, to, amount, timestamp, tx_type)` — **`nonce` et `hash` absents** du préimage. `ledger.rs:499-502` : `let new_hw = current.max(nonce.saturating_add(1)); for _ in current..new_hw { self.increment_nonce(&from); }`. `ledger_types.rs:26` `pub nonce: u64 #[serde(default)]`. Le garde du dispatcher `:750` (`if nonce+1 < high_water`) ne rejette que les nonces **bas**. `seen_tx_hashes` est clé sur le `tx.hash` **du wire, jamais recalculé** en réception. La boucle s'exécute sous `ledger.write().await` (`dispatcher.rs:778`).
- **Scénario adverse** : capter une tx signée d'une victime, la re-sérialiser avec `nonce=u64::MAX` + un `hash` frais (tous deux non signés → `verify_tx` toujours vrai), diffuser. Admise, puis `apply_verified_remote_tx` entre dans une boucle ~2⁶⁴ **tenant le lock global** → mining/seal/sync **figés à jamais** (DoS dur d'un message). Effet secondaire : high-water de la victime poussé à `u64::MAX` → **toutes ses vraies tx futures (petits nonces) rejetées réseau-wide** = censure permanente.
- **Correctif (esquisse)** : (1) inclure `nonce` dans le préimage signé (`verify_tx` **et** `build_signed_tx_at`) et **rejeter** toute tx gossip dont `hash != hash recalculé du contenu` ; (2) remplacer la boucle par un set direct `account_nonces.insert(from, new_hw)` pour tuer la complexité algorithmique indépendamment.

#### `NONCE-MEM` — maps par-expéditeur non bornées, alimentées avant la signature
- **Surface** 1.5 (H-1.5-noncetracker-presig-unbounded, haute→**critique**). **Joignable : oui. Décision : non.** Classe B.
- **Preuve** : `dispatcher.rs:59-61` `last_nonces: HashMap<String,u64>`, `rate_counters: HashMap<String,(u64,u32)>`. Insertions `.entry(sender_pk.to_string()).or_insert(..)` à `:91` (`check_and_advance`) et `:125-128` (`check_rate_limit`). Dans `dispatch_incoming` : rate `:304`, nonce `:324`, **puis** `verify_envelope_signature` `:337`. Grep arbre entier : **aucun** `.remove/.retain/.clear/éviction/cap** sur ces deux maps. Le `#[allow(dead_code)] // Used in security_tests` est trompeur — la struct est sur le chemin de réception **production**.
- **Scénario adverse** : **sans clé**, sprayer des enveloppes JSON bien formées, chacune avec un `sender` frais + `id` frais (passe le dedup). Chaque expéditeur distinct ajoute **définitivement** une entrée à `last_nonces` **et** `rate_counters`. La signature qui rejetterait le faux sender s'exécute **après** → n'empêche jamais l'insertion → **croissance RAM non bornée = OOM distant**. (Amplification faible ~300 o/msg, mais classe DoS distant = critique.)
- **Correctif (esquisse)** : (a) réordonner pour que la vérif Ed25519 précède toute écriture par-expéditeur, **et/ou** (b) borner les deux maps (évincer les fenêtres rate périmées ; cap LRU ∝ pairs vivants). Le signature-gating seul laisse un porteur de clé les gonfler → le cap est le filet robuste.

### 2.2 🟠 Haute

#### `SLICE-LEDGER` — slices char-unsafe `[..min(16)]` dans les logs de validation de bloc
- **Surface** 1.3 (H-1.3-ledger-blockfield-multibyte-min-slices). **Joignable : oui (sites 923/943). Décision : non.** Classe A.
- **Preuve** : `ledger.rs:923` `&tx.id[..tx.id.len().min(16)]` ; `:943` `&block.hash[..*.min(16)]`. Tous `String` (`ledger_types.rs:15,50`). **Correction du vérificateur** : `:819-820` et `:883-884` sont des branches **mortes** (`validate_remote_block` n'est appelée que sous `block.prev_hash == tip.hash`, et la branche fork retourne `Err` à `:1082` avant `:1086`). Les sites **réels** sont `:923` (sur `verify_tx → Ok(false)`, tx à `id` multioctet + signature invalide) et `:943` (sur mismatch de hash, `block.hash` multioctet).
- **Scénario adverse** : même mécanique char-boundary que les critiques mais protégée par `.min(N)` → seul un multioctet à cheval sur l'octet N panique. Atteint via `NewBlock`/`ChainSegment` sur les chemins de **rejet**. Même DoS « tâche unique → nœud sourd ». **Haute** (pas critique) : DoS distant sans perte de fonds ni divergence de consensus.
- **Correctif (esquisse)** : helper char-safe à chaque `&<champ>[..*.min(N)]` des logs de `ledger.rs`, ou valider hex `block.hash`/`prev_hash`/`tx.id` en tête de `validate_block_against_prev`/`verify_tx`. → `SLICE-CLASS`.

#### `SLICE-PEERID` — slices char-unsafe sur `ReportPeer.peer_id` et `Hello.known_peer_ids`
- **Surface** 1.3 (H-1.3-dispatcher-peerid-multibyte-slices). **Joignable : oui. Décision : non.** Classe A.
- **Preuve** : `dispatcher.rs:821`/`:836` `&peer_id[..peer_id.len().min(12)]` (`handle_report_peer`) ; `:666`/`:678`/`:686` `[..*.min(16)]` (`handle_hello` NET-2). `peer_id` vient de `ReportPeer{peer_id:String}` (`gossip.rs:102`) et `known_peer_ids:Vec<String>` (`gossip.rs:79`) — **champs payload ≠ sender vérifié**, jamais validés hex.
- **Scénario adverse** : distinct des slices `sender` — l'enveloppe peut être **parfaitement signée** par une clé légitime (passe les 9 étapes), mais le **payload** porte un `peer_id`/`known_peer_ids[i]` à multioctet sur l'octet 12/16 → panic **dans** le handler. `:821` log inconditionnel à chaque `ReportPeer`. Même DoS gossip tâche-unique. (`:666` `spawn(connect_peer)` sur ces ids non validés = préoccupation secondaire, hors 1.3.)
- **Correctif (esquisse)** : helper char-safe à `:666/678/686/821/836` ; et/ou rejeter les entrées `ReportPeer`/`known_peer_ids` non-hex-64 avant de logger/agir.

#### `LEADERLESS-ACCEPT` — l'acceptation ne vérifie pas le leader élu
- **Surface** 1.6 (H-1.6-acceptance-leaderless-no-election-check). **Joignable : oui. Décision : OUI (Alexandre).** Classe C.
- **Preuve** : `validate_remote_block:806-825` vérifie index/prev_hash/`validate_block_against_prev`/`validate_block_emission` mais **jamais** `is_valid_proposer`/`elect_leader`. Grep arbre : seuls appelants de `is_valid_proposer`/`elect_leader`/`build_validator_set` hors `pos_consensus.rs` = `mining_loop.rs:212/238` (**producteur**) et `sm/node.rs:382` (proposeur du cœur) — **aucun** chemin `*_remote_block`. `handle_new_block` (`dispatcher.rs:850-864`) passe `block.miner` directement, `env.sender` seulement pour le log.
- **Scénario / invariant** : tout pair (zéro stake, jamais élu) peut sceller et diffuser un bloc avec `miner` = n'importe quelle pubkey ; s'il chaîne au tip et passe merkle/hash/émission, il est accepté à `tip+1`. L'autorité pondérée-stake documentée (`pos_consensus.rs:43-49`) est **décorative côté acceptation**. Frère de la classe « hash aveugle au contenu » (le datum « qui peut proposer » n'est pas vérifié). **Haute** et **non critique** : `validate_block_emission` garde encore le plafond et le coinbase crédite `block.miner==sealer` → pas d'inflation ni de vol par ce trou seul.
- **Décision** : lier le validator-set à l'état chaîne + gérer le bootstrap (`mining_loop.rs:216-221` permissionless) est une **décision de consensus** → [[ADR-002 — Validator set & comité BFT]]. **Ne pas résoudre.** Esquisse : dans `validate_remote_block`, reconstruire validator-set + beacon enterré pour `block.index` et exiger `is_valid_proposer(block.miner, …)`.

### 2.3 🟠 Moyenne

#### `CRDT-MEM` — ledger CRDT fantôme non borné, persisté
- **Surface** 1.5 (M-1.5-crdt-balances-unbounded). **Joignable : oui. Décision : non.**
- **Preuve** : `consensus.rs:27` `balances: HashMap<String, PNCounter<String>>` ; `credit`/`debit` font `.entry(addr.to_string()).or_default()` (`:49-51`, `:68-70`), **aucun retrait** (grep `remove/retain/clear` = 0). Piloté réseau : `dispatcher.rs:766-770` `if tx_type==Transfer { debit; credit }` pour chaque `BroadcastTx`, **avant** l'admission autoritative (`apply_verified_remote_tx` `:779`) et **sans** garde de solde. Persisté (`state_persistence.rs` KEYS inclut `consensus`). `CRDT balance_of` a **zéro** consommateur hors `consensus.rs` (seul `account_count()` lu pour le frontend `lib.rs:388`).
- **Scénario** : un porteur de clé diffuse des Transfer signés vers un flux d'adresses `to` fraîches → chaque nouvelle adresse ajoute une entrée + `PNCounter` **persistée, jamais récupérée**, même pour des tx que le ledger linéaire rejette ensuite. Croissance disque+RAM lente mais non bornée d'un **ledger fantôme non autoritatif**. **Moyenne** (rate-limited ~30 clés/min/pair, pas de divergence de consensus ni perte de fonds).
- **Correctif (esquisse)** : gater la mutation CRDT sur le **même** résultat d'admission que le ledger linéaire (muter seulement si `apply_verified_remote_tx` = true), et/ou GC des entrées à solde 0 absentes du `balance_cache`. **Reconsidérer si le double ledger CRDT est encore utile en mode crypto-only** (candidat à suppression).

#### `CRDT-CPU` — boucle unaire O(montant) jusqu'à 10 M itérations par tx sous le lock
- **Surface** 1.5 (M-1.5-crdt-credit-cpu-loop). **Joignable : oui. Décision : non.**
- **Preuve** : `consensus.rs:52-55` `for _ in 0..clamped { let op = counter.inc(actor); counter.apply(op); }` avec `clamped = uqta.min(MAX_CRDT_BATCH)`, `MAX_CRDT_BATCH = 10_000_000` (`:37`). Atteint à chaque `Transfer` gossip (`dispatcher.rs:767-770`) en tenant `consensus.write().await`. Le solde n'est pas vérifié avant la boucle (`VerifiedTx::new` ne fait que `verify_tx`).
- **Scénario** : un Transfer signé de ≥10 QUANTA force 10 M itérations (×2 avec le debit) par message, sérialisées sous le lock consensus → CPU épinglé + lock bloqué par message. **Moyenne** : dégradation (dizaines–centaines de ms), pas crash/fonds/divergence ; borné par `MAX_CRDT_BATCH` et le rate-limit.
- **Correctif (esquisse)** : remplacer la boucle unaire par un **delta bulk** O(1) — le crate `crdts` expose `PNCounter::inc_many/dec_many` (`pncounter.rs:125/133`) — ou stocker un solde entier direct ; gater aussi sur l'admission linéaire.

#### `REP-WEIGHT` — le poids du leader mélange une réputation mesurée localement
- **Surface** 1.6 (M-1.6-reputation-weight-locally-measured). **Joignable : oui. Décision : OUI. Known-open.**
- **Preuve** : `pos_consensus.rs:94-97` `weight = stake + reputation.saturating_mul(10_000).min(stake)`. La réputation = `trust_score` de `reputation.rs:91-97` (`uptime_factor + energy_factor + stake_factor`), où `uptime_minutes`/`energy_kwh` sont **mesurés localement** (incrémentés `:130-131`). `mining_loop.rs:200-212` construit le validator-set depuis le leaderboard **local**. Rien n'est dérivé du contenu de la chaîne.
- **Scénario / invariant** : même set, même slot → l'élection est fonction des poids, mais les poids embarquent l'uptime/énergie **locaux** → nœud A et B élisent des leaders différents. Aujourd'hui sans fork de **validation** (acceptation leaderless, cf. `LEADERLESS-ACCEPT`) → impact = désaccord côté **producteur** (blocs concurrents résolus au hash) + liveness. **Devient un fork dur le jour où l'élection garde l'acceptation.** Frère de l'item « divergence de réputation » déjà corrigé : la réputation reste un **input de poids de consensus** et reste **mesurée localement**.
- **Décision** : décider si la réputation peut pondérer l'élection, et si oui la dériver d'une quantité **engagée on-chain**. **Ne pas résoudre.**

#### `FORK-HASH-CHOICE` — tie-break fork = hash lexicographique, pas pondéré-stake
- **Surface** 1.6 (M-1.6-fork-choice-hash-not-stake-weighted) + 1.8b. **Joignable : oui. Décision : OUI. Known-open.**
- **Preuve** : `ledger.rs:1057-1061` résout les forks même-hauteur par `if block.hash > tip.hash { remote gagne }` — comparaison de hash pure. `pos_consensus.rs:43-49` documente pourtant « le bloc du leader élu gagne ; sinon repli sur le hash ». L'implémentation ne consulte **jamais** `elect_leader` (grep confirme). Limitation single-block assumée délibérée (`sm/node.rs:1392`).
- **Scénario / invariant** : la métrique de fork-choice est le **hash**, contrôlé par le proposeur (grindable via timestamp + ordre des tx) → un attaquant gagne presque toujours le tie-break en grindant un hash supérieur, **indépendamment du stake/élection**. Les nœuds **convergent** (métrique déterministe) → pas un bug de divergence, mais un choix de **sécurité/qualité de chaîne** : le grinding bat l'autorité-stake voulue.
- **Décision** : préférer le bloc du leader élu de slot, repli hash sinon (règle documentée). Item ouvert du fork-choice intérimaire single-block → [[ADR-001 — Fork-choice]]. **Ne pas résoudre.**

#### `RECOVERY-KEY-ZEROIZE` — `get_recovery_key` fuit le secret maître Ed25519 non effacé
- **Surface** 1.4 (M-1.4-recovery-key). **Joignable : non (commande Tauri locale). Décision : non.**
- **Preuve** : `lib.rs:117-125` `let secret = engine.get_secret_bytes()?; let hex = hex::encode(&secret); … formatted.join("-")` — ni `secret`, ni la copie, ni `hex` ne sont zeroizés. `get_secret_bytes` (`mod.rs:75-77`) retourne `signing_key.to_bytes().to_vec()` **sans** `Zeroizing`. Le seed Ed25519 32 o est copié sur le heap (Vec) **et** en `String` hex, tous deux libérés sans wipe — frère du trou Ed25519 déjà corrigé, au **bord d'export**.
- **Invariant menacé** : règle §3 « zeroize tout secret ». Impact **local** uniquement (forensic mémoire/swap/core-dump sur la machine du propriétaire ; aucun vecteur réseau/consensus) → **moyenne**.
- **Correctif (esquisse)** : `get_secret_bytes` retourne un `Zeroizing<Vec<u8>>` ; lier `secret` et `hex` en `Zeroizing`. → `ZEROIZE-SWEEP`.

### 2.4 🟡 Basse

> Réelles mais à impact borné (hygiène locale, croissance lente rate-limited, ou test-only).
> Plusieurs ont été **descendues** de moyenne par le vérificateur (raison : pas de chemin
> réseau/consensus, ou `#[cfg(test)]` absent du release).

**Hygiène des clés (zeroize, frères de `RECOVERY-KEY`, → `ZEROIZE-SWEEP`)**
- `GET-SECRET-ZEROIZE` (1.4, moyenne→basse) — `mod.rs:75-77` retourne un `Vec` non-zeroizing ; tableau transitoire `to_bytes()` non effacé. (Le seul autre appelant `pq_vault.rs:50` wipe déjà.)
- `ARGON2-KEY-ZEROIZE` (1.4, moyenne→basse, non joignable) — `cipher.rs:14-22` `derive_key` retourne `[u8;32]` ; `enc_key` jamais zeroizé à `pq_vault.rs:54/84` (le secret, lui, est wipé `:57/:91`).
- `UNLOCK-SK-CLONE-ZEROIZE` (1.4, moyenne→basse) — `pq_vault.rs:85-91` : `sk_bytes.clone().try_into()` → `sk_arr` non effacé ; le `zeroize()` visible `:91` n'efface que l'original (fausse assurance).

**Mémoire bornée (caps manquants, → `COLL-CAPS`)**
- `USERNAME-REGISTRY` (1.5, moyenne→basse) — `username.rs:203` `by_name` sans cap par `owner_pk` ni TTL ; `rebuild_by_pk` O(n) à chaque insert → coût quadratique. Une clé peut revendiquer un espace énorme de handles valides signés, persistés. (Rate-limité ~120/min → spam lent.) → `REGISTRY-CAP`.
- `PEER-COUNTRY-UNBOUNDED` (1.5) — `willow_node.rs:111` + `dispatcher.rs:620-626` : `peer_country_reports` clé sur un `country` String brut (pas de validation ISO). Correctif : valider contre l'ensemble ISO-3166 connu (l'oracle énergie énumère ~33 pays).
- `KNOWN-PEERS-NO-CAP` (1.5) — `willow_node.rs:140,279-281,322` : `known_peers` sans cap global ni GC des entrées épuisées. Correctif : évincer/cap au cleanup.
- `PEER-INFO-NO-GLOBAL-CAP` (1.5) — `willow_node.rs:113,202-211` : `peer_info` TTL-borné (300 s) mais sans cap **instantané** → fenêtre de burst Sybil. Correctif : LRU global par `last_seen`.

**Arithmétique (à durcir, pas de bug live)**
- `SHAPLEY-FLOAT-PAYOUT` (1.2, non joignable comme bug) — `reputation.rs:160` `(share * emission as f64) as u64` ; `share` vient d'une normalisation f64 sommée en itérant un `HashMap` (`shapley.rs:179`, addition non-associative). **Sûr aujourd'hui** : récompense **locale par nœud**, jamais une valeur de consensus inter-nœuds. Correctif : rendre `compute_all_shares` ordre-indépendant (trier les ids / domaine entier) ; **documenter** que la sortie shapley ne doit jamais devenir une valeur de consensus tant qu'elle est flottante.
- `I128-CACHE-NEGATIVE` (1.2, known-open) — `balance_cache: HashMap<String,i128>` (`ledger.rs:63`) ; `cache_apply_tx` débite sans garde (`:189`) ; `balance_of`/`all_balances` saturent `.max(0)` (`:1216/:1225`) → masque un négatif au lieu de surfacer la divergence. Correctif : garder l'`i128` (bon choix) **+** `debug_assert`/invariant périodique « aucun solde chaîne négatif » et sommer l'`i128` brut (pas la version saturée) dans toute assertion de conservation.
- `REMOTE-MIN-AMOUNT` (1.2) — `transfer_with_burn_at` rejette `amount==0` / `amount<10_000` **seulement** à la construction locale (`ledger.rs:336/400`) ; l'admission distante (`:511-528`) ne vérifie que signature+nonce. Correctif : si la règle de montant minimal est un invariant **réseau**, l'appliquer dans le chemin d'admission partagé ; sinon la documenter comme politique locale.

**Vacuité des tests (→ `TEST-TEETH`, tous `#[cfg(test)]`, absents du release)**
- `s3-replay-never-replays` (1.7, moyenne→basse) — `security_tests.rs:127-145` : `s3_replay_tx_rejected` ne **rejoue jamais** ; une seule transfer + un `assert_eq!` de solde. Couverture fausse sur l'anti-replay. Correctif : re-soumettre la tx signée via `apply_verified_remote_tx`, asserter rejet **et** solde inchangé.
- `sim-emission-count-only` (1.7, moyenne→basse, sim-only) — `sim.rs:434-450` : la règle d'émission de `check_invariants` ne teste que `mining COUNT > 1`, jamais le **montant** par bloc. Un overflow de mint serait **invisible** au checker. Correctif : ajouter un invariant montant indépendant (`Σ amounts Mining ≤ emission_for_tick(supply_avant)` et `total ≤ MAX_SUPPLY`) + un test à dents plantant un bloc sur-montant.
- `int2-cache-onesided` (1.7, moyenne→basse) — `integration_tests.rs:164-171` n'itère que les clés de `full_scan` → un compte fantôme du `balance_cache` passerait en silence. Correctif : comparer les **ensembles** de clés (somme + cardinalité).
- `shapley-sumto1-tautology` (1.7) — `shapley.rs:186-190` normalise puis les tests affirment la somme forcée (`:261`, `security_tests.rs:911-931` accepte même `near_one || near_zero`). Correctif : tester les scores **pré-normalisation** contre des valeurs calculées à la main + des ratios normalisés spécifiques.
- `tests-with-no-assertion` (1.7) — `security_tests.rs:244-246` et `:578` (`s5_negative_balance`, `p1_balance_never_negative`) **jettent** le solde sans assertion. Correctif : asserter le résidu exact + somme des soldes == minted.
- `transfer-burn-boundary-noassert` (1.7) — `security_tests.rs:972-987` « pas de panic = passé » ; le sibling `:962` `prop_assert_eq!(burn, amount/100)` **re-dérive** la formule de prod (ne peut pas détecter un mauvais taux). Correctif : asserter le post-état par borne + comparer le burn à une **table indépendante**.
- `thread-sleep-in-tests` (1.7, flaky) — `security_tests.rs:830`, `integration_tests.rs:84-88` : `std::thread::sleep` pour forcer des timestamps distincts. Correctif : blocs distincts **par contenu** (miner/reward), pas par horloge.

**Décisions latentes / known-open (signalées, → §4)**
- `REP-F64-LATENT` (1.1, décision, known-open) — `reputation.rs:91-96` f64 → `pos_consensus.rs:94-97` ; frontière f64→u64 à `mining_loop.rs:207` (`u.trust_score as u64`). Latent : **deviendrait** un fork si l'élection garde l'acceptation. Correctif (à enregistrer, pas à appliquer) : invariant « l'élection-proposeur est **consultative**, jamais une règle d'acceptation » ; si cela change, quantifier la réputation à une frontière déterministe unique. Cluster avec `REP-WEIGHT` + `LEADERLESS-ACCEPT`.
- `TX-TIMESTAMP-UNBOUNDED` (1.1 / 1.6 / 1.8f, décision, known-open) — `ledger.rs:592-595` : `tx.timestamp` signé mais **non borné** à la validation ; idem `block.timestamp` (`:930-938`, pas de monotonie). Ancien check ±5 min retiré délibérément (`:319-326`, « validation reste sans horloge »). ±90 s s'applique à l'**enveloppe** (`dispatcher.rs:536`), pas à la tx. → `DEC-tx-timestamp`.

---

## 3. Confirmations de non-régression (positives)

Vérifiées, **rien à changer** — l'état attendu tient :
- **SIGN-DET non régressé** (INFO-1.3) : `build_signed_tx_at` branche `det_sign` `#[cfg(test)]` (`ledger.rs:1364-1368`) → signature release toujours hedged ; `verify_ed25519`/`verify_ml_dsa` parsent les octets adverses via `try_into().ok()?` (pas de panic).
- **Frontière horloge bord/cœur intacte** (I-1.1-edge) : `dispatcher.now_epoch_secs` lit `SystemTime` **mais** `validate_envelope_at(.., now_secs)` prend le temps **injecté** ; les wrappers prod de `ledger.rs` stampent `Utc::now()` puis délèguent aux variantes `_at` injectées. Aucune lecture d'horloge dans le cœur/validation.
- **±90 s au bord, non fuité** (INFO-1.3-freshness) : `is_fresh` sur `env.timestamp` au bord ; `pos_consensus` garde `% total_weight` (==0), index `eligible[0]` gardé par `is_empty`, `b[0..8]` d'un BLAKE3 32 o.
- **Merkle lie signature + nonce** (1.6 solide) : `tx_content_bytes` inclut le nonce → le nonce **est** authentifié **au niveau bloc** (mais pas pour les tx gossip libres, cf. `NONCE-DOS`).
- **Plafond d'émission appliqué sur le chemin happy + tests** (OPEN-1.8e) — **mais voir §5.2** : la prétention de 1.8e que le plafond garde aussi la branche `:1086` est **fausse** et constitue exactement `FORK-CAP`.

---

## 4. Décisions d'Alexandre (lister, ne pas résoudre — §4 constitution)

Items ouverts confirmés **sans régression silencieuse**, chacun `alexandre_decision=true` :

| id | état actuel (preuve) | ADR |
|----|----------------------|-----|
| `DEC-finality` / partition multi-blocs (OPEN-1.8a) | `ledger.rs:1058` n'entre en résolution que pour `index == tip.index` ; un fork multi-blocs sous le tip meurt « out of range » (`:1145`). Gap borné (conservation re-checkée), **délibérément non corrigé jusqu'au gadget de finalité**. | [[ADR-001]] |
| `DEC-fork-choice` (OPEN-1.8b) | tie-break single-block hash lexicographique (`ledger.rs:1061`), assumé (`sm/node.rs:1392`). = `FORK-HASH-CHOICE`. | [[ADR-001]] |
| `DEC-aggregation` (OPEN-1.8c) | **aucune** agrégation BLS/PQ ; commentaires roadmap `sim.rs:1244/2628` ; WHITEPAPER §finalité différée. | [[ADR-004]] |
| `DEC-slashing` (OPEN-1.8d) | **aucun** slashing/pénalité en prod (grep `slash/jail/penalize` = 0) ; `detect_equivocation` **sim-only**. | [[ADR-003]] |
| `DEC-emission-policy` (OPEN-1.8e) | loi d'émission implémentée, plafond 100 M appliqué au consensus (happy path + tests) — **non régressé**. Constantes = politique. | — |
| `DEC-tx-timestamp` (OPEN-1.8f) | `tx.timestamp`/`block.timestamp` non bornés à la validation. Median-time-past / skew = choix de consensus. | — |
| `DEC-acceptance-leader` | acceptation leaderless (`LEADERLESS-ACCEPT`) — lier l'élection à l'acceptation. | [[ADR-002]] |
| `DEC-rep-weight` | réputation locale comme poids de consensus (`REP-WEIGHT` / `REP-F64-LATENT`). | (consensus) |

---

## 5. Réfutées & écarts

### 5.1 Trouvailles réfutées par le vérificateur adverse (bien réfutées)
- **`I-1.4-test-only`** (zeroize de seeds sim/test) — `node.rs:711` / `username.rs:320` dans `#[cfg(test)] mod tests` ; `sm/rng.rs` `Blake3Rng` est un PRNG de **simulation** (auto-documenté, seedé d'un `u64`), consommé seulement par `sim.rs` (`#[cfg(test)]`). Seeds publics connus (`0x11`…), aucun secret de prod. Les zeroizer ne protégerait rien. **Réfutée comme finding de prod.**
- **`L-1.7-solo-full-le-not-eq`** (`assert!(qta <= cap)` vs `== cap`, `reputation.rs:316-328`) — **réfutée comme joignable en release** : `#[cfg(test)]` ; la logique solo de prod (`reputation.rs:152-154`) paie déjà la pleine émission → aucun bug live masqué. Conservée comme **note de durcissement de test** (frère de la classe « `≤` trop molle ») dans `TEST-TEETH`.

### 5.2 Écart inter-surfaces (renforce la confiance)
La surface **1.8e** a affirmé que `validate_block_emission` « garde `ledger.rs:1016` **et** `:1086** » — reprenant le **commentaire faux** du code (`ledger.rs:887-893`). Les surfaces **1.2** et **1.6**, en relisant la branche `:1086`, ont prouvé le contraire. **L'audit a donc détecté sa propre contradiction** : le commentaire trompeur est précisément ce qui masque `FORK-CAP`. Le correctif de `FORK-CAP` doit aussi **corriger ce commentaire**.

---

## 6. Méthode & provenance

- **Cadre** : [[QUANTA_HARDENING_AUDIT]] (HARDEN-AUDIT-1), lecture seule, §3 invariants / §4 stop.
- **Orchestration** : workflow fan-out — 8 auditeurs (un par surface 1.1–1.8) avec listes de
  fichiers ciblées et contexte « déjà-corrigé / déjà-ouvert » (pour distinguer **nouveau** vs
  **régression**), puis **un vérificateur adverse par trouvaille** relisant le `fichier:ligne`
  et tentant de réfuter, avec le filtre **joignable-en-release** (cfg(test)/`sim.rs`/code mort
  ⇒ non-finding). Réf. run `wip85bgdn` : **53 agents, ~2,6 M tokens, 660 appels d'outils**.
- **Calibrage de sévérité** : critique = casse le consensus inter-nœuds / DoS distant / perte de
  fonds ; haute = DoS distant sans divergence ; moyenne = ressource/dégradation bornée ou
  divergence latente ; basse = hygiène locale / test-only / lent rate-limited.
- **Corrections du vérificateur** intégrées (lignes exactes, branches mortes écartées :
  `SLICE-LEDGER` sites réels 923/943 ; ajustements de sévérité H-1.5→critique,
  M-1.4×3→basse, M-1.7×3→basse).
- **Limites** : audit statique par lecture ; les `confidence: low` (ex. `I128-CACHE-NEGATIVE`,
  joignabilité d'un négatif **persistant**) appellent un repro ciblé dans le spec dédié.
- **Suite** : chaque entrée du Top-N (§0.4) devient un spec chirurgical séparé, revu et gated
  individuellement. **Aucun correctif appliqué ici.** Le **baseline git** (manuel, Alexandre)
  reste la priorité n°1, indépendante de cet audit.
