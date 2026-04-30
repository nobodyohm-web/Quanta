# SOVA V2 — Pool Mondial d'Énergie

> Ce fichier est la spec de référence pour Claude Code.
> Il remplace le modèle halving/cap de la V1.

---

## Concept fondamental

**SOVA est un certificat d'énergie universel.** Chaque token représente une fraction vérifiable de l'énergie totale consommée par le réseau mondial. Pas de rareté artificielle, pas de cap, pas de halving. L'énergie EST la valeur.

---

## Les 3 règles

### Règle 1 — Émission fixe par heure

Le réseau émet un nombre CONSTANT de SOVA par heure, quel que soit le nombre de nœuds.

```rust
const NETWORK_EMISSION_PER_HOUR: f64 = 100.0; // SOVA/heure, toujours
```

Ce chiffre ne change jamais. Pas de halving. Pas d'époque.

### Règle 2 — Répartition proportionnelle aux watts

Les SOVA émises chaque heure sont réparties entre tous les nœuds actifs, proportionnellement à leur consommation électrique mesurée.

```
ma_part = (mes_watts / watts_total_réseau) × NETWORK_EMISSION_PER_HOUR
```

Un laptop à 15W dans un réseau de 50 000W total :
→ 15/50000 × 100 = 0,03 SOVA/heure

Un poste de montage vidéo à 300W :
→ 300/50000 × 100 = 0,6 SOVA/heure

### Règle 3 — Valeur = énergie totale du réseau

```
valeur(1 SOVA) = énergie_totale_réseau_kWh / SOVA_en_circulation
```

Plus il y a de nœuds → plus l'énergie totale est grande → plus chaque SOVA vaut cher.
La supply est illimitée (100/h pour toujours), mais la VALEUR par token croît avec le réseau.

---

## Deux rôles automatiques

### Mode ACTIF — Tu travailles, tu mines

Quand l'utilisateur utilise son ordinateur (naviguer, coder, monter une vidéo, jouer), le CPU/GPU consomme des watts. Ces watts sont mesurés en temps réel et convertis en SOVA selon la Règle 2.

- Plus l'ordi travaille dur → plus de watts → plus de SOVA
- L'app SOVA tourne en background, consomme <1% CPU elle-même
- Aucune action requise de l'utilisateur

### Mode PASSIF — Tu valides, tu gagnes

Quand l'ordinateur est idle (nuit, pause, écran de veille), il passe automatiquement en mode validateur :

- Reçoit les blocs des autres nœuds via gossip
- Vérifie les signatures et insère dans le DAG local
- Confirme les transactions du réseau
- Récompense : **10% du taux de minage normal** (contribution à la sécurité du réseau)

```
récompense_validation = 0.10 × (mes_watts_idle / watts_total_réseau) × NETWORK_EMISSION_PER_HOUR
```

---

## Pourquoi le prix monte avec le réseau

### Scénario chiffré

| Utilisateurs | Watts total | SOVA/heure | Valeur 1 SOVA | Gain/heure (50W) |
|-------------|-------------|------------|---------------|------------------|
| 100 | 5 kW | 100 | 0,0075 € | 0,0075 € |
| 10 000 | 500 kW | 100 | 0,75 € | 0,75 € |
| 1 000 000 | 50 MW | 100 | 75 € | 75 € |
| 100 000 000 | 5 GW | 100 | 7 500 € | 7 500 € |

**Observation clé** : le gain en EUROS par watt est IDENTIQUE pour tous, quel que soit le moment où on rejoint. Pas d'avantage early adopter. C'est ça l'égalité.

### Pourquoi ça marche

1. L'émission est fixe → pas d'inflation proportionnelle aux utilisateurs
2. L'énergie totale croît avec chaque nouvel utilisateur → le backing augmente
3. Le prix plancher = coût énergétique total / SOVA en circulation → monte mécaniquement
4. Sur le marché (Binance etc.), le prix peut aller AU-DESSUS du plancher (offre/demande)

---

## Le Pool Mondial

Chaque nœud qui se connecte rejoint automatiquement le pool via iroh-gossip :

1. **Annonce Hello** → déclare ses watts mesurés + son code pays
2. **Le réseau agrège** → watts_total_réseau est la somme de tous les nœuds actifs
3. **Chaque minute** → le nœud calcule sa part proportionnelle et l'inscrit dans le DAG
4. **Les validateurs confirment** → les nœuds idle vérifient que les watts déclarés sont cohérents

### Anti-triche

- Les watts sont mesurés par le hardware (compteurs CPU/GPU du système)
- Un nœud qui déclare 1000W mais dont les compteurs montrent 15W → flaggé par les validateurs
- La validation croisée entre pairs détecte les incohérences
- Ratio watts déclarés / watts validés trop élevé → score PoC réduit → gains réduits

---

## Arbitrage énergétique mondial

Le marché SOVA crée naturellement un arbitrage :

- Un mineur en Inde (0,072 €/kWh) produit des SOVA à bas coût
- Un acheteur au Danemark (0,368 €/kWh) préfère acheter plutôt que miner
- Prix de marché = quelque part entre les deux → les deux y gagnent

C'est un vrai commerce basé sur une vraie différence de coût de production. Pas de la spéculation.

---

## Ce qui change par rapport à la V1

| Aspect | V1 (actuel) | V2 (nouveau) |
|--------|-------------|--------------|
| Supply | Cap ~200k ATN | **Illimitée** |
| Halving | Oui (tous les 100k) | **Non** |
| Émission | Par nœud (1 ATN/h chacun) | **Fixe réseau (100 SOVA/h total)** |
| Répartition | Égale par nœud | **Proportionnelle aux watts** |
| Early adopter | Avantagé (plus de tokens) | **Même gain €/watt que tout le monde** |
| Valeur vient de | Rareté + halving | **Énergie totale du réseau** |
| Réseau social | Oui (Feed, Editor, etc.) | **Non — pure crypto en background** |
| App visible | Interface complète | **Tray icon + dashboard minimal** |

---

## Modifications code requises

### Backend Rust — `src-tauri/src/p2p/`

1. **reputation.rs** — Supprimer `HALVING_THRESHOLD`, `MAX_HALVING_EPOCH`, `ATN_PER_HOUR_UPTIME`. Remplacer par `NETWORK_EMISSION_PER_HOUR = 100.0`. Le `uptime_tick` calcule la part proportionnelle aux watts au lieu d'un taux fixe par nœud.

2. **energy.rs** — Ajouter `total_network_watts()` qui agrège les watts de tous les nœuds connus via les reports gossip. Ajouter `sova_value()` = énergie totale kWh / SOVA en circulation.

3. **gossip.rs** — Le message `Hello` doit inclure les watts mesurés du nœud en plus du code pays. Ajouter un message `NetworkStats` périodique avec le total watts/nœuds.

4. **consensus.rs** — Le CRDT doit tracker `total_sova_minted` (G-Counter global) et `total_kwh_consumed` (G-Counter global) pour calculer la valeur.

5. **lib.rs** — Le mining loop utilise la nouvelle formule : `ma_part = (mes_watts / watts_réseau) × 100/60` par minute.

### Frontend Svelte — Simplifier

1. Supprimer Feed, Editor, TemplatePicker, Browser (réseau social)
2. Garder : Wallet (solde, valeur, énergie), Dashboard (stats réseau), Settings
3. App en mode tray icon (background), dashboard accessible en clic

### Nouvelles commandes Tauri

```rust
get_network_pool()     → { total_watts, total_nodes, total_sova, sova_value_eur }
get_my_contribution()  → { my_watts, my_share_pct, sova_earned_today, eur_value }
get_pool_history()     → Vec<{ timestamp, total_watts, total_nodes, sova_value }>
```

---

## Mode RECHERCHE — Calcul utile (le game changer)

Le Mode Recherche est le CŒUR de SOVA, pas une option. C'est ce qui crée la demande,
la valeur réelle, et le narratif.

### Concept

Quand l'ordinateur est idle, au lieu de simplement valider, il exécute des tâches
de calcul utile distribuées : recherche scientifique, rendu 3D, entraînement IA.

```
Mode ACTIF     → tu travailles, tes watts minent des SOVA
Mode RECHERCHE → ton ordi idle fait de la science, tu mines des SOVA (bonus ×2)
Mode VALIDATEUR → ton ordi vérifie les preuves des autres (bonus ×0.1)
```

### Sources de tâches

- **BOINC** (Berkeley, 22 ans d'existence, 800 000 users)
  - Climatologie, protéines, astronomie, physique
  - API documentée, intégration prouvée par Gridcoin
- **Tâches soumises par des labos/entreprises** (payées en SOVA)
  - L'Institut Pasteur soumet des simulations moléculaires
  - Un studio de cinéma soumet du rendu 3D
  - Une startup IA soumet de l'entraînement de modèle
  - **Ce sont les ACHETEURS** → c'est ce qui crée la demande de SOVA

### Multiplicateurs

```rust
const MODE_ACTIVE_MULTIPLIER: f64 = 1.0;    // travail normal
const MODE_RESEARCH_MULTIPLIER: f64 = 2.0;  // calcul scientifique vérifié
const MODE_VALIDATOR_MULTIPLIER: f64 = 0.1;  // validation passive
```

### Pourquoi c'est faisable

- BOINC : open source, API C/C++, wrappable en Rust via FFI — existant
- Render Network : 4 milliards $ de market cap — prouve la demande pour le GPU distribué
- Golem Network : marketplace de calcul distribué — prouve le modèle économique

---

## ZK-Proof of Work — Énergie vérifiable via RISC Zero

### Le problème fondamental

On ne peut PAS prouver cryptographiquement les lectures RAPL (Intel) ou powermetrics (Apple).
Un OS compromis peut mentir sur les watts. Aucune bibliothèque ZK ne résout ça directement.

### La solution : prouver le TRAVAIL, déduire l'ÉNERGIE

```
Étape 1 : Le réseau distribue une tâche de calcul au nœud
Étape 2 : Le nœud exécute la tâche dans le zkVM RISC Zero
Étape 3 : Le zkVM produit un PROOF cryptographique du résultat
Étape 4 : Le nœud publie : (résultat, proof, modèle_CPU)
Étape 5 : Les validateurs vérifient le proof (~1ms, pas de recalcul)
Étape 6 : L'énergie est DÉDUITE :
          flops_prouvés × joules_par_flop[modèle_CPU] = énergie certifiée
Étape 7 : Le nœud reçoit sa part de SOVA proportionnelle à l'énergie certifiée
```

### Outils production-ready (Rust natif)

```toml
# Cargo.toml — dépendances Phase 4
risc0-zkvm = "1.x"      # zkVM — écris du Rust, obtiens un ZK-proof
risc0-zkp  = "1.x"      # Vérification côté validateur
```

**RISC Zero** : zkVM open source, Rust natif, $40M de financement, utilisé en production.
Tu écris du Rust normal dans un "guest program", RISC Zero génère automatiquement
le proof que le code a été exécuté correctement avec les bonnes entrées/sorties.

### Exemple de guest program SOVA

```rust
// guest/src/main.rs — exécuté dans le zkVM
#![no_main]
risc0_zkvm::guest::entry!(main);

fn main() {
    // Lire la tâche de calcul
    let task: ComputeTask = risc0_zkvm::guest::env::read();

    // Exécuter le calcul (BOINC work unit, rendu, etc.)
    let result = execute_task(&task);
    let result_hash = blake3::hash(&result);

    // Publier le résultat — RISC Zero génère le proof automatiquement
    risc0_zkvm::guest::env::commit(&WorkProof {
        task_id: task.id,
        result_hash,
        flops_executed: task.estimated_flops,
        cpu_model: task.cpu_model,
    });
}
```

### Table énergie par FLOP (données publiques)

```rust
// Joules par GFLOP — sources : Intel ARK, AMD Product Specs, Apple Silicon specs
fn joules_per_gflop(cpu_model: &str) -> f64 {
    match cpu_model {
        "apple_m1"     => 0.15,  // ~15W pour ~100 GFLOPS
        "apple_m1_max" => 0.12,  // ~30W pour ~250 GFLOPS
        "intel_i5_13"  => 0.45,  // ~65W pour ~145 GFLOPS
        "intel_i7_14"  => 0.38,  // ~125W pour ~330 GFLOPS
        "amd_r7_7800"  => 0.30,  // ~105W pour ~350 GFLOPS
        _              => 0.40,  // fallback conservateur
    }
}
```

### Impact

- **L'énergie n'est plus auto-déclarée** → elle est mathématiquement dérivée du travail prouvé
- **Impossible à falsifier** → le ZK-proof est cryptographiquement sûr
- **Pas besoin de faire confiance au nœud** → la vérification est trustless
- **Instantané à vérifier** → un proof se vérifie en ~1ms, pas besoin de refaire le calcul

### Planning

- Phase 1-2 (MVP) : trust-but-verify (TDP check + cross-validation)
- Phase 3 : intégration BOINC
- **Phase 4 : RISC Zero zkVM** — le proof cryptographique remplace les heuristiques
- Phase 5 : table joules/FLOP maintenue par DAO governance

---

## Burn-and-Mint Equilibrium (BME)

### Mécanisme

L'émission est illimitée (100 SOVA/h). Pour éviter l'hyperinflation, chaque utilisation
du réseau BRÛLE des tokens :

```rust
const BURN_RATE_TRANSFER: f64 = 0.01;  // 1% brûlé par transfert
const BURN_RATE_VALIDATE: f64 = 0.001; // 0.1% brûlé par validation
const BURN_RATE_BRIDGE: f64 = 0.005;   // 0.5% brûlé par bridge ERC-20
const BURN_RATE_TASK_SUBMIT: f64 = 0.02; // 2% brûlé par soumission de tâche
```

### Résultat

- Réseau peu utilisé → supply augmente doucement (100/h)
- Réseau très utilisé → les burns dépassent l'émission → **supply déflationnaire**
- L'équilibre se trouve naturellement via le marché

### Inspiration

- Render Network utilise exactement ce modèle → 4 milliards $ de market cap
- Ethereum EIP-1559 brûle une partie des frais de gas → même philosophie

---

## Critères de validation

### Phase 1 (MVP)
- [ ] `cargo test` vert
- [ ] `npm run ai:check` → 0/0
- [ ] Simulation 2 nœuds : watts différents → parts proportionnelles correctes
- [ ] Valeur SOVA monte quand un 3ème nœud rejoint
- [ ] Pas de halving, pas de cap dans le code
- [ ] Mode passif (validateur) fonctionne quand CPU idle
- [ ] Le wallet affiche la valeur en EUR en temps réel
- [ ] Burn 1% sur chaque transfert

### Phase 3 (BOINC)
- [ ] Un nœud idle exécute une tâche BOINC
- [ ] Le résultat est publié dans le DAG
- [ ] Le multiplicateur ×2 est appliqué
- [ ] Les labos peuvent soumettre des tâches payées en SOVA

### Phase 4 (ZK-Proof)
- [ ] Guest program RISC Zero compile et s'exécute
- [ ] Le proof est généré et vérifié par un validateur
- [ ] L'énergie est déduite du travail prouvé via la table FLOP/joule
- [ ] Un nœud menteur (faux watts) est détecté et rejeté

---

## Philosophie

> SOVA n'est pas une crypto de plus.
>
> C'est un réseau mondial où chaque ordinateur transforme son énergie
> en valeur — pour son propriétaire et pour la science.
>
> Ton ordi aide à guérir le cancer pendant que tu dors.
> Tu es payé pour ça.
>
> Plus le réseau grandit, plus chaque contribution vaut.
> Les laboratoires paient pour accéder à un supercalculateur mondial
> construit par des gens ordinaires.
>
> Pas de gagnants précoces. Pas de perdants tardifs.
> Pas de gaspillage : chaque watt sert la recherche.
>
> Le premier protocole où consommer de l'énergie = créer de la valeur = aider l'humanité.
