# Sécurité — Proof-of-Contribution témoignée (anti-Sybil v2)

> Objectif : que le minage soit **réel et impossible à truquer**. Statut : **conception
> — en attente du feu vert avant implémentation** (touche la distribution des récompenses,
> cœur « irréversible » du CLAUDE.md).

## Le trou
`reputation::uptime_tick` distribue l'émission d'un tick par **valeur de Shapley** sur
l'ensemble des `peer_contribs`. Or chaque contribution de pair provient de son **Hello
gossip**, où les **watts sont auto-déclarés**. Conséquence :

- ✅ Le **plafond dur** (100M, `validate_block_emission`) borne le **total** : un Sybil ne
  peut **pas créer** de monnaie au-delà du plafond.
- ❌ Mais un attaquant peut lancer N fausses identités déclarant des watts élevés et **voler
  une part** de l'émission de chaque tick — dilution des mineurs honnêtes. C'est un trou de
  **fairness/anti-Sybil**, pas de finalité.

## Principe de la correction : ne pondérer que ce qui est **observé**, pas déclaré
On remplace la confiance dans le watts auto-déclaré par des **signaux que CE nœud a lui-même
observés** pour chaque pair — non forgeables gratuitement (il faut vraiment relayer des
données et rester en ligne *envers moi* pour compter) :

| Signal observé localement | Déjà suivi ? | Ce qu'il prouve |
|---|---|---|
| `messages_in` / `bytes_in` du pair | ✅ NET-9 | le pair m'a réellement servi/relayé des données |
| `uptime_secs` observé (durée où je l'ai vu vivant) | ✅ NET-9 | présence réelle, pas un ghost |
| `quality_score` (latence/perte/uptime) | ✅ NET-10 | lien réel et utile |

## Plan incrémental (3 étages, du plus sûr au plus avancé)

### Phase 2b-1 — Pondération par activité observée (SÛR, testable maintenant)
Dans la distribution Shapley, remplacer le poids brut `watts` d'un pair par
`effective = clamp(self_report, 0, k · observed_activity(pair))`, où `observed_activity`
dérive de `messages_in`/`uptime_secs`/`quality_score` **que j'ai mesurés**. Effet :
- un pair honnête et actif garde sa part ;
- un ghost / sur-déclarant voit sa part **plafonnée à ce qu'il m'a réellement apporté**.
- Conservateur, additif, **0 changement de protocole/consensus**. Tests de régression :
  un Sybil inactif déclarant 10 000 W obtient ≈ la part d'un nœud inactif.

### Phase 2b-2 — Reçus de contribution signés (témoignage croisé)
Les pairs émettent des **reçus signés** (« j'ai reçu le bloc X / la page Y de toi »). On
agrège ces témoignages (quorum + pondération par le **web-of-trust** existant `trust_graph`)
→ contribution témoignée robuste à la collusion (borne d'influence par témoin).

### Phase 3 — Élection de leader non-grindable (VDF)
Découplé : ajouter une **fonction à délai vérifiable** sur l'aléa d'élection PoS pour qu'un
leader ne puisse pas « grinder » plusieurs tirages. (Le beacon enterré actuel aide déjà ;
le VDF ferme le reste.)

## Pourquoi prudemment
Changer « qui gagne » est sensible et difficilement réversible une fois en réseau. La Phase
2b-1 est volontairement **conservatrice** (ne fait que **plafonner le non-vérifiable**, ne
retire jamais la part d'un nœud honnête actif) et **entièrement testée** avant diffusion.

## Sources (recherche antérieure)
Helium (Proof-of-Coverage témoignée par les pairs), Filecoin (PoRep/PoSt), Chia (VDF),
littérature anti-Sybil (coût de ressource réel + témoignage croisé).
