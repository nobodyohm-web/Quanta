# Audit interne complet — Quanta v3.12.0

**Date** : 2026-07-25 · **Commit** : `bd2f18e` · **Branche** : `feat/frontend-rebuild-clean`
**Méthode** : huit auditeurs spécialisés en parallèle, chaque constat soumis à des sceptiques
indépendants chargés de le réfuter. 44 agents, 42 constats bruts, 20 passés au filtre adversarial,
**17 survivants** (dont un doublon inter-axes), 3 réfutés. Plafonds atteints : 11 constats moyens
n'ont pas pu être vérifiés faute de budget, 11 bas/imperfections sont listés sans vérification.
Les cinq constats les plus lourds ont été **revérifiés à la main sur le code source** avant d'entrer
ici. Les découvertes hors fan-out (§7) sont de la même main.

---

## 0. État des corrections (mis à jour le 2026-07-25)

Tout ce qui suit a été corrigé, chaque correctif accompagné d'un test qui échouait
avant lui. Suite finale : **473 tests + 1 intégration deux-nœuds**, clippy
`--all-targets` silencieux, svelte-check 0/0.

| Constat | État | Commit |
|---|---|---|
| C1 auto-équivocation du validateur | **fermé** | `58b4696` |
| C2 émission illimitée par expéditeur synthétique | **fermé** | `ba89597` |
| C3 `Unstake` non borné par l'enjeu bondé | **fermé** | `7f3dfa0` |
| C4 RPC monnaie sans authentification | **fermé** | `cd1a969` |
| H1 + H3 identifiant d'enveloppe / dedup avant signature | **fermé** | `5b3accb` |
| H2 halt de finalité par éviction inversée | **fermé** | `e5eb5ba` |
| H4 relecture de couverture ignorant les slashes | **fermé** | `f09b90d` |
| H5 tampon de réconciliation épinglable | **fermé** | `3b33413` |
| H6 cartes de pairs non bornées | **fermé** | `ab9d4ee` |
| H7 injection HTML dans l'explorateur | **fermé** | `716b05c` |
| H8 annulation de bloc non-inverse | **fermé** | `0d93c19` |
| M1 délais et plafonds RPC | **fermé** | `cd1a969` |
| M2 arbre de blocs reconstruit à chaque vote | **fermé** | `540ebd7` |
| M3 inventaire cryptographique mensonger | **fermé** | `441b47f` |
| M4 Touch ID survivant à une restauration | **fermé** (sans test — voir §5) | `93fe20f` |

Les changements de règle d'admission (C2, C3, H2, H1/H3) sont groupés derrière
`TORUS_PROTOCOL_VERSION` 6→7 : un nœud v6 et un nœud v7 n'acceptent pas le même
ensemble de blocs et d'enveloppes.

**Vague 2 (2026-07-25, hors périmètre du fan-out).** Traités ensuite : le déclencheur
`issue_comment` de `claude-review.yml` restreint aux `OWNER`/`MEMBER`/`COLLABORATOR`
et rétrogradé en `contents: read` (c'était une *pwn request* ouverte sur un dépôt
public) ; `permissions: contents: read` explicite sur la CI ; `libsql` sans features
par défaut (**8 → 4** avis RUSTSEC, cf. rectificatif §2) ; `npm audit fix` (la haute
`postcss` corrigée, restent 3 basses qu'on refuse de payer par un retour à
SvelteKit 0.0.30) ; endpoint de mise à jour et métadonnées repointés de `Torus` vers
`Quanta` ; origines Google Fonts retirées de la CSP ; **phrase BIP39 désormais
zeroizée** (feature `zeroize` de la caisse, `ZeroizeOnDrop` sur `Mnemonic`) ;
`cipher::decrypt` ne panique plus sur un nonce malformé et respecte son contrat
d'erreur opaque ; `getmultisigaddress` rejette un seuil hors bornes au lieu de le
tronquer ; `MAX_CHAIN_SEGMENT` dérivé d'une source unique ; règle projet du thème
alignée sur le mode sombre réellement livré.

**Tentative écartée, sciemment.** Retirer `script-src 'unsafe-inline'` de la CSP a
été essayé puis **annulé** : le HTML produit par `adapter-static` contient des
scripts d'amorçage inline, donc la directive stricte empêche l'application de
démarrer. Une CSP stricte demanderait des nonces ou des hashes côté SvelteKit —
c'est une contrainte de la chaîne de build, pas un oubli, et elle reste ouverte.

**Reste ouvert et le demeure explicitement** : les onze constats moyens jamais passés
au filtre adversarial, le reste de l'annexe, et l'intégralité des angles morts du §8
— au premier rang desquels `username.rs` et `sm/node.rs`, que personne n'a lus, et
l'absence de tout invariant de vivacité. Aucun audit externe n'a eu lieu.

---

## 1. Verdict

Le noyau est sain et l'ingénierie est réelle : 449 tests passent sans une seule exception, clippy
`--all-targets` est parfaitement silencieux, `svelte-check` rend 0 erreur et 0 avertissement sur
316 fichiers, la simulation déterministe rejoue à l'octet près sur 128 exécutions. Les chiffres
annoncés dans `CLAUDE.md` sont exacts. Ce n'est pas un projet qui bluffe.

Mais « parfaitement parfait », non. L'audit a trouvé **quatre défauts critiques**, dont trois
touchent directement l'argent et un le fait d'une manière que personne n'attend : le client se
punit lui-même. Le motif est constant et il vaut plus que la liste des bugs — **les règles sont
justes, et ce sont leurs exemptions qui fuient**. Chaque fois qu'un chemin dit « ce cas-là est
spécial, on le saute », le garde d'à côté ne sait pas que le cas est passé. Les tests couvrent
magnifiquement les chemins nominaux et à peu près pas les combinaisons d'exemptions.

Le second constat structurel est de portée : la surface la plus dangereuse n'est plus le consensus,
c'est **le nœud sans tête**. Le RPC monnaie, écrit à la main et livré il y a six jours, n'a aucune
authentification, aucun délai de lecture, aucun plafond de connexions et sert un explorateur qui
injecte du HTML non échappé. Le consensus a reçu neuf ADR et cinq vagues d'audit ; le RPC, zéro.

---

## 2. Les faits — ce que la machine répond

| Vérification | Résultat |
|---|---|
| `cargo test` | **449 passés, 0 échec** (40,9 s) + 1 test d'intégration deux-nœuds réel |
| `cargo clippy --all-targets` | **aucun avertissement, aucune erreur** |
| `svelte-check` | **316 fichiers, 0 erreur, 0 avertissement** |
| `cargo audit` | **8 vulnérabilités**, 24 avertissements tolérés |
| `npm audit` | **4 vulnérabilités** (1 haute, 3 basses) |

Les huit vulnérabilités Rust sont toutes transitives, aucune dans le code Quanta. Leur composition
mérite pourtant un regard : **quatre sont dans `rustls-webpki 0.102.8`** — panic atteignable au
parsing de CRL, contraintes de noms acceptées à tort sur des wildcards et des URI, CRL mal
rattachées à leur point de distribution. Deux touchent `quick-xml` (allocation non bornée, temps
quadratique), deux `hickory` (boucle non bornée sur validation NSEC3, encodage O(n²)). `CLAUDE.md`
annonçait « 8 vulns transitives évaluées » en v3.11 : le compte est identique mais les identifiants
RUSTSEC sont datés de mars à juin 2026 — **ce ne sont pas les mêmes huit**, et l'évaluation
documentée portait donc sur un jeu périmé.

> **Rectificatif (2026-07-25, après lecture de l'arbre de dépendances).** La première rédaction de
> ce paragraphe affirmait que les quatre avis `rustls-webpki` touchaient « le chemin TLS du
> transport QUIC, celui-là même durci en PQ-TRANSPORT-1 ». **C'était faux**, et c'est l'erreur la
> plus significative de ce rapport. `cargo tree` montre qu'iroh tire `rustls-webpki 0.103.13`,
> la version corrigée ; la 0.102.8 vulnérable venait de `libsql` via `hyper-rustls`, donc de sa
> pile HTTP distante — jamais exercée, la base étant ouverte en `Builder::new_local`. Le transport
> post-quantique n'a jamais été concerné. Passer `libsql` en
> `default-features = false, features = ["core"]` retire cette pile morte : le compte tombe de
> **8 à 4**. Les quatre restantes viennent toutes d'iroh — `quick-xml` par la lecture de
> configuration réseau macOS, `hickory` par le résolveur DNS — dette amont, non actionnable ici.

Côté npm, la haute est `postcss` (lecture de fichier arbitraire via `sourceMappingURL`), corrigeable
sans casse par `npm audit fix`. Les trois basses viennent de `cookie` tiré par `@sveltejs/kit` et
leur « correctif » rétrograderait SvelteKit en 0.0.30 — à ignorer sciemment, en le documentant.

---

## 3. Critiques

### C1 — Un validateur honnête s'auto-équivoque et se fait brûler la totalité de son enjeu
`src-tauri/src/p2p/finality_live.rs:373`

`build_vote_to_cast` est appelé à **chaque tick de minage de 60 s** (`mining_loop.rs:151`, hors de
la porte `SEAL_EVERY_N_TICKS`) et reconstruit le vote de zéro : `source` = le dernier checkpoint
justifié *courant*, `target` = le checkpoint de l'époque du tip sur la chaîne *courante*. Le seul
garde est `if target.height <= source.height { return None }`. **Rien n'enregistre « j'ai déjà voté
pour l'époque N ».** J'ai vérifié : `grep` sur `last_vote|voted_epoch|slashing_protection` dans tout
`src-tauri/src/` ne rend rien. Il n'existe aucune mémoire anti-slashing dans ce programme.

Deux déclencheurs, aucun n'exige d'adversaire. Le premier : le tip est à une hauteur frontière
d'époque, un bloc concurrent gagne le départage lexicographique à hauteur égale (`reorg.rs:362` —
un aboutissement *prévu* du fallback PoS), la hauteur ne bouge pas mais le hash du checkpoint change ;
au tick suivant le validateur signe un second vote pour la même époque cible. Le second : un
certificat tardif complète un lien pendant, `source` avance, la cible ne bouge pas — deuxième vote,
même époque. Dans les deux cas `detect_fault` (`finality_slashing.rs:115`) classe l'ensemble en
`Fault::DoubleVote`, chaque pair diffuse une preuve **cryptographiquement valide**, et la sanction
est un **brûlage intégral** (SLASH_NUM/SLASH_DEN = 1/1) de l'enjeu bondé *et* en déverrouillage.

Le sceptique a resserré la fenêtre — elle se situe dans les premiers ticks après une frontière
d'époque, avant que le certificat local ne se forme — sans la refermer. Un adversaire peut d'ailleurs
la provoquer à volonté en publiant un bloc concurrent lexicographiquement gagnant à une hauteur
frontière, pour brûler l'enjeu des votants honnêtes. C'est le seul défaut de cette liste où le
programme se retourne contre son propre utilisateur.

**Correctif** : une base anti-slashing persistée — pour chaque époque cible, le vote exact déjà
signé ; `build_vote_to_cast` ne rend un vote que s'il est identique octet pour octet au vote stocké,
ou si l'époque cible est strictement supérieure à la dernière votée. C'est le mécanisme standard des
clients Ethereum, et son absence transforme le client en son propre attaquant.

### C2 — Émission illimitée par l'expéditeur synthétique
`src-tauri/src/p2p/ledger/validation.rs:44`, `:287`, `:699`

Trois gardes protègent la création de monnaie, et un `Transfer` dont le `from` est la chaîne
`"NETWORK"` ou `"ESCROW"` passe les trois. `verify_tx` rend `Ok(true)` sans condition pour ces
expéditeurs — vérifié ligne 44, aucune restriction de type de transaction. `uncovered_tx_indices`
saute le débit de couverture pour eux (`if !synthetic(&tx.from)`, ligne 699) : aucun solde n'a
besoin d'exister derrière le mouvement. Et `validate_block_emission_against` ne somme que les
transactions de type `Mining` puis **retourne `Ok(())` immédiatement si ce total vaut zéro**
(lignes 287-293). La règle coinbase EMIT-1 filtre elle aussi sur `TxType::Mining` (ligne 581) : elle
ne voit jamais la transaction.

Un attaquant qui a bondé 1 QUANTA — ou qui scelle pendant le bootstrap permissionless — scelle un
bloc contenant une seule transaction `Transfer` de `"NETWORK"` vers sa propre adresse, pour cent
millions de QUANTA, sans signature. Le Merkle et le hash sont recalculés honnêtement, donc
BLK-HASH-1 passe. Sur chaque nœud honnête, `validate_block_against_prev` accepte : le proposeur est
bondé, le vecteur `mining` est vide donc la règle coinbase est sautée, `verify_tx` rend vrai ligne 44,
la couverture ne réclame rien, l'émission calcule zéro. `cache_apply_tx` crédite le destinataire et
ne débite personne. Les coins sont pleinement dépensables et `total_mined` ne les compte jamais — les
vues d'offre continuent d'afficher le chiffre honnête pendant que la monnaie a été créée.

Le projet **sait déjà** que ce trou existe : `sim.rs:1726` teste qu'une libération ESCROW sans
verrou casse la conservation. Mais cet invariant n'est vérifié que par le contrôleur de la simulation,
jamais par une règle de consensus. Et les dix tests adversariaux de `ledger/tests.rs` forgent tous
la transaction synthétique avec `TxType::Mining` — aucun n'essaie `Transfer`.

**Correctif** : dans `validate_block_against_prev`, rejeter tout bloc non-genèse contenant une
transaction dont le `from` est `"NETWORK"` ou `"ESCROW"`, sauf l'unique coinbase déjà validée par
EMIT-1 (`tx_type == Mining && from == "NETWORK" && to == block.miner`). `escrow_release_to` n'a
aucun appelant hors tests, donc `ESCROW` peut être interdit comme expéditeur, purement et simplement.
Symétriser dans `seal_block_at` pour préserver l'invariant COVER-2.

### C3 — Un `Unstake` n'est jamais confronté à l'enjeu réellement bondé
`src-tauri/src/p2p/ledger/validation.rs:688` et `src-tauri/src/p2p/ledger/stake.rs:51`

`uncovered_tx_indices` sort en `continue` sur `Unstake` avec le commentaire « no spendable debit to
cover » — exact, mais **aucune autre règle ne vérifie que `tx.amount <= bonded[tx.from]`**. Vérifié
ligne par ligne : `apply_block_stake_effects` fait `bonded.saturating_sub(tx.amount)` puis pousse
inconditionnellement `UnbondEntry { amount: tx.amount, .. }` — le montant **verbatim**, jamais clampé
à ce qui a réellement été retiré. La seule vérification de montant bondé du dépôt vit dans
`unstake_tx_at` (`stake.rs:366`), c'est-à-dire dans le *constructeur local*, que tout nœud modifié
n'exécute simplement pas.

Un attaquant sans enjeu signe un `Unstake` valide de dix millions de QUANTA depuis sa propre adresse.
La signature est authentique — elle porte sur son propre compte — donc `verify_tx` l'accepte.
`handle_broadcast_tx` ne rejette que `Slash` et `Mining`. La transaction entre dans le mempool de
chaque nœud honnête. Le prochain leader honnête la scelle, car COVER-2 ne l'exclut pas. Dix mille
quatre-vingts blocs plus tard, `mature_unbonding` crédite dix millions de QUANTA dépensables qui
n'ont jamais été minés. Le puits STAKE part profondément négatif, et le `.max(0)` de
`locked_stake_total()` **efface silencieusement la divergence** — le clamp que §4 défend comme une
sûreté de cast devient ici le tapis sous lequel la fabrication disparaît. Pire, `onchain_spendable_before`
rejoue la même logique permissive, donc COVER-1 sur chaque nœud confirme que l'attaquant possède ces
coins fantômes.

**Correctif** : une règle jumelle de COVER, séquentielle sur une carte `bonded` courante, qui marque
tout `Unstake` dépassant l'enjeu bondé de son émetteur ; câblée des deux côtés — rejet dans
`validate_block_against_prev`, exclusion dans `seal_block_at` — exactement comme COVER-1/COVER-2.
Et clamper l'`UnbondEntry` créé au montant effectivement retiré.

### C4 — Le RPC monnaie n'a aucune authentification, et le navigateur suffit à vider le portefeuille
`src-tauri/src/rpc.rs:178`

Vérifié par grep exhaustif : **aucune occurrence** de `Authorization`, de jeton, de cookie, d'`Origin`
ou de `Host` dans tout `rpc.rs`. `handle_conn` accepte n'importe quel POST /, désérialise le corps et
dispatche. L'unique garde est `if public && public_denied(method)` — c'est-à-dire que la protection
n'existe **que** dans le mode lecture seule, et pas du tout dans le mode par défaut, celui du nœud
souverain documenté dans `QUICKSTART.md` : `QUANTA_WALLET_PASSWORD=… quanta-node --mine`, portefeuille
ML-DSA chargé, `sendtoaddress` actif.

Le bind par défaut sur `127.0.0.1:8645` porte le commentaire « Money RPC is never exposed to the open
internet by default ». C'est vrai pour le réseau et faux pour le navigateur, qui est déjà à
l'intérieur du périmètre. Comme aucun `Content-Type` n'est exigé, un `fetch()` depuis n'importe quelle
page web ouverte sur la même machine est une requête CORS *simple* : pas de préflight, le navigateur
l'envoie, le serveur parse le JSON quoi qu'il arrive. L'attaquant ne lit pas la réponse — il n'en a
pas besoin, l'effet de bord est le vol. Et comme `Host` n'est jamais validé, la même attaque marche
en DNS rebinding contre un nœud que l'attaquant ne peut pas router directement.

**Correctif** : le modèle `.cookie` de Bitcoin Core — un jeton aléatoire écrit dans le répertoire de
données au démarrage, comparé en temps constant sur toute méthode mutante. Indépendamment : exiger
`Content-Type: application/json` (l'explorateur l'envoie déjà, donc c'est gratuit) et rejeter un
`Origin` présent et non identique, ce qui tue à lui seul le chemin CSRF ; valider `Host` contre
l'adresse de bind pour fermer le rebinding.

---

## 4. Hauts

**H1 — Le dedup s'écrit avant la vérification de signature, avec un identifiant choisi par
l'attaquant.** `dispatcher.rs:427`. L'insertion dans le LRU de 100 000 entrées a lieu à l'étape ④,
la signature ML-DSA n'est vérifiée qu'à l'étape ⑧ — et le commentaire ligne 435 assume ce choix
explicitement. Or `env.id` est une `String` libre venue du fil, que rien ne recalcule. Un pair non
authentifié insère donc des clés de dedup arbitraires gratuitement, et le limiteur de débit ne peut
rien puisqu'il tourne *après* la porte de signature. Composé avec H3, l'attaque devient chirurgicale :
l'attaquant précalcule les identifiants des futurs `RequestChain` de chaque plage de hauteur et les
empoisonne d'avance — **la synchronisation de chaîne est censurable par un inconnu, pour rien**.
*Correctif* : déplacer `mark_seen` après la vérification (en gardant une sonde `contains` en lecture
seule avant, si le délestage précoce compte), et rejeter tout identifiant qui n'est pas le BLAKE3 des
octets canoniques.

**H2 — Un validateur à 1 µQTA peut arrêter la finalité de tout le réseau, définitivement.**
`finality_live.rs:279`. Le plafond du pool de certificats évince `pool.keys().next()`, soit la clé
`BTreeMap` la plus basse, donc **le lien de plus basse époque source**. Le commentaire affirme que
c'est « le plus périmé » — mais l'époque est entièrement choisie par l'attaquant : `link_well_formed`
n'exige qu'une hauteur multiple de 32 et une cohérence interne, rien ne borne l'époque contre la
hauteur réelle de la chaîne. Les liens de l'attaquant portent donc les clés les plus hautes et
survivent, tandis que le lien honnête — d'époque courante, donc basse — est évincé à chaque insertion.
Coût de l'attaque : un `Stake` de 1 µQTA et 4096 votes bien formés. *Correctif* : rejeter à l'ingestion
tout vote dont l'époque cible dépasse celle de la hauteur de chaîne courante, évincer par récence
d'insertion plutôt que par ordre de clé, et borner les liens pendants par validateur.

**H3 — L'identifiant de message est BLAKE3 de la charge utile seule.** `gossip.rs:484`. Ni
l'expéditeur, ni le nonce, ni l'horodatage n'entrent dans l'identifiant, alors que la signature les
couvre tous les trois. Deux enveloppes de charge identique partagent donc une seule case de dedup,
sur tout le réseau et pour toujours — le LRU est persisté dans le snapshot. Or `RequestChain{from,max}`
et `Ping{nonce}`/`Pong{nonce}` sont exactement des charges sans entropie propre : le second nœud qui
demande la même plage voit sa requête silencieusement jetée, et ne synchronise jamais. *Correctif* :
calculer l'identifiant sur `signable_envelope_bytes` — les octets mêmes que la signature couvre.

**H4 — `onchain_spendable_before` ignore la consommation d'unbonding d'un `Slash`.**
`validation.rs:366`. Le semeur unique de la règle de couverture reconstruit le pool de déverrouillage
depuis les `Unstake` et le fait mûrir hauteur par hauteur, mais saute les `Slash` d'un `continue`.
Or LIVE-3B détruit précisément des entrées d'unbonding. Le chemin vivant les supprime, la relecture
de validation les conserve : à l'échéance, la règle de couverture recrédite des coins que la chaîne a
déjà brûlés. Les deux vues de la même chaîne divergent définitivement. *Correctif* : appliquer la
ventilation `slash_unbonding` dans la relecture, avec une assertion de non-dérive en `cfg(test)`
comme celle qui existe déjà pour `staked_before`.

**H5 — Le tampon du `ForkReconciler` s'épingle avec des blocs de pacotille.** `fork_heal.rs:148`.
`offer` admet tout bloc qui échoue l'intégration linéaire, sans aucune validation, et applique le
plafond de 1024 en évinçant l'index *le plus haut* tout en refusant tout nouveau venu d'index
supérieur. Un tampon rempli de 1024 blocs de bas index qui ne gagneront jamais est donc **stable**,
et LIVE-4 — seul appelant vivant de `reorg_to_fork` — est éteint. Vingt et un messages
`ChainSegment` suffisent. *Correctif* : évincer par utilité et non par index, plafonner par index et
par expéditeur, borner en octets, et exiger une validation minimale (hash, Merkle, proposeur bondé)
avant d'accorder une place.

**H6 — Les cartes du `NonceTracker` sont indexées par des clés ML-DSA de 3,9 Ko et l'ensemble des
rapporteurs n'a aucun plafond.** `dispatcher.rs:262`. `prune_reports_and_bans` teste le nombre de
*cibles*, jamais le nombre de rapporteurs : une cible avec un million de rapporteurs ne déclenche
jamais rien. PQ-ENVELOPE-1 amplifie le problème — `MAX_TRACKED_SENDERS = 100_000` a été dimensionné
pour des clés Ed25519 de 64 caractères, il indexe désormais des clés hexadécimales de 3904 octets.
L'attaquant génère des paires de clés (quelques microsecondes chacune) et envoie un `ReportPeer`
parfaitement légitime par clé ; le limiteur par expéditeur ne se déclenche jamais puisque chaque clé
ne sert qu'une fois. *Correctif* : indexer par un condensé court, plafonner l'ensemble des
rapporteurs par cible (trois suffisent pour bannir), compter les plafonds en octets.

**H7 — Injection HTML stockée dans l'explorateur web.** `explorer.html:177`. Vérifié : `esc()` existe
et protège le type de transaction, les hash, le mineur, l'horodatage — mais les champs `from` et `to`
passent par `short()`, qui **rend la chaîne verbatim quand elle fait 18 caractères ou moins**. Le
champ `to` n'est jamais contraint à une forme d'adresse : `verify_tx` lie `from` à la clé ML-DSA et
couvre `to` dans le préimage signé, sans lui imposer aucune structure. Une transaction d'1 µQTA vers
`to = "<base href=//a.co>"` — exactement 18 caractères — injecte un élément `<base>` dans le document
de quiconque consulte ce bloc, et détourne dès lors toutes les requêtes relatives, y compris les
appels RPC de l'explorateur lui-même. Le budget de 18 caractères se contourne par concaténation sur
plusieurs transactions du même bloc. *Correctif* : envelopper chaque interpolation dans `esc()`, et
contraindre `to` au niveau du protocole (64 hexadécimaux ou un puits synthétique connu).

**H8 — `revert_block_stake_effects` n'est pas l'inverse de son application.** `stake.rs:141`.
L'annulation parcourt les transactions dans l'ordre *direct* au lieu de l'ordre inverse. Les
opérations n'étant pas commutatives — `saturating_sub` suivi de l'effacement de clé à zéro —
annuler dans l'ordre direct ne défait pas appliquer dans l'ordre direct dès qu'un même bloc contient
un `Stake` et un `Unstake` du même compte. Les deux chemins de reorg appellent cette fonction, donc
un réarrangement laisse de l'enjeu bondé fabriqué. *Correctif* : itérer `.rev()`. Le constat a été
remonté « moyen » et relevé en « haut » par le sceptique, à raison — c'est de l'enjeu créé.

---

## 5. Moyens

**M1 — Le RPC n'a ni délai de lecture ni plafond de connexions.** `rpc.rs:115`. Deux auditeurs
indépendants ont trouvé ce défaut, ce qui est plutôt bon signe pour la méthode. `read_request` attend
sans échéance et `serve` engendre une tâche par connexion sans sémaphore : un client muet immobilise
une tâche et un descripteur pour toujours. À la limite de descripteurs, `accept()` rend EMFILE, et la
branche d'erreur reboucle immédiatement — une boucle chaude qui brûle un cœur et inonde le journal,
en affamant le minage et le gossip sur le même runtime. C'est le mode `--public`, celui documenté
comme « exposable sans risque ». *Correctif* : `tokio::time::timeout` autour de `handle_conn`, un
`Semaphore` avant `accept()`, et une pause de 50 ms sur erreur d'acceptation.

**M2 — Chaque vote entrant reconstruit tout l'arbre de blocs, sous le verrou d'écriture de finalité.**
`finality_live.rs:183`. `observe_chain` parcourt la chaîne du bloc 1 au tip à chaque appel, avec
trois opérations `BTreeMap` et trois allocations de chaîne par bloc, et le dispatcher l'appelle sur
**chaque** `FinalityVote` accepté. Sur une chaîne de 100 000 blocs, cent pairs livrant leur quota
légitime produisent de l'ordre du milliard d'opérations par minute, sérialisées derrière le verrou
que le tick de minage réclame aussi. *Correctif* : un curseur incrémental — l'arbre est append-only
sur une chaîne linéaire, donc `last_observed+1..height` est exact.

**M3 — L'écran de sécurité de l'application affirme des algorithmes faux.** `crypto_agility.rs:38`.
`CryptoBOM::current()`, étiqueté « used for audit reporting », code en dur `signing: Ed25519 / RFC 8032
/ quantum_safe: false` et `key_exchange: X25519 / PendingMigration`. Les deux sont faux depuis
PQ-MIG-3B, PQ-ENVELOPE-1 et PQ-TRANSPORT-1. Ce n'est pas du code mort : `HelpModal.svelte` le rend à
l'utilisateur. L'application dit donc à qui la consulte — utilisateur, auditeur, intégrateur
d'échange — que ses signatures sont l'exacte primitive classique que toute la migration a retirée.
Au regard de la règle zéro-fake du projet, c'est l'affirmation la plus embarrassante du dépôt.
*Correctif* : réécrire l'inventaire sur la réalité (ML-DSA-65 / FIPS 204 actif ; entrée distincte
pour l'Ed25519 de transport, honnêtement marquée dette amont ; X25519MLKEM768 actif), idéalement
dérivé des constantes réelles plutôt que recopié.

**M4 — Le déverrouillage Touch ID survit à la création ou à la restauration d'identité.**
`identity.rs:198`. Ni `create_wallet` ni `restore_wallet` n'efface la ligne `biometric_wrap_v1` ni le
KEK du trousseau. L'ancien enrobage survit à la nouvelle identité : `biometric_status` annonce
« activé », l'utilisateur pose son doigt, Touch ID réussit, l'AES-GCM échoue sur le nouveau coffre —
et **chaque tentative consomme le backoff anti-force-brute partagé avec le mot de passe**. Le scénario
est exactement celui pour lequel RECOVER-1 existe : l'utilisateur a oublié son mot de passe et restaure
depuis sa phrase. *Correctif* : réutiliser le corps de `disable_biometric_unlock` à la fin des deux
commandes.

---

## 6. Ce que le filtre adversarial a tué

Trois constats bien argumentés n'ont pas survécu, et cette section vaut autant que les précédentes :
elle documente des protections réelles qu'un futur auditeur croira absentes.

Le **plancher de finalité poussé une seule fois sans reprise** : la lecture du flot de contrôle était
juste, mais le plancher **est** persisté dans le snapshot (`ledger/mod.rs:1513` et `:1539`), donc la
jambe « nœud frais à plancher zéro » du scénario était fausse.

Le **quorum multisig non borné** (10⁶ vérifications ML-DSA par message) : la forme du code est réelle,
la boucle est bien non plafonnée, mais l'auditeur avait pris `MAX_RAW_ENVELOPE_BYTES = 10 Mo` pour
budget de fil alors que le vrai plafond est le `max_message_size` d'iroh-gossip, laissé à 4096 octets
par défaut. Erreur d'un facteur 2500. Le durcissement reste souhaitable ; la criticité, non.

L'**ancêtre source→cible jamais vérifié sur la chaîne** : l'observation littérale est exacte — aucun
code ne vérifie que la cible d'un certificat descend de sa source — mais les deux conséquences
avancées sont fausses, donc le défaut tel qu'énoncé n'existe pas. À noter tout de même comme
question ouverte de conception.

---

## 7. Hors fan-out — trouvé en vérifiant les angles morts

Ces trois points ne venaient d'aucun axe. Ils sont sortis de l'agent chargé de nommer ce que personne
n'avait regardé, et je les ai vérifiés moi-même.

**Le canal de mise à jour pointe vers l'ancien nom du dépôt.** `tauri.conf.json:46` interroge
`github.com/nobodyohm-web/Torus/releases/latest/download/latest.json`, et `package.json` déclare le
même dépôt en `repository` et `homepage`, alors que l'origine réelle est `nobodyohm-web/Quanta`. Cela
ne fonctionne aujourd'hui que par la redirection GitHub des dépôts renommés, et cette redirection
**disparaît le jour où un dépôt nommé `Torus` réapparaît sous ce compte**. Il faut être juste sur la
gravité : la clé publique minisign est bien présente, donc un détournement de l'endpoint ne permet
pas d'exécuter du code — la signature du bundle tient. L'impact réel est un déni de mise à jour et
une incohérence de marque sur le canal le plus sensible du produit. À corriger, sans dramatiser.

**La CSP autorise `script-src 'unsafe-inline'` et Google Fonts.** `tauri.conf.json:27`. Le
`unsafe-inline` affaiblit la défense en profondeur : une seule injection DOM devient exécution de
script. Et la CSP autorise encore `fonts.googleapis.com` et `fonts.gstatic.com` alors que — vérifié —
**plus aucun fichier de `src/` ou `static/` ne les appelle** : les polices ont bien été localisées.
C'est donc du legacy à nettoyer, mais du legacy qui contredit la doctrine : une monnaie « sans
serveur, sans cloud, sans intermédiaire » dont la politique de sécurité liste Google comme origine
autorisée, c'est une phrase qu'un auditeur externe relèvera.

**Le workflow `claude-review.yml` accorde `contents: write` sur `issue_comment`.** Sur un dépôt public,
c'est la forme classique de la *pwn request* : le déclencheur est un commentaire contenant `@claude`,
et je n'ai pas vu de vérification d'`author_association` dans les lignes lues. À examiner avant tout
autre chose de la chaîne CI — c'est le seul endroit de cet audit où un inconnu peut potentiellement
écrire dans le dépôt.

---

## 8. Ce que cet audit n'a pas couvert

Le plafond de vérification a laissé **onze constats moyens sans passage devant les sceptiques** : le
`RequestChain` répondu par diffusion complète (amplification N-voies), les tâches de composition
engendrées sans plafond depuis les `known_peer_ids` d'un pair, une inversion d'ordre de verrous entre
`state.crypto` et `state.db` sur les chemins création/restauration contre déverrouillage, un scan
sysinfo complet exécuté sur le runtime asynchrone sous verrou d'écriture, un échec d'abonnement au
topic gossip qui laisse le nœud afficher « Connecté » alors qu'il est muet, le fait que les « watts »
annoncés et récompensés soient une **constante d'inactivité codée en dur** plutôt qu'une mesure, deux
DoS de lecture sur `listtransactions` et `gettransaction`, une chaîne de statut en français en dur
rendue dans les six langues, un badge de mode de nœud alimenté par un champ qui n'existe pas côté
backend, et le fait que `GossipMessage` compte **onze variantes** quand `CLAUDE.md` en documente neuf
dans son tableau et dix dans sa prose. Onze constats bas ou imperfections sont également listés sans
vérification, dont la phrase BIP39 — le secret qui contrôle les fonds — **jamais zeroizée** parce que
la caisse `bip39` est compilée sans la fonctionnalité `zeroize`.

Au-delà des plafonds, l'agent de complétude a nommé des pans entiers que la découpe en huit axes
n'atteignait pas. Parmi les fichiers que **personne n'a ouverts** : `username.rs` (587 lignes) — un
registre vivant, joignable par gossip, dont l'ingestion, la re-liaison de propriété après rotation de
clé, les homoglyphes et la croissance non bornée sont inaudités ; `sm/node.rs` (2029 lignes), le cœur
`Event→Effect` sur lequel repose toute la revendication de déterminisme ; les tests eux-mêmes
(`ledger/tests.rs`, `security_tests.rs`) — personne n'a posé la question inverse, celle de savoir si
ces tests affirment la bonne chose ; `AuthGate.svelte` et `WalletStake.svelte`, deux fichiers
critiques pour l'argent et la sécurité ; toute la surface ACL Tauri (`capabilities/default.json`) ;
et la liste de suppressions RustSec de `deny.toml`, jamais relue.

Et neuf classes de menace sont structurellement absentes de la découpe : intégrité de la chaîne de
construction, parcours de sauvegarde et de récupération de clé de bout en bout, chemin de migration
et de retour arrière entre versions de protocole, données au repos et permissions de fichiers,
hypothèses d'horloge (il n'y a pas de NTP, et un nœud désynchronisé jette silencieusement tout le
gossip), théorie des jeux économique au-delà de l'arithmétique, **liveness** — aucun invariant de
vivacité n'est affirmé nulle part —, vie privée et graphe de liaison adresse↔pseudo↔IP, et le fait
que la pile réseau réelle sous le protocole n'a reçu zéro relecture.

Enfin, trois coutures inter-axes que personne n'a possédées, dont une qui compte plus que les autres :
**le cœur pur `sm/` et le câblage vivant `p2p/` sont censés implémenter les mêmes règles, et personne
n'a audité la couture.** C'est exactement là qu'un désaccord de consensus se logerait.

---

## 9. Ce que je retiens

Quatre choses à faire avant tout le reste, dans cet ordre : la base anti-slashing (C1), parce que le
programme punit ses propres utilisateurs honnêtes ; les deux trous d'émission (C2, C3), parce qu'ils
créent de la monnaie et que le plafond de cent millions est la promesse centrale du projet ;
l'authentification du RPC (C4), parce qu'un utilisateur qui suit le guide de démarrage documenté et
ouvre ensuite une page web perd son portefeuille.

Le reste se traite calmement. Mais le motif mérite une réponse structurelle et pas seulement quatre
correctifs : **chaque exemption d'une règle doit être une valeur nommée, partagée par le producteur
et le validateur, et testée dans ses combinaisons.** Les trois défauts d'argent de cet audit sont le
même défaut vu sous trois angles — une exemption locale que le garde d'à côté ne connaît pas.
