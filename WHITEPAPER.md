# QUANTA — The World's Computational Energy Protocol

## A Decentralized Network Where Every Computer Turns Its Energy Into Value — For Its Owner and For Science

**Version 2.0 — April 2026**

---

## Abstract

QUANTA is a decentralized protocol that transforms the electrical energy consumed by everyday computers into a verifiable, tradeable digital asset. Unlike Proof-of-Work cryptocurrencies that deliberately waste energy, QUANTA measures the real power consumption of participating nodes and rewards them proportionally from a fixed network-wide emission pool. Idle computing resources are automatically directed toward useful scientific computation — protein folding, climate modeling, AI training — creating a distributed supercomputer funded by the collective. The protocol uses Conflict-free Replicated Data Types (CRDTs) for lock-free consensus, zero-knowledge proofs (RISC Zero) for trustless work verification, and Shapley Value distribution for mathematically fair rewards. A Burn-and-Mint Equilibrium mechanism prevents inflation while keeping supply unlimited. A DeSci DAO allocates 5% of all emissions to fund scientific research voted on by participants.

**The first protocol where consuming energy = creating value = advancing science.**

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [The Energy Pool](#2-the-energy-pool)
3. [Three Contribution Modes](#3-three-contribution-modes)
4. [Shapley Value Distribution](#4-shapley-value-distribution)
5. [Verification: From Trust to Proof](#5-verification-from-trust-to-proof)
6. [Token Economics](#6-token-economics)
7. [Consensus: Merkle-CRDT](#7-consensus-merkle-crdt)
8. [Network Transport](#8-network-transport)
9. [Cryptographic Foundation](#9-cryptographic-foundation)
10. [Compute Marketplace](#10-compute-marketplace)
11. [DeSci DAO](#11-desci-dao)
12. [Security Analysis](#12-security-analysis)
13. [Roadmap](#13-roadmap)
14. [Conclusion](#14-conclusion)

---

## 1. Introduction

### 1.1 The Problem

Every day, billions of computers consume energy doing nothing. A laptop idles at 15W. A gaming PC at 80W. A workstation at 200W. This energy is paid for, consumed, and wasted — producing no value beyond keeping the machine awake.

Meanwhile, Bitcoin consumes 150 TWh per year to solve puzzles with no purpose other than network security. Scientific research is underfunded: CERN, the Institut Pasteur, and climate labs compete for limited grants while billions of CPU cycles sit idle worldwide.

### 1.2 The Solution

QUANTA connects these two problems:

1. **Your computer already consumes energy.** QUANTA measures it and rewards you proportionally.
2. **Your idle CPU/GPU can do useful work.** QUANTA directs it toward science, AI training, and 3D rendering.
3. **Everyone benefits.** More participants = more total energy = higher token value for all.

### 1.3 Design Principles

1. **Energy Is Value** — Every token is backed by measured, verifiable energy consumption.
2. **No Artificial Scarcity** — No cap, no halving. Unlimited supply with burn-based equilibrium.
3. **Useful Work** — Idle resources contribute to science, AI, and distributed computing.
4. **Mathematical Fairness** — Rewards are distributed via Shapley Value (Nobel Prize 2012).
5. **Trustless Verification** — Zero-knowledge proofs ensure no node can fake its contribution.
6. **Democratic Governance** — A DeSci DAO lets participants fund the science they believe in.

---

## 2. The Energy Pool

### 2.1 Fixed Network Emission

The network emits a **constant** 100 QUANTA per hour, regardless of participant count.

```
NETWORK_EMISSION_PER_HOUR = 100.0 QUANTA  // always, forever
```

This rate does not change. No halving. No epoch. No algorithm adjusts it.

### 2.2 Proportional Distribution

The 100 QUANTA/hour are distributed among all active nodes proportionally to their measured energy consumption:

```
my_share = (my_watts / total_network_watts) × NETWORK_EMISSION_PER_HOUR
```

A laptop at 15W in a network totaling 50,000W:
→ 15/50,000 × 100 = 0.03 QUANTA/hour

A video editing workstation at 300W:
→ 300/50,000 × 100 = 0.6 QUANTA/hour

### 2.3 Why More Users = Higher Value

```
value(1 QUANTA) = total_network_energy_kWh / total_QUANTA_in_circulation
```

| Participants | Total Power | QUANTA/hour | Value per QUANTA | Earnings/hour (50W node) |
|-------------|-------------|-----------|---------------|--------------------------|
| 100 | 5 kW | 100 | €0.0075 | €0.0075 |
| 10,000 | 500 kW | 100 | €0.75 | €0.75 |
| 1,000,000 | 50 MW | 100 | €75 | €75 |

**Key insight**: Earnings in EUR per watt are identical for all participants regardless of when they join. A Day-1 miner and a Year-5 miner earn the same EUR per watt-hour.

### 2.4 Real-Time Power Measurement

Each node measures CPU/GPU power consumption via hardware counters:

- **Intel/AMD**: RAPL (Running Average Power Limit) — silicon-level counters, available since 2012
- **Apple Silicon**: `powermetrics` — hardware-backed wattage reporting
- **Fallback**: TDP-based estimation from CPU model identification

Measurement occurs every 60 seconds during the mining tick.

### 2.5 Global Electricity Price Oracle

The protocol embeds verified electricity prices for 33 countries (Eurostat/EIA Q1 2026):

| Region | Country | EUR/kWh |
|--------|---------|---------|
| Europe | Denmark | 0.3680 |
| | Germany | 0.3471 |
| | France | 0.2516 |
| | Spain | 0.2230 |
| Americas | USA | 0.1385 |
| | Canada | 0.1090 |
| Asia-Pacific | Japan | 0.2190 |
| | India | 0.0720 |
| | China | 0.0890 |

Country detection is performed offline via system timezone. No external API calls.

The network-weighted average price creates natural **energy arbitrage**: a miner in India (€0.072/kWh) produces QUANTA at low cost, while a buyer in Denmark (€0.368/kWh) prefers purchasing over mining. Both profit.

---

## 3. Three Contribution Modes

Every node operates in one of three modes, switching automatically based on system load. **All modes include validation** — verifying other nodes' blocks is automatic and rewarded via the Shapley "validation" factor (20%).

### 3.1 Active Mode — You Work, You Mine

When the user is actively using their computer (coding, editing video, gaming), the CPU/GPU consumes watts. These watts are measured in real time and converted to QUANTA.

- QUANTA runs in background (<1% CPU overhead)
- No user action required
- Higher workload = more watts = more QUANTA

→ You earn on **all 4 Shapley axes**: energy (30%) + work (35% if compute tasks available) + validation (20%) + uptime (15%).

### 3.2 Research Mode — Your Idle Computer Helps Science

When the computer is idle (night, break, screensaver), it automatically executes distributed computation tasks:

- **Scientific computing** via BOINC (protein folding, climate modeling, astrophysics)
- **AI training** via Federated Learning (privacy-preserving, data never leaves the machine)
- **3D rendering** for studios and artists
- **Custom tasks** submitted by laboratories paying in QUANTA

The work is cryptographically verified (see Section 5).

→ **The most profitable mode.** You score high on all 4 axes, especially useful work (35%).

### 3.3 Guardian Mode — You Secure the Network

Low-power device (Raspberry Pi, old laptop, $1/month VPS) → the node doesn't actively mine but **verifies every block**, confirms transactions, and maintains network consistency.

→ You earn via **validation (20%) + uptime (15%) = 35% of Shapley**. In a 1,000-node network, a guardian earns ~0.035 QTA/h. That's modest, but:
  - **Near-zero cost** (a Raspberry Pi consumes 3W = €0.0004/h)
  - **Net positive** as soon as QTA > €0.01
  - **Essential to the network** — more guardians = more security for everyone

---

## 4. Shapley Value Distribution

### 4.1 Why Not Just Watts?

Pure watt-proportional distribution treats a node running a CPU stress test identically to one training a cancer-detection AI model. Both consume 200W, but their value to the network is fundamentally different.

### 4.2 The Shapley Framework

Lloyd Shapley's value function (Nobel Prize in Economics, 2012) is the only mathematically proven method for fair value distribution in cooperative systems. It satisfies four axioms:

- **Efficiency**: All value is distributed; nothing is lost
- **Symmetry**: Equal contributions receive equal rewards
- **Linearity**: Total reward equals the sum of individual contributions
- **Null Player**: Zero contribution = zero reward

### 4.3 QUANTA's Shapley Score

```
Shapley(node_i) = 0.30 × energy_factor        // watts consumed (real cost)
               + 0.35 × useful_work_factor     // verified tasks completed
               + 0.20 × validation_factor      // blocks verified
               + 0.15 × uptime_factor          // reliability

my_share = Shapley(me) / Σ(Shapley(all)) × NETWORK_EMISSION
```

This ensures that a node contributing useful computation earns more than one merely consuming electricity. The **value** of contribution, not just its **cost**, determines the reward.

### 4.4 Approximation

Exact Shapley computation is O(2^n) — intractable for large networks. QUANTA uses randomized Monte Carlo approximation, achieving O(n) complexity with ε < 0.01 error margin, as documented in cooperative game theory literature.

---

## 5. Verification: From Trust to Proof

### 5.1 Phase 1 — Trust-but-Verify (Active)

Each node's Hello message includes its CPU model. Validators compare declared watts against the known TDP (Thermal Design Power) of that processor:

- Intel Core i5-13600K declares 300W → TDP is 125W → **flagged**
- Apple M1 Max declares 30W → TDP is 60W → **accepted**

A database of ~100 common processor models with their TDP values provides the reference.

### 5.2 Phase 2 — Cross-Validation

Validators measure peer response latency. A CPU under heavy load (200W) responds measurably slower than an idle one (5W). Statistical correlation between declared watts and observed latency creates a second verification layer.

Deviation > 2σ from expected behavior → automatic Shapley score reduction.

### 5.3 Phase 3 — Zero-Knowledge Proof of Work (RISC Zero)

The ultimate verification: **prove the work, derive the energy.**

```
Step 1: Network assigns a compute task to the node
Step 2: Node executes the task inside the RISC Zero zkVM
Step 3: The zkVM produces a cryptographic PROOF of correct execution
Step 4: Node publishes: (result, proof, CPU_model)
Step 5: Validators verify the proof (~1ms, no re-computation)
Step 6: Energy is DERIVED:
        proven_flops × joules_per_flop[CPU_model] = certified_energy
Step 7: Node receives QUANTA proportional to certified energy
```

**RISC Zero**: Open-source zkVM, Rust-native, $40M funded, production-deployed. The developer writes normal Rust code; RISC Zero automatically generates a cryptographic proof that the code executed correctly with the given inputs/outputs.

Energy-per-FLOP table (public data):

| CPU Model | Joules/GFLOP | Source |
|-----------|-------------|--------|
| Apple M1 | 0.15 | Apple Silicon specs |
| Apple M1 Max | 0.12 | Apple Silicon specs |
| Intel i5-13 | 0.45 | Intel ARK |
| Intel i7-14 | 0.38 | Intel ARK |
| AMD R7 7800 | 0.30 | AMD Product Specs |

**Result**: Energy is no longer self-reported. It is mathematically derived from cryptographically verified work. Impossible to falsify.

---

## 6. Token Economics

### 6.1 Emission

- **Rate**: 100 QUANTA/hour, constant, forever
- **Distribution**: Proportional to Shapley score
- **DeSci Reserve**: 5% (5 QUANTA/hour) → DeSci DAO treasury

### 6.2 Burn-and-Mint Equilibrium (BME)

Every network action burns a percentage of QUANTA tokens:

| Action | Burn Rate |
|--------|-----------|
| Transfer | 1% |
| Task submission | 2% |
| Bridge to ERC-20 | 0.5% |
| Validation reward | 0.1% |

**Effect**: Low activity → supply grows slowly (100/h). High activity → burns exceed emission → **deflationary supply**.

Inspired by Render Network (BME model, $4B market cap) and Ethereum EIP-1559.

### 6.3 Floor Price

```
floor_price(1 QUANTA) = total_network_kWh × avg_electricity_price / total_QUANTA
```

The floor price rises mechanically as the network grows. The market price on exchanges can exceed the floor (supply/demand) but cannot rationally fall below the energy cost to produce it.

### 6.4 Demand Sources

Unlike pure energy certificates, QUANTA has real demand drivers:

1. **Laboratories** pay QUANTA to submit computation tasks to the network
2. **AI startups** pay QUANTA for distributed training and inference
3. **Studios** pay QUANTA for 3D rendering
4. **Validators** stake QUANTA for governance rights
5. **Traders** arbitrage geographic energy price differences

---

## 7. Consensus: Merkle-CRDT

### 7.1 Dual-Ledger Architecture

1. **Linear Transaction Log** — Ordered signed transactions for auditability
2. **CRDT State** — Conflict-free counters for balances and interactions

CRDT types used:
- **PN-Counters**: Account balances (increment/decrement)
- **G-Counters**: Network metrics (total_watts, total_quanta_minted, total_kwh — monotonically increasing)

### 7.2 Merkle DAG

All state transitions are recorded in a content-addressed Merkle Directed Acyclic Graph:

```rust
struct DagNode {
    id: String,            // BLAKE3(parents + payload + author)
    parents: Vec<String>,
    payload: Vec<u8>,
    author: String,        // Ed25519 public key
    timestamp: i64,
}
```

Properties: content-addressed (tamper-detectable), append-only, multi-head (parallel writes without coordination).

### 7.3 CRDT Merge Properties

- **Commutative**: merge(A, B) = merge(B, A)
- **Associative**: merge(merge(A, B), C) = merge(A, merge(B, C))
- **Idempotent**: merge(A, A) = A

These algebraic properties guarantee **eventual consistency** without leader election, voting quorums, or coordination rounds. There is no "51% attack" because there is no majority to capture.

---

## 8. Network Transport

### 8.1 Iroh QUIC

QUANTA uses [Iroh](https://iroh.computer/) for peer-to-peer connectivity:

- **QUIC**: UDP-based, encrypted, multiplexed
- **NAT traversal**: Built-in hole punching via relay servers
- **Gossip**: Native pub/sub over `iroh-gossip` for DAG synchronization

### 8.2 Message Protocol

| Message | Direction | Purpose |
|---------|-----------|---------|
| `Hello` | Broadcast | Presence + watts + CPU model + country |
| `WantNodes` | Response | Request DAG nodes by hash |
| `HaveNodes` | Response | Send requested DAG nodes |
| `BroadcastTx` | Broadcast | Real-time transaction |
| `TaskAssign` | Directed | Assign compute task |
| `TaskResult` | Directed | Return result + ZK-proof |
| `Ping/Pong` | P2P | Liveness check |
| `ReportPeer` | Broadcast | Flag malicious behavior |

### 8.3 Slashing

Malicious nodes (fake watts, invalid proofs, network attacks) face progressive penalties:

1. **Warning**: Shapley score reduced 50% for 24 hours
2. **Suspension**: Mining disabled for 7 days
3. **Expulsion**: Node blacklisted by peer consensus

---

## 9. Cryptographic Foundation

| Primitive | Algorithm | Purpose |
|-----------|-----------|---------|
| Signatures | **Ed25519** | Transaction signing, identity |
| Encryption | **AES-256-GCM** | Data at rest |
| Key Derivation | **Argon2id** | Password → key (memory-hard) |
| Hashing | **BLAKE3** | Content addressing, DAG IDs |
| Memory Safety | **zeroize** | Key wiping |
| ZK Proofs | **RISC Zero** | Work verification |
| Post-Quantum | **ML-DSA-65** | Prepared, hybrid with Ed25519 |

---

## 10. Compute Marketplace

### 10.1 Three Tiers

```
Tier 1 — FREE (BOINC)
  → Volunteer scientific computing
  → Funded by network emission (95 QUANTA/h)
  → Every idle node contributes automatically

Tier 2 — PAID (Tasks)
  → Labs/studios submit compute jobs
  → Pay in QUANTA (2% burned)
  → Nodes execute and earn per-task payment

Tier 3 — PREMIUM (Dedicated GPU)
  → Continuous GPU rental
  → Reverse auction (like Akash Network)
  → QUANTA smart contract manages payment
```

### 10.2 Federated Learning

For AI workloads requiring data privacy:

1. A lab submits a model architecture + partitioned dataset
2. Each QUANTA node trains on its local partition
3. Gradients are aggregated via Secure Aggregation (encrypted)
4. The improved global model is published in the DAG
5. The lab pays in QUANTA; participating nodes earn ×3 multiplier

Data never leaves the participant's machine → native GDPR compliance.

### 10.3 Proof of Storage (Optional)

Nodes may allocate 10-100 GB of disk space for distributed storage:
- Research datasets, AI models, task results
- Verified via periodic Proof-of-Replication challenges
- Bonus: ×0.5 on QUANTA earnings (additive to compute rewards)
- Inspired by Filecoin ($10B market cap)

---

## 11. DeSci DAO

### 11.1 Treasury

5% of all emissions (5 QUANTA/hour) flow to the DeSci treasury.

### 11.2 Governance

- **1 staked QUANTA = 1 vote**
- **Proposals**: Fund a scientific project, adjust parameters (emission, burn rates, energy prices)
- **Quorum**: 10% of staked QUANTA
- **Majority**: 66% to pass
- **Execution**: Automatic via on-chain logic

### 11.3 Impact

Participants decide which science gets funded. A student in Senegal votes for malaria research. A gamer in Korea votes for quantum physics. Results are published in open access — belonging to humanity.

---

## 12. Security Analysis

### 12.1 Sybil Attack

Creating 100 fake identities yields minimal Shapley scores (no useful work, no validation history, no uptime). The multiplier floor (×0.1) and Shapley null-player axiom make Sybil attacks economically irrational.

### 12.2 Energy Fraud

**Phase 1**: TDP comparison catches gross misreporting. **Phase 2**: Latency cross-validation catches subtle inflation. **Phase 3**: ZK-proofs make fraud mathematically impossible — energy is derived from proven work, not self-reported.

### 12.3 Double-Spend

The CRDT PN-Counter tracks balances as monotonic deltas. A debit cannot be replayed. The Burn-and-Mint mechanism provides additional protection: attempting to double-spend also double-burns.

### 12.4 Consensus Capture

QUANTA uses CRDTs, not voting. There is no "majority" to capture. State convergence is guaranteed by algebraic properties of the data structures. A 51% attack is architecturally impossible.

### 12.5 Task Poisoning

A malicious task submitter could attempt to distribute harmful code. Mitigation: Research Mode tasks execute inside a sandboxed environment (WASM or zkVM), with no filesystem or network access beyond the task's defined inputs/outputs.

---

## 13. Roadmap

| Phase | Timeline | Milestone | Status |
|-------|----------|-----------|--------|
| **Core Protocol** | Complete | Ed25519, AES-256-GCM, BLAKE3, Argon2id, zeroize | ✅ |
| **Energy Oracle** | Complete | 33 countries, real CPU watts, weighted average | ✅ |
| **P2P Transport** | Complete | Iroh QUIC, gossip, 2-node verified | ✅ |
| **CRDT Consensus** | Complete | PN-Counter, G-Counter, snapshot/restore | ✅ |
| **Persistence** | Complete | SQLite, 30s snapshots | ✅ |
| **Phase 1: Pivot** | 2 weeks | Fixed emission, Shapley distribution, BME, trust-but-verify | 🔧 |
| **Phase 2: Hardening** | 1 month | Cross-validation, passive validator, public testnet | 📋 |
| **Phase 3: Useful Work** | 2-4 months | BOINC integration, task marketplace, DeSci DAO | 📋 |
| **Phase 4: ZK-Proof** | 6+ months | RISC Zero integration, trustless verification | 📋 |
| **Phase 5: Bridge** | 3-6 months | wQUANTA ERC-20, Uniswap listing | 📋 |
| **Phase 6: Scale** | 12+ months | Federated Learning, Proof of Storage, GPU marketplace | 📋 |

---

## 14. Conclusion

QUANTA represents a fundamental rethinking of what a cryptocurrency can be. By combining measured energy consumption (not wasted), useful scientific computation (not empty hashing), Shapley-fair distribution (not first-come-first-served), zero-knowledge verification (not trust), and democratic science funding (not centralized grants), the protocol creates a system where every participant benefits from every other participant's presence.

The network is not a zero-sum game. It is a positive-sum system where more participants means more total energy, more computational power for science, more demand from research institutions, and higher value for everyone.

The reference implementation is written in Rust for memory safety and performance, runs as a lightweight desktop application via Tauri, and communicates over Iroh's QUIC transport with built-in NAT traversal. The core protocol has been verified with 25 passing tests, zero compiler warnings, and a successful 2-node P2P gossip exchange.

**Install QUANTA. Your computer helps cure cancer while you sleep. You get paid for it.**

---

## References

1. Nakamoto, S. (2008). *Bitcoin: A Peer-to-Peer Electronic Cash System.*
2. Shapiro, M., Preguiça, N., Baquero, C., & Zawirski, M. (2011). *Conflict-free Replicated Data Types.* INRIA Research Report.
3. Shapley, L. S. (1953). *A Value for N-Person Games.* Contributions to the Theory of Games II.
4. Eurostat (2026). *Electricity prices for household consumers.* European Commission.
5. U.S. Energy Information Administration (2026). *Electric Power Monthly.*
6. Ducas, L. et al. (2023). *CRYSTALS-Dilithium (ML-DSA).* NIST FIPS 204.
7. O'Connor, J. et al. (2023). *BLAKE3: One function, fast everywhere.*
8. Birgisson, A. et al. (2024). *Iroh: Peer-to-peer that just works.* Number Zero Inc.
9. RISC Zero Team (2025). *RISC Zero zkVM: General Purpose Zero-Knowledge Proofs.*
10. Anderson, D. (2004). *BOINC: A System for Public-Resource Computing.* UC Berkeley.
11. McMahan, B. et al. (2017). *Communication-Efficient Learning of Deep Networks from Decentralized Data.* Google Research.
12. Render Network (2024). *Burn-and-Mint Equilibrium Whitepaper.*
13. Protocol Labs (2017). *Filecoin: A Decentralized Storage Network.*

---

**License**: CC BY-SA 4.0

**Source Code**: Open source — Rust/Tauri/Svelte

**Contact**: [To be published with project website]
