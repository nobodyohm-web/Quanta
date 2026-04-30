# Polytracker V2 — Architecture & Refactoring Workflow

// turbo-all

## 🎯 Objectif
Transformer **Polytracker** (actuellement un script monolithique basé sur `HTTPServer` avec du HTML embarqué) en une application de niveau institutionnel.
Le but est d'avoir une séparation nette entre le backend (FastAPI + WebSockets) et le frontend (Svelte 5 ou Vanilla JS ultra-moderne avec Tailwind CSS / Framer Motion), tout en optimisant la boucle de trading.

## 🛠️ Stack Technique Cible
- **Backend** : FastAPI (remplace `http.server`), Uvicorn, WebSockets pour le temps réel.
- **Base de données** : aiosqlite (maintenu), mais avec une couche de cache (ex: Redis local ou dictionnaire en mémoire sécurisé) pour alléger les lectures.
- **Frontend** : Séparation stricte des fichiers. Fichiers statiques (HTML/CSS/JS) servis par FastAPI, avec un design "Glassmorphism", un mode sombre élégant (Dark Mode), des micro-animations et une responsivité parfaite.
- **Logique** : Optimisation des boucles asynchrones dans `engine.py` et `watcher.py` avec `asyncio.gather` pour un traitement parallèle des wallets.

## 📋 Plan d'Action (Étapes pour Claude)

### Étape 1 : Refonte du Serveur API Backend
1. Installe les dépendances nécessaires : `fastapi`, `uvicorn`, `websockets`.
2. Modifie `polytracker/dashboard.py` pour remplacer `HTTPServer` par une application **FastAPI**.
3. Crée des endpoints REST propres (`/api/v1/live`, `/api/v1/wallets`, `/api/v1/stats`, `/api/v1/arb`).
4. Mets en place un endpoint **WebSocket** (`/ws/live`) pour pousser les mises à jour (positions, arbitrage) au client en temps réel, éliminant ainsi le besoin de rafraîchir toutes les 30 secondes.

### Étape 2 : Extraction et Modernisation du Frontend
1. Supprime l'immense chaîne de caractères `_PAGE_HTML` dans `dashboard.py`.
2. Crée un dossier `/frontend/` ou `/static/` à la racine du projet.
3. Développe un tableau de bord ultra-premium :
   - Design esthétique (UI de type "Trading Terminal" ou "Bloomberg Terminal" moderne).
   - Utilisation de la police d'écriture Inter/Roboto, mode sombre (Backgrounds `#08090d`, Surfaces `#11131a`).
   - Ajout d'animations fluides lors de l'arrivée d'une nouvelle position (via WebSockets).
   - Intégration de badges dynamiques pour le score Kelly et les alertes de consensus.

### Étape 3 : Optimisation du Moteur (Engine & Watcher)
1. Dans `engine.py` et `watcher.py`, optimise les appels API de fetch_positions en utilisant des sémaphores (`asyncio.Semaphore`) ou du traitement par lots asynchrone (`asyncio.gather`) tout en respectant la limite de taux (rate limit).
2. Ajoute une gestion d'erreurs plus robuste pour éviter que la boucle de `surveillance_loop` ne plante silencieusement en cas de Timeout.
3. Assure-toi que les calculs lourds (ex: scan d'arbitrage dans `arbitrage.py`) ne bloquent pas l'event loop principal.

### Étape 4 : Intégration de l'Intelligence
1. Améliore `classifier.py` pour potentiellement intégrer une passe LLM locale (ex: Llama.cpp ou MLX avec Gemma 4) afin de détecter si un événement Polymarket a un réel potentiel d'inversion.
2. Affine la logique de "Kelly Criterion" dans `kelly.py` pour prendre en compte le Drawdown Maximum dynamique.

---

> **Note pour Claude** : 
> Avant de modifier le code, vérifie l'architecture actuelle via `ls -la` et `cat polytracker/dashboard.py`.
> Ne casse pas l'intégration Telegram existante ni la logique d'arbitrage (garanti/quasi-arb).
> Commence par créer le serveur FastAPI et isoler le frontend dans `static/index.html`.
