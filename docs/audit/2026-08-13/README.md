# Audit externe du 13 août 2026 — les rapports d'origine

Ces fichiers sont le **constat brut**, publié tel qu'il a été rendu. Rien n'a été
retiré, atténué ni reformulé après coup : ni les treize constats critiques, ni les
passages où l'auditeur explique qu'un correctif précédent du projet était faux.

La raison de les publier est simple. La remédiation
([`../REMEDIATION-2026-08-13.md`](../REMEDIATION-2026-08-13.md)) affirme avoir fermé
85 constats. Sans le constat d'origine, cette phrase demande d'être crue sur parole,
et un lecteur ne peut ni vérifier qu'un correctif répond vraiment à ce qui a été
trouvé, ni juger la sévérité de ce qui reste ouvert.

| fichier | périmètre | ce qu'il contient |
|---|---|---|
| [`00-SYNTHESE.md`](00-SYNTHESE.md) | verdict général | les 13 critiques, le motif dominant, la hiérarchie des risques |
| [`01-CRYPTO.md`](01-CRYPTO.md) | primitives, vault, signatures | canonicalisation des préimages, dérivation de clés, Argon2id |
| [`02-CONSENSUS.md`](02-CONSENSUS.md) | ledger, élection, finalité | unicité des transactions, fork-choice, émission, slashing |
| [`03-RESEAU.md`](03-RESEAU.md) | gossip, dispatcher, rendez-vous | anti-rejeu, bans, bornes DoS, éclipse |
| [`04-APPLICATION.md`](04-APPLICATION.md) | Tauri, RPC, frontend | ACL des commandes, CSRF, CSP, diagnostics |
| [`05-SUPPLY-CHAIN.md`](05-SUPPLY-CHAIN.md) | CI, dépendances, release | actions épinglées, cargo-deny, clé de signature |
| [`poc-ban-C1.rs`](poc-ban-C1.rs) | preuve exécutable | le PoC de `C-1` — bannir n'importe quel nœud du réseau sans clé ni enjeu |

## Sur la publication du PoC

`poc-ban-C1.rs` exploite une faille **corrigée** (voir `R1` dans la remédiation), sur
un réseau qui n'a pas de pairs d'amorçage publics et ne porte aucune valeur. Le
publier ne met personne en danger et permet de vérifier que le correctif répond bien
à l'attaque décrite plutôt qu'à une reformulation commode de celle-ci.

## Ce que l'audit a trouvé de bien

Il serait malhonnête de ne citer que la partie accablante. Le rapport de synthèse
relève aussi ce qui tenait : la liaison intrinsèque adresse-clé, la récompense de
bloc recalculée par chaque nœud au lieu d'être crue, la couverture des dépenses
écrite une seule fois pour la production et la vérification, l'ordre du pipeline
gossip pensé pour ne rien écrire avant la signature, Argon2id au-dessus des
recommandations OWASP, et aucun `unwrap` dans le code de production.

Son verdict tient en une phrase, qui est aussi la plus utile du document :

> Le projet vérifie très bien ce qu'il a décidé de vérifier, et ne vérifie pas ce
> dont il n'a jamais écrit la règle.
