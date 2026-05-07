# Torus Dev API

> Petit serveur HTTP local exposé par Quanta pour publier, lister et chercher
> des sites Torus depuis VSCode, un terminal ou tout outil externe — sans
> passer par le PageBuilder graphique.

- **Endpoint** : `http://127.0.0.1:7654`
- **Désactivé par défaut** — l'API ne répond qu'aux requêtes provenant de
  loopback (127.0.0.1) et seulement quand le toggle a été activé dans
  l'application.
- **Authentification** : Bearer token (32 octets hex). Stocké dans
  `~/.torus/dev-api-token`.

---

## 1. Activer l'API

1. Ouvrir Quanta.
2. Aller dans **Réglages → API Développeur**.
3. Cliquer sur le toggle pour passer à **Activée**.
4. Cliquer sur **Afficher** pour révéler le token, puis **Copier**.

> Le bouton **Régénérer** invalide immédiatement l'ancien token.

---

## 2. Endpoints

### `GET /api/health`

Sans authentification. Retourne `200` si l'API est activée, `503` sinon. Utile
pour les outils externes qui veulent vérifier la disponibilité.

```bash
curl http://127.0.0.1:7654/api/health
# → { "status": "ok", "enabled": true }
```

---

### `GET /api/status`

Retourne l'état du nœud : clé publique, solde, nombre de sites publiés,
documents indexés.

```bash
curl -H "Authorization: Bearer $TOKEN" \
     http://127.0.0.1:7654/api/status
```

Réponse :

```json
{
  "pk": "9f0a…",
  "balance_qta": 42.5,
  "sites_count": 1,
  "search_docs": 153,
  "endpoint": "127.0.0.1:7654"
}
```

---

### `POST /api/publish`

Publie un site (signature Ed25519, store local, broadcast P2P, auto-index
dans le moteur de recherche avec tags).

Champs :

| champ    | type      | défaut          | description                                            |
|----------|-----------|-----------------|--------------------------------------------------------|
| `title`  | string    | _requis_        | Titre humain (≤ 100 chars)                             |
| `html`   | string    | _requis_        | HTML complet (≤ 64 KB)                                 |
| `tags`   | string[]  | auto-extraction | Liste de tags (max 10, normalisés)                     |
| `lang`   | string    | `"fr"`          | Code langue ISO court (`fr`, `en`, `es`)               |
| `kind`   | string    | `"site"`        | `site`, `blog`, `forum`, `comment`, `shop`             |
| `domain` | string?   | `null`          | Domaine `.torus` associé (optionnel)                   |

```bash
curl -X POST http://127.0.0.1:7654/api/publish \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "title": "Mon site",
       "html": "<h1>Hello</h1><p>Bienvenue sur Torus.</p>",
       "tags": ["web", "torus"],
       "lang": "fr",
       "kind": "site"
     }'
```

Réponse :

```json
{
  "cid": "8a1f…",
  "author_pk": "9f0a…",
  "version": 3,
  "tags": ["web", "torus"]
}
```

---

### `GET /api/search?q=…&lang=…&tag=…&limit=…`

Recherche dans l'index P2P local (ranking BM25 + signaux sociaux + tag boost).

```bash
curl -H "Authorization: Bearer $TOKEN" \
     "http://127.0.0.1:7654/api/search?q=chaussures&lang=fr&tag=mode&limit=10"
```

Retourne un tableau de hits :

```json
[
  {
    "cid": "8a1f…",
    "title": "Boutique de chaussures",
    "snippet": "Nos modèles running…",
    "author_pk": "9f0a…",
    "torus_domain": "boutique.torus",
    "kind": "Site",
    "lang": "fr",
    "updated_at": 1714998000,
    "score": 12.43
  }
]
```

---

### `DELETE /api/site`

Dépublie le site du wallet courant. La page est remplacée localement par un
HTML vide signé (incrémentation de version) et retirée de l'index de
recherche local.

```bash
curl -X DELETE \
     -H "Authorization: Bearer $TOKEN" \
     http://127.0.0.1:7654/api/site
# → { "deleted": true, "version": 4 }
```

---

## 3. Workflow VSCode

1. Activer l'API et copier le token.
2. Dans VSCode, créer un fichier `index.html`.
3. Coller la commande suivante dans un terminal intégré :

   ```bash
   export TORUS_TOKEN=$(cat ~/.torus/dev-api-token)
   curl -X POST http://127.0.0.1:7654/api/publish \
     -H "Authorization: Bearer $TORUS_TOKEN" \
     -H "Content-Type: application/json" \
     -d "$(jq -Rn --arg html "$(cat index.html)" \
                  --arg title "Mon site Torus" \
                  '{title:$title, html:$html, kind:"site"}')"
   ```

4. Re-publier après chaque modification — la version s'incrémente
   automatiquement, le réseau récupère la nouvelle copie via gossip.

> **Astuce** : créer une tâche VSCode dans `.vscode/tasks.json` pour publier
> avec `Ctrl+Shift+B`.

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "Torus: publish",
      "type": "shell",
      "command": "curl",
      "args": [
        "-s", "-X", "POST",
        "http://127.0.0.1:7654/api/publish",
        "-H", "Authorization: Bearer ${env:TORUS_TOKEN}",
        "-H", "Content-Type: application/json",
        "--data-binary",
        "@${workspaceFolder}/site.json"
      ],
      "problemMatcher": []
    }
  ]
}
```

---

## 4. Sécurité

- Le serveur ne bind que sur **127.0.0.1** (`is_loopback()` re-vérifié sur
  chaque connexion).
- Toutes les routes sauf `/api/health` exigent le token Bearer.
- Le token est généré via BLAKE3 sur des entrées système non prédictibles
  (timestamp nano + adresse mémoire + thread id) ; si tu suspectes une fuite,
  régénère-le depuis Settings.
- Aucune route n'accepte de redirection ou de fetch externe : l'API n'agit
  que sur l'état local du wallet déverrouillé.
- Si l'identité n'est pas déverrouillée, `POST /api/publish` et
  `DELETE /api/site` retournent **500** avec un message explicite.

---

## 5. Limites

| Limite                            | Valeur          |
|-----------------------------------|-----------------|
| Taille requête                    | 16 MB           |
| Taille HTML d'une page            | 64 KB           |
| Tags par site                     | 10 max          |
| Longueur d'un tag                 | 30 chars max    |
| Concurrence                       | 1 connexion / handler dédié (tokio) |
