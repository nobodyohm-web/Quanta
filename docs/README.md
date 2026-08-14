# Documentation Quanta

Quanta est une monnaie pair-à-pair post-quantique : une application de bureau et un
nœud headless, en Rust et Svelte. Cette page existe pour qu'on trouve le bon
document en moins d'une minute.

## Commencer ici

| Vous voulez… | Lisez |
|---|---|
| comprendre le protocole de bout en bout | [`ARCHITECTURE.md`](ARCHITECTURE.md) — la visite guidée, écrite pour être lue d'un trait *(en anglais)* |
| comprendre le post-quantique, sans le connaître | [`POST-QUANTUM.md`](POST-QUANTUM.md) — ce que Shor casse, pourquoi c'est urgent sans machine quantique, et le prix en octets |
| lire la thèse du projet | [`../WHITEPAPER_FR.md`](../WHITEPAPER_FR.md) · [`../WHITEPAPER.md`](../WHITEPAPER.md) *(EN)* |
| faire tourner un nœud | [`ops/RUN-WITH-A-FRIEND.md`](ops/RUN-WITH-A-FRIEND.md) — écrit pour quelqu'un qui n'a pas écrit ce code |

## Sécurité

Le dépôt publie l'audit qui l'a démoli, pas seulement le récit de sa réparation.

- [`audit/2026-08-13/`](audit/2026-08-13/) — **les rapports d'audit externe d'origine**,
  publiés tels quels : 85 constats dont 13 critiques, avec le PoC exécutable de l'un d'eux.
- [`audit/REMEDIATION-2026-08-13.md`](audit/REMEDIATION-2026-08-13.md) — ce qui a été
  corrigé, comment chaque correctif a été prouvé, et **ce qui reste ouvert avec la raison**.
- [`audit/THREAT-MODEL.md`](audit/THREAT-MODEL.md) — l'adversaire qu'on prétend arrêter,
  et celui qu'on ne prétend pas arrêter.
- [`audit/SCOPE.md`](audit/SCOPE.md) · [`audit/RFQ.md`](audit/RFQ.md) — le dossier préparé
  pour un audit indépendant, pas encore commandé.
- [`../SECURITY.md`](../SECURITY.md) — divulgation de vulnérabilités.

## Décisions d'architecture

Chaque décision structurante est écrite avec l'alternative qui a été écartée et
pourquoi. Une ADR renversée reste dans le registre, marquée comme telle.

- [`decisions/`](decisions/) — le registre (fork-choice, comité BFT, slashing, aléa
  d'élection, agrégation des votes, gouvernance, portée du post-quantique).

## Protocole

- [`protocol/FORK-RANK.md`](protocol/FORK-RANK.md) — le départage des forks par rang
  d'élection, et pourquoi il n'y a pas de VRF.
- [`protocol/FINALITY-GADGET.md`](protocol/FINALITY-GADGET.md) — le gadget de finalité
  Casper-FFG : votes, certificats, accountable safety.
- [`protocol/LIVE-WIRING.md`](protocol/LIVE-WIRING.md) — comment le gadget est câblé au
  réseau vivant.
- [`protocol/CONSENSUS-DAG-BFT.md`](protocol/CONSENSUS-DAG-BFT.md) — la piste envisagée
  pour une finalité sous-seconde. Conception, pas implémentation.

## Économie

- [`economy/DOCTRINE.md`](economy/DOCTRINE.md) — la doctrine monétaire, y compris les
  mécanismes envisagés puis refusés.
- [`economy/LISTING-READINESS.md`](economy/LISTING-READINESS.md) — ce qu'il faudrait
  avant qu'un marché existe. Rien de tout cela n'est fait.

## Marque

- [`brand/BRAND.md`](brand/BRAND.md) — identité visuelle et vocabulaire.

## Archive

- [`archive/`](archive/) — audits antérieurs, journaux de travail, notes de conception
  supersédées. **Rien de ce répertoire ne décrit le protocole courant.**
