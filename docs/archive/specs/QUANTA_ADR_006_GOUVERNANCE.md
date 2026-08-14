# ADR-006 — Gouvernance & évolutivité (noyau immuable par construction, évolution par fork)

**Statut : proposé (à ratifier par Alexandre — décision de vision, pas technique) · Juin 2026**
**Lié à : politique d'émission · [[ADR-005 — Agrégation des votes]] (abstraction) · couche au-dessus du consensus**

> Décision de **vision**, pas d'implémentation. Elle fixe le rapport de Quanta au changement.
> **Ne construit aucun mécanisme de gouvernance maintenant** : en alpha le code est entièrement
> mutable par le fondateur ; la question ne se pose qu'après le lancement. Cet ADR n'affecte pas
> la conception du gadget de finalité (la gouvernance est une couche au-dessus du consensus).

## Contexte

Quanta vise la réserve de valeur face à Bitcoin. Question : **comment le protocole change-t-il
une fois le réseau vivant ?** Deux philosophies opposées :

- **Monnaie ossifiée** (Bitcoin) : règles dures à changer, **aucune gouvernance on-chain**,
  évolution par **fork à adoption volontaire**. Sa crédibilité monétaire **vient** de
  l'immuabilité.
- **Plateforme gouvernée** (Tezos, Cosmos, Polkadot) : vote on-chain pondéré par l'enjeu, les
  mises à jour s'appliquent **automatiquement**.

Le PoS rend la gouvernance on-chain techniquement **naturelle** (la machinerie de vote existe).
Mais pour une **monnaie**, la facilité de changement est un **passif** : elle érode la confiance
et mène à la ploutocratie (les baleines décident) plus une **surface d'attaque** de gouvernance.
La bonne réponse à « améliorable sans péril » est la **séparation entre un noyau immuable et une
périphérie gouvernable**.

## Décision

1. **Noyau immuable, par construction.** Le plafond de 100M + la loi d'émission, les signatures
   post-quantiques, les règles de conservation, la sûreté du consensus : **gravés**. L'immuabilité
   n'est **pas une serrure solide**, c'est l'**absence de porte** : aucun chemin de code ne permet
   de les changer, donc aucune gouvernance ne peut les atteindre, parce qu'il n'y a **aucun
   levier** dessus.
2. **Périphérie ajustable, derrière des abstractions propres.** Paramètres opérationnels (frais,
   certains paramètres réseau) **et** les constantes marquées du §12 (longueur d'époque, enjeu
   minimum, taille de comité). Ce sont des **réglages, pas des promesses**.
3. **Évolution par fork volontaire + développement ouvert** (le **vrai** modèle de Bitcoin),
   **pas** de gouvernance on-chain. Un changement est proposé, codé, et **chaque nœud choisit**
   de l'adopter. **Zéro surface d'attaque de gouvernance**, parce que la gouvernance vit **hors**
   du code, dans le consensus social et le choix de chacun.
4. **Aucun mécanisme de gouvernance dormant dans le code.** La préparation du futur vient de la
   **structure** (abstractions propres, ex. l'abstraction de certificat d'ADR-005), **pas** d'un
   interrupteur caché. Du code de gouvernance dormant = une **surface d'attaque dormante** (leçon
   du CRDT fantôme).

## Le principe directeur

On ne **protège** pas un invariant critique en le rendant difficile à modifier. On le rend **non
modifiable par construction**, en ne lui donnant **aucun chemin de code** de modification. C'est
plus sûr qu'une serrure : une serrure se crochète, une porte absente non.

## Conséquences

- La monnaie est **digne de confiance** sur ce qui compte (noyau monétaire immuable).
- Le projet reste **vivant et améliorable** (dev ouvert + fork volontaire) sur le reste.
- **Rien à construire maintenant** : pas de moteur de gouvernance ; en alpha le fondateur change
  le code lui-même.
- Les constantes du §12 sont explicitement des **paramètres ajustables, pas des promesses**.

## Deux sens de « améliorable par la communauté » (ne pas confondre)

- **Développement ouvert** (la communauté propose, code, contribue ; adoption par fork
  volontaire) : **sain, sans danger, déjà en place** sur GitHub. Conservé.
- **Gouvernance on-chain** (vote pondéré par l'enjeu modifiant le protocole automatiquement, en
  vivant) : le sens **risqué**, **rejeté** pour une monnaie.

## Alternatives considérées

- **Gouvernance on-chain pondérée par l'enjeu** (Tezos/Cosmos/Polkadot) : rejetée pour une
  monnaie (passif, ploutocratie, surface d'attaque). Convient aux **plateformes**, pas à la
  monnaie saine.
- **Ossification totale sans apport communautaire** : non retenue ; le développement ouvert et
  les propositions communautaires sont **gardés**, seule la gouvernance **automatique** est
  écartée.

## Ouvert / à ratifier

- C'est une décision de **vision** : le fondateur ratifie la **frontière exacte** (ce qui est
  gravé vs ajustable).
- La **liste précise** des invariants « gravés » vs « ajustables » est à finaliser (l'esquisse
  ci-dessus est un point de départ).

> Couche au-dessus du consensus : cet ADR ne bloque ni n'oriente la conception du gadget. La
> validation de la conception du gadget se poursuit indépendamment.
