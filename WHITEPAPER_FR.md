# Quanta — Monnaie souveraine pair-à-pair

> **Whitepaper — Quanta v3.3**
> La monnaie que tu forges, que tu possèdes, et que personne ne peut t'enlever.
> Pas de serveur. Pas de banque. Pas d'autorité d'émission. Pas de censeur.

> **État d'implémentation.** Quanta est un logiciel alpha de recherche. Ce document décrit
> le protocole tel que conçu et largement implémenté ; pour la répartition précise
> « réel vs expérimental vs pas-encore », voir le tableau d'état du [README](README.md).
> Rien ici n'est une promesse de sécurité de production ni de valeur monétaire actuelle.
> QUANTA n'est coté sur aucune bourse et n'a aucun prix de marché ; le projet n'en invente aucun.

---

## 1. Pourquoi Quanta

La monnaie d'aujourd'hui est contrôlée par des autorités que tu n'as pas choisies :

| Problème | Effet |
|---|---|
| Les émetteurs créent à volonté | Ton épargne est diluée en silence |
| Les comptes vivent sur le serveur d'un tiers | Ils peuvent être gelés, annulés, fermés |
| Des intermédiaires sont dans chaque transfert | Une commission est prise, une trace gardée |
| La garde est déléguée à une plateforme | « Tes » coins sont une reconnaissance de dette que tu ne détiens pas |

**Quanta inverse le contrat.** C'est une monnaie **rare, à plafond dur**, que tu mines en
gardant un nœud en ligne, que tu gardes avec **tes propres clés**, et que tu envoies en
pair-à-pair. Pas de société, pas de serveur à assigner, pas de clé admin, et aucune
autorité capable de l'inflater, de te geler, ou de signer à ta place.

---

## 2. Le coin QUANTA — invariants

| Paramètre | Valeur |
|---|---|
| Plafond dur | **100 000 000 QUANTA**, vérifié au consensus — jamais dépassable |
| Émission | **décroissante** : chaque minute frappe `(plafond − miné) / 50 000 000` µQTA |
| Rythme à la genèse | ≈ **120 QUANTA / heure**, décroissant doucement vers le plafond |
| Premine / autorité d'émission | **aucun** — personne ne peut créer du QUANTA hors de la règle |
| Unité | 1 QUANTA = 1 000 000 µQTA (`u64`, arithmétique entière déterministe, zéro flottant) |
| Burn | **1% détruit à chaque transfert** (burn-and-mint) |

**La rareté est le cœur.** Le *rythme* d'émission est front-loaded mais borné : chaque tick
libère une fraction fixe de l'offre **restante**, donc le rythme baisse à mesure qu'on
approche du plafond, et le total émis tend asymptotiquement vers — sans jamais atteindre —
100 000 000. La borne est vérifiée deux fois au consensus : un plafond d'émission **par
bloc** et le plafond dur **global**, pour qu'un pair malveillant ne puisse ni dépasser le
cap, ni rafler une année d'émission en un seul bloc.

Deux nuances honnêtes. (1) « Front-loaded » qualifie le *rythme* (maximal à la genèse, ne
fait que baisser) — **pas** les montants absolus, qui prennent des siècles à approcher le
plafond :

| Échéance | Offre cumulée (approx.) |
|---|---|
| Rythme genèse | ≈ 120 QUANTA/h ≈ 1,05 M/an, décroissant |
| An 1 | ≈ 1,05 M (~1% du plafond) |
| An 10 | ≈ 10 M (~10%) |
| ~66 ans | 50 M (la moitié) |
| ~219 ans | 90 M (90%) |
| → ∞ | approche sans jamais atteindre 100 M |

(2) Le burn de 1% ne rend l'offre **nette** déflationniste **qu'au-delà d'un seuil de volume
de transferts** — quand le burn dépasse l'émission. À faible volume, l'émission domine et
l'offre croît encore vers le plafond. On ne prétend pas à une déflation inconditionnelle.

**Sur la valeur.** Miner coûte de l'électricité réelle, mais un coût de production n'est pas
un prix. QUANTA n'a aujourd'hui aucune bourse ni valeur de marché ; une valeur d'échange
n'existera que si des gens choisissent librement de l'échanger. L'app n'affiche jamais de
chiffre fiat inventé.

---

## 3. Comment tu gagnes du QUANTA — minage par contribution

Garder un nœud Quanta en ligne, *c'est* miner. Une fois par minute, le réseau frappe
l'émission du tick et la distribue selon la **contribution mesurée**, via une pondération
fixe. Elle s'*inspire* des axiomes d'efficience (somme des parts = 1) et de symétrie (nœuds
identiques → parts égales) de la valeur de Shapley, mais c'est un score de contribution
linéaire O(n) — **pas** un calcul de Shapley exact (NP-difficile). Les poids :

```
énergie 30% · travail 30% · validation 25% · uptime 15%   (somme = 1,0)
```

L'énergie est **mesurée localement** (RAPL Intel/AMD, `powermetrics` Apple, ou un repli
sysinfo calibré) — jamais auto-déclarée comme un nombre invérifiable. Le terme *travail*
suit aujourd'hui le ratio d'énergie (pas de marché de calcul dans la version crypto-only),
donc en pratique les récompenses suivent l'énergie mesurée, la validation et l'uptime. En
solo, tu reçois le tick entier ; avec des pairs, ta part est proportionnelle à ta
contribution mesurée, multipliée par un facteur anti-sybil. Pas de classe de mineurs
privilégiée, pas de course à l'armement matériel : un laptop laissé en ligne contribue.

---

## 4. Le ledger

- **Les blocs** sont scellés par le leader du slot environ toutes les 2 minutes ; chacun
  porte une racine de Merkle BLAKE3 des IDs de ses transactions.
- **Les transferts** sont signés Ed25519 ; le destinataire reçoit 99%, 1% est brûlé, et
  l'expéditeur est débité du montant total — le tout en `u64` µQTA, donc aucune dérive.
- **Le cache de solde** est O(1) (`HashMap` incrémental), mis à jour à l'application et
  reverté en cas de reorg.
- **Anti-replay** : nonce strictement monotone par compte + ensemble `seen_tx_hashes`.
- **La résolution de fork est déterministe** : on valide le challenger avant toute mutation,
  on pop le tip perdant, on remet en attente les transactions exclusives à la branche
  perdante, puis on applique la gagnante — aucun bloc validé n'est jamais perdu en silence.
- **La synchronisation de chaîne** est paginée (`RequestChain → ChainSegment`, ≤50
  blocs/segment) et reprend depuis n'importe quelle hauteur.

---

## 5. Protocole réseau & sécurité

La seule unité sur le fil est un `GossipEnvelope` signé. Les octets bruts ne sont jamais de confiance.

- **La signature** couvre `(sender, nonce, timestamp, payload)` canoniquement — jamais le payload seul.
- **Le nonce** est strictement monotone par expéditeur (commence à 1), donnant un anti-replay par pair.
- **Le timestamp** doit être frais (fenêtre ±90 s) ; le même timestamp est signé et envoyé.
- **L'ID de message** est `BLAKE3(payload)` — déterministe, permettant la déduplication.

Chaque message entrant passe un pipeline fixe avant tout handler :

```
garde de taille (≤10 Mo) → décode JSON → vérif bannissement → dédup (LRU 100K)
  → fraîcheur timestamp (±90s) → rate limit adaptatif
  → nonce anti-replay (≥1, strictement monotone) → vérif signature Ed25519 → handler
```

Défenses : rate limiting adaptatif par pair (échelle `sqrt`), bannissement (3 reports → 1 h),
plafonds DoS (10 Mo/envelope, 50 blocs/segment), et une heuristique d'éclipse qui alerte
quand trop de pairs partagent un préfixe de clé publique.

---

## 6. Consensus — Proof-of-Stake, élection vérifiable pondérée par le stake

La production de blocs est par leader et déterministe par slot (= hauteur de chaîne) :

```
beacon = BLAKE3(domaine ‖ bloc_enterré_hash ‖ slot)   (enterré = plusieurs slots derrière le tip)
seed   = BLAKE3(domaine ‖ beacon ‖ slot ‖ round)
leader = seed % stake_total_pondéré                    (poids = stake seul — enjeu on-chain, ADR-002)
```

Le stake minimum de validateur est 1 QUANTA. Si le leader élu ne scelle pas dans un timeout
de 30 s, la production bascule vers le suivant (rounds bornés) ; quand personne n'a staké, le
bootstrap est sans permission. L'entropie vient d'un bloc **enterré** (plusieurs slots
derrière le tip), pas du tip frais — le validateur qui vient de sceller ne peut donc pas
grinder sa propre ré-élection.

**Honnêteté de nommage.** C'est une élection *déterministe et publiquement vérifiable* —
**pas** un VRF cryptographique : aucune composante à clé secrète, donc le leader d'un slot
futur est publiquement prévisible (une surface de DoS ciblé). Un vrai VRF à clé secrète
(imprévisibilité) et un VDF (résistance au grinding) sont au roadmap, pas livrés.

---

## 7. Cryptographie

| Couche | Mécanisme |
|---|---|
| Identité / signatures | **Hybride Ed25519 + ML-DSA-65** (NIST FIPS 204), actif sur la couche transaction |
| Dérivation de clé | Argon2id (64 Mio, 3 itérations, parallélisme 4) |
| Chiffrement au repos | AES-256-GCM (nonce unique de 12 octets par opération) |
| Hachage / content-addressing | BLAKE3 |
| Sûreté mémoire | `zeroize` + `ZeroizeOnDrop` sur chaque secret |

**Post-quantique — actif (hybride).** Chaque **transaction** est signée par une signature
hybride **Ed25519 + ML-DSA-65** (NIST FIPS 204) via la crate autonome `fips204` (Rust pur,
temps constant, zéro `unsafe`). La clé ML-DSA est **dérivée de la graine Ed25519** (XOF
BLAKE3), donc aucun secret supplémentaire n'est persisté et aucune migration de coffre n'est
nécessaire. La vérification est **strictement ET** quand une couche PQ est présente — forger
exige de casser *les deux* schémas — avec un repli Ed25519 pour les signatures
pré-activation. Les enveloppes gossip restent en Ed25519 (transport éphémère, fenêtre de
fraîcheur ±90 s, déjà à l'intérieur de QUIC/TLS) ; un jour-drapeau *require-PQ* (`REQUIRE_PQ`)
à l'échelle du réseau est un futur bump de protocole.

---

## 8. Identité & auto-custodie

Ton identité est une paire de clés — rien de plus, rien de loué. On te joint par un court
**`@pseudo`** (et un code de connexion à usage unique), pas par un domaine qu'il faut payer en
continu. La clé privée ne quitte jamais l'appareil : elle vit dans un coffre chiffré
(Argon2id + AES-256-GCM) et c'est la seule chose qui peut déplacer tes fonds. Une **clé de
récupération** affichée une seule fois à la création est l'unique moyen de restaurer le
compte sur un autre appareil.

Aucun KYC, aucun tracking, aucun compte à fermer. Tu détiens les clés ; personne ne peut
signer à ta place, et personne ne peut geler, annuler ou confisquer ce que tu détiens.

---

## 9. Modèle de menace & limites honnêtes

- **Non audité par un tiers.** La cryptographie et le réseau n'ont eu aucun audit indépendant.
- **Réseau à l'échelle alpha.** La convergence a été vérifiée entre deux machines physiques,
  pas à grande échelle. NAT traversal à grande échelle, résilience aux partitions et
  résistance à l'éclipse sont en cours.
- **L'anti-sybil est un proof-of-concept.** Quanta résiste aux attaques sybil via la
  pondération par réputation, le poids de stake et le rate limiting — mais il n'y a pas
  encore de puzzle d'admission proof-of-work/stake durci, et la couche gossip elle-même
  n'est pas filtrée anti-sybil. L'heuristique d'éclipse ne détecte que les attaquants
  *paresseux* (pairs partageant un préfixe de clé) ; la vraie résistance à l'éclipse exige
  diversité IP/AS et pairs d'ancrage persistants — au roadmap.
- **Le consensus converge mais n'est pas encore économiquement final.** L'élection du leader
  est publiquement prévisible (pas de VRF à clé secrète) et il n'y a **pas de slashing** de
  l'équivocation : la résolution de fork fait *converger* le réseau, mais rien ne *pénalise*
  économiquement un leader qui signe deux blocs à la même hauteur. Considère les confirmations
  profondes comme plus fortes, jamais comme finales, tant que finalité BFT + slashing ne sont
  pas livrés.
- **Aucune valeur monétaire réelle.** QUANTA est expérimental et non coté. Ne stocke pas une
  valeur que tu ne peux pas perdre.

On documente ça parce qu'un protocole qui cache ses limites ne peut pas être digne de
confiance pour ce qu'il fait bien.

---

## 10. Feuille de route

Audit de sécurité externe · tests multi-nœuds durcis (chaos & partitions) · pipeline de
release signé + notarisé · finalité BFT sub-seconde
([design DAG-BFT](docs/DESIGN-CONSENSUS-DAG-BFT.md) — un DAG de *consensus*, sans rapport
avec le DAG de contenu social retiré lors de la refonte crypto-only) · aléa d'élection
durci par VDF ·
jour-drapeau *require-PQ* à l'échelle du réseau · admission anti-sybil durcie · gouvernance
on-chain des paramètres économiques. L'UI est internationalisée (EN · FR · ES · RU · ZH · JA).

---

## 11. Conclusion

Quanta est une monnaie souveraine : rare par la règle, minée par la contribution, détenue
par toi seul, et déplacée en pair-à-pair sans autorité au milieu. Le plafond dur et
l'émission décroissante sont gravés dans le code et vérifiés au consensus ; les clés sont
les tiennes ; le réseau n'a pas de propriétaire. Le moteur est réel et testé ; le réseau est
jeune et ouvert. Tout est libre (Apache-2.0) — le code est ouvert, les règles vivent dans le
protocole, et la gouvernance future sera on-chain.

<p align="center"><strong>◈ Quanta — La rareté que tu forges ◈</strong></p>
