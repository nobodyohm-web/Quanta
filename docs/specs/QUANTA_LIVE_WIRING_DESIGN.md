# Conception — câblage du gadget de finalité en vivant

**Document de conception (à enregistrer, pas un /goal) · Juin 2026**
**Suit : gadget complet (GADGET-3/4/5) · migration PQ complète (PQ-MIG-5) · [[ADR-009]]**

> Le gadget tourne, **prouvé en simulation déterministe**. Il n'est **pas câblé en vivant** : les
> votes ne circulent pas par le gossip, `LatestVotes` est vide, le slashing opère sur un instantané.
> Bonne nouvelle : le fil rouge « réconciliation clé-de-vote ↔ clé-d'enjeu » est **déjà résolu** par
> PQ-MIG-3B (enjeu et vote sur la même identité ML-DSA). Ce qui reste n'est pas une réconciliation
> d'identités, c'est de l'**intégration IO** : connecter le cœur pur à la couche réseau/ledger
> réelle. C'est le chantier le plus sérieux qui reste, et il **franchit la frontière sans-IO**.

## 1. Le principe (et le risque)
- Le cœur `src/sm/` reste **pur et déterministe** (sans-IO, le sceau de tout le projet). La couche
  IO (dispatcher, gossip, ledger) **appelle** le cœur ; elle apporte le réseau et le temps réel,
  le cœur garde le verdict déterministe.
- **Risque n°1** : ne pas laisser le non-déterminisme (ordre réseau, horloge) **fuir** dans le
  verdict de consensus. C1 doit rester vrai pour le cœur ; l'IO est testée séparément.

## 2. Ce qu'il reste à câbler (4 connexions)
1. **Propager les votes** : un validateur signe son vote de finalité (ML-DSA, déjà fait) et le
   **gossipe**. Réception, dé-duplication, validation (`Vote::verify`, déjà fait).
2. **Alimenter le fork-choice + la finalité** : les votes reçus peuplent `LatestVotes` (5A) et
   `FinalityState` (3) du **ledger réel**. Le poids GHOST « s'active » dès que les votes coulent
   (le moteur est inchangé, il attend juste de vraies entrées).
3. **Slashing sur ledger réel** : une `PreuveDeFaute` (GADGET-4) gossipée déclenche, sur le ledger
   vivant, un mouvement **STAKE→BURN** réel (nouvelle tx/preuve), conservation via le bilan réel
   (pas l'instantané). Fenêtre ≤ unbonding (ADR-009).
4. **Boucle de proposition** : le proposeur utilise `ghost_head` (5A) comme tête, ancré finalité,
   pour bâtir le bloc suivant (remplace le fork-choice intérimaire dans la boucle vivante).

## 3. Découpage en pièces (chirurgical, après ce design)
- **LIVE-1** : gossip des votes de finalité (propagation + réception + validation), `LatestVotes`
  peuplé depuis le réseau. Cœur inchangé ; IO testée à part.
- **LIVE-2** : brancher `FinalityState`/`ghost_head` du ledger vivant dans la boucle de
  proposition (la tête vivante devient finalité-consciente).
- **LIVE-3** : preuve de faute gossipée → tx STAKE→BURN sur ledger réel (slashing vivant),
  conservation réelle.
- (ordre indicatif ; chaque pièce prouvée, IO et cœur testés séparément, C1 du cœur préservé.)

## 4. Limites honnêtes
- C'est de l'**ingénierie réseau sérieuse**, pas une formalité : entre « prouvé en simulation » et
  « tournant sur un vrai réseau », il y a la gestion de l'ordre des messages, des pairs malveillants
  au niveau transport, et la non-régression du déterminisme du cœur.
- Le **transport reste Ed25519** (différé, conception PQ §6) : hors chemin de valeur, à passer en PQ
  plus tard si voulu.
- Ceci **n'invente aucune règle de consensus** : tout le consensus est déjà décidé et prouvé. C'est
  purement de la **connexion** du cœur prouvé au monde réel.

> Après ce chantier, le gadget ne sera plus seulement prouvé en simulation : il **tournera**. C'est
> le pas qui fait passer Quanta de « cœur de consensus correct » à « réseau qui finalise pour de
> vrai ». Première pièce concrète : **LIVE-1** (gossip des votes).
