Tu es en mode DEEP ENGINEERING — SCOPE COMPLET. Ton objectif : rendre PARFAIT tout ce qui concerne les échanges entre ordinateurs dans Torus.

Lis CLAUDE.md, les skills .claude/skills/*, et les rules .claude/rules/* AVANT de commencer.

## Contexte : état actuel

- 219 tests passent, clippy clean
- Réseau NET-1 → NET-16 déjà implémenté (priority queue, reconnect, sync parallèle, metrics, mempool, nicknames)
- 30 commandes Tauri exposées au frontend
- 13,747 lignes de code P2P réparties sur ~20 modules

## Mission

Tu vas auditer CHAQUE sous-système d'échange entre machines, identifier les bugs/faiblesses/edge cases, et corriger tout. Pour chaque sous-système :
1. Lis le module en entier
2. Lis le handler dans dispatcher.rs
3. Lis la commande Tauri correspondante dans lib.rs
4. Identifie : bugs, race conditions, cas non gérés, sérialisation manquante, signatures non vérifiées
5. Corrige et ajoute des tests
6. `cargo check` + `cargo test` + `cargo clippy -- -D warnings`
7. NE T'ARRÊTE PAS

## Sous-systèmes à auditer (dans l'ordre)

### 1. Transactions & Transferts (CRITIQUE)
**Fichiers** : `ledger.rs`, `ledger_types.rs`, `dispatcher.rs::handle_broadcast_tx`
**Audit** :
- [ ] Le cycle complet fonctionne : créer tx localement → signer (hybrid Ed25519+PQ) → broadcast via gossip → recevoir sur l'autre machine → verify_tx → replay_remote_tx → balance mise à jour
- [ ] Les nonces sont correctement incrémentés des deux côtés
- [ ] Le burn rate 1% est appliqué correctement sur les remote txs
- [ ] Les txs en double sont rejetées (seen_tx_hashes)
- [ ] Les txs avec montant > balance sont rejetées AVANT broadcast
- [ ] Le CRDT et le ledger linéaire restent synchronisés
- [ ] Test : simuler un transfert A→B entre deux ledgers et vérifier que les balances convergent

### 2. Blocs & Mining (CRITIQUE)
**Fichiers** : `ledger.rs::seal_block`, `dispatcher.rs::handle_new_block`, `mining_loop.rs`
**Audit** :
- [ ] Le bloc sealé est correctement broadcasté via NewBlock
- [ ] Le receveur valide le hash (BLAKE3), le prev_hash, l'index, et TOUTES les tx signatures
- [ ] Fork resolution fonctionne : si même height, le hash le plus grand gagne
- [ ] Après intégration d'un remote block, les txs du bloc sont retirées du pending local
- [ ] Le mining loop respecte le puzzle difficulty (PoC hash prefix)
- [ ] Test : simuler un fork avec 2 blocs à la même height

### 3. Chain Sync (CRITIQUE)
**Fichiers** : `dispatcher.rs::handle_request_chain`, `handle_chain_segment`
**Audit** :
- [ ] RequestChain → ChainSegment → integrate fonctionne pour N blocs
- [ ] La pagination fonctionne (si le peer a 200 blocs, on les demande en segments de 50)
- [ ] Les blocs reçus sont validés dans l'ORDRE (index croissant)
- [ ] Si un bloc intermédiaire est invalide, on s'arrête (pas de gap)
- [ ] La compression gzip fonctionne et le fallback JSON aussi
- [ ] Test : reconstruire une chaîne de 100 blocs via sync paginé

### 4. Pages Web P2P
**Fichiers** : `page_store.rs`, `dispatcher.rs::PublishPage/RequestPage/PublishSiteManifest`
**Audit** :
- [ ] Publier une page → signer → broadcast → recevoir → vérifier signature → stocker
- [ ] Les sites multi-pages (SiteManifest) se synchronisent correctement
- [ ] La version des pages est respectée (un peer ne peut pas downgrade une page)
- [ ] Le contenu HTML est sanitisé (pas de script injection via gossip)
- [ ] RequestPage retourne bien la bonne page au bon auteur
- [ ] Test : publier une page, la recevoir sur un second store, vérifier le contenu

### 5. Domaines .torus
**Fichiers** : `domains.rs`, `dispatcher.rs::handle_publish_domain/handle_publish_subdomain`
**Audit** :
- [ ] Claim → signer → broadcast → recevoir → vérifier signature → insérer dans le registre
- [ ] L'overbid fonctionne : un nouveau claimant avec plus d'argent peut prendre le domaine
- [ ] Le Harberger tax est calculé correctement
- [ ] Les subdomains sont validés (le parent owner doit signer la délégation)
- [ ] validate_name() rejette les noms invalides (caractères spéciaux, trop long, etc.)
- [ ] La résolution name→pk fonctionne après sync
- [ ] Test : claim + overbid + subdomain grant round-trip

### 6. Social (Likes, Follows, Tips, Boost)
**Fichiers** : `social.rs`, `dispatcher.rs::handle_broadcast_social_action`
**Audit** :
- [ ] Chaque action est signée par l'auteur (Ed25519)
- [ ] verify() est appelé AVANT apply()
- [ ] Les likes sont quadratiques (coût croissant pour le même content)
- [ ] Les follows sont réversibles (unfollow remet le compteur à 0)
- [ ] Les tips transfèrent réellement des QUANTA (pas juste un compteur)
- [ ] Le boost factor calcule correctement l'influence sur le QuantaRank
- [ ] Les doublons sont gérés (reliker le même contenu = erreur propre)
- [ ] Test : like + unlike + follow + tip round-trip avec vérification des balances

### 7. Forums
**Fichiers** : `forums.rs`, `dispatcher.rs::handle_publish_forum_node`
**Audit** :
- [ ] Forum → Thread → Comment hiérarchie fonctionne via gossip
- [ ] Les signatures sont vérifiées pour chaque noeud (build_forum, build_thread, build_comment)
- [ ] Le handler dispatch correctement selon le `kind` ("forum"|"thread"|"comment")
- [ ] Les doublons sont rejetés (même ID = skip)
- [ ] Les orphelins sont gérés (comment sans thread parent = queue ou reject gracieux)
- [ ] Le snapshot/restore préserve tous les forums, threads, et comments
- [ ] Test : créer un forum + thread + comment, snapshot, restore, vérifier l'intégrité

### 8. Modération (VRF Jury)
**Fichiers** : `moderation.rs`, `dispatcher.rs::handle_broadcast_report/juror_commit/juror_reveal`
**Audit** :
- [ ] Le cycle complet : report → accumulation → jury VRF → commit → reveal → verdict
- [ ] Les votes scellés ne sont pas falsifiables (commit = hash du vote)
- [ ] La révélation vérifie que le hash correspond au commit
- [ ] Le seuil de reports est respecté avant de déclencher un jury
- [ ] Les jurors sont sélectionnés via VRF (BLAKE3) — pas manipulable
- [ ] Le verdict majority-wins est correctement appliqué
- [ ] Test : simuler un cycle complet de modération

### 9. Recherche (BM25 + QuantaRank)
**Fichiers** : `search.rs`, `dispatcher.rs::handle_publish_site`
**Audit** :
- [ ] Les documents indexés arrivent via gossip et sont correctement tokenisés
- [ ] Le BM25 ranking fonctionne (k1=1.2, b=0.75)
- [ ] Le QuantaRank intègre les signaux sociaux (likes, follows, tips)
- [ ] La recherche retourne des résultats pertinents et ordonnés
- [ ] Les documents mis à jour remplacent les anciens (pas de doublons)
- [ ] Test : indexer 3 documents, chercher un mot, vérifier l'ordre BM25

### 10. Trust Graph (Web of Trust)
**Fichiers** : `trust_graph.rs`
**Audit** :
- [ ] Le graphe de confiance est correctement construit à partir des follows
- [ ] Le PageRank personnalisé converge
- [ ] Les cycles sont gérés (A follow B follow C follow A)
- [ ] Les nœuds isolés ont un score 0
- [ ] Le score est recalculé quand le graphe change
- [ ] Test : vérifier que le trust score d'un nœud populaire > nœud isolé

## Règles strictes

- **NE T'ARRÊTE PAS** entre les sous-systèmes. Enchaîne les 10.
- Pour chaque bug trouvé : corrige + ajoute un test de régression
- Backward compat OBLIGATOIRE : `#[serde(default)]` sur tous les nouveaux champs
- Respecte les patterns existants : `Arc<RwLock<T>>`, `CancellationToken`, `signable_envelope_bytes()`
- Si un handler gossip ne vérifie pas la signature → c'est un bug CRITIQUE → corrige immédiatement
- Le résultat final doit être : 2 machines connectées échangent PARFAITEMENT tout type de donnée
- Commit après chaque sous-système majeur (grouper 2-3 si les changements sont petits)
- À la fin, mets à jour CLAUDE.md et crée un résumé des corrections
- Cible : >250 tests passants
