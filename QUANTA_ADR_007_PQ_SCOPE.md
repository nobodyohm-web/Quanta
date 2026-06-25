# ADR-007 — Portée du post-quantique : comptes en ML-DSA (Quanta entièrement post-quantique)

**Statut : proposé — recommandation forte (b), à ratifier · décision de vision fondatrice, à Alexandre · Juin 2026**
**Lié à : [[ADR-005 — Agrégation des votes]] · CRYPTO-ID-1 (audit) · bloque GADGET-3**

> **La décision la plus structurante du projet.** L'ingénierie recommande (b) sans réserve ;
> l'**engagement** au coût de (b) appartient au fondateur, parce qu'il dépend de son horizon et
> de ses ressources, pas de la seule technique. Cet ADR grave la recommandation ; la
> **ratification** d'Alexandre est le déclencheur. **Aucun code de migration n'est écrit avant
> ratification**, et la migration se **conçoit** ensuite (comme le gadget), elle ne se balance
> pas en un jet.

## Contexte

L'audit **CRYPTO-ID-1** a révélé, preuves à l'appui, que la promesse « entièrement
post-quantique » **n'est pas tenue dans le code**. Comptes, autorisation de transaction, enjeu et
transport sont **enracinés Ed25519**.

Le piège subtil : la signature est **hybride** (Ed25519 + ML-DSA), mais la **racine de confiance**
d'un compte reste Ed25519, pour deux raisons. `REQUIRE_PQ = false` autorise un repli Ed25519
seul. Et la clé ML-DSA est **auto-déclarée par transaction** et **jamais liée** au compte
(`verify_hybrid` vérifie `ed_ok && pq_ok` sans contrôler l'appartenance). Donc un adversaire
quantique qui casse Ed25519 forge la signature classique de `from`, **attache sa propre clé
ML-DSA**, et passe la vérification. La couche PQ **ne protège pas** les comptes. Parler de
« comptes post-quantiques » aujourd'hui serait du mensonge.

Découvert **avant la genèse**, donc au bon moment. La question : **Quanta est-elle réellement
entièrement post-quantique ?**

## Les deux chemins

- **(a) Comptes Ed25519 + finalité PQ seulement**, via un registre on-chain liant compte → clé
  ML-DSA de finalité. **Léger.** Mais les **comptes restent vulnérables au quantique** : la
  promesse « entièrement PQ » garde un **astérisque permanent**, sur exactement le
  différenciateur du projet.
- **(b) Comptes entièrement en ML-DSA.** Enjeu et finalité coïncident **nativement**. **Lourd** :
  re-raciner `CryptoEngine`/le vault, adresses hachées en BLAKE3 (une clé ML-DSA fait ~1952 o),
  refaire `verify_tx`, le builder, `validator_stakes`, le registre `@pseudo`, migrer les soldes
  existants, et un bump de `TORUS_PROTOCOL_VERSION`. La promesse **sans astérisque**.

## Décision (recommandée, à ratifier) : (b), tout en ML-DSA

Pour une monnaie dont la **raison d'être** est le post-quantique, l'astérisque de (a) n'est pas
un compromis, c'est un **renoncement** à la proposition de valeur, sur la chose même qui
distingue Quanta. Un auditeur qui trouverait ce que CRYPTO-ID-1 a trouvé ferait s'effondrer la
revendication. Et une signature de transaction est de **longue vie** : si elle est forgeable par
un futur ordinateur quantique, des fonds réels sont volables (« récolter aujourd'hui, forger
demain »). (a) construit une monnaie **qui n'est pas ce qu'elle prétend être**.

## Le coût, sans le minimiser

(b) est vraisemblablement le **plus gros chantier** du projet, plus gros que le gadget : il
re-racine la fondation cryptographique. Il **doit se concevoir** (un document de conception, puis
des specs chirurgicaux), comme on l'a fait pour le gadget. Il ne se balance pas en un jet.

## Ce qui appartient au fondateur (la part §4 / vision)

L'**engagement** à payer ce coût, contre l'alternative d'expédier plus vite avec l'astérisque de
(a), dépend de l'**horizon, des ressources et de la tolérance** d'Alexandre, que la
recommandation d'ingénierie ne peut pas trancher. Cet ADR fixe la **recommandation** ((b)) ; le
fondateur **ratifie l'engagement**.

## Conséquences

- **(b)** : Quanta devient **réellement** entièrement post-quantique. Re-architecture lourde,
  lancement retardé, mais la proposition de valeur tient **sans astérisque**. Débloque GADGET-3
  (enjeu et finalité partagent **nativement** la même identité).
- `REQUIRE_PQ` doit passer à **true** ; le repli Ed25519 est **retiré** ; la clé PQ auto-déclarée
  par transaction est remplacée par l'**identité ML-DSA du compte**.
- La migration **mérite son propre document de conception**, puis des specs chirurgicaux.

## Alternatives considérées

- **(a) registre** : rejetée **comme recommandation**. Astérisque permanent sur la revendication
  centrale ; comptes forgeables par un adversaire quantique. Acceptable **seulement** si le
  fondateur choisit **consciemment** un v1 plus rapide avec une couche de comptes non-PQ, ce qui
  contredit la prémisse du projet.
- **Statu quo** (hybride mais enraciné Ed25519) : rejeté, c'est l'état de **fausse promesse** que
  CRYPTO-ID-1 a exposé.

## Ouvert / à ratifier

- **L'engagement du fondateur à (b)** (le déclencheur).
- Ensuite : un **document de conception** de la migration (stratégie de re-racinage, schéma
  d'adresses, migration des soldes, version de protocole), **puis** des specs chirurgicaux.

> Cet ADR **bloque GADGET-3** (qui a besoin de l'identité autoritaire tranchée). C'est la
> décision la plus fondatrice qui reste, et elle vaut d'être prise à tête reposée.
