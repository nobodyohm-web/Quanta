---
type: task-spec
id: ONCHAIN-STAKE-1
status: à exécuter
priorité: 🔴 soundness — sourcer l'enjeu depuis l'état de la chaîne (ferme la 2e moitié du vecteur de fork)
classe: enjeu on-chain (état d'enjeu + tx Stake/Unstake + rewire du validator-set), prérequis de GADGET-2
origine: STAKE-WEIGHT-1 §4 (enjeu encore sourcé localement) · [[ADR-002 — Validator set]]
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[ADR-003 — Slashing]] · [[DESIGN-FINALITY-GADGET]] §10 · [[AUDIT_QUANTA_2_PROGRESS]]
---

# ONCHAIN-STAKE-1 : sourcer l'enjeu depuis la chaîne (fermer le vecteur de fork pour de bon)

> STAKE-WEIGHT-1 a retiré la réputation du poids. Mais l'enjeu lui-même est **encore lu
> localement** (`build_validator_set` source l'enjeu depuis le leaderboard local), donc deux
> nœuds peuvent **encore** diverger, non plus par la réputation mais par l'enjeu. Ce spec ferme
> la **seconde moitié** : l'enjeu devient un **état de la chaîne**, identique sur tous les nœuds.
> Consensus/ledger profond, mais paramétrique. Diff logique seule, déterministe, conservation
> préservée.

## Décisions (recommandées, réglables, à ratifier par Alexandre)
- **Enjeu propre seul** : pas de délégation pour l'instant (additif plus tard).
- **Déverrouillage différé**, avec la **contrainte gravée** : `UNBONDING_PERIOD_BLOCKS ≥ fenêtre
  de slashing` (sinon le slashing est contournable). Valeur cible ~2 semaines d'équivalent-blocs,
  **réglable** (constante marquée 🛑, à figer avec le temps de bloc).
- **Enjeu minimum** = une **porte** (`MIN_VALIDATOR_STAKE`, constante marquée 🛑) : sa **valeur**
  se fixe avec la taille de comité du §12 ; le **mécanisme** se code maintenant.

## 1. État d'enjeu dans le ledger
- Par compte : montant **staké** et une liste d'entrées **en déverrouillage** `(montant,
  hauteur_de_déblocage)`. C'est un **état du ledger**, donc déterministe et identique partout.
- Le solde d'un compte se scinde en : **dépensable**, **staké** (verrouillé, pèse dans le
  consensus), **en déverrouillage** (verrouillé, ne pèse plus, pas encore dépensable).

## 2. Transaction Stake
Verrouille N pièces : **dépensable → staké**. Réduit le solde dépensable, augmente le poids de
consensus. Un compte dont le staké atteint `MIN_VALIDATOR_STAKE` devient éligible validateur.

## 3. Transaction Unstake
Lance le déverrouillage : **staké → entrée en déverrouillage** avec
`hauteur_de_déblocage = hauteur_courante + UNBONDING_PERIOD_BLOCKS`. Les fonds ne redeviennent
**dépensables** qu'une fois cette hauteur atteinte. Déblocage indexé par **hauteur de bloc**, pas
par horloge (déterministe).

## 4. Rewire de `build_validator_set` (le cœur du fix)
`build_validator_set` cesse de sourcer l'enjeu depuis le **leaderboard local** et le source
depuis l'**état d'enjeu on-chain**. Le poids d'un validateur = son **staké**, lu de la chaîne,
**déterministe et identique sur tous les nœuds**. C'est ce qui referme le vecteur de fork.

## 5. Conservation (le piège subtil à ne pas casser)
Staker ne **détruit** pas de pièces, ça les **déplace**. L'invariant de conservation
(`Σ soldes + brûlé == miné`) doit donc compter le staké et le déverrouillage comme des soldes
**verrouillés mais non détruits** :
```
Σ(dépensable + staké + en_déverrouillage) + brûlé == miné
```
Mets à jour le **vérificateur de conservation** du harnais en conséquence. Sans ça, staker
**paraîtrait** brûler des pièces et casserait la conservation. **Test** : un cycle
Stake → Unstake → déblocage **préserve** la conservation à chaque étape.

## 6. Hook de slashing (ADR-003, non implémenté ici)
Ne code **pas** le slashing. Mais pose la contrainte : `UNBONDING_PERIOD_BLOCKS ≥ fenêtre de
slashing`, et laisse l'état d'enjeu comme l'**emplacement** où le slashing réduira le staké plus
tard. **§4** : si l'articulation avec ADR-003 soulève un vrai choix, signale-le, ne tranche pas.

## 7. La propriété anti-divergence (les dents, le vrai sceau)
- **Test** : deux nœuds, **même chaîne**, **mêmes** validateurs et **mêmes poids**, **même
  comité élu**, malgré des **leaderboards locaux différents**. C'est la fermeture complète du
  vecteur de fork.
- **Non-vacuité** : prouve que l'**ancien** chemin (enjeu sourcé localement) aurait fait diverger
  deux nœuds aux leaderboards différents, et que le **nouveau** (enjeu on-chain) ne le fait plus.
  L'accord doit venir du **changement de source**, pas d'entrées identiques.

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : état d'enjeu pur, déblocage par **hauteur** (jamais horloge) ; `src/sm/`
  sans-IO préservé ; **C1 vert**.
- **Conservation** : ne **jamais** la casser ; le staké est verrouillé, pas brûlé (§5).
- **Pas de masquage** : la divergence d'enjeu est une **vraie** faille à fermer à la racine.
- **§4** : ne décide **ni** la taille de comité **ni** le quorum (§12) ; `MIN_VALIDATOR_STAKE` et
  `UNBONDING_PERIOD_BLOCKS` restent des constantes **marquées** que tu fixeras.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant : conservation sous Stake/Unstake/déblocage (§5), et la
  **propriété anti-divergence non vacueuse** (§7).
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep + conservation verts**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **ONCHAIN-STAKE-1** au tracker + auto-revue §3, avec : l'état d'enjeu, les deux tx, le
  rewire, la mise à jour de conservation, la preuve anti-divergence, et les constantes marquées.

## Séquence
1. **§1** état d'enjeu (dépensable/staké/déverrouillage).
2. **§2 + §3** tx Stake et Unstake (déblocage par hauteur).
3. **§4** rewire `build_validator_set` vers l'état on-chain.
4. **§5** mise à jour de la conservation.
5. **§7** test anti-divergence non vacueux (+ conservation §5).

> Une fois ce spec posé, le vecteur de fork est **entièrement** fermé : réputation hors du poids
> (STAKE-WEIGHT-1) **et** enjeu sourcé de la chaîne (ici). Le poids de consensus devient une
> fonction pure de l'état, identique partout, ce qui est exactement le socle dont GADGET-2 a
> besoin pour mesurer ⅔ de l'enjeu. GADGET-2 reste en attente de ta validation de conception et
> de tes décisions §12 (E, taille de comité, quorum).
