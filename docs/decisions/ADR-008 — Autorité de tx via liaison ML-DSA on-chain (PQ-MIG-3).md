---
type: adr
id: ADR-008
status: reversed
decision-class: portée d'implémentation (tranchée 2026-06-24 ; **reversée** 2026-06-25 par PQ-MIG-3B)
proposed: 2026-06-24
ratification: tranchée (Alexandre — « Authority layer, keep `from` ») puis **reversée** (« terminer le tout-PQ, sans astérisque »)
origine: PQ-MIG-3 (mise en œuvre) — découverte de l'identifiant de compte unifié
amende: ADR-007 — **rétabli** : la recommandation (b) « comptes tout ML-DSA » est **réalisée** (PQ-MIG-3B)
liens: [[ADR-007 — Portée du post-quantique (comptes ML-DSA)]] · CRYPTO-ID-1 · [[AUDIT_QUANTA_2_PROGRESS]]
---

# ADR-008 — Autorité de tx post-quantique (PQ-MIG-3) — **REVERSÉ par PQ-MIG-3B**

← [[README|Registre ADR]] · **rétablit** [[ADR-007 — Portée du post-quantique (comptes ML-DSA)]] (b)

> [!warning] STATUT — **REVERSÉ le 2026-06-25 (PQ-MIG-3B)**
> Cet ADR avait choisi l'**Option (a)** d'ADR-007 (garder `from` = clé Ed25519 + **registre de
> liaison on-chain**), en assumant un **astérisque permanent** sur « comptes entièrement
> post-quantiques ». **PQ-MIG-3B a reversé cette décision** : `from`/`to` deviennent l'**adresse
> ML-DSA** (`BLAKE3(ADDR_DOMAIN ‖ clé)`, PQ-MIG-2) **partout** — solde, récompense (`mine_tx`),
> enjeu/validateur (`validator_stakes`), `@pseudo`. **ADR-007 (b) est tenu, sans astérisque.** Le
> texte ci-dessous est conservé pour la trace ; la section [§ Réversion](#réversion-pq-mig-3b) en
> bas explique pourquoi le **fait du code** qui semblait bloquer (b) en est en réalité la
> justification.

## Contexte — le fait qui force la décision

ADR-007(b) (et donc les specs PQ-MIG-1→3) supposent qu'on peut faire de `tx.from` une **adresse
ML-DSA** pour l'autorité **sans** toucher l'enjeu (différé à PQ-MIG-4) ni la genèse (PQ-MIG-5).

**Le code dit le contraire.** L'identifiant de compte est **unifié** : le **même** `tx.from`
sert simultanément de
- clé de **solde** (`balance_cache`),
- **cible de minage** (`mine_tx → to = miner_pk`),
- clé d'**enjeu/validateur** (`staked: HashMap<from, …>`, `validator_stakes`),
- liaison **@pseudo**,
- et (à l'origine) identité de **transport** gossip.

Or `Stake`/`Unstake` sont des **tx signées** qui passent par `verify_tx` et dont `validator_stakes`
indexe sur `from`. Donc changer `from` en adresse ML-DSA **re-clé inévitablement l'enjeu et le
minage du même geste**. ADR-008 (2026-06-24) a d'abord **lu cela comme un blocage** du découpage
PQ-MIG-3→4 ⇒ §4 STOP ⇒ Option (a). **PQ-MIG-3B (2026-06-25) le relit comme une opportunité** :
l'identité étant **unifiée**, (b) n'est pas une cascade fragile mais une **unique bascule
cohérente** — « tout bascule ensemble, c'est voulu ».

## Décision d'origine (2026-06-24, depuis reversée)

**Option (a) — « couche d'autorité, garder `from` ».** `from` = Ed25519 ; **registre de liaison
on-chain** `from → clé ML-DSA` (premier-vu immuable) ; autorité = signature ML-DSA de la clé liée
(+ Ed25519 co-facteur). Astérisque assumé : `from` reste Ed25519.

## Réversion (PQ-MIG-3B)

**Décision rétablie : ADR-007 (b), sans astérisque.**

1. `from`/`to` = **adresse ML-DSA** (`BLAKE3(ADDR_DOMAIN ‖ clé)`, PQ-MIG-2) **partout** : solde,
   récompense (`mine_tx` crédite l'adresse), enjeu/validateur (`validator_stakes` indexé par
   adresse), `@pseudo` (`owner_pk` = adresse, signé ML-DSA + `lie`).
2. **Autorité de `verify_tx`** = **pur ML-DSA** : la clé révélée doit **se lier** à `from`
   (`lie(from, clé)` : `from == BLAKE3(ADDR_DOMAIN ‖ clé)`) **et** signer la préimage. Le
   co-facteur Ed25519 **quitte le chemin d'autorité** — casser Ed25519 ne donne **aucun** pouvoir
   sur un compte, puisque `from` ne dérive plus d'une clé Ed25519.
3. **CRYPTO-ID-1 fermé *intrinsèquement*** : la liaison clé↔compte n'est plus un **état**
   (registre) mais une **fonction sans état** — une clé différente ⇒ un `from` différent. Le
   registre de liaison de PQ-MIG-3 est **conservé** comme filet redondant, désormais subsumé.
4. **Transport** (gossip / PeerId / enveloppes) reste **Ed25519**, éphémère, **hors chemin de
   valeur** — différé (conception §6, garde-fou §4). C'est la *seule* couche encore classique.

### Pourquoi c'est sûr — la liaison est intrinsèque

L'attaque CRYPTO-ID-1 (casser Ed25519, attacher sa propre clé ML-DSA à `from`) est **impossible
par construction** : `from` **EST** `BLAKE3(ADDR_DOMAIN ‖ clé)`. Révéler une autre clé donne un
autre hash ≠ `from` ⇒ `lie` faux ⇒ rejet (`verify_tx` Ok(false)). Aucune signature classique forgée
ne change ce fait. Teeth : `pqmig3_unbound_key_rejected_closes_crypto_id_1`,
`pqmig3b_nominal_address_tx_accepted` (ledger), `rejects_unbound_key_closes_pseudo_hijack`
(username).

### Conséquences

- **Tenu, sans astérisque** : l'**identité de compte** est entièrement post-quantique. Une rupture
  d'Ed25519 ne permet ni de dépenser, ni de staker, ni de revendiquer un `@pseudo`.
- **Conservation / couverture** inchangées : on change la **clé d'indexation** (adresse au lieu de
  clé Ed25519), pas la valeur ; COVER-1/2 et `Σ(dépensable+staké+déverrouillage)+brûlé==miné`
  restent verts sur l'identité ML-DSA. **C1** (déterminisme) vert : l'adresse est une fonction pure
  BLAKE3.
- **PQ-MIG-4 / GADGET-3 débloqués** : l'identité d'enjeu = l'identité de finalité (toutes deux
  l'adresse ML-DSA), donc « faire coïncider enjeu et vote de finalité » est immédiat.
- **Reste** : PQ-MIG-5 (genèse PQ) et, si un jour voulu, le **transport** PQ (la dernière couche
  Ed25519).
