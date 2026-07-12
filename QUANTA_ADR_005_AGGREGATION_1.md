# ADR-005 — Agrégation des votes & certificats de finalité (post-quantique pur, par époque)

**Statut : accepté (à ratifier formellement par Alexandre) · Date : juin 2026**
**Lié à : [[ADR-001 — Fork-choice]] · [[ADR-003 — Slashing]] · [[ADR-004 — Aléa d'élection]]**

> Décision de conception, pas une tâche d'implémentation. Fixe le schéma de vote du gadget de
> finalité et son modèle de sécurité. La construction du gadget se fera en plusieurs specs
> **après** ratification et conception protocolaire détaillée.

## Contexte

Le gadget de finalité fait voter un comité de validateurs sur les blocs ; un quorum finalise.
Le format de ces votes posait une tension apparente :

- **BLS** agrège N votes en une signature de taille constante (certificats compacts), mais
  repose sur des couplages de courbes elliptiques, donc **n'est pas post-quantique**.
- **ML-DSA (post-quantique)** tient la promesse PQ partout, mais **ne s'agrège pas** : N votes
  = N signatures d'environ 3,3 Ko.

Cette tension se dissout sous deux observations.

1. **Un vote de finalité est éphémère, une transaction est éternelle.** Le post-quantique est
   urgent là où la signature doit rester infalsifiable des années (les **transactions**, déjà
   signées PQ). Un vote de finalité ne compte que dans la fenêtre où la finalité se décide ;
   il n'y a pas de « récolter aujourd'hui, forger demain » sur une décision jetable. Mais cela
   ne **réduit** pas l'intérêt du PQ sur les votes : ça enlève seulement l'argument du « trou
   BLS acceptable ».
2. **Le poids des certificats dépend de la granularité de finalisation.** Le coût « N × 3,3 Ko »
   n'est vrai qu'en finalisant **bloc par bloc**. En finalisant **par époque**, on produit un
   certificat par lot de blocs, pas par bloc. Cinquante validateurs ≈ 165 Ko **par époque**,
   amortis sur des dizaines de blocs et élagables. Gérable.

Conjuguées, ces deux observations rendent le PQ pur **viable et préférable** pour une chaîne
au comité modeste, sans rien céder de la promesse.

## Décision

**Agrégation post-quantique pure (ML-DSA), finalisation par époque.** Le certificat de finalité
d'une époque est l'ensemble des votes ML-DSA du comité atteignant le quorum. Pas de BLS, pas
d'ancrage, un seul système cryptographique.

Le tout derrière une **abstraction de certificat** propre, qui isole le schéma d'agrégation du
reste du gadget, pour qu'une agrégation future (BLS, SNARK) soit un remplacement local et non
une réécriture.

## Modèle de sécurité

- **Finalité entièrement post-quantique**, certificats compris. Aucune primitive classique sur
  le chemin de l'irréversibilité.
- **Aucune fenêtre** de vulnérabilité quantique à expliquer (contrairement à l'hybride
  initialement envisagé).
- **Promesse whitepaper** : « finalité post-quantique pure », **sans astérisque**. Les
  transactions **et** la finalité sont PQ de bout en bout.

## Pourquoi ce choix (et pas l'hybride)

- **Marketing** : « la seule L1 entièrement post-quantique, finalité comprise » est l'accroche
  la plus nette. L'hybride **retirait** cet argument en imposant un astérisque BLS. Le BLS est
  une optimisation d'ingénierie invisible au public, pas un argument de vente.
- **Simplicité** : un seul système crypto, pas d'ancrage, attribution de faute (slashing) plus
  simple sur des signatures séparées que sur un agrégat. Le build le moins risqué tant qu'il
  n'y a pas d'audit externe.
- **Le seul mauvais côté du PQ pur**, la **taille** des signatures (et non un coût de calcul,
  qui est négligeable), est **neutralisé par la finalisation par époque** tant que le comité
  reste modeste.

## Paramètres à fixer (🛑 décisions d'Alexandre)

- **Taille du comité** et **seuil de quorum** (tolérance BFT, ⅓).
- **Longueur d'époque** (en blocs ou en temps) : règle la fréquence des certificats et leur
  amortissement.
- **Schéma PQ** : ML-DSA, cohérent avec la signature de tx (niveau 65 probable).
- **Format du certificat d'époque** et **stratégie d'élagage** des certificats anciens.

## Alternatives considérées

- **Hybride BLS + ancrage PQ** : rejeté. À la fois **moins distinctif** (astérisque BLS sur la
  promesse) **et plus complexe** (deux systèmes crypto, ancrage, slashing plus délicat). Malin
  en apparence, moins bon pour ce projet.
- **BLS pur** : rejeté. Troue le post-quantique sur l'irréversibilité, contredit la proposition
  de valeur.
- **Agrégat PQ compressé par SNARK** : différé. Idéal à très grande échelle (PQ partout **et**
  certificats compacts) mais exige un SNARK lui-même PQ-sûr (STARK / hash-based) et un coût de
  preuve élevé. Évolution possible, pas un point de départ.

## Évolution future (différée, non bloquante)

Le BLS (ou un SNARK PQ) ne devient pertinent qu'à **très grand comité** (centaines à milliers de
validateurs), ce qu'une chaîne jeune n'a pas. L'**abstraction de certificat** garde ce chemin
ouvert : si l'échelle l'impose un jour, l'agrégation se substitue localement, sans toucher au
reste du gadget. Tant que le comité est modeste, c'est inutile.

## Conséquences

- **Débloque** la conception protocolaire du gadget, sans dépendance crypto résiduelle à
  trancher : on construit en ML-DSA dès l'étape 1.
- **Promesse whitepaper renforcée** et simplifiée (PQ pur, sans nuance).
- **Slashing** ([[ADR-003]]) : attribution de faute directe sur des votes séparés, plus simple
  que sous agrégat.
- Impose la **finalisation par époque** comme propriété structurante du gadget (pas un détail).

## Questions ouvertes

- Taille de comité, quorum, longueur d'époque, format et élagage des certificats (ci-dessus).
- Interaction avec l'aléa d'élection ([[ADR-004]]) pour la rotation du comité.
- Seuil concret de comité au-delà duquel l'agrégation redeviendrait nécessaire (à surveiller,
  pas à résoudre maintenant).

> Prochaine étape, une fois ratifié : la **conception protocolaire du gadget** (machine à états
> des votes, époques, certificats, quorum, articulation slashing et beacon), à réfléchir à la
> main avant tout découpage en specs. C'est là que commence la vraie montagne.
