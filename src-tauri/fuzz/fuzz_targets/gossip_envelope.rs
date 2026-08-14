#![no_main]

use libfuzzer_sys::fuzz_target;

// Le parseur d'enveloppes gossip est la porte d'entrée des octets **non fiables**
// venus du réseau. Quoi qu'envoie un pair malveillant — JSON tronqué, payloads
// géants, signatures malformées, UTF-8 invalide — ce chemin ne doit jamais faire
// autre chose que `Ok(())` ou `Err(_)` : jamais de panique, de débordement ni de
// boucle.
//
// **SC-06 (audit 2026-08-13)** — la cible n'entrait QUE par `fuzz_parse_gossip`,
// qui vérifie une signature ML-DSA-65. Aucune entrée produite par un fuzzer n'en
// porte une : **100 % des cas mouraient au mur d'authentification**, et la
// couverture réelle des parseurs était nulle. La porte existait, elle ne testait
// rien. On garde ce chemin (il éprouve le mur lui-même) et on ajoute celui qui
// commence **après** — c'est-à-dire ce qu'un pair authentifié, donc n'importe
// qui, peut faire avaler au nœud.
//
// Run with:
//   rustup toolchain install nightly      # once
//   cargo install cargo-fuzz              # once
//   cd src-tauri && cargo +nightly fuzz run gossip_envelope
fuzz_target!(|data: &[u8]| {
    // ① Le pipeline complet, mur d'authentification compris.
    let _ = quanta_lib::fuzz_parse_gossip(data);
    // ② Les parseurs tels qu'ils sont atteints APRÈS authentification :
    //    serde_json sur chaque variante, décodage hex, décompression gzip.
    let _ = quanta_lib::fuzz_parse_payload(data);
});
