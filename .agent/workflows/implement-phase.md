# Workflow : Implémenter une Phase Torus

## Quand utiliser ce workflow
Lorsqu'on te demande d'implémenter une phase spécifique de la roadmap Torus (Phase 2.1, 4.1, 5.1, etc.)

## Étapes

### 1. Charger le contexte
- Lis `.agent/memory.md` pour les décisions architecturales et bugs passés
- Lis le plan de tâche correspondant dans `.agent/tasks/phase-*.md`
- Si le plan n'existe pas, crée-le d'abord

### 2. Comprendre l'état actuel
- Lis les fichiers Rust impactés (listés dans le plan de tâche)
- Lis les composants Svelte impactés
- Note les types et interfaces existants

### 3. Implémenter (ordre strict)
1. **Backend Rust d'abord** : Types, puis logique, puis commandes Tauri
2. **Commandes Tauri** : Enregistrer dans `generate_handler![]` dans `lib.rs`
3. **Frontend Svelte ensuite** : Appels `invoke()` + UI

### 4. Valider
```bash
# OBLIGATOIRE — ne réponds jamais sans avoir vérifié :
npm run ai:check
```
Si des erreurs apparaissent, corrige-les toi-même avant de continuer.

### 5. Mettre à jour la mémoire
- Ajoute les nouvelles décisions architecturales dans `.agent/memory.md`
- Note les bugs rencontrés et résolus

### 6. Résumé
- Liste les fichiers modifiés
- Décris les changements majeurs
- Indique la prochaine phase recommandée
