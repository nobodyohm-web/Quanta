# Phase 4.1 — Notifications Temps Réel via Tauri Events

## Objectif
Remplacer le polling manuel (les `refresh()` déclenchés par l'utilisateur) par des événements push natifs du backend Rust vers le frontend Svelte. L'UI doit réagir instantanément à chaque changement d'état.

## Événements à Implémenter

| Event Name | Déclencheur Rust | Payload | Composant Svelte |
|------------|-----------------|---------|------------------|
| `atn-mined` | `uptime_tick()` dans la boucle 60s | `{ pk, amount, total_balance }` | `Dashboard.svelte`, `Wallet.svelte` |
| `tx-received` | `ledger_transfer()`, `reward_tx()` | `{ tx_id, from, to, amount, tx_type }` | `Wallet.svelte` |
| `peer-connected` | Handler d'acceptation Iroh | `{ node_id, peer_count }` | `Dashboard.svelte` |
| `site-published` | `start_sync()` | `{ site_id, title }` | `Feed.svelte` |
| `chain-sealed` | `seal_block()` | `{ block_index, tx_count }` | `Wallet.svelte` |
| `identity-created` | `create_identity()` | `{ display_name }` | Router principal |

## Implémentation Rust

### Étape 1 : Stocker l'AppHandle dans AppState
```rust
pub struct AppState {
    pub crypto: Mutex<CryptoEngine>,
    pub db: Mutex<Option<Database>>,
    pub node: WillowNode,
    pub app_handle: Mutex<Option<tauri::AppHandle>>,  // NOUVEAU
}
```

### Étape 2 : Initialiser l'AppHandle dans setup()
```rust
.setup(move |app| {
    let handle = app.handle().clone();
    // ... existing code ...
    *state.app_handle.lock().await = Some(handle);
})
```

### Étape 3 : Émettre des événements
```rust
// Dans la boucle uptime_tick
if let Some(handle) = ms.app_handle.lock().await.as_ref() {
    let _ = handle.emit("atn-mined", serde_json::json!({
        "amount": atn, "pk": pk
    }));
}
```

## Implémentation Svelte

### Étape 1 : Import de l'API d'écoute
```typescript
import { listen } from "@tauri-apps/api/event";
```

### Étape 2 : Écouter dans les composants avec $effect
```svelte
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  let balance = $state(0);

  $effect(() => {
    const unlisten = listen("atn-mined", (event: any) => {
      balance = event.payload.total_balance;
    });
    return () => { unlisten.then(fn => fn()); };
  });
</script>
```

## Fichiers à Modifier
- `src-tauri/src/lib.rs` — AppHandle dans AppState + émissions d'événements
- `src/lib/Dashboard.svelte` — Écoute `atn-mined`, `peer-connected`
- `src/lib/Wallet.svelte` — Écoute `tx-received`, `chain-sealed`
- `src/lib/Feed.svelte` — Écoute `site-published`

## Contraintes
- L'AppHandle doit être cloné (pas déplacé) dans les tasks Tokio
- Les événements ne doivent jamais bloquer le thread Rust (fire-and-forget)
- Utiliser `$effect` avec cleanup pour éviter les fuites de listeners
