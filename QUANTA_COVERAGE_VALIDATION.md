---
type: task-spec
id: COVER-1
status: à exécuter
priorité: 🔴 intégrité avant genèse — rejeter toute dépense/stake non couvert
classe: validation de couverture au bloc, sur le chemin de validation PARTAGÉ
origine: ONCHAIN-STAKE-1 §4 (trou préexistant : verify_tx ne vérifie que la signature) · leçon FORK-CAP
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_FORK_CAP]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# COVER-1 : valider la couverture au bloc (on ne dépense pas ce qu'on n'a pas)

> `verify_tx` ne vérifie que la **signature**, jamais la **couverture**. Une tx forgée à la
> main avec un montant supérieur au solde est donc acceptée, ce qui **gonfle l'offre**. Ce
> n'est **pas un fork** (déterministe sur tous les nœuds), mais c'est une vraie faille
> d'intégrité, et c'est le **dernier trou de validation** avant le gadget. Le principe ne se
> discute pas : on ne dépense ni ne stake ce qu'on n'a pas. Diff logique seule, déterministe.

## Le principe (pas un choix §4)
Toute chaîne rejette une dépense non couverte : Bitcoin par l'UTXO, Ethereum par un contrôle
de solde à l'exécution. Quanta doit faire pareil, **transferts comme stakes**. La seule
question de conception est **où** le contrôle vit, et la réponse est dictée par FORK-CAP.

## 1. Où : dans la validation PARTAGÉE (la leçon FORK-CAP)
Le contrôle de couverture va dans `validate_block_against_prev`, la fonction de validation que
**les deux chemins** utilisent (intégration linéaire **et** reorg). FORK-CAP a montré qu'un
contrôle posé sur un seul chemin est sauté par l'autre. Donc : **un** contrôle, dans la
validation partagée, appliqué aux deux chemins.

## 2. Le contrôle : couverture séquentielle contre le solde on-chain
- Calcule le solde **avant le bloc** depuis l'**état on-chain** (la chaîne jusqu'au parent),
  **déterministe et identique sur tous les nœuds** (jamais le mempool local, sinon
  divergence).
- **Rejoue les tx du bloc dans l'ordre**, en maintenant un solde courant par expéditeur. Pour
  chaque tx à expéditeur **réel** (transfert ou Stake), exige `solde_courant(expéditeur) ≥
  montant`. Sinon, le **bloc est INVALIDE** et rejeté.
- La couverture est **séquentielle** : dans un bloc où Alice (100) fait `→Bob 50` puis
  `→Carol 60`, la première passe (reste 50), la seconde **échoue** (50 < 60). L'ordre est
  déterministe, donc tous les nœuds tranchent identiquement.

## 3. Expéditeurs synthétiques exemptés
Les émissions à expéditeur synthétique (`NETWORK`/`BURN`/`ESCROW`) ne sont **pas** couvertes
par un solde, elles **mintent** : elles sont régies par les règles d'émission (FORK-CAP), pas
par ce contrôle. Le solde courant **inclut** les crédits antérieurs du même bloc, donc une
récompense créditée puis dépensée dans le même bloc est couverte séquentiellement.

## 4. Tuer le clamp qui masquait le bug
Le clamp `.max(0)` sur les soldes **masquait** justement les dépenses non couvertes (c'est lui
qui cachait le sur-bondage trouvé dans ONCHAIN-STAKE-1). Avec la couverture imposée, un solde
négatif **ne peut plus survenir**. Donc vérifie si ce clamp est encore nécessaire : s'il ne
faisait que masquer ça, **retire-le**, pour qu'un négatif futur (un bug) soit attrapé
bruyamment par la conservation au lieu d'être caché. **§4** : si le retrait a d'autres effets,
signale-le, ne force pas.

## 5. Défense en profondeur au mempool (optionnel)
L'admission peut **aussi** rejeter une tx manifestement non couverte, mais le contrôle
**autoritaire** est au bloc (§1). Garde le spec centré sur le bloc ; la couche mempool est un
bonus, pas le cœur.

## 6. Tests (adverses, obligatoires)
- **transfert non couvert** : un bloc contenant un transfert à expéditeur réel dont le montant
  dépasse le solde on-chain ⇒ **bloc rejeté**.
- **stake non couvert** : idem avec un Stake ⇒ **bloc rejeté**.
- **couverture séquentielle** : un bloc où une tx ultérieure devient non couverte après les
  précédentes ⇒ **rejeté** (le cas Alice/Bob/Carol).
- **les deux chemins** : un **gagnant de reorg** contenant une tx non couverte est **rejeté**
  exactement comme à l'intégration linéaire (preuve que le contrôle est bien sur le chemin
  partagé, pas un seul).
- **bloc valide** : un bloc dont toutes les tx sont couvertes **passe** (pas de régression de
  vivacité).
- **conservation + déterminisme** : avec la couverture imposée, la classe « dépense non
  couverte casse/masque la conservation » est fermée **à la validation** ; **C1 vert**.

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : couverture calculée depuis l'**état on-chain**, jamais le mempool local ;
  `src/sm/` sans-IO préservé ; **C1 vert**.
- **Pas de masquage** : un bloc non couvert doit être **rejeté à la validation**, jamais
  absorbé par un clamp ou un test mou.
- **§4** : le **principe** (rejeter le non-couvert) est décidé ; si une **sous-question** réelle
  émerge (retrait du clamp avec effets de bord, interaction inattendue), signale-la.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les 6 tests du §6.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep + conservation verts**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **COVER-1** au tracker + auto-revue §3, avec : l'emplacement du contrôle (validation
  partagée), la sémantique séquentielle, le sort du clamp `.max(0)`, et la preuve que les deux
  chemins rejettent le non-couvert.

## Séquence
1. **§1 + §2** contrôle de couverture séquentiel dans la validation partagée.
2. **§3** exempter les expéditeurs synthétiques, inclure les crédits intra-bloc.
3. **§4** statuer sur le clamp `.max(0)`.
4. **§6** tests adverses, dont le chemin de reorg.

> Ce spec ferme le **dernier trou de validation** avant le gadget : aucune dépense ni aucun
> stake non couvert n'est plus accepté, sur les deux chemins. Il est **indépendant** d'ADR-003
> et du slashing (c'est une règle fondamentale, pas une pénalité). GADGET-2 reste, lui, en
> attente de ta validation de la conception et de tes décisions §12 (E, taille de comité,
> quorum).
