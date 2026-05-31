Effectue un checkpoint complet du projet Torus :

1. **État du code** :
   ```bash
   cargo check --manifest-path src-tauri/Cargo.toml
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
   npm run build
   ```

2. **Métriques** :
   ```bash
   wc -l src-tauri/src/**/*.rs | tail -1
   wc -l src/lib/*.svelte | tail -1
   grep -c "pub fn\|pub async fn" src-tauri/src/p2p/*.rs
   grep -c "#\[test\]" src-tauri/src/p2p/*.rs
   ```

3. **Git status** :
   ```bash
   git status --short
   git log --oneline -5
   ```

4. **Génère un rapport** :
   ```
   ## Torus Checkpoint — [date]
   
   ### Code Health
   - Compilation: ✅/❌
   - Tests: N passed, N failed
   - Clippy: clean/N warnings
   - Frontend build: ✅/❌
   
   ### Metrics
   - Rust LoC: N
   - Svelte LoC: N
   - Public APIs: N
   - Tests: N
   
   ### Recent Commits
   - [hash] message
   
   ### Next Steps
   - [ ] ...
   ```

5. **Met à jour CLAUDE.md** si des changements majeurs ont été faits
