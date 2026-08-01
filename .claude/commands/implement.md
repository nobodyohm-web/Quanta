Implémente la fonctionnalité demandée en suivant ce workflow strict :

1. Lis CLAUDE.md (référence vivante du projet)
2. Crée une branche git : `git checkout -b feat/<nom>`
3. Implémente le changement
4. Exécute la boucle de vérification :
   - `cargo check --manifest-path src-tauri/Cargo.toml`
   - `cargo test --manifest-path src-tauri/Cargo.toml`
   - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
   - `npm run build`
5. Si erreur → corrige et reboucle étape 4
6. Commit : `git add -A && git commit -m "<type>: <description>"`
7. Résume ce qui a été fait

Types de commit : feat, fix, refactor, docs, test, chore
