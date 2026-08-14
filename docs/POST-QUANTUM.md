# La cryptographie post-quantique

Ce document a deux objectifs : que tu **comprennes** ce qui se passe, et que tu puisses
l'**expliquer** — à un investisseur, à un recruteur, à un ami, à un auditeur. Il part de
zéro et va jusqu'aux détails de ce que Quanta fait réellement, avec les chiffres lus dans
le code plutôt que recopiés d'un article de blog.

Trois choses à savoir avant de commencer, parce qu'elles évitent les trois quarts des
malentendus :

La cryptographie post-quantique **tourne sur des ordinateurs normaux**. Ce n'est pas de la
cryptographie quantique. Il n'y a aucun matériel exotique, aucun laser, aucune fibre
dédiée : ce sont des algorithmes classiques, écrits en Rust ou en C, dont on pense qu'un
ordinateur quantique ne saura pas les casser.

« Post-quantique » ne veut **pas** dire « prouvé incassable ». Ça veut dire : aucune
attaque quantique connue ne fait mieux que les attaques classiques connues. La confiance
vient de la cryptanalyse publique, pas d'une démonstration. Deux candidats sérieux sont
morts en 2022, on y reviendra — c'est la partie la plus instructive de l'histoire.

Enfin, l'ordinateur quantique capable de casser RSA **n'existe pas** aujourd'hui. Le
problème est quand même urgent, et la raison est contre-intuitive : elle tient en une
formule qu'on verra au chapitre 4.

---

## 1. Le résumé en trente secondes

Toute la sécurité d'internet repose sur des problèmes mathématiques faciles à poser et
difficiles à résoudre : *quels sont les deux nombres premiers dont le produit fait ce
nombre de 617 chiffres ?* Personne ne sait le faire en un temps raisonnable — sur un
ordinateur classique.

En 1994, Peter Shor a montré qu'un ordinateur quantique le ferait en quelques heures. Le
même algorithme casse aussi les courbes elliptiques, donc RSA, Diffie-Hellman, ECDSA,
Ed25519 et X25519 tombent ensemble. C'est-à-dire : les signatures de ton wallet, le
cadenas de ton navigateur, les clés SSH, les certificats, à peu près tout ce qui prouve
une identité ou établit un secret en ligne.

La réponse n'est pas de faire des clés plus grandes — ça ne sert à rien contre Shor. Il
faut changer de problème mathématique. On remplace « factoriser un grand nombre » par
« trouver un vecteur court dans un réseau euclidien bruité », un problème pour lequel
personne ne connaît de raccourci quantique. C'est ce que le NIST a standardisé en août
2024, et c'est ce que Quanta utilise.

---

## 2. Sur quoi repose la cryptographie d'aujourd'hui

Une **fonction à sens unique** est facile à calculer dans un sens et infaisable dans
l'autre. Multiplier les deux nombres premiers 1 073 741 827 et 2 147 483 659 est
instantané : ça fait 2 305 843 027 467 304 993. Retrouver ces deux facteurs à partir du
seul produit, sans les connaître, demande de chercher — et à cette taille-là un ordinateur
y arrive encore. Avec des nombres de 2048 bits, « chercher » veut dire plus longtemps que
l'âge de l'univers.

Une **trappe** est un raccourci secret dans une fonction à sens unique. C'est ce qui rend
la cryptographie à clé publique possible : tout le monde peut chiffrer vers toi, seul toi
peux déchiffrer, parce que seul toi détiens la trappe.

Deux familles se partagent aujourd'hui la quasi-totalité du terrain.

**RSA** repose sur la difficulté de la factorisation. Sa clé publique est un grand nombre
qui est le produit de deux premiers ; sa clé privée, ce sont ces deux premiers.

**Les courbes elliptiques** reposent sur le logarithme discret : sur une courbe, on sait
calculer `k·G` (additionner un point à lui-même *k* fois) très vite, mais retrouver *k* à
partir de `k·G` est infaisable. Ed25519 signe avec ça, X25519 échange des clés avec ça, et
Bitcoin comme Ethereum signent avec la variante secp256k1.

Ces deux familles semblent différentes. Du point de vue quantique, **elles sont le même
problème** : les deux se ramènent à trouver une période cachée dans une fonction, et c'est
exactement là que l'ordinateur quantique excelle. C'est pour ça qu'elles tombent ensemble,
d'un seul coup, avec un seul algorithme. Il n'y a pas de repli d'une famille vers l'autre.

---

## 3. Ce qu'un ordinateur quantique change vraiment

### Shor : le tueur

L'idée fausse la plus répandue est qu'un ordinateur quantique « essaie toutes les clés en
parallèle ». Ce n'est pas ça. Une superposition contient bien toutes les possibilités,
mais quand on mesure, on n'en obtient qu'une seule, au hasard — ce qui ne vaut pas mieux
que tirer à pile ou face.

Ce que fait Shor est plus subtil et plus beau. Il transforme « factoriser N » en « trouver
la période d'une fonction ». Puis il arrange le calcul pour que les mauvaises réponses
**s'annulent entre elles** par interférence, comme deux vagues en opposition de phase, et
que les bonnes se renforcent. Quand on mesure à la fin, la période sort avec une forte
probabilité. De la période, on déduit les facteurs par de l'arithmétique élémentaire.

La leçon à retenir, et c'est la phrase qui fait comprendre le sujet à quelqu'un : **un
ordinateur quantique n'est pas rapide, il est structuré**. Il ne bat le calcul classique
que sur des problèmes qui cachent une structure — ici, une périodicité — qu'une
interférence sait révéler. Un problème sans cette structure ne lui donne aucune prise.

### Grover : l'embêtant

Le second algorithme quantique important, dû à Lov Grover, cherche dans un ensemble non
structuré. Pour trouver une aiguille parmi N possibilités, il ne faut plus N essais mais
environ √N. C'est une accélération réelle mais **quadratique**, pas exponentielle.

Concrètement : une clé AES-128 offrirait au mieux 64 bits de résistance dans une lecture
naïve — en pratique bien plus, parce que Grover se parallélise très mal et exige un
circuit d'une profondeur absurde. Une clé AES-256 reste largement hors de portée. Pour les
fonctions de hachage, la résistance à la préimage passe de 2ⁿ à 2^(n/2), et la résistance
aux collisions ne bouge pratiquement pas puisqu'elle était déjà en 2^(n/2) classiquement,
grâce au paradoxe des anniversaires.

La règle de pouce est donc : **le symétrique double, l'asymétrique meurt.**

| Primitive | Effet d'un ordinateur quantique | Verdict |
|---|---|---|
| RSA, Diffie-Hellman | Shor, temps polynomial | Cassé |
| ECDSA, Ed25519, X25519, secp256k1 | Shor, temps polynomial | Cassé |
| AES-128 | Grover, √N | Affaibli, à éviter pour du long terme |
| AES-256 | Grover, √N | Tient |
| SHA-256, BLAKE3 (256 bits) | Grover sur la préimage | Tient |

Autrement dit : ton disque chiffré en AES-256 va bien. Ta signature Ed25519, non.

---

## 4. Le calendrier, et pourquoi il est déjà en retard

Aucun ordinateur quantique existant ne s'approche de ce qu'il faudrait. Le problème n'est
pas le nombre de qubits bruts mais la **correction d'erreur** : les qubits physiques sont
fragiles et décohèrent, il en faut donc des milliers pour former un seul qubit *logique*
fiable.

Deux publications donnent l'échelle, et surtout la tendance.

En 2019, Craig Gidney et Martin Ekerå estimaient qu'il faudrait **20 millions de qubits
bruités pendant 8 heures** pour factoriser une clé RSA de 2048 bits.

En 2025, Gidney a réévalué son propre chiffre, sous **exactement les mêmes hypothèses
physiques** — même grille de qubits, même taux d'erreur de 0,1 %, même cycle de code de
surface. Nouveau résultat : **moins d'un million de qubits, en moins d'une semaine**.

Lis ce qui vient de se passer. Aucun matériel nouveau n'est intervenu. Le facteur vingt
est venu **des algorithmes** : arithmétique modulaire approchée, codes de surface
« attelés » pour stocker les qubits inactifs, et une meilleure production des états
magiques. La menace n'avance pas seulement au rythme des laboratoires de physique ; elle
avance aussi au rythme des théoriciens, et ceux-là n'ont besoin d'aucun budget
d'infrastructure.

### « Récolter maintenant, déchiffrer plus tard »

Un adversaire patient — un État, typiquement — peut enregistrer aujourd'hui du trafic
chiffré qu'il ne sait pas lire, le stocker, et attendre la machine. Le jour où elle
existe, il déchiffre rétroactivement dix ans d'archives. En anglais : *harvest now, decrypt
later*, abrégé HNDL.

C'est ce qui rend la question urgente **maintenant**, sans aucun ordinateur quantique en
vue. Michele Mosca en a tiré une inégalité qui tient en trois lettres. Soit **X** la durée
pendant laquelle tes données doivent rester secrètes, **Y** le temps qu'il te faudra pour
migrer, et **Z** le temps qui reste avant la première machine capable. Si

> **X + Y > Z**

alors tu es **déjà** en retard. Pour un dossier médical, X vaut cinquante ans. Pour un
protocole monétaire dont la chaîne est publique et éternelle, X est infini.

### Le cas particulier d'une monnaie

Pour la plupart des systèmes, casser une signature est un problème *futur* : l'attaquant
pourra forger des signatures le jour où il aura la machine, pas avant. Une blockchain est
différente, et c'est le point que peu de gens voient.

Sur une chaîne publique, **la clé publique est publiée pour toujours**. Elle est dans le
bloc, elle y restera. Un adversaire qui obtient un ordinateur quantique en 2040 peut donc
remonter à une clé publique de 2026, en dériver la clé privée par Shor, et dépenser les
fonds qui y sont attachés. Le vol est **rétroactif** : il ne s'agit pas de protéger les
transactions futures, il s'agit de constater que tout ce qui a jamais été signé est
exposé.

Bitcoin y est vulnérable de manière inégale — une adresse jamais réutilisée ne révèle
qu'un *hash* de clé publique jusqu'à sa première dépense, ce qui offre une protection
partielle et temporaire, mais les anciennes sorties de type P2PK et toute adresse déjà
dépensée exposent la clé en clair. C'est précisément le raisonnement qui a conduit Quanta
à mettre l'argent en post-quantique dès maintenant plutôt que « quand ce sera nécessaire ».
Quand ce sera nécessaire, il sera trop tard : l'histoire est déjà écrite.

---

## 5. Les familles de remplacement

Il faut des problèmes mathématiques durs qui ne cachent **pas** de périodicité exploitable
par interférence. Cinq familles ont été proposées. Trois sont en usage, deux servent de
leçon.

**Les réseaux euclidiens** (*lattices*) sont la famille dominante, et celle que Quanta
utilise. Un réseau est une grille infinie de points engendrée par des vecteurs de base — en
dimension deux, du papier millimétré ; en cryptographie, la dimension est de plusieurs
centaines. Le problème dur est de trouver le point du réseau le plus proche d'une cible, ou
le vecteur non nul le plus court. En grande dimension, c'est infaisable, y compris
quantiquement.

La formulation moderne s'appelle **LWE**, *Learning With Errors*, et son intuition est
remarquablement simple. Résoudre un système d'équations linéaires est un exercice de
lycée. Ajoute un petit bruit aléatoire à chaque équation, et le même système devient un
mur : les méthodes d'élimination amplifient le bruit au lieu de le réduire. La clé secrète
est la solution ; la clé publique est le système bruité. ML-KEM et ML-DSA utilisent une
variante structurée, **Module-LWE**, qui remplace les vecteurs par des polynômes pour
gagner en taille et en vitesse.

**Le hachage** donne la famille la plus conservatrice. Une signature construite uniquement
à partir d'une fonction de hachage n'est vulnérable que si le hachage l'est — et on a vu
que Grover ne fait que doubler les tailles. C'est SLH-DSA (ex-SPHINCS+), très lent et très
volumineux, mais reposant sur l'hypothèse la plus solide qui existe. On le garde comme
filet de sécurité.

**Les codes correcteurs d'erreurs** sont l'autre famille éprouvée : McEliece date de 1978
et n'a jamais été cassé, au prix d'une clé publique de plusieurs centaines de kilooctets.
Le NIST a retenu HQC en mars 2025 comme mécanisme de secours, précisément parce qu'il
repose sur une hypothèse différente des réseaux.

**Les isogénies** proposaient des clés minuscules à partir de la géométrie des courbes
elliptiques. Le candidat SIKE avait atteint le quatrième tour du NIST. En 2022, Wouter
Castryck et Thomas Decru l'ont cassé — non pas avec un ordinateur quantique, mais avec un
**ordinateur portable en une heure**, grâce à un théorème de 1997 que personne n'avait
pensé à appliquer là.

**Le multivarié** a subi le même sort la même année : Rainbow, finaliste, cassé par Ward
Beullens en un week-end de calcul.

Retiens ces deux morts. Elles ne prouvent pas que le post-quantique est bancal ; elles
prouvent que **la cryptanalyse publique fonctionne** — ces schémas sont morts pendant la
compétition, avant tout déploiement, ce qui est exactement le but d'une compétition
ouverte. Et elles justifient à elles seules l'hybridation dont on parlera au chapitre 11.

---

## 6. Les standards NIST

Le NIST a lancé un concours public en 2016. Après huit ans, trois tours et beaucoup de
cadavres, les standards ont été publiés le **13 août 2024**.

| Standard | Nom officiel | Nom de compétition | Rôle |
|---|---|---|---|
| FIPS 203 | ML-KEM | Kyber | Échange de clés (confidentialité) |
| FIPS 204 | **ML-DSA** | Dilithium | **Signature (authenticité)** |
| FIPS 205 | SLH-DSA | SPHINCS+ | Signature de secours, à base de hachage |
| (FIPS 206, en projet) | FN-DSA | Falcon | Signature compacte — projet non finalisé en août 2026 (statut NIST, non vérifiable depuis ce dépôt) |
| (sélectionné en mars 2025) | HQC | — | KEM de secours, à base de codes |

Les noms ont changé à la standardisation : « Kyber » et « Dilithium » sont les noms des
soumissions, « ML-KEM » et « ML-DSA » ceux des normes. *ML* veut dire *Module-Lattice*, la
structure mathématique sous-jacente. Dans une conversation, les deux se disent ; dans un
document technique, on écrit ML-DSA.

Chaque schéma existe en plusieurs tailles, rattachées aux **catégories de sécurité** du
NIST : la catégorie 1 équivaut à casser AES-128, la catégorie 3 à AES-192, la catégorie 5 à
AES-256. Quanta utilise **ML-DSA-65** et **ML-KEM-768**, tous deux de catégorie 3 — le choix
d'ingénierie standard, celui qui laisse une marge confortable sans payer les tailles de la
catégorie 5.

---

## 7. Les deux métiers : chiffrer et signer

Une confusion fréquente mérite d'être levée, parce qu'elle change tout le raisonnement sur
l'urgence.

**Un KEM** (*Key Encapsulation Mechanism*) sert à établir un secret partagé. Son
fonctionnement : tu publies une clé publique ; quelqu'un exécute `encapsulate(pk)` qui lui
rend un couple *(chiffré, secret)* ; il t'envoie le chiffré ; tu exécutes
`decapsulate(sk, chiffré)` et tu obtiens **le même secret**. Ce secret sert ensuite de clé
AES pour la vraie conversation. C'est le remplaçant de Diffie-Hellman, et son métier est la
**confidentialité**.

**Une signature** prouve qu'un message vient bien du détenteur d'une clé et n'a pas été
modifié. Son métier est l'**authenticité**.

Les deux ne sont pas menacés au même moment. La confidentialité est attaquable
**rétroactivement** — c'est HNDL, l'adversaire enregistre aujourd'hui et lit demain.
L'authenticité, elle, est normalement un problème du présent : on ne peut pas forger une
signature dans le passé.

Sauf, comme on l'a vu au chapitre 4, quand la clé publique est publiée pour l'éternité dans
une chaîne de blocs. Pour une monnaie, la signature redevient un problème rétroactif. C'est
pourquoi Quanta a migré **les signatures en premier**, ce qui est l'inverse de la priorité
habituelle du reste de l'industrie.

---

## 8. Comment marche ML-DSA, sans les mathématiques

C'est l'algorithme qui signe chaque transaction Quanta. Il mérite d'être compris dans ses
grandes lignes.

Le secret est un ensemble de petits polynômes à coefficients minuscules. La clé publique
est le résultat d'un calcul qui les mélange avec une matrice publique et y ajoute du bruit :
`t = A·s₁ + s₂`. Retrouver `s₁` et `s₂` à partir de `A` et `t`, c'est exactement le problème
Module-LWE — le système d'équations bruité du chapitre 5.

Signer suit un schéma en trois temps hérité des protocoles d'identification :

**L'engagement.** Le signataire tire un vecteur aléatoire `y` et publie `w = A·y`. Il s'est
engagé sur un aléa sans le révéler.

**Le défi.** Au lieu d'attendre un défi de son interlocuteur, il le fabrique lui-même en
hachant `w` avec le message : `c = H(w ‖ message)`. C'est la **transformation de
Fiat-Shamir**, qui rend le protocole non interactif — on peut signer hors ligne, et
n'importe qui peut vérifier plus tard.

**La réponse.** Il calcule `z = y + c·s₁`. La réponse mélange l'aléa et le secret.

Et c'est ici que se trouve la subtilité qui donne son nom complet au schéma, *Fiat-Shamir
with aborts* — Fiat-Shamir **avec abandons**. Si `z` était publié tel quel, sa distribution
dépendrait légèrement de `s₁` ; en collectant assez de signatures, un attaquant
reconstituerait le secret par analyse statistique. La parade est brutale et élégante :
lorsque `z` sort d'une plage prédéfinie, le signataire **jette tout et recommence** avec un
nouveau `y`. On appelle ça l'échantillonnage par rejet. Les signatures publiées sont
uniquement celles dont la distribution ne dit rien du secret — la fuite n'est pas réduite,
elle est **rejetée**.

Conséquence pratique amusante : signer prend un nombre variable de tentatives. Vérifier, en
revanche, est déterministe et rapide.

Voilà aussi pourquoi une signature ML-DSA est grosse. Elle contient `z`, qui est un vecteur
de polynômes en dimension élevée, plus un indice de compression. On ne peut pas la
raccourcir sans réduire la dimension, donc la sécurité.

---

## 9. Comment marche ML-KEM, sans les mathématiques

Même terrain mathématique, autre métier. La clé publique est encore un système bruité
`t = A·s + e`.

Pour encapsuler, l'expéditeur tire un aléa, s'en sert pour construire deux valeurs bruitées
à partir de la clé publique, et en dérive un secret partagé. Le destinataire, qui connaît
`s`, peut retirer *presque* tout le bruit — il reste un résidu assez petit pour être arrondi
correctement, et les deux parties tombent sur le même secret. Un tiers, qui ne connaît pas
`s`, se retrouve devant Module-LWE.

Une couche supplémentaire, la **transformation de Fujisaki-Okamoto**, durcit ce mécanisme
contre les attaques où l'adversaire soumet des chiffrés malformés et observe les réactions.
Le destinataire re-chiffre ce qu'il vient de déchiffrer et vérifie que ça correspond ; sinon
il renvoie un secret bidon déterministe plutôt qu'une erreur. C'est ce qui fait passer le
schéma de « sûr contre un observateur passif » à IND-CCA2, « sûr contre un adversaire
actif ».

---

## 10. Le prix à payer, en chiffres réels

Le post-quantique n'est pas gratuit, et son coût n'est ni le temps de calcul ni la
consommation : c'est la **taille**.

| Rôle | Schéma | Clé publique | Signature ou chiffré |
|---|---|---|---|
| Signature classique | Ed25519 | 32 o | 64 o |
| Signature PQ, catégorie 1 | ML-DSA-44 | 1 312 o | 2 420 o |
| **Signature PQ, catégorie 3** | **ML-DSA-65** | **1 952 o** | **3 309 o** |
| Signature PQ, catégorie 5 | ML-DSA-87 | 2 592 o | 4 627 o |
| Échange de clés classique | X25519 | 32 o | 32 o |
| Échange de clés PQ, catégorie 3 | ML-KEM-768 | 1 184 o | 1 088 o |

*(Les valeurs ML-DSA sont celles de la crate `fips204` 0.4.6 utilisée par Quanta —
`ml_dsa_65::PK_LEN` et `ml_dsa_65::SIG_LEN`, pas des chiffres recopiés.)*

Une signature passe donc de 64 à 3 309 octets : **cinquante-deux fois plus grosse**. Sur un
disque, personne ne le remarque. Sur un réseau pair-à-pair où chaque message porte une
signature *et* la clé publique de l'émetteur, c'est une autre affaire.

Quanta en a fait l'expérience de la pire manière possible, et l'anecdote vaut mieux que
n'importe quelle explication abstraite. Depuis le fork v4, chaque enveloppe de gossip est
signée en ML-DSA-65 : 3 309 octets de signature, 1 952 octets de clé publique, le tout
encodé en hexadécimal dans du JSON — ce qui **double** encore la taille. Un simple message
de présence `Hello` pèse, relevé dans le journal d'un nœud réel — le format de ligne
vient de `p2p/gossip_tasks.rs`, la valeur est une mesure de session et ne se rejoue pas
depuis le dépôt :

```
◈ [Gossip] outgoing 14827 bytes id=e592666f8b5b
```

Or la bibliothèque de transport plafonnait un message à **4 096 octets**. Les nœuds se
découvraient, établissaient leur canal QUIC, se déclaraient voisins — et n'échangeaient
strictement rien. Pendant deux mois, sans qu'aucun test ne le voie, parce que l'émission
ratée était comptée comme réussie. Le correctif tient en une ligne, mais le diagnostic a
demandé de lancer deux vrais démons.

C'est ça, le prix du post-quantique : pas une facture, un **changement de régime de
taille** qui casse les hypothèses implicites du reste du système.

---

## 11. L'hybridation, et pourquoi elle n'est pas de la timidité

Rappelle-toi SIKE : cassé sur un portable, en une heure, après avoir survécu à quatre tours
d'une compétition internationale. Les schémas à réseaux sont bien plus étudiés, mais
personne de sérieux ne jure qu'aucune idée nouvelle ne les entamera.

D'où l'**hybridation** : on exécute l'ancien et le nouveau schéma en parallèle, et on
combine leurs deux secrets. L'attaquant doit casser **les deux** — le classique, ce qui
demande un ordinateur quantique, et le post-quantique, ce qui demande une percée
mathématique. Aucun des deux seuls ne suffit.

C'est exactement ce que fait le groupe **X25519MLKEM768**, aujourd'hui déployé par défaut
dans Chrome, Firefox et Cloudflare, et que Quanta active dans son transport QUIC. Le nom se
lit littéralement : un échange X25519 classique, **plus** une encapsulation ML-KEM-768, dont
les deux secrets sont concaténés puis dérivés ensemble. Coût réel : environ un kilooctet de
plus, **une seule fois** par connexion.

Une nuance à connaître, parce qu'elle explique une asymétrie du code de Quanta :
l'hybridation est la norme pour l'**échange de clés**, beaucoup moins pour les
**signatures**. Pour un échange de clés, le surcoût est payé une fois par connexion. Pour
une signature, il serait payé sur *chaque* message, et il faudrait transporter deux
signatures au lieu d'une — pour Quanta, cela ferait passer une enveloppe déjà à 14 827
octets encore au-dessus. Quanta a donc choisi l'hybride sur le transport et le **PQ pur**
sur les signatures — une décision assumée et écrite, pas un oubli : l'ADR-005 fixe la
règle pour les votes de finalité, en rejetant explicitement une proposition d'agrégation
hybride BLS, et l'ADR-007 fixe la portée pour les comptes.

---

## 12. Ce que fait Quanta, exactement

Voici l'inventaire complet, vérifiable ligne par ligne. Ce tableau est le genre de chose
qu'un auditeur regarde en premier, et c'est aussi la meilleure réponse à « concrètement,
vous faites quoi ? ».

| Usage | Primitive | Où c'est dans le code |
|---|---|---|
| Autorité d'une transaction (l'argent) | **ML-DSA-65** pur (FIPS 204) | règle : `p2p/ledger/validation.rs::verify_tx` · primitives : `security/hybrid_crypto.rs` |
| Votes de finalité (l'irréversibilité) | **ML-DSA-65** | `sm/finality_vote.rs` |
| Enveloppes de gossip (chaque message réseau) | **ML-DSA-65** | `p2p/gossip.rs`, `p2p/dispatcher.rs` |
| Multisig M-parmi-N | **ML-DSA-65**, quorum de clés distinctes | `p2p/ledger/validation.rs::verify_multisig` |
| Échange de clés du transport | **X25519MLKEM768** (hybride) | `Cargo.toml`, `lib.rs::run` |
| Chiffrement du coffre au repos | AES-256-GCM | `security/pq_vault.rs` |
| Dérivation depuis le mot de passe | Argon2id — 64 Mio, 3 passes, parallélisme 4 | `security/cipher.rs` |
| Hachage, adressage par contenu | BLAKE3 | partout |
| **Identité de nœud (NodeId Iroh)** | **Ed25519 — classique** | dépendance amont |

La première ligne mérite une précision, parce que la distinction est exactement celle
qu'un auditeur cherche : `security/hybrid_crypto.rs` contient les **primitives**
(`derive_ml_dsa`, `ml_dsa_sign_deterministic`, `verify_ml_dsa`), pas la règle. La règle
d'autorité — couche ML-DSA obligatoire, `from` qui doit être `BLAKE3(ADDR_DOMAIN ‖ clé
révélée)`, aucun repli Ed25519, type `Mining` refusé à tout émetteur utilisateur — est
appliquée dans `p2p/ledger/validation.rs::verify_tx`, le seul portail traversé aussi bien
à l'admission d'une transaction qu'à la validation d'un bloc.

La dernière ligne est la limite honnête, et il faut savoir l'énoncer sans la maquiller : la
bibliothèque de transport Iroh identifie chaque nœud par une clé Ed25519, et attend un
consensus d'industrie avant de passer ses identifiants en post-quantique. Ce n'est pas du
code Quanta, et le jour où Iroh livrera, la bascule sera immédiate. Ce que ça coûte
concrètement : un adversaire quantique pourrait usurper l'**identité réseau** d'un nœud. Il
ne pourrait toujours pas signer une transaction, forger un vote de finalité ni falsifier
une enveloppe — l'argent, la finalité et l'authentification des messages sont ailleurs.

Deux détails d'implémentation qui valent le détour.

**La graine plutôt que la clé.** La clé ML-DSA n'est pas tirée au hasard puis sauvegardée :
elle est **dérivée de manière déterministe** d'une graine de 32 octets
(`derive_ml_dsa(seed32)`). C'est ce qui rend la restauration par phrase de 24 mots possible
— on ne sauvegarde pas 4 032 octets de clé privée, on sauvegarde la graine, et la clé se
reconstruit à l'identique. Une clé post-quantique n'est pas plus difficile à sauvegarder
qu'une clé classique, à condition de sauvegarder la bonne chose.

**La séparation de domaine.** L'adresse d'un compte est `BLAKE3(ADDR_DOMAIN ‖ clé
publique)` avec `ADDR_DOMAIN = "QUANTA-ADDR-V1"`, et les politiques multisig utilisent
`MSIG_DOMAIN = "QUANTA-MSIG-V1"`. Ce préfixe constant paraît décoratif ; il ne l'est pas. Il
garantit qu'un hachage calculé dans un contexte ne peut jamais être confondu avec un
hachage calculé dans un autre — un hash d'adresse ne peut pas être rejoué comme un hash de
politique multisig. C'est une des protections les moins chères et les plus souvent oubliées
de toute la cryptographie appliquée.

---

## 13. Le vocabulaire

Chaque terme en une phrase, pour pouvoir suivre n'importe quelle conversation sur le sujet.

**Qubit** — l'unité de calcul quantique ; contrairement à un bit, il peut être dans une
superposition de 0 et de 1 jusqu'à la mesure.

**Superposition** — l'état d'un qubit avant mesure ; ce n'est pas « les deux à la fois »
mais une combinaison pondérée dont seule l'une des issues survivra à la mesure.

**Interférence** — le mécanisme réellement exploité par les algorithmes quantiques : on
arrange le calcul pour que les mauvaises réponses s'annulent et que les bonnes se
renforcent.

**Décohérence** — la perte de l'état quantique par interaction avec l'environnement ; c'est
l'ennemi principal, et la raison pour laquelle il faut la correction d'erreur.

**Qubit logique vs physique** — un qubit logique fiable est construit à partir de milliers
de qubits physiques bruités ; c'est ce facteur qui sépare les machines actuelles de la
menace.

**Code de surface** — le schéma de correction d'erreur le plus étudié ; c'est lui que
supposent les estimations de coût citées au chapitre 4.

**CRQC** — *Cryptographically Relevant Quantum Computer*, un ordinateur quantique assez
grand pour casser la cryptographie déployée. N'existe pas à ce jour.

**Shor** — l'algorithme quantique qui casse la factorisation et le logarithme discret,
donc RSA et toutes les courbes elliptiques.

**Grover** — l'algorithme quantique de recherche non structurée, accélération quadratique
seulement ; il affaiblit le symétrique sans le tuer.

**PQC** — *Post-Quantum Cryptography*, algorithmes classiques résistants au quantique. À ne
pas confondre avec la **QKD** (*Quantum Key Distribution*), qui distribue des clés par
des propriétés physiques et exige du matériel dédié — ce n'est pas ce dont on parle ici,
et ce n'est pas ce que Quanta fait.

**HNDL** — *Harvest Now, Decrypt Later* : enregistrer aujourd'hui du trafic chiffré pour le
lire quand la machine existera.

**Inégalité de Mosca** — X + Y > Z : si la durée de secret requise plus la durée de
migration dépasse le temps restant, tu es déjà en retard.

**KEM** — *Key Encapsulation Mechanism* : le mécanisme qui remplace Diffie-Hellman ;
`encapsulate` produit un chiffré et un secret, `decapsulate` retrouve le secret.

**IND-CCA2** — le niveau de sécurité attendu d'un chiffrement moderne : résistant à un
adversaire qui peut soumettre des chiffrés choisis et observer les réponses.

**Transformation de Fujisaki-Okamoto** — la construction générique qui fait passer un
schéma de IND-CPA (passif) à IND-CCA2 (actif) ; elle est au cœur de ML-KEM.

**Réseau euclidien** (*lattice*) — une grille infinie de points en grande dimension ; les
problèmes durs y sont « trouver le vecteur le plus court » et « trouver le point le plus
proche ».

**LWE** — *Learning With Errors* : un système d'équations linéaires auquel on a ajouté du
bruit, ce qui le rend infaisable à résoudre.

**Module-LWE** — la variante structurée utilisée par ML-KEM et ML-DSA ; elle remplace les
vecteurs par des polynômes, ce qui réduit fortement la taille des clés.

**Fiat-Shamir** — la transformation qui rend non interactif un protocole d'identification,
en remplaçant le défi de l'interlocuteur par un hachage.

**Échantillonnage par rejet** (*Fiat-Shamir with aborts*) — jeter et refaire une signature
tant que sa distribution risquerait de laisser fuir le secret.

**Séparation de domaine** — préfixer chaque hachage d'une étiquette de contexte pour qu'un
hachage d'un domaine ne puisse jamais être rejoué dans un autre.

**Catégorie de sécurité NIST** — l'échelle 1 à 5 alignée sur AES-128, AES-192, AES-256 ;
Quanta est en catégorie 3.

**Hybride** — combiner un schéma classique et un post-quantique pour qu'il faille casser
les deux.

**Confidentialité persistante** (*forward secrecy*) — la propriété qu'une clé compromise
aujourd'hui ne révèle pas les sessions passées ; orthogonale au post-quantique, et tout
aussi nécessaire.

**Agilité cryptographique** — la capacité à changer de primitive sans réécrire le système ;
c'est la vraie leçon d'ingénierie de toute cette histoire.

---

## 14. L'expliquer simplement

Trois versions selon le temps dont tu disposes.

**En une phrase.** Un ordinateur quantique cassera un jour les signatures et le chiffrement
qui protègent internet ; on remplace donc les mathématiques sur lesquelles ils reposent par
d'autres, qu'aucune machine quantique connue ne sait attaquer — et on le fait avant, parce
qu'après il sera trop tard.

**En un paragraphe.** La sécurité d'internet repose sur des problèmes qu'un ordinateur
normal ne sait pas résoudre, comme factoriser un très grand nombre. En 1994, on a démontré
qu'un ordinateur quantique les résoudrait facilement — pas en essayant toutes les réponses,
mais en organisant le calcul pour que les mauvaises s'annulent entre elles. Cette machine
n'existe pas encore, mais deux raisons rendent le sujet urgent : un adversaire peut
enregistrer aujourd'hui des données chiffrées et les lire dans quinze ans, et sur une
blockchain les clés publiques restent visibles pour toujours, donc tout ce qui a été signé
est exposé rétroactivement. Le NIST a standardisé en août 2024 des remplaçants fondés sur
un autre problème — retrouver une aiguille dans une grille de très grande dimension avec du
bruit ajouté — pour lequel personne ne connaît de raccourci quantique. Quanta les utilise
déjà pour l'argent, pour les votes de finalité et pour chaque message réseau.

**L'analogie, pour quelqu'un qui n'est pas technique.** La cryptographie actuelle, c'est un
cadenas dont la combinaison est cachée derrière une multiplication : facile à faire dans un
sens, impossible à défaire. L'ordinateur quantique ne force pas le cadenas plus vite — il
change de question. Au lieu d'essayer les combinaisons une par une, il fait *résonner* le
cadenas et écoute la note qui en sort ; cette note révèle la combinaison d'un coup. La
parade n'est pas un cadenas plus gros : c'est un cadenas qui **ne résonne pas**. Les
mathématiques post-quantiques sont ces serrures-là — pas plus solides au marteau, mais
sourdes à la seule chose que la machine quantique sait faire.

---

## 15. Les erreurs à ne pas commettre

Elles se glissent partout, y compris chez des gens sérieux.

**Dire « quantum-proof ».** Personne ne peut le prouver. On dit *résistant au quantique*,
ou *post-quantique*. La différence n'est pas cosmétique : elle est la raison pour laquelle
on hybride et pour laquelle on garde un schéma de secours à base de hachage.

**Confondre post-quantique et cryptographie quantique.** La PQC tourne sur ton ordinateur
portable. La QKD demande de la fibre dédiée, ne fait que distribuer des clés, ne résout pas
le problème de l'authentification et n'a aucun rapport avec ce que fait Quanta.

**Croire qu'il faut remplacer AES.** Non. Grover réduit AES-256 à une marge encore
confortable. Les hachages tiennent aussi. Seul l'asymétrique tombe.

**Croire que Bitcoin meurt à la seconde.** L'exposition est réelle mais graduée, elle
dépend de la réutilisation d'adresses et des types de sorties, et un fork est possible. Le
vrai problème est politique et logistique — migrer des millions de portefeuilles — plus que
mathématique.

**Inventer son propre schéma.** C'est la faute la plus grave. Il n'y a aucune raison de ne
pas utiliser une implémentation standardisée et auditée : Quanta utilise `fips204`, une
crate en Rust pur, sans `unsafe`, écrite en temps constant. Un schéma maison, même
astucieux, n'aura jamais subi ce que SIKE a subi — et SIKE est mort.

**Dire « on migrera quand ce sera nécessaire ».** Relis l'inégalité de Mosca, puis souviens-
toi que sur une chaîne publique, X est infini.

---

## 16. Pour aller plus loin

Les sources primaires, sans intermédiaire.

Les standards eux-mêmes se lisent mieux qu'on ne le croit : **FIPS 203** (ML-KEM),
**FIPS 204** (ML-DSA) et **FIPS 205** (SLH-DSA), publiés par le NIST le 13 août 2024, sont
téléchargeables librement et leurs introductions sont accessibles.

Sur le coût d'une attaque : Craig Gidney et Martin Ekerå, *How to factor 2048 bit RSA
integers in 8 hours using 20 million noisy qubits*, **Quantum**, 2021 ; puis Craig Gidney,
*How to factor 2048 bit RSA integers with less than a million noisy qubits*, 2025 — à lire
l'un après l'autre, pour voir un facteur vingt apparaître sans qu'aucun matériel ne change.

Sur la fragilité des candidats : Wouter Castryck et Thomas Decru, *An efficient key recovery
attack on SIDH*, 2022 — l'attaque qui a tué SIKE sur un ordinateur portable.

Côté Quanta, la posture exacte est dans [`SECURITY.md`](../SECURITY.md) ; les décisions
motivées, avec à chaque fois l'alternative écartée, dans
[`docs/decisions/`](decisions/) — en particulier
[**ADR-005**](decisions/ADR-005-vote-aggregation.md) (agrégation des votes et certificats
de finalité) et [**ADR-007**](decisions/ADR-007-post-quantum-scope.md) (portée du
post-quantique : les comptes en ML-DSA) ; et le fonctionnement du protocole dans
[`docs/ARCHITECTURE.md`](ARCHITECTURE.md).

---

*Tous les chiffres concernant Quanta ont été relus dans le code le 14 août 2026, version
3.16.0, `TORUS_PROTOCOL_VERSION = 10`, `CHAIN_ID = "quanta-mainnet-v10"`. Les tailles
ML-DSA proviennent des constantes de la crate `fips204` 0.4.6 effectivement compilée
(`ml_dsa_65::PK_LEN` = 1 952, `SIG_LEN` = 3 309, `SK_LEN` = 4 032), et la taille
d'enveloppe citée provient du journal d'un nœud réel, non reproductible depuis le dépôt.*
