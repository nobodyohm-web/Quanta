Effectue une revue de code complète en utilisant le pattern de délégation :

1. **Explore** : Délègue à `@explorer` pour analyser les fichiers modifiés
   ```bash
   git diff --stat HEAD~1
   git diff HEAD~1 -- src-tauri/src/
   ```

2. **Review** : Délègue à `@code-reviewer` avec les diffs
   - Applique la checklist complète (Rust + Svelte)
   - Vérifie chaque point de sécurité
   - Identifie les régressions potentielles

3. **Test** : Délègue à `@test-runner`
   ```bash
   cargo check --manifest-path src-tauri/Cargo.toml
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
   ```

4. **Rapport consolidé** :
   - Combine les résultats des 3 subagents
   - Verdict final : ✅ APPROVE / ⚠️ CHANGES NEEDED / 🔴 BLOCK
   - Si APPROVE → `git add -A && git commit -m "type: description"`
