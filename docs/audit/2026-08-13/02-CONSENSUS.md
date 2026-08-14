# AUDIT DE SÉCURITÉ — QUANTA : CONSENSUS, REGISTRE ET ÉCONOMIE

**Périmètre** : le « chemin de l'argent » — `p2p/ledger/*`, `p2p/pos_consensus.rs`,
`p2p/finality_live.rs`, `p2p/fork_heal.rs`, `p2p/consensus.rs`, `sm/finality_*`,
`sm/fork_choice.rs`, `p2p/mining_loop.rs`, `p2p/shapley.rs`, `p2p/reputation.rs`,
`p2p/ledger_types.rs`, plus les chemins d'admission de `p2p/dispatcher.rs`.
**Version auditée** : 3.15.1, protocole TORUS v9, commit de travail au 2026-08-13.
**Méthode** : lecture intégrale des fichiers du périmètre + **21 tests écrits et
exécutés** dans une copie jetable du dépôt (`/tmp/q_consensus_8ccf`, jamais commitée,
aucun fichier suivi par git modifié). La suite existante passe : `533 passed; 0 failed`.
Toute affirmation marquée **PROUVÉ** correspond à un test dont la sortie est citée
telle quelle. Ce qui n'est pas prouvé est marqué **non prouvé**.

---

## 1) Résumé exécutif

Le registre est bien écrit, très commenté, et la plupart des défenses « monétaires »
annoncées tiennent réellement (plafond 100 M, couverture des dépenses, émission
recalculée, slashing conservatif). Mais **trois trous structurels rendent la chaîne
actuellement non-sûre pour porter de la valeur** :

1. **Rejeu de transaction on-chain (CRITIQUE).** Rien, au niveau bloc, ne vérifie
   qu'une transaction n'a pas déjà été incluse — ni nonce, ni unicité de hash.
   `seen_tx_hashes` n'est qu'un dispositif de mempool. **Prouvé** : une seule
   signature d'Alice a été rejouée 10 fois d'affilée par un proposeur hostile et a
   vidé son compte (100 QTA → 0) vers l'attaquant.
2. **Réorganisation longue portée gratuite (CRITIQUE).** Le timestamp d'un bloc
   n'est jamais validé, il n'y a ni PoW ni cadence imposée, et le choix de branche
   vivant est « la plus longue, départage au hash le plus grand ». **Prouvé** : un
   validateur à **1 QTA** a forgé une branche de 50 blocs **en 405 µs** et l'a fait
   adopter, effaçant un paiement de 50 QTA déjà confirmé 3 fois.
3. **L'élection PoS n'est pas appliquée à la réception (CRITIQUE).** La validation
   n'exige qu'une *appartenance* au jeu de validateurs. **Prouvé** : un validateur
   pesant 1/1001 de l'enjeu, élu sur 1 slot sur 40, a vu **40/40** de ses blocs
   acceptés. Couplé au départage lexicographique **grindable en 4 essais**, un seul
   validateur à 1 QTA capture la production de blocs et censure à volonté.

Accessoirement : un vote de finalité honnête émis sur un autre réseau Quanta suffit
à faire **slasher 100 %** de l'enjeu d'un validateur honnête, et un seul message
gossip tue définitivement un nœud compilé en debug.

---

## 2) Tableau des constats

| id | sévérité | ancre | une ligne |
|----|----------|-------|-----------|
| C-01 | **CRITIQUE** | `p2p/ledger/validation.rs:602` | Aucune vérification d'unicité de tx ni de nonce à l'inclusion : toute tx déjà scellée peut être rejouée dans un bloc suivant. |
| C-02 | **CRITIQUE** | `p2p/ledger/validation.rs:233` | Le `timestamp` d'un bloc n'est validé nulle part (ni bornes, ni monotonie, ni parsabilité). |
| C-03 | **CRITIQUE** | `p2p/fork_heal.rs:340` + `p2p/ledger/reorg.rs:402` | Fork-choice vivant = plus longue chaîne + départage lexicographique, sans coût : long-range / nothing-at-stake à 1 QTA. |
| C-04 | **CRITIQUE** | `p2p/ledger/validation.rs:650` | La règle de proposeur à la réception est une simple appartenance : l'élection pondérée par l'enjeu n'est jamais appliquée. |
| H-05 | **HAUT** | `sm/finality_vote.rs:51` + `:87` | Ni la préimage de vote ni celle de tx ne lient un identifiant de réseau : rejeu inter-chaînes, et **slash à 100 % d'un validateur honnête**. |
| H-06 | **HAUT** | `p2p/finality_live.rs:276` | Le quorum ⅔ est pondéré par l'enjeu **courant**, pas celui de l'époque du vote : finalisation rétroactive et perte de finalité. |
| H-07 | **HAUT** | `p2p/ledger/validation.rs:290` | `sum()` u64 non protégé sur l'émission d'un bloc : **panique** en debug → tâche gossip morte, nœud sourd définitivement. |
| H-08 | **HAUT** | `p2p/reputation.rs:130` + `p2p/ledger/mod.rs:961` | Sybil de participation : 28 identités captent 45 % de chaque récompense pour 28 QTA d'enjeu. |
| M-09 | MOYEN | `p2p/ledger/mod.rs:370` | `energy_kwh` n'est pas engagé dans le hash de bloc : bloc malléable, `total_energy` divergent entre nœuds. |
| M-10 | MOYEN | `p2p/ledger/validation.rs:233` | Aucune borne sur la taille d'un bloc ni sur son nombre de tx (50 000 tx traitées avant rejet, sous le write-lock). |
| M-11 | MOYEN | `p2p/shapley.rs:84` | `+=` u64 sur des valeurs déclarées par un pair et non bornées : panique du tick de minage en debug. |
| M-12 | MOYEN | `p2p/ledger/stake.rs:150` | `revert_block_stake_effects` ne défait pas la maturation : un reorg qui la franchit fabrique du poids de consensus. |
| M-13 | MOYEN | `p2p/pos_consensus.rs:109` | Pas de VRF : le beacon est public et **grindable** par le proposeur de `slot−3` (auto-réélection à coût nul). |
| M-14 | MOYEN | `p2p/ledger/validation.rs:403` | Coût de validation d'un bloc entrant linéaire en la hauteur de chaîne (4 parcours complets), payé avant tout rejet. |
| B-15 | BAS | `sm/finality_slashing.rs:76` | `SLASH_EVIDENCE_WINDOW_BLOCKS` est déclaré et documenté comme contrainte gravée, mais **n'est utilisé nulle part**. |
| B-16 | BAS | `p2p/ledger/mod.rs:1241` | `total_supply()` mélange deux bases (miné = chaîne seule, brûlé = chaîne + mempool). |
| B-17 | BAS | `sm/fork_choice.rs:1` | Le moteur LMD-GHOST existe et est testé, mais n'est **jamais** consulté sur le chemin d'adoption de blocs. |

---

## 3) Développement par constat

### C-01 — CRITIQUE — Rejeu de transaction on-chain (`p2p/ledger/validation.rs:602`)

**Ce qui est faux.** `validate_block_against_prev` (validation.rs:602-779) est le
validateur partagé par *tous* les chemins d'admission. Il vérifie : `prev_hash`,
l'appartenance du proposeur, l'unicité des bénéficiaires de coinbase, les
expéditeurs synthétiques, les sur-retraits d'enjeu, la signature de chaque tx, la
liaison de clé ML-DSA, le hash de bloc, la couverture des dépenses. **Il ne vérifie
ni le nonce, ni l'unicité de la transaction.** Le mot `nonce` n'apparaît dans
`validation.rs` que dans la construction de la préimage de signature (lignes 103 et
165) — jamais comme état de compte.

L'anti-rejeu réel du projet est `seen_tx_hashes` (`mod.rs:158`), mais il n'est
consulté qu'à l'**admission mempool** (`mod.rs:1042`, `:1114`, `:1145`, `:1158`,
`stake.rs:342`, `:398`). Sur le chemin bloc, `integrate_remote_block` **insère** les
hashes sans jamais tester le retour (`reorg.rs:336` et `reorg.rs:520` :
`self.seen_tx_hashes.insert(tx.hash.clone());` — valeur de retour ignorée).

**Chemin d'exploitation concret.** Qui : n'importe quel proposeur de bloc valide —
donc n'importe quel compte bondé à 1 QTA (voir C-04), ou n'importe qui sur un slot
ouvert (1 sur 16). Accès requis : gossip seulement, aucune clé de la victime.
Gain : ré-exécuter arbitrairement tout `Transfer`/`Burn` déjà publié, jusqu'à
épuisement du solde de l'émetteur. Si l'attaquant est le destinataire d'un paiement
même minuscule, il vide le compte payeur.

**Preuve (test `audit_b1` et `audit_b1b`, exécutés) :**

```
AUDIT-B1: integrate(rejeu) = Ok(true)
AUDIT-B1: solde alice = 80000000 µQTA, bob = 20000000 µQTA
AUDIT-B1b: arrêt après 10 inclusions — Err("... dépense non couverte ... (COVER-1)")
AUDIT-B1b: 10 inclusions de LA MÊME tx ; alice=0 bob=100000000
```

Alice signe **une** fois un paiement de 10 QTA à Bob (scellé au bloc 1). Le bloc 2
reforgé contient la **même** transaction, hash identique : accepté. Après 10
répétitions, Alice est à 0 et Bob à 100 QTA. Le seul mur est COVER-1 (la couverture
du solde) — c'est-à-dire que le rejeu s'arrête quand la victime est vide.

**Corollaire monétaire (tests `audit_d2`, `audit_d3`).** Le même défaut porte sur la
coinbase :

```
AUDIT-D2: rejeu de la coinbase = Ok(true) ; solde mineur 2000000 -> 4000000
AUDIT-D3: la MÊME coinbase (même hash) a été minée 60 fois ; minté total = 120000000 µQTA
```

La borne par bloc `emission_for_block` empêche de dépasser le barème total, donc ce
n'est **pas** une inflation au-delà du plafond ; c'est la démonstration que le hash
d'une transaction n'a **aucune** unicité on-chain.

**Ce qu'il faut.** Un vrai état de nonce vérifié à l'inclusion (`tx.nonce ==
account_nonce[from]` pour chaque expéditeur réel, séquentiel dans le bloc comme le
fait déjà `uncovered_tx_indices`), et/ou un ensemble de hashes déjà scellés
consulté par `validate_block_against_prev`. La forme séquentielle par bloc existe
déjà pour la couverture : la même boucle peut porter le nonce.

---

### C-02 — CRITIQUE — Le timestamp de bloc n'est jamais validé (`p2p/ledger/validation.rs:233`)

**Ce qui est faux.** `validate_remote_block` (validation.rs:233-264) et
`validate_block_against_prev` (validation.rs:602-779) n'examinent jamais
`block.timestamp` autrement que comme octets entrant dans la préimage du hash
(`validation.rs:751`). Aucune borne de dérive d'horloge, aucune monotonie vis-à-vis
du parent, aucune cadence minimale, pas même une exigence de parsabilité RFC3339.
La seule fraîcheur du système est celle de **l'enveloppe** gossip (±90 s,
`p2p/gossip.rs:519-527`) — elle ne dit rien du contenu du bloc.

**Chemin d'exploitation.** Le timestamp devient (a) un compteur libre pour le
grinding de hash (C-03, M-13), (b) un moyen de fabriquer une chaîne « longue »
instantanément sans coût temporel, (c) une source de désaccord applicatif
(`pos_seal_if_leader` lit `tip_time` depuis ce champ, `mining_loop.rs:389-400`, pour
décider des rounds de repli : un timestamp posé loin dans le futur fige
`elapsed = 0` et **désactive** tous les repli de liveness ; posé en 1970 il ouvre
immédiatement le tier « n'importe quel validateur bondé peut proposer »).

**Preuve (test `audit_b3`, exécuté) :**

```
AUDIT-B3: timestamp "1970-01-01T00:00:00+00:00" -> Ok(true)
AUDIT-B3: timestamp "9999-12-31T23:59:59+00:00" -> Ok(true)
AUDIT-B3: timestamp "2000-01-01T00:00:00+00:00" -> Ok(true)   (antérieur au parent)
AUDIT-B3: timestamp "pas-une-date" -> Ok(true)
AUDIT-B3: timestamp "" -> Ok(true)
```

**Note honnête.** Le projet a une doctrine explicite « la validation ne relit jamais
l'horloge » (C2/§1.1, commentée en `mod.rs:1009-1016`). Elle est défendable pour la
*détermination*, mais elle a été appliquée jusqu'à supprimer toute contrainte : il
faut au minimum une monotonie `block.ts > prev.ts` (purement chaîne, déterministe,
sans horloge) et une borne de futur relative au parent.

---

### C-03 — CRITIQUE — Long-range / nothing-at-stake (`p2p/fork_heal.rs:340`)

**Ce qui est faux.** La règle de choix de branche *vivante* est, textuellement
(`fork_heal.rs:340-341`) :

```rust
let wins = run_tip.index > our_tip.index
    || (run_tip.index == our_tip.index && run_tip.hash > our_tip.hash);
```

soit « la plus longue, sinon le hash le plus grand » — la même règle que le
départage 1-bloc de `reorg.rs:402`. Ce n'est ni GHOST, ni pondéré par l'enjeu (le
module l'assume, `fork_heal.rs:42-49`). Or produire un bloc ne coûte **rien** : pas
de PoW, pas d'horloge validée (C-02), pas de VDF. Le seul frein est la borne de
finalité `finalized_floor_index` — qui vaut **0** tant que le gadget Casper n'a pas
finalisé, c'est-à-dire tant qu'il n'y a pas ⅔ de l'enjeu en ligne, votant, sur deux
époques consécutives (32 blocs ≈ 64 min chacune).

**Chemin d'exploitation.** Qui : un validateur bondé à `MIN_VALIDATOR_STAKE` =
**1 QTA** (soit un quart d'une seule récompense de bloc). Accès : gossip. Gain :
réécrire toute l'histoire au-dessus du plancher de finalité — donc annuler tout
paiement reçu, faire de la double-dépense contre un marchand, ou réécrire la liste
des participants pour se payer.

**Preuve (test `audit_c3`, exécuté) :**

```
AUDIT-C3: marchand payé, 3 confirmations, hauteur = 4
AUDIT-C3: 50 blocs forgés en 405.042µs ; floor=0 ; reorg_to_fork = Ok(true) ; hauteur = 51
AUDIT-C3: le paiement est-il encore sur la chaîne ? false
```

Le test met en place PROPOSER-1 *actif* (deux comptes bondés, dont l'attaquant à
exactement 1 QTA), fait payer 50 QTA à un marchand, laisse 3 confirmations, puis
forge 50 blocs vides datés de 1970 en **405 microsecondes** : `reorg_to_fork`
adopte, le paiement disparaît de la chaîne.

**Chiffrage.** Coût de l'attaque : 1 QTA d'enjeu (jamais slashé — le slashing ne
punit que l'équivocation *de vote de finalité*, pas la production de blocs
concurrents) + quelques millisecondes de CPU. La profondeur pratique est bornée par
`FORK_BUFFER_MAX_BLOCKS = 1024` (`fork_heal.rs:74`) soit ~34 h de chaîne — bien
au-delà de toute fenêtre de confirmation commerciale. Il n'y a **aucun checkpoint de
confiance faible** (weak subjectivity) : un nœud neuf qui synchronise suit la plus
longue branche qu'on lui présente.

**Réponse à la question 6 (long-range / nothing-at-stake).** L'unbonding existe et
est correct (`UNBONDING_PERIOD_BLOCKS = 10 080`, `mod.rs:68`), et le slashing frappe
bien `bonded + unbonding` (`slash.rs:39-67`, LIVE-3B) — donc l'*unstake-and-run*
d'un équivocateur de vote est fermé. Mais ça ne protège pas contre le long-range :
un ancien validateur qui a **retiré** son enjeu il y a plus de 10 080 blocs n'est
plus slashable et peut fournir une histoire alternative depuis l'époque où il était
bondé. Rien ne l'en empêche : il n'y a ni checkpoint signé, ni règle de
subjectivité faible, ni borne de profondeur de reorg autre que le plancher de
finalité.

---

### C-04 — CRITIQUE — L'élection de leader n'est pas appliquée à la réception (`p2p/ledger/validation.rs:650`)

**Ce qui est faux.** Le code l'écrit lui-même (validation.rs:625-634) : faute
d'horloge de confiance, la règle de réception est « l'UNION non temporisée de
{primaire ∪ replis ∪ tout éligible} = *n'importe quel validateur bondé* ». Concrètement :

```rust
if block.index > 0 && !is_open_slot(block.index) {
    let has_eligible = bonded_before.values().any(|&s| s >= MIN_VALIDATOR_STAKE);
    if has_eligible {
        let proposer_stake = bonded_before.get(&block.miner).copied().unwrap_or(0);
        if proposer_stake < MIN_VALIDATOR_STAKE { return Err(...) }
    }
}
```

`elect_leader` / `is_valid_proposer` (`pos_consensus.rs:163`, `:266`) ne sont appelés
**que** côté scellement (`mining_loop.rs:434`). Un nœud modifié ignore simplement ce
contrôle local.

**Chemin d'exploitation.** Qui : un compte bondé à 1 QTA. Gain : proposer à *toutes*
les hauteurs. Combiné au départage lexicographique et au timestamp libre, il **gagne
systématiquement** la course : le hash est grindable.

**Preuve 1 — l'élection est ignorée (test `audit_f1`, exécuté) :**

```
AUDIT-F1: l'attaquant (1 QTA sur 1001) est élu 1/40 slots, mais 40/40 de ses blocs
          sont ACCEPTÉS par la validation de réception
```

**Preuve 2 — le départage est grindable (test `audit_c1`, exécuté) :**

```
AUDIT-C1: 4 essais pour battre le tip honnête (fd65e8fa94a7… vs 5b8f8c26a60e…)
AUDIT-C1: integrate = Ok(true) ; nouveau tip miner = ffffffffffff
AUDIT-C1: 200 000 forges en 1.598682416s -> meilleur hash ffff86582a03…
```

200 000 variantes de timestamp en 1,6 s **en build debug** produisent un hash
commençant par `ffff` — qui bat 65 535 tips honnêtes sur 65 536. En release c'est
plus rapide d'un ordre de grandeur. Le « départage déterministe » est donc une
**course au calcul non bornée**, remportée par qui veut bien la courir.

**Contrôle négatif (ce qui tient, test `audit_f2`) :** hors slot ouvert, un
proposeur non bondé est bien refusé ; sur le slot 16 (ouvert, `pos_consensus.rs:98`)
n'importe qui passe — conforme à OPEN-DOOR-1 et assumé.

**Impact chiffré.** Avec `MIN_VALIDATOR_STAKE = 1 QTA` et une récompense de genèse
de `emission_for_block(0) = 4 QTA/bloc` (`reputation.rs:115` : `2 × (10^14 − miné)/5·10^7`),
le ticket d'entrée à l'attaque vaut **un quart d'un seul bloc**. À 720 blocs/jour,
l'attaquant capture jusqu'à 15/16 de l'émission (les slots non ouverts) plus sa part
sur les slots ouverts, et censure toute transaction qu'il choisit.

---

### H-05 — HAUT — Aucun identifiant de réseau lié : rejeu inter-chaînes, et slash d'un honnête (`sm/finality_vote.rs:51`, `p2p/ledger/mod.rs:1471`)

**Ce qui est faux.** Les deux préimages signées du système ne lient **aucun**
identifiant de chaîne :

* vote de finalité (`finality_vote.rs:51` et `:87-95`) :
  `VOTE_DOMAIN ‖ source ‖ target ‖ voting_epoch ‖ validator` — le domaine est la
  constante littérale `b"QUANTA-FINALITY-VOTE-v1"`, identique sur tout réseau ;
* transaction (`mod.rs:1471-1482`) :
  `id:from:to:amount:ts:type:nonce:pq_pk` — pas de chain-id, pas de hash de genèse.

**Chemin d'exploitation 1 — faire slasher un validateur honnête à 100 %.**
`detect_fault` (`finality_slashing.rs:108-123`) classe `DoubleVote` **toute** paire
de votes distincts partageant l'époque cible. Deux votes parfaitement honnêtes émis
par le même porteur de clé sur deux réseaux Quanta différents (testnet/mainnet,
chaîne v3 / chaîne v4 — le projet a déjà fait ce redémarrage, cf. `GENESIS-V4`,
`mod.rs:213-219`) partagent l'époque 1 et diffèrent par le hash de checkpoint. La
paire vérifie `verify_proof`, et `queue_slash` détruit **tout** l'enjeu
(`SLASH_NUM/SLASH_DEN = 1/1`, `finality_slashing.rs:57-59`).

**Preuve (test `audit_c5`, exécuté) :**

```
AUDIT-C5: detect_fault(vote mainnet, vote testnet) = Some(DoubleVote)
AUDIT-C5: Slash de 5000000 µQTA sur un validateur HONNÊTE
AUDIT-C5: enjeu après slash = 0 ; brûlé = 5000000
```

Qui : n'importe qui capable d'observer les votes des deux réseaux (ils sont
publiquement gossipés). Accès : lecture. Gain : détruire l'enjeu d'un concurrent —
et, en le faisant sur ⅓ de l'enjeu, tuer la finalité du réseau.

**Chemin d'exploitation 2 — rejeu de transaction inter-réseaux.** Une tx signée sur
un réseau est bit-pour-bit valide sur l'autre (même adresse, même clé, même
préimage). *Non prouvé par test* (il faudrait deux réseaux), mais la lecture de la
préimage est sans ambiguïté et C-01 démontre déjà que rien au niveau bloc ne
rejette une tx « déjà vue ailleurs ».

**Remède minimal.** Lier le hash de genèse (déjà disponible : `Ledger::genesis_hash()`,
`mod.rs:662`) dans `VOTE_DOMAIN` et dans `tx_signing_preimage`, et exiger dans
`detect_fault`/`verify_proof` que les deux votes portent le même ancrage.

---

### H-06 — HAUT — Le quorum ⅔ est pondéré par l'enjeu courant, pas celui de l'époque du vote (`p2p/finality_live.rs:276`)

**Ce qui est faux.** `FinalityTracker::ingest_vote` fait, ligne 276 :

```rust
let stakes = ledger.validator_stakes_by_pubkey();
```

c'est-à-dire l'enjeu bondé **à l'instant de l'ingestion**. Ce même instantané sert à
`Vote::verify` (poids > 0), à `MlDsaCertificate::backing_weight` et au dénominateur
`total_stake` de `meets_supermajority` (`finality_vote.rs:239-249`). Un vote pour
l'époque *e* est donc pesé avec l'enjeu de l'époque *courante*.

**Deux conséquences.**

1. **Sécurité — finalisation rétroactive.** Un ensemble de votes qui n'atteignait
   pas ⅔ au moment de son émission le devient plus tard, quand les autres
   validateurs se désengagent. Les votes sont publics, non expirants, rejouables
   (aucun ancrage, cf. H-05) : un attaquant les collecte et les rejoue au bon moment.
2. **Liveness — perte de finalité.** Symétriquement, les votes d'un validateur qui
   unstake cessent de compter rétroactivement, ce qui peut défaire un certificat en
   cours d'assemblage.

**Preuve (test `audit_c4`, exécuté) :**

```
AUDIT-C4: même certificat — valide avec l'enjeu d'époque ? false ; plus tard ? true
```

Trois validateurs à 5 QTA : le vote de A seul pèse 1/3 → pas de quorum. Après le
désengagement de B et C, **le même certificat, inchangé**, franchit les ⅔.

**Ce qui est correct malgré tout.** L'ingestion refuse les liens dont l'époque cible
est ≤ l'époque finalisée (`finality_live.rs:326`) et borne l'époque contre la chaîne
(`:336-343`), et `apply_certificate` exige que la source soit déjà justifiée
(`finality_rule.rs:156`). Cela **limite** la portée du rejeu au futur immédiat de
l'état de finalité local — mais ne le ferme pas, en particulier sur un nœud neuf
dont l'état de finalité est à la genèse.

**Autres réponses à la question 5.**
* *Le slashing détecte-t-il les deux fautes ?* Oui, correctement :
  `DoubleVote` (même époque cible, votes distincts) et `Surround` (intervalle
  strictement englobant), `finality_slashing.rs:108-131`. La détection est pure et
  déterministe, et `verify_proof` exige des signatures ML-DSA valides du **même**
  validateur — donc pas d'accusation forgée *à l'intérieur d'un réseau*.
* *Peut-on faire slasher un honnête ?* Oui — par rejeu inter-chaînes (H-05).
* *Le plancher irréversible peut-il être contourné par un fork_heal ?* **Non.**
  Le plancher est vérifié trois fois sur ce chemin : `ForkReconciler::offer`
  (`fork_heal.rs:134`), `assemble_winning_run` (`fork_heal.rs:306`) et
  `reorg_to_fork` (`reorg.rs:587`), plus le veto du départage 1-bloc
  (`reorg.rs:391`). `set_finalized_floor` est monotone et vérifie le hash
  (`mod.rs:689-716`). C'est bien fait.

---

### H-07 — HAUT — Débordement u64 sur la somme d'émission d'un bloc (`p2p/ledger/validation.rs:290`)

**Ce qui est faux.** `validate_block_emission_against` somme les montants `Mining`
d'un bloc avec l'implémentation `Sum for u64`, qui utilise `+` :

```rust
let block_minted: u64 = block.transactions.iter()
    .filter(|t| t.tx_type == TxType::Mining).map(|t| t.amount).sum();
if block_minted == 0 { return Ok(()); }
```

`Cargo.toml` ne contient **aucune** section `[profile.release]`, donc les valeurs par
défaut de Rust s'appliquent : `overflow-checks = true` en debug, `false` en release.
Depuis REWARD-SHARE-1 (v9) un bloc porte **plusieurs** coinbases, la somme est donc
attaquable. Le même motif existe ligne 367 (`let total: u64 = actual.values().copied().sum();`).

**(b) En debug : DoS distant permanent. PROUVÉ.**

```
thread '…audit_a1…' panicked at core/src/iter/traits/accum.rs:204: attempt to add with overflow
AUDIT-A1: PANIQUE (build debug, overflow-checks=on) => DoS distant
AUDIT-A2: PANIQUE sur integrate_remote_block
```

Deux coinbases de 2^63 (ou `u64::MAX` + 1) vers deux adresses distinctes, hash de
bloc correct. Aggravant décisif : `dispatch_incoming` est appelé **en ligne** dans
l'unique boucle `spawn_incoming_dispatch` (`p2p/gossip_tasks.rs:63-66`), qui n'est
jamais relancée. La panique tue la tâche → le nœud devient **sourd à tout gossip,
définitivement**, avec un seul message. Le même raisonnement vaut pour n'importe
quelle panique atteignable depuis le dispatch.

**(a) En release : la somme s'enroule, mais l'exploitation échoue. PROUVÉ.**

```
AUDIT-A1: pas de panique (release/wrap) => verdict =
  Err("bloc rejeté : répartition de la récompense non conforme — 2 bénéficiaire(s)
       déclaré(s), 0 attendu(s) par le plan recalculé depuis la chaîne (REWARD-SHARE-1)")
```

La somme enroule à 0, `validate_block_emission_against` sort en `Ok(())` immédiat —
et c'est `validate_block_reward_plan` qui rattrape. **Argument structurel** (pas
seulement empirique) : si `actual == expected`, alors les montants sont exactement le
plan calculé sur `total`, dont la somme **vraie** vaut `total` — donc aucun
enroulement n'a pu avoir lieu. Et le nombre de bénéficiaires du plan est borné par
`SHARE_WINDOW_BLOCKS + 1 = 33`.

**Vérification des chemins (demandée).** Les deux contrôles sont bien appelés sur
**tous** les chemins d'admission : intégration linéaire (`validation.rs:260` et
`:263`), départage 1-bloc (`reorg.rs:463` et `:469`), reorg profond
(`reorg_to_fork` → `integrate_remote_block` → chemin linéaire, `reorg.rs:595`),
synchronisation `ChainSegment` (`dispatcher.rs:1592` → même appel). **Aucun chemin
n'appelle l'un sans l'autre.**

**Conclusion honnête.** Pas de mint infini aujourd'hui — mais la sûreté monétaire
repose ici entièrement sur une règle **économique** (le partage v9) qui n'a pas été
écrite pour ça. Avant v9, ou si le partage est assoupli, ce même code devient un
mint illimité. À corriger par `checked_add`/`saturating_add` explicite, pas par
chance.

---

### H-08 — HAUT — Sybil de participation sur le partage v9 (`p2p/reputation.rs:130`, `p2p/ledger/mod.rs:961`)

**Ce qui est faux (partiellement).** `expected_block_rewards` (`mod.rs:961-993`)
donne `PROPOSER_SHARE_NUM/DEN = 1/2` au proposeur et répartit le reste **à parts
égales** entre les `recent_participants` (`mod.rs:928-936`) — les adresses
**distinctes** ayant scellé un bloc dans les `SHARE_WINDOW_BLOCKS = 32` derniers.
« À parts égales » signifie qu'un acteur multipliant ses identités multiplie ses parts.

**La bonne nouvelle d'abord (réponse à la question 4).** Le plan est bien une
**fonction pure de la chaîne**, recalculée par chaque récepteur
(`validate_block_reward_plan`, `validation.rs:350-381`) et comparée exactement à ce
que le bloc déclare. Un producteur ne peut ni garder la totalité, ni payer un
bénéficiaire hors plan, ni s'écarter des montants. La participation est **prouvée**
(`block.miner` est engagé dans le hash de bloc) et non déclarée. C'est solide.

**La faille est en amont : entrer dans la liste.** Il faut avoir scellé un bloc dans
la fenêtre. Deux portes : (i) le slot ouvert, 1 bloc sur 16 (`pos_consensus.rs:94`)
— cette porte-là est effectivement bornée, comme le prétend OPEN-DOOR-1 ; (ii) être
bondé — et par C-04 un compte bondé à 1 QTA peut proposer **à toutes les hauteurs**,
en gagnant le départage par grinding. La borne « au plus 1/16 de l'émission »
annoncée dans `pos_consensus.rs:85-90` ne tient donc **pas** : elle ne borne que la
porte (i).

**Preuve (test `audit_c2bis`, exécuté)** — population honnête fixe de 4 mineurs,
plus K identités de l'attaquant dans la fenêtre :

```
AUDIT-C2bis: K= 0 -> participants=4  ; l'attaquant capte       0 µQTA (0.0 %)
AUDIT-C2bis: K= 1 -> participants=5  ; l'attaquant capte  500000 µQTA (12.5 %)
AUDIT-C2bis: K= 4 -> participants=8  ; l'attaquant capte 1142856 µQTA (28.6 %)
AUDIT-C2bis: K=12 -> participants=16 ; l'attaquant capte 1599996 µQTA (40.0 %)
AUDIT-C2bis: K=28 -> participants=32 ; l'attaquant capte 1806448 µQTA (45.2 %)
```

**Chiffrage.** 28 identités = 28 QTA d'enjeu, soit **7 blocs de récompense**
(~14 minutes d'émission réseau). Elles rapportent ensuite 45,2 % de **chaque** bloc,
indéfiniment, *en plus* de la part de proposeur quand l'attaquant propose. La borne
dure est `SHARE_WINDOW_BLOCKS` : au plus 32 participants, donc au plus ~50 % du pot
— mais 50 % du pot pour 32 QTA est un rendement absurde.

---

### M-09 — MOYEN — `energy_kwh` n'est pas engagé dans le hash de bloc (`p2p/ledger/mod.rs:370`)

`block_hash_hex` couvre `index:prev_hash:ts:miner:len:merkle_root`. Le champ
`energy_kwh` de `Block` (`ledger_types.rs:89`) n'y figure pas. Un relais peut donc
le modifier sans invalider quoi que ce soit : deux nœuds « d'accord sur le même
bloc » stockent des contenus différents.

**Preuve (test `audit_b4`, exécuté) :**

```
AUDIT-B4: bloc à energy_kwh falsifié = Ok(true)
AUDIT-B4: total_energy sceleur = 1.5, récepteur = 1000000000
```

**Impact.** Aujourd'hui `energy_kwh` ne pèse plus rien sur le chemin monétaire
(MINT-EXACT-1 a sorti l'énergie de l'émission) : l'impact est la divergence de
`stats().total_energy`, donc des vues « énergie réseau » et du récit « valeur
adossée à l'énergie réelle ». C'est **une malléabilité de bloc**, c'est-à-dire une
violation de la propriété « le hash identifie le bloc » — dangereuse à conserver si
le champ redevient un jour consensuel. Sévérité MOYEN aujourd'hui, HAUT si l'énergie
revient dans une règle.

---

### M-10 — MOYEN — Aucune borne de taille ni de nombre de transactions par bloc (`p2p/ledger/validation.rs:233`)

Rien dans `validate_remote_block` / `validate_block_against_prev` ne borne
`block.transactions.len()` ni la taille sérialisée. Les seuls plafonds sont au
transport : `MAX_RAW_ENVELOPE_BYTES = 10 Mo` (`dispatcher.rs:100`) et 50 Mo après
décompression pour un `ChainSegment` (`gossip.rs:310`). Une tx `Mining` minimale
pèse ~300 octets JSON, soit **~165 000 tx par bloc** à la limite du transport.

**Preuve (test `audit_a3`, exécuté) :**

```
AUDIT-A3: bloc de 50000 tx traité en 1.660785834s (debug) / 49.757542ms (release),
          verdict = rejet par la règle de répartition — jamais par une borne de taille
```

**Impact.** Le travail est payé **avant** le rejet, et entièrement **sous
`state.node.ledger.write().await`** (`dispatcher.rs:1371`), donc il bloque le
minage, la finalité, le RPC et l'UI. Avec `MAX_MSG_PER_WINDOW = 120` messages/minute
et par expéditeur (`dispatcher.rs:37`), quelques identités suffisent à saturer le
verrou. Ajouter une borne dure (`MAX_TXS_PER_BLOCK`, `MAX_BLOCK_BYTES`) vérifiée
**en premier** dans `validate_remote_block`.

---

### M-11 — MOYEN — Débordement sur des valeurs de pair non bornées (`p2p/shapley.rs:84`)

`handle_hello` stocke `tasks_completed`, `blocks_verified`, `uptime_minutes` tels
quels (`dispatcher.rs:1071-1073` ; seuls les watts sont clampés, `:1044-1055`).
`NetworkTotals::from_contributions` les additionne avec un `+=` nu
(`shapley.rs:84-85`), depuis le tick de minage (`mining_loop.rs:85`, `:114`).

**Preuve (test `audit_e1`, exécuté) :**

```
thread '…audit_e1…' panicked at src/p2p/shapley.rs:84:13: attempt to add with overflow
AUDIT-E1: PANIQUE (debug) dans le tick de minage => la tâche meurt
```

Deux `Hello` signés déclarant chacun `2^63` tâches suffisent. En debug, la tâche de
minage meurt : le nœud ne scelle plus jamais et ne vote plus la finalité. En release
la somme s'enroule et les parts Shapley deviennent absurdes — sans conséquence
consensuelle depuis MINT-EXACT-1 (Shapley n'alimente plus que l'affichage local),
mais c'est un signal faux.

---

### M-12 — MOYEN — Un reorg franchissant la maturation d'unbonding fabrique du poids de consensus (`p2p/ledger/stake.rs:150`)

Le commentaire de `revert_block_stake_effects` (`stake.rs:143-149`) affirme :
« *Maturation is intentionally NOT un-done … fork resolution here is single-block
(≤1 deep) … so no reorg can ever span an unbonding maturation* ». **C'est faux
depuis LIVE-4** : `pop_above` (`reorg.rs:632-643`) l'appelle pour une réorganisation
de profondeur quelconque, et `reorg_to_fork` est `pub` sans borne de profondeur.

Si un reorg défait un bloc contenant un `Unstake` **après** que l'entrée d'unbonding
a mûri, la revert re-crédite `staked += tx.amount` (`stake.rs:167`) alors que la
maturation a déjà rendu les fonds dépensables et n'est pas annulée.

**Preuve (test `audit_g1`, exécuté, au niveau de l'état interne) :**

```
AUDIT-G1: après revert -> dépensable = 15000000 µQTA ET enjeu bondé = 5000000 µQTA
          (les 5 QTA comptent DEUX FOIS : dépensables ET poids de consensus)
```

**Portée honnête.** La conservation `Σ dépensable + enjeu verrouillé + brûlé = minté`
reste vraie (le sink STAKE compense) ; ce qui est fabriqué est le **poids de
consensus**, donc du droit de proposer et du poids de quorum. Atteignabilité :
il faut un reorg de plus de 10 080 blocs, or `FORK_BUFFER_MAX_BLOCKS = 1024` borne
le chemin réseau actuel. **Donc : bug latent prouvé au niveau de l'état, non
atteignable via le chemin réseau d'aujourd'hui** — mais la seule chose qui le retient
est une constante de tampon DoS, pas une règle de consensus. À corriger (borner
explicitement la profondeur de reorg à `< UNBONDING_PERIOD_BLOCKS`, ou rendre la
maturation réversible).

Note associée : l'asymétrie `apply` / `revert` sur `Unstake` est réelle —
`apply_block_stake_effects` clampe (`let moved = tx.amount.min(*bonded);`,
`stake.rs:60`) alors que la revert ré-ajoute `tx.amount` entier (`stake.rs:167`).
Sur une chaîne validée les deux coïncident (C3 rejette les sur-retraits), mais
`rebuild_cache` rejoue `apply` **sans** revalider : une chaîne héritée/restaurée
contenant un sur-retrait ferait diverger les deux. *Non prouvé.*

---

### M-13 — MOYEN — Pas de VRF ; le beacon est grindable par le proposeur de `slot−3` (`p2p/pos_consensus.rs:109`)

Le module l'annonce honnêtement (`pos_consensus.rs:1-9`) : ce n'est pas un VRF, il
n'y a pas de clé secrète, la sortie est une fonction publique de données publiques,
donc **les leaders futurs sont publiquement prédictibles** (surface de DoS ciblé).
L'atténuation choisie est le beacon « enterré » : `LEADER_ENTROPY_LOOKBACK = 2`
(`:116`), et `pos_seal_if_leader` calcule `buried_index = tip_index − 2`
(`mining_loop.rs:349`), donc le beacon du slot *S* vient du bloc *S−3*.

**Ce que ça laisse ouvert.** Le proposeur du bloc *S−3* choisit librement le hash de
son bloc — le `timestamp` n'étant pas validé (C-02), il peut l'énumérer sans coût —
et donc **choisir qui sera leader au slot S**, y compris lui-même. Auto-réélection
tous les 3 slots à coût nul. Avec 200 000 essais/1,6 s mesurés (C-04), et un jeu de
validateurs de taille modeste, la probabilité de trouver un hash qui s'auto-élit est
proche de 1.

**Portée réelle.** Comme la réception n'applique de toute façon pas l'élection
(C-04), ce constat est aujourd'hui *subsumé* par C-04. Il redeviendra critique le
jour où l'élection sera appliquée à la réception : corriger C-04 sans corriger le
grinding déplacerait le problème sans le résoudre. Le README/ADR-004 le reconnaît ;
ce rapport confirme que le contournement est trivial dans l'état actuel.

---

### M-14 — MOYEN — Coût de validation linéaire en la hauteur de chaîne (`p2p/ledger/validation.rs:403`)

Chaque bloc entrant déclenche, dans `validate_remote_block` : `onchain_spendable_before`
(parcours complet de la chaîne, `validation.rs:403-477`), `pq_bindings_before`
(parcours complet, `:532-548`), `stats()` deux fois (`:263`, `:275`, parcours
complets). Soit ~4 parcours de toute l'histoire **par bloc reçu**, sous le write-lock,
**avant** tout rejet.

**Preuve (test `audit_h1`, exécuté, blocs vides, release) :**

```
AUDIT-H1: hauteur   100 -> 1.354µs par bloc entrant
AUDIT-H1: hauteur  1000 -> 7.25µs
AUDIT-H1: hauteur  4000 -> 38.275µs
```

Croissance linéaire nette (~9,6 ns par bloc d'histoire, sur des blocs vides ; le coût
réel suit le nombre total de **transactions**). À un an de chaîne (720 blocs/jour →
~262 000 blocs) et 5 tx/bloc, on est à ~1,3 M de transactions revisitées à chaque
bloc reçu. Un attaquant paie O(1) pour faire payer O(hauteur), et le rejet arrive
après le travail.

---

### B-15 — BAS — `SLASH_EVIDENCE_WINDOW_BLOCKS` n'est utilisé nulle part (`sm/finality_slashing.rs:76`)

La constante est déclarée, documentée comme « contrainte GRAVÉE par ADR-009 », et
protégée par un `const _: () = assert!(…)` de compilation (`:80`). Recherche
exhaustive : **aucune** utilisation en dehors de sa propre déclaration et du
commentaire de `mod.rs:65`. Aucune fenêtre d'admissibilité de preuve n'est donc
appliquée : une preuve d'équivocation d'il y a un an est acceptée telle quelle.

**Pourquoi ce n'est pas grave aujourd'hui** : l'effet recherché est obtenu
indirectement — `expected_slash_consumption` (`slash.rs:39`) calcule le montant sur
`staked + unbonding` **courants**, donc un offenseur totalement retiré donne
`amount == 0` et `build_slash_tx` renvoie `None`. La fenêtre effective est bien
l'unbonding. C'est correct **par accident** ; la constante ment sur ce qui est
implémenté.

---

### B-16 — BAS — `total_supply()` mélange deux bases (`p2p/ledger/mod.rs:744`)

`total_supply()` = `stats().total_mined` (chaîne **seule**) − `total_burned()`
(chaîne **+ mempool**, `mod.rs:1241-1257`). Quand un burn est en attente, l'offre
affichée est sous-estimée du montant en attente. `total_minted()` (`:1264`) utilise
la base cohérente (chaîne + mempool) — les deux coexistent. Sans conséquence
consensuelle (aucune règle ne lit `total_supply`), mais c'est un compteur public
faux par intermittence.

---

### B-17 — BAS — Le moteur LMD-GHOST n'est jamais consulté sur le chemin d'adoption (`sm/fork_choice.rs`)

`ghost_head` est de bonne facture (pondération par l'enjeu, ancrage justifié,
plancher finalisé, tie-break déterministe, marche bornée contre les cycles). Il
n'est appelé qu'en un seul endroit : `FinalityTracker::head()`
(`finality_live.rs:398-402`), qui n'est utilisé par aucun chemin d'adoption de bloc
(vérifié par recherche : seuls des tests et l'observabilité l'appellent). Le choix
de branche réel est la règle « plus longue + hash » de `fork_heal.rs:340`. Le module
`fork_heal` l'assume en commentaire (`:42-49`) — mais l'effet net est que le projet
possède un fork-choice pondéré par l'enjeu **qui ne sert pas**, et que C-03 en est
la conséquence directe.

---

## 4) Ce qui est solide

Ces défenses ont été lues **et** éprouvées ; elles tiennent, et voici pourquoi.

1. **Plafond dur de 100 M — vérifié au consensus, pas seulement à l'émission.**
   `validate_block_emission_against` (`validation.rs:286-335`) applique ① le plafond
   total avec `saturating_add` et ② une borne **par bloc** égale à la récompense
   canonique recalculée depuis la chaîne (`emission_for_block`, `reputation.rs:115`).
   Ces deux contrôles sont sur les **quatre** chemins d'admission (vérifié ligne à
   ligne, cf. H-07). `emission_for_tick` n'émet qu'une fraction du **restant**
   (`MAX − miné)/50 000 000`), donc le dépassement est arithmétiquement impossible.
   *Réponse à la question 2* : un reorg profond ne rejoue pas les récompenses
   au-delà du plafond, parce que `prior_mined` est **recalculé depuis la chaîne
   courante** — l'émission de la branche abandonnée disparaît avec elle
   (`reorg.rs:456-463` soustrait explicitement le tip remplacé, précisément pour ne
   pas double-compter).

2. **Émission recalculée, jamais crue.** MINT-EXACT-1 a sorti du chemin monétaire
   les watts auto-déclarés et la part Shapley : la récompense est une fonction pure
   de la chaîne, que producteur et vérificateur dérivent avec **le même** code
   (`mod.rs:816-859` côté frappe, `validation.rs:325` côté contrôle). C'est la bonne
   architecture, et le commentaire qui documente la marge de `32 × N` de l'ancienne
   version est un modèle d'honnêteté.

3. **Répartition de la récompense recalculée, elle aussi.** `validate_block_reward_plan`
   (`validation.rs:350-381`) reconstruit l'ensemble **exact** des bénéficiaires et
   leurs montants et compare par égalité de `BTreeMap`. Un producteur ne peut ni
   tout garder, ni payer un tiers hors plan. L'invariance d'échelle (émettre moins
   est permis, mais la *forme* du partage est imposée) est bien pensée.

4. **Couverture séquentielle (COVER-1/COVER-2), une seule règle pour produire et
   vérifier.** `uncovered_tx_indices` (`validation.rs:899-934`) est la source unique :
   la validation **rejette** le bloc, le scellement **exclut** exactement les mêmes
   index (`reorg.rs:74-125`). C'est ce qui garantit qu'un bloc auto-scellé est valide
   par construction, et c'est ce qui a arrêté mon attaque de rejeu (B1b) à
   l'épuisement du solde. Le même patron est appliqué à `illegal_synthetic_indices`
   (C2), `overdrawn_unstake_indices` (C3), `binding_violations` (PQ-MIG-3) et
   `invalid_slash_indices` (LIVE-3). Cette symétrie produire/vérifier est la
   meilleure idée d'ingénierie du fichier.

5. **Autorité de transaction post-quantique intrinsèque.** `verify_tx`
   (`validation.rs:41-123`) exige que `from` **soit** l'adresse ML-DSA de la clé
   révélée (`address_hex_binds_key_hex`), recalcule le hash depuis la préimage et
   rejette toute divergence (pas de malléabilité de hash), et n'accepte que la
   signature ML-DSA (l'Ed25519 n'a plus d'autorité). `Mining` d'un expéditeur
   utilisateur est rejeté au portail (MINT-GUARD-1, `:61-63`), et `Slash` est
   bloc-seulement. Le type `VerifiedTx` (`mod.rs:93-117`) transforme « la signature
   a été vérifiée » en garantie de **type**. C'est propre.

6. **Racine de Merkle correctement domaine-séparée.** `compute_merkle_root`
   (`mod.rs:1324-1358`) : feuilles `H(0x00 ‖ …)`, nœuds `H(0x01 ‖ g ‖ d)`, nœud
   impair **promu** sans duplication — CVE-2012-2459 fermée. Les feuilles lient le
   contenu **et** la couche d'autorité ML-DSA, plus la preuve de faute et le
   détail d'unbonding consommé pour un `Slash`.

7. **Slashing conservatif et vérifié par tous.** `slash_tx_valid` (`slash.rs:161-193`)
   re-vérifie la preuve, re-dérive le plan de consommation exact et exige une
   correspondance **au détail près** ; `invalid_slash_indices` est séquentiel et
   interdit un second slash du même offenseur dans un bloc, ainsi que la coexistence
   avec un mouvement d'enjeu. La destruction est `STAKE → BURN`, neutre pour la
   conservation. La base slashable est `bonded + unbonding` : l'*unstake-and-run*
   est fermé.

8. **Le plancher de finalité est un veto réel.** Vérifié sur les quatre chemins
   (cf. H-06), monotone, et — point important, corrigé après un audit précédent —
   il ne se fige que si le bloc localement détenu à cette hauteur **est** le bloc
   finalisé (`mod.rs:689-716`).

9. **Protection anti-auto-équivocation (le « slashing DB »).** `cast_memo`
   (`finality_live.rs:88-99`, `build_vote_to_cast:494-505`) empêche le nœud de
   signer deux votes différents pour une même époque cible, **et il est persisté**
   (`state_persistence.rs:44`, `:168`, `:262-265`) donc il survit au redémarrage.
   Beaucoup de clients Ethereum ont mis des années à faire ça correctement.

10. **Certificats de finalité correctement construits.** `MlDsaCertificate::backing_weight`
    (`finality_vote.rs:209-231`) rejette les liens mixtes, les votes invalides et
    les votants dupliqués, et somme en `checked_add` ; `meets_supermajority`
    (`:239-241`) est entièrement entier (`u128`), sans flottant, et exige
    `backing > 0`. `apply_certificate` (`finality_rule.rs:143-176`) applique bien la
    règle Casper en **deux temps** (justifier, puis finaliser sur un lien
    d'époques consécutives) et est append-only.

11. **Déterminisme sérieusement tenu.** Ordonnancements par `BTreeMap`/`BTreeSet`
    partout où un verdict est produit, tri par clé publique avant l'élection
    (`pos_consensus.rs:184`, invariance par permutation testée), heures injectées
    dans le cœur (`*_at`), et un harnais DST (`sm/sim.rs`, 3 780 lignes) qui rejoue
    des campagnes avec graine. Les 533 tests passent.

12. **Bornes DoS déjà présentes et pertinentes** : `MAX_NONCE_GAP` (le vieux
    `for _ in current..new_hw` à 2^64 itérations sous le verrou est bien fermé,
    `mod.rs:47`, `:737`), pool de certificats borné et éviction déterministe par
    distance à l'époque de la chaîne (`finality_live.rs:367-389`), tampon de fork
    borné avec éviction par utilité (`fork_heal.rs:156-195`), CRDT borné et O(1)
    (`consensus.rs:105-116`), `short()` sûr en UTF-8 pour les logs (`mod.rs:28-34`).
    Beaucoup de ces bornes portent la trace d'audits précédents et sont bien faites.

---

## 5) Ce que je n'ai PAS pu vérifier, et pourquoi

1. **Le comportement multi-nœuds réel** (convergence, propagation, courses entre
   `mining_loop` et le dispatcher). Tout ce rapport est établi au niveau de la
   bibliothèque, sur des ledgers en mémoire. Monter un réseau iroh réel de N nœuds
   dépassait le budget. Les constats C-01 à C-04 ne dépendent pas du réseau (ce sont
   des règles de validation), mais l'**exploitabilité opérationnelle** (fenêtres de
   course, propagation gossip, cadence réelle) n'est pas mesurée.

2. **Le rejeu de transaction inter-réseaux** (H-05, second chemin). Prouvé par
   lecture de la préimage (`mod.rs:1471-1482` : aucun identifiant de chaîne) et par
   analogie exacte avec le rejeu de votes qui, lui, est prouvé par test. Monter deux
   réseaux distincts partageant une clé n'a pas été fait.

3. **L'atteignabilité réseau de M-12** (reorg franchissant la maturation). Le bug
   est prouvé au niveau de l'état interne ; établir qu'un attaquant peut vraiment
   pousser un reorg de plus de 10 080 blocs demanderait de contourner
   `FORK_BUFFER_MAX_BLOCKS` ou d'enchaîner des adoptions successives, ce que je n'ai
   pas construit.

4. **La persistance libsql** (`storage/`) : hors périmètre. Or `LedgerSnapshot`
   restaure `finalized_floor_index` et `account_nonces` **sans** revalider la chaîne
   (`mod.rs:1720-1748`) — un snapshot corrompu ou substitué injecte directement un
   état de consensus. La surface « qui peut écrire ce fichier » n'a pas été
   examinée. **Non prouvé.**

5. **Les chemins RPC/Tauri** (`rpc.rs`, `commands/`) : hors périmètre, non lus
   intégralement. Ils exposent des vues et des commandes de portefeuille qui
   touchent le ledger ; une écriture non authentifiée y serait un contournement de
   tout ce qui précède.

6. **Analyse cryptographique de fips204 / ML-DSA-65** : hors périmètre (l'auditeur
   crypto sœur s'en charge). J'ai supposé `verify_pq` correct. Si `verify_pq` est
   faux, C-01 et H-05 empirent, rien ne s'améliore.

7. **La question « le nonce est-il monotone après un reorg »** n'a de réponse
   qu'en creux : le nonce n'étant **pas** un état de consensus (C-01), il n'est ni
   réappliqué ni révisé lors d'un reorg — `account_nonces` n'apparaît ni dans
   `pop_above`, ni dans `revert_block_stake_effects`, ni dans `rebuild_cache`. Il
   n'est donc qu'un compteur local de portefeuille, monotone par construction
   (`raise_nonce_high_water`, `mod.rs:737`), et sans effet sur l'admission d'un bloc.

8. **Le budget d'exécution.** Les tests de ce rapport sont volontairement petits et
   déterministes. Je n'ai pas lancé de campagne `proptest` dédiée aux constats
   (par exemple : « existe-t-il une combinaison de montants qui passe *à la fois*
   la borne d'émission enroulée et le plan de récompense ? » — j'ai donné un
   argument structurel, pas une preuve exhaustive par recherche).

---

## Annexe — reproduction

Les 21 tests sont dans un module jetable `src/p2p/ledger/audit2026.rs` d'une **copie**
du dépôt (`/tmp/q_consensus_8ccf`), accroché par `#[cfg(test)] mod audit2026;` dans
`src/p2p/ledger/mod.rs` de cette copie uniquement. **Aucun fichier suivi par git n'a
été modifié, rien n'a été commité ni poussé.**

```
cargo test --lib audit2026 -- --nocapture --test-threads=1     # debug : A1/A2/E1 paniquent (attendu)
cargo test --release --lib audit2026 -- --nocapture            # release : montre l'enroulement
```

| test | constat |
|------|---------|
| `audit_a1`, `audit_a2` | H-07 (débordement d'émission, debug + release) |
| `audit_a3` | M-10 (pas de borne de taille de bloc) |
| `audit_b1`, `audit_b1b` | C-01 (rejeu de tx, vidage de compte) |
| `audit_b2` | C-01 (nonce non vérifié à l'inclusion) |
| `audit_b3` | C-02 (timestamp non validé) |
| `audit_b4` | M-09 (`energy_kwh` malléable) |
| `audit_c1` | C-04 (départage grindable) |
| `audit_c2bis` | H-08 (sybil de participation) |
| `audit_c3` | C-03 (long-range à 1 QTA) |
| `audit_c4` | H-06 (quorum pondéré par l'enjeu courant) |
| `audit_c5` | H-05 (slash d'un honnête par rejeu inter-chaînes) |
| `audit_d1` | conservation préservée par le rejeu (constat négatif utile) |
| `audit_d2`, `audit_d3` | C-01 sur la coinbase |
| `audit_e1` | M-11 (débordement Shapley) |
| `audit_f1` | C-04 (élection ignorée à la réception) |
| `audit_f2` | contrôle négatif : PROPOSER-1 et OPEN-DOOR-1 fonctionnent |
| `audit_g1` | M-12 (revert × maturation) |
| `audit_h1` | M-14 (coût linéaire en la hauteur) |
