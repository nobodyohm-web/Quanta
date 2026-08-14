# Remédiation — audit externe du 13 août 2026

**Cible** `quanta-protocol` 3.15.1 · protocole TORUS **v9 → v10**
**Source** `QUANTA_AUDIT.md` et ses cinq rapports détaillés (crypto, consensus, réseau, applicatif,
chaîne d'approvisionnement) — 85 constats, dont 13 critiques.

Ce document dit ce qui a été corrigé, **comment on le sait**, et ce qui reste ouvert. Il ne
revendique rien qu'un test ne tienne.

---

## 1. Le constat que l'audit a fait sur la méthode, et qui compte plus que la liste

> Le projet vérifie très bien ce qu'il a décidé de vérifier, et ne vérifie pas ce dont il n'a jamais
> écrit la règle. Chaque défense existante est bien faite ; ce sont les défenses **absentes** qui
> ouvrent le système, et elles sont absentes en silence, sans test rouge pour le dire.

Trois des correctifs ci-dessous ne corrigent pas un bug : ils **écrivent une règle qui n'existait
pas** (l'unicité on-chain d'une transaction, l'horodatage d'un bloc, la taille d'un bloc). Chacun
arrive avec le test rouge qui manquait.

Un détail mérite d'être relevé : pour la préimage injective, **le bon modèle était déjà dans le
dépôt** — `sm/finality_vote.rs::signable_bytes` fait exactement ce qu'il faut depuis GADGET-2
(séparateur de domaine, champs préfixés en longueur). Il n'avait simplement jamais été appliqué aux
transactions. Le correctif consiste à généraliser ce qui était déjà juste.

---

## 2. Rupture de protocole assumée — v9 → v10

Deux changements, chacun suffisant à casser la compatibilité :

- **CANON-1** — la préimage signée d'une transaction, la feuille Merkle et l'en-tête de bloc passent
  à un encodage **injectif** (séparateur de domaine + champs préfixés en longueur, type de
  transaction en tag numérique stable plutôt que `{:?}`).
- **NONCE-ONCHAIN-1** — le nonce de compte devient **séquentiel et vérifié à l'inclusion**.

Toute signature et tout hash changent, genèse comprise (`8a06b5f9…` puis, après l'ancrage chain-id,
`ecc2d774…`). Un nœud v9 et un nœud v10 ne peuvent pas converger ; l'échange est refusé au `Hello`.

---

## 3. Les treize critiques

| id | ce qu'un attaquant faisait | correctif | preuve |
|---|---|---|---|
| **R1** | Bannir n'importe quel nœud du réseau **sans posséder aucune clé** | `REPORT-NOAUTH-1` — à l'échec de signature : on jette, on compte, on ne dénonce **personne** | `r1_a_forged_envelope_denounces_nobody` |
| **CRIT-1** | Une signature autorisait **deux transactions différentes** | `CANON-1` — préimage injective | `crit1_tx_preimage_is_injective_across_field_boundaries`, `crit1_tx_type_tags_are_distinct_and_stable` |
| **C-01** | Rejouer une transaction signée **une seule fois**, jusqu'à vider la victime | `NONCE-ONCHAIN-1` — nonce séquentiel vérifié à l'inclusion, sur les 4 chemins d'admission | `c01_a_signed_tx_cannot_be_included_twice`, `c01_the_same_tx_ten_times_in_one_block_is_rejected` |
| **R2** | Voler n'importe quel `@pseudo` avec `claimed_at: 0` | `CLAIM-WINDOW-1` — dates bornées + fenêtre de contestation | `r2_a_claim_predating_the_protocol_is_refused`, `r2_an_established_name_cannot_be_taken_after_the_window` |
| **A1** | Les 41 commandes IPC échappaient à l'ACL de Tauri | Manifeste ACL applicatif déclaré dans `build.rs` ; chaque commande accordée explicitement | `gen/schemas/acl-manifests.json` contient désormais `__app-acl__` |
| **A2** | Le verrouillage du portefeuille était un `ready = false` en Svelte | `LOCK-1` — `CryptoEngine::lock()` + commandes `lock_wallet` / `is_wallet_unlocked` ; `get_recovery_phrase` exige le mot de passe | revue + `svelte-check` 0/0 |
| **A3** | Planter le cookie RPC pour obtenir l'autorité de dépense | `COOKIE-OWN-1` — propriété (preuve par `chmod`), permissions et non-lien-symbolique vérifiés avant adoption | revue |
| **H-07** | **Un seul message** rendait le nœud sourd à tout gossip, définitivement | Somme d'émission `checked` — le débordement est un rejet de bloc, identique en debug et en release | `h07_overflowing_block_emission_is_a_rejection_not_a_panic` |
| **R3** | OOM distant : 10 Mo par message, relayés et cachés 30 s **avant** authentification | Borne dérivée du plus gros message légal (4 Mio) + budget d'octets sur `ChainSegment` | revue |
| **C-02** | Le `timestamp` d'un bloc n'était validé **nulle part** | `BLOCK-TIME-1` — parsable RFC3339 et non décroissant vis-à-vis du parent | `c02_block_timestamp_must_parse_and_never_go_backwards` |
| **SC-01** | La clé de signature de l'updater passait à un tag mutable | 8 actions sur 8 épinglées par SHA (résolus via l'API GitHub), job de release sous `environment: release`, `permissions: {}` par défaut | `grep -rnE "uses: [^@]+@(v[0-9]\|main\|master\|stable)"` → vide |
| **SC-02** | `cargo deny check` renvoyait 0 avec 4 vulns et un use-after-free | `unmaintained = "all"`, `unsound = "all"`, `unused-ignored-advisory = "deny"`, 6 `ignore` périmés supprimés | `cargo deny check` exécuté localement |
| **C-03 / C-04** | Long-range / nothing-at-stake ; élection PoS non appliquée à la réception | **OUVERT** — chantier de conception (VRF, ADR-004), pas un correctif | — |

---

## 4. Les autres constats traités

| id | objet | correctif |
|---|---|---|
| **MOY-1** | Feuille Merkle et en-tête de bloc ambigus | Même encodage injectif, domaines séparés ; la signature et la couche ML-DSA entrent dans la feuille en champs préfixés |
| **H-05** | Pas de chain-id : rejeu inter-réseaux, **slash d'un validateur honnête** | `CHAIN-ID-1` — `crate::CHAIN_ID` lié aux préimages de transaction **et** de vote de finalité |
| **HAUT-1** | `verify_multisig` : `K×S` vérifications ML-DSA non bornées (~135 s CPU/message) | `MSIG-BOUND-1` — 16 clés / 16 signatures max, vérifié **avant** toute vérification |
| **M-09** | `energy_kwh` hors du hash de bloc (bloc malléable) | Traité par la réécriture de l'en-tête ; le champ reste hors chemin monétaire |
| **M-10** | Aucune borne de taille ni de nombre de transactions par bloc | `BLOCK-SIZE-1` — `MAX_TXS_PER_BLOCK = 256`, vérifié **avant** toute vérification de signature |
| **M-11** | `+=` u64 sur des compteurs auto-déclarés ⇒ panique du tick de minage | Clamp à l'admission + `saturating_add` dans `NetworkTotals` |
| **R4** | `decompress_blocks` bornait les octets, pas le **nombre d'éléments** | Plafond d'octets aligné sur l'enveloppe + cardinalité refusée avant allocation |
| **R13** | `Hello.known_peer_ids` / `heads` sans plafond ; `collect()` avant `.take(3)` | `.take` remonté avant le filtre + longueur d'identifiant bornée |
| **A4** | `sendtoaddress` dépensait sans déverrouillage ni plafond | `SEND-OPTIN-1` — fermé par défaut (`QUANTA_RPC_ALLOW_SEND=1`), plafonné |
| **A6** | `quanta.db` (coffre + graine ML-DSA) en 0644 | 0600 sur la base et ses journaux `-wal`/`-shm`, 0700 sur le répertoire |
| **A11** | L'écran de confirmation d'envoi n'affichait jamais l'adresse résolue | L'adresse réellement signée est affichée, en entier |
| **A13** | CSP : `script-src 'unsafe-inline'`, `connect-src` vers `api.github.com` | `connect-src 'self'` + directives manquantes ; SvelteKit passe en CSP **hash** (les deux politiques s'intersectent) |
| **A14** | Cookie RPC écrit sous l'umask puis `chmod` (TOCTOU) | Créé directement en 0600 (`OpenOptions::mode`) |
| **SC-07** | `overflow-checks` OFF en release, ON en test | `[profile.release] overflow-checks = true` |
| **SC-08/11/15** | `npm audit` absent, pas de `--locked`, `.gitignore` incomplet | Traités en CI et dans `.gitignore` |
| **SECURITY.md** | Quatre affirmations fausses, dont deux servant de justification | Réécrites et datées |

---

## 5. Ce qui reste ouvert, et pourquoi

- **C-03 / C-04 / M-13 — fork-choice et élection.** « Plus longue chaîne + départage
  lexicographique », sans coût ni pondération par l'enjeu, et l'appartenance à l'ensemble bondé
  tenant lieu d'élection à la réception. C'est un chantier de conception (VRF, ADR-004), pas un
  correctif. **Le réseau ne doit porter aucune valeur tant qu'il est ouvert.**
- **Borne de dérive d'horloge sur les blocs.** Délibérément absente : sans NTP elle produirait des
  forks (deux nœuds désynchronisés divergeraient sur la validité d'un même bloc), et le grinding
  qu'on lui prête de fermer reste illimité de toute façon — un timestamp RFC3339 porte des fractions
  de seconde. Voir le commentaire de `validate_block_timestamp`.
- **`REPORT_BAN_THRESHOLD` compte des identités.** Une identité ML-DSA coûte ~165 µs : trois
  marionnettes bannissent encore n'importe qui. Adosser le signalement à un coût non falsifiable
  (l'enjeu bondé) est une décision de conception non prise ici.
- **HAUT-2 / HAUT-3 — TOFU sur l'identité de fonds.** Un `pq_identity_v1` absent fait fabriquer une
  clé de fonds neuve au déverrouillage. Sabotage/rançon, pas vol. Non corrigé : le correctif exige
  de persister l'adresse attendue à côté du keypair, donc une migration de schéma.
- **MOY-2 — sel Argon2id dérivé de la clé publique.** Correct à corriger, mais change le format du
  coffre : migration nécessaire.
- **Relais et cache pré-authentification de plumtree.** Comportement d'`iroh-gossip`, pas de code
  d'ici. Réduire la borne divise le coût par 2,5 ; elle ne le supprime pas.
- **Deux actions manuelles de chaîne d'appro.** Créer l'environnement GitHub `release` et y déplacer
  `TAURI_SIGNING_PRIVATE_KEY` ; `npm audit fix` (`nanoid`). Voir `.audit_fixes/DONE_supplychain.md`.

---

## 6. État de la vérification

- `cargo test --lib` — **533 tests verts** (513 avant l'audit, +20 de non-régression).
- `cargo clippy --all-targets -- -D warnings` — **RC=0**.
- `npx svelte-check` — **0 erreur, 0 avertissement**.
- Non fait, et donc non revendiqué : aucun essai sur un **réseau réel**, aucune mesure de temps
  constant, aucun fuzzing, aucune revue de `fips204` lui-même.

---

## 7. Constats supplémentaires traités dans la même passe

| id | objet | correctif |
|---|---|---|
| **A9** | `ui_diag` écrivait `msg` brut, sans borne, sans rotation, en 0644 — un `\n` fabriquait des lignes de journal | `DIAG-SANITIZE-1` — une seule ligne (caractères de contrôle échappés), 2 Kio max, rotation à 256 Kio, fichier en 0600 |
| **A16** | Les réponses RPC n'émettaient ni `nosniff`, ni CSP, ni `frame-ancestors` | Trois en-têtes de sécurité sur **toute** réponse (JSON et HTML) |
| **SC-06** | La cible de fuzz mourait à 100 % au mur de signature ML-DSA : couverture réelle du parseur **nulle** | Seconde porte `fuzz_parse_payload` qui commence **après** l'authentification (serde par variante, hex, gzip) |

## 8. Reproduire la vérification

```bash
cd src-tauri
cargo test --lib                                  # 533 verts
cargo test --test '*'                             # 1 vert (intégration deux nœuds)
cargo clippy --all-targets -- -D warnings         # RC=0
cargo deny --manifest-path Cargo.toml check       # advisories/bans/licenses/sources ok
cd .. && npx svelte-check                         # 0 erreur, 0 avertissement
```

---

## 9. Deuxième passe (2026-08-14) — les chantiers de conception

La première passe fermait les trous. Celle-ci attaque ce que le §5 avait nommé « chantier de
conception, pas correctif ». Trois y passent en entier.

### 9.1 C-04 — FORK-RANK-1 : le départage de fork cesse d'être gratuit

Le fork-choice était « le plus grand hash gagne ». Le `timestamp` entre dans le hash, donc un
proposeur broyait quelques milliers de BLAKE3 et **gagnait tous les départages**, sans posséder un
µQTA de plus que son voisin. L'élection pondérée par l'enjeu existait pourtant déjà — elle ne servait
qu'au scellement.

Le départage devient `(rang d'élection du proposeur, hash)`. Le rang est la position dans un
classement total de l'ensemble bondé *as-of-parent*, par tirages pondérés **sans remise** sur la
graine de `elect_leader`. Ses trois entrées — beacon enterré, hauteur, ensemble bondé chez le parent —
ne sont dans aucun des deux blocs concurrents : **le broyage ne déplace plus rien**. Le cas ex æquo
est exactement l'auto-équivocation, que le gadget de finalité punit.

Détail complet, y compris pourquoi ni VRF ni horloge : [`docs/DESIGN-FORK-RANK.md`](../DESIGN-FORK-RANK.md).

### 9.2 C-03 — REORG-DEPTH-1 et GENESIS-ANCHOR-1 : le long-range

- Aucune réorganisation de plus de **128 blocs**, quel que soit son score et **indépendamment de la
  finalité** — le plancher LIVE-2 ne protège que ce que les votes ont atteint, et tant que personne
  ne vote il vaut 0. Le prix est nommé : au-delà, une partition ne guérit plus seule.
- `verify_chain` vérifiait « bien chaînée », jamais « chaînée à *notre* genèse ».

Effet de bord gratuit : **M-12** (un reorg franchissant une maturation d'unbonding fabriquait du
poids de consensus) devient impossible par construction, et une `const _: () = assert!` grave le lien
`MAX_REORG_DEPTH < UNBONDING_PERIOD_BLOCKS` — si quelqu'un retouche l'une des deux constantes, **le
crate ne compile plus**.

### 9.3 H-08 — REWARD-WEIGHT-1 : le Sybil devient neutre

Le pot de participation se partageait **à parts égales entre adresses distinctes**. Une adresse ne
coûte rien : 45,2 % de chaque récompense captés avec 28 identités, contre 12,5 % avec une seule. On
pondère désormais par le **nombre de blocs effectivement produits** dans la fenêtre. Les slots sont
une ressource finie : scinder son identité en K ne produit pas un bloc de plus, donc ne rapporte
rien. Pas de pondération par l'enjeu — elle recréerait la rente de capital que la doctrine refuse, et
elle divergerait entre produire et vérifier selon le chemin d'admission.

### 9.4 BLOCK-TIME-2 — un cliquet introduit par notre propre correctif

Trouvé en relisant BLOCK-TIME-1 (première passe). La règle `ts >= prev.ts` fermait le recul mais
créait pire : un **unique** bloc daté de 2099 était parsable, supérieur à son parent, donc accepté —
et **plus aucun bloc honnête ne pouvait jamais être scellé**. Un champ texte gelait la monnaie pour
soixante-treize ans. La règle passe au *median-time-past* de Bitcoin (fenêtre de 11) : la médiane ne
bouge que si une majorité de la fenêtre ment, donc le menteur isolé ne déplace rien. Toujours sans
horloge.

### 9.5 Le reste de la deuxième passe

| id | objet | correctif |
|---|---|---|
| **H-06** | Un certificat de finalité valait ce que l'instant présent lui accordait : le même certificat inchangé échouait puis réussissait quand d'autres se désengageaient | Ensemble d'enjeu **figé à la frontière d'époque**, fonction pure de la chaîne — rien à persister, rien qui puisse diverger |
| **M-09** | `energy_kwh` voyageait dans le bloc, s'affichait, mais n'entrait pas dans le hash : champ réécrivable par n'importe quel relais | Entre dans la préimage d'en-tête (`to_bits`, `-0.0` normalisé) ; NaN et infinis refusés en amont |
| **MOY-6** | `tx.signature` : exigé non vide, jamais vérifié, et pourtant dans la feuille Merkle — deux variantes d'une même tx scellaient des blocs différents | Retiré de la feuille et de l'exigence. Une contrainte qui n'impose que la présence d'octets arbitraires est un faux témoin |
| **B-15** | `SLASH_EVIDENCE_WINDOW_BLOCKS` déclarée, documentée « GRAVÉE par ADR-009 », protégée par une assertion… et appliquée nulle part | Fenêtre réellement appliquée dans la règle partagée seal/receive |
| **B-16** | `total_supply` soustrayait deux bases différentes : compteur public faux dès qu'un burn était en attente | Même base des deux côtés, dans le ledger et dans la vue |
| **SC-06** | La cible de fuzz mourait à 100 % au mur ML-DSA : couverture réelle nulle | Seconde porte commençant **après** l'authentification |
| **HAUT-2/3, MOY-2/4/5** | Blob de fonds absent ⇒ clé neuve fabriquée ; sel Argon2id dérivé de la clé publique ; phrase de récupération non effacée | Ancre d'adresse `pq_fund_anchor_v1` + refus, sel aléatoire persisté, `SecretString(Zeroizing)`, **avec migration ancien→neuf testée** |
| **R12, R5, R6/R11/R16, R10, A5, A8** | Files non bornées, amplification O(N²), travail avant authentification, bannissement *fail-open*, RPC sans plafond de connexions, rebinding DNS | Voir `.audit_fixes/DONE_net2.md` |

### 9.6 Ce qui reste ouvert après la deuxième passe

- **Grinding de la graine d'élection.** Le proposeur de `h − LEADER_ENTROPY_LOOKBACK` influence le
  beacon de `h`. Le lookback rend l'attaque non triviale, il ne la supprime pas. Fermeture propre :
  VDF ou RANDAO à révélation différée.
- **Prédictibilité de l'élection.** Sans VRF, le leader de `h+1` est connu à `h`, donc ciblable. Il
  n'existe pas de VRF post-quantique déployable ; le fallback par rang limite le dommage.
- **M-14 — coût de validation linéaire en l'histoire.** L'attaquant paie O(1), la victime O(hauteur).
  Correctif = caches incrémentaux, chantier de performance non entamé.
- **MOY-3 — contexte FIPS-204 vide.** Le poser d'un seul côté invaliderait toutes les signatures du
  réseau ; à faire des deux côtés en une passe, sous rupture de protocole.
- **BAS-1 — `address::parse` accepte l'hex sans checksum.**
- **R14 — code mort dans le chemin réseau** : signalé, **non supprimé** (règle du dépôt : on demande
  avant de retirer du code intentionnel). Décision attendue.
- **Ancre de fonds non signée**, dans la même base que le coffre : effacer les deux lignes rejoue le
  TOFU une fois. Une ancre inviolable demande un magasin séparé (Keychain/TPM).

**Le fork-choice est désormais pondéré par l'enjeu et la profondeur de reorg est bornée. Ce n'est
toujours pas une preuve de sûreté : le réseau ne doit porter aucune valeur.**

## 10. État de la vérification (2026-08-14, deuxième passe)

- `cargo test --lib` — **587 tests verts** (513 avant l'audit, +74).
- `cargo test --test '*'` — 1 vert (intégration deux nœuds).
- `cargo clippy --all-targets -- -D warnings` — **RC=0**.
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok.
- `npx svelte-check` — **0 erreur, 0 avertissement**.
- Chaque correctif majeur a été vérifié **rouge sans lui** : sabotage temporaire, suite relancée,
  fichier restauré.

---

## 11. Troisième passe (2026-08-14) — la queue de liste

| id | ce qui n'allait pas | correctif |
|---|---|---|
| **R14** | `wrap_outgoing`, `wrap_outgoing_with_nonce`, `payload_bytes` : trois fonctions publiques, documentées, portant les noms les plus évidents du module — **et sans un seul appelant**. Les enveloppes qu'elles produisaient étaient rejetées deux fois (identifiant calculé sur le payload seul, `nonce: 0` figé) | **Supprimées.** Ce n'était pas une faille, c'était un piège : un développeur pressé les aurait appelées en croyant bien faire et aurait émis des messages que tout le réseau ignore **en silence** — la pire classe de panne, celle qui ne lève aucune erreur. Un commentaire à leur place dit pourquoi elles ne doivent pas repousser |
| **R15** | Le `NonceTracker` (anti-rejeu + bannissements) n'était **pas persisté** : une enveloppe authentique capturée avant l'arrêt redevenait acceptable après — et un attaquant peut provoquer le redémarrage — tandis qu'un pair banni pour abus repartait avec une ardoise nette | 8ᵉ clé de snapshot. Restauration **monotone** sur les nonces (`max`, jamais redescendre), bans expirés **non** ressuscités. Les compteurs de débit restent volatils : ils sont fenêtrés sur le temps courant, les restaurer fausserait la fenêtre |
| **MOY-3 / H-05** | La préimage de revendication de pseudo était `format!("QUSER\|{}\|{}\|{}\|{}", …)` : injective par **accident de format**, pas par règle, et sans chain-id | Encodage canonique du projet (domaine + champs préfixés en longueur) + `CHAIN_ID`. Même correctif que CRIT-1, sur le champ le plus proche de l'utilisateur |
| **BAS-1** | `address::parse` retombait silencieusement sur `hex::decode` : la somme de contrôle disparaissait pour tout appelant recevant de l'hexadécimal — c'est-à-dire les **deux chemins de dépense**. Une adresse hexadécimale d'un caractère faux reste 64 caractères hexadécimaux valides | `parse` devient un décodeur Bech32m **strict** ; la tolérance existe encore mais s'appelle `parse_hex_unchecked` et dit ce qu'elle coûte. `validateaddress` (RPC) est strict — c'est la fonction qu'un échange appelle avant de créditer. L'aperçu d'envoi **avertit** quand le destinataire est de l'hexadécimal nu, dans les six langues |

### Note de méthode

La séparation de domaine FIPS-204 (`ctx`) évoquée par MOY-3 n'a **pas** été posée sur les quatre
domaines. Le paramètre doit être identique à la signature et à la vérification, sur vingt-cinq sites ;
une asymétrie invalide toutes les signatures du réseau, silencieusement. Le bénéfice réel est faible —
les quatre préimages portent déjà un séparateur de domaine explicite depuis CANON-1, H-05 et ce
correctif-ci — et l'audit note lui-même que le rejeu inter-domaines n'est pas exploitable. On a donc
corrigé le maillon faible réel (la préimage de pseudo) plutôt que d'ajouter une seconde ceinture au
prix d'un risque de rupture. **C'est une décision, pas un oubli.**

## 12. État final de la vérification

```
cargo test --lib                             -> 592 verts   (513 avant l'audit, +79)
cargo test --test '*'                        -> 1 vert       (intégration deux nœuds)
cargo clippy --all-targets -- -D warnings    -> RC=0
cargo deny --manifest-path src-tauri/Cargo.toml check -> advisories/bans/licenses/sources ok
npx svelte-check                             -> 0 erreur, 0 avertissement
```
