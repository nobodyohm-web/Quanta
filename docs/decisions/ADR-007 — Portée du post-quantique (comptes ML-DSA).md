---
type: adr
id: ADR-007
status: proposed
decision-class: vision fondatrice (engagement du fondateur — recommandation forte (b))
proposed: 2026-06-24
ratification: en attente (Alexandre — l'engagement au coût de (b) est le déclencheur)
origine: CRYPTO-ID-1 (audit identité Ed25519 vs ML-DSA)
bloque: GADGET-3 (a besoin de l'identité autoritaire tranchée)
updated: 2026-06-24
---

# ADR-007 — Portée du post-quantique : comptes en ML-DSA (Quanta entièrement post-quantique)

← [[README|Registre ADR]] · cadre : [[DESIGN-CONSENSUS-DAG-BFT]]
Lié à : [[ADR-005 — Agrégation des votes & certificats de finalité]] · [[ADR-006 — Gouvernance & évolutivité]] (signatures PQ = noyau **gravé**) · **CRYPTO-ID-1** (audit, `AUDIT_QUANTA_2_PROGRESS.md`) · **bloque GADGET-3**

> [!warning] DÉCISION DE **VISION FONDATRICE** (proposée 2026-06-24 — recommandation **forte (b)**, à ratifier)
> L'audit **CRYPTO-ID-1** a prouvé, `fichier:ligne` à l'appui, que « entièrement post-quantique »
> **n'est pas tenu** : comptes, autorisation de tx, enjeu et transport sont **enracinés Ed25519** ;
> la couche ML-DSA est **auto-déclarée par tx et non liée** au compte, donc elle **ne protège pas**
> les comptes contre la forge quantique. Deux chemins : **(a)** comptes Ed25519 + finalité PQ via
> registre (léger, **astérisque permanent**) ; **(b)** comptes **tout ML-DSA** (lourd, promesse
> **sans astérisque**). **L'ingénierie recommande (b) sans réserve.** L'**engagement** au coût de (b)
> appartient au fondateur (horizon, ressources, tolérance) — c'est la part §4/vision. **Aucun code de
> migration n'est écrit avant ratification** ; la migration se **conçoit** ensuite (doc de conception
> → specs chirurgicaux), comme le gadget. Cet ADR **bloque GADGET-3** (qui exige l'identité
> autoritaire tranchée).

## Contexte

L'audit **CRYPTO-ID-1** a révélé, preuves à l'appui, que la promesse « entièrement post-quantique »
**n'est pas tenue dans le code**. Comptes, autorisation de transaction, enjeu et transport sont
**enracinés Ed25519**.

Le piège subtil : la signature est **hybride** (Ed25519 + ML-DSA), mais la **racine de confiance**
d'un compte reste Ed25519, pour deux raisons. `REQUIRE_PQ = false` autorise un repli Ed25519 seul. Et
la clé ML-DSA est **auto-déclarée par transaction** et **jamais liée** au compte (`verify_hybrid`
vérifie `ed_ok && pq_ok` sans contrôler l'appartenance). Donc un adversaire quantique qui casse
Ed25519 forge la signature classique de `from`, **attache sa propre clé ML-DSA**, et passe la
vérification. La couche PQ **ne protège pas** les comptes. Parler de « comptes post-quantiques »
aujourd'hui serait du mensonge.

Découvert **avant la genèse**, donc au bon moment. La question : **Quanta est-elle réellement
entièrement post-quantique ?**

## Vérification — les faits qui fondent la décision (audit CRYPTO-ID-1, 2026-06-24)

> *Constitution §2 : « aucune assertion de sécurité sans vérification ». La prémisse de cet ADR est
> falsifiable et a été **vérifiée** (lecture seule).*

- **Compte = Ed25519.** `public_key_hex = hex(vk.to_bytes())`, `vk` = clé Ed25519 (`security/mod.rs:45-46`) ;
  `import_keypair` part d'un secret **Ed25519** 32 o (`security/mod.rs:56`).
- **Repli Ed25519 autorisé.** `pub const REQUIRE_PQ: bool = false` (`security/hybrid_crypto.rs:58`) ;
  `verify_hybrid` retombe en Ed25519-seul si la couche PQ est absente (`:132-140`).
- **Clé ML-DSA non liée.** `verify_hybrid` = `ed_ok && pq_ok`, **aucun cross-check** pq↔ed
  (`security/hybrid_crypto.rs:129-145`) ; `tx.pq_public_key: Option<String>` est **portée par chaque
  tx** (`ledger_types.rs:31`), jamais un registre lié au compte.
- **Enjeu keyé Ed25519.** `staked.entry(tx.from)` (`ledger.rs:403`), `validator_stakes()` filtre
  `staked` (`ledger.rs:513`) ⇒ enjeu indexé par le compte **Ed25519**.
- **Finalité keyée ML-DSA.** GADGET-2 (`sm/finality_vote.rs`) : `Vote.validator` = clé publique ML-DSA.
- **Transport Ed25519.** `GossipEnvelope` : pubkey + signature **Ed25519** (`gossip.rs:39,42`).

⇒ enjeu (Ed25519) et finalité (ML-DSA) **disjoints parce que les comptes sont Ed25519** ; **aucune
réconciliation triviale** (CRYPTO-ID-1 §3 → §4 STOP). D'où cet ADR de portée.

## Les deux chemins

- **(a) Comptes Ed25519 + finalité PQ seulement**, via un registre on-chain liant compte → clé
  ML-DSA de finalité. **Léger.** Mais les **comptes restent vulnérables au quantique** : la promesse
  « entièrement PQ » garde un **astérisque permanent**, sur exactement le différenciateur du projet.
- **(b) Comptes entièrement en ML-DSA.** Enjeu et finalité coïncident **nativement**. **Lourd** :
  re-raciner `CryptoEngine` / le vault (`pq_vault.rs`), adresses hachées en BLAKE3 (une clé ML-DSA
  fait ~1952 o), refaire `verify_tx`, le builder, `validator_stakes`, le registre `@pseudo`, migrer
  les soldes existants, et un bump de `TORUS_PROTOCOL_VERSION`. La promesse **sans astérisque**.

## Décision (recommandée, à ratifier) : (b), tout en ML-DSA

Pour une monnaie dont la **raison d'être** est le post-quantique, l'astérisque de (a) n'est pas un
compromis, c'est un **renoncement** à la proposition de valeur, sur la chose même qui distingue
Quanta. Un auditeur qui trouverait ce que CRYPTO-ID-1 a trouvé ferait s'effondrer la revendication.
Et une signature de transaction est de **longue vie** : si elle est forgeable par un futur ordinateur
quantique, des fonds réels sont volables (« récolter aujourd'hui, forger demain »). (a) construit une
monnaie **qui n'est pas ce qu'elle prétend être**.

## Le coût, sans le minimiser

(b) est vraisemblablement le **plus gros chantier** du projet, plus gros que le gadget : il re-racine
la fondation cryptographique. Il **doit se concevoir** (un document de conception, puis des specs
chirurgicaux), comme on l'a fait pour le gadget. Il ne se balance pas en un jet.

## Ce qui appartient au fondateur (la part §4 / vision)

L'**engagement** à payer ce coût, contre l'alternative d'expédier plus vite avec l'astérisque de (a),
dépend de l'**horizon, des ressources et de la tolérance** d'Alexandre, que la recommandation
d'ingénierie ne peut pas trancher. Cet ADR fixe la **recommandation** ((b)) ; le fondateur **ratifie
l'engagement**.

## Conséquences

- **(b)** : Quanta devient **réellement** entièrement post-quantique. Re-architecture lourde,
  lancement retardé, mais la proposition de valeur tient **sans astérisque**. **Débloque GADGET-3**
  (enjeu et finalité partagent **nativement** la même identité — fin de la disjonction de CRYPTO-ID-1).
- `REQUIRE_PQ` doit passer à **true** ; le repli Ed25519 est **retiré** ; la clé PQ auto-déclarée par
  transaction est remplacée par l'**identité ML-DSA du compte**.
- **Cohérence ADR-006** : « signatures post-quantiques » figure déjà dans le **noyau gravé** ([[ADR-006 — Gouvernance & évolutivité]]) ;
  (b) est ce qui rend cet invariant gravé **honnête** au niveau du compte (aujourd'hui il ne l'est
  qu'au niveau de la couche de signature, pas de la racine).
- La migration **mérite son propre document de conception**, puis des specs chirurgicaux.

## Alternatives considérées

- **(a) registre** : *rejetée comme recommandation*. Astérisque permanent sur la revendication
  centrale ; comptes forgeables par un adversaire quantique. Acceptable **seulement** si le fondateur
  choisit **consciemment** un v1 plus rapide avec une couche de comptes non-PQ, ce qui contredit la
  prémisse du projet.
- **Statu quo** (hybride mais enraciné Ed25519) : *rejeté*, c'est l'état de **fausse promesse** que
  CRYPTO-ID-1 a exposé.

## Statut & ce dont j'ai besoin de toi (🛑)

ADR **proposé** — recommandation **forte (b)**. À ratifier par Alexandre :

- **L'engagement du fondateur à (b)** (le déclencheur).
- Ensuite : un **document de conception** de la migration (stratégie de re-racinage, schéma
  d'adresses, migration des soldes, version de protocole), **puis** des specs chirurgicaux.

> Cet ADR **bloque GADGET-3** (qui a besoin de l'identité autoritaire tranchée). C'est la décision la
> plus fondatrice qui reste, et elle vaut d'être prise à tête reposée. **Aucun code de migration**
> n'est écrit tant qu'elle n'est pas ratifiée.
