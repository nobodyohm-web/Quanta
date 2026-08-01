# Lancer un nœud Quanta à deux

Ce guide s'adresse à la **deuxième personne** : quelqu'un qui n'a pas écrit ce code
et qui ne devrait pas avoir à le lire pour participer.

Tu vas lancer un **nœud** de Quanta sur ta machine. Un nœud, c'est le programme qui
tient la chaîne, produit des blocs et parle aux autres nœuds. Il n'y a **aucun
serveur** au milieu : ta machine et celle d'en face se trouvent toutes seules.

> **À dire tout de suite.** Quanta est en **alpha**, **non auditée par un tiers**, et
> QUANTA n'a **aucun prix de marché**. Les pièces que tu vas produire ne valent rien
> et ne sont pas censées valoir quelque chose. C'est une expérience technique : on
> cherche à savoir si deux machines derrière deux box internet différentes arrivent à
> se parler. C'est précisément ce qui n'a **jamais** été éprouvé sur le code actuel.

---

## Ce qu'il te faut

- Un Mac ou un Linux, et un terminal.
- **~6 Go** de disque libre (c'est la compilation qui prend la place, pas la chaîne :
  les données du nœud pèsent quelques centaines de Ko).
- Une connexion internet **ordinaire** : box maison, partage de connexion 4G. Un
  réseau d'entreprise ou un VPN qui bloque l'UDP sortant empêchera la découverte —
  c'est la cause n°1 d'échec, et elle n'a rien à voir avec le code.
- Rien à ouvrir sur ta box : **aucune redirection de port**. Le nœud traverse le NAT
  tout seul, et c'est justement ce qu'on teste.

## Installation (une fois, ~15 min dont l'essentiel en compilation)

```bash
# 1. Rust (si tu ne l'as pas déjà)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 2. Le code
git clone https://github.com/nobodyohm-web/Quanta.git
cd Quanta

# 3. Le nœud
./docs/ops/peer.sh start
```

Pas besoin de Node, ni de npm, ni de l'application graphique : le nœud est *headless*
et se compile avec Rust seul.

Au premier lancement, le script demande un **mot de passe de coffre**. Il chiffre les
clés de ton portefeuille sur ta machine. Il n'est **écrit nulle part** — ni sur le
disque, ni transmis — donc si tu le perds, ce portefeuille de test est perdu avec lui.
Prends-en un simple, c'est un test. Si tu préfères éviter la question à chaque
démarrage :

```bash
export QUANTA_WALLET_PASSWORD='ton-mot-de-passe'
```

---

## Lire l'état : la seule commande qui compte

```bash
./docs/ops/peer.sh status
```

Sortie réelle de deux nœuds qui viennent de se trouver (les flèches sont des notes
ajoutées ici, pas du texte du programme) :

```
Quanta peer — RUNNING (pid 10415)
  build      : 68f35ba   v3.15.1, protocol 9
  wallet     : qta1ex5d0gzl2px97430y957paev8gxzk4hwt44n7hx7a20ahqz720dq5efvx4
  node id    : 3200493b3014b9f63a32e9d3cf8f847116f6bc38e209b95fd500f3af9125b14b
  height     : 3    finalized: 0
  balance    : 5.999999 QTA
  peers      : 1          <- you are talking to someone     ← la ligne qui compte

  >>> SEND THIS LINE to the other machine — identical = converged:
      TIP@3 = 67cae4ca682957c465276427d45924e02daae5610d0021a780253704605f5d3c
```

`wallet` est l'adresse où l'argent arrive ; `node id` est l'identité réseau, celle
qu'un pair compose pour te joindre. Elle est **stable** : elle survit aux
redémarrages et aux changements d'IP. `finalized` reste à `0` tant que personne n'a
staké — la finalité n'avance qu'avec des validateurs qui votent, ce qui n'arrive pas
sur un réseau de test à deux sans enjeu.

Trois lignes décident, dans cet ordre :

1. **`peers`** — à `0`, tu es seul au monde ; la découverte prend en général moins
   d'une minute, parfois plusieurs. À `1` ou plus, le réseau fonctionne.
2. **`TIP@…`** — le hash du dernier bloc. **Envoie cette ligne à l'autre machine.**
   Identique des deux côtés = vous êtes sur la **même** chaîne. C'est la preuve de
   convergence, et aucune ligne de log ne la remplace.
3. **`protocol`** — deux nœuds sur des versions de protocole différentes s'ignorent
   **volontairement**. À l'écran, ça ressemble trait pour trait à une panne de
   réseau. Si vos deux `build` diffèrent, mettez-vous à jour tous les deux avant de
   chercher plus loin.

Tu peux aussi ouvrir **http://127.0.0.1:8645/** dans un navigateur : c'est
l'explorateur du nœud, avec la hauteur, l'offre prouvée et les derniers blocs.

Pour suivre en direct : `./docs/ops/peer.sh logs` (Ctrl-C pour sortir).

---

## Recevoir un correctif

Quand un problème est corrigé, tu récupères le correctif toi-même :

```bash
./docs/ops/peer.sh update
```

La commande fait quatre choses, dans cet ordre : elle **affiche les commits entrants
avant de les appliquer** (tu vois ce qui va tourner chez toi), elle refuse d'écraser
des modifications locales, elle n'accepte qu'une avance en ligne droite
(*fast-forward*), puis elle recompile et relance le nœud. Si la version de protocole
a changé, elle te le dit en clair : **les deux machines doivent alors être à jour**,
sinon vous cessez de vous voir sans le moindre message d'erreur.

> **Ce qui n'existe pas, et n'existera pas.** Personne ne peut pousser du code sur ta
> machine à distance. Il n'y a pas de canal de commande, pas de mise à jour
> silencieuse, pas d'exécution déclenchée d'ailleurs. Un nœud de monnaie qui pourrait
> se faire dicter le code qu'il exécute serait une porte dérobée, pas un confort.
> C'est **toujours toi** qui lances `update`, après avoir vu la liste des commits.

## Signaler un problème

```bash
./docs/ops/peer.sh report
```

Le fichier produit contient l'état du nœud, la chaîne vue de chez toi, et les
300 dernières lignes de journal. Il ne contient **ni clé, ni mot de passe, ni jeton
RPC** — ceux-ci sont retirés avant écriture. Il contient en revanche des **adresses
IP** (la tienne sur ton réseau local, celles de tes pairs) : c'est en général là que
se voient les problèmes de connexion. Joins-le à une
[issue GitHub](https://github.com/nobodyohm-web/Quanta/issues/new/choose), avec la
ligne `TIP@…` des **deux** machines.

---

## Quand ça ne marche pas

| Symptôme | Cause la plus fréquente | Quoi faire |
|---|---|---|
| `peers : 0` après 10 min | UDP sortant bloqué (VPN, réseau d'entreprise, DNS filtré) | Coupe le VPN, ou passe en partage de connexion 4G |
| `peers : 0` mais les deux nœuds tournent | Versions de protocole différentes — les pairs s'ignorent exprès | Comparez la ligne `build`, puis `update` des deux côtés |
| Le nœud s'arrête juste après `start` | Mot de passe de coffre incorrect | Relance avec le bon ; le script affiche les dernières lignes du journal |
| `height` ne bouge pas | Normal : un bloc toutes les ~2 min, et seul le proposeur élu scelle | Attends quelques minutes ; regarde `peers` d'abord |
| `TIP@…` différents à la même hauteur | Vous avez divergé — c'est un vrai constat, pas un bug de ton côté | Envoie les deux lignes + `report` des deux machines |

Une ligne `ERROR iroh_gossip::net] gossip; me=…` apparaît au démarrage : c'est une
étiquette interne de la bibliothèque réseau, pas une erreur. Les lignes qui viennent
de Quanta sont celles marquées `quanta_lib` et préfixées `◈`.

Pour tout arrêter et ne rien laisser derrière :

```bash
./docs/ops/peer.sh stop
rm -rf ~/.quanta-peer      # les données du nœud (dont ton portefeuille de test)
```

---

## Et de l'autre côté

Celui qui a le dépôt fait exactement la même chose : `./docs/ops/peer.sh start`, puis
compare les lignes `TIP@…`. Les deux machines doivent être sur **deux réseaux
différents** — deux nœuds sur la même box partagent une IP publique et ne traversent
aucun NAT, donc ce test-là prouve beaucoup moins.

Pour la version instrumentée de la même épreuve, qui déroule les quatre étapes
(découverte DHT, traversée de NAT, convergence + partage de récompense, survie au
redémarrage) et rend un verdict : [`two-machines.sh`](two-machines.sh).

Le reste — l'application de bureau, les 17 méthodes JSON-RPC, le multisig
post-quantique — est dans [`QUICKSTART.md`](QUICKSTART.md).
