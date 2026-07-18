# Quanta — Prêt pour la cotation (Listing Readiness)

> Cartographie honnête des exigences **réelles** des grands exchanges (Coinbase, Kraken,
> Binance) face à l'**état réel** de Quanta. Compagnon de [`../economy/DOCTRINE.md`](../economy/DOCTRINE.md) §IX.
>
> **Recherche : juillet 2026.** Cadres cités : interprétation conjointe **SEC + CFTC** de mars
> 2026 (*digital commodity*), **MiCA** (UE, whitepaper obligatoire depuis janvier 2025), **FATF**
> Travel Rule. Les exigences bougent — re-vérifier avant toute démarche.

---

## Avertissement d'honnêteté

Trois vérités à garder en tête en lisant ce document :

1. **Aucun des trois exchanges ne publie de grille chiffrée.** Pas de seuil public de market cap,
   de volume, de nombre de détenteurs, de % de décentralisation, de liste d'auditeurs accrédités,
   ni de barème. C'est **délibéré** (Binance l'assume : éviter le *reverse-engineering* des
   critères). Ce document ne contient donc **aucun seuil inventé** — quand un chiffre circule dans
   la presse, il est marqué comme informel.
2. **La plupart des critères sont discrétionnaires et privés**, traités en revue interne sous NDA.
   Coinbase déclare rejeter **~90 %** des actifs examinés. « Répondre aux exigences » ne garantit
   jamais une cotation.
3. **Quanta est alpha, non audité.** Rien ici ne prétend que Quanta est *prêt* — ce document dit
   *ce qui est déjà un atout* et *ce qui manque*, sans maquillage.

Légende : ⭐ **atout** (Quanta dépasse l'attendu) · ✅ **satisfait** · 🟡 **partiel** · 🔴 **gap** ·
🧊 **hors code** (dépend de tiers, budget ou adoption — non livrable par du logiciel).

---

## Les 4 découvertes qui changent la donne

Avant les tableaux, les points non évidents que la recherche a fait remonter :

1. **Coinbase exige Rosetta/Mesh pour tout L1 natif.** C'est la **seule** exigence technique
   *nommée et publiée* par un grand exchange : une implémentation de la spec **Mesh** (ex-Rosetta)
   — *Data API* (lecture d'état) + *Construction API* (transferts, staking) dans un format
   standardisé. C'est notre cible d'ingénierie la plus concrète pour Coinbase.

2. **⚠️ Tension post-quantique ↔ custody institutionnelle.** Les grands dépositaires (BitGo,
   Fireblocks, Copper) sécurisent les fonds des exchanges par **MPC / signature à seuil**. Or
   **ML-DSA-65 (notre autorité de compte post-quantique) n'a aucun support MPC/threshold mature**
   chez ces trois-là aujourd'hui. Notre plus grande force de sécurité est donc, paradoxalement, un
   **point de friction pour la custody d'exchange.** C'est une décision stratégique, pas un bug
   (voir §Décisions).

3. **MiCA est le seul mur légal *daté et contraignant*.** Pour être coté dans l'UE : un
   **whitepaper conforme** (format machine-readable iXBRL, taxonomie ESMA), un **LEI** et un **DTI**,
   soumis **≥ 20 jours ouvrés** avant. **Sans lui, un exchange UE doit délister.** Kraken précise
   qu'il ne rédige pas le whitepaper — c'est au projet.

4. **Le profil économique de Quanta est un vrai atout légal.** Le nouveau cadre SEC+CFTC (mars
   2026) définit une *digital commodity* comme un actif dont la valeur vient du **fonctionnement
   programmatique du système, pas des efforts d'un tiers**. Miné, zéro premine, zéro ICO, zéro
   entreprise, **zéro autorité d'émission** : Quanta coche ce profil (proche de Bitcoin). *(Ce
   n'est pas un avis juridique — il en faudra un ; mais la structure joue pour nous.)*

---

## Front A — TECHNIQUE

| Exigence (source) | État de Quanta | Statut |
|---|---|---|
| **Rosetta/Mesh** (Coinbase, requis pour L1 natif) | Rien | 🔴 |
| **Nœud *headless* (daemon)** opérable par l'exchange | Quanta est une **app de bureau Tauri**, pas de daemon | 🔴 |
| **API/RPC stable** (gen/validation d'adresse, solde, build/sign/broadcast, scan des dépôts, historique) | Aucune surface RPC exposée | 🔴 |
| **Format d'adresse documenté + validation** | Adresse = `BLAKE3(ADDR_DOMAIN ‖ clé ML-DSA)`, déterministe — mais **pas de spec publique ni de format humain à checksum** (type Base58Check/Bech32) ni de lib de validation | 🟡 |
| **Dérivation HD** (BIP32/44 ; SLIP-44 exige un wallet BIP44 fonctionnel *avant* enregistrement) | Graine → clé ML-DSA via le vault (Argon2id), **pas** un arbre BIP32/44 standard ; ML-DSA hors des courbes usuelles | 🔴 |
| **Politique de confirmations / finalité** (crédit du dépôt) | **Finalité déterministe Casper-FFG** (`sm/finality_rule.rs`, quorum ⅔, `finalized_floor_index`) → réponse **nette** « crédit à la finalisation ». Les exchanges valorisent exactement ça (crédit quasi-immédiat post-finalité vs N confirmations probabilistes) | ⭐ |
| **Anti-rejeu + txid non-malléable** | Nonce anti-replay par compte, **BLAKE3 tx IDs**, Merkle root, signature canonique | ✅ |
| **Cold storage / MPC / multisig** | Signage hors-ligne via le vault ; **mais pas de MPC/threshold ML-DSA chez les dépositaires** (voir Découverte 2) | 🔴 🧊 |
| **Testnet public** | Aucun | 🔴 |
| **Explorateur de blocs public** | Un explorateur *in-app* existe, mais **pas d'explorateur web public** | 🔴 |
| **Gestion des reorgs** exposable au backend exchange | Résolution de fork déterministe + plancher de finalité (le reorg sous plancher est refusé) — solide, mais **pas exposé via RPC** | 🟡 |
| **Code source public et vérifiable** (Coinbase : privé = *red flag*) | **Apache-2.0, ouvert sur GitHub** | ⭐ |

**Lecture** : le socle de *correctness* (finalité, anti-rejeu, code ouvert) est bon, parfois un
atout. Le manque est la **surface d'intégration** : Quanta ne parle pas encore le langage d'un
exchange (daemon + RPC + Mesh + adresses standard + testnet + explorer). C'est le **gap n°1**, et
c'est **le seul front que du code peut fermer** (voir Roadmap).

---

## Front B — SÉCURITÉ

| Exigence (source) | État de Quanta | Statut |
|---|---|---|
| **Audit de sécurité par un tiers reconnu** (Coinbase : « pas requis mais l'absence ralentit », surtout si code *complexe/novel* — ce qui est notre cas ; attendu de facto) | **Non audité** | 🔴 🧊 |
| **Least-privilege** — *red flag* si actions admin unilatérales, saisie de fonds, clé admin individuelle (Coinbase) | **Zéro autorité de mint, zéro admin, zéro saisie possible.** Il n'existe pas de fonction pour créer, geler ou saisir | ⭐ |
| **Consensus / résilience réseau / gouvernance** (Coinbase *Technical Security review*) | Gadget de finalité prouvé en simulation DST multi-seed ; **mais alpha, un seul test réel 2-machines** — pas de track-record à l'échelle | 🟡 |
| **Bug bounty du projet** | Aucun — **et ce n'est pas exigé** des projets candidats (les bounties Coinbase/Kraken couvrent *leur* infra, pas la nôtre) | ✅ (n/a) |

**Lecture** : le **least-privilege est un atout majeur** — la structure « pas de prometteur » de la
doctrine est exactement ce que Coinbase cherche. Le mur dur est l'**audit tiers** : incontournable,
non codable, il faut un cabinet et un budget.

---

## Front C — JURIDIQUE / CONFORMITÉ

| Exigence (source) | État de Quanta | Statut |
|---|---|---|
| **Profil *digital commodity*** (SEC+CFTC, mars 2026 : valeur du fonctionnement programmatique, pas d'efforts d'un tiers) | Miné, zéro premine/ICO/entreprise, zéro autorité d'émission → **profil favorable** (proche BTC) | ⭐ *(sous réserve d'avis juridique)* |
| **Avis juridique security/commodity par juridiction** | Aucun | 🔴 🧊 |
| **Whitepaper MiCA conforme** (iXBRL, taxonomie ESMA) + **LEI** + **DTI**, soumis ≥ 20 j ouvrés — sinon délisting UE | Aucun | 🔴 🧊 |
| **Entité / sponsor du dossier** (un coin décentralisé *peut* être listé sans émetteur — BTC — mais quelqu'un doit porter l'intégration + la paperasse) | Aucune entité | 🔴 🧊 |
| **Travel Rule (FATF)** — côté exchange, facilitée par un ledger traçable | Ledger **transparent** (adresses/tx visibles) → aucun frein | ✅ |
| **Filtrage sanctions OFAC** — côté exchange | n/a (responsabilité de l'exchange) | ✅ |
| **Risque privacy-coin** (Monero/Zcash délistés faute de traçabilité) | **Pas un privacy-coin** : montants et parties visibles ; la couche PQ concerne la *signature*, pas l'obfuscation | ⭐ |

**Lecture** : le profil *commodity* et la transparence jouent **pour** nous. Mais **MiCA est un mur
daté** (whitepaper iXBRL + LEI + DTI) et rien de tout cela n'est du code : il faut un avocat, une
entité, et un whitepaper conforme.

---

## Front D — MARCHÉ / ADOPTION

| Exigence (source) | État de Quanta | Statut |
|---|---|---|
| **Transparence de l'offre circulante** (Coinbase ; MiCA la rend légalement obligatoire) | **Offre prouvable on-chain** : plafond 100M vérifié au consensus, zéro premine, circulation vérifiable sans faire confiance à personne | ⭐ |
| **Distribution décentralisée** (aucun seuil public, mais critère de revue) | Anti-baleine par le *Dividende du Commun* (roadmap doctrine) + zéro premine — **mais réseau alpha, minuscule** | 🟡 |
| **Teneur de marché nommé / liquidité engagée** (Kraken exige un market maker au listing ; Binance : arrangement MM avant candidature) | Aucun marché, aucun MM | 🔴 🧊 |
| **Volume / détenteurs / communauté** | Alpha, minimal | 🔴 🧊 |
| **MVP fonctionnel** (Binance : sinon la candidature est rejetée) | Application fonctionnelle + réseau P2P vérifié 2-machines | 🟡 |

**Lecture** : la **transparence de l'offre est un atout fort** (rare et exactement demandé). Tout le
reste du front marché — liquidité, teneurs de marché, volume, communauté — **ne se code pas.** C'est
de l'adoption, dans le temps.

---

## Bilan en une image

```
  Front           │ Atouts réels de Quanta            │ Ce qui manque
 ─────────────────┼───────────────────────────────────┼──────────────────────────────
  Économique  🟢  │ offre prouvable, non-security,     │  (rien — c'est notre force)
                  │ finalité déterministe, anti-baleine│
  Technique   🔧  │ finalité, anti-rejeu, code ouvert  │  nœud RPC · Mesh · adresses HD ·
                  │                                    │  testnet · explorer  ← CODABLE
  Sécurité    🛡️  │ least-privilege (zéro admin)       │  audit tiers            ← $ + cabinet
  Juridique   ⚖️  │ profil commodity, transparent      │  avis légal · MiCA · entité ← avocat
  Marché      📈  │ offre transparente                 │  liquidité · market maker · adoption
```

**Le seul front que du code ferme, c'est le technique. Les trois autres murs (audit, juridique,
liquidité) exigent des tiers, du budget et de l'adoption — pas du logiciel.**

---

## Roadmap — dans l'ordre

### Phase 1 — La surface d'intégration (code, ce qu'on peut faire)
Rendre Quanta « parlable » par un exchange. Chaque brique débloque la suivante.

1. **`quanta-node` headless** — extraire le cœur P2P/ledger de l'app Tauri en un **binaire daemon**
   (sans UI), configurable, loggable, avec arrêt gracieux. *Fondation de tout le reste.*
2. **API JSON-RPC** sur le daemon : `getnewaddress`, `validateaddress`, `getbalance`,
   `listdeposits`/scan par bloc, `buildtx` / `signtx` / `sendtx`, `getblock`, **hauteur de
   finalité**. Stable, versionnée, documentée. *(Le patron de facto est le RPC de Bitcoin Core.)*
3. **Format d'adresse public + validation** — spécifier un format humain à checksum (type
   Bech32m) au-dessus de `BLAKE3(ADDR_DOMAIN ‖ clé)`, et une lib de validation. *Prérequis wallets.*
4. **Testnet public + explorateur web** — un réseau de test ouvert et un explorateur consultable
   (retrouver une tx par ID) ; briques open-source standard.
5. **Implémentation Mesh (ex-Rosetta)** — Data API + Construction API par-dessus le RPC.
   *L'exigence nommée de Coinbase.*
6. **Wallet HD + SLIP-44** — dérivation déterministe (une graine → N adresses) et demande d'un
   `coin_type` SLIP-44 (nécessite un wallet BIP44 fonctionnel). ⚠️ ML-DSA hors des courbes usuelles
   → il faudra probablement **spécifier une dérivation HD propre à Quanta** (chantier de conception).

### Phase 2 — Les murs non-codables 🧊 (tiers, budget, temps)
À mener en parallèle, mais hors du code :
- **Audit de sécurité** par un cabinet reconnu (Trail of Bits, Least Authority, Quantstab, …).
- **Juridique** : entité/fondation porteuse, avis *commodity/security* par juridiction,
  **whitepaper MiCA** conforme (iXBRL) + LEI + DTI pour l'UE.
- **Marché** : accord avec un **teneur de marché**, amorçage de liquidité, communauté, volume.

---

## Décisions stratégiques (pour Alexandre)

Deux choix ne relèvent pas de l'ingénierie de routine et t'appartiennent :

1. **⚠️ Post-quantique vs custody institutionnelle.** ML-DSA-65 n'a pas de support MPC/threshold
   chez BitGo/Fireblocks/Copper. Options : (a) accepter un cold storage à clé unique au départ ;
   (b) concevoir un **multisig / seuil ML-DSA au niveau protocole** (chantier de recherche, mais qui
   *renforcerait* Quanta) ; (c) offrir une voie de dépôt à signature classique (— trahit la
   posture PQ, à éviter). C'est le seul endroit où notre plus grande force devient un obstacle.
2. **Jusqu'où pousser les fronts non-codables.** L'audit, le juridique et la liquidité coûtent de
   l'argent et du temps, et sortent de ce que je peux livrer. Il faut décider **si et quand** on
   engage ces dépenses — ou si on vise d'abord une cotation sur des plateformes plus légères
   (DEX, exchanges régionaux) où les barrières sont moindres, avant les tier-1.

---

## Ce qu'on ne poursuit PAS (pièges à seuils fantômes)

Ces « exigences » circulent mais **ne sont publiées par aucun exchange** — ne pas les traiter comme
des cibles fermes : market cap / volume / nombre de détenteurs minimum, % maximal de concentration,
liste de cabinets d'audit accrédités, montant de bug bounty du projet, ancienneté minimale du
mainnet, frais de listing réels (contestés chez Binance). Tout cela est discrétionnaire et interne.

---

*Sources : recherche web sourcée (juillet 2026) sur documents officiels Coinbase (PDF *Listing
Prioritization*, guide DALG, Mesh spec), Kraken (`get-listed`, tables de confirmations), Binance
(FAQ listing, règles market maker 2026), cadres SEC+CFTC (mars 2026), MiCA/ESMA, FATF, et standards
ouverts (BIP32/44, SLIP-44, Mesh). Beaucoup de portails exact (Coinbase Asset Hub, formulaire Kraken)
sont derrière authentification — à vérifier manuellement au moment d'agir. Statut Quanta : alpha,
non audité, sans marché ni prix.*
