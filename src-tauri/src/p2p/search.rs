#![allow(dead_code)] // V3 module — types/utilitaires exposés pour DHT & tests externes
//! Torus V3 — Moteur de recherche P2P par mots-clés.
//!
//! Index inversé local : `token → Vec<PostingEntry>`. La pondération combine
//! TF-IDF + signaux sociaux (likes pondérés, abonnés, réputation, fraîcheur).
//!
//! Le sharding DHT (un pair stocke `hash(token) % N`) sera assemblé dans
//! `dispatcher.rs` ; ici on garde une logique pure et testable.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Apparitions max d'un même token dans une page (anti-spam SEO).
pub const MAX_TOKEN_REPEATS_PER_DOC: u32 = 5;

/// Demi-vie de fraîcheur (30 jours). Décroissance exponentielle.
pub const FRESHNESS_HALF_LIFE_SECS: f64 = 30.0 * 86_400.0;

/// Type de contenu indexé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocKind {
    Site,
    Blog,
    Forum,
    Comment,
    Shop,
}

impl DocKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DocKind::Site => "site",
            DocKind::Blog => "blog",
            DocKind::Forum => "forum",
            DocKind::Comment => "comment",
            DocKind::Shop => "shop",
        }
    }
}

/// Métadonnées sociales d'un document — fournies par `social.rs` au moment
/// du calcul du ranking. Pas stockées dans l'index pour rester réactives.
#[derive(Debug, Clone, Default)]
pub struct SocialSignals {
    /// Likes pondérés (somme √(montant_QTA)) reçus par ce doc.
    pub weighted_likes: f64,
    /// Abonnés du créateur.
    pub follower_count: u64,
    /// Score de réputation du créateur ∈ [0,1].
    pub creator_reputation: f64,
    /// Malus modération ∈ [0,1] (0 = ok, 1 = banni).
    pub moderation_malus: f64,
}

/// Une page indexée — résumé minimal pour le ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedDoc {
    /// Content ID BLAKE3.
    pub cid: String,
    /// Titre humain.
    pub title: String,
    /// Snippet pour l'affichage des résultats.
    pub snippet: String,
    pub author_pk: String,
    pub kind: DocKind,
    pub lang: String, // ex: "fr", "en"
    pub updated_at: u64,
    /// Tokens uniques + nb d'occurrences (cap MAX_TOKEN_REPEATS_PER_DOC).
    pub term_freq: HashMap<String, u32>,
    /// Domaine torus associé (optionnel). Améliore l'affichage.
    pub torus_domain: Option<String>,
}

/// Une entrée dans la liste de postings d'un terme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingEntry {
    pub cid: String,
    pub tf: u32,
}

/// Résultat de recherche enrichi (prêt pour l'UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub cid: String,
    pub title: String,
    pub snippet: String,
    pub author_pk: String,
    pub torus_domain: Option<String>,
    pub kind: DocKind,
    pub lang: String,
    pub updated_at: u64,
    pub score: f64,
}

/// Filtres optionnels d'une recherche.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
    pub lang: Option<String>,
    pub since_ts: Option<u64>,
    pub kind: Option<DocKind>,
    pub creator_pk: Option<String>,
    pub min_likes: Option<f64>,
}

// ─── Tokenizer ──────────────────────────────────────────────────────────────

/// Stop-words multilingues minimaux (FR/EN/ES). Liste volontairement courte
/// pour ne pas pénaliser des langues exotiques ; à étendre via DAG plus tard.
const STOP_WORDS: &[&str] = &[
    // FR
    "le", "la", "les", "de", "des", "du", "un", "une", "et", "ou", "à", "au", "aux", "ce", "ces",
    "cette", "que", "qui", "pour", "par", "sur", "dans", "en", "se", "sa", "son", "ses",
    "est", "sont", "pas", "ne", "plus", "avec",
    // EN
    "the", "a", "an", "of", "to", "in", "and", "or", "is", "are", "for", "on", "at", "by", "with",
    "this", "that", "as", "be", "it", "its", "from",
    // ES
    "el", "los", "las", "uno", "una", "y", "o", "para", "por", "con", "en", "del",
];

/// Tokenize : lowercase + split sur tout ce qui n'est pas alphanum + filter stop-words + len ≥ 2.
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && t.len() >= 2 && !STOP_WORDS.contains(t))
        .map(|t| t.to_string())
        .collect()
}

/// Comptage avec cap anti-spam (max 5 par token).
pub fn term_freq(tokens: &[String]) -> HashMap<String, u32> {
    let mut tf: HashMap<String, u32> = HashMap::new();
    for t in tokens {
        let e = tf.entry(t.clone()).or_insert(0);
        if *e < MAX_TOKEN_REPEATS_PER_DOC {
            *e += 1;
        }
    }
    tf
}

// ─── Index inversé ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexSnapshot {
    pub docs: Vec<IndexedDoc>,
}

#[derive(Debug, Default, Clone)]
pub struct SearchIndex {
    /// `cid → IndexedDoc`
    docs: HashMap<String, IndexedDoc>,
    /// `token → set(cid)` (postings ; tf récupéré via docs).
    inverted: HashMap<String, HashSet<String>>,
    /// `token → df` (nb de docs contenant ce terme), maintenu en cache.
    df: HashMap<String, u32>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }

    /// V3.3 — Récupère un doc indexé par son CID. Permet à `commands_v3::search_pages`
    /// d'enrichir le scoring avec les vrais signaux follower/reputation de l'auteur.
    pub fn doc_by_cid(&self, cid: &str) -> Option<&IndexedDoc> {
        self.docs.get(cid)
    }

    /// V3.3 — Liste les docs publiés par un ensemble d'auteurs (feed Subscriptions).
    /// Renvoie au plus `limit` documents, dans l'ordre interne (pas de tri).
    pub fn list_by_authors(
        &self,
        authors: &std::collections::HashSet<String>,
        limit: usize,
    ) -> Vec<IndexedDoc> {
        self.docs
            .values()
            .filter(|d| authors.contains(&d.author_pk))
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn snapshot(&self) -> SearchIndexSnapshot {
        SearchIndexSnapshot {
            docs: self.docs.values().cloned().collect(),
        }
    }

    pub fn restore(snap: SearchIndexSnapshot) -> Self {
        let mut s = Self::default();
        for d in snap.docs {
            s.upsert(d);
        }
        s
    }

    /// Ajoute/met à jour un document. Recalcule l'index inversé pour ce doc.
    pub fn upsert(&mut self, doc: IndexedDoc) {
        // Retirer les anciennes entrées si le doc existait
        if let Some(old) = self.docs.remove(&doc.cid) {
            for token in old.term_freq.keys() {
                if let Some(s) = self.inverted.get_mut(token) {
                    s.remove(&old.cid);
                    if s.is_empty() {
                        self.inverted.remove(token);
                    }
                }
                if let Some(d) = self.df.get_mut(token) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        self.df.remove(token);
                    }
                }
            }
        }
        // Insérer les nouvelles
        for token in doc.term_freq.keys() {
            self.inverted
                .entry(token.clone())
                .or_default()
                .insert(doc.cid.clone());
            *self.df.entry(token.clone()).or_insert(0) += 1;
        }
        self.docs.insert(doc.cid.clone(), doc);
    }

    /// Supprime un document.
    pub fn remove(&mut self, cid: &str) -> Option<IndexedDoc> {
        let doc = self.docs.remove(cid)?;
        for token in doc.term_freq.keys() {
            if let Some(s) = self.inverted.get_mut(token) {
                s.remove(cid);
                if s.is_empty() {
                    self.inverted.remove(token);
                }
            }
            if let Some(d) = self.df.get_mut(token) {
                *d = d.saturating_sub(1);
                if *d == 0 {
                    self.df.remove(token);
                }
            }
        }
        Some(doc)
    }

    /// IDF : log((N + 1) / (df + 1)) + 1 (smoothing).
    fn idf(&self, token: &str) -> f64 {
        let n = self.docs.len() as f64;
        let df = self.df.get(token).copied().unwrap_or(0) as f64;
        ((n + 1.0) / (df + 1.0)).ln() + 1.0
    }

    /// Score TF-IDF d'un document pour une liste de tokens query.
    fn tfidf(&self, doc: &IndexedDoc, q_tokens: &[String]) -> f64 {
        let mut score = 0.0;
        for t in q_tokens {
            if let Some(tf) = doc.term_freq.get(t) {
                score += (*tf as f64) * self.idf(t);
            }
        }
        score
    }

    /// Décroissance fraîcheur : 2^(-elapsed/half_life).
    fn freshness(&self, doc_ts: u64, now: u64) -> f64 {
        if now <= doc_ts {
            return 1.0;
        }
        let elapsed = (now - doc_ts) as f64;
        2f64.powf(-elapsed / FRESHNESS_HALF_LIFE_SECS)
    }

    /// Récupère les CIDs candidats pour une query (union des postings).
    fn candidates(&self, q_tokens: &[String]) -> HashSet<String> {
        let mut out = HashSet::new();
        for t in q_tokens {
            if let Some(set) = self.inverted.get(t) {
                for cid in set {
                    out.insert(cid.clone());
                }
            }
        }
        out
    }

    /// Recherche complète. `signals` est une fn fournissant les signaux
    /// sociaux par CID (le caller les obtient via `social.rs`/`moderation.rs`).
    pub fn search<F>(
        &self,
        query: &str,
        filters: &SearchFilters,
        now: u64,
        limit: usize,
        mut signals: F,
    ) -> Vec<SearchHit>
    where
        F: FnMut(&str) -> SocialSignals,
    {
        let q_tokens = tokenize(query);
        if q_tokens.is_empty() {
            return Vec::new();
        }
        let candidates = self.candidates(&q_tokens);

        let mut hits: Vec<SearchHit> = candidates
            .into_iter()
            .filter_map(|cid| self.docs.get(&cid))
            .filter(|d: &&IndexedDoc| {
                if let Some(lang) = &filters.lang {
                    if &d.lang != lang {
                        return false;
                    }
                }
                if let Some(since) = filters.since_ts {
                    if d.updated_at < since {
                        return false;
                    }
                }
                if let Some(k) = filters.kind {
                    if d.kind != k {
                        return false;
                    }
                }
                if let Some(creator) = &filters.creator_pk {
                    if &d.author_pk != creator {
                        return false;
                    }
                }
                true
            })
            .filter_map(|d| {
                let s = signals(&d.cid);
                if let Some(min_likes) = filters.min_likes {
                    if s.weighted_likes < min_likes {
                        return None;
                    }
                }
                let textual = self.tfidf(d, &q_tokens);
                if textual <= 0.0 {
                    return None;
                }
                let likes_factor = (1.0 + s.weighted_likes).ln().max(0.1);
                let followers_factor = ((1.0 + s.follower_count as f64).ln()).max(0.1);
                let rep_factor = s.creator_reputation.max(0.1).sqrt();
                let fresh = self.freshness(d.updated_at, now);
                let mod_factor = (1.0 - s.moderation_malus).max(0.0);
                if mod_factor <= 0.0 {
                    return None; // banni → exclu de l'index
                }
                let score = textual
                    * likes_factor
                    * followers_factor
                    * rep_factor
                    * fresh
                    * mod_factor;
                Some(SearchHit {
                    cid: d.cid.clone(),
                    title: d.title.clone(),
                    snippet: d.snippet.clone(),
                    author_pk: d.author_pk.clone(),
                    torus_domain: d.torus_domain.clone(),
                    kind: d.kind,
                    lang: d.lang.clone(),
                    updated_at: d.updated_at,
                    score,
                })
            })
            .collect();

        // Tri par score décroissant.
        // Tri stable : score décroissant, puis cid asc en tie-breaker (déterminisme).
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cid.cmp(&b.cid))
        });

        // Diversity bonus : pénaliser les hits ultérieurs du même auteur.
        let mut by_author: HashMap<String, u32> = HashMap::new();
        for h in &mut hits {
            let count = by_author.entry(h.author_pk.clone()).or_insert(0);
            if *count > 0 {
                let penalty = 0.7f64.powi(*count as i32);
                h.score *= penalty;
            }
            *count += 1;
        }
        // Re-trier après diversity.
        // Tri stable : score décroissant, puis cid asc en tie-breaker (déterminisme).
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cid.cmp(&b.cid))
        });
        hits.truncate(limit);
        hits
    }
}

// ─── Sharding DHT (utilitaire) ──────────────────────────────────────────────

/// Index de shard sur N pour un token donné, à partir des deux premiers octets BLAKE3.
pub fn shard_for_token(token: &str, n_shards: u16) -> u16 {
    if n_shards == 0 {
        return 0;
    }
    let h = blake3::hash(token.as_bytes());
    let bytes = h.as_bytes();
    let v = u16::from_be_bytes([bytes[0], bytes[1]]);
    v % n_shards
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(cid: &str, author: &str, title: &str, body: &str, ts: u64, lang: &str) -> IndexedDoc {
        let toks = tokenize(&format!("{title} {body}"));
        IndexedDoc {
            cid: cid.into(),
            title: title.into(),
            snippet: body.chars().take(120).collect(),
            author_pk: author.into(),
            kind: DocKind::Site,
            lang: lang.into(),
            updated_at: ts,
            term_freq: term_freq(&toks),
            torus_domain: None,
        }
    }

    #[test]
    fn tokenize_basic() {
        let t = tokenize("Bonjour le Monde, C'est génial !");
        assert!(t.contains(&"bonjour".to_string()));
        assert!(t.contains(&"monde".to_string()));
        assert!(t.contains(&"génial".to_string()));
        assert!(!t.contains(&"le".to_string())); // stopword
        assert!(!t.contains(&"c".to_string())); // trop court
    }

    #[test]
    fn tf_capped() {
        let toks: Vec<String> = (0..20).map(|_| "spam".to_string()).collect();
        let tf = term_freq(&toks);
        assert_eq!(tf.get("spam"), Some(&MAX_TOKEN_REPEATS_PER_DOC));
    }

    #[test]
    fn upsert_and_search_basic() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("cidA", "alice", "Cuisine vegan", "Recettes saines rapides", 100, "fr"));
        idx.upsert(doc("cidB", "bob", "Football", "But spectaculaire et défense", 100, "fr"));
        let hits = idx.search(
            "cuisine vegan",
            &SearchFilters::default(),
            200,
            10,
            |_| SocialSignals { creator_reputation: 1.0, ..Default::default() },
        );
        assert!(!hits.is_empty());
        assert_eq!(hits[0].cid, "cidA");
    }

    #[test]
    fn filter_by_lang() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("cidA", "alice", "Cuisine vegan", "saine", 100, "fr"));
        idx.upsert(doc("cidB", "bob", "Vegan cooking", "healthy", 100, "en"));
        let hits = idx.search(
            "vegan",
            &SearchFilters { lang: Some("en".into()), ..Default::default() },
            200,
            10,
            |_| SocialSignals { creator_reputation: 1.0, ..Default::default() },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].cid, "cidB");
    }

    #[test]
    fn empty_query_returns_nothing() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("cidA", "alice", "test", "test", 100, "fr"));
        let hits = idx.search("   ", &SearchFilters::default(), 200, 10, |_| SocialSignals::default());
        assert!(hits.is_empty());
    }

    #[test]
    fn social_signals_boost_ranking() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("popular", "alice", "Cuisine", "recettes", 100, "fr"));
        idx.upsert(doc("obscure", "bob", "Cuisine", "recettes", 100, "fr"));
        let hits = idx.search(
            "cuisine recettes",
            &SearchFilters::default(),
            200,
            10,
            |cid| {
                if cid == "popular" {
                    SocialSignals {
                        weighted_likes: 1000.0,
                        follower_count: 5000,
                        creator_reputation: 0.95,
                        moderation_malus: 0.0,
                    }
                } else {
                    SocialSignals {
                        weighted_likes: 1.0,
                        follower_count: 1,
                        creator_reputation: 0.5,
                        moderation_malus: 0.0,
                    }
                }
            },
        );
        assert_eq!(hits[0].cid, "popular");
    }

    #[test]
    fn moderation_malus_kills_score() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("banned", "evil", "Cuisine vegan", "x", 100, "fr"));
        idx.upsert(doc("ok", "alice", "Cuisine vegan", "x", 100, "fr"));
        let hits = idx.search(
            "cuisine vegan",
            &SearchFilters::default(),
            200,
            10,
            |cid| {
                if cid == "banned" {
                    SocialSignals { moderation_malus: 1.0, creator_reputation: 1.0, ..Default::default() }
                } else {
                    SocialSignals { creator_reputation: 1.0, ..Default::default() }
                }
            },
        );
        assert!(hits.iter().all(|h| h.cid != "banned"));
    }

    #[test]
    fn freshness_decays() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("fresh", "a", "test cuisine vegan", "x", 1_000_000, "fr"));
        idx.upsert(doc("old",   "b", "test cuisine vegan", "x", 0, "fr"));
        let hits = idx.search(
            "cuisine vegan",
            &SearchFilters::default(),
            1_000_000,
            10,
            |_| SocialSignals { creator_reputation: 1.0, ..Default::default() },
        );
        assert_eq!(hits[0].cid, "fresh");
    }

    #[test]
    fn diversity_penalizes_same_author() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("a1", "alice", "cuisine", "recette", 100, "fr"));
        idx.upsert(doc("a2", "alice", "cuisine", "recette", 100, "fr"));
        idx.upsert(doc("b1", "bob",   "cuisine", "recette", 100, "fr"));
        let hits = idx.search(
            "cuisine recette",
            &SearchFilters::default(),
            200,
            10,
            |_| SocialSignals { creator_reputation: 1.0, ..Default::default() },
        );
        // Bob doit apparaître avant le 2e doc d'Alice (penalty 0.7)
        let pos_b1 = hits.iter().position(|h| h.cid == "b1").unwrap();
        let pos_a2 = hits.iter().position(|h| h.cid == "a2").unwrap();
        assert!(pos_b1 < pos_a2);
    }

    #[test]
    fn remove_doc_removes_postings() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("cidA", "alice", "vegan cuisine", "recettes", 100, "fr"));
        assert!(idx.remove("cidA").is_some());
        let hits = idx.search("vegan", &SearchFilters::default(), 200, 10, |_| SocialSignals::default());
        assert!(hits.is_empty());
    }

    #[test]
    fn shard_distribution_is_deterministic() {
        let s1 = shard_for_token("cuisine", 64);
        let s2 = shard_for_token("cuisine", 64);
        assert_eq!(s1, s2);
        assert!(s1 < 64);
    }

    #[test]
    fn snapshot_round_trip() {
        let mut idx = SearchIndex::new();
        idx.upsert(doc("cidA", "alice", "vegan cuisine", "recettes", 100, "fr"));
        let snap = idx.snapshot();
        let idx2 = SearchIndex::restore(snap);
        assert_eq!(idx2.doc_count(), 1);
    }
}
