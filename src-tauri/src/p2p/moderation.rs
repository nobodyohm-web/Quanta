#![allow(dead_code)] // V3 module — constantes & helpers exposés pour tests externes & V3.3
//! Torus V3 — Modération décentralisée style Kleros.
//!
//! Pipeline :
//! 1. `Report` signé (coût 0,1 QTA anti-spam)
//! 2. Quand ≥5 reports indépendants → ouverture d'un dossier
//! 3. Sélection pseudo-aléatoire de N jurés (BLAKE3 PRNG depuis seed publique)
//! 4. Phase commit (24 h) : chaque juré commit `H(verdict || nonce)`
//! 5. Phase reveal (24 h) : chaque juré révèle (verdict, nonce). Vote validé ssi `H == commit`
//! 6. Verdict = majorité simple. Récompense jurés gagnants. Slashing créateur si coupable.
//!
//! La sélection VRF "complète" (`schnorrkel`) sera ajoutée plus tard ; ici on
//! utilise un tirage déterministe vérifiable depuis un seed observable on-chain
//! (block hash + dossier id). C'est suffisant tant que le seed est imprévisible
//! au moment des reports.
//!
//! Anti-troll : voir `apply_anti_troll_malus` (utilisé par `reputation.rs`).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ─── Constantes ─────────────────────────────────────────────────────────────

pub const REPORT_COST_MICRO_QTA: u64 = 100_000;
pub const JURY_SIZE: usize = 7;
pub const JURY_STAKE_MIN_MICRO_QTA: u64 = 100_000_000;
pub const JURY_REWARD_MICRO_QTA: u64 = 500_000;
pub const REPORTS_TO_OPEN_CASE: usize = 5;
pub const COMMIT_PHASE_SECS: u64 = 24 * 3600;
pub const REVEAL_PHASE_SECS: u64 = 24 * 3600;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReportCategory {
    Spam,
    Scam,
    IllegalContent,
    Harassment,
    Impersonation,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verdict {
    Innocent,
    Warning,
    Hide,
    Ban,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CasePhase {
    Open,        // accumulant des reports
    Commit,      // les jurés commit (24h)
    Reveal,      // les jurés reveal (24h)
    Decided,     // verdict prononcé
    Appealed,    // appel en cours
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub target_cid: String,
    pub target_author_pk: String,
    pub category: ReportCategory,
    pub evidence_cid: Option<String>,
    pub reporter_pk: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitVote {
    pub case_id: String,
    pub juror_pk: String,
    pub commit_hash: String, // hex BLAKE3(verdict_byte || nonce_hex)
    pub timestamp: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealVote {
    pub case_id: String,
    pub juror_pk: String,
    pub verdict: Verdict,
    pub reveal_nonce: String, // hex 32 bytes
    pub timestamp: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationCase {
    pub id: String, // BLAKE3(target_cid || open_seed)
    pub target_cid: String,
    pub target_author_pk: String,
    pub reports: Vec<Report>,
    pub jurors: Vec<String>, // pks
    pub commits: HashMap<String, CommitVote>,
    pub reveals: HashMap<String, RevealVote>,
    pub verdict: Option<Verdict>,
    pub phase: CasePhase,
    pub opened_at: u64,
    pub commit_deadline: u64,
    pub reveal_deadline: u64,
    /// Seed utilisé pour la sélection juré (block_hash || id). Auditable.
    pub selection_seed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationError {
    InvalidSignature,
    DuplicateReport,
    DuplicateCommit,
    DuplicateReveal,
    UnknownCase,
    NotJuror,
    PhaseMismatch,
    CommitMismatch,
    NotEnoughEligibleJurors,
}

// ─── Signatures ─────────────────────────────────────────────────────────────

pub fn signable_report(r: &Report) -> Vec<u8> {
    let evidence = r.evidence_cid.clone().unwrap_or_default();
    format!(
        "REP|{}|{}|{:?}|{}|{}|{}|{}",
        r.target_cid,
        r.target_author_pk,
        r.category,
        evidence,
        r.reporter_pk,
        r.timestamp,
        r.nonce
    )
    .into_bytes()
}

pub fn signable_commit(c: &CommitVote) -> Vec<u8> {
    format!("COM|{}|{}|{}|{}", c.case_id, c.juror_pk, c.commit_hash, c.timestamp).into_bytes()
}

pub fn signable_reveal(r: &RevealVote) -> Vec<u8> {
    format!(
        "REV|{}|{}|{:?}|{}|{}",
        r.case_id, r.juror_pk, r.verdict, r.reveal_nonce, r.timestamp
    )
    .into_bytes()
}

fn verify_sig(pk_hex: &str, sig_hex: &str, msg: &[u8]) -> Result<(), ModerationError> {
    let pk_bytes = hex::decode(pk_hex).map_err(|_| ModerationError::InvalidSignature)?;
    let sig_bytes = hex::decode(sig_hex).map_err(|_| ModerationError::InvalidSignature)?;
    let pk_arr: [u8; 32] = pk_bytes.try_into().map_err(|_| ModerationError::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| ModerationError::InvalidSignature)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| ModerationError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(msg, &sig).map_err(|_| ModerationError::InvalidSignature)
}

pub fn sign_report(sk: &SigningKey, r: &mut Report) {
    r.reporter_pk = hex::encode(sk.verifying_key().as_bytes());
    let sig = sk.sign(&signable_report(r));
    r.signature = hex::encode(sig.to_bytes());
}

pub fn sign_commit(sk: &SigningKey, c: &mut CommitVote) {
    c.juror_pk = hex::encode(sk.verifying_key().as_bytes());
    let sig = sk.sign(&signable_commit(c));
    c.signature = hex::encode(sig.to_bytes());
}

pub fn sign_reveal(sk: &SigningKey, r: &mut RevealVote) {
    r.juror_pk = hex::encode(sk.verifying_key().as_bytes());
    let sig = sk.sign(&signable_reveal(r));
    r.signature = hex::encode(sig.to_bytes());
}

/// Helper : build le commit hash à partir du verdict + nonce hex.
pub fn build_commit_hash(verdict: Verdict, reveal_nonce_hex: &str) -> String {
    let v = verdict_byte(verdict);
    let mut buf = vec![v];
    buf.extend(reveal_nonce_hex.as_bytes());
    hex::encode(blake3::hash(&buf).as_bytes())
}

fn verdict_byte(v: Verdict) -> u8 {
    match v {
        Verdict::Innocent => 0,
        Verdict::Warning => 1,
        Verdict::Hide => 2,
        Verdict::Ban => 3,
    }
}

// ─── Sélection juré (BLAKE3 PRNG) ───────────────────────────────────────────

/// Tire `n` jurés uniques depuis `pool` en utilisant un seed pseudo-aléatoire
/// public (BLAKE3-based). Déterministe pour audit.
pub fn select_jurors(pool: &[String], seed: &str, n: usize) -> Vec<String> {
    if pool.len() <= n {
        return pool.to_vec();
    }
    let mut chosen: Vec<String> = Vec::with_capacity(n);
    let mut taken: HashSet<usize> = HashSet::new();
    let mut counter: u64 = 0;
    while chosen.len() < n {
        let mut buf = seed.as_bytes().to_vec();
        buf.extend(counter.to_be_bytes());
        let h = blake3::hash(&buf);
        let bytes = h.as_bytes();
        let v = u64::from_be_bytes(bytes[..8].try_into().unwrap_or([0u8; 8]));
        let idx = (v as usize) % pool.len();
        counter += 1;
        if taken.insert(idx) {
            chosen.push(pool[idx].clone());
        }
    }
    chosen
}

// ─── Engine ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModerationSnapshot {
    pub cases: Vec<ModerationCase>,
    pub seen_report_ids: Vec<String>,
    /// Compteur reports validés (verdict ≠ Innocent) sur 30j par auteur — pour anti-troll.
    pub validated_reports_30d: HashMap<String, Vec<u64>>,
    /// AUDIT-MOD-1: reports queued under the open-case threshold. Without
    /// persisting these, a node restart erased every accumulating report
    /// below 5 — users had to re-report from scratch and could miss the
    /// threshold purely because of an untimely restart. `#[serde(default)]`
    /// keeps backward compat with snapshots taken before this field existed.
    #[serde(default)]
    pub pending_reports: HashMap<String, Vec<Report>>,
}

#[derive(Debug, Default, Clone)]
pub struct ModerationEngine {
    cases: HashMap<String, ModerationCase>,
    /// Reports en attente d'ouverture par target_cid (clé).
    pending_reports: HashMap<String, Vec<Report>>,
    seen_report_ids: HashSet<String>,
    /// Auteur → timestamps de reports validés.
    validated_reports_30d: HashMap<String, Vec<u64>>,
}

impl ModerationEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> ModerationSnapshot {
        ModerationSnapshot {
            cases: self.cases.values().cloned().collect(),
            seen_report_ids: self.seen_report_ids.iter().cloned().collect(),
            validated_reports_30d: self.validated_reports_30d.clone(),
            // AUDIT-MOD-1: persist pending reports across restart.
            pending_reports: self.pending_reports.clone(),
        }
    }

    pub fn restore(snap: ModerationSnapshot) -> Self {
        let mut s = Self::default();
        for c in snap.cases {
            s.cases.insert(c.id.clone(), c);
        }
        s.seen_report_ids = snap.seen_report_ids.into_iter().collect();
        s.validated_reports_30d = snap.validated_reports_30d;
        s.pending_reports = snap.pending_reports;
        s
    }

    pub fn case(&self, id: &str) -> Option<&ModerationCase> {
        self.cases.get(id)
    }
    pub fn open_cases(&self) -> impl Iterator<Item = &ModerationCase> {
        self.cases
            .values()
            .filter(|c| c.phase != CasePhase::Decided)
    }

    fn report_id(r: &Report) -> String {
        let mut buf = signable_report(r);
        buf.extend(r.signature.as_bytes());
        hex::encode(blake3::hash(&buf).as_bytes())
    }

    /// Retourne `Some(case_id)` si l'ouverture d'un dossier est déclenchée.
    pub fn submit_report<F>(
        &mut self,
        report: Report,
        eligible_jurors: F,
        seed_block_hash: &str,
        now: u64,
    ) -> Result<Option<String>, ModerationError>
    where
        F: FnOnce() -> Vec<String>,
    {
        verify_sig(
            &report.reporter_pk,
            &report.signature,
            &signable_report(&report),
        )?;
        let id = Self::report_id(&report);
        if !self.seen_report_ids.insert(id) {
            return Err(ModerationError::DuplicateReport);
        }
        let key = report.target_cid.clone();
        let entry = self.pending_reports.entry(key.clone()).or_default();
        // Indépendance : un même reporter_pk ne compte qu'une fois.
        if entry.iter().any(|r| r.reporter_pk == report.reporter_pk) {
            return Err(ModerationError::DuplicateReport);
        }
        entry.push(report.clone());
        if entry.len() >= REPORTS_TO_OPEN_CASE {
            let pool = eligible_jurors();
            if pool.len() < JURY_SIZE {
                // pas assez de jurés éligibles : on garde les reports en attente
                return Err(ModerationError::NotEnoughEligibleJurors);
            }
            let case_seed = format!("{seed_block_hash}|{}", report.target_cid);
            let case_id = hex::encode(blake3::hash(case_seed.as_bytes()).as_bytes());
            let jurors = select_jurors(&pool, &case_seed, JURY_SIZE);
            let case = ModerationCase {
                id: case_id.clone(),
                target_cid: report.target_cid.clone(),
                target_author_pk: report.target_author_pk.clone(),
                reports: self.pending_reports.remove(&key).unwrap_or_default(),
                jurors,
                commits: HashMap::new(),
                reveals: HashMap::new(),
                verdict: None,
                phase: CasePhase::Commit,
                opened_at: now,
                commit_deadline: now + COMMIT_PHASE_SECS,
                reveal_deadline: now + COMMIT_PHASE_SECS + REVEAL_PHASE_SECS,
                selection_seed: case_seed,
            };
            self.cases.insert(case_id.clone(), case);
            return Ok(Some(case_id));
        }
        Ok(None)
    }

    pub fn submit_commit(
        &mut self,
        commit: CommitVote,
        now: u64,
    ) -> Result<(), ModerationError> {
        let case = self
            .cases
            .get_mut(&commit.case_id)
            .ok_or(ModerationError::UnknownCase)?;
        if case.phase != CasePhase::Commit {
            return Err(ModerationError::PhaseMismatch);
        }
        if now > case.commit_deadline {
            return Err(ModerationError::PhaseMismatch);
        }
        if !case.jurors.iter().any(|j| j == &commit.juror_pk) {
            return Err(ModerationError::NotJuror);
        }
        verify_sig(&commit.juror_pk, &commit.signature, &signable_commit(&commit))?;
        if case.commits.contains_key(&commit.juror_pk) {
            return Err(ModerationError::DuplicateCommit);
        }
        case.commits.insert(commit.juror_pk.clone(), commit);
        Ok(())
    }

    pub fn submit_reveal(
        &mut self,
        reveal: RevealVote,
        now: u64,
    ) -> Result<(), ModerationError> {
        let case = self
            .cases
            .get_mut(&reveal.case_id)
            .ok_or(ModerationError::UnknownCase)?;
        // Auto-transition Commit → Reveal si commit_deadline passée
        if case.phase == CasePhase::Commit && now > case.commit_deadline {
            case.phase = CasePhase::Reveal;
        }
        if case.phase != CasePhase::Reveal {
            return Err(ModerationError::PhaseMismatch);
        }
        if now > case.reveal_deadline {
            return Err(ModerationError::PhaseMismatch);
        }
        if !case.jurors.iter().any(|j| j == &reveal.juror_pk) {
            return Err(ModerationError::NotJuror);
        }
        verify_sig(&reveal.juror_pk, &reveal.signature, &signable_reveal(&reveal))?;
        let commit = case
            .commits
            .get(&reveal.juror_pk)
            .ok_or(ModerationError::PhaseMismatch)?;
        let expected = build_commit_hash(reveal.verdict, &reveal.reveal_nonce);
        if expected != commit.commit_hash {
            return Err(ModerationError::CommitMismatch);
        }
        if case.reveals.contains_key(&reveal.juror_pk) {
            return Err(ModerationError::DuplicateReveal);
        }
        case.reveals.insert(reveal.juror_pk.clone(), reveal);
        Ok(())
    }

    /// Décide le dossier (à appeler après reveal_deadline). Retourne le verdict.
    pub fn finalize(&mut self, case_id: &str, now: u64) -> Result<Verdict, ModerationError> {
        let case = self
            .cases
            .get_mut(case_id)
            .ok_or(ModerationError::UnknownCase)?;
        if case.phase == CasePhase::Decided {
            return case.verdict.ok_or(ModerationError::PhaseMismatch);
        }
        if now <= case.reveal_deadline {
            return Err(ModerationError::PhaseMismatch);
        }
        let mut tally: HashMap<Verdict, u64> = HashMap::new();
        for r in case.reveals.values() {
            *tally.entry(r.verdict).or_insert(0) += 1;
        }
        // Si quorum < majorité simple des jurés, on déclare Innocent (bénéfice du doute)
        let quorum_min = case.jurors.len() / 2 + 1;
        let total_revealed: u64 = tally.values().sum();
        let verdict = if total_revealed < quorum_min as u64 {
            Verdict::Innocent
        } else {
            *tally.iter().max_by_key(|(_, n)| *n).map(|(v, _)| v).unwrap_or(&Verdict::Innocent)
        };
        case.verdict = Some(verdict);
        case.phase = CasePhase::Decided;
        if verdict != Verdict::Innocent {
            self.validated_reports_30d
                .entry(case.target_author_pk.clone())
                .or_default()
                .push(now);
        }
        Ok(verdict)
    }

    /// Liste des jurés majoritaires (gagnent la récompense) pour un dossier décidé.
    pub fn winning_jurors(&self, case_id: &str) -> Vec<String> {
        let Some(case) = self.cases.get(case_id) else {
            return vec![];
        };
        let Some(verdict) = case.verdict else {
            return vec![];
        };
        case.reveals
            .values()
            .filter(|r| r.verdict == verdict)
            .map(|r| r.juror_pk.clone())
            .collect()
    }

    /// Compteur de reports validés sur 30j (pour anti-troll mining malus).
    pub fn validated_count_30d(&self, author_pk: &str, now: u64) -> usize {
        let cutoff = now.saturating_sub(30 * 86_400);
        self.validated_reports_30d
            .get(author_pk)
            .map(|v| v.iter().filter(|t| **t >= cutoff).count())
            .unwrap_or(0)
    }
}

/// Anti-troll graduel : convertit un compteur de reports en facteur de mining.
/// Renvoie `1.0` (mining plein) → `0.0` (mining off).
pub fn anti_troll_mining_factor(validated_reports_30d: usize) -> f64 {
    match validated_reports_30d {
        0 => 1.0,
        1..=2 => 1.0,    // warning, pas de malus matériel
        3..=4 => 0.90,
        5..=7 => 0.75,
        8..=11 => 0.50,
        _ => 0.0,
    }
}

/// % de slashing balance appliqué pour Hide/Ban (en bps).
pub fn slash_bps_for_verdict(v: Verdict) -> u64 {
    match v {
        Verdict::Innocent | Verdict::Warning => 0,
        Verdict::Hide => 0,    // pas de slashing balance, juste mining malus
        Verdict::Ban => 1_000, // 10%
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
    fn pk_of(s: &SigningKey) -> String {
        hex::encode(s.verifying_key().as_bytes())
    }

    fn mk_report(reporter: &SigningKey, target: &str, author: &str, ts: u64, nonce: u64) -> Report {
        let mut r = Report {
            target_cid: target.into(),
            target_author_pk: author.into(),
            category: ReportCategory::Spam,
            evidence_cid: None,
            reporter_pk: String::new(),
            timestamp: ts,
            nonce,
            signature: String::new(),
        };
        sign_report(reporter, &mut r);
        r
    }

    #[test]
    fn select_jurors_deterministic() {
        let pool: Vec<String> = (0..20).map(|i| format!("pk_{i}")).collect();
        let a = select_jurors(&pool, "seed", 7);
        let b = select_jurors(&pool, "seed", 7);
        assert_eq!(a, b);
        assert_eq!(a.len(), 7);
    }

    #[test]
    fn select_jurors_distinct() {
        let pool: Vec<String> = (0..20).map(|i| format!("pk_{i}")).collect();
        let chosen = select_jurors(&pool, "seed", 7);
        let unique: HashSet<_> = chosen.iter().collect();
        assert_eq!(unique.len(), 7);
    }

    #[test]
    fn report_signed_then_accepted() {
        let mut eng = ModerationEngine::new();
        let r = mk_report(&sk(1), "cid", "author", 0, 1);
        let pool: Vec<String> = (0..10).map(|i| pk_of(&sk(50 + i))).collect();
        assert!(eng.submit_report(r, || pool.clone(), "blockA", 1).unwrap().is_none());
    }

    #[test]
    fn forged_report_rejected() {
        let mut eng = ModerationEngine::new();
        let mut r = mk_report(&sk(1), "cid", "author", 0, 1);
        r.signature = "00".repeat(64);
        let pool: Vec<String> = vec![];
        let res = eng.submit_report(r, || pool, "block", 1);
        assert_eq!(res, Err(ModerationError::InvalidSignature));
    }

    #[test]
    fn duplicate_reporter_blocked() {
        let mut eng = ModerationEngine::new();
        let pool: Vec<String> = (0..10).map(|i| pk_of(&sk(50 + i))).collect();
        eng.submit_report(mk_report(&sk(1), "cid", "author", 0, 1), || pool.clone(), "b", 1).unwrap();
        let res = eng.submit_report(mk_report(&sk(1), "cid", "author", 0, 2), || pool.clone(), "b", 2);
        assert_eq!(res, Err(ModerationError::DuplicateReport));
    }

    #[test]
    fn case_opens_after_threshold() {
        let mut eng = ModerationEngine::new();
        let pool: Vec<String> = (0..10).map(|i| pk_of(&sk(50 + i))).collect();
        for i in 0..(REPORTS_TO_OPEN_CASE - 1) {
            eng.submit_report(
                mk_report(&sk(i as u8 + 1), "cid", "author", 0, i as u64),
                || pool.clone(),
                "blockA",
                1,
            )
            .unwrap();
        }
        let opened = eng
            .submit_report(
                mk_report(&sk(REPORTS_TO_OPEN_CASE as u8), "cid", "author", 0, 999),
                || pool.clone(),
                "blockA",
                1,
            )
            .unwrap();
        assert!(opened.is_some());
    }

    #[test]
    fn full_flow_innocent() {
        let mut eng = ModerationEngine::new();
        let jurors_sk: Vec<SigningKey> = (0..10).map(|i| sk(50 + i)).collect();
        let pool: Vec<String> = jurors_sk.iter().map(pk_of).collect();
        // 5 reports
        for i in 0..REPORTS_TO_OPEN_CASE {
            eng.submit_report(
                mk_report(&sk(i as u8 + 1), "cid", &pk_of(&sk(99)), 0, i as u64),
                || pool.clone(),
                "blockA",
                1,
            )
            .unwrap_or_default();
        }
        let case_id = eng.cases.keys().next().cloned().unwrap();
        let case = eng.case(&case_id).unwrap().clone();
        // tous les jurés commit Innocent
        for j_pk in &case.jurors {
            let j_sk = jurors_sk.iter().find(|s| pk_of(s) == *j_pk).unwrap();
            let nonce_hex = "00".repeat(32);
            let mut c = CommitVote {
                case_id: case_id.clone(),
                juror_pk: String::new(),
                commit_hash: build_commit_hash(Verdict::Innocent, &nonce_hex),
                timestamp: 100,
                signature: String::new(),
            };
            sign_commit(j_sk, &mut c);
            eng.submit_commit(c, 100).unwrap();
        }
        // reveal
        for j_pk in &case.jurors {
            let j_sk = jurors_sk.iter().find(|s| pk_of(s) == *j_pk).unwrap();
            let mut r = RevealVote {
                case_id: case_id.clone(),
                juror_pk: String::new(),
                verdict: Verdict::Innocent,
                reveal_nonce: "00".repeat(32),
                timestamp: case.commit_deadline + 10,
                signature: String::new(),
            };
            sign_reveal(j_sk, &mut r);
            eng.submit_reveal(r, case.commit_deadline + 10).unwrap();
        }
        let v = eng.finalize(&case_id, case.reveal_deadline + 1).unwrap();
        assert_eq!(v, Verdict::Innocent);
    }

    #[test]
    fn commit_reveal_mismatch_rejected() {
        let mut eng = ModerationEngine::new();
        let jurors_sk: Vec<SigningKey> = (0..10).map(|i| sk(50 + i)).collect();
        let pool: Vec<String> = jurors_sk.iter().map(pk_of).collect();
        for i in 0..REPORTS_TO_OPEN_CASE {
            eng.submit_report(
                mk_report(&sk(i as u8 + 1), "cid", &pk_of(&sk(99)), 0, i as u64),
                || pool.clone(),
                "blockA",
                1,
            )
            .unwrap_or_default();
        }
        let case_id = eng.cases.keys().next().cloned().unwrap();
        let case = eng.case(&case_id).unwrap().clone();
        let j_pk = case.jurors[0].clone();
        let j_sk = jurors_sk.iter().find(|s| pk_of(s) == j_pk).unwrap();
        let mut c = CommitVote {
            case_id: case_id.clone(),
            juror_pk: String::new(),
            commit_hash: build_commit_hash(Verdict::Innocent, &"00".repeat(32)),
            timestamp: 100,
            signature: String::new(),
        };
        sign_commit(j_sk, &mut c);
        eng.submit_commit(c, 100).unwrap();
        // tente un reveal avec un autre verdict → mismatch
        let mut r = RevealVote {
            case_id: case_id.clone(),
            juror_pk: String::new(),
            verdict: Verdict::Ban,
            reveal_nonce: "00".repeat(32),
            timestamp: case.commit_deadline + 10,
            signature: String::new(),
        };
        sign_reveal(j_sk, &mut r);
        assert_eq!(
            eng.submit_reveal(r, case.commit_deadline + 10),
            Err(ModerationError::CommitMismatch)
        );
    }

    #[test]
    fn anti_troll_progressive() {
        assert_eq!(anti_troll_mining_factor(0), 1.0);
        assert_eq!(anti_troll_mining_factor(2), 1.0);
        assert_eq!(anti_troll_mining_factor(3), 0.90);
        assert_eq!(anti_troll_mining_factor(5), 0.75);
        assert_eq!(anti_troll_mining_factor(8), 0.50);
        assert_eq!(anti_troll_mining_factor(20), 0.0);
    }

    #[test]
    fn slash_for_ban() {
        assert_eq!(slash_bps_for_verdict(Verdict::Ban), 1_000);
        assert_eq!(slash_bps_for_verdict(Verdict::Hide), 0);
        assert_eq!(slash_bps_for_verdict(Verdict::Innocent), 0);
    }

    /// AUDIT-MOD-1: pending (below-threshold) reports must survive a
    /// snapshot/restore cycle. Previously a restart wiped every
    /// accumulating report under the open-case threshold of 5.
    #[test]
    fn audit_mod1_pending_reports_persist_across_restart() {
        let mut eng = ModerationEngine::new();
        let pool: Vec<String> = (0..10).map(|i| pk_of(&sk(50 + i))).collect();
        // Submit 3 reports — below the threshold of 5, so no case opens.
        for i in 0..3 {
            eng.submit_report(
                mk_report(&sk(i + 1), "cid_X", "author_X", 0, i as u64),
                || pool.clone(),
                "blockA",
                1,
            )
            .unwrap();
        }
        // No case opened yet (below threshold).
        assert_eq!(eng.cases.len(), 0);
        // Pending should hold 3 reports under cid_X.
        assert_eq!(eng.pending_reports.get("cid_X").map(|v| v.len()), Some(3));

        // Snapshot + restore.
        let snap = eng.snapshot();
        let restored = ModerationEngine::restore(snap);
        assert_eq!(
            restored.pending_reports.get("cid_X").map(|v| v.len()),
            Some(3),
            "pending reports must survive restart (AUDIT-MOD-1)"
        );
        // seen_report_ids must also survive so duplicate-report dedup keeps working.
        assert_eq!(
            restored.seen_report_ids.len(),
            3,
            "seen_report_ids must survive restart"
        );
    }
}
