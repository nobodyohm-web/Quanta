# Audit de sécurité — QUANTA v3.15.1 (protocole TORUS v9)
## Périmètre : réseau P2P et surface d'attaque distante

**Auditeur** : agent « réseau ». **Date** : audit sur l'arbre `/Users/alex/Desktop/Quanta`
à la version `3.15.1`, `TORUS_PROTOCOL_VERSION = 9`.
**Méthode** : lecture intégrale de `dispatcher.rs` (2 209 l.), `gossip.rs` (1 051),
`gossip_tasks.rs`, `gossip_priority.rs`, `rendezvous.rs`, `reputation.rs`, `sybil.rs`,
`willow_node.rs`, `state_persistence.rs`, `security_tests.rs`, `username.rs`, `mod.rs`,
plus le code de `iroh-gossip 0.98.0` (`proto/plumtree.rs`, `proto/hyparview.rs`,
`net/util.rs`) qui exécute le transport.
**Preuves** : 20 tests écrits et exécutés dans une copie jetable du dépôt
(`/tmp/qaudit_net`, `CARGO_TARGET_DIR=/tmp/qtarget_net_rel`), **20/20 verts en profil
`--release`**. Aucun fichier du dépôt d'origine n'a été modifié. Les chiffres cités
sont des mesures `--release` sur cette machine, pas des estimations.

---

## 1) Résumé exécutif

Le réseau est **ouvert, non authentifié à l'entrée, et il travaille avant de vérifier**.
Les trois pires choses :

1. **R1 — Diffamation à distance (CRITIQUE).** À l'échec de la vérification de
   signature, le nœud diffuse un `ReportPeer` contre `env.sender` — une donnée qui
   vient d'être jugée non authentique. 3 enveloppes forgées (32 Ko) suffisent à faire
   bannir 1 h n'importe quel nœud honnête par tout le maillage. Amplification CPU
   mesurée : **×1 500** (1 µs attaquant → 1,58 ms victime).
2. **R2 — Vol de `@pseudo` (CRITIQUE).** `claimed_at` est choisi par l'attaquant et la
   règle est « le plus ancien gagne ». Un `claimed_at = 0` évince le détenteur
   légitime de n'importe quel pseudo. Comme `@alex` **est** l'adresse de paiement,
   les fonds envoyés à `@alex` partent chez le voleur.
3. **R3 — Plafond transport porté de 4 Ko à 10 Mo (CRITIQUE).** `iroh-gossip` **relaie
   et met en cache 30 s** tout message reçu **avant** de le remettre à l'application,
   dans un cache sans borne de taille. Quanta a multiplié l'unité de flood par 2 560 :
   un attaquant abonné au topic public provoque un OOM distant sous le dispatcher,
   là où aucune garde de Quanta ne peut le voir.

Ces trois constats sont **exploitables sans posséder la moindre pièce, sans enjeu,
sans être connu du réseau** : il suffit d'être abonné au topic gossip public.

---

## 2) Tableau des constats

| id | sévérité | ancre | résumé |
|----|----------|-------|--------|
| R1 | **CRITIQUE** | `src-tauri/src/p2p/dispatcher.rs:554` | `ReportPeer` diffusé sur un `env.sender` NON authentifié → bannissement à distance de tout nœud + amplification CPU ×1 500 |
| R2 | **CRITIQUE** | `src-tauri/src/p2p/username.rs:173` | `claimed_at` non contraint + « le plus ancien gagne » → vol de n'importe quel `@pseudo` → détournement des paiements |
| R3 | **CRITIQUE** | `src-tauri/src/p2p/willow_node.rs:597` | `max_message_size` = 10 Mo (défaut 4 Ko) ; plumtree relaie + cache 30 s AVANT authentification, cache non borné en taille → OOM distant |
| R4 | **HAUT** | `src-tauri/src/p2p/gossip.rs:325` | Bombe de décompression : borne en octets (50 Mo) mais pas en **nombre d'éléments** → 47 Ko sur le fil → 366 Mo de tas (×2 950) |
| R5 | **HAUT** | `src-tauri/src/p2p/dispatcher.rs:1307` et `:1532` | `Pong` et `ChainSegment` sont **diffusés à tout le maillage** au lieu d'être unicast → amplification O(N²) |
| R6 | **HAUT** | `src-tauri/src/p2p/dispatcher.rs:438` et `:466` | Parse JSON de 10 Mo + re-sérialisation canonique + BLAKE3 **avant** la signature et **hors** limiteur de débit |
| R7 | **HAUT** | `src-tauri/src/p2p/username.rs:309` | Registre de pseudos **sans plafond** + `rebuild_by_pk()` O(n) par insertion → O(n²) CPU, ~1 Go pour 100 k pseudos, persisté sur disque |
| R8 | **HAUT** | `src-tauri/src/p2p/dispatcher.rs:247` | Limitation de débit **uniquement par expéditeur**, aucun plafond global ; une identité ML-DSA coûte 165 µs → contournement total par rotation de clés |
| R9 | **MOYEN** | `src-tauri/src/p2p/rendezvous.rs:315` + `:361` | Ordre de composition DHT = ordre croissant des `EndpointId` (« minable ») et aucune limite par source → éclipse à l'amorçage |
| R10 | **MOYEN** | `src-tauri/src/p2p/dispatcher.rs:336` | Saturer les 10 000 cibles de la table de signalement (3 clés suffisent) **désactive définitivement** le bannissement pour tout le monde (fail-open) |
| R11 | **MOYEN** | `src-tauri/src/p2p/dispatcher.rs:498` | Comptabilité par pair (`bytes_in`, `messages_in`) écrite **avant** la signature → falsification des métriques d'un pair honnête |
| R12 | **MOYEN** | `src-tauri/src/p2p/gossip_priority.rs:182` | Les 4 lanes mpsc sont **non bornées** ; le producteur (dispatcher, sur chemin non authentifié) peut dépasser le drain |
| R13 | **MOYEN** | `src-tauri/src/p2p/dispatcher.rs:1109` | `Hello.known_peer_ids` et `Hello.heads` sans plafond de cardinalité ni de longueur |
| R14 | **BAS** | `src-tauri/src/p2p/gossip.rs:637` | `wrap_outgoing` : id sur le payload seul + `nonce: 0` → enveloppe systématiquement rejetée. **Code mort**, pas un contournement, mais un piège |
| R15 | **BAS** | `src-tauri/src/p2p/state_persistence.rs:32` | Le `NonceTracker` (nonces reçus, bans) n'est **pas** persisté : remise à zéro à chaque redémarrage |
| R16 | **BAS** | `src-tauri/src/p2p/dispatcher.rs:450` | `is_banned` prend un verrou en **écriture** sur le tracker global pour chaque message non authentifié → point de sérialisation global |
| R17 | **BAS** | `src-tauri/src/p2p/fork_heal.rs:170` | Éviction O(n) avec ~1 024 clones de `String` par bloc offert quand le buffer est plein |

---

## 3) Développement par constat

### R1 — CRITIQUE — Diffamation à distance : bannir tout le réseau pour 3 Mo

**Ancre** : `src-tauri/src/p2p/dispatcher.rs:547-565` (le bloc), `:554` (le `broadcast`).

**Ce qui est faux.** Le pipeline d'entrée est, dans l'ordre : taille (`:429`), parse JSON
(`:438`), ban (`:450`), id canonique (`:466`), dedup lecture seule (`:481`), statistiques
(`:487`), comptabilité par pair (`:497`), fraîcheur (`:512`), **puis** signature (`:547`).
À l'échec de la signature :

```rust
// dispatcher.rs:547
if let Err(reason) = verify_envelope_signature(&env) {
    ...
    broadcast(state, GossipMessage::ReportPeer {
        peer_id: env.sender.clone(),          // <-- donnée NON authentifiée
        reason: ReportReason::InvalidSignature,
    }).await;
```

`env.sender` est un champ libre de l'enveloppe. La branche est atteinte *précisément
parce que* la signature qui aurait pu le lier à quelqu'un est fausse. Le nœud honnête
signe donc lui-même, avec sa vraie clé ML-DSA, une dénonciation nominative d'une
victime choisie par l'attaquant, et la diffuse.

**Chemin d'exploitation (prouvé).** L'attaquant n'a besoin d'aucune clé, d'aucun solde,
d'aucune ancienneté : seulement d'être abonné au topic `quanta-network-v1` (public,
`willow_node.rs:49`) ou de parler à un nœud.
1. Il lit la clé publique ML-DSA d'une victime : elle est en clair dans le champ
   `sender` de **chaque** enveloppe que la victime émet.
2. Il forge : `sender = clé de la victime`, `payload` quelconque, `timestamp` frais,
   `nonce` quelconque ≥ 1, `signature` = octets aléatoires, `id` = `envelope_id(...)`
   (fonction publique, `gossip.rs:596`).
3. Il envoie cette enveloppe à **3 nœuds honnêtes distincts**. Chacun émet un
   `ReportPeer{peer_id: victime}` **signé de sa propre clé**.
4. `record_report` (`dispatcher.rs:278`) compte 3 rapporteurs distincts →
   `REPORT_BAN_THRESHOLD = 3` → ban de `REPORT_BAN_TTL_SECS = 3600` s.
5. Le ban est appliqué à l'entrée (`dispatcher.rs:451`) : **tout** le trafic
   parfaitement signé de la victime est jeté avant même d'être compté.

**Preuve exécutée** (`audit_net::n1_*` et `n1b_*`, verts) :
```
N1 : coût attaquant 10 724 o -> le nœud ÉMET 14 680 o d'accusation signée (×1,37)
N1b: victime bannie 3 600 s, ses enveloppes valides sont jetées
```
Le test `n1b` va jusqu'au bout : trois nœuds honnêtes manipulés → un quatrième
observateur bannit la victime → `stats.messages_received` de l'observateur n'augmente
plus quand la victime parle.

**Impact chiffré.**
- *Coût attaquant* : 10 724 o par enveloppe, **zéro opération cryptographique** (la
  signature est du remplissage). Coût CPU ≈ un BLAKE3 de 10 Ko ≈ 1 µs.
- *Coût victime* (mesuré, release) : **1,583 ms de CPU par enveloppe**
  (`audit_net_b::a2`), décomposé en une vérification ML-DSA-65 ratée (157,5 µs,
  `a1`) + une **signature ML-DSA-65 complète** (455,6 µs, `a1`) + sérialisations.
  → **amplification CPU ≈ ×1 500**.
- *Amplification en octets* : ×1,37 par pair « eager ». La vue active HyParView vaut
  5 (`hyparview.rs:202`), et le `ReportPeer` est lui-même diffusé épidémiquement :
  **×6,8 au premier saut**, puis inondation du maillage entier.
- *Saturation d'un nœud* : à 1,583 ms/enveloppe, 632 enveloppes/s (**6,8 Mo/s ≈
  54 Mbit/s**) monopolisent la boucle de dispatch, qui est **unique et séquentielle**
  (`gossip_tasks.rs:63-66` : `while let Some(event) = rx.next().await { dispatch(...).await }`).
  Au-delà, `Event::Lagged` (`gossip_tasks.rs:92`) fait perdre du gossip **honnête**.
- *Coût de faire taire tout le réseau* : 3 enveloppes par victime, soit
  **3 × N × 10,7 Ko**. Pour N = 100 nœuds : **3,2 Mo de trafic pour un blackout d'une
  heure de tout le réseau**.

**Variante encore plus simple (prouvée, `audit_net_c::e1`).** Il n'est même pas
nécessaire de passer par la diffamation. `REPORT_BAN_THRESHOLD` compte des **identités**,
et une identité ML-DSA-65 coûte **164,5 µs** à fabriquer (`a1`). Trois marionnettes
signant chacune un `ReportPeer` bannissent directement n'importe qui :
```
E1 : 3 identités ML-DSA fabriquées + 3 messages = 3,21 ms de CPU attaquant
     -> victime bannie 3 600 s
```
Le commentaire `SEC-REPORT-1` de `dispatcher.rs:56-59` (« il faut désormais N parties
prenantes indépendantes ») est **faux** : une partie prenante coûte 165 µs.

**Correctif suggéré.** (a) Ne **jamais** émettre de `ReportPeer` depuis le chemin
non authentifié — au mieux incrémenter un compteur local. (b) Faire du bannissement
une décision **strictement locale et first-hand** (« j'ai moi-même vu ce pair mal se
comporter »), jamais transitive. (c) Si un signalement réseau est conservé, l'adosser
à un coût non falsifiable (enjeu bondé), pas à un compte d'identités.

---

### R2 — CRITIQUE — Vol de `@pseudo` : `claimed_at` est choisi par l'attaquant

**Ancres** : `src-tauri/src/p2p/username.rs:53` (le champ), `:173` (`challenger_wins`),
`:309` (`apply`), `src-tauri/src/p2p/dispatcher.rs:942` (`handle_publish_username`).

**Ce qui est faux.** La résolution de conflit est :
```rust
// username.rs:173
fn challenger_wins(challenger: &UsernameRecord, incumbent: &UsernameRecord) -> bool {
    (challenger.claimed_at, challenger.owner_pk.as_str())
        < (incumbent.claimed_at, incumbent.owner_pk.as_str())
}
```
« Le plus ancien gagne ». Or `claimed_at: u64` (`:53`) est un champ **libre**, couvert
par la signature du revendiquant lui-même — ce qui prouve qu'il l'a choisi, pas qu'il
est vrai. `apply` (`:309`) valide le nom, la forme de l'adresse et la signature, et
**rien du tout** sur `claimed_at` : pas de borne supérieure (« pas dans le futur »),
pas d'ancrage sur une hauteur de bloc, pas de preuve de temps.

**Chemin d'exploitation (prouvé).** L'attaquant n'a besoin que d'une clé ML-DSA
(165 µs) et d'un message gossip.
1. Il génère `UsernameRecord{ username: "alex", owner_pk: <son adresse>,
   owner_key: <sa clé>, claimed_at: 0 }` et le signe correctement.
2. Il le diffuse en `PublishUsername`.
3. Chez **tous** les nœuds, `challenger_wins` est vrai (`0 < now`) → `Replaced`.
4. `resolve("alex")` renvoie désormais l'adresse du voleur. Comme le module l'annonce
   lui-même (`username.rs:3-5` : « on envoie des QUANTA à `@alex` au lieu d'une clé
   publique »), **tout paiement adressé à `@alex` part chez le voleur**.

En cas d'égalité à `claimed_at = 0`, le départage est `owner_pk` croissant : l'attaquant
« mine » une adresse ML-DSA à préfixe nul (quelques octets suffisent, l'adresse est
`BLAKE3(domaine ‖ clé)`, donc grindable à coût linéaire) et devient **imbattable**.

**Preuve exécutée** (`audit_net::n7`, vert) :
```
@alex est enregistré à claimed_at = now par le détenteur honnête -> Inserted
Le voleur publie @alex avec claimed_at = 0                        -> Replaced
resolve("alex") == adresse du VOLEUR
```

**Impact chiffré.** Coût : une clé (165 µs) + une enveloppe (~11 Ko) **par pseudo volé**.
Un attaquant peut préempter en bloc tous les pseudos courts et « de marque » du réseau
(`quanta`, `admin`, `binance`, `alex`, …) et rester définitivement propriétaire, puisque
personne ne peut produire un `claimed_at` inférieur à 0. La convergence déterministe
promise par le module (`username.rs:11-14`) est intacte — elle converge simplement
**vers le voleur**.

**Ce qui est correctement fait ici** (à conserver) : la liaison `owner_key → owner_pk`
via `address_hex_binds_key_hex` (`:145`) ferme bien l'usurpation cryptographique
(test `rejects_unbound_key_closes_pseudo_hijack`). Le trou est temporel, pas crypto.

**Correctif suggéré.** Ancrer la revendication dans la chaîne : `claimed_at` doit être
une **hauteur de bloc** que le récepteur vérifie contre sa propre chaîne (`≤ hauteur
locale`, et `≥ hauteur_locale − fenêtre`), ou mieux, faire de la revendication une
transaction du ledger — l'ordre des blocs est alors la seule horloge, et elle n'est
pas falsifiable. Refuser tout enregistrement dont le `claimed_at` est antérieur à la
genèse.

---

### R3 — CRITIQUE — Le plafond transport à 10 Mo transforme plumtree en amplificateur

**Ancres** : `src-tauri/src/p2p/willow_node.rs:596-598`,
`src-tauri/src/p2p/dispatcher.rs:100` (`MAX_RAW_ENVELOPE_BYTES = 10 * 1024 * 1024`).

**Ce qui est faux.** Le README annonce que le plafond `iroh-gossip` de 4 096 o rendait
le nœud muet (une enveloppe ML-DSA pèse ~10,7 Ko — mesuré : `|pk| = 3 904` caractères
hex, `|sig| = 6 618`). Le correctif GOSSIP-MTU-1 n'est **ni** de la fragmentation **ni**
de la compression : c'est un relèvement brut du plafond du transport à la garde
anti-DoS applicative :

```rust
// willow_node.rs:596
let gossip = Gossip::builder()
    .max_message_size(crate::p2p::dispatcher::MAX_RAW_ENVELOPE_BYTES)  // 10 485 760
    .spawn(endpoint.clone());
```

`DEFAULT_MAX_MESSAGE_SIZE = 4096` (`iroh-gossip-0.98.0/src/proto.rs:69`). Le facteur
est **×2 560**. Or la lecture du code de `iroh-gossip 0.98.0` montre ce qui se passe
**avant** que Quanta ne voie quoi que ce soit
(`iroh-gossip-0.98.0/src/proto/plumtree.rs:490`, `fn on_gossip`) :

```rust
self.cache.insert(message.id, message.clone(), now + self.config.message_cache_retention);
self.eager_push(message.clone(), &sender, io);   // relais à tous les voisins « eager »
self.lazy_push(message.clone(), &sender, io);
...
io.push(OutEvent::EmitEvent(Event::Received(...)));  // <-- SEULEMENT ICI l'application voit le message
```

Trois faits, tous vérifiés par lecture du code de la dépendance :
- **Le message est relayé avant d'être authentifié.** Le relais est du ressort de
  plumtree ; `dispatch_incoming` n'est appelé qu'après (`gossip_tasks.rs:66`).
- **Le message est mis en cache 30 s** (`message_cache_retention: Duration::from_secs(30)`,
  `plumtree.rs` `impl Default for Config`), et ce cache est un `TimeBoundCache`
  (`plumtree.rs:369`) dont la définition (`proto/util.rs:317`) est un simple
  `HashMap` + `TimerMap` : **borne temporelle uniquement, aucune borne de taille**.
- **La vue active vaut 5** (`hyparview.rs:202`) : chaque message reçu repart vers
  jusqu'à 5 voisins.

**Chemin d'exploitation (non prouvé par test end-to-end — voir §5 —, mais établi par
lecture du code des deux couches).** Un attaquant s'abonne au topic public et diffuse
des messages de 10 Mo de JSON quelconque (ils n'ont même pas besoin d'être des
enveloppes valides : le rejet applicatif arrive **après** le cache et le relais).
- *Mémoire pré-authentification* : à 12,5 Mo/s (lien 100 Mbit/s), le cache plumtree
  d'une victime atteint **375 Mo en 30 s** ; à 1 Gbit/s, **3,75 Go**. Rien dans Quanta
  ne peut l'empêcher : `MAX_RAW_ENVELOPE_BYTES` s'applique en `dispatch_incoming`,
  c'est-à-dire **en aval** du cache.
- *Amplification réseau* : 10 Mo injectés une fois → relayés vers 5 voisins par chaque
  nœud atteint → inondation épidémique du maillage à 10 Mo l'unité.
- *Effet secondaire mesuré* : même en restant sous le plafond, une enveloppe de 8 Mo
  coûte **16 ms** de dispatch (`audit_net::n5`, release) avant toute vérification de
  signature ; la boucle de dispatch étant unique, 62 messages/s suffisent à la
  monopoliser (`Lagged` → perte de gossip honnête).

**Impact.** OOM distant, sans identité, sans coût, et **invisible depuis les compteurs
de Quanta** (`GossipStats` ne compte que ce qui atteint `dispatch_incoming`).

**Correctif suggéré.** Découpler les deux plafonds : réduire l'enveloppe (l'énorme
majorité de sa taille est la clé publique ML-DSA de 1 952 o et la signature de 3 309 o,
toutes deux transmises **en hexadécimal** dans du JSON — un encodage binaire ou base64
divise déjà la taille par deux ; et le `sender` pourrait être une **adresse** de 32 o
avec la clé révélée seulement quand elle est inconnue du récepteur). Fixer ensuite
`max_message_size` au plus juste (≈ 32 Ko), et non à 10 Mo.

---

### R4 — HAUT — Bombe de décompression `ChainSegment` : borne en octets, pas en éléments

**Ancres** : `src-tauri/src/p2p/gossip.rs:307-327` (`decompress_blocks`),
`src-tauri/src/p2p/dispatcher.rs:787-801` (l'appel), `:1552` (la troncature… trop tard).

**Ce qui est faux.** La borne existe et fonctionne, mais elle borne la **mauvaise
grandeur** :
```rust
// gossip.rs:320
if out.len() + n > MAX_DECOMPRESSED_BYTES {   // 50 Mo
    return Err(...);
}
...
// gossip.rs:325
serde_json::from_slice::<Vec<String>>(&out)   // AUCUNE borne sur le NOMBRE d'éléments
```
Et le côté dispatcher tronque à 50 blocs (`:1552`) **après** que le `Vec<String>` a été
entièrement construit (`:787` → `:802`).

**Chemin d'exploitation (prouvé).** Un attaquant authentifié (une clé ML-DSA, 165 µs)
envoie un `ChainSegment` dont `blocks_compressed` est le gzip de `["","","",…]` :
3 octets par élément, taux de compression ~1 000:1.

**Preuve exécutée** (`audit_net::n4`, vert, release) :
```
N4 : gzip 46 660 o (sur le fil ~130 166 o après encodage JSON de Vec<u8>)
     -> 48 000 001 o décompressés
     -> Vec<String> de 16 000 000 entrées = 366 Mo de tas (226 ms)
     Ratio fil -> tas : ×2 950
```

**Impact chiffré.** **47 Ko sur le fil → 366 Mo de tas et 226 ms de CPU**, ×2 950.
Le limiteur de débit autorise 30 messages/minute/expéditeur ; avec 4 clés (660 µs de
fabrication) l'attaquant force **2 msg/s**, soit ~730 Mo/s d'allocation transitoire et
100 % d'un cœur en permanence. Avec le pic d'allocation de `Vec` (doublement) le
sommet réel est plutôt **~700 Mo par message** — OOM sur toute machine à 8 Go dès que
deux dispatchs se chevauchent.

**Correctif suggéré.** Décoder en **flux** avec un plafond sur le nombre d'éléments
(`MAX_CHAIN_SEGMENT_RECEIVED = 50`) *avant* d'allouer, et ramener
`MAX_DECOMPRESSED_BYTES` à `50 × taille_max_d'un_bloc`, pas à 50 Mo. Rejeter au-delà,
ne pas tronquer après coup.

---

### R5 — HAUT — Les réponses point-à-point sont diffusées : amplification O(N²)

**Ancres** : `src-tauri/src/p2p/dispatcher.rs:1307` (`handle_ping` → `broadcast(Pong)`),
`:1532` (`handle_request_chain` → `broadcast(ChainSegment)`), `:1741` (`broadcast`).

**Ce qui est faux.** `broadcast()` (`:1741`) pousse sur `gossip_tx`, drainé par
`spawn_outgoing_drain` qui appelle `sender.broadcast(...)` (`gossip_tasks.rs:41`) —
c'est-à-dire **le topic entier**. Il n'existe aucun chemin unicast dans le module. Donc :
- Un `Ping` émis par un pair est reçu par les N nœuds ; **chacun** diffuse un `Pong` à
  tout le maillage.
- Un `RequestChain` est reçu par les N nœuds ; **chacun** diffuse un `ChainSegment`
  complet, sans vérifier que quiconque l'a demandé, ni que d'autres répondent déjà.

**Preuve exécutée** (`audit_net::n2`, `n3`, verts, release) :
```
N2 : Ping 10 724 o -> Pong DIFFUSÉ 10 739 o. Avec N pairs : 1 Ping = N×10 739 o émis,
     reçus N fois => O(N²)
N3 : RequestChain 10 772 o -> ChainSegment DIFFUSÉ 27 851 o (×2,6) ;
     max_blocks = u64::MAX est accepté (clampé à 50 en interne)
```

**Impact chiffré.** Un `ChainSegment` de 50 blocs pleins pèse bien plus que les
27,8 Ko du test (chaîne quasi vide) : quelques centaines de Ko. Pour N = 20 pairs et
un segment de 200 Ko, **un seul `RequestChain` de 10,8 Ko déclenche 20 × 200 Ko émis,
chacun reçu 20 fois = 80 Mo de trafic maillé** — amplification ≈ **×7 400**. Le
limiteur autorise 30 `RequestChain`/min/expéditeur, et il se contourne par rotation
de clés (R8). Chaque réponse coûte en plus au répondant une lecture de 50 blocs, leur
sérialisation JSON, un gzip et une signature ML-DSA (456 µs).

Note : `handle_ping` accepte le Ping et répond même si l'expéditeur est inconnu de
`peer_info` (`:1300-1307` : la mise à jour de liveness est conditionnelle, la réponse
ne l'est pas).

**Correctif suggéré.** Répondre en unicast (iroh sait ouvrir une session directe vers
un `EndpointId`). À défaut : ne répondre à un `RequestChain` que si l'on est le pair
le « plus proche » du demandeur par une règle déterministe, avec un délai aléatoire
et une annulation si quelqu'un a déjà répondu ; et ne jamais répondre à un `Ping`
d'un pair absent de `peer_info`.

---

### R6 — HAUT — Le travail lourd est fait avant la signature, hors limiteur de débit

**Ancres** : `src-tauri/src/p2p/dispatcher.rs:429` → `:547` (tout le préambule),
en particulier `:438` (parse) et `:466` (id canonique).

**Ce que fait le nœud sur des octets non authentifiés**, dans l'ordre exact :

| # | ancre | opération | coût |
|---|-------|-----------|------|
| ① | `:429` | test de taille | O(1) — **correct** |
| ② | `:438` | `serde_json::from_slice::<GossipEnvelope>` sur **jusqu'à 10 Mo** | allocation + parse complets |
| ③ | `:450` | `nonce_tracker.write()` → `is_banned` | **verrou en écriture global** + mutation |
| ④ | `:466` | `GossipRouter::envelope_id(...)` = **re-sérialisation JSON canonique de tout le payload** + BLAKE3 | O(taille) |
| ⑤ | `:481` | `gossip.read()` → `has_seen` | verrou lecture |
| ⑥ | `:487` | `gossip.write()` → `messages_received += 1`, `bytes_received += len` | **verrou écriture + état écrit par l'attaquant** |
| ⑦ | `:497` | `peer_info.write()` → `bytes_in`/`messages_in` de `env.sender` | **verrou écriture + état d'un pair honnête falsifié** (voir R11) |
| ⑧ | `:512` | `is_fresh` = parse RFC3339 ; en échec, 2ᵉ parse + `gossip.write()` | verrou écriture |
| ⑨ | `:547` | vérification ML-DSA-65 (157 µs) | — |
| ⑩ | `:554` | **`broadcast(ReportPeer)` = signature ML-DSA (456 µs) + émission** | voir R1 |

Le limiteur de débit est en `:581`, le contrôle de nonce en `:601` — **après** tout
cela. Le commentaire de `:541-546` affirme que « les filtres sans état bon marché
(taille / JSON / ban / dedup / fraîcheur) tournent d'abord » : c'est inexact sur deux
points — ③ prend un verrou en **écriture** et **mute** l'état, et ④ n'est pas bon
marché (re-sérialisation complète).

**Preuve exécutée** (`audit_net::n5`, vert, release) :
```
N5 : 8 Mo non authentifiés -> 16 ms de travail côté victime (parse + id canonique +
     BLAKE3), AVANT toute vérification de signature et hors limiteur de débit
     (stats.dropped_rate_limit == 0)
```

**Impact chiffré.** 8 Mo envoyés (~0,2 s sur 400 Mbit/s) → 16 ms de CPU victime.
Le rapport CPU est modeste, mais **la boucle de dispatch est unique** : 62 messages
de 8 Mo par seconde (500 Mbit/s) la saturent totalement et déclenchent `Lagged`. Le
vrai levier reste R1 (×1 500) et R3 (relais pré-authentification).

**Correctif suggéré.** Vérifier la signature **immédiatement après** le parse
structurel, avant l'id canonique, avant le ban, avant toute statistique. Le seul
ordre défendable est : taille → parse → signature → tout le reste. Le commentaire de
`:416-421` décrit d'ailleurs un ordre (« 1. JSON, 2. dedup, 3. fraîcheur,
4. signature ») qui ne correspond plus au code — la documentation elle-même a
décroché.

---

### R7 — HAUT — Registre de pseudos : aucun plafond, coût quadratique, persisté

**Ancres** : `src-tauri/src/p2p/username.rs:309` (`apply` — aucune borne),
`:260` (`rebuild_by_pk`, O(n)), `:332` (appelé à chaque `Inserted`/`Replaced`),
`src-tauri/src/p2p/state_persistence.rs:38` (la clé `usernames` est persistée).

**Ce qui est faux.** `apply` n'a **aucun plafond** sur `by_name.len()`. Et chaque
insertion réussie déclenche `rebuild_by_pk()` qui reparcourt **tout** le registre.
L'espace de noms est `[a-z][a-z0-9_]{2,19}` — astronomique.

**Preuve exécutée** (`audit_net::n8`, vert, release) :
```
registre de       0 entrées -> une seule insertion coûte 191 µs
registre de  25 000 entrées -> une seule insertion coûte 4,28 ms
registre de  50 000 entrées -> une seule insertion coûte 9,53 ms
registre de 100 000 entrées -> une seule insertion coûte 21,0 ms
~10 671 o par enregistrement en JSON persisté => 1 017 Mo pour 100 000 pseudos
```
La croissance est bien **linéaire par insertion**, donc **quadratique au total**.

**Impact chiffré.** Un enregistrement pèse 10,7 Ko (clé ML-DSA 3 904 + signature
6 618 caractères hex). Avec 100 clés d'expéditeur (16,5 ms de fabrication) et
30 msg/min/clé, l'attaquant insère **3 000 pseudos/minute** :
- après 33 min : 100 000 entrées, **1 Go de RAM** et **1 Go de JSON réécrit sur
  disque toutes les 30 s** (`state_persistence.rs:99-104`, la clé `usernames` est
  sérialisée **en entier** dès qu'elle change) ;
- coût CPU cumulé du `rebuild_by_pk` : ~ n²/2 × coût unitaire ≈ **17 minutes de CPU**
  pour ces 100 000 insertions, sur la même tâche que le reste du nœud.

C'est donc simultanément une bombe mémoire, une bombe disque (amplification d'écriture :
10,7 Ko reçus → 1 Go réécrit) et une bombe CPU.

**Correctif suggéré.** Plafonner le registre (ex. 50 000 entrées) avec une éviction
déterministe ; remplacer `rebuild_by_pk` intégral par une mise à jour incrémentale de
`by_pk` ; et surtout faire payer la revendication (une transaction du ledger, cf. R2)
— la gratuité totale est ce qui rend le squat rationnel.

---

### R8 — HAUT — La limitation de débit est par expéditeur, sans plafond global

**Ancres** : `src-tauri/src/p2p/dispatcher.rs:247` (`check_rate_limit`),
`:29-40` (les constantes), `:46` (`MAX_TRACKED_SENDERS = 100_000`).

**Ce qui est faux.** Il n'existe, dans tout `src-tauri/src`, **aucune** limite globale
de débit, de bande passante ou de connexions sur le chemin P2P. Le seul `Semaphore`
du dépôt est dans `rpc.rs:79` (`RPC_MAX_INFLIGHT`), qui protège le JSON-RPC local, pas
le gossip. Le budget est de 30 à 120 messages/minute **par clé publique**, et le code
lui-même reconnaît (`dispatcher.rs:79-83`, `:295-298`) qu'« un attaquant frappant des
paires de clés (microsecondes chacune) » est le modèle de menace.

**Preuve exécutée** (`audit_net_b::b1` + `a1`, verts, release) :
```
B1 : 500 identités -> 15 000 messages admis dans la MÊME fenêtre de 60 s
     (plafond affiché : 30/pair/min). Aucun plafond global n'existe.
A1 : keygen ML-DSA-65 = 164,54 µs/clé  ;  sign = 455,6 µs  ;  verify = 160,5 µs
```

**Impact chiffré.** Un cœur fabrique **6 080 identités/s**. Chacune ouvre un budget de
30 messages/min. Le coût réel par message **authentifié** est la signature (455,6 µs),
donc un cœur produit **~2 190 messages signés/s** — tous admis, aucun plafond ne s'y
oppose. Face à cela le récepteur ne paie que 160 µs de vérification : sur le chemin
**authentifié**, l'asymétrie joue en faveur du défenseur (≈ 3:1), ce qui est correct.
**Toute la vulnérabilité tient donc dans les chemins où le nœud travaille avant de
vérifier** (R1, R3, R4, R6) : c'est là que l'asymétrie s'inverse à ×1 500.

Effet secondaire : `MAX_TRACKED_SENDERS = 100_000` est atteint en ~16 s de flood à
6 000 clés/s, ce qui déclenche en permanence le balayage O(n) de
`note_activity_and_prune` (`:186-224`) — la boucle `while` de `:211` cherche le minimum
sur 100 000 entrées **à chaque message excédentaire**, soit un O(n) par message. C'est
une deuxième bombe CPU (non mesurée séparément, mais visible à la lecture).

**Correctif suggéré.** Ajouter un plafond **global** (messages/s et octets/s pour le
nœud entier) et un plafond **par connexion iroh** (`EndpointId`, pas clé ML-DSA), qui
est la ressource réellement coûteuse à multiplier. Remplacer la recherche du minimum
par une structure d'éviction O(1) ou O(log n).

---

### R9 — MOYEN — Éclipse à l'amorçage : ordre de composition DHT « minable », aucune limite par source

**Ancres** : `src-tauri/src/p2p/rendezvous.rs:315-326` (`harvest` → `BTreeSet`),
`:361-381` (la boucle de composition), `:100` (`SLOTS = 32`),
`:108` (`MAX_IDS_PER_BOARD = 24`), `:112` (`MAX_DIALS_PER_CYCLE = 8`),
`:145` (`slot_signing_key` — clés publiques par construction).

**Ce qui est faux.** Le module documente honnêtement que **tout le monde détient la
clé privée de chaque slot** (`:41-44`) et conclut que « le rayon de souffle est
l'amorçage uniquement » et que « le refus du premier contact est le plafond de ce
qu'un attaquant DHT peut obtenir » (`:64-70`). **C'est faux : ce n'est pas seulement
un refus, c'est une substitution.**

Deux propriétés se combinent :
1. `harvest` agrège dans un `BTreeSet<[u8;32]>` puis retourne
   `found.into_iter().collect()` (`:315-323`) — donc **trié par ordre croissant des
   octets bruts de l'EndpointId**.
2. `cycle` parcourt cette liste dans l'ordre et compose les **8 premiers** inconnus
   (`:361-381`). Il n'y a **aucune** limite du nombre de pairs adoptés depuis une même
   source, aucune exigence de diversité, aucun mélange aléatoire.

**Preuve exécutée** (`audit_net::n10`, vert) : avec 100 pairs honnêtes aléatoires et
8 nœuds attaquants à préfixe nul dans le même ensemble, **les 8 premiers composés sont
les 8 attaquants**, à 100 %.
```
N10 : 32 slots × 24 entrées = 768 pairs publiables par un attaquant ;
      les 8 premières connexions vont aux EndpointId les plus petits (grindables)
```

**Chemin d'exploitation.** Un `EndpointId` est une clé publique Ed25519 : « miner » un
préfixe de 3-4 octets nuls coûte 2²⁴–2³² essais, soit quelques secondes à quelques
minutes sur un CPU. L'attaquant :
1. génère 8 identités iroh à préfixe minimal ;
2. écrit dans les **32 slots** (il en a la clé privée) un board contenant ses 8
   identités — `effective_seq` (`:233`) garantit même qu'il peut surenchérir
   indéfiniment sur les écrivains honnêtes ;
3. tout nœud froid harvest → trie → compose **exclusivement ses nœuds**.

**Impact.** Éclipse complète d'un nouveau nœud : il ne voit que la chaîne, les blocs,
les votes de finalité et les `@pseudo` que l'attaquant veut bien lui montrer. Un
paiement peut lui apparaître confirmé sur une chaîne qui n'est pas celle du réseau.
La contre-mesure NET-12 (`willow_node.rs:367`) ne sert à rien ici : elle regroupe par
préfixe de **clé ML-DSA**, pas d'EndpointId, et l'attaquant diversifie ses clés ML-DSA
gratuitement. Elle ne fait par ailleurs que **journaliser** (`gossip_tasks.rs:330-337`),
sans aucune action.

Atténuation réelle : les enveloppes restent authentifiées (un pair DHT n'a pas plus
d'autorité qu'un autre) et RDV-2 restaure le carnet de pairs à chaque redémarrage
(`state_persistence.rs:48`), donc un nœud **déjà** connecté n'est pas éclipsé par la
DHT. Le risque porte sur le **premier démarrage** — exactement le moment où
l'utilisateur reçoit ses premières pièces.

**Correctif suggéré.** Composer dans un **ordre aléatoire** (mélanger `discovered`),
plafonner le nombre de pairs adoptés **par slot** (ex. 2), et exiger une diversité de
`/24` IP ou d'ASN si l'information est disponible. Faire de NET-12 une action, pas un
journal.

---

### R10 — MOYEN — Saturer la table de signalement désactive le bannissement pour tous

**Ancres** : `src-tauri/src/p2p/dispatcher.rs:318-350` (`prune_reports_and_bans`),
`:336-349` (la boucle d'éviction), `:68` (`MAX_TRACKED_REPORTS = 10_000`).

**Vérification de la borne annoncée (SEC-REPORT-2)** : elle **tient**. La boucle
d'éviction n'évince que les cibles **sous le seuil** (`:340`) et `break` sinon (`:347`) ;
comme une cible sous le seuil est créée puis évincée immédiatement dès que le plafond
est atteint, `report_counts` ne dépasse jamais 10 000. Mesuré : 12 000 cibles fictives
injectées → 10 000 retenues.

**Ce qui est faux, c'est la conséquence.** Une fois les 10 000 places occupées par des
cibles **bannies**, plus **aucune** nouvelle cible ne peut atteindre le seuil : elle est
évincée avant son 3ᵉ rapporteur. Le mécanisme de bannissement — la seule réponse du
protocole face à un pair malveillant — devient **inopérant pour tout le réseau**.

**Preuve exécutée** (`audit_net_b::d1`, vert) :
```
D1 : table saturée à 10 000 cibles fictives (plafond annoncé 10 000)
D1 : après saturation, un vrai malveillant signalé 3× n'est PLUS bannissable
```

**Impact chiffré.** Coût attaquant : **3 clés ML-DSA** (494 µs) et 30 000 messages
`ReportPeer` sur des `peer_id` inventés. À 120 msg/min/clé (plafond haut) c'est
**83 minutes**, et rien n'empêche d'utiliser 100 clés pour le faire en **2,5 minutes**.
Coût mémoire côté victime : ~6 Mo épinglés en permanence. Renouvelable à l'infini
(TTL de 1 h).

Effet combiné avec R1 : l'attaquant peut d'abord bannir ses cibles (R1), puis saturer
la table pour que **plus personne ne puisse bannir l'attaquant**.

**Correctif suggéré.** N'accepter un `ReportPeer` que si `peer_id` correspond à un pair
effectivement présent dans `peer_info` (« je ne connais pas cette cible, je ne la
comptabilise pas »), et faire expirer les entrées sous-seuil par TTL plutôt que par
pression de cardinalité.

---

### R11 — MOYEN — Comptabilité par pair écrite avant la signature

**Ancre** : `src-tauri/src/p2p/dispatcher.rs:496-502`.

Le commentaire l'assume explicitement (« done before signature verification, so even
peers whose signature later fails contribute one "bad" message — useful for spotting
noisy peers »). Le problème est que `env.sender` est **choisi par l'attaquant** : ce
n'est pas le pair bruyant qui est comptabilisé, c'est **celui que l'attaquant
désigne**.

**Preuve exécutée** (`audit_net_b::c1`, vert) :
```
C1 : le pair HONNÊTE se voit imputer 410 734 o et 1 message par une enveloppe
     non authentifiée ; stats globales : 410 734 o reçus
```

**Impact.** Falsification des métriques NET-9 (`bytes_in`, `messages_in`), donc du
tableau « Réseau » de l'interface et de tout diagnostic opérateur. Pas de croissance
mémoire (l'entrée doit préexister), pas d'effet consensus. Sévérité **MOYEN** parce
que ces métriques sont ce sur quoi un opérateur s'appuierait précisément pendant une
attaque.

---

### R12 — MOYEN — Les 4 lanes de sortie sont non bornées

**Ancre** : `src-tauri/src/p2p/gossip_priority.rs:182-190`, commentaire `:19-21`
(« Each lane is unbounded … DoS protection lives at the dispatcher layer »).

La prémisse est fausse pour la lane `Low` : `ReportPeer` (R1) et `Pong` (R5) y sont
produits **depuis le chemin non authentifié / non limité**. Si le drain
(`gossip_tasks.rs:35-51`, qui `await` `sender.broadcast(...)` message par message)
est plus lent que le producteur, la file croît sans borne — ~10,7 Ko par entrée.
Non prouvé par test end-to-end (il faudrait un vrai endpoint iroh lent), mais c'est
une propriété structurelle de `mpsc::unbounded_channel` avec un producteur non
régulé. Atténuation de fait : la signature ML-DSA de `broadcast()` (456 µs) est
exécutée **dans** la tâche de dispatch, ce qui plafonne le producteur à ~2 200/s ;
si le drain descend sous ce débit, la croissance est réelle.

**Correctif** : lanes bornées avec abandon de la lane `Low` en surcharge.

---

### R13 — MOYEN — `Hello.known_peer_ids` et `Hello.heads` sans plafond de cardinalité

**Ancres** : `src-tauri/src/p2p/gossip.rs:166` et `:147` (les champs),
`src-tauri/src/p2p/dispatcher.rs:1109-1119`.

`known_peer_ids: Vec<String>` et `heads: Vec<String>` n'ont ni plafond de nombre ni
plafond de longueur de chaîne — seule la taille d'enveloppe (10 Mo) les borne.
`dispatcher.rs:1109` collecte **toute** la liste filtrée dans un `Vec` avant le
`.take(3)` de `:1123`, et interroge `our_known_peers.contains_key(id)` pour chacun.
Un `Hello` authentifié de 10 Mo de `known_peer_ids` provoque donc ~1 M de comparaisons
de chaînes et un `Vec` de ~1 M de `String`. `heads` n'est jamais utilisé que pour un
`.len()` dans un log (`:1034`) — champ mort qui n'en reste pas moins un vecteur
d'allocation. Post-authentification, donc borné par R8. Le tableau de reconnexion,
lui, est correctement plafonné (`MAX_KNOWN_PEERS = 1024`, `willow_node.rs:44`).

**Correctif** : plafonner à ~64 entrées de ≤ 64 octets, côté sérialisation.

---

### R14 — BAS — `wrap_outgoing` : code mort qui contredit `envelope_id`

**Ancres** : `src-tauri/src/p2p/gossip.rs:631-647`, en particulier `:637`
(`id = BLAKE3(payload_json)`) et `:645` (`nonce: 0`).

**Verdict : code mort, pas un contournement.** Vérifié par recherche exhaustive dans
`src-tauri/src` : les seuls appelants de `wrap_outgoing` sont `wrap_outgoing_with_nonce`
(`:685`), lui-même **sans aucun appelant**. Le module `p2p` est déclaré `mod p2p;`
(privé) dans `lib.rs:19`, donc aucun appelant externe n'est possible non plus.

Ce n'est donc **pas** un contournement des correctifs H1/H3 — c'est un piège : une
enveloppe produite par cette fonction est rejetée deux fois par un nœud v9
(id non canonique en `dispatcher.rs:466`, puis `nonce == 0` en `:601`). Si un futur
développeur la réutilise, ses messages disparaîtront **silencieusement** sur tout le
réseau, avec `stats.messages_sent` qui continue de monter — exactement la classe de
panne décrite dans GOSSIP-MTU-1.

**Preuve exécutée** (`audit_net::n9`, vert) : l'enveloppe produite par `wrap_outgoing`
a bien `nonce == 0`, un `id ≠ envelope_id(...)`, et `dispatch_incoming` la jette sans
incrémenter `messages_received`.

**Correctif** : supprimer `wrap_outgoing` et `wrap_outgoing_with_nonce`.

---

### R15 — BAS — Le `NonceTracker` n'est pas persisté

**Ancre** : `src-tauri/src/p2p/state_persistence.rs:32-49` (les 7 clés :
`ledger`, `reputation`, `consensus`, `gossip`, `usernames`, `finality_cast_memo`,
`known_peers` — **pas** `nonce_tracker`).

Conséquences : au redémarrage, les repères de nonce **reçus** (`last_nonces`) et les
bans repartent à zéro. La protection anti-rejeu retombe alors sur (a) la fenêtre de
fraîcheur ±90 s (`gossip.rs:528`) et (b) la LRU `seen_messages`, elle **persistée**
(`gossip.rs:701`). Il subsiste une fenêtre : les enveloppes vues durant les ≤ 30 s qui
précèdent un **crash** (l'arrêt propre fait une sauvegarde finale,
`state_persistence.rs:60-62`, `:132`) ne sont pas dans la LRU restaurée et sont donc
rejouables si elles sont encore fraîches. Impact faible : le rejeu d'un `BroadcastTx`
est arrêté par le dedup de transaction du ledger, celui d'un `Hello`/`Ping` est
inoffensif. Bénéfice inattendu : les **bans** disparaissent aussi au redémarrage, ce
qui atténue partiellement R1.

**Réponse à la question posée** : le nonce **sortant** est, lui, persisté
(`gossip.rs:707`) *et* réamorcé sur l'horloge en microsecondes (`gossip.rs:473-481`) —
c'est un bon correctif (voir §4). L'horloge de fraîcheur est celle du **récepteur**
(`gossip.rs:515` : `chrono::Utc::now()`), l'attaquant ne contrôle que le `timestamp`
qu'il déclare, et il doit tomber dans ±90 s de l'horloge locale ; il n'y a pas de NTP,
donc une dérive d'horloge locale est une panne (le projet le documente honnêtement en
`gossip.rs:356-365`), pas une faille.

---

### R16 — BAS — `is_banned` prend un verrou en écriture sur le chemin non authentifié

**Ancre** : `src-tauri/src/p2p/dispatcher.rs:449-458`.

`is_banned` (`:354`) mute (`bans.remove`, `report_counts.remove`), donc exige
`nonce_tracker.write()`. Ce verrou est pris pour **chaque** message entrant, y compris
non authentifié, avant tout autre traitement. C'est un point de sérialisation global
qu'un attaquant sans identité peut solliciter à volonté, et il est partagé avec le
limiteur de débit (`:580`) et le contrôle de nonce (`:600`).

**Correctif** : séparer une lecture pure (`bans.get(...)` + comparaison de temps) de
l'éviction paresseuse, et ne prendre l'écriture que sur le cas rare.

---

### R17 — BAS — Éviction O(n) avec clones dans le buffer de fork

**Ancre** : `src-tauri/src/p2p/fork_heal.rs:170-195`.

Quand le buffer est plein (`FORK_BUFFER_MAX_BLOCKS = 1024`), chaque bloc offert
déclenche un balayage complet avec `hash.clone()` **par bloc bufférisé** (`:176-177`).
Un `ChainSegment` de 50 blocs rejetés coûte donc ~51 200 allocations de `String`.
Post-authentification et borné ; signalé pour l'exhaustivité. Le reste du module est
correctement borné (cap global + cap par index `FORK_BUFFER_MAX_PER_INDEX = 8`, et
l'inversion d'éviction sous attaque a manifestement déjà été corrigée — voir le
commentaire H5 en `:156-169`, qui est un bon raisonnement adversarial).

---

## 4) Ce qui est solide

Le propriétaire doit savoir ce qui **tient**, et pourquoi.

1. **Le correctif H1/H3 sur l'identifiant d'enveloppe tient.** `envelope_id` est
   BLAKE3 de la pré-image **signée** (`gossip.rs:596-604`), le récepteur le recalcule
   et rejette tout id non canonique (`dispatcher.rs:466`), et l'insertion dans la LRU
   est déplacée **après** la signature (`:570`). Conséquence : **la LRU de dédup n'est
   pas empoisonnable** par un non authentifié. *Vérifié par test* (`audit_net_c::e3`) :
   un attaquant qui vise l'id exact d'un futur `RequestChain` honnête échoue, et le
   message honnête passe ensuite normalement. C'est la réponse nette à la question
   « cache poisoning » : **non**, plus depuis H1. La LRU est bornée (100 000,
   `gossip.rs:376`) et persistée avec son ordre d'éviction — correct.

2. **PRESIG-ORDER tient : aucune carte par expéditeur n'est écrite depuis un
   `sender` usurpé.** Le limiteur de débit et le tracker de nonce sont tous deux
   après la porte de signature (`:581`, `:601`), donc `last_nonces` et `rate_counters`
   ne peuvent pas être gonflés par un attaquant sans clé. Les tests du projet
   (`presig_bad_signature_writes_no_per_sender_state`) le vérifient réellement.

3. **L'espace de nonce d'un pair honnête n'est PAS épuisable par un tiers.**
   *Vérifié par test* (`audit_net::n11`) : une enveloppe forgée avec `nonce = u64::MAX`
   au nom d'une victime ne pousse pas son repère — elle meurt à la signature. Combiné
   au réamorçage horloge de `next_outgoing_nonce` (`gossip.rs:473-481`), qui règle
   proprement le cas « restauration de portefeuille sur une machine neuve », c'est un
   bon design. Réponse nette à la question 3 : **non**.

4. **`peer_key()` (H6) est un vrai correctif.** Hacher les clés ML-DSA de 3 904
   caractères en digest BLAKE3 de 64 avant de les utiliser comme clés de map
   (`dispatcher.rs:89-91`) divise par ~61 le pire cas mémoire. L'éviction bornée du
   `NonceTracker` (expiration puis borne absolue, `:186-224`) est correcte et
   l'argument anti-rejeu qui l'accompagne (TTL 120 s > fenêtre 90 s) est juste.

5. **Le parsing d'entrée hostile de la DHT est propre.** `decode_board`
   (`rendezvous.rs:179-199`) ne fait jamais confiance au compteur déclaré, le recoupe
   avec la longueur réelle, plafonne à `MAX_IDS_PER_BOARD`, rejette la clé nulle, et
   ne fait **aucun** `Vec::with_capacity` piloté par l'attaquant. `effective_seq`
   (`:233-238`) sature au lieu de déborder. Recherche exhaustive : **aucun
   `Vec::with_capacity(n)` du dépôt n'est piloté par une valeur distante**.

6. **Pas de débordement de pile par imbrication JSON.** *Vérifié par test*
   (`audit_net_c::e2`) : 5 000 niveaux d'imbrication produisent une erreur `serde_json`,
   jamais une panique. La limite de récursion de `serde_json` fait son travail.

7. **Le plafond de taille brute est appliqué avant le parse** (`dispatcher.rs:429`),
   et le point d'entrée fuzz `try_process_raw_gossip` / `validate_envelope_at`
   (`:959`, `:976`) est une bonne pratique (validateur pur, temps injecté, jamais de
   panique).

8. **La liaison cryptographique `@pseudo → adresse` est correcte.**
   `address_hex_binds_key_hex` (`username.rs:145`) empêche de revendiquer un pseudo
   pour l'adresse d'autrui avec sa propre clé. Le trou de R2 est purement temporel.

9. **Le durcissement des transactions à l'entrée est bon.** `handle_broadcast_tx`
   refuse explicitement les `Slash` (`dispatcher.rs:1180`) et les `Mining`
   (`:1191`) arrivant par gossip, et n'applique une transaction que via le jeton
   `VerifiedTx` (`:1202`) — un « type-state » qui rend la vérification de signature
   non contournable par une édition future. C'est du bon design.

10. **Les assainissements d'entrée existent et sont testés** :
    `sanitize_country_code` (`dispatcher.rs:141`, espace de clés borné à 64 codes),
    `sanitize_display_name` (`gossip.rs:270`, contrôle-caractères et 32 octets,
    troncature sur frontière UTF-8).

11. **Le buffer de réconciliation de fork est borné et déterministe**
    (`fork_heal.rs:74`, `:79`, `:170-195`), avec un raisonnement d'attaque explicite
    et correct sur l'inversion d'éviction.

12. **Le JSON-RPC (hors périmètre, mais surface distante)** est lié à `127.0.0.1`,
    protégé par un jeton cookie en `0600` et une vérification `Origin`/`Host`
    (`rpc.rs:147-238`), avec un sémaphore d'inflight (`rpc.rs:79`). C'est la seule
    partie du code où une limite globale de concurrence existe.

13. **La qualité de la documentation interne est réellement au-dessus de la moyenne.**
    Beaucoup de commentaires décrivent l'attaque que le code ferme, avec le raisonnement.
    Le défaut symétrique est que plusieurs de ces commentaires sont désormais **faux**
    (`dispatcher.rs:416-421` décrit un ordre de pipeline obsolète ; `:541-546` qualifie
    de « bon marché » un contrôle qui re-sérialise 10 Mo ; `rendezvous.rs:64-70` sous-estime
    l'éclipse ; `dispatcher.rs:56-59` prétend qu'il faut « N parties prenantes
    indépendantes » alors qu'une identité coûte 165 µs). **Un commentaire de sécurité
    faux est pire qu'absent** : il fait passer une revue.

---

## 5) Ce que je n'ai PAS pu vérifier, et pourquoi

1. **R3 en conditions réelles.** Je n'ai pas monté deux nœuds iroh vivants pour mesurer
   la croissance effective du `TimeBoundCache` de plumtree sous flood de 10 Mo.
   L'analyse repose sur la lecture de `iroh-gossip-0.98.0/src/proto/plumtree.rs:490`
   (`on_gossip` : cache + eager_push + lazy_push **avant** `EmitEvent`),
   `proto/util.rs:317-336` (`TimeBoundCache` = `HashMap` + `TimerMap`, **aucune borne
   de taille**) et `willow_node.rs:597` (10 Mo). Ces trois lectures sont sans ambiguïté,
   mais je marque **non prouvé expérimentalement** le chiffre exact du OOM.

2. **R12 (lanes non bornées) end-to-end.** Il faudrait un endpoint iroh dont le
   `broadcast` est artificiellement lent. Propriété structurelle établie par lecture ;
   **non prouvée par mesure**.

3. **La fragmentation MTU** : la question posée supposait une fragmentation ou une
   compression. Il n'y en a **aucune** — vérifié par recherche exhaustive de
   `fragment`, `chunk`, `mtu`, `reassembl` dans `src-tauri/src` (seuls résultats :
   `ledger/mod.rs:1344` pour l'arbre de Merkle et `rpc.rs:260` pour un tampon HTTP).
   Le « correctif » est le relèvement du plafond décrit en R3. Il n'y a donc **pas** de
   réassemblage de fragments à attaquer, pas de fragments orphelins, pas de mélange
   d'expéditeurs : ces trois questions sont **sans objet**, et c'est la réponse.

4. **Le comportement sous partition réelle / N nœuds.** Les amplifications O(N²) de R5
   sont établies analytiquement à partir du fait prouvé « la réponse est un broadcast »,
   pas mesurées sur un maillage de N nœuds. Le facteur N² lui-même est une déduction,
   solide mais non instrumentée.

5. **Le contenu de `finality_live.rs`, `pos_consensus.rs`, `ledger/` et
   `mining_loop.rs`** est hors de mon périmètre (auditeur réseau). Je n'ai lu de ces
   fichiers que les points d'entrée appelés depuis le dispatcher. En particulier je
   **n'affirme rien** sur la validité de `integrate_remote_block`,
   `verify_block_slashes`, `ingest_vote` ou `reorg_to_fork` — un défaut là-dedans
   changerait l'impact de R9 (éclipse) de « censure » à « double dépense ».

6. **La couche TLS/QUIC post-quantique** (`X25519MLKEM768` via rustls
   `prefer-post-quantum`, `Cargo.toml`) n'a pas été testée : je n'ai pas capturé de
   poignée de main. Je note seulement, d'après le `Cargo.toml` lui-même, que
   **l'identité de nœud reste Ed25519** — le NodeId n'est donc pas post-quantique,
   contrairement à l'enveloppe. C'est documenté honnêtement par le projet.

7. **Les 20 tests de preuve** ont été exécutés dans une **copie** du dépôt
   (`/tmp/qaudit_net`) pour ne modifier aucun fichier suivi par git, avec
   `CARGO_TARGET_DIR=/tmp/qtarget_net_rel`. Ils ne sont pas dans l'arbre du projet.
   Pour les rejouer :
   `cp -r` du dépôt, ajouter `mod audit_net_tests;` dans `src/p2p/mod.rs`, puis
   `cargo test --release --lib audit_net -- --nocapture`.
   Résultat obtenu : **20 passed, 0 failed** (profil `--release`).
   Source des tests et sortie complète conservées dans
   `/tmp/quanta_audit_reseau_preuves/audit_net_tests.rs` et
   `/tmp/quanta_audit_reseau_preuves/resultats_release.txt`.
   Contrôle final : `git status --porcelain` sur `/Users/alex/Desktop/Quanta` est
   **vide** — aucun fichier suivi n'a été modifié.

---

## Annexe — mesures brutes (profil `--release`, machine de l'audit)

```
ML-DSA-65 : keygen 164,54 µs | sign 455,61 µs | verify(valide) 160,45 µs
            verify(invalide) 157,50 µs | |pk|hex 3 904 o | |sig|hex 6 618 o
Enveloppe Ping signée complète : 10 724 o     Enveloppe ReportPeer : 14 680 o
R1  : 1 µs attaquant -> 1,583 ms victime            = amplification CPU ×1 500
R1  : 10 724 o entrants -> 14 680 o signés émis/pair = ×1,37 (×6,85 sur 5 pairs eager)
R3  : max_message_size 10 485 760 o vs défaut 4 096 o = ×2 560 ; cache plumtree 30 s non borné
R4  : 46 660 o gzip -> 48 000 001 o -> 16 000 000 String = 366 Mo de tas, 226 ms  (×2 950)
R5  : Ping 10 724 o -> Pong diffusé 10 739 o ; RequestChain 10 772 o -> segment 27 851 o
R6  : 8 Mo non authentifiés -> 16 ms de CPU avant la signature, hors limiteur
R7  : insertion de pseudo : 0,19 ms @0 | 4,28 ms @25k | 9,53 ms @50k | 21,0 ms @100k
      10 671 o/enregistrement -> 1 017 Mo pour 100 000 pseudos, réécrits toutes les 30 s
R8  : 500 identités -> 15 000 messages admis dans la même fenêtre de 60 s
R10 : 3 clés + 30 000 messages -> bannissement définitivement inopérant pour tout le réseau
```
