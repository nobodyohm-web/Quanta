---
type: task-spec
id: CRDT-BOUND-1
status: à exécuter
priorité: 🟠 moyenne — dernier item de l'audit (croissance non bornée + boucle O(montant))
classe: ledger CRDT « fantôme » (vestige possible du purge social/marketplace)
nature: audit-first — déterminer vivant/mort, puis retirer ou borner en conséquence
origine: HARDEN-AUDIT-1 (CRDT-BOUND)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# CRDT-BOUND-1 : retirer ou borner le ledger CRDT fantôme

> Dernier item de l'audit. **Audit d'abord** : la question « vestige ou vivant » est un
> **fait** lisible dans le code, pas une préférence. L'agent le **détermine**, puis :
> **retire** si c'est mort (du code mort est de la surface d'attaque), **borne** si c'est
> vivant (sans casser la convergence), et **n'escalade** vers Alexandre que le seul vrai
> arbitrage de conception. Diff logique seule. **Biais conservateur : dans le doute, c'est
> vivant** (on borne, on ne retire pas).

## 1. Déterminer la vivacité (le fait, à rapporter)
Trace les références du ledger CRDT / dual-ledger. Est-il **réellement utilisé** sur le chemin
de comptabilité ou de consensus (soldes, conservation, validation), ou **orphelin** depuis le
purge des modules social/marketplace ? Reporte le constat **avec preuves** (sites d'appel,
qui lit/écrit la structure). Le nom « fantôme » de l'audit suggère le vestige ; confirme-le
ou réfute-le par le code.

## 2a. Si MORT (orphelin / fantôme) → retirer
- Supprime la structure, ses chemins de mise à jour, et la **boucle O(montant)** avec.
- **Prouve la mort avant de retirer** : aucune référence sur un chemin comptabilité/consensus,
  et après retrait la **suite complète**, l'invariant de **conservation** et le **sweep**
  restent **verts**. Si quoi que ce soit casse, c'est que ce n'était pas mort ⇒ bascule en 2b.
- Justification : du code mort sur un projet crypto est de la surface d'attaque pure ; le
  retirer est le correctif **durable**.

## 2b. Si VIVANT (porteur) → corriger les deux bugs, escalader la suite
Il est load-bearing : on **ne le retire pas** ici. On ferme ses deux défauts réels :
- **Croissance non bornée** : ajoute une borne **préservant la convergence** (la commutativité
  CRDT, exactement la contrainte du cap `@pseudo` : pas un compteur dépendant de l'ordre, une
  politique qui ne casse pas `apply()`). Si la politique d'éviction commutative-safe est un
  vrai choix, **§4 signale-la**.
- **Boucle O(montant)** : remplace-la par un calcul **O(1)** ou borné ; une boucle indexée par
  un montant télécommandé est un DoS CPU.
- **§4, le seul vrai arbitrage** : garder-et-borner (fait ici) **contre** refactorer la
  comptabilité pour **se débarrasser** du CRDT est une décision d'Alexandre. **Ne refactore
  pas en silence** : reporte qu'il est vivant, ce qui en dépend, et laisse-lui le choix.

## 3. Biais conservateur
Si la vivacité est **ambiguë** (référencé mais peut-être inerte, chemin indirect), traite-le
comme **vivant** : borne, ne retire pas, et reporte l'ambiguïté. Ne **jamais** supprimer sur
un doute.

## Tests
- **Si retiré** : suite complète + conservation + sweep **verts** (preuve que rien n'en
  dépendait).
- **Si borné** : la borne **tient** sous insertion adverse, la **convergence/commutativité**
  est préservée (deux ordres d'application convergent au même état), et la boucle est désormais
  **bornée** (test adverse sur un gros montant ⇒ pas de DoS).

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact (travail
  niveau ledger).
- **Pas de masquage** : si vivant, les bugs se ferment à la racine, pas par un test mou ; si
  mort, on retire vraiment, on ne neutralise pas en gardant le squelette.
- **§4** : la politique d'éviction commutative-safe, et le choix garder-vs-refactorer, se
  **signalent**, ne se tranchent pas.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert** · `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO
  propre · **C1 vert** · **sweep par défaut vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **CRDT-BOUND-1** au tracker + auto-revue §3, avec **le constat de vivacité** (preuves),
  l'action prise (retrait ou bornage), et toute décision renvoyée en §4.

## Séquence
1. **§1** tracer la vivacité, rapporter.
2. **§2a** retirer si mort (prouvé), **sinon §2b** borner + escalader.
3. Tests selon le cas.

> C'est le dernier verrou de l'audit. Une fois posé, il ne reste de la phase de durcissement
> que la convergence des deux chemins de validation (petite, la cartographie a montré qu'ils
> partagent déjà le cœur), et la vraie suite : tes décisions et le gadget.
