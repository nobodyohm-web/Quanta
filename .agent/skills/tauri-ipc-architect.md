# Tauri IPC Architect Skill

Le pont (IPC - Inter-Process Communication) entre Svelte 5 et Rust est critique pour les performances du moteur local Torus.

## Règles de Conception IPC

1. **Fonctions Asynchrones par Défaut** :
   Toutes les `#[tauri::command]` qui font des I/O ou appellent la P2P DHT doivent être `async`. Cela empêche le thread principal (UI) de bloquer ou freezer.
   ```rust
   // OUI
   #[tauri::command]
   pub async fn query_ledger(query: String) -> Result<Vec<ResultItem>, String> { ... }
   ```

2. **Typage Strict (Rust <-> TypeScript)** :
   - Assure-toi que les structures de retour Rust dérivent bien `serde::Serialize` (et `serde::Deserialize` pour les paramètres JSON d'entrée).
   - Les types TypeScript (`src/lib/types/` ou `src/lib/templates.ts`) doivent correspondre parfaitement aux Structs Rust. 

3. **Gestion des Erreurs Tauri** :
   - Les commandes doivent TOUJOURS retourner `Result<T, String>` (ou une enum d'erreur implémentant Serialize) plutôt que de paniquer.
   - Côté Svelte, l'appel de `invoke()` doit toujours être dans un bloc `try/catch`.

4. **Événements (Event Emitters)** :
   - Pour les données en temps réel (ex: mise à jour du P2P Ledger, Attention feed), n'utilise pas le "polling" classique.
   - Utilise l'API d'émission d'événements de Tauri (`app_handle.emit_all("event-name", payload)`).
