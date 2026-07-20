# Quanta : une monnaie pair-à-pair post-quantique à finalité irréversible

> Statut : alpha, pas encore audité par un tiers. QUANTA n'a aucun marché ni
> prix ; aucun n'est avancé ni prédit dans ce document. Chaque constante
> ci-dessous est gravée dans le code et vérifiée par chaque nœud à chaque bloc.

**Résumé.** Une monnaie purement pair-à-pair doit survivre aux deux
défaillances que ses prédécesseurs acceptent. Les signatures à courbes
elliptiques cèdent face à un adversaire quantique : tout ce qui est signé
aujourd'hui pourra être forgé le jour où une telle machine existera. Le
règlement probabiliste cède face à la patience : un bloc Bitcoin n'est jamais
définitif, seulement exponentiellement improbable à inverser. Quanta répond
aux deux. L'autorité des comptes, les votes de finalité et les enveloppes
réseau sont des signatures ML-DSA-65, le standard post-quantique FIPS 204 ;
le transport négocie un échange de clés hybride post-quantique ; et l'histoire
devient irréversible par certificat plutôt que par probabilité : un checkpoint
portant les signatures des deux tiers de l'enjeu inscrit est final, et
finaliser une histoire concurrente détruit de façon prouvable au moins un
tiers de cet enjeu. L'offre est une forme close : cent millions de pièces,
zéro préminage, une émission qui décroît géométriquement vers le plafond.

---

## 1. Introduction

La monnaie électronique repose sur des promesses. Une monnaie fiat repose sur
la retenue de sa banque centrale ; un solde de plateforme repose sur un serveur
qu'on peut saisir ou geler. Les monnaies pair-à-pair ont retiré le prometteur,
mais elles ont gardé deux hypothèses plus discrètes : que les signatures à
logarithme discret ne seraient jamais cassées, et qu'une histoire enfouie sous
assez de travail serait suffisamment sûre.

Ces deux hypothèses ont une date de péremption. Les signatures à courbes
elliptiques sont cassées par un ordinateur quantique suffisant, et la menace
n'attend pas la machine : le trafic et les clés publiques enregistrés
aujourd'hui pourront être exploités rétroactivement le jour venu — c'est
l'attaque dite « harvest now, decrypt later », moissonner maintenant pour
déchiffrer plus tard. Quant à l'histoire enfouie sous le travail, elle reste
une probabilité, pas un fait : l'inversion demeure toujours possible,
seulement coûteuse, et le coût se discute.

Ce qu'il faut, c'est une monnaie dont les signatures résistent au quantique
dès le bloc de genèse, et dont l'histoire réglée est protégée par la preuve.
La réécrire ne doit pas être improbable : elle doit coûter une destruction
prouvable, automatique, de l'argent de l'attaquant. Quanta est construite
autour de ces deux exigences. Tout le reste, le transport, le registre,
l'émission, l'application, existe pour les servir.

## 2. Pièces, clés et signatures

Une pièce, dans Quanta, est une entrée d'un registre répliqué par tous les
nœuds, dépensable uniquement par une signature ML-DSA-65 qui se vérifie sous
la clé publique engagée dans l'adresse de l'expéditeur. Tous les montants sont
des entiers en µQTA, où 1 QTA vaut 10^6 µQTA ; aucun nombre flottant n'existe
sur un chemin d'argent, ce qui élimine par construction toute dérive
d'arrondi entre nœuds.

```
adresse            a = BLAKE3(dom_addr ‖ pk)                 pk : ML-DSA-65 (FIPS 204)
multisig (m-de-n)  a = BLAKE3(dom_msig ‖ tri(pk_1…pk_n) ‖ m)
```

L'adresse s'engage sur la clé par hachage de domaine séparé : la clé ne peut
pas changer sans que l'adresse change, et personne ne peut substituer une clé
sous une adresse existante. Pour l'usage humain, l'adresse s'écrit au format
Bech32m sous le préfixe `qta1…`, avec somme de contrôle : une faute de frappe
est détectée avant qu'un seul µQTA ne parte. Une adresse multisig s'engage sur
une politique entière, l'ensemble trié des clés et le seuil, si bien que la
politique ne peut pas être reliée autrement une fois les fonds arrivés. Une
dépense multisig est valide si et seulement si elle porte au moins m
signatures valides de signataires distincts appartenant à l'ensemble engagé.
C'est une garde de fonds à quorum entièrement post-quantique, construite sans
attendre qu'un schéma de signature à seuil soit standardisé pour les réseaux
de treillis.

La clé de compte naît d'une graine de 32 octets que l'utilisateur sauvegarde
en phrase de récupération de 24 mots, le standard BIP39 que tout détenteur de
portefeuille connaît. Sur la machine, la graine vit dans un coffre chiffré :
la clé de chiffrement est dérivée du mot de passe par Argon2id, une fonction à
mémoire dure qui rend la force brute matérielle hors de prix, et le contenu
est scellé en AES-256-GCM, chiffrement authentifié. Sur macOS, l'empreinte
digitale peut déverrouiller le coffre : une clé d'enveloppe aléatoire vit dans
le trousseau système derrière la biométrie, le mot de passe n'est jamais
stocké et reste le recours. Tout secret est effacé de la mémoire dès qu'il a
servi.

Ed25519 ne subsiste que là où l'argent n'est pas : l'identité de nœud QUIC de
la bibliothèque de transport, une contrainte amont détaillée au paragraphe 9.
Toute signature qui déplace de la valeur, vote la finalité ou authentifie une
enveloppe réseau est ML-DSA-65.

## 3. L'offre

L'émission est une formule close de l'état de la chaîne, pas une politique
qu'un comité pourrait amender.

```
E_tick  = (S_max − M) / D            S_max = 10^8 QTA,   D = 5·10^7
M_n     = S_max · (1 − (1 − 1/D)^n)  après n ticks (un tick par minute)
n_demi  = D · ln 2 ≈ 3,47·10^7 ticks ≈ 66 ans pour la moitié du restant
burn(x) = ⌊x / 100⌋                  sur chaque transfert de x µQTA
```

La première ligne dit tout : à chaque minute, le réseau émet la fraction
1/D de ce qui reste à émettre. L'émission est donc élevée aux premiers jours,
décroît géométriquement, et n'atteint jamais le plafond : la deuxième ligne en
est la forme close, la troisième en donne l'échelle humaine, environ
soixante-six ans pour émettre la moitié de ce qui reste, quel que soit le
moment d'où l'on compte. La quatrième ligne est le seul puits : un pour cent
de chaque transfert est brûlé, ce qui rend l'offre circulante lentement
déflationniste à mesure que la monnaie sert.

Il n'y a pas de préminage : l'état de genèse ne contient aucun solde, pas même
pour l'auteur du projet. Il n'y a pas d'autorité d'émission : un bloc dont
l'émission dépasse E_tick est invalide aux yeux de chaque nœud, qui recalcule
la borne lui-même. Le registre maintient enfin un invariant de conservation,
vérifié à chaque bloc :

```
Σ_comptes (dépensable + staké + déverrouillage) + brûlé = émis ≤ S_max
```

Staker déplace des pièces entre compartiments d'un même compte ; le slashing
les déplace vers le brûlé ; rien ne crée ni ne perd un µQTA. Une chaîne qui
viole cette équation n'est pas une chaîne Quanta valide, et aucun nœud ne la
suivra.

## 4. Le consensus

À chaque slot, c'est-à-dire à chaque hauteur de bloc, un proposeur est élu
parmi les validateurs, pondéré par l'enjeu inscrit sur la chaîne elle-même,
jamais par une vue locale ou une réputation déclarée. C'est un point de
sécurité essentiel : le poids d'un validateur est une fonction pure de la
chaîne, donc identique sur chaque nœud, qu'il soit en vie depuis le premier
bloc, restauré d'une sauvegarde ou fraîchement synchronisé.

```
beacon = BLAKE3(dom_b ‖ hash(B_{h−L}) ‖ slot)      B_{h−L} : bloc L slots derrière le tip
seed   = BLAKE3(dom_s ‖ beacon ‖ slot ‖ round)
P(le validateur i propose) = s_i / S               s_i : enjeu bondé,  S = Σ s_j
```

Le beacon est dérivé d'un bloc enterré L slots derrière la pointe : un
proposeur ne peut pas remodeler son propre bloc pour influencer l'élection
qui le suit, puisque la graine de cette élection est déjà figée loin derrière
lui. L'élection est déterministe et publiquement vérifiable par chaque nœud
depuis la seule chaîne ; elle n'est pas un VRF, et le proposeur est donc
publiquement prévisible un slot à l'avance, une limite assumée au paragraphe
9. Si l'élu se tait trente secondes, l'élection bascule au suivant dans
l'ordre, jusqu'à trois tours ; et tant que personne n'a staké, le scellement
est permissionless, pour que le réseau puisse naître sans permission de
personne.

L'éligibilité est imposée à la réception, pas seulement à la production : un
nœud rejette tout bloc dont le proposeur n'était pas un validateur bondé dans
l'état parent, si bien qu'un nœud malveillant ne peut pas s'auto-introniser.
L'enjeu entre et sort par des transactions signées ordinaires, visibles par
tous ; le retrait s'achève U = 10 080 blocs après sa demande, environ deux
semaines au rythme nominal, et cette lenteur est une pièce de sécurité que le
paragraphe suivant explique.

## 5. La finalité

Tous les E = 32 blocs, la frontière d'époque est un checkpoint. Les
validateurs signent des votes de finalité, des paires source vers cible, avec
les mêmes clés ML-DSA-65 qui détiennent leur argent : voter engage la même
matière que posséder. Les votes s'accumulent en certificats :

```
cert(C) valide  ⟺  3 · Σ_{v ∈ V(C)} s_v  ≥  2 · S
```

Un checkpoint certifié est dit justifié ; deux liens justifiés consécutifs
finalisent l'aîné. Sous ce plancher finalisé, la chaîne est de pierre : chaque
nœud refuse tout fork qui remplacerait un bloc finalisé, quelle que soit sa
longueur, quel que soit son auteur. Au-dessus du plancher, les forks se
départagent par la règle la plus simple qui converge : la chaîne la plus
longue gagne, les égalités se tranchent lexicographiquement, et deux
partitions qui se retrouvent adoptent la même branche sans échanger un mot de
plus que leurs blocs.

**Théorème (sûreté responsable).** Si deux checkpoints en conflit sont un jour
finalisés, alors des validateurs détenant ensemble au moins S/3 ont signé des
votes contradictoires, chacun est identifié par ses propres signatures, et
chacun perd la totalité de son enjeu, bondé comme en déverrouillage.

*Esquisse de preuve.* Deux finalisations en conflit exigent deux quorums aux
deux tiers ; deux quorums aux deux tiers d'un même total s'intersectent sur au
moins un tiers ; et, comme dans Casper FFG, chaque validateur de
l'intersection a nécessairement produit soit un double vote, deux votes pour
la même cible de hauteur, soit un vote enveloppant, un vote qui en entoure un
autre. Dans les deux cas, la paire de signatures ML-DSA est elle-même la
preuve, non répudiable. Elle est embarquée dans une transaction de slashing
que chaque nœud re-vérifie indépendamment avant de l'appliquer, et qui brûle
l'enjeu du fautif. La fenêtre de slashing égale la période de déverrouillage,

```
W_slash = U = 10 080 blocs
```

si bien que quitter le corps des validateurs ne distance pas la punition : un
validateur reste punissable tant que son retrait n'est pas complété. C'est
pour cela que le retrait est lent.

Bitcoin rend la réécriture de l'histoire exponentiellement coûteuse ; Quanta
la fait payer un tiers de la monnaie, par la preuve.

## 6. Le réseau

Les nœuds se connectent en QUIC et s'échangent neuf types de messages par
gossip : la présence, la demande et la livraison de segments de chaîne, les
nouveaux blocs, les transactions, l'enregistrement des pseudonymes, la
vivacité et le signalement. L'échange de clés du transport est l'hybride
X25519MLKEM768, la combinaison d'une courbe classique et du standard
post-quantique ML-KEM-768 : un adversaire quantique qui enregistre le trafic
d'aujourd'hui n'en déchiffrera rien demain, et si l'un des deux mécanismes
tombait, l'autre tiendrait seul.

Chaque enveloppe est signée ML-DSA-65 et franchit neuf portes avant de toucher
le moindre état :

```
① taille ≤ 10 Mo        ② décodage              ③ expéditeur non banni
④ dédup (LRU 10^5)      ⑤ |Δt| ≤ 90 s           ⑥ débit ≤ √(pairs/4)·30/min
⑦ nonce monotone        ⑧ vérif ML-DSA          ⑨ dispatch
```

La borne de taille et la déduplication ferment les inondations ; la fraîcheur
d'horodatage et le nonce strictement croissant ferment le rejeu ; la limite de
débit s'adapte à la taille du réseau ; et rien ne se traite sans signature
valide. La synchronisation déplace au plus cinquante blocs par requête, quatre
fenêtres en vol, avec compression optionnelle. L'état complet du nœud est
photographié sur disque toutes les trente secondes : une coupure de courant
coûte au pire une demi-minute d'état local, jamais la chaîne.

Le nœud existe aussi sans interface : un daemon headless expose la même chaîne
par une API JSON-RPC de dix-sept méthodes, de quoi interroger un bloc, un
solde, la finalité, l'offre prouvée, soumettre une transaction signée ou
balayer les dépôts, ce qu'attendent un explorateur, un service ou une
plateforme d'échange. L'application de bureau et le daemon partagent le même
cœur, démarré par le même chemin de code.

## 7. La validation

Un bloc n'admet aucune dépense non couverte. En traitant ses transactions
séquentiellement contre les soldes on-chain d'avant le bloc, crédits
intra-bloc comptés, mempool jamais consulté, chaque débit doit être couvert à
son tour de passage. Une seule et même fonction impose cette règle partout :
elle valide les blocs reçus du réseau, elle filtre les blocs que le nœud
produit, si bien qu'un bloc auto-scellé passe par construction la validation
que les autres lui appliqueront, et elle re-vérifie chaque bloc d'un fork
candidat sur une copie d'essai avant toute réorganisation. Un nœud ne peut ni
accepter un découvert, ni en sceller un, ni corrompre sa propre chaîne par
une réorganisation hasardeuse ; et aucune réorganisation, jamais, ne descend
sous le plancher de finalité.

## 8. L'incitation

Chaque tick émet E_tick et le partage selon la contribution mesurée, l'énergie
engagée, le travail accompli, la validation rendue et le temps de présence,
par valeurs de Shapley, la règle de partage qui attribue à chacun sa
contribution marginale moyenne :

```
part_i = φ_i / Σ_j φ_j
```

Un nœud seul gagne le tick entier. Les récompenses sont des pièces ordinaires
sous des adresses ordinaires ; le minage est la seule émission, et le burn de
un pour cent sur les transferts le seul puits. Les validateurs ne sont pas
payés pour voter : ils stakent pour être élus proposeurs, et perdent l'enjeu
s'ils équivoquent. L'application rend ce cycle visible : elle mine en
arrière-plan, montre chaque récompense à l'instant où la chaîne l'inscrit, et
laisse envoyer et recevoir par un simple pseudonyme précédé d'un arobase,
résolu on-chain vers l'adresse de son détenteur.

## 9. Les limites

Dites platement, parce que la confiance se construit sur ce qu'un système
admet. L'élection du proposeur est prévisible un slot à l'avance ; un VRF
cryptographique, qui rendrait l'élu imprévisible jusqu'à sa révélation, et un
VDF anti-grinding sont des travaux futurs. L'identité de nœud du transport,
l'endpoint QUIC, reste Ed25519 : c'est une contrainte de la bibliothèque
amont, hors de ce code, et elle basculera le jour où l'amont livrera des
identités post-quantiques. Les relevés d'énergie déclarés pondèrent une part
de l'émission ; ils sont hors du chemin de sécurité du consensus, le poids
d'un validateur étant l'enjeu on-chain et rien d'autre, mais ils restent une
surface de jeu économique à l'étude. Le réseau vivant est petit ; les
propriétés de ce document sont imposées par chaque nœud et exercées en
simulation déterministe multi-graines, elles ne sont pas encore éprouvées à
l'échelle. Aucun audit externe n'a encore eu lieu ; le dossier de préparation
complet, modèle de menace, périmètre chiffré et engagement de publication
intégrale du rapport, vit dans `docs/audit/`. Enfin QUANTA n'a ni marché ni
prix, et ce document ne valorise rien.

## 10. Calculs

La loterie du proposeur donne chaque slot à un attaquant détenant la fraction
q de l'enjeu avec probabilité q. Sceller seul une époque entière exige de
gagner trente-deux slots consécutifs :

```
q = 0,10      P = 10^−32
q = 0,30      P ≈ 2·10^−17
q = 0,45      P ≈ 8·10^−12
```

Mais la loterie n'est pas le mur. Finaliser une histoire concurrente n'est une
affaire de chance pour aucun q : il y faut les signatures certifiées des deux
tiers de l'enjeu, donc, par le théorème du paragraphe 5, la destruction
prouvable d'au moins un tiers. Sous le plancher, l'inversion n'est pas
improbable : elle a un prix, et le prix est automatique.

## 11. Conclusion

Nous avons proposé une monnaie sans prometteur : pas d'émetteur, pas de
serveur, pas de compte à geler, et pas une signature qu'un ordinateur
quantique mette à la retraite. Les pièces sont des chaînes de signatures
ML-DSA-65 sous des adresses engagées par hachage ; l'offre est une formule
close qui converge vers un plafond dur ; les proposeurs sont élus par l'enjeu
on-chain ; et l'histoire durcit en certificats dont la violation détruit un
tiers de l'argent qui les a signés. Les règles sont peu nombreuses, et chaque
nœud les vérifie toutes.

---

*Protocole `TORUS_PROTOCOL_VERSION = 6` · Apache-2.0 · L'implémentation de
référence, sa suite de tests et sa simulation déterministe du consensus vivent
dans ce dépôt.*
