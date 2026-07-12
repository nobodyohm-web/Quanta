---
type: task-spec
id: PQ-MIG-1
status: à exécuter (première pièce du chantier PQ, §8 de la conception)
priorité: 🔴 re-raciner CryptoEngine/vault sur ML-DSA (fondation du tout-PQ)
classe: identité primaire ML-DSA dans le moteur crypto + vault, AUCUNE autre couche touchée
origine: [[QUANTA_PQ_MIGRATION_DESIGN]] §4.1, §8 · ratifie [[ADR-007]] (b)
liens: [[QUANTA_AGENT_CONSTITUTION]] · CRYPTO-ID-1 (audit) · [[AUDIT_QUANTA_2_PROGRESS]]
---

# PQ-MIG-1 : re-raciner CryptoEngine et le vault sur ML-DSA

> Première pièce du chantier post-quantique (ADR-007 (b) tranché). On fait de **ML-DSA-65
> l'identité primaire** générée, stockée et chargée par le moteur crypto et le vault. **Aucune
> autre couche n'est touchée** : ni `verify_tx`, ni les adresses, ni l'enjeu, ni le transport.
> C'est la pièce la plus contenue, sans dépendance amont. Diff logique seule, déterministe,
> C1 vert.

## Cadre (ce que PQ-MIG-1 fait, et surtout ne fait PAS)
- **Fait** : génère, stocke (zeroize), recharge une paire **ML-DSA-65** comme identité primaire
  du moteur crypto / vault. La clé ML-DSA devient la racine.
- **Ne fait PAS** (pièces ultérieures, ne pas anticiper) : adresses BLAKE3 (PQ-MIG-2),
  `verify_tx` + liaison + retrait du repli Ed25519 (PQ-MIG-3), re-clé de l'enjeu et `@pseudo`
  (PQ-MIG-4), genèse (PQ-MIG-5). **§4** : si une de ces couches semble devoir bouger pour
  compiler, signale-le, ne déborde pas.
- **Coexistence temporaire** : Ed25519 peut **rester présent** ailleurs à ce stade (les couches
  qui l'utilisent encore ne sont pas dans cette pièce). On ajoute/établit la racine ML-DSA ; on
  ne casse pas encore les consommateurs Ed25519. L'objectif est que la **génération d'identité
  primaire** soit ML-DSA, proprement, sans régression.

## 1. Génération de l'identité primaire ML-DSA
- Le moteur crypto génère une paire **ML-DSA-65** comme identité primaire.
- **Déterminisme en sim** : la génération et le signage empruntent le **chemin déterministe**
  déjà en place (travail SIGN-DET, cfg(test)). Jamais d'entropie hedgée en simulation, sinon
  **C1 casse**. C'est le point de vigilance n°1.

## 2. Stockage et chargement (vault)
- Le vault stocke et recharge la clé secrète ML-DSA, avec **zeroize** sur la clé secrète en
  mémoire (invariant §3 de la constitution, déjà appliqué à Ed25519, à reproduire pour ML-DSA).
- Recharger une identité stockée redonne **exactement** la même clé publique (round-trip
  stockage→chargement testé).

## 3. Pas de fuite non déterministe
- La clé ML-DSA secrète n'apparaît dans **aucune** empreinte déterministe (C1). Vérifie que
  l'ajout de la racine ML-DSA ne fait pas lire une donnée non déterministe dans le fingerprint.

## Garde-fous
- **Périmètre strict** : moteur crypto + vault **seulement**. Ne touche pas `verify_tx`, les
  adresses, l'enjeu, le transport, la genèse. **§4 STOP** si une de ces couches semble requise.
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : génération/signage ML-DSA par le chemin déterministe en sim ; `src/sm/`
  sans-IO préservé ; **C1 vert** (vigilance n°1).
- **Zeroize** sur la clé secrète ML-DSA, comme pour Ed25519.
- **Pas de masquage** : aucune régression cachée derrière un `#[allow]` ou un test mou.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant : round-trip stockage→chargement de l'identité ML-DSA
  (même clé publique), et un test de signage/vérification ML-DSA de bout en bout via le moteur.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule**, confinée au moteur crypto + vault · `dispatcher.rs` intact ·
  les couches Ed25519 non visées **inchangées**.
- Entrée **PQ-MIG-1** au tracker + auto-revue §3, notant : l'identité primaire ML-DSA, le
  zeroize, le déterminisme en sim, et le périmètre tenu (aucune autre couche touchée).

## Séquence
1. **§1** génération de la paire ML-DSA-65 comme identité primaire (chemin déterministe en sim).
2. **§2** stockage/chargement vault avec zeroize et round-trip testé.
3. **§3** vérifier l'absence de fuite non déterministe (C1).

> Pièce suivante, **PQ-MIG-2** : adresses = `BLAKE3(pk_ml_dsa)` tronquées, puisque la clé brute
> fait ~1952 o. Puis PQ-MIG-3 (`verify_tx` ML-DSA + liaison clé↔adresse, retrait du repli
> Ed25519), PQ-MIG-4 (enjeu re-clé → **GADGET-3 débloqué**), PQ-MIG-5 (genèse PQ). On avance
> couche par couche, chacune prouvée par le harnais.
