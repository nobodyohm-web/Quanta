---
type: task-spec
id: CRYPTO-ID-1
status: à exécuter (audit d'abord ; prérequis de GADGET-3)
priorité: 🟠 identité — faire coïncider identité d'enjeu et identité de finalité (sinon GADGET-3 ne peut pondérer les votes)
classe: audit Ed25519 vs ML-DSA + réconciliation triviale si triviale, sinon STOP §4
origine: GADGET-2 §4 (deux clés disjointes : enjeu Ed25519, vote ML-DSA)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[ADR-005 — Agrégation des votes]] · [[DESIGN-FINALITY-GADGET]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# CRYPTO-ID-1 : réconcilier l'identité d'enjeu et l'identité de finalité

> GADGET-2 a révélé deux identités **disjointes** : `validator_stakes()` est indexé par la clé
> de compte **Ed25519**, mais les votes de finalité sont signés en **ML-DSA**. GADGET-3 câble de
> vrais votes pondérés par l'enjeu : il lui faut **mapper un vote à son poids**, donc les deux
> identités doivent **coïncider**. Ce spec établit d'abord l'état réel (audit), puis réconcilie
> si c'est trivial, sinon **s'arrête** sur la décision qui t'appartient. Diff logique seule,
> déterministe.

## 1. Audit (lecture seule, le fait à rapporter)
Trace les identités cryptographiques à travers le système, **preuves à l'appui** :
- Qu'est-ce qui **signe une transaction** ? Ed25519, ML-DSA, les deux ? (où : `hybrid_crypto.rs`,
  `ledger.rs`, builder de tx)
- Quelle clé **identifie un compte** ? Quelle clé **indexe l'enjeu** (`validator_stakes()`) ?
- Quelle clé **signe les votes** de finalité (GADGET-2) ?
- **Où vit chaque clé** et comment elles se relient (ou pas) aujourd'hui.

Rapporte l'état **réel** du partage Ed25519 / ML-DSA. C'est lui qui décide la suite.

## 2. La décision de fond que ça soulève (🛑 ta décision de vision)
Jusqu'où va le post-quantique dans Quanta ? Deux chemins :
- **(a) Comptes Ed25519 + finalité PQ seulement** : les comptes/tx restent Ed25519, les
  validateurs enregistrent une clé de finalité ML-DSA (registre de liaison). Plus léger, mais
  les comptes restent **vulnérables au quantique**, ce qui **contredit** « entièrement PQ ».
- **(b) Tout en ML-DSA** : comptes, tx, enjeu, finalité, tout en post-quantique. La proposition
  de valeur **sans astérisque**. Plus lourd **si** les comptes sont aujourd'hui Ed25519.

**Recommandation (à ratifier) : (b).** Une signature de transaction est de **longue vie** ; si
elle est en Ed25519, un adversaire quantique futur peut forger des transactions et voler des
fonds (« récolter aujourd'hui, forger demain »). Pour une monnaie qui se dit post-quantique,
l'identité des comptes **doit** être PQ. L'audit du §1 dira **combien** ce choix coûte.

## 3. La réconciliation (selon ce que dit l'audit)
Le but **minimal** de ce spec : que l'**identité d'enjeu et l'identité de finalité coïncident**,
pour qu'un vote se mappe à son poids (besoin de GADGET-3).
- **Si l'audit montre que les comptes sont déjà ML-DSA** (le keying Ed25519 de l'enjeu n'étant
  qu'un vestige) : **re-clé** `validator_stakes()` / l'identité de finalité sur la clé ML-DSA,
  pour qu'enjeu et finalité partagent **la même** identité. Petit, sûr, **fais-le**.
- **Si l'audit montre que les comptes sont vraiment Ed25519** (donc le « tout PQ » exige une
  **migration de comptes**) : **§4 STOP**. Rapporte la portée de la migration, **ne migre pas**
  les comptes ici. C'est la décision (b) ci-dessus, à Alexandre.
- Dans tous les cas, **ne décide pas** la portée du PQ ; **réconcilie** seulement ce qui est
  trivial, **escalade** le reste.

## Garde-fous
- **Audit d'abord, lecture seule** ; aucune modification avant d'avoir rapporté l'état réel.
- **La décision de portée PQ est à Alexandre** : re-keying trivial autorisé, **migration de
  comptes interdite** dans ce spec (§4 STOP).
- **Diff logique seule** pour tout changement ; `src/sm/` sans-IO ; **C1 vert**.
- **Pas de masquage** : si les identités ne peuvent pas coïncider trivialement, **dis-le**, ne
  bricole pas un pont fragile.
- **Déterminisme** : toute clé servant au consensus reste une donnée déterministe de l'état.
- **Snapshot git** avant tout changement.

## Porte d'acceptation
- **Rapport d'audit** §1 livré (qui signe les tx, quelles clés, où, comment reliées).
- **Décision de portée PQ** clairement cadrée pour Alexandre, **avec les faits réels** (§2).
- **Si** une réconciliation triviale a eu lieu (§3, cas comptes-déjà-ML-DSA) : `cargo test --lib`
  **vert**, clippy propre, **C1 vert**, sweep vert, diff logique seule, `dispatcher.rs` intact.
- **Sinon** : §4 STOP documenté (portée de la migration de comptes), aucun code de migration
  écrit.
- Entrée **CRYPTO-ID-1** au tracker + auto-revue §3.

## Séquence
1. **§1** auditer les identités cryptographiques, rapporter l'état réel.
2. **§2** cadrer la décision de portée PQ pour Alexandre, avec les faits.
3. **§3** réconcilier si trivial (re-keying enjeu↔finalité), **sinon §4 STOP**.

> Une fois enjeu et finalité sous **la même** identité, GADGET-3 (la règle justifier/finaliser)
> pourra pondérer de vrais votes et la finalité cessera d'être vacueuse. Si l'audit révèle une
> migration de comptes, c'est ta décision (b) à prendre avant d'aller plus loin. Reste aussi
> §12-réglable : E, taille de comité, quorum.
