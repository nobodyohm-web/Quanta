---
type: task-spec
id: COVER-2
status: à exécuter
priorité: 🟠 symétrie d'intégrité — couverture aussi au seal (un nœud ne corrompt plus sa propre chaîne)
classe: couverture au seal par CONSTRUCTION (exclure, pas rejeter), réutilise le helper de COVER-1
origine: COVER-1 §4 (seal_block_at ne vérifie pas la couverture → auto-corruption locale)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_COVERAGE_VALIDATION]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# COVER-2 : couverture au seal (construire un bloc valide, pas en rejeter un)

> COVER-1 a fermé la couverture à l'**intégration**, sur les deux chemins. Mais `seal_block_at`
> ne vérifie **pas** la couverture, donc un nœud qui admet une tx non couverte (via
> `replay_remote_tx`) puis la **scelle** corrompt sa **propre** chaîne. Aucun nœud intégrateur
> ne l'accepte, donc rien ne se propage, mais le producteur se saborde. Ce spec ferme la
> **moitié symétrique** côté production. Diff logique seule, déterministe.

## Le principe (la recherche le confirme)
La validation est **déterministe et symétrique** : producteur et intégrateur appliquent les
mêmes règles. Et un producteur **doit construire un bloc valide**, une tx invalide rendant le
bloc entier rejeté par tous les autres. Donc au seal, on ne **rejette** pas (on fabrique le
bloc), on **exclut** les tx non couvertes pour produire un bloc valide **par construction**.

C'est la **différence de sémantique** avec COVER-1 :
- **À l'intégration** (COVER-1) : tu reçois un bloc que tu n'as pas fait ⇒ s'il contient une tx
  non couverte, tu **rejettes le bloc entier**.
- **Au seal** (ici) : tu **fabriques** le bloc ⇒ tu **exclus** les tx non couvertes et tu scelles
  un bloc qui ne contient que du couvert. **Ne refuse pas de sceller**, produis un bloc valide.

## 1. Où, et réutiliser le helper existant
Dans `seal_block_at` (et tout chemin de construction de bloc). **Réutilise** le helper de
couverture de COVER-1 (`onchain_spendable_before` + le rejeu séquentiel), **ne le
réimplémente pas** : une seule source de vérité pour la règle de couverture, sinon les deux
copies divergeront un jour.

## 2. Le contrôle au seal : exclusion séquentielle
- Calcule le solde **avant le bloc** depuis l'**état on-chain** (le helper COVER-1),
  déterministe.
- Parcours les tx candidates **dans l'ordre**, en maintenant un solde courant. Pour chaque tx à
  expéditeur **réel** (Transfert ou Stake) : si elle est **couverte**, garde-la (et applique son
  effet au solde courant) ; si elle est **non couverte**, **exclus-la** du bloc.
- Expéditeurs synthétiques (`NETWORK`/`ESCROW`/`BURN`) exemptés, comme COVER-1, et les crédits
  intra-bloc comptent dans le solde courant.
- Le bloc scellé ne contient donc **que** des tx couvertes.

## 3. L'invariant qui ferme le trou (le test clé)
**Tout bloc produit par `seal_block_at` doit passer `validate_block_against_prev`** (le contrôle
de COVER-1). Autrement dit : un bloc auto-produit est **toujours valide**. C'est la propriété qui
prouve que l'auto-corruption est fermée. Teste-la directement.

## 4. Éviction des tx exclues (hygiène)
Une tx non couverte exclue au seal ne pourra jamais être scellée telle quelle : **évince-la du
mempool** (ou au minimum ne la re-sélectionne pas), pour qu'elle ne stagne pas. **§4** : si la
politique d'éviction soulève un vrai choix (ex. une tx temporairement non couverte qui le
deviendrait), signale-le ; le défaut raisonnable est d'exclure du bloc et d'évincer.

## 5. Le clamp `.max(0)` reste (ne pas y toucher ici)
COVER-1 a **gardé** le clamp parce que `replay_remote_tx` admet encore des tx non couvertes au
mempool (concession à la convergence hors-ordre d'AUDIT-TX-2), donc le cache peut transitoirement
passer négatif. COVER-2 ne change **pas** l'admission, seulement le seal. Donc le clamp **reste**.
Ne le retire pas dans ce spec.

## 6. Tests (obligatoires)
- **exclusion d'un transfert non couvert** : une tx non couverte candidate au seal est **absente**
  du bloc scellé, et le bloc **passe** la validation d'intégration.
- **exclusion d'un stake non couvert** : idem avec un Stake.
- **exclusion séquentielle** : une tx qui devient non couverte après les précédentes est exclue.
- **scénario d'auto-corruption fermé** : admets une tx non couverte via `replay_remote_tx`, puis
  scelle ⇒ le bloc scellé est **valide** (la tx a été exclue), la chaîne n'est **pas** corrompue.
- **INVARIANT §3** : un bloc quelconque produit par `seal_block_at` passe
  `validate_block_against_prev`.
- **pas de régression** : les tx **couvertes** sont scellées normalement (vivacité préservée).
- **conservation + C1 verts**.

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Une seule source de vérité** : réutiliser le helper de couverture de COVER-1, pas le
  dupliquer.
- **Déterminisme** : couverture depuis l'**état on-chain** ; `src/sm/` sans-IO préservé ; **C1
  vert**.
- **Pas de masquage** ni de test mou ; l'invariant §3 doit être **réellement** vérifié.
- **§4** : politique d'éviction (§4) à signaler si elle soulève un choix ; ne pas toucher au
  clamp (§5) ni à l'admission.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les tests du §6 (surtout l'**invariant §3**).
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep + conservation verts**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **COVER-2** au tracker + auto-revue §3, avec : la réutilisation du helper, la sémantique
  d'exclusion, la preuve de l'invariant « bloc auto-produit toujours valide », et le sort des tx
  évincées.

## Séquence
1. **§1 + §2** câbler la couverture au seal avec exclusion séquentielle (helper réutilisé).
2. **§4** évincer les tx exclues.
3. **§6** tests, dont l'**invariant §3** et le scénario d'auto-corruption.

> Après ce spec, la couverture est **symétrique** : rejet à la réception, exclusion à la
> production. Un nœud ne peut plus ni propager ni sceller une dépense non couverte. C'est le
> **dernier durcissement de validation** avant le gadget. Reste devant ce qui ne se délègue pas :
> la validation de la conception du gadget et tes décisions §12 (E, taille de comité, quorum).
