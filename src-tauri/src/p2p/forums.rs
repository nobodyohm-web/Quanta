//! Torus V3 — Forums P2P : threads DAG + commentaires.
//!
//! Modèle :
//! - **Forum** : nœud racine (`name`, `description`, créateur).
//! - **Thread** : enfant d'un forum, avec titre + body. Body > 64 KB → DAG chunked.
//! - **Comment** : enfant d'un thread ou d'un autre comment (réponses imbriquées).
//!
//! Chaque nœud est signé par son auteur. L'ID est BLAKE3(canonical bytes).
//! Les votes/reports sur un nœud passent par `social.rs` / `moderation.rs`
//! en référençant le `node_id` comme `target_cid`.
//!
//! Soft-fork : un user peut publier un `Thread` qui pointe (`forked_from`)
//! vers un autre thread. Utile pour scinder une conversation qui dérive.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_TITLE_LEN: usize = 200;
pub const MAX_FORUM_NAME_LEN: usize = 60;
pub const MAX_BODY_INLINE_LEN: usize = 64_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forum {
    pub id: String,           // BLAKE3 du payload signé
    pub name: String,         // unique humain-friendly (lowercase, max 60)
    pub description: String,
    pub creator_pk: String,
    pub created_at: u64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub forum_id: String,
    pub title: String,
    /// Body inline si <= 64 KB, sinon CID DAG.
    pub body: String,
    pub body_is_dag_cid: bool,
    pub author_pk: String,
    pub created_at: u64,
    pub forked_from: Option<String>, // soft-fork
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub thread_id: String,
    pub parent_comment_id: Option<String>, // None = réponse directe au thread
    pub body: String,
    pub author_pk: String,
    pub created_at: u64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForumError {
    InvalidName,
    InvalidPayload,
    InvalidSignature,
    DuplicateId,
    UnknownForum,
    UnknownThread,
    UnknownParentComment,
    BodyTooLarge,
}

// ─── Signing helpers ────────────────────────────────────────────────────────

fn signable_forum(f: &Forum) -> Vec<u8> {
    format!("F|{}|{}|{}|{}", f.name, f.description, f.creator_pk, f.created_at).into_bytes()
}

fn signable_thread(t: &Thread) -> Vec<u8> {
    let forked = t.forked_from.clone().unwrap_or_default();
    format!(
        "T|{}|{}|{}|{}|{}|{}|{}",
        t.forum_id, t.title, t.body, t.body_is_dag_cid, t.author_pk, t.created_at, forked
    )
    .into_bytes()
}

fn signable_comment(c: &Comment) -> Vec<u8> {
    let parent = c.parent_comment_id.clone().unwrap_or_default();
    format!(
        "C|{}|{}|{}|{}|{}",
        c.thread_id, parent, c.body, c.author_pk, c.created_at
    )
    .into_bytes()
}

fn id_of(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

fn verify_sig(pk_hex: &str, sig_hex: &str, msg: &[u8]) -> Result<(), ForumError> {
    let pk_bytes = hex::decode(pk_hex).map_err(|_| ForumError::InvalidSignature)?;
    let sig_bytes = hex::decode(sig_hex).map_err(|_| ForumError::InvalidSignature)?;
    let pk_arr: [u8; 32] = pk_bytes.try_into().map_err(|_| ForumError::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| ForumError::InvalidSignature)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| ForumError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(msg, &sig).map_err(|_| ForumError::InvalidSignature)
}

pub fn build_forum(sk: &SigningKey, name: &str, description: &str, ts: u64) -> Result<Forum, ForumError> {
    let name = name.trim().to_lowercase();
    if name.is_empty() || name.len() > MAX_FORUM_NAME_LEN {
        return Err(ForumError::InvalidName);
    }
    let pk = hex::encode(sk.verifying_key().as_bytes());
    let mut f = Forum {
        id: String::new(),
        name,
        description: description.into(),
        creator_pk: pk,
        created_at: ts,
        signature: String::new(),
    };
    let bytes = signable_forum(&f);
    let sig = sk.sign(&bytes);
    f.signature = hex::encode(sig.to_bytes());
    f.id = id_of(&bytes);
    Ok(f)
}

pub fn build_thread(
    sk: &SigningKey,
    forum_id: &str,
    title: &str,
    body: &str,
    body_is_dag_cid: bool,
    forked_from: Option<String>,
    ts: u64,
) -> Result<Thread, ForumError> {
    if title.is_empty() || title.len() > MAX_TITLE_LEN {
        return Err(ForumError::InvalidPayload);
    }
    if !body_is_dag_cid && body.len() > MAX_BODY_INLINE_LEN {
        return Err(ForumError::BodyTooLarge);
    }
    let pk = hex::encode(sk.verifying_key().as_bytes());
    let mut t = Thread {
        id: String::new(),
        forum_id: forum_id.into(),
        title: title.into(),
        body: body.into(),
        body_is_dag_cid,
        author_pk: pk,
        created_at: ts,
        forked_from,
        signature: String::new(),
    };
    let bytes = signable_thread(&t);
    let sig = sk.sign(&bytes);
    t.signature = hex::encode(sig.to_bytes());
    t.id = id_of(&bytes);
    Ok(t)
}

pub fn build_comment(
    sk: &SigningKey,
    thread_id: &str,
    parent_comment_id: Option<String>,
    body: &str,
    ts: u64,
) -> Result<Comment, ForumError> {
    if body.is_empty() || body.len() > MAX_BODY_INLINE_LEN {
        return Err(ForumError::BodyTooLarge);
    }
    let pk = hex::encode(sk.verifying_key().as_bytes());
    let mut c = Comment {
        id: String::new(),
        thread_id: thread_id.into(),
        parent_comment_id,
        body: body.into(),
        author_pk: pk,
        created_at: ts,
        signature: String::new(),
    };
    let bytes = signable_comment(&c);
    let sig = sk.sign(&bytes);
    c.signature = hex::encode(sig.to_bytes());
    c.id = id_of(&bytes);
    Ok(c)
}

// ─── Engine ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForumsSnapshot {
    pub forums: Vec<Forum>,
    pub threads: Vec<Thread>,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Default, Clone)]
pub struct ForumsEngine {
    forums: HashMap<String, Forum>,
    forums_by_name: HashMap<String, String>,
    threads: HashMap<String, Thread>,
    comments: HashMap<String, Comment>,
    /// thread → ids des commentaires (pour parcours rapide)
    by_thread: HashMap<String, Vec<String>>,
    seen_ids: HashSet<String>,
}

impl ForumsEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> ForumsSnapshot {
        ForumsSnapshot {
            forums: self.forums.values().cloned().collect(),
            threads: self.threads.values().cloned().collect(),
            comments: self.comments.values().cloned().collect(),
        }
    }

    pub fn restore(snap: ForumsSnapshot) -> Self {
        let mut s = Self::default();
        for f in snap.forums {
            s.forums_by_name.insert(f.name.clone(), f.id.clone());
            s.seen_ids.insert(f.id.clone());
            s.forums.insert(f.id.clone(), f);
        }
        for t in snap.threads {
            s.seen_ids.insert(t.id.clone());
            s.threads.insert(t.id.clone(), t);
        }
        for c in snap.comments {
            s.by_thread
                .entry(c.thread_id.clone())
                .or_default()
                .push(c.id.clone());
            s.seen_ids.insert(c.id.clone());
            s.comments.insert(c.id.clone(), c);
        }
        s
    }

    pub fn forums(&self) -> impl Iterator<Item = &Forum> {
        self.forums.values()
    }
    pub fn threads_in(&self, forum_id: &str) -> Vec<&Thread> {
        self.threads
            .values()
            .filter(|t| t.forum_id == forum_id)
            .collect()
    }
    pub fn comments_of(&self, thread_id: &str) -> Vec<&Comment> {
        self.by_thread
            .get(thread_id)
            .map(|ids| ids.iter().filter_map(|i| self.comments.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn add_forum(&mut self, f: Forum) -> Result<(), ForumError> {
        if self.forums_by_name.contains_key(&f.name) {
            return Err(ForumError::DuplicateId);
        }
        if self.seen_ids.contains(&f.id) {
            return Err(ForumError::DuplicateId);
        }
        verify_sig(&f.creator_pk, &f.signature, &signable_forum(&f))?;
        let recomputed = id_of(&signable_forum(&f));
        if recomputed != f.id {
            return Err(ForumError::InvalidPayload);
        }
        self.forums_by_name.insert(f.name.clone(), f.id.clone());
        self.seen_ids.insert(f.id.clone());
        self.forums.insert(f.id.clone(), f);
        Ok(())
    }

    pub fn add_thread(&mut self, t: Thread) -> Result<(), ForumError> {
        if !self.forums.contains_key(&t.forum_id) {
            return Err(ForumError::UnknownForum);
        }
        if self.seen_ids.contains(&t.id) {
            return Err(ForumError::DuplicateId);
        }
        verify_sig(&t.author_pk, &t.signature, &signable_thread(&t))?;
        let recomputed = id_of(&signable_thread(&t));
        if recomputed != t.id {
            return Err(ForumError::InvalidPayload);
        }
        if let Some(forked) = &t.forked_from {
            if !self.threads.contains_key(forked) {
                return Err(ForumError::UnknownThread);
            }
        }
        self.seen_ids.insert(t.id.clone());
        self.threads.insert(t.id.clone(), t);
        Ok(())
    }

    pub fn add_comment(&mut self, c: Comment) -> Result<(), ForumError> {
        if !self.threads.contains_key(&c.thread_id) {
            return Err(ForumError::UnknownThread);
        }
        if let Some(p) = &c.parent_comment_id {
            if !self.comments.contains_key(p) {
                return Err(ForumError::UnknownParentComment);
            }
        }
        if self.seen_ids.contains(&c.id) {
            return Err(ForumError::DuplicateId);
        }
        verify_sig(&c.author_pk, &c.signature, &signable_comment(&c))?;
        let recomputed = id_of(&signable_comment(&c));
        if recomputed != c.id {
            return Err(ForumError::InvalidPayload);
        }
        self.by_thread
            .entry(c.thread_id.clone())
            .or_default()
            .push(c.id.clone());
        self.seen_ids.insert(c.id.clone());
        self.comments.insert(c.id.clone(), c);
        Ok(())
    }

    pub fn forum_by_name(&self, name: &str) -> Option<&Forum> {
        self.forums_by_name
            .get(name)
            .and_then(|id| self.forums.get(id))
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn sk(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn create_and_query_forum() {
        let mut e = ForumsEngine::new();
        let f = build_forum(&sk(1), "rust", "Rust talk", 1).unwrap();
        e.add_forum(f).unwrap();
        assert!(e.forum_by_name("rust").is_some());
    }

    #[test]
    fn duplicate_forum_name_rejected() {
        let mut e = ForumsEngine::new();
        let f1 = build_forum(&sk(1), "rust", "x", 1).unwrap();
        let f2 = build_forum(&sk(2), "rust", "y", 2).unwrap();
        e.add_forum(f1).unwrap();
        assert!(e.add_forum(f2).is_err());
    }

    #[test]
    fn add_thread_and_comment() {
        let mut e = ForumsEngine::new();
        let f = build_forum(&sk(1), "rust", "x", 1).unwrap();
        let fid = f.id.clone();
        e.add_forum(f).unwrap();
        let t = build_thread(&sk(2), &fid, "Title", "body", false, None, 2).unwrap();
        let tid = t.id.clone();
        e.add_thread(t).unwrap();
        let c = build_comment(&sk(3), &tid, None, "Hi", 3).unwrap();
        e.add_comment(c).unwrap();
        assert_eq!(e.comments_of(&tid).len(), 1);
    }

    #[test]
    fn nested_comment() {
        let mut e = ForumsEngine::new();
        let f = build_forum(&sk(1), "rust", "x", 1).unwrap();
        let fid = f.id.clone();
        e.add_forum(f).unwrap();
        let t = build_thread(&sk(2), &fid, "Title", "body", false, None, 2).unwrap();
        let tid = t.id.clone();
        e.add_thread(t).unwrap();
        let c1 = build_comment(&sk(3), &tid, None, "Top", 3).unwrap();
        let c1_id = c1.id.clone();
        e.add_comment(c1).unwrap();
        let c2 = build_comment(&sk(4), &tid, Some(c1_id.clone()), "reply", 4).unwrap();
        e.add_comment(c2).unwrap();
        assert_eq!(e.comments_of(&tid).len(), 2);
    }

    #[test]
    fn comment_under_unknown_parent_rejected() {
        let mut e = ForumsEngine::new();
        let f = build_forum(&sk(1), "rust", "x", 1).unwrap();
        let fid = f.id.clone();
        e.add_forum(f).unwrap();
        let t = build_thread(&sk(2), &fid, "Title", "body", false, None, 2).unwrap();
        let tid = t.id.clone();
        e.add_thread(t).unwrap();
        let c = build_comment(&sk(3), &tid, Some("ghost".into()), "Hi", 3).unwrap();
        assert_eq!(e.add_comment(c), Err(ForumError::UnknownParentComment));
    }

    #[test]
    fn forged_signature_rejected() {
        let mut e = ForumsEngine::new();
        let mut f = build_forum(&sk(1), "rust", "x", 1).unwrap();
        f.signature = "00".repeat(64);
        assert!(matches!(e.add_forum(f), Err(ForumError::InvalidSignature)));
    }

    #[test]
    fn body_too_large_rejected() {
        let big = "x".repeat(MAX_BODY_INLINE_LEN + 1);
        let res = build_thread(&sk(1), "fid", "title", &big, false, None, 1);
        assert!(matches!(res, Err(ForumError::BodyTooLarge)));
    }

    #[test]
    fn forked_thread_links_to_original() {
        let mut e = ForumsEngine::new();
        let f = build_forum(&sk(1), "rust", "x", 1).unwrap();
        let fid = f.id.clone();
        e.add_forum(f).unwrap();
        let t1 = build_thread(&sk(2), &fid, "Original", "body", false, None, 2).unwrap();
        let t1_id = t1.id.clone();
        e.add_thread(t1).unwrap();
        let t2 = build_thread(&sk(3), &fid, "Fork", "alt body", false, Some(t1_id.clone()), 3).unwrap();
        e.add_thread(t2).unwrap();
        let t2_stored = e
            .threads
            .values()
            .find(|t| t.forked_from.as_deref() == Some(&t1_id))
            .unwrap();
        assert_eq!(t2_stored.title, "Fork");
    }

    #[test]
    fn snapshot_round_trip() {
        let mut e = ForumsEngine::new();
        let f = build_forum(&sk(1), "rust", "x", 1).unwrap();
        let fid = f.id.clone();
        e.add_forum(f).unwrap();
        let t = build_thread(&sk(2), &fid, "Title", "body", false, None, 2).unwrap();
        let tid = t.id.clone();
        e.add_thread(t).unwrap();
        let c = build_comment(&sk(3), &tid, None, "Hi", 3).unwrap();
        e.add_comment(c).unwrap();
        let snap = e.snapshot();
        let e2 = ForumsEngine::restore(snap);
        assert_eq!(e2.threads_in(&fid).len(), 1);
        assert_eq!(e2.comments_of(&tid).len(), 1);
    }

    /// AUDIT-FORUM-1: full forum + thread + comment round-trip across two
    /// engines (mirrors gossip propagation). Each node MUST verify and accept
    /// on the receiver in the SAME order they were authored.
    #[test]
    fn audit_forum_round_trip_across_engines() {
        let mut source = ForumsEngine::new();
        let f = build_forum(&sk(1), "general", "all things", 100).unwrap();
        let fid = f.id.clone();
        source.add_forum(f.clone()).unwrap();
        let t = build_thread(&sk(2), &fid, "Hello", "world", false, None, 101).unwrap();
        let tid = t.id.clone();
        source.add_thread(t.clone()).unwrap();
        let c1 = build_comment(&sk(3), &tid, None, "first", 102).unwrap();
        let cid1 = c1.id.clone();
        source.add_comment(c1.clone()).unwrap();
        let c2 = build_comment(&sk(4), &tid, Some(cid1.clone()), "reply", 103).unwrap();
        source.add_comment(c2.clone()).unwrap();

        // Receiver receives the same nodes via gossip — must accept all.
        let mut receiver = ForumsEngine::new();
        assert!(receiver.add_forum(f.clone()).is_ok());
        assert!(receiver.add_thread(t.clone()).is_ok());
        assert!(receiver.add_comment(c1.clone()).is_ok());
        assert!(receiver.add_comment(c2.clone()).is_ok());

        // Replays must dedup gracefully.
        assert_eq!(receiver.add_forum(f), Err(ForumError::DuplicateId));
        assert_eq!(receiver.add_thread(t), Err(ForumError::DuplicateId));
        assert_eq!(receiver.add_comment(c1), Err(ForumError::DuplicateId));
        assert_eq!(receiver.add_comment(c2), Err(ForumError::DuplicateId));

        // Snapshot + restore preserves the entire hierarchy.
        let snap = receiver.snapshot();
        let restored = ForumsEngine::restore(snap);
        assert_eq!(restored.forums().count(), 1);
        assert_eq!(restored.threads_in(&fid).len(), 1);
        assert_eq!(restored.comments_of(&tid).len(), 2);
    }

    /// AUDIT-FORUM-2: out-of-order arrivals (comment before thread, or
    /// thread before forum) must be rejected gracefully so the user can
    /// retry — never silently swallowed.
    #[test]
    fn audit_forum_orphan_rejection_is_graceful() {
        let mut e = ForumsEngine::new();
        let t = build_thread(&sk(2), "ghost_forum", "Hello", "world", false, None, 1).unwrap();
        assert_eq!(e.add_thread(t), Err(ForumError::UnknownForum));
        let c = build_comment(&sk(3), "ghost_thread", None, "first", 1).unwrap();
        assert_eq!(e.add_comment(c), Err(ForumError::UnknownThread));
    }
}
