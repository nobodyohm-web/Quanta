# SOVA — Références Techniques

> Guide de référence pour les technologies clés du protocole SOVA V2.
> Claude Code doit consulter ce fichier avant d'implémenter les phases avancées.

---

## 1. RISC Zero zkVM — Phase 4 (ZK-Proofs)

### Installation
```bash
curl -L https://risczero.com/install | bash
rzup install
cargo risczero --version
```

### Architecture
- **Guest** : Programme Rust compilé en RISC-V qui s'exécute dans le zkVM
- **Host** : Programme Rust qui lance le guest, fournit les inputs, génère la preuve
- **Receipt** : Sortie cryptographique (journal + seal) = preuve vérifiable

### Usage pour SOVA
```rust
// Guest (s'exécute dans le zkVM)
// Prouve : "J'ai exécuté N instructions RISC-V en T secondes"
// → Énergie déduite : E = TDP_processeur × T × utilisation_CPU

// Host (lance la preuve)
let receipt = prover.prove(elf, &input)?;
receipt.verify(IMAGE_ID)?;  // Vérification O(1)
```

### Repos GitHub
- `risc0/risc0` : zkVM core + exemples
- `risc0/bonsai-foundry-template` : Intégration blockchain
- Doc : https://dev.risczero.com/

### Intégration SOVA (Phase 4)
1. Guest = programme de benchmark CPU (calcul matrice, FFT, etc.)
2. Le Receipt prouve le nombre d'instructions exécutées
3. L'énergie est déduite du nombre d'instructions × TDP du CPU
4. Le Receipt est gossipé aux pairs pour vérification trustless

---

## 2. Shapley Value — Distribution Équitable

### Formule simplifiée pour N nœuds
```
φ_i = Σ_{S⊆N\{i}} |S|!(|N|-|S|-1)! / |N|! × [v(S∪{i}) - v(S)]
```

### Approximation SOVA (linéaire, O(n))
```rust
fn shapley_score(node: &NodeStats, network: &NetworkStats) -> f64 {
    let energy = node.watts_measured / network.total_watts;       // 30%
    let work = node.tasks_completed / network.total_tasks;         // 35%
    let validation = node.blocks_verified / network.total_blocks;  // 20%
    let uptime = node.uptime_hours / network.max_uptime;           // 15%
    
    0.30 * energy + 0.35 * work + 0.20 * validation + 0.15 * uptime
}
```

### Source académique
- Lloyd S. Shapley, "A Value for n-Person Games", 1953
- Nobel Prize Economics 2012 (Shapley & Roth)
- Implémentation inspirée de `shap` (Python) pour l'interprétabilité ML

---

## 3. CRDT — Consensus Sans Verrouillage

### Types utilisés dans SOVA
| Type | Usage | Crate |
|------|-------|-------|
| `GCounter` | Métriques monotones (vues, ticks) | `crdts` |
| `PNCounter` | Balances (dépôt + retrait) | `crdts` |
| `LWWRegister` | Dernière valeur (prix énergie) | custom |

### Propriétés
- **Commutatif** : l'ordre d'application des ops n'importe pas
- **Idempotent** : appliquer la même op 2× = 1×
- **Associatif** : regrouper les ops librement
- **Convergent** : tous les nœuds convergent vers le même état

### Gossip CRDT dans SOVA
```
Node A mine → incrémente GCounter(A) → gossip aux pairs
Node B reçoit → merge GCounter(A) dans sa copie locale
→ Tous convergent sans coordination
```

---

## 4. Iroh — Transport P2P QUIC

### Version utilisée : 0.98
```rust
// Créer un endpoint
let endpoint = Endpoint::builder().discovery_n0().bind().await?;

// Gossip
let gossip = Gossip::builder().spawn(endpoint.clone()).await?;
let topic_id = blake3::hash(b"sova-global-v1");
let topic = gossip.subscribe(topic_id, vec![])?;

// Envoi
topic.broadcast(bytes.into()).await?;

// Réception
while let Some(event) = topic.next().await {
    match event? {
        Event::Received(msg) => { /* dispatch */ }
        Event::NeighborUp(id) => { /* nouveau pair */ }
        _ => {}
    }
}
```

### NAT Traversal
- Iroh utilise des relais intégrés pour le holepunching
- QUIC = UDP → passe la plupart des firewalls
- Pas besoin de configuration manuelle

---

## 5. Burn-and-Mint Equilibrium (BME)

### Modèle
```
Supply effective = Émissions cumulées - Tokens brûlés

Quand demande ↑ → plus de tâches soumises → plus de burn → supply ↓ → prix ↑
Quand demande ↓ → moins de burn → supply ↑ → prix ↓

Équilibre naturel sans intervention
```

### Paramètres SOVA
```rust
const BURN_RATE_TRANSFER: f64 = 0.01;  // 1% par transfert
const BURN_RATE_TASK: f64 = 0.02;       // 2% par soumission de tâche
const DESCI_ALLOCATION: f64 = 0.05;     // 5% de l'émission → DAO science
```

### Référence
- Factom (2015) : Premier protocole BME
- Helium (HNT) : BME pour IoT
- The Graph (GRT) : BME pour indexation

---

## 6. Energy Oracle — Mesure Réelle

### Méthodes de mesure (existant dans energy.rs)
| Plateforme | Méthode | Précision |
|-----------|---------|-----------|
| Linux | Intel RAPL (`/sys/class/powercap/`) | ±2% |
| macOS | `powermetrics` + SMC | ±5% |
| Windows | WMI + MSR (futur) | ±10% |
| Fallback | Estimation TDP × utilisation CPU | ±20% |

### 33 pays supportés
Prix kWh : de 0.02$/kWh (Venezuela) à 0.44$/kWh (Danemark)
Détection pays : `reqwest` → API géolocalisation → ISO 3166-1
