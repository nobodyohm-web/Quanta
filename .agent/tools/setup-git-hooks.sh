#!/bin/bash
# Configuration automatique des Git Hooks pour l'agent IA

HOOK_FILE=".git/hooks/pre-commit"

echo "Installation du hook de pré-commit pour Torus..."

mkdir -p .git/hooks

cat << 'EOF' > $HOOK_FILE
#!/bin/bash
echo "🤖 [AI Workflow] Vérification avant commit..."

# 1. Empêcher les commits qui cassent le build
npm run check
if [ $? -ne 0 ]; then
  echo "❌ Erreur Svelte détectée. Commit annulé. Demandez à Claude de corriger."
  exit 1
fi

cd src-tauri
cargo clippy -- -D warnings
if [ $? -ne 0 ]; then
  echo "❌ Erreur Rust détectée. Commit annulé. Demandez à Claude de corriger."
  exit 1
fi
cd ..

# 2. Régénérer la Repomap silencieusement pour que Claude ait toujours un contexte à jour
echo "🗺️  Génération de la Repomap pour la prochaine session..."
npm run ai:map > /dev/null 2>&1

echo "✅ Prêt."
EOF

chmod +x $HOOK_FILE
echo "✅ Hook installé ! L'IA aura toujours un contexte à jour et le code cassé ne sera plus commité."
