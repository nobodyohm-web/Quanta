# SOVA — Le Protocole Mondial d'Énergie Computationnelle

## Un Réseau Décentralisé Où Chaque Ordinateur Transforme Son Énergie en Valeur — Pour Son Propriétaire et Pour la Science

**Version 2.0 — Avril 2026**

---

## Résumé

SOVA est un protocole décentralisé qui transforme l'énergie électrique consommée par les ordinateurs du quotidien en un actif numérique vérifiable et échangeable. Contrairement aux cryptomonnaies à Preuve de Travail qui gaspillent délibérément de l'énergie, SOVA mesure la consommation réelle des nœuds participants et les récompense proportionnellement depuis un pool d'émission fixe à l'échelle du réseau. Les ressources informatiques inutilisées sont automatiquement dirigées vers du calcul scientifique utile — repliement de protéines, modélisation climatique, entraînement d'IA — créant un supercalculateur distribué financé par le collectif. Le protocole utilise des Types de Données Répliquées sans Conflit (CRDT) pour un consensus sans verrouillage, des preuves à divulgation nulle (RISC Zero) pour une vérification trustless du travail, et la distribution par Valeur de Shapley pour une récompense mathématiquement équitable. Un mécanisme de Burn-and-Mint prévient l'inflation tout en maintenant une offre illimitée. Une DAO DeSci alloue 5% des émissions pour financer la recherche scientifique choisie par les participants.

**Le premier protocole où consommer de l'énergie = créer de la valeur = faire avancer la science.**

---

## Table des Matières

1. [Introduction](#1-introduction)
2. [Le Pool d'Énergie](#2-le-pool-dénergie)
3. [Trois Modes de Contribution](#3-trois-modes-de-contribution)
4. [Distribution par Valeur de Shapley](#4-distribution-par-valeur-de-shapley)
5. [Vérification : De la Confiance à la Preuve](#5-vérification--de-la-confiance-à-la-preuve)
6. [Économie du Jeton](#6-économie-du-jeton)
7. [Consensus : Merkle-CRDT](#7-consensus--merkle-crdt)
8. [Transport Réseau](#8-transport-réseau)
9. [Fondation Cryptographique](#9-fondation-cryptographique)
10. [Marketplace de Calcul](#10-marketplace-de-calcul)
11. [DAO DeSci](#11-dao-desci)
12. [Analyse de Sécurité](#12-analyse-de-sécurité)
13. [Feuille de Route](#13-feuille-de-route)
14. [Conclusion](#14-conclusion)

---

## 1. Introduction

### 1.1 Le Problème

Chaque jour, des milliards d'ordinateurs consomment de l'énergie en ne faisant rien. Un laptop tourne au repos à 15W. Un PC gaming à 80W. Un poste de travail à 200W. Cette énergie est payée, consommée et gaspillée — ne produisant aucune valeur au-delà du maintien de la machine en veille.

Pendant ce temps, Bitcoin consomme 150 TWh par an pour résoudre des puzzles sans autre but que la sécurité du réseau. La recherche scientifique est sous-financée : le CERN, l'Institut Pasteur et les labos climatiques se battent pour des subventions limitées tandis que des milliards de cycles CPU restent inutilisés dans le monde entier.

### 1.2 La Solution

SOVA connecte ces deux problèmes :

1. **Ton ordinateur consomme déjà de l'énergie.** SOVA la mesure et te récompense proportionnellement.
2. **Ton CPU/GPU idle peut faire du travail utile.** SOVA le dirige vers la science, l'entraînement IA et le rendu 3D.
3. **Tout le monde en profite.** Plus de participants = plus d'énergie totale = plus de valeur pour chacun.

### 1.3 Principes de Conception

1. **L'Énergie Est Valeur** — Chaque jeton est adossé à une consommation énergétique mesurée et vérifiable.
2. **Pas de Rareté Artificielle** — Pas de cap, pas de halving. Offre illimitée avec équilibre par burn.
3. **Travail Utile** — Les ressources idle contribuent à la science, l'IA et le calcul distribué.
4. **Justice Mathématique** — Distribution via la Valeur de Shapley (Prix Nobel 2012).
5. **Vérification Trustless** — Les preuves à divulgation nulle garantissent qu'aucun nœud ne peut falsifier sa contribution.
6. **Gouvernance Démocratique** — Une DAO DeSci permet aux participants de financer la science de leur choix.

---

## 2. Le Pool d'Énergie

### 2.1 Émission Fixe du Réseau

Le réseau émet **100 SOVA par heure**, en permanence, quel que soit le nombre de participants.

### 2.2 Distribution Proportionnelle

```
ma_part = (mes_watts / watts_total_réseau) × 100 SOVA/heure
```

Laptop 15W dans un réseau de 50 000W → 0,03 SOVA/h. Station de montage 300W → 0,6 SOVA/h.

### 2.3 Plus d'Utilisateurs = Plus de Valeur

```
valeur(1 SOVA) = énergie_totale_réseau_kWh / SOVA_en_circulation
```

| Participants | Puissance | SOVA/h | Valeur/SOVA | Gain/h (50W) |
|-------------|-----------|--------|-------------|--------------|
| 100 | 5 kW | 100 | 0,0075 € | 0,0075 € |
| 10 000 | 500 kW | 100 | 0,75 € | 0,75 € |
| 1 000 000 | 50 MW | 100 | 75 € | 75 € |

**Le gain en EUR par watt est IDENTIQUE pour tous**, quel que soit le moment où l'on rejoint.

### 2.4 Mesure Énergétique

- **Intel/AMD** : compteurs RAPL (silicium, depuis 2012)
- **Apple Silicon** : `powermetrics` (hardware)
- **Oracle** : 33 pays, prix Eurostat/EIA Q1 2026, détection par timezone

---

## 3. Trois Modes de Contribution

### 3.1 Mode Actif — Tu travailles, tu mines (×1.0)

Tu utilises ton ordinateur normalement. Les watts sont mesurés et convertis en SOVA. Aucune action requise.

### 3.2 Mode Recherche — Ton idle aide la science (×2.0)

Ordinateur idle → exécute automatiquement du calcul scientifique (BOINC), de l'entraînement IA (Federated Learning), ou du rendu 3D. Le travail est vérifié cryptographiquement.

### 3.3 Mode Validateur — Tu vérifies, tu gagnes (×0.1)

CPU < 10% → le nœud vérifie les blocs des autres, confirme les transactions, insère dans le DAG.

---

## 4. Distribution par Valeur de Shapley

La Valeur de Shapley (Lloyd Shapley, Nobel d'Économie 2012) est la seule méthode mathématiquement prouvée pour distribuer équitablement la valeur dans un système coopératif.

```
Shapley(nœud) = 0.30 × énergie + 0.35 × travail_utile + 0.20 × validation + 0.15 × uptime
ma_part = Shapley(moi) / Σ(Shapley(tous)) × ÉMISSION_RÉSEAU
```

Un nœud qui fait du calcul utile gagne PLUS qu'un nœud qui consomme juste du courant. La **valeur** de la contribution, pas juste son **coût**, détermine la récompense.

---

## 5. Vérification : De la Confiance à la Preuve

### Phase 1 — Trust-but-Verify
Le message Hello inclut le modèle CPU. Les validateurs comparent les watts déclarés au TDP connu du processeur.

### Phase 2 — Cross-Validation
Corrélation statistique entre watts déclarés et latence réseau observée. Écart > 2σ → pénalité.

### Phase 3 — ZK-Proof of Work (RISC Zero)

```
Tâche → Exécution dans zkVM RISC Zero → PROOF cryptographique
→ Validateurs vérifient (~1ms) → Énergie DÉDUITE du travail prouvé
flops_prouvés × joules/flop[CPU] = énergie certifiée
```

**RISC Zero** : zkVM open source, Rust natif, $40M de financement, production. L'énergie n'est plus auto-déclarée — elle est mathématiquement dérivée du travail prouvé.

---

## 6. Économie du Jeton

### 6.1 Émission
- 100 SOVA/h, constant, pour toujours
- 5% → Trésorerie DeSci DAO

### 6.2 Burn-and-Mint Equilibrium

| Action | Taux de burn |
|--------|-------------|
| Transfert | 1% |
| Soumission de tâche | 2% |
| Bridge ERC-20 | 0,5% |
| Récompense validation | 0,1% |

Réseau peu actif → supply croît. Réseau très actif → burns > émission → **supply déflationnaire**.

### 6.3 Sources de Demande
1. **Laboratoires** paient en SOVA pour soumettre des tâches
2. **Startups IA** paient pour l'entraînement distribué
3. **Studios** paient pour le rendu 3D
4. **Traders** arbitrent les différences de prix d'énergie entre pays

---

## 7. Consensus : Merkle-CRDT

Double registre : Journal linéaire (auditabilité) + État CRDT (consensus sans verrouillage).

- **PN-Counters** : soldes (incrémentation/décrémentation)
- **G-Counters** : métriques réseau (total_watts, total_sova, total_kwh)
- **DAG de Merkle** : nœuds adressés par BLAKE3, append-only, multi-têtes

Propriétés CRDT : commutativité, associativité, idempotence → **cohérence à terme garantie** sans leader, sans vote, sans coordination. Pas d'attaque à 51% possible.

---

## 8. Transport Réseau

**Iroh QUIC** : UDP chiffré, traversée NAT, gossip natif.

Messages : Hello (watts+CPU+pays), WantNodes, HaveNodes, BroadcastTx, TaskAssign, TaskResult, Ping/Pong, ReportPeer.

**Slashing** : avertissement (Shapley -50% / 24h) → suspension (7j) → expulsion (consensus des pairs).

---

## 9. Fondation Cryptographique

| Primitive | Algorithme |
|-----------|-----------|
| Signatures | Ed25519 |
| Chiffrement | AES-256-GCM |
| Dérivation clé | Argon2id |
| Hachage | BLAKE3 |
| Effacement mémoire | zeroize |
| Preuves ZK | RISC Zero |
| Post-Quantique | ML-DSA-65 (préparé) |

---

## 10. Marketplace de Calcul

**Niveau 1 — GRATUIT** : Calcul scientifique BOINC bénévole, financé par l'émission réseau.

**Niveau 2 — PAYÉ** : Labos/studios soumettent des tâches, paient en SOVA (2% brûlé).

**Niveau 3 — PREMIUM** : Location GPU continue, enchères inversées (modèle Akash Network).

**Federated Learning** : Entraînement IA distribué, données ne quittent jamais la machine → conformité RGPD native. Multiplicateur ×3.

**Proof of Storage** (optionnel) : 10-100 GB de disque alloué, vérifié par challenges périodiques. Bonus ×0.5.

---

## 11. DAO DeSci

- **Trésorerie** : 5% des émissions (5 SOVA/h)
- **Vote** : 1 SOVA staké = 1 vote
- **Quorum** : 10% | **Majorité** : 66%
- **Impact** : Les participants décident quelle science est financée. Résultats publiés en accès libre.

---

## 12. Analyse de Sécurité

- **Sybil** : Shapley null-player + multiplicateur ×0.1 → économiquement irrationnel
- **Fraude énergie** : TDP check → cross-validation → ZK-proof (infalsifiable)
- **Double dépense** : PN-Counter monotone + burn double les pertes
- **Capture consensus** : Impossible — CRDTs convergent algébriquement, pas par vote
- **Tâche malveillante** : Sandboxing WASM/zkVM, aucun accès filesystem/réseau

---

## 13. Feuille de Route

| Phase | Délai | Jalon | Statut |
|-------|-------|-------|--------|
| Protocole Central | Terminé | Ed25519, AES-256, BLAKE3, Argon2id, zeroize | ✅ |
| Oracle Énergétique | Terminé | 33 pays, watts CPU réels | ✅ |
| Transport P2P | Terminé | Iroh QUIC, gossip vérifié 2 nœuds | ✅ |
| Consensus CRDT | Terminé | PN/G-Counter, snapshot/restore | ✅ |
| Phase 1 : Pivot | 2 semaines | Émission fixe, Shapley, BME | 🔧 |
| Phase 2 : Solidité | 1 mois | Cross-validation, validateur passif, testnet | 📋 |
| Phase 3 : Travail Utile | 2-4 mois | BOINC, marketplace, DeSci DAO | 📋 |
| Phase 4 : ZK-Proof | 6+ mois | RISC Zero, vérification trustless | 📋 |
| Phase 5 : Bridge | 3-6 mois | wSOVA ERC-20, Uniswap | 📋 |
| Phase 6 : Échelle | 12+ mois | Federated Learning, Storage, GPU | 📋 |

---

## 14. Conclusion

SOVA repense fondamentalement ce qu'une cryptomonnaie peut être. En combinant la mesure d'énergie réelle (pas gaspillée), le calcul scientifique utile (pas du hachage vide), la distribution équitable par Shapley (pas premier arrivé, premier servi), la vérification par preuves à divulgation nulle (pas la confiance), et le financement démocratique de la science (pas des subventions centralisées), le protocole crée un système où chaque participant bénéficie de la présence de chaque autre.

**Installe SOVA. Ton ordinateur aide à guérir le cancer pendant que tu dors. Tu es payé pour ça.**

---

## Références

1. Nakamoto, S. (2008). *Bitcoin : Un Système de Monnaie Électronique Pair-à-Pair.*
2. Shapiro et al. (2011). *Types de Données Répliquées sans Conflit.* INRIA.
3. Shapley, L. S. (1953). *Une Valeur pour les Jeux à N Personnes.* Prix Nobel 2012.
4. Eurostat (2026). *Prix de l'électricité pour les consommateurs domestiques.*
5. RISC Zero (2025). *RISC Zero zkVM : Preuves à Divulgation Nulle Généralistes.*
6. Anderson, D. (2004). *BOINC : Un Système de Calcul sur Ressources Publiques.* UC Berkeley.
7. McMahan et al. (2017). *Apprentissage Efficace en Communication sur Données Décentralisées.* Google.
8. Render Network (2024). *Burn-and-Mint Equilibrium.*
9. Protocol Labs (2017). *Filecoin : Un Réseau de Stockage Décentralisé.*

---

**Licence** : CC BY-SA 4.0 | **Code Source** : Open source — Rust/Tauri/Svelte
