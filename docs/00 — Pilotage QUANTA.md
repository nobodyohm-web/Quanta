---
type: moc
updated: 2026-06-21
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
- **Phase 0 (en cours)** — [[QUANTA_T0_DST_HARNESS]] (harness déterministe T0.1→T0.8)
- **Journal vivant** — [[AUDIT_QUANTA_2_PROGRESS]] (état réel, tests, auto-revues §3)
- **Backlog clos** — [[QUANTA_PATCH_CORRECTIONS]] (C1→C8 ✅)
- **Consensus (futur)** — [[DESIGN-CONSENSUS-DAG-BFT]]
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
- **Reste Phase 0** — **T0.8 débloqué** (runner **multi-seed** + replay `--seed`, **porte globale**, vérifiant désormais **trois** invariants : sûreté + conservation + **émission**) ; puis `Hello`/`Command`, et T0.3 (coquille prod) en additif. Slashing = réaction, attend [[docs/decisions/ADR-003 — Slashing (accountable safety)|ADR-003]].

## 🔀 Décisions ouvertes (à cadrer avant T0.4)
Le simulateur va **forcer** ces choix. Cadrées en ADR → [[docs/decisions/README|Registre des décisions]].
Cadre commun (trajectoire + méta-décisions §7) : [[DESIGN-CONSENSUS-DAG-BFT]].

**Périmètre tranché (2026-06-21)** : **Option 1 — finality gadget d'abord** (chaîne
linéaire + vote BFT qui finalise), **stake on-chain seul** pour le comité.

| Décision | ADR | Statut |
|---|---|---|
| Validator set & comité | [[ADR-002 — Validator set & comité BFT]] | ✅ **ACCEPTÉE** — stake on-chain seul |
| Fork-choice | [[ADR-001 — Fork-choice]] | **résolu par le gadget** ; reste un départage stake intérim |
| Slashing | [[ADR-003 — Slashing (accountable safety)]] | OUVERTE — brûlé vs redistribué ? montant ? fenêtre ? |
| Aléa d'élection | [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]] | OUVERTE — beacon OK P1 ; ECVRF/VDF→P2 (sauf si tu veux + tôt) |
| Signatures (agrégation votes) | [[docs/decisions/README\|§7]] | OUVERTE — BLS non-PQ vs 100 % hybride PQ |

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
Cadrer les **4 décisions ci-dessus** en ADR, puis enchaîner T0.3 → T0.4 avec des
réponses au lieu de placeholders.
