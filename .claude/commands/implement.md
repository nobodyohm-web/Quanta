Implémente la fonctionnalité demandée en suivant ce workflow strict :

1. Lis CLAUDE.md et .agent/memory.md
2. Lis les fichiers design pertinents dans .agent/design/
3. Crée une branche git : `git checkout -b feat/<nom>`
4. Implémente le changement
5. Exécute la boucle de vérification :
   - `cargo check --manifest-path src-tauri/Cargo.toml`
   - `cargo test --manifest-path src-tauri/Cargo.toml`
   - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
   - `npm run build`
6. Si erreur → corrige et reboucle étape 5
7. Commit : `git add -A && git commit -m "<type>: <description>"`
8. Résume ce qui a été fait

Types de commit : feat, fix, refactor, docs, test, chore
