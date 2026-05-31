---
description: Working on domains, search, social features, moderation, forums, trust graph, or web publishing
globs: ["src-tauri/src/p2p/domains.rs", "src-tauri/src/p2p/search.rs", "src-tauri/src/p2p/social.rs", "src-tauri/src/p2p/moderation.rs", "src-tauri/src/p2p/forums.rs", "src-tauri/src/p2p/trust_graph.rs", "src-tauri/src/p2p/page_store.rs"]
---

# Skill: Social Web (V3)

## Module Map
| Module | Purpose | Gossip Message |
|--------|---------|----------------|
| domains.rs | .torus name registry (Harberger tax) | PublishDomain, PublishSubdomain |
| search.rs | BM25 + QuantaRank search index | PublishSite |
| social.rs | Likes (quadratic), follows, tips, boost | BroadcastSocialAction |
| moderation.rs | Reports, VRF jury, commit-reveal voting | BroadcastReport, JurorCommit, JurorReveal |
| forums.rs | Threads DAG + comments | PublishForumNode(kind) |
| trust_graph.rs | Personalized PageRank (Web of Trust) | (via follow state) |
| page_store.rs | P2P web pages + multi-page sites | PublishPage, PublishSiteManifest |

## All social actions require Ed25519 signature
```rust
pub struct SignedAction {
    pub author_pk: String,
    pub action: SocialAction,
    pub signature: String,
    pub timestamp: u64,
}
```

## Domain Registry (.torus)
- Harberger tax: domains have a self-assessed value
- Overbidding: anyone can take a domain by paying more
- INITIAL_CLAIM_MICRO_QTA = cost to register
- Subdomains can be delegated to other public keys

## Search (BM25 + Social Signals)
- TF-IDF tokenization at publish time
- BM25 ranking: k1=1.2, b=0.75
- QuantaRank = BM25 * social_boost (likes, follows, tips)

## Moderation (VRF Jury)
1. Report submitted → accumulates to threshold
2. Jury selected via VRF (BLAKE3 seed)
3. Commit phase: jurors submit sealed votes
4. Reveal phase: jurors reveal votes
5. Majority decides: dismiss or penalize
