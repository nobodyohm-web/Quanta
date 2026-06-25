---
type: task-spec
id: GADGET-2
status: à exécuter
priorité: gadget de finalité, pièce 2 (§14) — votes ML-DSA + certificat d'époque
classe: vote (attestation) + certificat super-majoritaire, derrière l'abstraction ADR-005
origine: [[DESIGN-FINALITY-GADGET]] §3, §5 · construit sur GADGET-1 (squelette) + ONCHAIN-STAKE-1 (enjeu on-chain)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[ADR-005 — Agrégation des votes]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# GADGET-2 : votes ML-DSA et certificat d'époque

> Pièce 2 du gadget. On construit les **votes** (attestations) et le **certificat d'époque**
> (le lien super-majoritaire aux ⅔), derrière l'**abstraction de certificat** d'ADR-005. Ça
> s'appuie sur le squelette époque/point de contrôle de GADGET-1 et sur l'enjeu on-chain
> d'ONCHAIN-STAKE-1. **Rien ne finalise encore** : la règle justifier/finaliser est GADGET-3.
> Ici on bâtit la matière que la règle utilisera. Diff logique seule, déterministe, C1 vert.

## Décisions prises (défauts réglables, à corriger plus tard)
- **Quorum = ⅔ de l'enjeu** staké (standard BFT, hypothèse de toute la conception).
- **Comité = l'ensemble des validateurs actifs** (`ledger.validator_stakes()`, ONCHAIN-STAKE-1),
  **sans plafond** pour l'instant (ADR-005, comité modeste). Pas de taille de comité figée.
- **E = `EPOCH_LENGTH_BLOCKS`** (valeur GADGET-1, 32), réglable §12. Ne pas la durcir ailleurs.

## 1. Le vote (attestation)
- Une structure pure `(source, cible, époque)` où `source` et `cible` sont des points de
  contrôle (GADGET-1), plus l'identité du votant. **Signée en ML-DSA** par le validateur.
- **Vérification d'un vote** : signature ML-DSA valide ; `source` et `cible` sont des points de
  contrôle ; `cible` descend de `source` ; le signataire est un **validateur actif**
  (enjeu > 0 dans `validator_stakes()`). Le **poids** du vote est son **enjeu on-chain**.
- **Déterminisme du signage** : en simulation, le vote doit emprunter le **chemin de signature
  ML-DSA déterministe** déjà en place (cfg(test), travail SIGN-DET), pour être reproductible
  dans le harnais. Jamais d'entropie hedgée en sim.

## 2. Le certificat d'époque (lien super-majoritaire), derrière l'abstraction ADR-005
- Un certificat est un **ensemble de votes pour le MÊME lien** (même `source`, même `cible`),
  dont les votants totalisent **≥ ⅔ de l'enjeu staké total**. C'est un **lien
  super-majoritaire** `source → cible`.
- Range-le **derrière l'abstraction de certificat** d'ADR-005, pour qu'une agrégation future
  (BLS, SNARK) soit un remplacement local. **Une seule** définition du concept de certificat.
- **Validation d'un certificat** : tous les votes sont valides (§1) **et** tous pour le même
  lien **et** votants **distincts** **et** Σ(enjeu des votants) ≥ ⅔ de l'enjeu total.

## 3. Déterminisme
Vérification de vote et validation de certificat sont des **fonctions pures** de (les votes,
l'état d'enjeu on-chain). Aucune horloge, aucune entropie. L'enjeu venant de la chaîne
(ONCHAIN-STAKE-1), **tous les nœuds calculent le même seuil de ⅔** et le même verdict. `src/sm/`
sans-IO préservé, **C1 vert**.

## 4. Les dents (anti-vacuité, obligatoire)
Un certificat qui ne mord pas est inutile. Tests requis :
- **quorum insuffisant** : un certificat dont les votants totalisent **< ⅔** est **rejeté**.
- **vote forgé/invalide** : un certificat contenant un vote à signature invalide est **rejeté**.
- **liens mélangés** : un certificat mêlant des votes pour des liens différents est **rejeté**.
- **double comptage** : un certificat comptant deux fois le même validateur est **rejeté**.
- **certificat valide** : un lien aux ⅔ avec votants distincts et signatures valides est
  **accepté**.
- **déterminisme** : mêmes votes + même chaîne ⇒ **même verdict** sur deux nœuds.

## 5. Rien ne finalise ici
Ce spec **ne touche pas** l'ensemble finalisé ni l'invariant FinalitySafety de GADGET-1 (qui
reste donc encore quasi vacueux). Il **ne code pas** la règle justifier/finaliser : c'est
GADGET-3 qui consommera ces certificats. Ne pas anticiper la règle.

## Garde-fous
- **Une seule source de vérité** : le certificat vit derrière l'**abstraction ADR-005**, pas en
  double.
- **Réutiliser** le squelette `sm/finality.rs` (GADGET-1) et `ledger.validator_stakes()`
  (ONCHAIN-STAKE-1) ; ne pas réimplémenter les points de contrôle ni l'enjeu.
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : pur, signage ML-DSA déterministe en sim ; `src/sm/` sans-IO ; **C1 vert**.
- **Pas de masquage** : les dents du §4 doivent réellement mordre.
- **§4** : ne décide **pas** la taille de comité ni le plafond (ADR-005, §12) ; si un vrai choix
  émerge (échantillonnage du comité, format exact du certificat), signale-le.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les 6 tests du §4.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact · FinalitySafety **inchangé**.
- Entrée **GADGET-2** au tracker + auto-revue §3, avec : la structure de vote, la vérification,
  le certificat derrière l'abstraction, les dents du quorum, et les défauts marqués (⅔, comité =
  validateurs actifs, E).

## Séquence
1. **§1** structure et vérification du vote (ML-DSA, poids = enjeu on-chain).
2. **§2** certificat d'époque (lien super-majoritaire ⅔) derrière l'abstraction ADR-005.
3. **§4** les dents : quorum insuffisant, vote forgé, liens mélangés, double comptage,
   certificat valide, déterminisme.

> Pièce suivante, GADGET-3 : la **règle justifier/finaliser** en deux temps, qui consomme ces
> certificats et où la finalité devient **réelle** (l'invariant FinalitySafety cesse alors d'être
> vacueux). Puis GADGET-4 (slashing, les deux conditions) et GADGET-5 (fork-choice conscient de
> la finalité, le « à corriger plus tard » assumé). Les valeurs finales du §12 restent réglables.
