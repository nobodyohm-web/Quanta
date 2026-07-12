# Conception du gadget de finalité Quanta — style Casper FFG, post-quantique, par époque

**Document de conception (à valider, pas encore du code) · Juin 2026**
**Relie : [[ADR-001 — Fork-choice]] · [[ADR-002 — Validator set]] · [[ADR-003 — Slashing]] · [[ADR-004 — Aléa d'élection]] · [[ADR-005 — Agrégation des votes]]**

> Ceci est l'orfèvrerie : le protocole, raisonné de bout en bout, **avant** toute ligne de
> code. Il s'ancre sur **Casper FFG** (le gadget de finalité d'Ethereum, à sûreté responsable
> démontrée), adapté à Quanta : votes ML-DSA, finalisation par époque, et surtout **vérifié par
> le harnais DST existant**. Les arguments de sûreté et de vivacité ci-dessous sont des
> **esquisses rigoureuses**, pas des preuves formelles : la formalisation et l'audit externe
> restent devant. C'est l'architecture de départ, solide et cohérente, pas un théorème clos.

## 1. Ce sur quoi on construit

Quanta a déjà : une chaîne linéaire de blocs, une production de blocs en preuve d'enjeu avec
leader élu via le beacon ([[ADR-004]]), une validation partagée `validate_block_against_prev`,
et un harnais de simulation déterministe qui vérifie sûreté, conservation et émission à travers
des centaines de scénarios fautés. Le gadget se **superpose** à cette chaîne : il ne remplace
pas la production de blocs, il y ajoute une couche de **finalité déterministe**.

L'idée centrale de Casper, qui colle parfaitement à ta décision ADR-005, est que la finalité
opère sur des **points de contrôle** aux **frontières d'époque**, pas bloc par bloc. C'est ce
qui rend les certificats post-quantiques gérables.

## 2. Vocabulaire : époques et points de contrôle

La chaîne est découpée en **époques** de E blocs. La frontière de chaque époque est un **point
de contrôle** (checkpoint), identifié par (hauteur de frontière, hash du bloc). Le bloc de
genèse est le point de contrôle initial, **finalisé par définition**.

Tout le mécanisme de finalité raisonne sur la suite des points de contrôle, un objet bien plus
petit que la suite des blocs.

## 3. Les votes (attestations)

À chaque époque, chaque validateur du comité émet **un** vote, qui est un **lien** entre deux
points de contrôle :

```
vote = (source, cible, époque)   signé en ML-DSA, pondéré par l'enjeu du validateur
```

où `source` est un point de contrôle **déjà justifié** (voir §4) et `cible` est un point de
contrôle descendant. Le vote dit : « depuis cette histoire que je tiens pour acquise, je
soutiens cette nouvelle frontière ». La signature ML-DSA rend le vote **post-quantique** et,
crucialement, en fait une **preuve** : un vote signé est une attestation non répudiable, ce qui
fonde le slashing du §7.

## 4. Justification et finalisation (la règle en deux temps)

Deux seuils, tous deux à **deux tiers de l'enjeu** (le quorum BFT classique, qui tolère moins
d'un tiers de fautifs) :

- Un **lien super-majoritaire** `source → cible` existe quand des validateurs totalisant **≥ ⅔
  de l'enjeu** ont voté ce même lien.
- Un point de contrôle `cible` devient **justifié** s'il existe un lien super-majoritaire
  depuis un point de contrôle **déjà justifié** vers lui (la genèse est justifiée d'office).
- Un point de contrôle `c` devient **finalisé** quand `c` est justifié **et** qu'il existe un
  lien super-majoritaire de `c` vers son **enfant direct** (le point de contrôle de l'époque
  suivante). Autrement dit : **deux époques consécutives correctement liées** scellent la
  première. Finalisé signifie **irréversible**.

Cette règle en deux temps est le cœur de Casper. Elle est simple, et c'est sa simplicité qui
rend la sûreté démontrable.

## 5. Le certificat d'époque (post-quantique, ADR-005)

Le **certificat de finalité** d'une époque est l'**ensemble des votes ML-DSA** formant le lien
super-majoritaire qui justifie la frontière. **Un** certificat par époque, pas par bloc, donc
de l'ordre de 165 Ko pour cinquante validateurs, amorti sur E blocs et élagable une fois
l'époque profondément finalisée. Pas de BLS, pas d'ancrage, un seul système cryptographique,
exactement comme tranché. Le certificat est rangé derrière l'**abstraction de certificat** de
l'ADR-005, pour qu'une agrégation future reste un remplacement local.

## 6. Le fil rouge avec la production de blocs

La production de blocs (leader élu, chaîne linéaire) **continue** sans attendre la finalité :
elle donne la **vivacité** et le débit. Le gadget tourne **derrière**, à un rythme d'époque, et
**scelle** rétroactivement l'histoire. Tu obtiens donc deux régimes : une tête de chaîne
rapide mais encore probabiliste, et une histoire finalisée **absolue** derrière elle. C'est
exactement la séparation « finalité récente / finalité profonde », sauf qu'ici les deux sont
post-quantiques.

## 7. Sûreté responsable (le théorème, et le lien avec le slashing ADR-003)

C'est la propriété qui rend Casper élégant, et c'est elle qui impressionne un ingénieur
consensus. Deux **conditions de slashing**, et deux seulement, suffisent :

1. **Pas de double vote** : un validateur ne signe pas deux votes différents pour la **même
   époque cible**.
2. **Pas de vote enveloppant** : un validateur ne signe pas un vote dont l'intervalle
   (source, cible) en **entoure** un autre qu'il a déjà signé (source antérieure et cible
   postérieure).

**Théorème de sûreté responsable** (esquisse) : si deux points de contrôle **en conflit** sont
tous deux finalisés, alors des validateurs totalisant **au moins ⅓ de l'enjeu** ont
nécessairement violé la condition 1 ou la condition 2. Comme chaque vote est une signature
ML-DSA non répudiable, la violation est **prouvable** et donc **slashable**.

Conséquence forte : on ne peut pas casser la finalité **sans** qu'au moins un tiers de l'enjeu
soit détruisible par preuve. La sûreté n'est pas seulement « difficile à violer », elle est
**responsable** : toute violation laisse une preuve cryptographique de qui l'a commise. Voilà
ce qui donne corps à [[ADR-003]] : le slashing n'est pas un ajout, il **découle** des deux
conditions, et l'attribution de faute est directe parce que les votes sont séparés (un des
gains du PQ pur sur l'agrégat, noté en ADR-005).

## 8. Vivacité

Sous les hypothèses BFT usuelles (synchronie réseau après un délai, et **plus de ⅔ de l'enjeu
honnête et en ligne**), de nouveaux points de contrôle continuent d'être justifiés puis
finalisés : le gadget **progresse**.

Et la propriété duale, la **vivacité plausible** : depuis n'importe quel état atteignable, il
est **toujours possible** de finaliser un nouveau point de contrôle **sans** qu'aucun validateur
ait à violer une condition de slashing. Autrement dit, le protocole ne peut pas se peindre dans
un coin où finaliser exigerait de se faire slasher. C'est ce qui garantit qu'on n'est jamais
bloqué de façon irrécupérable.

Honnêteté : la vivacité est la partie **subtile**, et elle dépend de la règle de fork-choice du
§9 et des hypothèses de synchronie. C'est là que vit la vraie difficulté de la construction, et
c'est là qu'il faudra le plus de soin et, à terme, de formalisation.

## 9. Interaction avec le fork-choice (ADR-001) — et la résolution du trou de partition

C'est le passage le plus élégant, parce qu'il **referme un trou** que je te signale depuis des
sessions.

La règle de fork-choice devient **consciente de la finalité** : un nœud construit toujours sur
la branche qui contient le **dernier point de contrôle justifié** le plus récent, et **ne
revient jamais** sur un point de contrôle **finalisé**. La finalité est un plancher absolu.

Or souviens-toi du problème laissé ouvert : le fork-choice intérimaire ne savait départager
qu'un seul bloc au même index, pas réconcilier deux chaînes concurrentes de plusieurs blocs
après une longue partition. **Le gadget le résout.** Quand deux partitions guérissent :

- toute histoire **finalisée** de part et d'autre est irréversible, et par la sûreté du §7 il ne
  peut pas y avoir deux points de contrôle finalisés en conflit (sinon ⅓ est slashable), donc
  les histoires finalisées **coïncident** ;
- au-dessus de la finalité, les branches concurrentes se départagent par la règle de
  fork-choice, en suivant le **point de contrôle justifié** le plus récent.

C'est **exactement** la cible d'acceptation que ton harnais attend déjà : le test 2b
gadget-deferred, qui aujourd'hui asserte une divergence de sûreté, **bascule** pour asserter la
**convergence** une fois le gadget en place. Et l'exigence que je t'avais laissée pour ce
moment précis se code ici : la conservation **globale** au heal, c'est-à-dire défaire
proprement l'émission de toute branche perdante non finalisée, pour ne pas rouvrir la classe
double-mint à l'échelle de la partition.

## 10. Comité et rotation (ADR-002, ADR-004)

Le comité de validateurs est pondéré par l'**enjeu inscrit sur la chaîne** ([[ADR-002]], enjeu
seul, la réputation ayant été retirée du chemin de sécurité). Il **tourne** par époque via
l'aléa du **beacon** ([[ADR-004]]). Le quorum de ⅔ se mesure en enjeu, pas en têtes. La taille
du comité reste modeste au départ, ce qui est précisément ce qui rend les certificats PQ
gérables (ADR-005).

## 11. Vérification par le harnais DST (le vrai sceau d'orfèvre)

Voilà ce qui rendra ce gadget impressionnant pour de bonnes raisons : il se conçoit pour être
**continûment falsifié** par ta simulation déterministe. Quatre invariants nouveaux, vérifiés à
travers les centaines de graines fautées :

- **Sûreté de finalité** : jamais deux points de contrôle finalisés en conflit.
- **Sûreté responsable** : toute violation de sûreté injectée produit bien une **preuve** de
  faute couvrant **≥ ⅓** de l'enjeu (on injecte un comité byzantin qui double-vote ou enveloppe,
  et on vérifie que le slashing détecte et attribue).
- **Vivacité de finalité** : sous synchronie et ⅔ honnêtes, des points de contrôle continuent
  de se finaliser (le sweep ne reste pas bloqué sans finalité).
- **Vivacité plausible** : aucun scénario n'enferme le protocole dans un état d'où finaliser
  exigerait de se faire slasher.

Et le test **2b** bascule de « la partition multi-blocs diverge (gadget-deferred) » à « la
partition multi-blocs **réconcilie** via la finalité », avec conservation globale au heal. Un
gadget de finalité dont la sûreté responsable est prouvée **et** falsifiée en continu par
simulation déterministe : ça, c'est l'objet rare que les gens du domaine respectent.

## 12. Décisions à fixer (🛑 les tiennes)

- **E**, la longueur d'époque (en blocs ou en temps) : règle la latence de finalité et la
  fréquence des certificats.
- **Seuil de quorum** : ⅔ de l'enjeu est le standard BFT ; à confirmer.
- **Variante exacte de fork-choice** consciente de la finalité (de la plus simple, adaptée à ta
  chaîne linéaire, vers une LMD-GHOST si besoin).
- **Montants et fenêtre de slashing** ([[ADR-003]]) pour les deux conditions du §7.
- **Pénalité d'inactivité** éventuelle (le « inactivity leak » de Casper) pour récupérer la
  vivacité si plus d'un tiers de l'enjeu disparaît durablement : à décider plus tard.

## 13. Limites honnêtes

Les arguments des §7 et §8 sont des **esquisses** appuyées sur les résultats connus de Casper
FFG, pas des preuves formelles propres à Quanta. La composition fork-choice plus finalité (le
« Gasper » d'Ethereum) recèle des subtilités de vivacité réelles. Avant genèse, ce protocole
demandera une formalisation des invariants critiques et un audit externe. C'est l'architecture
de départ, choisie parce qu'elle est éprouvée et qu'elle colle à ton système, pas un acquis.

## 14. Comment ça devient du code (chirurgical, pas un jet)

La conception est faite d'un bloc ; l'**implémentation** se découpe, chaque pièce vérifiée par
le harnais avant la suivante :

1. Le **squelette d'époque et de point de contrôle** (découper la chaîne, identifier les
   frontières) plus l'invariant de sûreté de finalité dans le harnais.
2. Les **votes ML-DSA** et leur agrégation en certificat d'époque, derrière l'abstraction.
3. La **règle justification/finalisation** en deux temps, avec son invariant de vivacité.
4. Les **deux conditions de slashing** et la vérification de sûreté responsable (comité byzantin
   injecté).
5. Le **fork-choice conscient de la finalité**, et la **bascule du test 2b** (réconciliation de
   partition, conservation globale au heal).
6. Rotation du comité par le beacon, puis pénalité d'inactivité.

Chacune est un spec serré, exécuté et relu, comme tout ce qu'on a fait jusqu'ici. C'est ainsi
qu'un gros morceau se construit sans devenir un gros risque.

> L'orfèvrerie, c'est cette conception. Sa beauté n'est pas dans sa taille mais dans le fait que
> chaque pièce se démontre et se mesure. Le reste, c'est de la transcription patiente.
