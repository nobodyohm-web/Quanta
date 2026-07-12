---
type: design
status: implémenté en simulation DST (GADGET-1→5B, 2026-06-25) — câblage vivant en cours (LIVE-1 fait)
decision-class: 🛑 hard-stop (sous-paramètres §12) — tranchés par ADR-009
socle: ADR-001 → ADR-005, ADR-009
ancrage: Casper FFG (Ethereum)
updated: 2026-07-12
---

# Gadget de finalité Quanta — style Casper FFG, post-quantique, par époque

← [[00 — Pilotage QUANTA]] · cadre : [[DESIGN-CONSENSUS-DAG-BFT]] (Option 1 — Phase 1)
Socle ADR : [[ADR-001 — Fork-choice]] · [[ADR-002 — Validator set & comité BFT]] · [[ADR-003 — Slashing (accountable safety)]] · [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]] · [[ADR-005 — Agrégation des votes & certificats de finalité]]

> [!abstract] Statut — implémenté et prouvé en simulation DST (2026-06-25)
> L'orfèvrerie : le protocole raisonné de bout en bout, puis **construit**. Il s'ancre
> sur **Casper FFG** (le gadget de finalité d'Ethereum, à sûreté responsable démontrée), adapté
> à Quanta : votes **ML-DSA** ([[ADR-005 — Agrégation des votes & certificats de finalité]]),
> finalisation **par époque**, et **vérifié en continu par le harnais DST** existant. Les
> arguments de sûreté/vivacité (§7-8) restent des **esquisses rigoureuses, pas des preuves
> formelles** : la formalisation et l'audit externe restent devant (§13). Mais l'architecture
> décrite ici est **réalisée** — chaque étape du §14 a son fichier :
>
> | Étape | Fichier | Contenu |
> |---|---|---|
> | GADGET-1 | `src-tauri/src/sm/finality.rs` | époque/point de contrôle, `EPOCH_LENGTH_BLOCKS = 32` |
> | GADGET-2 | `src-tauri/src/sm/finality_vote.rs` | `Vote` signé ML-DSA + `MlDsaCertificate` (quorum ⅔) |
> | GADGET-3 | `src-tauri/src/sm/finality_rule.rs` | justification/finalisation, `FinalityState` |
> | GADGET-4 | `src-tauri/src/sm/finality_slashing.rs` | `detect_fault` / `FaultProof` / `apply_slash` |
> | GADGET-5A/B | `src-tauri/src/sm/fork_choice.rs` + `Ledger::reorg_to_fork` | LMD-GHOST `ghost_head` + résolution de partition |
>
> Les 4 méta-décisions §12 sont **tranchées** (ADR-009). Le câblage réseau vivant (gossip des
> votes, LIVE-1) est **fait** ; voir [[DESIGN-LIVE-WIRING]] pour la suite (LIVE-2, LIVE-3).

## 1. Ce sur quoi on construit

Quanta a déjà : une **chaîne linéaire** de blocs, une production en **preuve d'enjeu** avec
leader élu via le **beacon** ([[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]]), une
validation partagée `validate_block_against_prev`, et un **harnais de simulation déterministe**
(`src-tauri/src/sm/`) qui vérifie **sûreté, conservation et émission** à travers des centaines
de scénarios fautés. Le gadget se **superpose** à cette chaîne : il ne remplace pas la
production de blocs, il y ajoute une couche de **finalité déterministe**.

L'idée centrale de Casper, qui colle exactement à la décision
[[ADR-005 — Agrégation des votes & certificats de finalité]], est que la finalité opère sur des
**points de contrôle** aux **frontières d'époque**, pas bloc par bloc. C'est précisément ce qui
rend les certificats post-quantiques gérables.

## 2. Vocabulaire : époques et points de contrôle

La chaîne est découpée en **époques** de `E` blocs. La frontière de chaque époque est un **point
de contrôle** (checkpoint), identifié par `(hauteur de frontière, hash du bloc)`. Le bloc de
genèse est le point de contrôle initial, **finalisé par définition**.

Tout le mécanisme raisonne sur la **suite des points de contrôle** — un objet bien plus petit
que la suite des blocs.

## 3. Les votes (attestations)

À chaque époque, chaque validateur du comité émet **un** vote, qui est un **lien** entre deux
points de contrôle :

```
vote = (source, cible, époque)   signé ML-DSA, pondéré par l'enjeu du validateur
```

où `source` est un point de contrôle **déjà justifié** (§4) et `cible` un point de contrôle
descendant. Le vote dit : « depuis cette histoire que je tiens pour acquise, je soutiens cette
nouvelle frontière ». La signature **ML-DSA** rend le vote **post-quantique** et, crucialement,
en fait une **preuve** : un vote signé est une attestation **non répudiable** — c'est ce qui
fonde le slashing du §7.

## 4. Justification et finalisation (la règle en deux temps)

Deux seuils, tous deux à **deux tiers de l'enjeu** (quorum BFT classique, qui tolère moins d'un
tiers de fautifs) :

- Un **lien super-majoritaire** `source → cible` existe quand des validateurs totalisant **≥ ⅔
  de l'enjeu** ont voté ce même lien.
- Un point de contrôle `cible` devient **justifié** s'il existe un lien super-majoritaire depuis
  un point de contrôle **déjà justifié** vers lui (la genèse est justifiée d'office).
- Un point de contrôle `c` devient **finalisé** quand `c` est justifié **et** qu'il existe un
  lien super-majoritaire de `c` vers son **enfant direct** (le point de contrôle de l'époque
  suivante). Autrement dit : **deux époques consécutives correctement liées** scellent la
  première. Finalisé = **irréversible**.

Cette règle en deux temps est le cœur de Casper. Sa **simplicité** est ce qui rend la sûreté
démontrable.

## 5. Le certificat d'époque (post-quantique, ADR-005)

Le **certificat de finalité** d'une époque est l'**ensemble des votes ML-DSA** formant le lien
super-majoritaire qui justifie la frontière. **Un** certificat par époque (pas par bloc) — de
l'ordre de **165 Ko pour cinquante validateurs**, amorti sur `E` blocs et **élagable** une fois
l'époque profondément finalisée. Pas de BLS, pas d'ancrage, **un seul système cryptographique**,
exactement comme tranché en [[ADR-005 — Agrégation des votes & certificats de finalité]]. Le
certificat est rangé derrière l'**abstraction de certificat** de l'ADR-005, pour qu'une
agrégation future (BLS, SNARK PQ) reste un **remplacement local**.

## 6. Le fil rouge avec la production de blocs

La production de blocs (leader élu, chaîne linéaire) **continue** sans attendre la finalité :
elle donne la **vivacité** et le débit. Le gadget tourne **derrière**, au rythme d'époque, et
**scelle** rétroactivement l'histoire. D'où **deux régimes** : une tête de chaîne rapide mais
encore **probabiliste**, et une histoire finalisée **absolue** derrière elle. C'est exactement
la séparation « finalité récente / finalité profonde » — sauf qu'ici **les deux sont
post-quantiques**.

## 7. Sûreté responsable (le théorème, et le lien avec ADR-003)

C'est la propriété qui rend Casper élégant. **Deux conditions de slashing**, et deux seulement,
suffisent :

1. **Pas de double vote** : un validateur ne signe pas deux votes différents pour la **même
   époque cible**.
2. **Pas de vote enveloppant** : un validateur ne signe pas un vote dont l'intervalle
   `(source, cible)` en **entoure** un autre qu'il a déjà signé (source antérieure **et** cible
   postérieure).

> [!success] Théorème de sûreté responsable (esquisse)
> Si deux points de contrôle **en conflit** sont tous deux finalisés, alors des validateurs
> totalisant **au moins ⅓ de l'enjeu** ont nécessairement violé la condition 1 ou la 2. Comme
> chaque vote est une signature ML-DSA **non répudiable**, la violation est **prouvable** — donc
> **slashable**.

Conséquence forte : on ne peut pas casser la finalité **sans** qu'au moins **un tiers de
l'enjeu** soit détruisible par preuve. La sûreté n'est pas seulement « difficile à violer »,
elle est **responsable** : toute violation laisse une **preuve cryptographique de qui l'a
commise**. C'est ce qui donne corps à
[[ADR-003 — Slashing (accountable safety)]] — le slashing n'est pas un ajout, il **découle** des
deux conditions, et l'attribution de faute est **directe** parce que les votes sont **séparés**
(un des gains du PQ pur sur l'agrégat, noté en ADR-005).

## 8. Vivacité

Sous les hypothèses BFT usuelles (synchronie réseau après un délai, et **plus de ⅔ de l'enjeu
honnête et en ligne**), de nouveaux points de contrôle continuent d'être justifiés puis
finalisés : le gadget **progresse**.

Et la propriété duale, la **vivacité plausible** : depuis n'importe quel état atteignable, il
est **toujours possible** de finaliser un nouveau point de contrôle **sans** qu'aucun validateur
ait à violer une condition de slashing. Le protocole ne peut pas se peindre dans un coin où
finaliser exigerait de se faire slasher — c'est ce qui garantit qu'on n'est **jamais bloqué de
façon irrécupérable**.

> [!warning] Honnêteté — la vivacité est la partie subtile
> Elle dépend de la règle de fork-choice du §9 et des hypothèses de synchronie. C'est là que vit
> la vraie difficulté de la construction (le « Gasper » d'Ethereum, fork-choice + finalité,
> recèle des subtilités de vivacité réelles), et c'est là qu'il faudra le plus de soin et, à
> terme, de **formalisation** (§13).

## 9. Interaction avec le fork-choice (ADR-001) — et la fermeture du trou de partition

Le passage le plus élégant, parce qu'il **referme un trou** ouvert depuis des sessions.

La règle de fork-choice devient **consciente de la finalité** : un nœud construit toujours sur la
branche qui contient le **dernier point de contrôle justifié** le plus récent, et **ne revient
jamais** sur un point de contrôle **finalisé**. La finalité est un **plancher absolu**.

Or le problème laissé ouvert en [[ADR-001 — Fork-choice]] : le fork-choice intérimaire ne sait
départager qu'**un seul bloc au même index**, pas réconcilier deux chaînes concurrentes de
plusieurs blocs après une longue partition. **Le gadget le résout.** Quand deux partitions
guérissent :

- toute histoire **finalisée** de part et d'autre est irréversible, et par la sûreté du §7 il ne
  peut **pas** y avoir deux points de contrôle finalisés en conflit (sinon ⅓ est slashable) :
  les histoires finalisées **coïncident** ;
- **au-dessus** de la finalité, les branches concurrentes se départagent par la règle de
  fork-choice, en suivant le **point de contrôle justifié** le plus récent.

> [!success] Cible d'acceptation — réalisée (GADGET-5B)
> Le basculement attendu **a eu lieu** : le test **2b**, autrefois
> `t0_8_multiblock_partition_currently_diverges_gadget_deferred`, est maintenant
> `t0_8_multiblock_partition_reconciles_at_heal` (`src-tauri/src/sm/sim.rs:3467`) et asserte
> `tips[a] == tips[b]` — la **réconciliation**, plus la divergence. Le garde **§4** reste actif :
> une rupture de **conservation/émission** au heal (double-mint à l'échelle de la partition)
> continue de **paniquer** — c'est toujours un bug NEUF si elle survient. L'exigence qui
> accompagnait ce moment — la **conservation globale au heal**, c'est-à-dire **défaire
> proprement l'émission de toute branche perdante non finalisée** — est tenue.

## 10. Comité et rotation (ADR-002, ADR-004)

Le comité de validateurs est pondéré par l'**enjeu inscrit sur la chaîne**
([[ADR-002 — Validator set & comité BFT]] — enjeu **seul**, la réputation ayant été retirée du
chemin de sécurité). Il **tourne** par époque via l'aléa du **beacon**
([[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]]). Le quorum de ⅔ se mesure en **enjeu**,
pas en têtes. La taille du comité reste **modeste** au départ — ce qui est précisément ce qui
rend les certificats PQ gérables ([[ADR-005 — Agrégation des votes & certificats de finalité]]).

## 11. Vérification par le harnais DST (le vrai sceau d'orfèvre)

Ce qui rendra ce gadget impressionnant pour de **bonnes** raisons : il se conçoit pour être
**continûment falsifié** par la simulation déterministe. **Quatre invariants nouveaux**, vérifiés
à travers les centaines de graines fautées — dans la lignée des trois déjà en place (sûreté,
conservation, émission) :

- **Sûreté de finalité** : jamais deux points de contrôle finalisés **en conflit**.
- **Sûreté responsable** : toute violation de sûreté injectée produit bien une **preuve** de
  faute couvrant **≥ ⅓** de l'enjeu (on injecte un comité byzantin qui double-vote ou enveloppe,
  et on vérifie que le slashing **détecte et attribue**).
- **Vivacité de finalité** : sous synchronie et ⅔ honnêtes, des points de contrôle continuent de
  se finaliser (le sweep ne reste pas bloqué sans finalité).
- **Vivacité plausible** : aucun scénario n'enferme le protocole dans un état d'où finaliser
  exigerait de se faire slasher.

Et le test **2b** bascule de « partition multi-blocs **diverge** (gadget-deferred) » à
« partition multi-blocs **réconcilie** via la finalité », avec **conservation globale au heal**.
Un gadget dont la sûreté responsable est **esquissée** *et* **falsifiée en continu** par
simulation déterministe : c'est l'objet rare que les gens du domaine respectent.

## 12. Décisions — tranchées par ADR-009

> [!question] Ce dont j'avais besoin de toi — tranché
> La **règle d'arrêt §4** ([[QUANTA_AGENT_CONSTITUTION]]) nommait explicitement « quel modèle de
> finalité, faut-il du slashing et comment ». **[[ADR-009 — Frontière gravé-ajustable (ADR-006 ratifiée) et valeurs du §12]]**
> a tranché :
>
> - **`E`, la longueur d'époque** : **32 blocs** (`EPOCH_LENGTH_BLOCKS = 32`, `finality.rs`) —
>   gravé.
> - **Seuil de quorum** : **⅔ de l'enjeu**, gravé en `QUORUM_NUM`/`QUORUM_DEN` et vérifié par
>   `meets_supermajority` (`finality_vote.rs`).
> - **Variante de fork-choice** consciente de la finalité : **LMD-GHOST** (`ghost_head`,
>   `fork_choice.rs`, GADGET-5A).
> - **Montants et fenêtre de slashing** ([[ADR-003 — Slashing (accountable safety)]]) : montant
>   plein pour les deux conditions du §7 (double vote / vote enveloppant), fenêtre de preuve =
>   la fenêtre d'unbonding (`SLASH_EVIDENCE_WINDOW_BLOCKS`).
>
> Seul point encore **légitimement ouvert** : la **pénalité d'inactivité** éventuelle
> (l'« inactivity leak » de Casper) pour récupérer la vivacité si plus d'un tiers de l'enjeu
> disparaît durablement — reportée, non requise pour le gadget tel que livré.

Ces choix recoupent les sous-paramètres encore ouverts d'ADR-002/003/004/005 (taille de comité,
niveau ML-DSA, format/élagage du certificat) : voir [[docs/decisions/README|Registre des décisions]].

## 13. Limites honnêtes

Les arguments des §7 et §8 sont des **esquisses** appuyées sur les résultats connus de **Casper
FFG**, **pas** des preuves formelles propres à Quanta. La composition fork-choice + finalité (le
« **Gasper** » d'Ethereum) recèle des subtilités de vivacité réelles. **Avant genèse**, ce
protocole demandera une **formalisation** des invariants critiques et un **audit externe**. C'est
l'architecture de départ — choisie parce qu'elle est **éprouvée** et qu'elle **colle au système**
— pas un acquis.

## 14. Comment ça devient du code (chirurgical, pas un jet)

La conception est d'un bloc ; l'**implémentation** se découpe, **chaque pièce vérifiée par le
harnais avant la suivante** :

1. **Squelette d'époque et de point de contrôle** (découper la chaîne, identifier les frontières)
   + l'invariant de **sûreté de finalité** dans le harnais.
2. Les **votes ML-DSA** et leur agrégation en **certificat d'époque**, derrière l'abstraction
   ([[ADR-005 — Agrégation des votes & certificats de finalité]]).
3. La **règle justification/finalisation** en deux temps, avec son invariant de **vivacité**.
4. Les **deux conditions de slashing** et la vérification de **sûreté responsable** (comité
   byzantin injecté) — articule [[ADR-003 — Slashing (accountable safety)]].
5. Le **fork-choice conscient de la finalité**, et la **bascule du test 2b** (réconciliation de
   partition, **conservation globale au heal**).
6. **Rotation du comité** par le beacon ([[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]]),
   puis **pénalité d'inactivité**.

Chacune est un **spec serré**, exécuté et relu, comme tout ce qu'on a fait jusqu'ici. C'est ainsi
qu'un gros morceau se construit **sans devenir un gros risque**.

> [!note] Statut & suite
> Conception **validée et implémentée** (GADGET-1→5B, 2026-06-25) en simulation déterministe,
> suivant exactement le découpage du §14, chaque pièce **falsifiée par le harnais DST** avant la
> suivante. Les **🛑** du §12 sont tranchés par ADR-009. Prochaine étape : le **câblage vivant**
> (le gadget tourne en simulation, pas encore intégralement sur le réseau réel) — LIVE-1 (gossip
> des votes) est **fait** ; restent LIVE-2 (proposition finalité-consciente) et LIVE-3 (slashing
> vivant). Voir [[DESIGN-LIVE-WIRING]].

> L'orfèvrerie, c'est cette conception. Sa beauté n'est pas dans sa taille mais dans le fait que
> **chaque pièce se démontre et se mesure**. Le reste, c'est de la transcription patiente.
