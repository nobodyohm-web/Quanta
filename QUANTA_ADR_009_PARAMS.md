# ADR-009 — Frontière gravé/ajustable (ADR-006 ratifiée) et valeurs du §12

**Statut : accepté (défauts tranchés, réglables) · ratifie [[ADR-006]] · fixe les constantes §12 · Juin 2026**
**Document à enregistrer (docs/decisions/), pas un /goal.**

> ADR-006 posait le principe (noyau monétaire immuable par construction, périphérie ajustable
> derrière abstractions) mais laissait la **frontière exacte** et les **valeurs** ouvertes. Cet
> ADR les **tranche**, avec des défauts ancrés dans la littérature et l'état du code. Les valeurs
> **monétaires** existantes ne sont **pas redéfinies** ici (ce sont tes choix) ; elles sont
> **ratifiées comme gravées**. Le reste est fixé à des défauts **réglables**.

## 1. La frontière gravé/ajustable (ratification d'ADR-006)

**GRAVÉ (immuable par construction, aucun setter) — deux familles :**
- **Monétaire** : `MAX_SUPPLY_MICRO`, le calendrier d'émission (`EMISSION_DIVISOR`/courbe), le taux
  de **burn** (le 1% évoqué en ADR-006), l'unité **µQTA**, le **zéro premine** (pilier mission,
  confirmé en PQ-MIG-5 : allocation de genèse vide par défaut). Ces constantes **définissent la
  monnaie** et ne doivent jamais bouger.
- **Invariants de sûreté** : le **quorum ⅔** (`backing×3 ≥ total×2`), le fait que le **slashing
  brûle** (ne redistribue pas), la contrainte **fenêtre de preuve ≤ unbonding**, et la sûreté
  responsable (recouvrement ⅓ par pigeonnier). Abaisser l'un casse la sécurité, donc **gravé**.

**AJUSTABLE (réglage opérationnel, derrière abstractions, valeurs §12) :**
- Durées et tailles qui **règlent** performance/économie sans toucher la monnaie ni la sûreté :
  longueur d'époque, durée d'unbonding, enjeu minimum de validateur, fraction de slash (défaut
  plein). Modifiables par fork volontaire + dev ouvert (modèle Bitcoin, ADR-006), **pas** de
  gouvernance on-chain.

## 2. Les valeurs du §12 (tranchées, réglables)

| Constante | Valeur | Ancrage | Classe |
|---|---|---|---|
| `EPOCH_LENGTH_BLOCKS` (E) | **32** | Gasper/Ethereum (époque = 32 slots) ; déjà en code (GADGET-1) | ajustable |
| Quorum de finalité | **⅔** (entier) | seuil BFT ; ADR-005/GADGET-2 | **gravé** |
| `UNBONDING_PERIOD_BLOCKS` | **10080** | déjà en code (ONCHAIN-STAKE-1) ; ≥ fenêtre de slashing | ajustable (durée) |
| `SLASH_EVIDENCE_WINDOW_BLOCKS` | **= UNBONDING** | const-assert ≤ unbonding (GADGET-4) | **contrainte gravée** |
| `SLASH_NUM/DEN` | **1/1** (plein) | dissuasion maximale ; GADGET-4 | ajustable (fraction) |
| `SLASH_BURN` | **true** (brûlé) | sain monétairement ; GADGET-4 | **gravé** (brûle vs redistribue) |
| `MIN_VALIDATOR_STAKE` | **placeholder nominal 🛑** | anti-sybil ; **échelle économique à toi** (cf. §3) | ajustable |
| Allocation de genèse | **vide** (zéro premine) | pilier mission ; PQ-MIG-5 | **gravé** (principe) ; valeur réelle à toi |

**Finalité = ⅔ de l'enjeu total actif**, pas un comité échantillonné : il n'y a **pas** de
paramètre « taille de comité » dans le chemin de finalité. (Si l'élection de leader par beacon,
ADR-004, échantillonne, c'est un paramètre séparé, hors §12 finalité.)

## 3. Ce qui reste honnêtement à toi (valeurs économiques, pas structure)
- **`MIN_VALIDATOR_STAKE`** : j'ai posé un **placeholder nominal** parce que sa valeur sensée
  dépend de l'**échelle monétaire** (offre totale, valeur du µQTA) qui est ton noyau gravé, que je
  ne redéfinis pas. Fixe-le quand tu fixes l'échelle.
- **Distribution/émission réelles** : ratifiées **gravées** à leurs valeurs **actuelles en code**.
  Changer ces nombres reste ta décision (c'est la politique monétaire), mais la **frontière** (ce
  sont des constantes gravées sans setter) est tranchée.

## Conséquences
- Les constantes ci-dessus déjà en code (E, unbonding, slash, burn) sont **ratifiées**, pas
  changées. `MIN_VALIDATOR_STAKE`, si absent ou à 0, demande un petit spec pour poser le
  placeholder marqué (trivial, séparé).
- ADR-006 est désormais **opérationnel** : la frontière est nommée, pas seulement intentionnelle.
- Pas de gouvernance on-chain, pas de code dormant (ADR-006 tenu).

## Ouvert (réglages, pas blocages)
- L'échelle monétaire (offre, µQTA) et donc `MIN_VALIDATOR_STAKE` : ta décision économique, quand
  tu veux. Aucune ne bloque le câblage du gadget.
