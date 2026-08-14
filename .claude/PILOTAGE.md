---
type: moc
updated: 2026-08-01
---

# 🧭 Pilotage QUANTA

Centre de navigation du projet **et** surface de travail avec Claude. Ouvre ce
vault dans Obsidian (graphe + backlinks) et édite les mêmes fichiers dans
Antigravity. Tape `[[` pour lier ; renomme librement, les liens se réparent.

> [!abstract] Mission
> Monnaie souveraine P2P : saine, rare, vérifiable. Plafond dur 100M gravé,
> zéro premine, zéro autorité d'émission. Voir [[CLAUDE]].

## 🗺️ Carte du projet
- **Cap & spec** — [[CLAUDE]] (north star, stack, invariants)
- **Méthode** — [[QUANTA_AGENT_CONSTITUTION]] (Phase 0 d'abord ; règle d'arrêt §4 : on **n'invente pas** un arbitrage consensus/sécurité)
- **Phase 0 (close, hors T0.3 additif)** — [[QUANTA_T0_DST_HARNESS]] (harness déterministe T0.1→T0.8)
- **Journal vivant** — [[AUDIT_QUANTA_2_PROGRESS]] (état réel, tests, auto-revues §3)
- **Backlog clos** — [[QUANTA_PATCH_CORRECTIONS]] (C1→C8 ✅)
- **Consensus (futur)** — [[DESIGN-CONSENSUS-DAG-BFT]] (umbrella) · [[DESIGN-FINALITY-GADGET]] (Phase 1 — gadget Casper FFG, PQ, par époque ; *implémenté GADGET-1→5B, prouvé en simulation ; **câblage vivant complet LIVE-1→4***) · [[DESIGN-LIVE-WIRING]] (câblage du gadget en vivant — **LIVE-1→4 + 3B livrés**)
- **Rareté** — [[TOKENOMICS_V2]]
- **Cap produit** — [[ROADMAP_WEB3]] · [[BRAND_AND_TRUST]]
- **Sécurité** — [[SECURITY]] · [[SECURITY_POC_V2]]
- **Livre blanc** — [[WHITEPAPER]] · [[WHITEPAPER_FR]]
- **Règles de code** — `.claude/rules/` (côté éditeur : `rust`, `security`, `p2p-protocol`, `network-quality`, `frontend` — dossier masqué dans Obsidian)

## 📍 Où on en est
- **Phase 0 / T0.1** — cœur sans-IO `sm::Node` (Tick, MessageReceived, scellement à temps injecté).
- **Backlog C1→C8** ✅ — déterminisme transitif (méta-test 128 runs), sync-replay, observabilité consensus, reorg/rejets, admission signature-gated (typestate), zeroize, scellement déterministe, propagation transport-flood — [[AUDIT_QUANTA_2_PROGRESS]].
- **T0.4 (simulateur) — tr.1+2** ✅ — `sm/sim.rs` : horloge virtuelle + scheduler `(time_ms, seq)` + flood (C8). Convergence 3-nœuds, run byte-déterministe, **proposition event-driven** (timer consensus), **sync de chaîne** (`RequestChain → ChainSegment`).
- **T0.5 (fautes réseau)** ✅ — `NetFaults` : drop / dup / délai variable (⇒ réordre) / **partition** / withhold, piloté par RNG seedé. Phare : partition isole A↔B → `heal` → B rattrape par sync.
- **T0.6 (byzantins + fautes nœud)** ✅ — **crash/restart**, **rétention** de bloc, **équivocation** + **primitive de détection** (l'évidence du futur slashing). Phare : équivocation → honnêtes **convergent** + équivocation **détectée**.
- **T0.7 (invariants à chaque pas)** ✅ — **sûreté** + **conservation** µQTA, `run_checked` + `Violation{seed}`. Les dents : partition → fork → **sûreté violée détectée**. A fait remonter une **vraie faiblesse de hachage de bloc**.
- **BLK-HASH-1 (faiblesse trouvée par le harnais)** ✅ — le hash de bloc commet maintenant le **contenu** (Merkle content+sig, domaine-séparé anti-CVE-2012-2459) **et le `miner`** ; matching inter-nœuds + dedup content-addressed. **Vol de récompense rejeté** (T2), pas de double-mint au reorg (T5), pansement timestamp retiré (T4). Le harnais a aussi exposé un bug masqué (`int1` dépendait de la collision → corrigé sans masquage). **234 tests** verts. Spec : `QUANTA_BLK_HASH_INTEGRITY.md`.
- **EMIT-1 (double-mint au reorg + invariant d'émission)** ✅ — le harnais a trouvé que la **conservation est aveugle au mint illégitime**. §3 STOP déclenché (la prod scelle légitimement plusieurs récompenses/bloc) → **décision : Option A, une récompense par bloc**. Re-queue filtré (synthétiques exclus, §4.1), règle de validation ≤1 minage + `to==miner` sur **les deux chemins** (§4.2), **coalescing au seal** (`NETWORK→miner`, Σ, déterministe, `ts` injecté), 3ᵉ invariant `Violation::Emission` (§4.3). **E1–E5** verts + 4 fixtures legacy migrées (modèle pré-Option-A) ; `int1`/C4 ajustés. **239 tests**. Spec : `QUANTA_EMISSION_INTEGRITY.md`.
- **T0.8 (porte globale)** ✅ — runner multi-seed + replay `--seed`, trois invariants (sûreté + conservation + émission). Phase 0 close hormis T0.3 additif.
- **ONCHAIN-STAKE-1 / COVER-1 / COVER-2** ✅ — enjeu on-chain seul (fork fermé) ; couverture au bloc (réception + seal) ; conservation `Σ(dépensable+staké+déverr.)+brûlé==miné`.
- **Gadget de finalité GADGET-1→5B** ✅ — checkpoints (E=32) · votes ML-DSA + certificat ⅔ · justify/finalize (Casper-FFG) · slashing détecté (double-vote+surround) · fork-choice LMD-GHOST + réconciliation de partition. **Prouvé en simulation DST**, cœur `sm/` sans-IO (C1).
- **PQ-MIG-1→5** ✅ — identité de compte entièrement ML-DSA (adresse `BLAKE3(ADDR_DOMAIN‖clé)`), autorité de tx pur ML-DSA-65, genèse post-quantique ; `TORUS_PROTOCOL_VERSION` 2→3.
- **ADR-005→009** ✅ — agrégation PQ des votes ; §12 figé (E=32, quorum ⅔, unbonding 10 080, slash brûlé/plein) ; ADR-006 ratifiée, ADR-007 réalisée, ADR-008 reversé.
- **LIVE-1 (câblage vivant)** ✅ — gossip des votes de finalité (`FinalityVote` + dispatcher + `FinalityTracker`), les votes peuplent `LatestVotes`/`FinalityState` du ledger vivant — **379 tests**.
- **LIVE-2/3/3B/4 (câblage vivant complet)** ✅ — plancher de finalité persisté qu'aucun fork ne franchit ; slashing vivant STAKE→BURN atteignant l'enjeu en déverrouillage (« unstake-and-run » fermé) ; réconciliation de fork profonde (deux partitions ≥2 blocs convergent).
- **Écosystème de nœud** ✅ — daemon `quanta-node` headless, JSON-RPC 17 méthodes (authentifié), explorateur web, adresses `qta1…` Bech32m, multisig M-of-N ML-DSA (MSIG-1) ; protocole 5→6.
- **Audit interne v3.13 + hard-fork v7** ✅ (25/07/2026) — 4 critiques, 8 hauts, 4 moyens fermés → [[AUDIT-INTERNE-2026-07-25]].
- **Nettoyage v3.13.1** ✅ (01/08/2026) — code mort et dépendances jamais importées purgés ; 477 tests + 1 intégration, clippy propre, svelte-check 0/0.
- **Reste** — audit externe (dossier prêt, non commandé) ; testnet multi-nœuds durable ; notarisation macOS + release à jour ; ADR-004 (VRF+VDF) ; UX multi-partie du multisig ; T0.3 (coquille prod) additif.

## 🔀 Décisions d'architecture — état
Le simulateur va **forcer** ces choix. Cadrées en ADR → [[docs/decisions/README|Registre des décisions]].
Cadre commun (trajectoire + méta-décisions §7) : [[DESIGN-CONSENSUS-DAG-BFT]].

**Périmètre tranché (2026-06-21)** : **Option 1 — finality gadget d'abord** (chaîne
linéaire + vote BFT qui finalise), **stake on-chain seul** pour le comité.

| Décision | ADR | Statut |
|---|---|---|
| Validator set & comité | [[ADR-002 — Validator set & comité BFT]] | ✅ **ACCEPTÉE** — stake on-chain seul |
| Fork-choice | [[ADR-001 — Fork-choice]] | ✅ résolu — gadget + fork-choice GHOST pondéré stake (GADGET-5A/5B) |
| Slashing | [[ADR-003 — Slashing (accountable safety)]] | ✅ tranchée — slashing d'équivocation implémenté (GADGET-4) ; politique fixée par ADR-009 (brûlé/plein/fenêtre=unbonding) ; **câblé en vivant (LIVE-3/3B)** |
| Aléa d'élection | [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]] | OUVERTE — beacon OK P1 ; ECVRF/VDF→P2 (sauf si tu veux + tôt) |
| Signatures (agrégation votes) | [[ADR-005 — Agrégation des votes & certificats de finalité]] | ✅ **ACCEPTÉE** — PQ pur (ML-DSA) par époque ; comité/quorum/époque fixés par ADR-009 (E=32, quorum ⅔ gravé, pas de comité échantillonné) |
| Gouvernance & évolutivité | [[ADR-006 — Gouvernance & évolutivité]] | ✅ RATIFIÉE — par ADR-009 |
| Portée du post-quantique | [[ADR-007 — Portée du post-quantique (comptes ML-DSA)]] | 🟢 RÉALISÉE (b) — comptes ML-DSA, PQ-MIG-3B |
| Autorité de tx via liaison ML-DSA | [[ADR-008 — Autorité de tx via liaison ML-DSA on-chain (PQ-MIG-3)]] | 🔴 REVERSÉ — 2026-06-25 |
| Frontière gravé-ajustable & §12 | [[ADR-009 — Frontière gravé-ajustable (ADR-006 ratifiée) et valeurs du §12]] | ✅ ACCEPTÉE — §12 figé |

> [!question] Comment on avance
> Chaque ADR est **OUVERTE** : contexte (code réel), options + conséquences,
> contraintes croisées, et précisément **ce dont j'ai besoin de toi**. Tu
> tranches une ligne → je la grave (ACCEPTÉE) et j'enchaîne le code.

## 🤝 Travailler avec Claude
- **Charte** — [[QUANTA_AGENT_CONSTITUTION]] : incréments minimaux vérifiables, tests-d'abord, **règle d'arrêt §4** (remonter, jamais deviner).
- **Définition de « fait »** — `cargo test` 0 fail · `clippy -D warnings` · conservation prouvée · pas de régression déterministe · CHANGELOG + auto-revue §3 dans [[AUDIT_QUANTA_2_PROGRESS]].
- **Honnêteté radicale** — aucune valeur fabriquée (pas de prix marché) ; chaque chiffre colle au code.
- **Mémoire** — Claude tient une mémoire persistante du projet (faits non dérivables du code) ; ce hub en est la version humaine, navigable.

## ▶️ Prochaine action
Le code n'est plus le goulot. Dans l'ordre : **(1)** commander l'**audit externe**
(dossier prêt dans [[docs/audit/README|docs/audit]] — threat model, périmètre, RFQ) ;
**(2)** faire tourner un **testnet multi-nœuds durable** sur la genèse actuelle — l'échelle
réelle n'a jamais dépassé deux machines ; **(3)** notarisation macOS + pipeline de release
signé, la dernière release publiée datant de mai 2026. Ensuite seulement : ADR-004 (vrai VRF
+ VDF) et l'UX multi-partie du multisig.
