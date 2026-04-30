Effectue le refactoring V2 du système économique SOVA :

1. Lis les specs complètes :
   - .agent/design/sova_v2_vision.md
   - .agent/design/sova_v2_innovations.md
   - .agent/design/tech_references.md (section Shapley + BME)

2. Vérifie l'état actuel :
   - Quel modèle d'émission est implémenté ? (halving vs fixe)
   - Le Shapley Value est-il implémenté ?
   - Le burn rate est-il actif ?

3. Implémente la prochaine étape non-faite dans cette liste :
   - [ ] Émission fixe 100 SOVA/h (remplacer halving dans reputation.rs)
   - [ ] Distribution proportionnelle aux watts (remplacer uptime_tick)
   - [ ] Shapley Value basique (énergie + uptime)
   - [ ] Burn rate 1% sur transferts (modifier ledger.rs)
   - [ ] Total network watts via CRDT GCounter
   - [ ] Gossip des watts mesurés (Hello message)

4. Exécute la boucle de vérification complète
5. Commit avec message descriptif
