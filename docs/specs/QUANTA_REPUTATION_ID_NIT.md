---
type: task-spec
id: REPUT-ID-1
status: à exécuter (séparé de GADGET-3 ; hors chemin de sécurité)
priorité: 🟡 hygiène — nettoyer le mix transport/adresse dans le moteur de réputation
classe: cohérence d'identité dans reputation.rs, hors consensus
origine: NIT relevé par la revue adverse de PQ-MIG-3B
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# REPUT-ID-1 : nettoyer le mix transport/adresse dans la réputation

> Après le re-keying ML-DSA (PQ-MIG-3B), le moteur de réputation **mélange** encore clé de
> transport (Ed25519) et adresse (ML-DSA). C'est **hors chemin de sécurité** (la réputation a été
> retirée du consensus, ADR-002/STAKE-WEIGHT-1), donc cosmétique, mais c'est une incohérence
> d'identité à clarifier avant qu'elle ne devienne un vrai bug si on câble la réputation ailleurs.
> Petit, isolé, **aucun** consensus touché. Diff logique seule.

## 1. Audit + décision d'identité
- Trace où `reputation.rs` utilise une clé de **transport** vs une **adresse**.
- **Décide une identité cohérente** : la réputation est un signal applicatif par **acteur**.
  L'identité naturelle est l'**adresse ML-DSA** (l'acteur économique), pas la clé de transport
  éphémère. Re-clé la réputation sur l'**adresse**, sauf cas où la clé de transport est
  intrinsèquement la bonne (alors documente pourquoi).

## 2. Cohérence
- Plus de mélange : une seule notion d'identité dans la réputation.
- **Aucun effet consensus** : la réputation reste hors du poids/quorum/élection (vérifier qu'on
  n'a rien re-couplé au consensus par accident).

## Garde-fous
- **Périmètre strict** : `reputation.rs` (et appelants directs) **seulement**. **§4 STOP** si ça
  semble toucher le consensus.
- **Diff logique seule** ; `dispatcher.rs` intact.
- **C1 vert**, sweep + conservation verts, `src/sm/` sans-IO.
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert** · `clippy --lib -D warnings` propre · **C1 vert** · sweep +
  conservation verts.
- `git diff` logique seule, confinée à la réputation · consensus **inchangé** (poids/quorum/
  élection intacts).
- Entrée REPUT-ID-1 au tracker + auto-revue §3.

## Séquence
1. Auditer le mix transport/adresse.
2. Re-clé la réputation sur l'adresse ML-DSA (identité cohérente).
3. Vérifier zéro effet consensus.

> Pur nettoyage d'hygiène. À lancer **après** GADGET-3, dans un run séparé, pour garder le diff de
> consensus propre.
