# QUANTA — Tokenomics & Consensus v2 · « le triple verrou de la rareté »

> **✅ IMPLÉMENTÉ.** Ce document de design (issu de recherche scientifique, sources
> en bas) est désormais en place dans le code : **plafond dur 100M + émission
> décroissante** (`emission_for_tick`, `EMISSION_DIVISOR = 50_000_000`, ≈120 QUANTA/h
> à la genèse), vérifiés au consensus.
>
> Point de départ (v1) : émission **plate, sans plafond** → inflation linéaire,
> aucune rareté. Objectif : une rareté réelle et prouvable, un minage à impact réel,
> et la confiance par la transparence.

## Le problème, en une phrase
Une ressource illimitée ne peut pas prendre de valeur. BTC résout ça avec **un
seul** levier : un plafond dur (21M) + halving. On peut faire **mieux** en
combinant **trois** garanties de rareté indépendantes + un minage réellement
non-falsifiable.

---

## A. Rareté — émission décroissante vers un PLAFOND DUR (sans halving brutal)
Remplacer l'émission plate par une **récompense par bloc en décroissance
géométrique** (calcul entier en µQTA) :

```
récompense_n = floor(R₀ · r^n)          # n = numéro de bloc, 0 < r < 1
PLAFOND prouvable :  S_max = R₀ / (1 − r)   # somme géométrique finie
```

- **Rareté terminale prouvable** comme BTC (`S_max`), mais **courbe lisse** :
  pas de falaises de −50 % tous les 4 ans, économie des validateurs stable.
- **Front-loaded** : les premiers nœuds gagnent plus → incitation forte à
  **lancer le programme tôt et accumuler** (exactement « donner envie de garder
  le programme en fond »). BTC = cas particulier discret (`r=½` tous les 210k).
- QUANTA est **PoS** → pas de « security-budget cliff » comme BTC (les
  validateurs sont payés par poids de stake + frais + burn, pas seulement par la
  subvention). On peut donc décroître plus fort sans casser la sécurité.
- **Choix de marque** : fixer `R₀` et `r` pour viser un nombre-phare
  (ex. `S_max = 100 000 000 QUANTA` — c'est « le moment 21M » de QUANTA).

## B. Valeur — déflation pilotée par l'usage (plus fort que BTC)
Transformer le burn 1 % existant en **burn de base, net-destructeur** (style
EIP-1559), sur **chaque transfert** — **détruit, pas re-créé**. (Le loyer de
domaine, les tips, le marketplace et la publication ont été retirés du scope
crypto-only le 2026-06-20 ; seul le transfert existe aujourd'hui.)

```
ΔOffre = subvention(t) − burn(usage)
```

Comme `subvention(t) → 0` et que le burn persiste avec l'usage, **QUANTA devient
net-déflationniste dès que le volume dépasse la subvention (décroissante)**. La
rareté est alors **liée à l'utilité réelle** — ce que BTC n'a pas. Roughgarden
(EC'21) prouve que *brûler* (au lieu de payer le validateur) rend le mécanisme
incitatif-compatible et résistant à la collusion : le burn est une propriété de
**sécurité**, pas un gadget.

## C. Minage à impact réel & INCASSABLE
**Faille critique actuelle** : l'émission est répartie selon des **watts
auto-déclarés** → **falsifiable gratuitement** → un attaquant Sybil crée 1000
faux mineurs et rafle l'émission. À corriger en priorité.

1. **Preuve-de-Contribution témoignée par les pairs** (modèle Helium) : une
   contribution (bande passante / hébergement / calcul) n'est comptée que si
   **≥ k pairs indépendants la co-signent** (ils l'ont réellement consommée),
   avec contrôle de plausibilité + pondération par la distribution
   (`shapley.rs`, présent). Un graphe de confiance (`trust_graph.rs`) existait
   avant le refactor crypto-only du 2026-06-20 et a été retiré avec le module
   social ; s'il est repris en Phase 2, il devra être **reconstruit**. Un Sybil
   seul ne peut pas s'auto-témoigner. **Logiciel pur, pas de matériel.**
2. **Élection durcie par VDF** (Verifiable Delay Function, Boneh et al. 2018,
   en prod chez Chia) : on enveloppe la graine VRF existante dans un délai
   séquentiel non-parallélisable → tirage du leader **prouvablement
   non-grindable et infalsifiable dans le temps** = « impossible à cracker ».
3. **(Plus tard) Preuve de stockage/rétrievabilité légère** sur le contenu que
   le réseau héberge déjà (Permacoin-lite) → impact réel : le réseau héberge
   durablement et de façon vérifiable son propre contenu.

## D. Confiance absolue (ce qui fait lister par les exchanges)
- **Aucune autorité de création monétaire, aucun premine** — garanti **dans le
  code + tests**. (Le plus grand tueur de confiance, c'est une adresse qui peut
  créer des tokens.)
- **Offre prouvable en direct** : tableau de bord (émis / brûlé / en circulation
  / staké / émission nette / % du plafond) — auditable par tous.
- **Politique monétaire prévisible, gravée dans le code et publiée.** Jamais de
  changement discrétionnaire. **Rendement réel** (APY − inflation) affiché
  honnêtement.
- **À éviter absolument** (red flags) : inflation cachée, dev-mint, mécaniques de
  pump, APY > 100 %, concentration interne. Et **pas de demurrage sur la
  monnaie** (ça punirait la détention que tu veux encourager) — la taxe
  Harberger sur les domaines a été retirée avec le module domaines/site le
  2026-06-20 ; en crypto-only il n'existe **aucun** mécanisme de décroissance
  sur des ressources.

---

## Pourquoi ça surpasse BTC
| Levier de rareté | Bitcoin | QUANTA v2 |
|---|---|---|
| Plafond dur prouvable | ✅ (21M) | ✅ (`R₀/(1−r)`) |
| Déflation liée à l'usage | ❌ | ✅ (burn net EIP-1559) |
| Sécurité tardive sans falaise | ⚠️ (cliff des frais) | ✅ (PoS + frais + burn) |
| Minage à impact réel | ❌ (hash gaspillé) | ✅ (contribution témoignée) |
| Élection non-grindable | n/a | ✅ (VDF∘VRF) |
| Anti-Sybil au cœur de l'émission | ✅ (coût hash) | ✅ (témoignage pair + trust-graph) |

## Implémentation proposée — par phases (chaque phase testée)
- **Phase 1 (cœur de la rareté)** : courbe d'émission décroissante + plafond dur
  + burn net-destructeur + invariants de test (offre ≤ plafond, jamais de mint
  hors-règle) + **tableau de bord d'offre prouvable**. Migration prudente du
  ledger.
- **Phase 2 (anti-Sybil)** : Preuve-de-Contribution témoignée par les pairs
  (remplace les watts auto-déclarés).
- **Phase 3 (incassable)** : VDF sur la graine d'élection.
- **Phase 4 (impact)** : preuve de stockage légère + marketplace de calcul comme
  surcouche de récompense (hors chemin de consensus).

## Décisions à valider par le propriétaire
1. **Plafond dur cible** `S_max` (le « 21M » de QUANTA). Reco : **100 000 000
   QUANTA**.
2. **Profil de décroissance** : agressif (rareté forte, très front-loaded) vs
   doux. Reco : viser ~les 2/3 de l'offre émis dans les premières années.
3. **Go-ahead Phase 1** maintenant ?

---

### Sources clés (recherche 2026)
- Bitcoin cap/halving ; Carlsten et al. CCS 2016 (*Instability without block
  reward*) ; Budish QJE 2025 (*Economic Limits of Bitcoin*).
- EIP-1559 / « ultrasound money » ; Roughgarden, *Transaction Fee Mechanism
  Design* (arXiv:2106.01340, EC'21).
- Monero tail emission ; Peter Todd 2022 (*Tail emission n'est pas
  inflationniste* : `N(∞)=k/λ`).
- Décroissance géométrique → plafond `R₀/(1−r)` (Kaspa, courbes d'émission).
- VDF : Boneh, Bonneau, Bünz, Fisch, CRYPTO 2018 ; Chia (PoSpace+Time).
- Filecoin PoRep/PoSt ; Fisch EUROCRYPT 2019 ; Permacoin IEEE S&P 2014.
- Helium Proof-of-Coverage (témoignage par les pairs).
- Staking : *Towards an Optimal Staking Design* (arXiv:2405.14617).
