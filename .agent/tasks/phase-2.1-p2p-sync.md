# Phase 2.1 — P2P Réel : Iroh Gossip + Sync Protocol

## Objectif
Transformer le endpoint Iroh QUIC (actuellement actif mais passif) en un vrai système de synchronisation P2P bidirectionnel. Deux nœuds Torus doivent pouvoir :
1. Se découvrir via gossip
2. Demander et recevoir du contenu (subspaces)
3. Synchroniser les entries Willow

## État Actuel (ce qui existe déjà)

### `p2p/willow_node.rs`
- `init_endpoint()` crée un vrai endpoint `iroh::Endpoint::builder().bind()` — ✅ fonctionne
- `accept()` loop en background qui accepte les connexions entrantes — ✅ fonctionne
- **PROBLÈME** : `accept_uni()` lit les données mais les jette (ligne ~74)
- **PROBLÈME** : Aucune logique pour ENVOYER du contenu à un pair
- `get_ticket()` retourne le NodeId — ✅ fonctionne
- `start_sync()` crée des entries Willow locales signées — ✅ fonctionne, mais ne les envoie jamais

### Cargo.toml
```toml
iroh = "0.98"
iroh-blobs = "0.100"  # Disponible mais inutilisé
iroh-gossip = "0.98"  # Disponible mais inutilisé
```

## Plan d'Implémentation

### Étape 1 : Protocole ALPN custom
Définir un protocole applicatif sur la couche QUIC :
```rust
const TORUS_ALPN: &[u8] = b"torus/sync/1";
```

### Étape 2 : Messages du protocole
```rust
#[derive(Serialize, Deserialize)]
enum SyncMessage {
    /// "Quels subspaces as-tu ?"
    ListSubspaces,
    /// "Donne-moi le contenu de ce subspace"
    RequestSubspace { site_id: String },
    /// Réponse avec les entries + content blocks
    SubspaceData { site_id: String, entries: Vec<WillowEntry>, content: Vec<u8> },
    /// Liste des subspaces disponibles
    SubspaceList { site_ids: Vec<String> },
}
```

### Étape 3 : Handler de connexion entrante
Remplacer le `accept_uni()` passif par un vrai handler :
1. Lire le message entrant (désérialiser)
2. Router vers le bon handler (`ListSubspaces`, `RequestSubspace`, etc.)
3. Répondre avec les données locales

### Étape 4 : Méthode pour se connecter à un pair
```rust
pub async fn connect_to_peer(&self, node_id: &str) -> Result<Vec<String>, String>
```
1. Résoudre le NodeId en `NodeAddr`
2. Ouvrir une connexion QUIC
3. Envoyer `ListSubspaces`
4. Recevoir la liste → retourner au frontend

### Étape 5 : Iroh-Gossip pour la découverte
Créer un topic gossip `torus/discovery` :
1. Au `init_endpoint()`, rejoindre le topic
2. Broadcast son NodeId + liste de subspaces toutes les 60s
3. Stocker les pairs découverts dans une liste

### Étape 6 : Commande Tauri
```rust
#[tauri::command]
async fn connect_peer(state: ..., node_id: String) -> Result<Vec<String>, String>

#[tauri::command]
async fn fetch_remote_site(state: ..., node_id: String, site_id: String) -> Result<Site, String>
```

### Étape 7 : UI Svelte
- Ajouter un bouton "Connect Peer" dans le Dashboard ou la Sidebar
- Input pour coller un NodeId
- Afficher les peers connectés et leurs subspaces

## Fichiers à Modifier
- `src-tauri/src/p2p/willow_node.rs` — Le gros du travail
- `src-tauri/src/p2p/mod.rs` — Nouveaux types `SyncMessage`
- `src-tauri/src/lib.rs` — Nouvelles commandes Tauri
- `src/lib/Dashboard.svelte` ou nouveau `Peers.svelte`

## Contraintes
- Toujours async (`tokio::sync::RwLock`, jamais `std::sync::Mutex` à travers un `.await`)
- Sérialiser les messages en bincode ou JSON (bincode plus rapide pour le réseau)
- Timeout sur les connexions (5s) pour ne pas bloquer
- Gérer le cas "peer offline" proprement
