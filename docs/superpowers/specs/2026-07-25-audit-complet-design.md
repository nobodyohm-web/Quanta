# Audit interne complet — conception

**Date** : 2026-07-25 · **Version auditée** : v3.12.0 · **Branche** : `feat/frontend-rebuild-clean`
**Portée** : 32 673 lignes Rust (64 fichiers) + 9 634 lignes front (41 fichiers), documentation comprise.

## Intention

La demande est « voir si tout est parfaitement parfait ». Sur un projet de ce niveau de maturité —
449 tests, neuf ADR ratifiés, plusieurs vagues d'audit derrière lui, un dossier déjà préparé pour un
auditeur externe — cette phrase ne demande pas si le programme fonctionne. Elle demande ce qui reste
entre l'état actuel et l'irréprochable.

L'audit sépare donc trois natures de résultat, parce qu'elles ne se prouvent pas de la même façon.
Les **faits** sont ce que la machine répond : les tests passent-ils réellement, clippy est-il propre,
les huit vulnérabilités transitives relevées en v3.11 sont-elles toujours là. Les **défauts** sont
des chemins de code où un invariant casse, et ils exigent une preuve de lecture. Les **imperfections**
sont ce qui fonctionne mais reste en dessous du niveau visé : une constante documentée qui a divergé
du code, un nombre affiché qu'on ne peut pas rattacher à un état réel, une abstraction devenue morte.

L'audit constate. Il ne corrige rien. Les corrections relèvent du plan qui lui succède.

## Méthode

Le découpage retenu est **par axe technique** plutôt que par fichier ou par scénario d'attaque. Un
auditeur qui lit `ledger/mod.rs` sans `validation.rs` manque précisément les défauts de couverture ;
un auditeur qui ne pense qu'en scénarios de vol est aveugle à la qualité et au front. Chaque agent
reçoit donc un invariant à casser et l'ensemble des fichiers qui le portent.

Huit axes couvrent le programme. **Consensus et finalité** interroge le cœur pur `sm/` : correction
de la règle justify/finalize, arithmétique du quorum, complétude de la détection de surround-vote,
ancrage de LMD-GHOST au plancher, et surtout déterminisme — une `HashMap` égarée sur un chemin de
verdict suffit à forker deux nœuds honnêtes. **Ledger et économie** attaque l'invariant de
conservation `Σ(dépensable + staké + déverrouillage) + brûlé == miné` sur chacun de ses chemins de
mutation, y compris le slash sur unbonding et le reorg, et vérifie la construction du Merkle contre
la faille de duplication connue de Bitcoin. **Cryptographie et post-quantique** examine la séparation
des domaines, la canonicalisation des octets signés, la malléabilité de l'autorité multisig encodée
en JSON, la réutilisation de nonce AES-GCM et l'effectivité du zeroize. **Réseau et déni de service**
attaque l'ordre des neuf étapes du pipeline de dispatch — toute opération coûteuse ou mutante placée
avant la vérification de signature est une primitive d'amplification offerte — puis la bombe zip du
segment gzippé, le tampon borné du réconciliateur de fork et les croissances mémoire non plafonnées.
**Robustesse Rust** trie les 658 occurrences de `unwrap`/`expect`/`panic` pour n'en garder que les
atteignables, et traque les deux tueurs silencieux que rien ne vérifie mécaniquement ici : un `.await`
tenu sous un verrou, et deux chemins qui prennent la même paire de verrous en ordre inverse.
**Nœud et JSON-RPC** vise la surface la plus récente et la plus exposée : le mode `--public` est-il
gouverné par une liste blanche par méthode ou par un drapeau consulté au petit bonheur, l'analyseur
HTTP écrit à la main survit-il à un `Content-Length` menteur, l'explorateur échappe-t-il les chaînes
venues de la chaîne. **Front et vérité** applique la règle absolue du projet — aucun nombre fabriqué —
en remontant chaque chiffre affiché jusqu'à sa source, puis vérifie la complétude des six langues, la
couverture réelle du thème sombre et la libération du contexte WebGL. **Documentation, build et
dépendances** compare chaque constante documentée à la constante gravée, chaque affirmation de la
forme « X est fermé, X est vivant » au code correspondant, et le compte de tests annoncé au compte réel.

En parallèle, une **passe de faits exécutés** — `cargo test`, `cargo clippy --all-targets`,
`svelte-check`, `cargo audit`, `npm audit` — dont la sortie brute entre au rapport telle quelle.

## Le filtre adversarial

C'est ce qui sépare cet audit d'une liste de suspicions. Chaque constat remonté passe devant des
sceptiques indépendants dont la consigne est de le **réfuter** en relisant le code, avec « réfuté »
par défaut en cas de doute. Deux lentilles distinctes pour un constat critique ou haut — l'une
vérifie que l'auditeur n'a pas mal lu le flot de contrôle, l'autre cherche si la protection existe
déjà ailleurs, dans un validateur partagé, une étape antérieure du pipeline ou un const-assert.
Une seule lentille pour un constat moyen. Aucune pour le cosmétique, qui part en annexe explicitement
étiquetée comme non vérifiée. Un constat qui ne survit pas n'entre pas au rapport, si bien argumenté
soit-il — un faux positif plausible coûte plus cher qu'un défaut manqué, parce qu'il détruit la
confiance dans l'ensemble du document.

Chaque axe plafonne à ses six constats les plus graves et vingt constats au plus passent la
vérification. Tout ce qui tombe sous ces plafonds est **annoncé nommément** : un rapport tronqué qui
se lirait comme exhaustif serait pire que pas de rapport.

Un dernier agent ne cherche aucun défaut. Il nomme ce que personne n'a regardé — fichiers jamais
ouverts, classes de menace structurellement absentes de la liste des axes.

## Livrable

`docs/audit/AUDIT-INTERNE-2026-07-25.md` rassemble les constats survivants classés en critique, haut,
moyen, bas et imperfection. Chacun porte son `fichier:ligne`, un scénario d'échec concret, la preuve
de lecture et le correctif proposé. Le rapport porte aussi ce qui a été réfuté et pourquoi — cette
partie a autant de valeur que l'autre, puisqu'elle documente des protections réelles qu'un futur
auditeur croira absentes. Il se termine par un verdict global honnête, quel qu'il soit.

## Suite

Le plan de correction est écrit séparément, une fois le rapport lu. C'est là que se décide ce qui est
repris, dans quel ordre, et ce qui est sciemment laissé.
