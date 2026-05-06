# Torus — Le Web P2P récompensé

> **Whitepaper V3 — Mai 2026**
> Un protocole où créer, découvrir et modérer rapporte des QUANTA.
> Pas de serveur. Pas d'algorithme caché. Pas de censeur.

---

## 1. Pourquoi Torus

Le Web actuel est un oligopole d'attention :

| Problème | Effet |
|---|---|
| 5 plateformes captent 80% du trafic | Censure unilatérale, démonétisation arbitraire |
| Algorithmes de classement opaques | Personne ne sait *pourquoi* un contenu remonte |
| Hébergement centralisé | Si AWS tombe, la moitié du Web tombe |
| Modération opaque | Un compte supprimé = un travail perdu |
| Créateurs paient (hosting) ou vendent leur audience | Aucune valeur captée par le créateur direct |

**Torus inverse le contrat** :
- L'hébergement est mutualisé (P2P, BLAKE3 content-addressing).
- Le classement est public (algorithme **QuantaRank** open-source, paramètres on-chain).
- La modération est exercée par un jury aléatoire (style Kleros, mais P2P pur).
- Chaque interaction (publication, like, abonnement, modération honnête) **rapporte des QUANTA**.

---

## 2. Vision en 30 secondes

> Tu télécharges Torus. Tu génères ton wallet. Tu publies `torus://alex` en 3 clics. Quelqu'un cherche « cuisine vegan rapide », tombe sur ton site, te like, t'abonne. Tu mines plus. Tu achètes son ebook avec tes QUANTA. Lui modère un thread, gagne du QUANTA aussi. Personne ne possède Torus.

---

## 3. Coin QUANTA — invariants

| Paramètre | Valeur |
|---|---|
| Émission | **100 QUANTA / heure**, fixe, à perpétuité |
| Halving | **Aucun** — l'inflation devient asymptotiquement nulle quand le supply croît |
| Distribution (Shapley v2) | 25% énergie · 25% travail compute · 20% validation · 15% uptime · **15% utilité sociale** |
| Burn | 1% par transfert · 2% par tâche compute · 5% par boost · 10% slashing modération |
| Unité | 1 QUANTA = 1 000 000 µQTA (`u64`, déterministe) |
| Bridge | Quand masse critique : pool Uniswap v4 + bridge ERC-20 audité |

**Pourquoi 100/h fixe** : un coin avec halving favorise les early-adopters au détriment des nouveaux. Un coin avec inflation nominale fixe + burn variable converge vers une émission *réelle* nulle quand l'usage explose, sans privilégier qui que ce soit.

---

## 4. Architecture — vue d'ensemble

```
┌──────────────────────────────────────────────────────────┐
│                    APPLICATION TAURI                     │
│  ┌────────────────────┐    ┌──────────────────────────┐  │
│  │  Frontend Svelte 5 │ ←→ │   Backend Rust (tokio)   │  │
│  │ Browser · Builder  │IPC │  P2P · Search · Social   │  │
│  │ Search · Forums    │    │  Mining · Ledger · Mod   │  │
│  └────────────────────┘    └──────────┬───────────────┘  │
└────────────────────────────────────────┼──────────────────┘
                                          │ Iroh QUIC + Gossip
            ┌─────────────────────────────┼─────────────────────────────┐
            ▼                             ▼                             ▼
   ┌────────────────┐           ┌────────────────┐           ┌────────────────┐
   │  Pair Alice    │           │   Pair Bob     │           │  Pair Charlie  │
   │  Index shard A │           │ Index shard B  │           │  Index shard C │
   │  Pages, votes  │           │ Pages, votes   │           │  Pages, votes  │
   └────────────────┘           └────────────────┘           └────────────────┘
                          (DAG BLAKE3 + Ledger CRDT)
```

---

## 5. Publication d'un site

### 5.1 PageBuilder
- Éditeur WYSIWYG par blocs : **titre · paragraphe · image · vidéo · lien · code · embed**
- Templates : **blog · vitrine · portfolio · shop · landing · forum**
- Aperçu live, multi-page, navigation interne
- Toggle JS sandboxé (opt-in par site, désactivé par défaut)

### 5.2 Stockage
1. Le site est sérialisé en arbre `Site { routes: HashMap<path, PageNode> }`
2. Chaque page > 64 KB est découpée en chunks DAG BLAKE3 (réutilise `merkle_dag.rs`)
3. Le manifest `Site` est signé Ed25519 par le wallet créateur
4. Diffusion via gossip `PublishSite { manifest_cid }`
5. Les pairs intéressés (déjà abonnés ou recherche match) téléchargent à la demande

### 5.3 Pinning incentivé (innovation)
> **Problème classique du P2P** : si personne ne pin, le contenu disparaît.
> **Solution Torus** : un pair peut déclarer `Pin { cid, until_ts }`. À chaque téléchargement servi par ce pair, le créateur du contenu reçoit `0.001 QUANTA`, le pair pinneur `0.0005 QUANTA`. Statistique vérifiable on-chain (compteurs CRDT).

---

## 6. Noms de domaine — `*.torus`

### 6.1 Registre
- Format : `^[a-z0-9-]{2,40}\.torus$`
- Stockage : `HashMap<name, DomainRecord>` répliqué CRDT
- `DomainRecord { name, owner_pk, target_pk, value_qta, last_paid_ts, signature }`

### 6.2 Harberger Tax (innovation)
> **Problème** : sur ENS, des squatters achètent des noms évidents (`google.eth`) et les gardent à vie pour 5 $/an.
> **Solution Torus** : le propriétaire **déclare** la valeur de son domaine (`value_qta`). Il paie un loyer mensuel = `value_qta × 1%`. **N'importe qui** peut racheter le domaine en payant exactement `value_qta` au propriétaire actuel.
>
> ⇒ Si tu sous-évalues, tu te fais racheter à perte. Si tu surévalues, tu paies trop de loyer. **Le marché trouve le juste prix.**

### 6.3 Sous-domaines
- Le propriétaire de `alex.torus` signe un `SubdomainGrant { sub, target_pk }` pour `shop.alex.torus`
- Délégation arbitraire en profondeur (`a.b.c.alex.torus`)

### 6.4 Période de grâce
- 30 jours après expiration de loyer, le nom reste réservé au propriétaire (avertissement)
- Au-delà, le nom revient au pool public

---

## 7. Moteur de recherche — QuantaRank

### 7.1 Index inversé distribué
- Tokenizer multilingue (FR, EN, ES, DE, JA initial) : NFKD + lowercase + stop-words
- Chaque token est shardé : `shard_id = blake3(token)[0..2] % N`
- Réplication k=3 par shard (résilience à la perte de pairs)
- Chaque pair maintient les shards qui lui sont assignés

### 7.2 Algorithme QuantaRank
```
score(page, query) =
   Σ termes ∈ query  TF-IDF(terme, page)
 × log(1 + likes_pondérés(page))
 × log(1 + abonnés(auteur))
 × reputation(auteur)^0.5
 × freshness(page.updated_at)        # half-life 30 jours
 × diversity_bonus(page, results)    # pénalise la sur-représentation d'un auteur
 × (1 - moderation_malus(auteur))    # 0 si banni, 0.5 si warn
```

### 7.3 Anti-spam SEO
- Un mot-clé ne peut apparaître plus de 5× dans les méta-tags d'une page (sinon on ignore)
- Mots cachés (CSS `display:none`) → pénalité
- Réseau d'auto-likes (clusters fermés détectés via graphe) → pondération annulée

### 7.4 Filtres utilisateur
`lang` · `since` · `type` (`site`, `forum`, `shop`, `blog`, `comment`) · `creator` · `min_likes`

---

## 8. Économie d'attention — `social.rs`

### 8.1 Like quadratique (innovation)
> **Problème** : sur Twitter, un like coûte 0. Une ferme de bots peut générer 1 M de likes pour 0 $.
> **Solution Torus** : chaque like coûte au minimum 0,1 QUANTA. Tu peux mettre plus pour amplifier ; influence = √(QTA dépensé). Mettre 100 QTA sur **un** like = 10× influence d'un like normal. Mettre 1 QTA × 100 likes différents = 100× plus efficace que 100 QTA sur 1 like. **Force la diversité.**

### 8.2 Abonnements à 3 tiers
| Tier | Coût | Effet |
|---|---|---|
| 1 (signal) | 0 QTA | Suis le créateur (notifications) |
| 2 (supporter) | 1 QTA / mois | +5% de mining boost pour le créateur |
| 3 (mécène) | 10 QTA / mois | +15% de mining boost pour le créateur |

L'abonné ne paie pas le réseau ; il **redirige** une part de son propre mining futur. Les QUANTA brûlés en compensation sont aussi à l'échelle (5%/15% du mining mensuel de l'abonné).

### 8.3 Tip · Boost · Sponsor
- **Tip** : transfert direct, mémo, taxe 1% (BME)
- **Boost** : payer X QUANTA pour ranking ×1,5 pendant 24h. Cap : 100 QTA / page / jour. Burn 5%.
- **Sponsor** : flux récurrent créateur → créateur, déductible du mining sponsorisé

---

## 9. Modération — Jury VRF (Kleros-like)

### 9.1 Cycle d'un signalement
```
1. Reporter signale  ──► 0.1 QTA dépensé (anti-spam)
2. Si ≥5 reports indépendants  ──► déclenche jury
3. VRF tire 7 jurés  ──► parmi pool stake ≥100 QTA, rep > 0.6
4. Vote scellé 24h  ──► commit-reveal Schnorr
5. Verdict majorité  ──► payouts/slashing
```

### 9.2 Verdicts et conséquences
| Verdict | Créateur | Reporters | Jurés majoritaires | Jurés minoritaires |
|---|---|---|---|---|
| Innocent | rien | -0.1 QTA chacun | +0.5 QTA | 0 |
| Warning | -10% mining 7j | rien | +0.5 QTA | 0 |
| Hide | -50% mining 30j, vitrine masquée | rien | +0.5 QTA | 0 |
| Ban | vitrine permanente off, -10% slashing balance | rien | +0.5 QTA | 0 |

### 9.3 Appel
Coût : 50 QTA. Super-jury de 21 jurés. Verdict définitif.

### 9.4 Pourquoi VRF + commit-reveal
- **VRF** (`schnorrkel`) : sélection prouvablement aléatoire, vérifiable, non-manipulable
- **Commit-reveal** : empêche les jurés de copier les votes des premiers
- **Schelling point** : voter avec la majorité paie ; les jurés cherchent la "vérité focalisable", pas leur opinion

---

## 10. Anti-troll graduel

```
reports validés (30 derniers jours)  →  malus mining
   1                                 →  warning
   3                                 →  -10%
   5                                 →  -25%
   8                                 →  -50%
  12                                 →  -100% + vitrine off
```

**Récupération** : +1% par like positif validé. Reset complet après 30 jours sans nouveau report.

---

## 11. Web of Trust — `trust_graph.rs`

> **Problème PageRank classique** : sensible aux fermes de liens.
> **Solution Torus** : chaque user calcule **localement** un PageRank personnalisé partant de **lui-même** (damping 0,85, 20 itérations). Les fermes de likes externes ne te touchent pas si tu ne les suis pas.

Score de confiance utilisé pour pondérer :
- Le poids des likes reçus dans QuantaRank (du POV de qui cherche)
- L'éligibilité au pool de jurés
- La pondération des reports

---

## 12. Forums — Threads DAG

- Forum = nœud racine signé (`name`, `description`, `creator_pk`)
- Thread = enfant du forum, body sur DAG (>64 KB chunked)
- Comment = enfant d'un thread ou d'un comment (réponses imbriquées)
- Like / dislike / report par nœud
- **Soft-fork** : un user peut copier un thread + l'embrancher différemment (clone signé, lien retour vers original) — utile pour scinder une discussion qui dérive

---

## 13. Identité et vie privée

### 13.1 Pseudo + Wallet
- Identité = clé Ed25519 (32 octets)
- Pseudo affiché = libre, vérifié par le wallet (pas d'unicité globale forcée)
- Avatar = identicon BLAKE3 par défaut

### 13.2 Proof-of-personhood léger (innovation)
> Pour réduire la barre, on ne demande pas KYC. On utilise un **âge de wallet + uptime** pondéré.
> Pour des actions sensibles (jury, vote pondéré fort), on peut requérir un *attestation circle* : 5 wallets eux-mêmes attestés vouchent pour toi via signature. Web of trust scellé.

### 13.3 Pages chiffrées (groupes privés)
- Chiffrement symétrique AES-256-GCM, clé chiffrée par NaCl box pour chaque membre abonné
- Le créateur peut révoquer un membre (rotation de clé + re-publication chunks)

---

## 14. Marketplace (déjà existant, élargi)

Le module `marketplace.rs` v2 (tâches compute) reste. V3 ajoute :
- **Services humains** : devs, designers, traducteurs proposent leurs prestations payées QUANTA. Escrow + arbitrage par jury si litige.
- **Items numériques** : ebooks, musiques, modèles 3D, code source — chiffrés, débloqués à l'achat.
- **Commissions** : 1% au protocole (burn), 0,5% au créateur du shop si embed sur autre site.

---

## 15. Bridges et exchanges (long terme)

Quand 100 000 wallets actifs / 30 jours :
1. Pool Uniswap v4 (Ethereum L2) — paire QTA/USDC
2. Bridge ERC-20 audité (LayerZero ou Wormhole)
3. Listing CEX (CoinGecko → Gate.io → MEXC → Kraken progressif)
4. **Pas de pré-mine** : aucun token n'est créé hors mining. Le bridge nécessitera un *lockbox* on-protocol.

---

## 16. Roadmap

| Phase | Contenu | Statut |
|---|---|---|
| V2 | Mining énergie, ledger, gossip, marketplace compute | ✅ |
| V3.0 | CLAUDE.md + Whitepaper pivot | ✅ ce document |
| V3.1 | Modules backend `domains` · `search` · `social` · `moderation` · `forums` · `trust_graph` | 🚧 en cours |
| V3.2 | Site multi-pages + assets DAG, gossip étendu | 🚧 |
| V3.3 | Frontend : Browser, PageBuilder, Search, Profile, Forums | 🚧 |
| V3.4 | Tests + audit sécurité externe | À faire |
| V3.5 | Bêta publique 100 testeurs | À faire |
| V3.6 | Bridge + listing | T+12 mois |

---

## 17. FAQ

**Q. Et si un site est illégal ?**
R. Jury communautaire. Si verdict `Hide`/`Ban`, les pairs cessent volontairement de servir le contenu. Les pairs récalcitrants risquent leur propre réputation (les autres pairs peuvent les blacklister).

**Q. Et si Iroh tombe ?**
R. Iroh est un transport (QUIC). On peut basculer libp2p ou implémenter notre propre transport. L'architecture (DAG, ledger CRDT, gossip) est transport-agnostique.

**Q. Et si un État tente de censurer ?**
R. Aucun point central à fermer. Les pairs peuvent tourner sur Tor/I2P. Pour bloquer Torus, il faudrait bloquer toutes les connexions QUIC sortantes du pays.

**Q. Et si quelqu'un publie 1 million de pages spam ?**
R. Coût : 1 QTA × 1M = 1M QTA. Plus le coût Harberger des domaines. Plus le slashing si signalé. Économiquement non viable.

**Q. Comment commencer ?**
R. Télécharge l'app, génère un wallet. 1h plus tard tu as 100 QTA disponibles (mining solo). Tu peux acheter ton premier domaine.

---

## 18. Conclusion

Torus n'est pas un Twitter décentralisé, ni un Google libre, ni un Wordpress P2P.
**C'est les trois en un seul protocole, avec un coin qui aligne créateurs, modérateurs et lecteurs.**

L'architecture est conservatrice : Rust + Iroh + Ed25519 + BLAKE3 — du standard cryptographique audité. L'innovation est dans la composition : Harberger pour les noms, quadratic voting pour les likes, jury VRF pour la modération, web of trust personnel pour le ranking, mining énergie pour l'émission.

Le but n'est pas de remplacer Google demain. C'est de prouver qu'un Web où **chaque interaction crée de la valeur partagée** est techniquement possible — et économiquement soutenable.

---

*Torus est un protocole libre (AGPLv3). Le code est ouvert, les paramètres sont sur le ledger, les décisions de gouvernance future passeront par vote on-chain pondéré reputation.*
