# Quanta : une monnaie pair-à-pair post-quantique à finalité irréversible

> Statut : alpha. Un audit externe a été rendu le 13 août 2026 : 85 constats,
> dont 13 critiques. Les rapports sont publiés dans `docs/audit/2026-08-13/`,
> et ce qui a été corrigé, comment on le sait et ce qui reste ouvert dans
> `docs/audit/REMEDIATION-2026-08-13.md`. QUANTA n'a aucun marché ni prix ;
> aucun n'est avancé ni prédit dans ce document. Chaque constante ci-dessous
> est lue dans le code de la version 3.16.0 (`TORUS_PROTOCOL_VERSION = 10`).
> Celles qui lient le consensus — le plafond d'offre, l'émission, le partage de
> la récompense, le quorum, les fenêtres de finalité et de déverrouillage —
> sont recalculées et imposées par chaque nœud, et pas seulement appliquées par
> l'implémentation de référence ; les bornes de transport sont des politiques
> locales, identiques dans cette implémentation mais qu'aucune règle de
> consensus n'impose.

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
des entiers en µQTA, où 1 QTA vaut 10^6 µQTA : les soldes, l'émission, le
partage de la récompense et le burn sont en arithmétique entière, et aucun
nombre flottant ne décide d'un montant, ce qui élimine par construction toute
dérive d'arrondi entre nœuds. Un flottant subsiste sur le chemin du consensus
— le relevé d'énergie qu'un bloc déclare, qui entre dans le hash d'en-tête par
son motif IEEE-754 canonique, si bien qu'aucun relais ne peut réécrire le champ
sans changer le bloc. Il ne déplace aucune pièce.

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
E_bloc  = 2 · E_tick                 un bloc est scellé tous les deux ticks
M_n     = S_max · (1 − (1 − 1/D)^n)  après n ticks (un tick par minute)
n_demi  = D · ln 2 ≈ 3,47·10^7 ticks ≈ 66 ans pour la moitié du restant
burn(x) = ⌊x / 100⌋                  sur chaque transfert de x µQTA
```

La première ligne dit tout : à chaque minute, le réseau émet la fraction
1/D de ce qui reste à émettre. L'émission est donc élevée aux premiers jours,
décroît géométriquement, et n'atteint jamais le plafond. Un bloc est scellé
tous les deux ticks et les porte tous les deux, si bien que `E_bloc` est ce que
vaut un bloc. `M_n` est la forme close de l'offre émise et `n_demi` en donne
l'échelle humaine, environ soixante-six ans pour émettre la moitié de ce qui
reste, quel que soit le moment d'où l'on compte. Le burn est le seul puits : un
pour cent de chaque transfert est détruit, ce qui rend l'offre circulante
lentement déflationniste à mesure que la monnaie sert.

Il n'y a pas de préminage : l'état de genèse ne contient aucun solde, pas même
pour l'auteur du projet. Il n'y a pas d'autorité d'émission : un bloc vaut
exactement `E_bloc`, une fonction pure de la chaîne que chaque nœud recalcule
depuis l'offre émise avant ce bloc, et un bloc qui émet davantage est invalide
à leurs yeux à tous. Un bloc peut émettre moins, ou rien du tout — un
producteur qui renonce à sa récompense est strictement non-inflationniste —
mais aucun ne peut émettre au-dessus du barème. Le registre maintient enfin un
invariant de conservation, vérifié à chaque bloc :

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
beacon = BLAKE3(dom_b ‖ hash(B_{h−L}) ‖ slot)      B_{h−L} : bloc L = 2 slots derrière
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
l'état parent, si bien qu'un nœud malveillant ne peut pas s'auto-introniser. La
règle a une ouverture assumée, cadencée par la hauteur : un bloc sur seize est
un slot ouvert que n'importe quelle adresse peut proposer, bondée ou non. Sans
elle, le premier staker fermerait le réseau à tout nouvel arrivant, puisqu'il
n'existe ni faucet ni préminage — une adresse neuve aurait besoin d'une pièce
pour staker, de staker pour proposer, et de proposer pour gagner sa première
pièce. Ce que coûte cette ouverture est dit au paragraphe 9. L'enjeu entre et
sort par des transactions signées ordinaires, visibles par tous ; le retrait
s'achève U = 10 080 blocs après sa demande, environ deux semaines au rythme
nominal, et cette lenteur est une pièce de sécurité que le paragraphe suivant
explique.

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
longueur, quel que soit son auteur. Au-dessus du plancher, la chaîne la plus
longue gagne, et à hauteur égale le départage se fait au rang d'élection
pondéré par l'enjeu des deux proposeurs — le classement même qui désigne qui
peut sceller le slot, tiré du beacon enterré, de la hauteur et de l'ensemble
bondé tel qu'il est chez le parent. Aucune de ces trois entrées ne se trouve
dans l'un des blocs concurrents : rebroyer un bloc pour obtenir un meilleur
hash n'achète donc rien, et gagner un départage exige de l'enjeu, donc quelque
chose à perdre. Le hash ne tranche que ce que le rang ne sépare pas : deux
blocs d'un même proposeur, ce qui est une équivocation punissable, et le slot
ouvert, où un proposeur sous l'enjeu minimal n'a aucun rang. Deux partitions
qui se retrouvent adoptent la même branche sans échanger un mot de plus que
leurs blocs.

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

Les nœuds se connectent en QUIC et s'échangent onze types de messages par
gossip : la présence, la demande et la livraison de segments de chaîne, les
nouveaux blocs, les transactions, l'enregistrement des pseudonymes, le ping et
le pong, le signalement d'un pair, les votes de finalité et les preuves de
faute. L'échange de clés du transport est l'hybride X25519MLKEM768, la
combinaison d'une courbe classique et du standard post-quantique ML-KEM-768 :
un adversaire quantique qui enregistre le trafic d'aujourd'hui n'en déchiffrera
rien demain, et si l'un des deux mécanismes tombait, l'autre tiendrait seul.

Chaque enveloppe est signée ML-DSA-65, et l'ordre dans lequel elle est
contrôlée est lui-même la propriété de sécurité :

```
①  taille ≤ 4 Mio                  ②  décodage JSON
③  forme des champs de tête, O(1)  ④  sonde de bannissement, en lecture
⑤  vérif ML-DSA-65 sur la pré-image canonique   ← porte d'authentification
⑥  éviction d'un ban expiré        ⑦  id = BLAKE3(cette même pré-image)
⑧  |Δt| ≤ 90 s                     ⑨  sonde de dédup (LRU 10^5)
⑩  comptabilité par pair           ⑪  insertion dans la dédup
⑫  débit ≤ 30·max(1, √(pairs/4)) par minute, plafonné à 120, puis nonce monotone
⑬  dispatch
```

L'authentification précède toute écriture. Ce qui tourne avant la signature est
en O(1) et ne mute rien : une borne de taille, un décodage, un contrôle de
forme sur des champs de tête de longueur fixe, et une sonde en lecture de la
table des bannis. Ce qui coûte du travail ou laisse une trace vient après —
l'identifiant canonique, le cache de déduplication, les compteurs par pair, la
fenêtre de débit, la borne haute du nonce — parce que tant que la signature
n'est pas vérifiée, le champ expéditeur est une chaîne choisie par l'attaquant.
Dédupliquer avant d'authentifier n'est pas une préférence de style mais une
attaque concrète : cela permet à un pair non authentifié d'installer dans le
cache de déduplication les identifiants de son choix et de censurer
gratuitement la synchronisation de chaîne d'un pair, et cela impute à des pairs
honnêtes des octets qu'ils n'ont jamais envoyés. La borne de taille et le cache
ferment les inondations ; la fraîcheur d'horodatage et le nonce strictement
croissant ferment le rejeu ; la limite de débit s'adapte à la taille du réseau,
de trente messages par pair et par minute jusqu'à un plafond de cent vingt. La
synchronisation déplace au plus cinquante blocs par requête, ou trois
mébioctets, le premier des deux atteint, quatre fenêtres en vol, avec
compression optionnelle. L'état complet du nœud est photographié sur disque
toutes les trente secondes : une coupure de courant coûte au pire une
demi-minute d'état local, jamais la chaîne.

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
sous le plancher de finalité. Une seconde borne, indépendante de la finalité,
refuse toute réorganisation de plus de 128 blocs, quel que soit son score. Le
prix est nommé plutôt que caché : au-delà de cette profondeur, une partition ne
guérit plus toute seule, et resynchroniser demande une action explicite de
l'opérateur.

## 8. L'incitation

Un bloc émet E_bloc, et ce nombre n'est pas choisi par celui qui le scelle :
c'est une fonction pure de la chaîne, recalculée par chaque récepteur depuis
l'offre émise avant le bloc. Aucune mesure locale n'entre sur le chemin
monétaire. L'énergie, le temps de présence et le travail revendiqués par un
pair sont auto-déclarés, donc invérifiables par construction ; ils ont été
retirés de l'émission et ne restent que des signaux d'affichage. Une
récompense dérivée de la seule chaîne est identique sur chaque nœud, et c'est
ce qui permet à un récepteur de la recalculer au lieu de simplement la borner.

Le partage se recalcule de la même façon, il ne se croit jamais sur parole :

```
producteur     = E_bloc/2  +  ce que la division entière laisse
participant i  = (E_bloc − E_bloc/2) · b_i / Σ_j b_j        i ≠ producteur
b_i            = blocs produits par i dans les W = 32 derniers blocs
```

La moitié va au producteur du bloc, le reste aux autres adresses ayant produit
un bloc dans les trente-deux derniers, au prorata du nombre de blocs produits
par chacune. Aucun µQTA ne se perd en route : ce que la division entière laisse
revient au producteur, si bien que le plan somme exactement à la récompense. Le
poids, ce sont les blocs, pas les adresses : les slots sont une ressource
finie, si bien que scinder une identité en K ne produit pas un bloc de plus et
ne rapporte rien de plus. Partager à parts égales entre adresses distinctes
était la règle précédente, et elle subventionnait la duplication
d'identité — vingt-huit identités captaient 45,2 % de chaque récompense là où
une seule en gagnait 12,5 %. Chaque nœud récepteur recalcule le plan entier et
rejette un bloc qui s'en écarte : un producteur ne peut donc ni garder plus que
sa moitié, ni baisser la part des autres sans baisser la sienne dans la même
proportion. Un bloc peut porter moins que la récompense entière, ou rien ; sur
une chaîne sans autre participant récent, le producteur prend tout.

Les récompenses sont des pièces ordinaires sous des adresses ordinaires ; la
production de blocs est la seule émission, et le burn de un pour cent sur les
transferts le seul puits. Les validateurs ne sont pas payés pour voter : ils
stakent pour être élus proposeurs, et perdent l'enjeu s'ils équivoquent.
L'application rend ce cycle visible : elle fait tourner le nœud en
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
identités post-quantiques. Un bloc sur seize est un slot ouvert que n'importe
quelle adresse peut proposer, bondée ou non ; un protocole ne peut pas être à
la fois sans permission, résistant aux Sybils et gratuit, donc l'entrée libre
s'achète en résistance Sybil. Le prix est borné et cadencé par la hauteur, pas
par le nombre de prétendants : une ferme d'identités capte au plus ce seizième
de l'émission, quel que soit son nombre d'identités, et jamais davantage. Les
relevés d'énergie déclarés ne pèsent rien — ils ont quitté le chemin monétaire
— mais le total réseau qu'affiche l'application reste une somme de
déclarations et doit se lire comme telle. Le réseau vivant est petit ; les
propriétés de ce document sont imposées par chaque nœud et exercées en
simulation déterministe multi-graines, elles ne sont pas encore éprouvées à
l'échelle. L'audit externe du 13 août 2026 a rendu 85 constats, dont 13
critiques, et a imposé une rupture de protocole de v9 vers v10 ainsi qu'un
rejeu de la genèse ; ses rapports sont publiés dans `docs/audit/2026-08-13/` et
la remédiation, y compris ce qui reste ouvert, dans
`docs/audit/REMEDIATION-2026-08-13.md`. Enfin QUANTA n'a ni marché ni prix, et
ce document ne valorise rien.

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

*Quanta 3.16.0 · protocole `TORUS_PROTOCOL_VERSION = 10` · chain id
`quanta-mainnet-v10` · Apache-2.0 · L'implémentation de référence, sa suite de
tests et sa simulation déterministe du consensus vivent dans ce dépôt.*
