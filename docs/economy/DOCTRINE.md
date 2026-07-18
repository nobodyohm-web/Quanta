# La Constitution Économique de Quanta

> **Le Bien Commun Souverain.**
> Une valeur qui ne dépend de personne — donc enfin vraiment à toi, et à tous à la fois.

---

## Comment lire ce document

Ceci n'est ni un livre blanc ni une brochure. C'est une **charte fondatrice** : elle
grave ce qui est déjà décidé et déjà exécuté par le code, et elle pose nettement ce qui
reste à trancher.

Elle obéit à une seule règle de rigueur : **rien ici n'est une promesse en l'air.**
Chaque garantie affirmée « réelle » renvoie à une fonction, une constante ou un test qui
existe dans `src-tauri/src/`. Ce qui est un *cap* et non un *fait* est marqué comme tel,
sans maquillage. Et une contrainte tient au-dessus de tout : **QUANTA n'a aucun marché, aucun
prix.** Aucune valeur en euros n'est avancée nulle part, car il n'y en a pas. La valeur dont
parle cette doctrine n'est pas une cotation — c'est un ensemble de garanties.

Statut du projet : **alpha, non audité par un tiers.** Tout ce qui suit décrit une
architecture vérifiable en lecture de code et en simulation, pas un système éprouvé à
grande échelle ni revu par un auditeur externe.

Deux couches courent dans ce texte, et il ne faut jamais les confondre :

- 🟢 **RÉEL** — dans le code aujourd'hui, vérifiable.
- 🔵 **CAP** — la direction gravée comme intention, pas encore livrée.

---

## Préambule — la thèse en une respiration

Toute l'histoire de la monnaie est une suite de promesses **tenues par quelqu'un qui avait
le pouvoir de les rompre** : le roi qui rogne la pièce, la banque centrale qui imprime, la
plateforme qui gèle le compte. Chaque fois, la confiance reposait sur la retenue d'un
détenteur du pouvoir — et chaque fois, ce pouvoir a fini par servir.

Quanta n'ajoute pas une promesse de plus à cette liste. Elle **retire le prometteur**.

Pas « nous promettons de ne jamais imprimer » : **la fonction imprimer n'existe pas.**
Pas « nous promettons de ne pas geler vos avoirs » : **il n'y a pas de compte à geler, pas de
serveur à saisir, pas d'entreprise à sommer.** Pas « votre histoire est probablement sûre » :
**une fois finalisée, elle est mathématiquement irréversible.**

De cette abolition naît un paradoxe que le code fait tenir, et qui est le cœur de tout :

> **la souveraineté individuelle absolue** (ta clé, c'est toi ; ta part est intouchable)
> **et** **le bien commun** (la monnaie n'appartient à personne, se garde par tous, paie ses
> gardiens) — **dans le même objet.**

C'est le **Bien Commun Souverain**. Une monnaie qui appartient à tous *parce qu'elle
n'appartient à personne*, et qui te rend, toi seul, maître absolu de ta part — jusqu'au
droit de choisir la nature même de tes pièces.

---

## I. Les invariants gravés — ce qui ne changera jamais 🟢

Ce sont les fondations. Elles ne sont pas des politiques révisables : elles sont **appliquées
par chaque nœud**, qui rejette tout bloc qui les violerait. Les changer n'est pas une mise à
jour, c'est un autre protocole.

### 1. La rareté est prouvée, pas promise
Plafond **dur de 100 000 000 QUANTA** (`MAX_SUPPLY_MICRO`, `p2p/reputation.rs`). Il n'est pas
écrit sur un site : il est **vérifié au consensus** par `validate_block_emission_against`
(`p2p/ledger.rs`), qui fait rejeter tout bloc porterait l'offre au-delà du plafond — **sur le
chemin linéaire et sur les reorgs**. La rareté est une propriété du logiciel, pas une
politique de banque centrale.

### 2. Zéro premine, zéro autorité d'émission
La genèse alloue une table **vide** (testé : `trust_no_premine_at_genesis`). L'**unique**
origine de pièces dans tout le code est `mine_tx(from = "NETWORK", TxType::Mining)`
(`p2p/ledger.rs`), et son montant est plafonné par `emission_for_tick`. Aucune commande, aucune
clé, aucun rôle fondateur ne peut fabriquer un µQTA hors de la courbe — **pas même l'auteur du
projet.**

### 3. L'émission ne fait que rétrécir
```
emission_for_tick(total_miné) = (100M_µQTA − total_miné) / 50 000 000
```
Une **fonction pure** de ce qui est déjà miné (`EMISSION_DIVISOR = 50_000_000`). Décroissance
géométrique *front-loaded* : ~2 QUANTA par tick à la genèse (~120/h), puis une longue traîne
qui s'approche du plafond **en asymptote, sans jamais l'atteindre** (division entière → le
solde exact de 100M n'est jamais soldé). Chaque QUANTA se mérite ; il n'y en aura jamais
d'avalanche.

### 4. La déflation est inscrite dans l'usage
**1 % de chaque transfert est détruit** (`transfer_with_burn`, `amount / 100`, arithmétique
entière). L'usage lui-même resserre l'offre.

### 5. Les montants sont entiers
`1 QUANTA = 1 000 000 µQTA`. Toute la comptabilité est en `u64`/`u128` — **jamais un `float`
ne touche un solde** (règle Rust #6). Pas de dérive de virgule flottante, jamais.

### 6. La conservation est une loi, pas un espoir
```
Σ(dépensable + staké + en-déverrouillage) + brûlé  ==  miné
```
Gravée et testée sur tous les chemins (Stake/Unstake/Slash/reorg). Une pièce ne se crée ni ne
se perd ; elle se déplace entre compartiments.

### 7. Le passé devient irréversible
Au-dessus de l'élection PoS vit un **gadget de finalité de type Casper-FFG** (`sm/finality*.rs`,
vivant depuis LIVE-1→4). Après un **certificat de ⅔ du stake** (quorum gravé : `backing×3 ≥
total×2`), l'histoire est finalisée : `finalized_floor_index` **refuse tout reorg** sous le
plancher. Là où Bitcoin n'offre qu'une finalité *probabiliste* (« attendez six confirmations »),
Quanta offre une finalité **mathématique**.

### 8. La garde survit au quantique
L'autorité de compte, les votes de finalité et les enveloppes réseau sont signés
**ML-DSA-65 (FIPS 204)** ; l'échange de clés de transport est l'hybride **X25519MLKEM768**.
L'argent, sa finalité et sa confidentialité franchissent le saut quantique. *(Seule exception
honnête : l'identifiant de nœud réseau — le `NodeId` Iroh — reste classique Ed25519, dette
héritée en amont d'Iroh, hors de notre code. Voir §VII.)*

---

## II. La mécanique vivante — le cycle d'une pièce 🟢

Une pièce de QUANTA **naît, circule, se verrouille et meurt** — et à chaque étape, la loi de
conservation (§I.6) balance exactement.

```
        emission_for_tick (décroissante, plafonnée)
                    │
   NAÎTRE ──────────▼───────────  mine_tx(from="NETWORK")  ⟶  dépensable
      │
      │  transfert par @pseudo (signé ML-DSA)
   CIRCULER ────────────────────  −1 % brûlé à chaque saut  ⟶  offre resserrée
      │
      │  staking on-chain (déplace, ne brûle pas)
   SE VERROUILLER ──────────────  dépensable ⟶ staké ⟶ (unbonding ~2 sem) ⟶ dépensable
      │
      │  slashing d'un tricheur (preuve ML-DSA re-vérifiée par chaque nœud)
   MOURIR ──────────────────────  staké ⟶ brûlé   ·   burn d'usage  ⟶ brûlé
```

- **Naître** — seul le minage crée des pièces, dans la limite de la courbe.
- **Circuler** — on transfère à un **`@pseudo`** humain (l'identité est `BLAKE3(ADDR_DOMAIN ‖
  clé)`, `p2p/username.rs`) ; pas de compte, pas de KYC. Chaque saut brûle 1 %.
- **Se verrouiller** — staker **déplace** des pièces vers le compartiment *staké* (ne les brûle
  pas) et achète le **droit de valider** (PoS). Déverrouillage indexé par hauteur
  (`unlock = block.index + 10 080`, ~2 semaines).
- **Mourir** — par le burn d'usage, ou par **slashing** : un validateur qui triche voit son
  enjeu détruit (STAKE→BURN), sur preuve non-répudiable que **chaque nœud re-vérifie**.

---

## III. D'où vient la valeur 🟢

C'est la question que pose, à raison, quiconque regarde Quanta : *sans marché, sans prix, d'où
vient la valeur ?*

**Réponse : jamais du coût de production.** Une pièce qui aurait coûté mille kilowattheures
mais qu'on pourrait réinflater, réverser ou geler vaudrait zéro. Le coût n'est pas la valeur —
c'est une erreur d'attribution que l'on fait depuis Bitcoin.

La valeur de QUANTA **est l'ensemble des garanties qu'aucun tiers ne peut corrompre**, chacune
re-vérifiée par chaque nœud à chaque bloc :

- une **rareté** que personne ne peut diluer (§I.1–3) ;
- une **propriété** absolue — ta clé *est* toi, nulle autorité d'émission ni de saisie ;
- une **finalité** irréversible que même une majorité ne peut réécrire (§I.7) ;
- une **permanence** qui traverse le saut post-quantique (§I.8).

Ce que tu détiens, ce n'est pas la promesse d'une institution — c'est **l'absence
d'institution capable de la briser.** C'est l'exact inverse d'un billet fiat, dont la valeur
dépend entièrement de la retenue de celui qui tient la planche à billets. La valeur de Quanta,
elle, ne dépend de la retenue de personne.

---

## IV. Le doute énergétique, tranché 🟢

Une intuition fondatrice a lancé cette doctrine : *récompenser au prorata des watts consommés
ressemble au « brûler de l'électricité pour gagner » de Bitcoin — un coût sans contrepartie
établie.* L'intuition est **juste**. Voici comment le code y répond, et ce qu'il faut corriger.

**Le fait qui tranche.** `emission_for_tick` ne dépend **que** de `total_miné`. **Les watts
n'entrent nulle part dans la quantité émise.** Que le réseau brûle 3 W ou 3 MW, un tick libère
*exactement* le même nombre de pièces. Le péché structurel du PoW — « plus tu brûles, plus le
réseau émet, donc course d'armement énergétique infinie » — **est déjà absent** de Quanta. La
sécurité vient du **stake et de la finalité**, pas du hashrate ; chauffer un CPU ne rend la
chaîne ni plus sûre ni plus productive.

**Le résidu à corriger.** Il reste **un seul** endroit bancal : la clé de répartition **entre
nœuds** paie encore partiellement au prorata des watts (`W_ENERGY = 0.30` dans
`p2p/shapley.rs`, aggravé par le fait que la dimension « travail » est morte — `tasks_completed`
est câblé à `0` depuis le retrait du marketplace). Aujourd'hui, l'énergie **domine encore** le
partage réel entre nœuds. C'est un vestige, pas une intention.

**La correction (voir §VI).** On **rétrograde l'énergie** : de *base de récompense* à *simple
signal anti-sybil* — la preuve qu'une vraie machine, physiquement coûteuse à falsifier, monte
la garde, jamais un salaire. Le coût prouve la **présence** ; il n'achète pas la valeur.

---

## V. Ce que Quanta libère — l'utopie, en positif

Les invariants disent ce que *personne ne peut te faire*. Voici ce que *toi, tu peux*. Cinq
libertés, chacune une facette du Bien Commun Souverain.

### Tu gardes, donc tu crées
La valeur naît de **sécuriser** le réseau — valider, tenir la finalité, rester disponible —
jamais de brûler du courant. Un Raspberry Pi à 3 W qui garde fidèlement vaut mieux qu'un
data-center qui chauffe sans protéger. *« Le commun paie ses gardiens. »*

### Tu existes, donc tu reçois
Être un nœud vivant te donne déjà **une part du commun** — la même qu'au plus gros nœud du
monde. Pas besoin d'une ferme de GPU pour avoir droit à la monnaie du réseau. Le réseau te
dit : *« tu existes ici, donc une part du nouvel argent est déjà à toi. »*

### Tu traverses le temps
Tes pièces sont à toi **pour toujours** — et, si tu le veux, **léguables** : la mort de ta clé
cesse d'être la mort de ta valeur. Une monnaie qu'on peut transmettre comme une pierre, sans
notaire, sans autorité, et qui survit même à la cryptographie de son époque.

### Tu te meus sans permission
Aucun domicile à perquisitionner, aucun serveur à débrancher, aucun compte à geler. Tu es un
nœud souverain avec une clé, joignable par un `@pseudo` humain, sans dossier. La souveraineté
individuelle rendue **littérale et technique.**

### Ta valeur ne dépend de personne
Rareté prouvée, propriété absolue, finalité irréversible : voilà l'adossement de ta pièce —
pas une facture d'électricité, pas la parole d'un émetteur, pas une cotation. Une valeur
**autoportante.**

### Ce que ça change, en un regard
| | Ce qui tient la valeur | Ce que Quanta retire |
|---|---|---|
| **Fiat** | la retenue de la banque centrale | la planche à billets (aucun mint hors courbe) |
| **Banques** | la permission de l'intermédiaire | le compte gelable, le serveur saisissable |
| **Bitcoin** | la puissance de calcul (probabiliste) | le gaspillage énergétique **et** l'incertitude (finalité réelle) |
| **PoS classiques** | des signatures cassables au quantique | la fragilité post-quantique (ML-DSA partout) |

---

## VI. L'économie audacieuse — les mécanismes, et leur vérité

« Améliorer l'économie » ne veut pas dire retoucher des coefficients. Cinq mécanismes ont été
inventés puis passés à l'**épreuve du code** (réalisable ? déjà latent ? casse-t-il un
invariant gravé ?). Seuls ceux qui tiennent sont ici — chacun avec son statut honnête.

> **Cadre général.** Tous ces mécanismes vivent dans la **couche distribution**, aujourd'hui
> hors du chemin de sécurité du consensus (le module Shapley porte encore `#![allow(dead_code)]`
> « Phase 3 », et le seul chemin testé en réel — le minage solo — verse 100 % du tick sans même
> passer par Shapley). **Les graver, c'est de l'ingénierie à faire, pas un fait acquis.** Les
> paramètres monétaires (fractions, taux, seuils) relèvent de la ratification du fondateur (§VIII).

### L'âme choisie : permanence par défaut, circulation par volonté
Le carrefour monétaire — *or dur* (les pièces sont éternelles) contre *monnaie vivante* (les
pièces doivent circuler) — n'a pas été tranché en faveur d'un camp. Il a été **rendu
choisissable par le porteur**, ce qui est l'expression la plus pure du Bien Commun Souverain :

- **Par défaut : permanence.** Tes pièces sont à toi, indéfiniment, sans érosion.
- **Sur option : circulation.** Tu peux placer *tes* pièces dans un pool communautaire qui les
  fait couler vers les gardiens actifs. Ton or privé dort tranquille ; seul ce que tu confies
  au courant coule.

Personne ne t'impose une philosophie monétaire. **Tu choisis celle de tes propres pièces.**

---

### 🔵 Le Dividende du Commun — *la vedette réalisable*
> *Exister sur le réseau te donne déjà une part du commun — pas parce que tu payes le plus,
> mais parce que tu es là et que tu gardes.*

Chaque tick d'émission se scinde en deux poches, additivement : **le commun** (une fraction φ,
répartie en **parts égales** entre tous les gardiens vivants prouvés — un Pi et un data-center
touchent la **même** part de base) et **le mérite** (le reste, réparti par contribution
mesurée). La distribution passe d'une loi purement *multiplicative* (ta part ∝ ta puissance) à
un **plancher additif** que ni les watts ni le stake ne peuvent gonfler.

- **Pourquoi c'est audacieux** — la création monétaire elle-même porte une clause d'égalité.
  **Anti-baleine par construction**, pas par réglage : le plancher ne scale pas avec la
  puissance.
- **Ancrage réel** — s'insère pile dans `uptime_tick` (`p2p/reputation.rs`), sur le même
  `peer_contribs` déjà local, avec le même `scale_amount` (u128 virgule fixe, zéro `float`).
  L'anti-sybil qui l'endigue existe déjà (`sybil.rs::poc_score`, multiplicateur borné [0.1, 1.0]).
- **Statut honnête** — 🔵 **petit chantier, casse zéro invariant** (le plafond, le mint unique et
  la conservation restent intacts : on redistribue *qui reçoit* le tick, on ne crée pas un µQTA).
  Réserve nommée : comme Shapley aujourd'hui, le compte des nœuds présents est une vue **locale**,
  pas une fonction pure de la chaîne — donc c'est une équité **sociale**, bornée par le plafond
  consensus, **pas** une garantie de consensus. La rendre *consensus-grade* exigerait un
  `TxType::Presence` (heartbeat signé, ancré on-chain) — un chantier séparé, au roadmap, non promis.

### 🔵 Le Legs Scellé — *la permanence rendue tangible*
> *La mort n'est plus une perte : ta clé peut mourir, ta valeur passe aux vivants — selon ta
> volonté signée, sans notaire, sans autorité, post-quantique.*

Un primitif de « volonté comme code » : tu scelles un legs signé ML-DSA désignant des
bénéficiaires et un déclencheur indexé par hauteur — soit un **verrou-temps** (« débloqué au
bloc X, pour mon enfant »), soit un **dead-man's switch** (si ta clé ne signe plus depuis N
blocs, l'héritier peut réclamer, avec fenêtre de veto : une seule signature annule tout).

- **Pourquoi c'est audacieux** — le self-custody souverain avait un bug fatal : *« si je meurs,
  tout est perdu. »* Bitcoin a un cimetière (des pièces perdues à jamais). Quanta transforme le
  cimetière en jardin — **sans réintroduire le moindre intermédiaire.**
- **Ancrage réel** — clone architectural du trio `Stake`/`Unstake`/`Slash` : autorité par
  **preuve embarquée re-vérifiée par chaque nœud** (patron `verify_block_slashes`), indexation
  par hauteur (`unlock_height`) avec restauration exacte au reorg, état pur par-compte (frère
  d'`account_nonces`). Un legs **déplace** les pièces (comme le stake) : conservation neutre.
- **Statut honnête** — 🔵 le **verrou-temps** est propre et réalisable (chantier moyen). La
  **dormance** (deviner la mort depuis l'inactivité) est un chantier lourd et délicat — à livrer
  séparément, jamais à confondre avec le verrou : inférer une intention humaine n'est pas une
  preuve cryptographique.

### 🔵 Le Courant — *la circulation, en opt-in strict*
> *L'argent que tu confies au courant ne dort pas : il retourne au fleuve, re-gagné par ceux
> qui gardent le réseau éveillé.*

La forme **volontaire** de la monnaie vivante. Un porteur peut placer des pièces dans un pool
communautaire : au fil des époques, une fraction minuscule des soldes **oisifs du pool** est
prélevée (jamais brûlée) et **redistribuée aux nœuds actifs**. L'or que tu gardes hors du pool
n'est jamais touché — **la permanence de tout le monde est préservée.**

- **Pourquoi c'est audacieux** — c'est du Gesell/Wörgl porté par la cryptographie
  post-quantique : la vélocité n'est plus espérée, elle est offerte à qui la choisit. Les clés
  confiées et perdues cessent de hanter le grand livre : elles retournent aux vivants.
- **Ancrage réel** — décalque de la plomberie `Slash` (tx synthétique autorisée par preuve
  re-vérifiée, réservoir `ESCROW` déjà exempté de couverture, déclencheur de frontière d'époque
  `is_epoch_boundary`/`E=32` déjà vivant, redistribution via le chemin d'émission existant).
- **Statut honnête** — 🔵 **chantier lourd, hard-fork.** Il manque un suivi d'activité par compte
  (`last_activity_height`) qui n'existe pas aujourd'hui. **L'opt-in strict est ce qui le sauve** :
  il ne touche que les pièces explicitement confiées, donc ne trahit **jamais** la promesse de
  permanence par défaut (§VI, l'âme choisie). Les paramètres (taux, grâce) relèvent du §VIII.

### 🌟 La Paie du Sceau — *l'étoile polaire*
> *On n'est pas payé pour tourner. On est payé quand l'histoire qu'on a signée devient
> irréversible.*

Le renversement le plus pur : l'émission d'une époque est **accumulée en escrow** et n'est
mintée **que lorsqu'un certificat de ⅔ finalise** cette époque, répartie au prorata du stake
qui a **réellement signé** le certificat. Pas de certificat → pas de mint. La monnaie ne naît
plus du temps qui passe, mais du **bien commun produit** — l'irréversibilité elle-même.

- **Pourquoi c'est la direction ultime** — le mint devient physiquement impossible sans
  coopération aux ⅔ ; chaque QUANTA porte la trace des gardiens qui l'ont mérité. « Le commun
  paie ses gardiens » devient **littéral et vérifiable**. Et cela *renforce* les invariants
  (non-finalisé = non-minté ⇒ offre encore plus strictement bornée).
- **Statut honnête** — 🌟 **étoile polaire, pas prochain pas.** Chantier XL : le mint n'a pas
  encore de fonction de vérification-par-preuve (analogue à celle du slash), et il faut déplacer
  l'autorité de mint « chaque nœud se verse son tick » vers « seul un certificat autorise le
  versement » (hard-fork). Le cap est clair ; la route est longue.

*(Un cinquième mécanisme — « La Rosée », un nivellement anti-baleine par rendements
décroissants — a été écarté : il vise le même but que le Dividende du Commun, plus cher et plus
exposé au sybil. Le Dividende le subsume.)*

---

## VII. La ligne honnête

Cette doctrine ne vaut que si elle distingue scrupuleusement le réel du cap.

- **Réel aujourd'hui (🟢)** — les invariants (§I), la mécanique vivante (§II), l'origine de la
  valeur (§III), le fait que l'émission ne dépend pas des watts (§IV) : tout cela est dans le
  code, vérifiable en lecture et en test.
- **Cap, pas encore livré (🔵/🌟)** — les réinventions de distribution (§VI). La couche
  distribution est hors chemin de sécurité ; **aucun** de ces mécanismes n'est en production.
  Les graver est de l'ingénierie à faire.
- **Le résidu énergétique est présent, pas résolu** — `W_ENERGY = 0.30` pèse encore
  réellement dans le partage entre nœuds. La correction (§IV/§VI) est la direction, pas l'état.
- **Le seul primitif classique restant** — le `NodeId` réseau d'Iroh est Ed25519 (dette amont,
  hors de notre code). Un adversaire quantique pourrait usurper l'*identité réseau* d'un nœud en
  temps réel, mais **ne peut ni forger une transaction ou un vote de finalité** (ML-DSA) **ni
  déchiffrer le trafic passé** (X25519MLKEM768). On basculera le jour où Iroh livrera un
  `EndpointId` post-quantique.
- **Aucune valeur, aucun prix** — QUANTA n'a pas de marché. Rien ici ne prétend ni ne prédit
  qu'un QUANTA « vaudra » quoi que ce soit en échange. Cette doctrine parle de la valeur des
  **garanties**, jamais d'un montant.
- **Alpha, non audité** — aucune de ces garanties n'a reçu de revue de sécurité externe
  indépendante. La vérification P2P « 2 machines physiques » est réelle mais n'est pas une
  preuve à l'échelle.

---

## VIII. Les décisions ouvertes — ce que seul le fondateur tranche

L'économie de Quanta (émission, plafond, burn, enjeu minimal, et désormais les mécanismes de
distribution) relève d'une ratification unique. Cette section n'est **pas** un ensemble de
décisions — c'est leur **cadre**, posé nettement pour être tranché.

1. **La fraction du commun (φ)** — quelle part de chaque tick va au Dividende du Commun en parts
   égales, quelle part au mérite ? (Ex. 40/60.) Détermine le degré d'égalitarisme de l'émission.
2. **La séquence de livraison** — dans quel ordre grave-t-on ? Recommandation : *Dividende du
   Commun* (petit, sûr) → *Legs verrou-temps* (moyen, propre) → puis les chantiers lourds
   (*Courant* opt-in, *dormance*, *Paie du Sceau*) après observation réseau réelle.
3. **Le passage au consensus-grade** — quand rendre le Dividende vérifiable par la chaîne
   (`TxType::Presence`) plutôt que socialement local ? Hard-fork.
4. **Les paramètres du Courant** (si activé) — taux d'érosion, période de grâce : à calibrer sur
   des données réseau réelles, pas à l'aveugle.
5. **Les leviers hérités** — `MIN_VALIDATOR_STAKE` (aujourd'hui 1 QUANTA, placeholder),
   `UNBONDING_PERIOD_BLOCKS` (10 080), la forme de la traîne d'émission : ajustables par ADR-009,
   sous la contrainte gravée que la fenêtre de slashing reste couverte.

Aucune de ces décisions ne peut lever un invariant du §I. Elles se jouent **au-dessus** des
fondations, jamais contre elles.

---

## IX. La cotation — répondre aux exigences des brokers

Entrer sur Binance, Kraken ou Coinbase n'est pas une formalité : c'est un programme qui court
sur **quatre domaines** — juridique, technique, sécurité, marché. Cette doctrine ne couvre
honnêtement que le **volet économique** ; le reste est un chantier séparé, esquissé plus bas et
qui mérite son propre document (`docs/ops/LISTING-READINESS.md`, à écrire).

La bonne nouvelle, et elle est réelle : **l'économie de Quanta est un atout pour la cotation,
pas un obstacle.** C'est même l'un de ses points les plus solides.

### Ce que l'économie de Quanta apporte déjà 🟢
- **Offre vérifiable, zéro mint caché.** Le plafond de 100M et l'absence d'autorité d'émission
  sont **prouvables on-chain** (§I.1–2) : un exchange peut auditer l'offre circulante **sans
  faire confiance à personne**. La transparence de l'offre est une exigence centrale des comités
  de listing — Quanta la satisfait par construction.
- **Profil favorable au test « utility, pas security ».** Zéro premine, zéro ICO, zéro levée de
  fonds, aucune entreprise promettant un profit, distribution par **minage** : un profil proche
  de Bitcoin au regard du test de Howey (US). C'est ce qui réduit le risque « security » qui
  bloque surtout les plateformes américaines. *(Ce n'est **pas** un avis juridique — il en faudra
  un vrai ; mais la structure joue en notre faveur, pas contre.)*
- **Finalité déterministe.** Casper-FFG (§I.7) donne une réponse **nette** à la question que tout
  exchange pose — « combien de confirmations avant de créditer ? » : le plancher de finalité, pas
  une probabilité. Un vrai atout d'intégration face aux chaînes probabilistes.
- **Distribution anti-concentration.** Le Dividende du Commun (§VI) combat la concentration
  « baleine » — un critère que les comités regardent.
- **Ledger transparent, pas un privacy-coin.** Traçable de bout en bout : Quanta évite d'emblée
  la catégorie qui a valu des **délistages** aux monnaies de confidentialité.

### Les exigences hors doctrine — les vrais gaps, sans maquillage 🔵
- **Technique (le plus gros chantier de code).** Les exchanges exigent un **nœud *headless*
  (daemon) + une API/RPC stable et versionnée** : génération d'adresse, consultation de solde,
  construction et diffusion de transactions, suivi des dépôts/retraits, confirmations/finalité.
  Ils attendent aussi un **format d'adresse documenté + validation**, idéalement une **dérivation
  HD** (une graine → N adresses), le **signage hors-ligne / cold storage**, un **explorateur de
  blocs** et un **testnet**. Aujourd'hui Quanta est une **app de bureau Tauri**, pas un daemon
  RPC — **c'est le gap d'ingénierie n°1.**
- **Sécurité.** Un **audit de sécurité par un tiers reconnu** est quasi obligatoire (surtout
  Coinbase/Kraken). Quanta est explicitement **non audité** (§VII). Gap dur, non contournable par
  du code seul.
- **Juridique.** Avis juridique *security/non-security* par juridiction, et un porteur de dossier
  (entité ou sponsor). Un coin décentralisé sans émetteur **peut** être listé — Bitcoin l'a été —
  mais **quelqu'un** doit faire l'intégration et la paperasse.
- **Marché.** **Liquidité et teneurs de marché** : un exchange veut du volume et des détenteurs.
  Cela **ne se résout pas par du code** — c'est de l'adoption réelle, dans le temps.

### Conclusion honnête
« Répondre à toutes les exigences » est un **programme pluriannuel** : le volet économique est
**déjà un point fort**, le volet technique (nœud + RPC + wallets + explorateur + testnet) est un
**roadmap d'ingénierie réel et faisable**, mais l'**audit**, le **juridique** et la **liquidité**
exigent des tiers et de l'adoption — hors de portée d'un document ou d'une seule personne qui
code. La doctrine pose la fondation ; la cotation se gagne au-delà d'elle.

---

## Épilogue — pourquoi c'est révolutionnaire

Ce n'est pas une monnaie de plus. C'est le renversement d'une hypothèse vieille comme la
monnaie : que la valeur a besoin d'un **gardien** — un roi, une banque, un État, une
plateforme — quelqu'un à qui faire confiance pour ne pas trahir.

Quanta démontre, en code exécuté entre de vraies machines, que **la valeur peut tenir sans
gardien** : la rareté par les mathématiques, la propriété par la clé, l'irréversibilité par le
consensus, la permanence par la cryptographie post-quantique. Personne au sommet. Personne à
qui demander la permission. Personne capable de rompre la promesse — parce qu'il n'y a plus de
prometteur, seulement une règle que chacun fait respecter.

Et parce qu'elle n'a besoin de personne, elle peut enfin être **à tous** : un bien commun que
ses gardiens entretiennent et que l'émission rémunère ; une part offerte à quiconque existe sur
le réseau ; une valeur qui traverse le temps et qu'on lègue ; une souveraineté si complète que
tu choisis jusqu'à la nature de tes propres pièces.

C'est ce que ça inspire : l'idée qu'une monnaie n'est pas forcément un instrument de pouvoir,
mais peut être un **bien commun souverain** — libre de toute autorité, et pour cela même,
véritablement à chacun.

> *Une valeur qui ne dépend de personne — donc enfin vraiment à toi, et à tous à la fois.*

---

*Document vivant. Les affirmations 🟢 sont vérifiables dans `src-tauri/src/` à la date de
rédaction (protocole v5, `TORUS_PROTOCOL_VERSION = 5`). Les caps 🔵/🌟 sont des intentions
gravées, pas des livraisons. Statut du projet : alpha, non audité par un tiers. QUANTA n'a
aucun marché ni prix.*
