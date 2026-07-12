# ADR-005 — Agrégation des votes & certificats de finalité (hybride BLS + ancrage PQ)

**Statut : proposé (en attente de ratification d'Alexandre) · Date : juin 2026**
**Lié à : [[ADR-001 — Fork-choice]] · [[ADR-003 — Slashing]] · [[ADR-004 — Aléa d'élection]]**

> Ce document est une **décision de conception**, pas une tâche d'implémentation. Il fixe le
> schéma d'agrégation des votes du gadget de finalité, son modèle de sécurité et ses
> paramètres ouverts. La construction du gadget se fera en plusieurs specs **après** cette
> décision et sa conception protocolaire détaillée.

## Contexte

Le gadget de finalité fait voter un comité de validateurs sur les blocs ; un quorum finalise.
Le format de ces votes pose une tension irréductible :

- **BLS** agrège N votes en **une** signature de taille constante (certificats minuscules
  quel que soit le comité), mais repose sur des couplages de courbes elliptiques, donc **n'est
  pas post-quantique**.
- **ML-DSA (post-quantique)** tient la promesse PQ partout, mais **ne s'agrège pas** : N votes
  = N signatures d'environ 3,3 Ko, soit des certificats de centaines de Ko par bloc finalisé à
  comité de taille moyenne, croissant avec le comité.

La menace quantique est un risque de **long terme** (aucun adversaire quantique
cryptographiquement pertinent aujourd'hui), ce qui ouvre un compromis temporel.

## Décision

Adopter un schéma **hybride** : **agrégation BLS à chaque bloc** pour une finalité rapide et
des certificats compacts, plus un **ancrage post-quantique périodique** tous les K blocs (par
époque) qui re-scelle l'histoire en PQ.

## Modèle de sécurité (à assumer explicitement)

- **Finalité récente** (depuis la dernière ancre) : sécurisée par BLS, donc **classique**.
  Rapide, certificats minuscules.
- **Finalité profonde** (au-delà de la dernière ancre) : sécurisée en **PQ**. L'irréversibilité
  de long terme, celle qui compte pour une réserve de valeur, est protégée contre un adversaire
  quantique.
- **Conséquence à assumer** : un adversaire quantique **futur** pourrait forger la finalité
  **dans la fenêtre inter-ancrage**, mais **pas** l'histoire ancrée. Le paramètre K **borne**
  cette fenêtre.
- **Honnêteté whitepaper** : ce n'est pas « finalité PQ pure », c'est « **finalité
  PQ-ancrée** ». La section finalité du whitepaper devra le formuler ainsi. Les transactions
  restent signées PQ de bout en bout ; c'est l'agrégation de la **finalité récente** qui est
  classique.

Ce compromis est cohérent avec le calendrier de la menace : on protège en PQ ce qui doit
durer, et on laisse la finalité rapide du moment en BLS, puisque aucun adversaire quantique
n'existe au moment où cette finalité récente est vivante.

## Séquencement d'implémentation (recommandé)

Ne pas construire les deux systèmes crypto d'un coup.

1. **Étape 1, ligne de base** : gadget avec un **comité restreint en PQ direct**, sans
   agrégation. Le plus simple, la valeur-prop PQ intacte, des certificats acceptables tant que
   le comité reste petit. Cela fait **vivre** le gadget et valide sa logique (le test 2b
   gadget-deferred du harnais en est déjà la cible d'acceptation).
2. **Étape 2, optimisation** : ajouter l'**agrégation BLS + l'ancrage PQ** quand la taille du
   comité rend les certificats PQ directs trop lourds. Même destination, risque maîtrisé.

Ainsi la complexité de l'hybride n'arrive que quand elle est nécessaire, pas avant.

## Paramètres à fixer (🛑 décisions d'Alexandre)

- **K**, l'intervalle d'ancrage PQ (en blocs ou en temps). Plus petit = fenêtre quantique plus
  étroite, mais ancres plus fréquentes et plus coûteuses. Compromis sécurité/coût.
- **Taille du comité** et **seuil de quorum** (lié à la tolérance aux fautes byzantines, ⅓).
- **Format du certificat** : agrégat BLS + clés agrégées pour la finalité récente ; structure
  de l'ancre PQ pour l'époque.
- **Courbe BLS** (par ex. BLS12-381) et schéma PQ d'ancrage (ML-DSA, cohérent avec la signature
  de tx).

## Alternatives considérées

- **BLS pur** : rejeté. Troue le post-quantique sur l'opération la plus critique,
  l'irréversibilité, ce qui contredit la proposition de valeur du projet.
- **PQ pur** : viable et le plus simple en sécurité, mais certificats lourds à grand comité.
  Retenu comme **ligne de base** de l'étape 1, pas comme cible finale à l'échelle.
- **Agrégat PQ compressé par SNARK** : idéal en théorie (PQ partout **et** certificats
  compacts), mais exige un SNARK lui-même PQ-sûr (STARK / hash-based, sinon on réintroduit le
  trou quantique) et un coût de preuve élevé à prouver la vérification ML-DSA en circuit.
  Chantier majeur, **différé** comme évolution possible de l'étape 2.

## Conséquences

- **Débloque** la conception protocolaire du gadget de finalité (le gros morceau restant).
- Impose, **à terme**, deux systèmes cryptographiques (BLS + PQ) et leur interaction.
- **Nuance** la promesse de finalité, à refléter honnêtement dans le whitepaper.
- Le slashing ([[ADR-003]]) devra s'articuler avec les votes agrégés (attribuer une faute dans
  un agrégat BLS est plus délicat que sur des signatures séparées) : à traiter à la conception
  du gadget.

## Questions ouvertes

- Valeurs de K, taille de comité, seuil de quorum, format de certificat (ci-dessus).
- Attribution de faute (slashing) sous agrégation BLS.
- Interaction avec l'aléa d'élection ([[ADR-004]]) pour la rotation du comité.
- Stratégie de purge/archivage des certificats anciens (les ancres PQ suffisent-elles à élaguer
  les agrégats BLS intermédiaires ?).

> Prochaine étape, une fois cet ADR ratifié : la **conception protocolaire du gadget**
> (machine à états des votes, époques, ancrage, certificats), qui se découpera ensuite en
> specs chirurgicaux. C'est là que commence la vraie montagne.
