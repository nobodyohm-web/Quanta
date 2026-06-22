# Roadmap — « Crypto + Google Web3 »

> Vision (2026-06-10) : faire de Quanta **la crypto de l'avenir** — un web3 souverain,
> sans hébergement, où chacun peut **créer, vendre, acheter et partager** (sites,
> boutiques, blogs), avec QUANTA comme monnaie native et une recherche réseau
> de classe « Google web3 ».
>
> Ce fichier est l'état de la boucle d'évolution autonome. Chaque itération coche
> ce qui est fait et choisit l'incrément suivant.

## État des lieux (déjà livré avant la boucle)

- ✅ Créer : Site Engine v3.3 (PageBuilder no-code 20 blocs, multi-pages, domaines `.torus`)
- ✅ Chercher : index BM25 + QuantaRank + tags (recherche P2P)
- ✅ Social : likes quadratiques, follows 3 tiers, tips, boosts, forums, modération jury
- ✅ Monnaie : ledger PoS + VRF, signatures hybrides Ed25519 + ML-DSA-65, burn 1 %
- ❌ **Vendre / acheter entre utilisateurs** : `marketplace.rs` ne couvre que les tâches de calcul

## Phases

### Phase A — Commerce P2P backend (v3.4) — EN COURS (itération 1)
- [ ] Module `p2p/commerce.rs` : annonces signées Ed25519 (Publish/Delist),
      achats (Purchase adossé à un transfert ledger), avis (Review 1-5, réservé aux acheteurs)
- [ ] Gossip `BroadcastCommerceAction` (lane Medium) + handler dispatcher
- [ ] Store `commerce` dans WillowNode + persistence snapshot 30 s
- [ ] Commandes Tauri : `market_publish_listing`, `market_delist_listing`, `market_buy`,
      `market_listings`, `market_my_orders`, `market_my_sales`, `market_review`,
      `market_reviews`, `market_seller_stats`
- [ ] Tests unitaires (signature, ownership, achat, avis, snapshot) — `cargo test` vert

### Phase B — Boutique UI (Market.svelte)
- [ ] Onglet « Marché » dans la sidebar : grille d'annonces, recherche/filtres (tag, type, vendeur)
- [ ] Fiche annonce : prix QUANTA, note vendeur, bouton Acheter (confirmation), avis
- [ ] « Mes ventes / mes achats » + publication d'annonce (formulaire)
- [ ] i18n FR/EN comme le reste de l'app

### Phase C — Livraison numérique + escrow
- [ ] Contenu numérique chiffré (AES-256-GCM), clé remise à l'achat
- [ ] Escrow ledger (déjà des primitives `build_escrow_lock_tx`/`escrow_release_to`) + litiges via jury de modération

### Phase D — Blogs natifs & flux
- [ ] Type de publication « blog » de première classe (billets datés, flux des abonnements)
- [ ] Bloc PageBuilder « Produit » → vendre depuis son site no-code

### Phase E — Recherche « Google web3 »
- [ ] Page résultats riche (snippets, facettes type contenu : site / blog / produit / forum)
- [ ] Autocomplete + suggestions, indexation des annonces du marché

### Phase F — Robustesse & croissance
- [ ] Tests réseau multi-nœuds du commerce (convergence des annonces/achats)
- [ ] Documentation whitepaper : section économie du marché

## Journal des itérations

| Date | Itération | Travail effectué |
|------|-----------|------------------|
| 2026-06-10 | 1 | Démarrage Phase A : module commerce backend |
