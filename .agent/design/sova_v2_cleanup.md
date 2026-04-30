# SOVA V2 — Plan de Nettoyage Frontend + Backend

> Ce fichier est la spec de nettoyage pour Claude Code.
> Objectif : retirer tout le réseau social, garder la pure crypto.

---

## Fichiers Svelte à SUPPRIMER

```
src/lib/Feed.svelte              # Fil d'actualité social
src/lib/Editor.svelte            # Éditeur de contenu
src/lib/Browser.svelte           # Navigateur de contenu
src/lib/PostCard.svelte          # Carte de publication
src/lib/TemplatePicker.svelte    # Sélecteur de templates
src/lib/templates.ts             # Données des templates
src/lib/BadgeForge.svelte        # Badges de gamification
src/lib/ConstellationGraph.svelte # Graphe de connexions
src/lib/ActivityHeatmap.svelte   # Heatmap d'activité sociale
src/lib/OrbitalAvatar.svelte     # Avatar orbital
src/lib/NotificationBell.svelte  # Cloche de notifications
src/lib/UserProfile.svelte       # Profil utilisateur social
```

## Fichiers Svelte à GARDER

```
src/lib/Wallet.svelte            # ✅ Wallet (solde, énergie, valeur)
src/lib/Dashboard.svelte         # ✅ Dashboard (stats réseau, mining)
src/lib/Settings.svelte          # ✅ Settings (thème, préférences)
src/lib/Welcome.svelte           # ✅ Welcome screen
src/lib/BootSequence.svelte      # ✅ Séquence de démarrage
src/lib/NavBar.svelte            # ✅ Navigation (à simplifier)
src/lib/TopBar.svelte            # ✅ Barre supérieure
src/lib/CommandPalette.svelte    # ✅ Palette de commandes
src/lib/HelpModal.svelte         # ✅ Aide
src/lib/Identicon.svelte         # ✅ Identicon crypto
src/lib/LiveCounter.svelte       # ✅ Compteur en temps réel
src/lib/Sparkline.svelte         # ✅ Mini graphe
src/lib/StrengthMeter.svelte     # ✅ Force du mot de passe
src/lib/Sidebar.svelte           # ✅ Sidebar (à simplifier)
src/lib/prefs.ts                 # ✅ Préférences
src/routes/+layout.ts            # ✅ Layout
src/routes/+page.svelte          # ✅ Page principale (à modifier)
```

## Modifications de +page.svelte

### Imports — Supprimer
```diff
-  import Feed from "$lib/Feed.svelte";
-  import Editor from "$lib/Editor.svelte";
-  import Browser from "$lib/Browser.svelte";
```

### Vue par défaut — Changer
```diff
-  let view = $state("feed");
+  let view = $state("wallet");
```

### Navigation clavier — Simplifier
```diff
-  const map: Record<string, string> = {
-    "1": "feed", "2": "discover", "3": "editor",
-    "4": "wallet", "5": "profile", ",": "settings",
-  };
+  const map: Record<string, string> = {
+    "1": "wallet", "2": "profile", "3": "settings",
+  };
```

### Vue principale — Simplifier
```diff
-  {#if view === "feed"}<Feed />
-  {:else if view === "discover"}<Browser />
-  {:else if view === "editor"}<Editor />
-  {:else if view === "wallet"}<Wallet />
+  {#if view === "wallet"}<Wallet />
   {:else if view === "profile"}<Dashboard />
   {:else if view === "settings"}<Settings />
-  {:else}<Feed />{/if}
+  {:else}<Wallet />{/if}
```

## Modifications de NavBar.svelte

Réduire les onglets de 5 à 3 :
- ✅ Wallet (icône portefeuille)
- ✅ Network (icône graphe — renommage de "profile" vers le dashboard réseau)
- ✅ Settings (icône engrenage)

Supprimer les onglets :
- ❌ Feed
- ❌ Discover
- ❌ Editor

## Fichiers Rust backend — À GARDER intégralement

```
src/p2p/reputation.rs    # ✅ Mining, énergie (sera modifié pour V2)
src/p2p/energy.rs         # ✅ Oracle prix énergie
src/p2p/consensus.rs      # ✅ CRDTs
src/p2p/gossip.rs         # ✅ Gossip router
src/p2p/dispatcher.rs     # ✅ Dispatcher P2P
src/p2p/merkle_dag.rs     # ✅ DAG
src/p2p/willow_node.rs    # ✅ Nœud Iroh
src/p2p/sybil.rs          # ✅ Anti-Sybil (sera modifié pour Shapley)
src/p2p/ledger.rs         # ✅ Ledger de transactions
src/p2p/simulation.rs     # ✅ Tests de simulation
src/p2p/mod.rs            # ✅ Module root
src/security/*            # ✅ Tout garder
src/storage/*             # ✅ Tout garder
src/search/*              # ✅ Garder pour l'instant
src/lib.rs                # ✅ Boucle principale (sera modifiée pour V2)
src/main.rs               # ✅ Point d'entrée
```

## Fichiers Rust backend — À NETTOYER

### p2p/attention.rs
Vérifier si c'est lié au réseau social (likes, vues). Si oui → supprimer.
Si c'est lié au mining/énergie → garder.

### p2p/notifications.rs
Si c'est lié aux notifications sociales → supprimer.
Si c'est lié aux événements réseau/mining → garder.

## Commandes Tauri à GARDER

```rust
check_identity, create_identity, unlock_identity, get_recovery_key  // Auth
get_balance, get_energy_stats, transfer_atn                          // Wallet
get_node_ticket, connect_peer                                        // P2P
mine_tick                                                            // Mining
```

## Commandes Tauri à SUPPRIMER (si elles existent)

```rust
create_content, get_feed, like_post, view_post    // Social
get_templates, publish_article                      // Contenu
search_content                                      // Recherche sociale
```

## Critères de validation

- [ ] `cargo test` vert
- [ ] `npm run build` sans erreur
- [ ] Plus aucune référence à Feed/Editor/Browser/Template dans le code
- [ ] L'app démarre sur le Wallet par défaut
- [ ] Navigation : Wallet → Network → Settings uniquement
- [ ] Les fonctions crypto (mining, P2P, énergie) marchent toujours
