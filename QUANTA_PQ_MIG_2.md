---
type: task-spec
id: PQ-MIG-2
status: à exécuter (deuxième pièce du chantier PQ, §8 de la conception)
priorité: 🟠 schéma d'adresses ML-DSA (adresse = BLAKE3 de la clé publique)
classe: dérivation d'adresse + liaison clé↔adresse, SANS câbler from/to ni verify_tx
origine: [[QUANTA_PQ_MIGRATION_DESIGN]] §3, §4.2, §8 · construit sur PQ-MIG-1
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_PQ_MIG_1]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# PQ-MIG-2 : adresses = BLAKE3 de la clé publique ML-DSA

> Deuxième pièce du chantier PQ. La clé publique ML-DSA fait ~1952 o, **trop grande pour servir
> d'adresse**. On introduit l'**adresse = hash BLAKE3 de la clé publique ML-DSA**, plus la
> **fonction de liaison** clé↔adresse que PQ-MIG-3 utilisera. On **ne câble pas** encore `from`/
> `to` ni `verify_tx` : ça reste contenu, comme PQ-MIG-1 a posé la clé sans la brancher. Diff
> logique seule, déterministe, C1 vert.

## Cadre (ce que PQ-MIG-2 fait, et ne fait PAS)
- **Fait** : la dérivation d'adresse (BLAKE3 de la clé publique ML-DSA), l'encodage/décodage, et
  la **fonction de vérification de liaison** (une adresse correspond-elle à une clé donnée).
  L'identité du moteur (PQ-MIG-1) expose **son** adresse.
- **Ne fait PAS** (pièces ultérieures) : changer les champs `from`/`to` des transactions,
  `verify_tx`, la liaison dans la préimage, le retrait du repli Ed25519 (tout ça = **PQ-MIG-3**),
  l'enjeu (PQ-MIG-4), la genèse (PQ-MIG-5). **§4** : si une de ces couches semble devoir bouger
  pour compiler, signale-le, ne déborde pas.

## 1. Dérivation d'adresse
- `adresse = BLAKE3(domaine || clé_publique_ml_dsa)`, sur les **32 octets** de sortie naturelle
  de BLAKE3 (pas de troncature ; 256 bits de résistance aux collisions). **🛑 réglable** :
  longueur 32 o par défaut, marquée, modifiable plus tard si une adresse plus courte est voulue.
- **Séparation de domaine obligatoire** : préfixe le hash d'une étiquette dédiée (ex.
  `"QUANTA-ADDR-V1"`) pour qu'une adresse ne puisse **jamais** entrer en collision avec un hash
  de bloc ou de transaction. (Même discipline que la séparation de domaine déjà en place pour le
  hash de bloc.)
- **Fonction pure et déterministe** : même clé ⇒ même adresse, sur tous les nœuds. Aucune
  entropie.

## 2. Encodage / décodage
- Encode l'adresse (32 o) en une représentation textuelle (hex, ou l'encodage maison déjà
  utilisé pour les identités). **Round-trip** : encoder puis décoder redonne les mêmes 32 octets.

## 3. La fonction de liaison (ce dont PQ-MIG-3 aura besoin)
- `lie(adresse, clé_publique) -> bool` : vrai **si et seulement si** `adresse == BLAKE3(domaine
  || clé_publique)`. C'est la fonction que `verify_tx` appellera en PQ-MIG-3 pour exiger que la
  clé révélée hashe bien vers l'adresse `from`. Construis-la et **teste ses dents** ici.

## 4. L'identité du moteur expose son adresse
- Le `CryptoEngine` (PQ-MIG-1) expose l'**adresse** dérivée de sa clé publique ML-DSA primaire.
  Lecture seule, pure.

## Garde-fous
- **Périmètre strict** : dérivation + encodage + liaison + exposition d'adresse **seulement**. Ne
  touche pas `from`/`to`, `verify_tx`, l'enjeu, la genèse. **§4 STOP** si une de ces couches
  semble requise.
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : dérivation pure (BLAKE3), aucune entropie ; `src/sm/` sans-IO ; **C1 vert**.
- **Séparation de domaine** sur le hash d'adresse (obligatoire, §1).
- **Pas de masquage** : la fonction de liaison du §3 doit réellement mordre (clé fausse ⇒ faux).
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant : adresse déterministe (clé connue ⇒ adresse connue,
  identique sur deux dérivations) ; round-trip encode/décode ; **dents de liaison** (la bonne clé
  passe `lie()`, une **autre** clé échoue) ; séparation de domaine vérifiée (le même octet brut
  dans un autre contexte ne donne pas la même adresse).
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact · `from`/`to`/`verify_tx`/enjeu
  **inchangés**.
- Entrée **PQ-MIG-2** au tracker + auto-revue §3, notant : la dérivation BLAKE3 domaine-séparée,
  la longueur 32 o marquée réglable, la fonction de liaison et ses dents, le périmètre tenu.

## Séquence
1. **§1** dérivation d'adresse `BLAKE3(domaine || pk)`, 32 o, domaine-séparée.
2. **§2** encodage/décodage avec round-trip.
3. **§3** fonction de liaison `lie(adresse, clé)` + dents.
4. **§4** le moteur expose son adresse.

> Pièce suivante, **PQ-MIG-3** : `verify_tx` en ML-DSA, qui **exige** via `lie()` que la clé
> révélée hashe vers l'adresse `from`, met la clé dans la préimage signée, retire le repli
> Ed25519 et câble le vault PQ en production (le code d'échafaudage de PQ-MIG-1 y disparaît).
> Puis PQ-MIG-4 (enjeu re-clé → **GADGET-3 débloqué**), PQ-MIG-5 (genèse PQ).
