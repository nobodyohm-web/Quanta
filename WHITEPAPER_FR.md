# Quanta : une monnaie pair-à-pair post-quantique à finalité irréversible

> Statut : alpha — pas encore audité par un tiers. QUANTA n'a aucun marché ni
> prix ; aucun n'est avancé ni prédit dans ce document. Chaque constante
> ci-dessous est gravée dans le code et vérifiée par chaque nœud à chaque bloc.

**Résumé.** Une monnaie purement pair-à-pair doit survivre aux deux défaillances
que ses prédécesseurs acceptent. Les signatures à courbes elliptiques cèdent
face à un adversaire quantique : tout ce qui est signé aujourd'hui pourra être
forgé le jour où une telle machine existera. Et le règlement probabiliste cède
face à la patience : un bloc Bitcoin n'est jamais définitif, seulement
exponentiellement improbable à inverser. Nous proposons une monnaie dont
l'autorité des comptes, les votes de finalité et les enveloppes réseau sont des
signatures ML-DSA-65 (FIPS 204), dont le transport négocie un échange de clés
hybride post-quantique, et dont l'histoire devient irréversible par certificat
plutôt que par probabilité : un checkpoint portant les signatures des deux
tiers de l'enjeu inscrit est final, et finaliser une histoire concurrente
détruit de façon prouvable au moins un tiers de cet enjeu. L'offre est une
forme close : 100 000 000 de pièces, zéro préminage, émission décroissant
géométriquement vers le plafond.

---

## 1. Introduction

La monnaie électronique repose sur des promesses. Une monnaie fiat repose sur
la retenue de sa banque centrale ; un solde de plateforme repose sur un serveur
qu'on peut saisir ou geler. Les monnaies pair-à-pair ont retiré le prometteur,
mais gardé deux hypothèses plus discrètes : que les signatures à logarithme
discret ne seront jamais cassées, et qu'une histoire enfouie sous assez de
travail est suffisamment sûre.

Ces deux hypothèses ont une date de péremption. Les signatures à courbes
elliptiques sont cassées par un ordinateur quantique suffisant ; le trafic et
les clés publiques enregistrés aujourd'hui se forgent rétroactivement
(« harvest now, decrypt later »). Et l'histoire enfouie sous le travail est une
probabilité, pas un fait : l'inversion reste toujours possible, seulement
coûteuse.

Ce qu'il faut, c'est une monnaie dont les signatures résistent au quantique dès
le bloc de genèse, et dont l'histoire réglée est protégée par la preuve : la
réécrire ne doit pas être improbable — elle doit coûter une destruction
prouvable et automatique de l'argent de l'attaquant. Quanta est construite
autour de ces deux exigences. Tout le reste — le transport, le registre,
l'émission — existe pour les servir.

## 2. Pièces et signatures

Nous définissons une pièce comme une entrée d'un registre répliqué, dépensable
uniquement par une signature ML-DSA-65 vérifiée sous la clé publique engagée
dans l'adresse de l'expéditeur. Tous les montants sont des entiers en µQTA
(1 QTA = 10^6 µQTA) ; aucun flottant n'existe sur un chemin d'argent.

```
adresse            a = BLAKE3(dom_addr ‖ pk)                 pk : ML-DSA-65 (FIPS 204)
multisig (m-de-n)  a = BLAKE3(dom_msig ‖ tri(pk_1…pk_n) ‖ m)
```

L'adresse s'engage sur la clé ; la clé ne peut pas changer sans que l'adresse
change. Une adresse multisig s'engage sur une politique entière — l'ensemble
trié des clés et le seuil — que rien ne peut relier autrement une fois les
fonds arrivés. Une dépense multisig est valide si et seulement si elle porte au
moins m signatures valides distinctes sous des clés de l'ensemble engagé.

Ed25519 ne subsiste que là où l'argent n'est pas : l'identité de nœud QUIC de
la bibliothèque de transport (§9). Toute signature qui déplace de la valeur,
vote la finalité ou authentifie une enveloppe réseau est ML-DSA-65.

## 3. Offre

L'émission est une formule close de l'état de la chaîne, pas une politique.

```
E_tick  = (S_max − M) / D            S_max = 10^8 QTA,   D = 5·10^7
M_n     = S_max · (1 − (1 − 1/D)^n)  après n ticks (un tick par minute)
n_demi  = D · ln 2 ≈ 3,47·10^7 ticks ≈ 66 ans pour la moitié du restant
burn(x) = ⌊x / 100⌋                  sur chaque transfert de x µQTA
```

Chaque minute, le réseau émet E_tick — élevé au début, décroissant
géométriquement, sans jamais atteindre le plafond. Il n'y a pas de préminage :
l'état de genèse ne contient aucun solde. Il n'y a pas d'autorité d'émission :
un bloc dont l'émission dépasse E_tick est invalide pour chaque nœud.

Le registre maintient un invariant de conservation, vérifié à chaque bloc :

```
Σ_comptes (dépensable + staké + déverrouillage) + brûlé = émis ≤ S_max
```

Staker déplace des pièces entre compartiments ; le slashing les déplace vers le
brûlé ; rien ne crée ni ne perd un µQTA. Une chaîne qui viole l'équation n'est
pas une chaîne Quanta valide.

## 4. Consensus

À chaque slot (un par hauteur de bloc), un proposeur est élu parmi les
validateurs, pondéré par l'enjeu inscrit sur la chaîne elle-même — jamais par
une vue locale.

```
beacon = BLAKE3(dom_b ‖ hash(B_{h−L}) ‖ slot)      B_{h−L} : bloc L slots derrière le tip
seed   = BLAKE3(dom_s ‖ beacon ‖ slot ‖ round)
P(le validateur i propose) = s_i / S               s_i : enjeu bondé,  S = Σ s_j
```

L'élection est déterministe et publiquement vérifiable par chaque nœud depuis
la seule chaîne. Ce n'est pas un VRF : le proposeur est publiquement prévisible
un slot à l'avance (§9). Le beacon enterré interdit au proposeur tout grinding
immédiat sur son propre bloc. Si l'élu se tait 30 s, l'élection bascule au
suivant, jusqu'à trois tours ; tant que personne n'a staké, le scellement est
permissionless pour que le réseau puisse naître.

L'éligibilité est imposée à la réception, pas seulement à la production : un
nœud rejette tout bloc dont le proposeur n'était pas un validateur bondé dans
l'état parent. L'enjeu entre et sort par des transactions signées ordinaires ;
le retrait s'achève U = 10 080 blocs après sa demande (§5).

## 5. Finalité

Tous les E = 32 blocs, la frontière d'époque est un checkpoint. Les validateurs
signent des votes de finalité — des paires (source → cible) — avec les mêmes
clés ML-DSA-65 qui détiennent leur argent. Les votes s'accumulent en
certificats :

```
cert(C) valide  ⟺  3 · Σ_{v ∈ V(C)} s_v  ≥  2 · S
```

Un checkpoint certifié est justifié ; deux liens justifiés consécutifs
finalisent l'aîné. Sous le plancher finalisé, la chaîne est de pierre : chaque
nœud refuse tout fork qui remplacerait un bloc finalisé, quelle que soit sa
longueur.

**Théorème (sûreté responsable).** Si deux checkpoints en conflit sont un jour
finalisés, alors des validateurs détenant ensemble au moins S/3 ont signé des
votes contradictoires, chacun est identifié par ses propres signatures, et
chacun perd la totalité de son enjeu — bondé comme en déverrouillage.

*Esquisse.* Deux finalisations en conflit exigent deux quorums aux ⅔ ; deux
quorums aux ⅔ s'intersectent sur au moins S/3, et (comme dans Casper FFG)
chaque validateur de l'intersection a produit soit un double vote, soit un
vote enveloppant. La paire de signatures ML-DSA est elle-même la preuve : elle
est embarquée dans une transaction de slashing, re-vérifiée indépendamment par
chaque nœud, et brûle l'enjeu du fautif. La fenêtre de slashing égale la
période de déverrouillage,

```
W_slash = U = 10 080 blocs
```

quitter le corps des validateurs ne distance donc pas la punition.

Bitcoin rend la réécriture de l'histoire exponentiellement coûteuse ; Quanta la
fait payer un tiers de la monnaie, par la preuve.

## 6. Réseau

Les nœuds se connectent en QUIC et échangent neuf types de messages par gossip.
L'échange de clés du transport est l'hybride X25519MLKEM768 — un adversaire
quantique qui enregistre le trafic d'aujourd'hui n'en déchiffrera rien demain.
Chaque enveloppe est signée ML-DSA-65 et franchit neuf portes avant de toucher
le moindre état :

```
① taille ≤ 10 Mo        ② décodage              ③ expéditeur non banni
④ dédup (LRU 10^5)      ⑤ |Δt| ≤ 90 s           ⑥ débit ≤ √(pairs/4)·30/min
⑦ nonce monotone        ⑧ vérif ML-DSA          ⑨ dispatch
```

La synchronisation déplace au plus 50 blocs par requête, quatre fenêtres en
vol. Les partitions profondes guérissent par la règle que chaque nœud applique
seul : la chaîne la plus longue au-dessus du plancher finalisé gagne, les
égalités se départagent lexicographiquement — sous le plancher, rien ne bouge.

## 7. Validation

Un bloc n'admet aucune dépense non couverte. En traitant ses transactions
séquentiellement contre les soldes on-chain d'avant le bloc — crédits
intra-bloc comptés, mempool jamais consulté — chaque débit doit être couvert à
son tour. Une seule fonction l'impose partout : elle valide les blocs reçus,
filtre les blocs produits, et re-vérifie chaque bloc d'un fork candidat sur une
copie d'essai avant toute réorganisation. Un nœud ne peut ni accepter un
découvert ni en sceller un ; il ne peut pas corrompre sa propre chaîne.

## 8. Incitation

Chaque tick émet E_tick et le partage selon la contribution mesurée — énergie,
travail, validation, uptime — par valeurs de Shapley :

```
part_i = φ_i / Σ_j φ_j
```

Un nœud seul gagne le tick entier. Les récompenses sont des pièces ordinaires
sous des adresses ordinaires ; le minage est la seule émission, et le burn de
1 % sur les transferts le seul puits. Les validateurs ne sont pas payés pour
voter : ils stakent pour être élus, et perdent l'enjeu s'ils équivoquent.

## 9. Limites

Dites platement, parce que la confiance se construit sur ce qu'un système
admet.

- L'élection du proposeur est prévisible un slot à l'avance ; un VRF
  cryptographique et un VDF anti-grinding sont des travaux futurs.
- L'identité de nœud du transport (endpoint QUIC) reste Ed25519 — une
  contrainte de bibliothèque amont, hors de ce code ; elle bascule le jour où
  l'amont livre des identités post-quantiques.
- Les relevés d'énergie déclarés pondèrent une part de l'émission (§8) ; ils
  sont hors du chemin de sécurité du consensus — le poids d'un validateur est
  l'enjeu on-chain, rien d'autre — mais restent une surface de jeu économique
  à l'étude.
- Le réseau vivant est petit ; les propriétés ci-dessus sont imposées par
  chaque nœud et exercées en simulation déterministe, pas encore prouvées à
  l'échelle.
- Aucun audit externe n'a encore eu lieu. Le dossier de préparation vit dans
  `docs/audit/`.
- QUANTA n'a ni marché ni prix. Ce document ne valorise rien.

## 10. Calculs

La loterie du proposeur donne chaque slot à un attaquant de fraction d'enjeu q
avec probabilité q. Sceller une époque entière seul exige de gagner 32 slots
consécutifs :

```
q = 0,10      P = 10^−32
q = 0,30      P ≈ 2·10^−17
q = 0,45      P ≈ 8·10^−12
```

Mais la loterie n'est pas le mur. Finaliser une histoire concurrente n'est une
affaire de chance pour aucun q : il y faut les signatures certifiées des deux
tiers de l'enjeu, donc (§5) la destruction prouvable d'au moins un tiers. Sous
le plancher, l'inversion n'est pas improbable — elle a un prix, et le prix est
automatique.

## 11. Conclusion

Nous avons proposé une monnaie sans prometteur : pas d'émetteur, pas de
serveur, pas de compte à geler, et pas une signature qu'un ordinateur quantique
mette à la retraite. Les pièces sont des chaînes de signatures ML-DSA-65 sous
des adresses engagées par hachage ; l'offre est une formule close convergeant
vers un plafond dur ; les proposeurs sont élus par l'enjeu on-chain ; et
l'histoire durcit en certificats dont la violation détruit un tiers de
l'argent qui les a signés. Les règles sont peu nombreuses, et chaque nœud les
vérifie toutes.

---

*Protocole `TORUS_PROTOCOL_VERSION = 6` · Apache-2.0 · L'implémentation de
référence, sa suite de tests et sa simulation déterministe du consensus vivent
dans ce dépôt.*
