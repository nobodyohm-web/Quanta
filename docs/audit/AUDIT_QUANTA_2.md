# Audit Quanta — analyse complète et priorisée

> Périmètre réel de cet audit : documentation (README, WHITEPAPER FR/EN, CLAUDE.md,
> SECURITY.md, DEV_API.md), configuration (package.json, lockfile, vite/svelte/tsconfig,
> deny.toml), licence (LICENSE, NOTICE) et maquettes HTML. **Le code Rust `src-tauri/src/`
> n'a pas été fourni**, donc rien ici n'est une revue ligne par ligne de la logique de
> consensus, du ledger ou de la crypto. Les points marqués « à confirmer en code » nécessitent
> les sources pour être tranchés. Données vérifiées dans le lockfile le jour de l'audit.

Légende de sévérité : 🔴 critique · 🟠 élevé · 🟡 moyen · 🔵 vision (passer de solide à exceptionnel).

---

## 0. Ce qui est déjà fort (pour calibrer le reste)

Le fond est sérieux, et la critique qui suit n'a de sens que parce que la base mérite qu'on la pousse loin.

- Arithmétique de solde 100 % entière en `u64` µQTA. Zéro flottant sur les montants. C'est la bonne décision et elle est tenue partout dans la doc.
- Hybride post-quantique **actif** sur la couche valeur, avec la clé ML-DSA dérivée de la graine Ed25519 (XOF BLAKE3). Design élégant : aucun secret supplémentaire persisté, aucune migration de coffre. Peu de projets alpha ont déjà du PQ réel sur les signatures de transaction.
- Discipline Rust affichée : pas de `unwrap()`, `Result + ?`, `zeroize` sur les secrets, `tokio::sync` à travers `.await`, erreurs de déchiffrement opaques, aucune clé dans les logs.
- Sections « limites honnêtes » dans le whitepaper, le README et SECURITY.md. C'est rare et c'est la meilleure chose du projet sur le plan crédibilité.
- Hygiène supply-chain réelle : `deny.toml` détaillé avec triage daté, `cargo audit`, `clippy -D warnings`, scripts `ai:*`. Intention claire de porte d'approvisionnement en CI.
- Arbre de dépendances npm modeste (160 paquets résolus) pour un stack SvelteKit + Tauri.
- Trace d'audit interne documentée (corrections AUDIT-TX/BLK avec tests de régression). Le travail de sécurité a manifestement eu lieu.
- LICENSE Apache-2.0 correcte, ligne de copyright remplie (`Copyright 2026 Quanta Protocol Contributors`), aucun placeholder oublié.

---

## 1. Cohérence honnêteté : tes artefacts contredisent tes invariants 🔴

C'est la catégorie la plus importante **pour ce projet** parce que la promesse est l'honnêteté radicale. Chaque contradiction entre ce que tu affirmes et ce que livrent tes fichiers attaque directement la seule chose qui te différencie.

### 1.1 🔴 Un prix fiat inventé est affiché dans la maquette d'identité

`quanta-identity-preview.html` contient :

```html
<div class="bal-eur num">≈ 1 540,80 EUR</div>
```

et plus bas `≈ 1 540,80 EUR` dans le bandeau d'envoi (solde en EUR).

Or les deux whitepapers et le README répètent : « l'app n'affiche jamais de chiffre fiat inventé » / « the app never displays a fabricated fiat figure ». QUANTA n'est coté nulle part, donc ce 1 540,80 EUR est exactement le chiffre inventé que tu jures ne jamais montrer. La maquette `quanta-arc-mockup.html`, elle, respecte la règle (elle n'affiche aucun fiat). 

Action : retirer toute mention EUR de la maquette d'identité. N'afficher un équivalent fiat que le jour où un marché réel existe, et même alors via un oracle de prix vérifiable, pas une constante codée.

### 1.2 🔴 « Aucune requête HTTP sortante » est faux avec l'updater installé

SECURITY.md, règle d'or n°5 : « L'application ne fait **aucune** requête HTTP sortante. »

Mais le lockfile contient `@tauri-apps/plugin-updater@2.10.1` (et `package.json` le déclare). Cet updater, par conception, interroge un endpoint de release en HTTPS pour vérifier et télécharger les mises à jour. Donc soit :

- il est embarqué mais désactivé/non configuré (alors pourquoi le garder, et il faut le dire), soit
- il est actif et la règle 5 est fausse.

Aggravant : ce même argument « no HTTP » sert dans `deny.toml` à justifier l'ignore des advisories rustls-webpki (« l'app ne fait AUCUNE requête HTTP sortante » → chemin TLS de libsql non exercé). Si l'updater fait du HTTPS, l'argument perd sa généralité. Note aussi que iroh QUIC est évidemment du trafic réseau (c'est un app P2P), donc la formulation exacte « aucune requête **HTTP** sortante » visait HTTP au sens strict, mais l'updater casse même cette version stricte.

Action : décider. Soit retirer l'updater et la mise à jour manuelle (cohérent avec « souverain, sans serveur »), soit le garder et reformuler la règle 5 en « l'app n'émet de requête HTTPS sortante que pour la vérification de mise à jour signée, point ». Et alors revoir la justification deny.toml en conséquence.

### 1.3 🔴 Documentation morte après la refonte crypto-only (20/06/2026)

L'historique CLAUDE.md acte la suppression des modules web/social (sites, domaines, recherche, social, forums, modération, marketplace, DAG). Pourtant la doc décrit toujours ces features comme existantes :

- **`DEV_API.md` en entier** documente l'API web supprimée : `POST /api/publish` de sites, `GET /api/search`, `DELETE /api/site`, domaines `.torus`, kinds `forum`/`shop`/`comment`, workflow PageBuilder/VSCode. C'est de la doc qui décrit un sous-système qui n'existe plus. À supprimer (pas à corriger).
- **`SECURITY.md`** : « Forge de transactions / d'actions **sociales** », « chaque tx et **action** est signée ». Les actions sociales n'existent plus.
- **`NOTICE`** et **`package.json`** description : « sovereign peer-to-peer **web** and value network ». Le « web » est mort.
- **CLAUDE.md NET-7** : « DAG sync incrémental — skip WantNodes si heads inchangées ». Le DAG social a été supprimé, donc ce point de durcissement porte sur un sous-système retiré.
- **Le TLD `.torus`** est conservé « pour compatibilité réseau », mais sans domaines il n'y a plus rien à nommer. Vestige à questionner.

Action : purge complète des références web/social/forums/marketplace/DAG-social/`.torus`/PageBuilder de tous les `.md`, du `package.json` et du `NOTICE`. Supprimer `DEV_API.md`. Faire un `grep -ri "site\|forum\|social\|domain\|\.torus\|publish\|pagebuilder"` sur tout le repo et nettoyer.

### 1.4 🟡 Confusion DAG-social supprimé vs DAG-BFT du roadmap

Le README et les whitepapers pointent `docs/DESIGN-CONSENSUS-DAG-BFT.md` comme vision de consensus, alors que la refonte vient de supprimer « DAG » (le DAG du web social). Ce sont deux choses différentes (consensus DAG-BFT vs DAG de contenu social), mais un lecteur va les confondre. Action : préciser explicitement dans la doc que le DAG retiré (web) n'a rien à voir avec le DAG-BFT (consensus futur).

---

## 2. Sécurité 🔴🟠🟡

### 2.1 🟠 Pas de slashing : problème nothing-at-stake

Je ne vois aucune mention de slashing ni de pénalité d'équivocation. En PoS, c'est le mécanisme central : sans coût à signer deux blocs concurrents à la même hauteur, un leader peut équivoquer gratuitement (nothing-at-stake). La résolution de fork déterministe assure que le réseau **converge**, mais elle ne **dissuade** personne de forker, puisqu'il n'y a aucune perte économique. C'est le trou structurel le plus important de ton consensus.

Action : introduire une preuve d'équivocation (deux blocs signés même hauteur, même leader) qui slash le stake du fautif, ou au minimum un mécanisme de réputation négative fort et durable. Documenter la profondeur de confirmation considérée « finale » tant qu'il n'y a pas de finalité BFT.

### 2.2 🟠 Le « VRF » documenté n'est pas un VRF

La formule affichée partout est :

```
seed   = BLAKE3(prev_block_hash || slot)
leader = seed % stake_total_pondéré
```

C'est une fonction **publique et déterministe** : n'importe qui calcule le leader de chaque slot à l'avance. Un vrai VRF (ECVRF par ex.) utilise la **clé secrète** du validateur pour produire une sortie imprévisible mais vérifiable. Deux conséquences :

1. **Leader prévisible** : on connaît le prochain leader, donc on peut le cibler (DoS, tentative d'eclipse ciblée juste avant son slot).
2. **Grindable** : le proposeur du bloc N-1 influence `prev_block_hash` (sélection/ordre des tx, timestamp dans la fenêtre), donc il peut biaiser le seed du slot N en sa faveur. Avec un poids qui inclut la réputation, un mineur pourrait grinder pour rester leader.

L'historique mentionne « aléa d'élection non-grindable (beacon enterré) » et SECURITY.md parle d'« entropie d'epoch passée + accumulateur », mais **rien de ça n'apparaît dans la formule documentée**. Soit la doc sous-décrit l'implémentation réelle (alors corrige la doc), soit l'implémentation est bien `BLAKE3(prev_hash||slot)` (alors c'est grindable). À trancher en code.

Action : (a) si pas déjà fait, passer à un vrai VRF à clé secrète pour l'éligibilité, (b) dériver le seed d'entropie accumulée profonde (sortie VRF de plusieurs blocs en arrière, ou accumulateur XOR), (c) le VDF du roadmap traite le grinding mais pas la prévisibilité : le VRF traite la prévisibilité. Tu as probablement besoin des deux. (d) Arrêter d'appeler « VRF » une fonction publique tant qu'il n'y a pas de composante à clé secrète : c'est trompeur dans un whitepaper.

### 2.3 🟠 Heuristique d'eclipse illusoire

« Warning si plus de 80 % des peers partagent un préfixe pubkey de 8 hex. » Les pubkeys sont **gratuites** à générer. Un attaquant qui veut t'éclipser utilise des clés **diverses**, pas des clés à préfixe colliding. Donc cette heuristique attrape l'éclipse paresseuse (un attaquant qui réutilise un préfixe par flemme) et rien d'autre. Elle donne un faux sentiment de sécurité.

La vraie résistance à l'eclipse repose sur la **diversité réseau** : sélection de peers par diversité d'IP et d'AS, connexions d'ancrage persistantes vers des peers de confiance, buckets type Kademlia/Bitcoin addrman, limite de peers par sous-réseau. Action : ne pas présenter l'heuristique de préfixe comme une protection eclipse ; ajouter de la diversité IP/AS et des ancres.

### 2.4 🟠 Couche gossip sans résistance Sybil

Le stake protège l'élection du leader PoS, mais **pas** la couche gossip : n'importe quelle pubkey peut rejoindre et flooder jusqu'à la limite de débit adaptative. Il n'y a pas de puzzle d'admission. À deux nœuds c'est invisible ; à l'échelle, un attaquant génère des milliers d'identités gossip pour saturer, amplifier ou biaiser la propagation. Le dedup LRU 100K est ta principale défense contre les boucles, et 100K peut être trop petit sous charge (éviction puis re-traitement). Action : prévoir un puzzle d'admission léger (PoW par connexion) ou un scoring de peers strict avant l'ouverture du réseau public, et tester le dimensionnement du LRU.

### 2.5 🟠 Génération du token Dev API à entropie faible

(Pertinent seulement si un serveur HTTP local subsiste après la refonte ; sinon, supprimer avec `DEV_API.md`.)

DEV_API.md §4 : « Le token est généré via BLAKE3 sur des entrées système non prédictibles (timestamp nano + adresse mémoire + thread id). » Ce **ne sont pas** des sources d'aléa cryptographique : le timestamp est devinable à la milliseconde, l'adresse mémoire ne donne que quelques bits via ASLR, le thread id est quasi nul. Pour un bearer token qui autorise à publier et signer avec ta clé, c'est insuffisant : un attaquant qui estime ces entrées peut réduire l'espace de recherche.

Action : générer le token avec un CSPRNG (`getrandom` / `OsRng`, 32 octets), point. BLAKE3 d'un compteur n'ajoute pas d'entropie. Et noter qu'un serveur loopback n'est pas isolé : tout process local et, via DNS rebinding/CSRF, une page web malveillante peuvent l'atteindre. Le token est la seule barrière réelle, donc il doit être fort.

### 2.6 🟡 deny.toml : la DoS hickory n'est probablement pas « local-only »

Les advisories ignorés sont bien réels. Le raisonnement « local-only » tient **pour rustls-webpki via libsql** si libsql ne fait vraiment aucune sortie réseau (à confirmer). Mais `RUSTSEC-2026-0119/0120` (DoS hickory) arrive **via iroh**, et iroh fait de la **découverte DNS** sur le chemin réseau actif. Donc ce chemin est potentiellement exercé, contrairement à ce que « DoS découverte DNS » laisse passer comme si c'était inoffensif. Action : prioriser le bump iroh (tu l'identifies déjà comme le fix), ou désactiver la découverte DNS d'iroh et le documenter, parce que c'est la seule entrée ignorée qui touche un chemin live.

### 2.7 🟡 Points crypto à confirmer en code

- **Argon2id 64 MiB / t=3 / p=4** : acceptable mais bas pour une **clé maître de wallet**. Viser une auto-calibration ciblant un temps d'unlock (par ex. 250 à 500 ms sur la machine cible) plutôt qu'un coût mémoire fixe, et documenter le rationnel. Pour un coffre haute valeur, monter le coût mémoire si l'UX le permet.
- **Séparation de domaine du XOF** pour dériver ML-DSA depuis la graine Ed25519 : vérifier qu'un contexte distinct est utilisé, sinon réutilisation de matière. L'argument « casser les deux schémas » tient pour la **forge** (la sortie XOF est one-way), à formuler clairement : la compromission de la graine fait tomber les deux clés (acceptable, c'est une seule identité).
- **Seconde-préimage Merkle** : confirmer la séparation leaf/node (préfixes de domaine distincts) dans l'arbre BLAKE3 des tx IDs, pour éviter le bug type CVE-2012-2459 (duplication du dernier nœud). Sinon deux ensembles de tx différents peuvent produire la même racine.
- **GCM nonce** : « nonce unique 12 octets par opération ». Pour un coffre (chiffrement à la sauvegarde, faible volume) un nonce aléatoire va bien ; confirmer juste qu'il n'y a pas de réutilisation de nonce entre re-sauvegardes du même coffre.

### 2.8 🟡 Arithmétique et croissance non bornée du ledger

- **Checked arithmetic partout** : en release, Rust **wrap silencieusement** sur overflow `+`/`-`/`*`. Pour un ledger, tout doit être `checked_add`/`checked_sub`/`checked_mul` qui **erre** plutôt que de wrapper ou saturer. La correction AUDIT-TX-3 a réglé un cas, mais la mention « saturé par `balance_of` » m'inquiète : **saturer un solde masque une violation d'invariant** au lieu de la révéler. Préférer l'erreur explicite à la saturation. À confirmer en code que `weight = stake + reputation*10000` et les sommes ne peuvent pas overflow.
- **`seen_tx_hashes` non borné** = fuite mémoire à terme (chaque tx jamais vue y reste). Avec un nonce strictement monotone par compte, tu n'as pas besoin de garder tous les hashes pour toujours : fenêtrer ou élaguer (garder le dedup récent, s'appuyer sur le nonce pour le reste). Le gossip a bien un LRU 100K ; le ledger devrait avoir une stratégie équivalente.

---

## 3. Modèle économique 🟡🔵

### 3.1 🟡 « Shapley » est probablement abusif

Les vraies valeurs de Shapley sont exponentielles en nombre de joueurs (moyenne des contributions marginales sur toutes les coalitions). Ce que tu décris (énergie 30 / travail 30 / validation 25 / uptime 15) ressemble à une **pondération de contribution fixe**, pas à du Shapley. Si c'est bien une pondération, l'appeler « Shapley » ou « Shapley-style » est du vocabulaire marketing qui ne survivra pas à une revue technique. Action : soit prouver que c'est du vrai Shapley (et montrer comment tu le calcules à coût raisonnable), soit renommer en « pondération de contribution » dans la doc. La règle « somme = 1.0 » que testent tes tests est une normalisation de pondération, pas une propriété Shapley.

### 3.2 🟡 « Déflationniste » est conditionnel, pas absolu

Émission continue (asymptote vers le cap, jamais nulle) plus burn 1 % par transfert. Le solde **net** n'est déflationniste que si le volume de burn dépasse l'émission, ce qui dépend de l'usage. À faible volume, l'émission domine et l'offre **croît** vers le cap (inflationniste). Action : préciser « déflationniste **au-delà d'un certain volume de transferts** », sinon c'est faux à bas volume. C'est exactement le genre de nuance que ta posture honnête exige.

### 3.3 🔵 Le calendrier d'émission est multi-siècles, « front-loaded » peut tromper

La décroissance géométrique (chaque minute frappe 1/50 000 000 du **restant**) donne, en approximation continue `M(t) = cap·(1 - e^(-t/k))` avec `k = 50M` minutes :

- demi-cap (50M QUANTA émis) atteint vers **66 ans**,
- 90 % du cap (90M) vers **219 ans**,
- rythme genèse ≈ 120 QUANTA/h ≈ 1,05M QUANTA/an, puis décroît.

Le **rythme** est bien front-loaded (maximal à la genèse, ne fait que baisser), mais les **montants absolus** prennent des siècles à approcher le cap. C'est un choix valable (longue traîne bornée, façon émission perpétuelle Monero mais cappée), mais un lecteur pressé lira « front-loaded » comme « la majorité est émise tôt », ce qui est faux. Action : documenter le timeline explicitement (table année → offre cumulée) pour ne rien survendre. La rareté est réelle, autant la montrer précisément.

### 3.4 🔵 Couplage réputation → poids de consensus

`weight = stake + reputation·10_000`. Avec stake min = 1 QUANTA = 1 000 000 µQTA et une réputation gagnée par le minage (énergie, uptime), `reputation·10000` peut atteindre l'ordre du stake minimum dès que la réputation dépasse ~100. Conséquence : un nœud à forte réputation et stake minimal peut avoir une probabilité de leader disproportionnée, et la réputation s'achète avec de l'**énergie**. Cela transforme partiellement ton PoS en quasi-PoW (la dépense d'électricité achète du poids d'élection) et ouvre une voie Sybil si farmer de la réputation coûte moins cher que staker. Action : clarifier le modèle de sécurité voulu (PoS pur ? hybride PoS/contribution ?) et **borner** l'influence de la réputation dans le poids, sinon l'attaque la moins chère définit ta sécurité.

---

## 4. Build, dépendances, versioning 🟠🟡

### 4.1 🟠 Contradiction Tailwind vs CSS vanilla

Vérifié dans le lockfile : `tailwindcss@4.2.4` et `@tailwindcss/vite@4.2.4` sont **réellement installés**, et `vite.config.js` branche le plugin Tailwind. Or CLAUDE.md (« CSS : Vanilla CSS, tokens ») et README (« Styles : CSS vanilla (tokens) ») disent l'inverse, et les deux maquettes sont en CSS pur avec variables. Donc soit Tailwind est **mort** (dépendance plus plugin de build inutiles, surface et temps de build pour rien), soit la doc ment. Action : décider. Si l'UI réelle est en CSS vanilla, retirer Tailwind et son plugin. Si tu adoptes Tailwind, corriger la doc et migrer les tokens.

### 4.2 🟡 Dépendances probablement mortes après la refonte

`marked@18.0.2` et `dompurify@3.4.1` (plus `@types/dompurify`) servaient au rendu HTML des sites publiés (le moteur web supprimé). En crypto-only, vérifier si elles sont encore utilisées (rendu markdown du whitepaper in-app ?). Si non, les retirer réduit la surface et la chaîne d'approvisionnement. `dompurify` notamment n'a de sens que si tu rends du HTML non fiable, ce qui ne devrait plus arriver sans le web social.

### 4.3 🟡 Supply-chain lucide à vérifier

`lucide-svelte@1.0.1` est résolu depuis npm avec un hash d'intégrité valide, donc c'est un paquet publié réel. Mais le paquet **canonique** de lucide pour Svelte est désormais `@lucide/svelte` (scopé) ; l'ancien nom non-scopé plafonnait vers 0.4xx avant la migration. Un « 1.0.1 » sur le nom **non-scopé déprécié** mérite une vérification de provenance (risque qu'un tiers ait repris le nom abandonné). Action : vérifier qui maintient `lucide-svelte@1.0.1`, et migrer vers `@lucide/svelte` qui est la source officielle aujourd'hui.

### 4.4 🟡 Version applicative incohérente (3.3.0 vs 0.1.0)

Tout dit 3.3.0 : `package.json`, badges README, en-tête CLAUDE.md. Mais CLAUDE.md référence `Quanta_0.1.0_aarch64.dmg`, ce qui veut dire que `tauri.conf.json` (non fourni) est probablement resté à **0.1.0**. C'est la version que voient l'utilisateur final **et l'updater** pour décider d'une mise à jour. Un updater qui compare 0.1.0 alors que le monde dit 3.3.0, c'est un bug d'update en puissance. Action : aligner `tauri.conf.json` sur la version réelle et bumper.

### 4.5 🟡 Branding Torus vs Quanta à finir

Choix assumé et correct de garder les identifiants **wire** internes (`.torus`, `TORUS_PROTOCOL_VERSION`, events `torus://`) pour la compatibilité réseau. Mais la couche **utilisateur** mélange les deux marques : `package.json` `repository`/`homepage` pointent le repo `Torus`, `DEV_API.md` s'intitule « Torus Dev API », le README pointe `audit/Torus-Audit-360.html`, le protocole est nommé « Protocole Torus » partout dans CLAUDE.md. Action : renommer le repo GitHub en `Quanta` (GitHub redirige les anciens liens), aligner la doc user sur « Quanta », et ne garder « Torus » que comme nom d'identifiant technique interne explicitement étiqueté comme tel.

### 4.6 🟡 NOTICE à corriger (pas seulement « web »)

Apache-2.0 §4(d) impose la redistribution du NOTICE, donc il doit être propre. Au-delà du « web and value network » à corriger (cf. 1.3), vérifier que la liste des dépendances reflète le code post-refonte : si des crates liées au web/social sont parties, leurs attributions n'ont plus à y figurer ; et s'il manque `fips204`, `iroh-blobs` ou d'autres composants notables actifs, les ajouter.

---

## 5. Tests, qualité, vérification 🔵

174 tests, c'est bien, mais le **nombre** compte moins que la **couverture des chemins critiques**. Les marches suivantes naturelles, par ordre de valeur :

- **Fuzzing du parseur d'enveloppes** (déjà au roadmap, à faire) : `cargo-fuzz` sur tout le chemin `raw bytes → désérialisation GossipEnvelope`, c'est ta surface d'attaque réseau n°1.
- **Testnet multi-nœuds en chaos** : passer de 2 machines à 5 et plus, avec partitions réseau, latence injectée, et au moins un nœud byzantin (équivoque, rejoue, refuse de sceller). C'est là que se révèlent les bugs de convergence et de fork.
- **Property-tests de conservation** : tu en as déjà (Σ soldes + brûlé == miné). Excellent, à étendre aux séquences de reorg profondes et aux arrivées hors-ordre.
- **Vérification formelle légère du state machine de consensus** : modéliser en TLA+ ou avec Stateright (Rust) les invariants « pas de double-application », « conservation », « convergence sous reorg ». Tu as déjà proptest ; le model checking exhaustif sur petits états est la couche d'assurance au-dessus.

---

## 6. Pour rendre Quanta exceptionnel (vision) 🔵

Au-delà des corrections, ce qui sépare « projet alpha solide » d'« exceptionnel » :

1. **Audit tiers** (déjà reconnu). Indispensable avant toute valeur réelle. Cible la crypto (l'hybride PQ, la dérivation de graine) et le consensus.
2. **Slashing + vrai VRF + résistance au grinding** (cf. 2.1, 2.2). Le triptyque qui rend le PoS défendable. Sans ça, le consensus est « plausible » mais pas « sûr ».
3. **Builds reproductibles + release signée + notarisation macOS** (déjà au roadmap). Une release souveraine doit être vérifiable bit à bit par l'utilisateur.
4. **Récupération de compte standardisée** : tu utilises une « clé de récupération » affichée une fois. Une récupération **custom** est risquée (formats, compatibilité, erreurs de saisie). Envisager un schéma standard (type BIP39 mnémonique) pour l'interopérabilité et la robustesse de l'UX de sauvegarde, ou documenter précisément pourquoi le format custom est sûr.
5. **Light client / vérification SPV** : permettre de vérifier des soldes et des preuves d'inclusion sans la chaîne complète. Important pour l'adoption hors machines toujours-en-ligne.
6. **Marché de frais / anti-spam transactionnel** : aujourd'hui le seul coût d'un transfert est le burn 1 %. À l'échelle, sans frais ni PoW par transaction, la mempool est spammable. Le cap mempool (1000 tx, TTL 10 min) limite mais ne tarife pas le spam. Penser un mécanisme de priorité.
7. **Confidentialité (roadmap long)** : soldes et tx publics aujourd'hui (reconnu). Une couche d'engagement/ZK est un différenciateur fort si la souveraineté inclut la vie privée.

---

## 7. Tableau de priorisation (impact × effort)

| # | Sujet | Sévérité | Effort | Impact |
|---|-------|----------|--------|--------|
| 1.1 | Retirer le prix EUR inventé de la maquette identité | 🔴 | Faible | Élevé (crédibilité) |
| 1.3 | Supprimer DEV_API.md, purger la doc web/social morte | 🔴 | Faible | Élevé (crédibilité) |
| 1.2 | Trancher updater vs règle « no HTTP » et aligner deny.toml | 🔴 | Faible | Élevé (cohérence) |
| 4.1 | Trancher Tailwind vs CSS vanilla, retirer le mort | 🟠 | Faible | Moyen |
| 4.4 | Aligner la version tauri.conf.json (0.1.0 → 3.3.0) | 🟡 | Faible | Moyen (updater) |
| 2.5 | Token Dev API via CSPRNG (si l'API subsiste) | 🟠 | Faible | Élevé (si présent) |
| 2.6 | Prioriser bump iroh (DoS hickory sur chemin live) | 🟡 | Moyen | Moyen |
| 4.2 | Retirer marked/dompurify si inutilisés | 🟡 | Faible | Faible (surface) |
| 4.3 | Vérifier provenance lucide, migrer vers @lucide/svelte | 🟡 | Faible | Moyen (supply-chain) |
| 2.8 | Checked arithmetic + élaguer seen_tx_hashes | 🟡 | Moyen | Élevé (intégrité) |
| 2.2 | Vrai VRF + entropie accumulée (ou corriger la doc) | 🟠 | Élevé | Élevé (sécurité) |
| 2.1 | Slashing / pénalité d'équivocation | 🟠 | Élevé | Élevé (sécurité) |
| 2.3 | Diversité IP/AS + ancres (vraie résistance eclipse) | 🟠 | Élevé | Élevé (réseau) |
| 3.x | Corriger « Shapley », « déflationniste », timeline d'émission | 🟡 | Faible | Moyen (honnêteté) |
| 5/6 | Fuzzing, testnet chaos, audit tiers, builds repro | 🔵 | Élevé | Exceptionnel |

---

## 8. Limites de cet audit

Sans le code Rust, je n'ai pas pu vérifier : la sécurité réelle de l'élection de leader (le « beacon enterré » est-il vraiment implémenté ?), la présence effective de checked arithmetic, la séparation de domaine des dérivations crypto et de l'arbre Merkle, l'existence ou non d'un serveur HTTP local après la refonte, et le comportement réel sous partition réseau. Les points 2.1 à 2.8 et la section 3.4 demandent les sources `src-tauri/src/` pour être tranchés définitivement. Si tu me donnes le code (au minimum `pos_consensus.rs`, `ledger.rs`, `dispatcher.rs`, `hybrid_crypto.rs`), je fais la revue ligne par ligne de la couche valeur et consensus.
