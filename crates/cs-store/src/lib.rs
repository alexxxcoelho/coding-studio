//! SQLite persistence and the append-only, hash-chained audit trail.
//!
//! Every write method takes the `actor` and records one audit event in the same
//! transaction as the change. Protocol locks are enforced here, not in the UI:
//! - AI output import refused while human coding is incomplete (blind guard, P-04).
//! - Human coding frozen once any AI output exists for the interview.
//! - Re-segmentation refused once any coding exists for the interview.
//!
//! Protocol refusals are returned as `cs_model::CsError::Guard`, bad input as
//! `CsError::Validation`, unknown ids as `CsError::NotFound` (wrapped in anyhow).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use cs_model::{
    AiOutput, AiRun, AiRunMeta, Arbitration, AuditEvent, ChainCheck, Code, Codebook, CodingMap,
    HumanCoding, InterviewEntry, InterviewSummary, PromptPart, Segmentation,
};

pub struct Store {
    #[allow(dead_code)]
    conn: rusqlite::Connection,
}

/// Result of the blind-coding guard.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GuardStatus {
    pub complete: bool,
    pub n_units: usize,
    pub missing: Vec<String>,
}

impl Store {
    /// Opens (or creates) the database, applies the schema, `journal_mode = DELETE`
    /// (safe on synced folders), `foreign_keys = ON`.
    pub fn open(path: &Path) -> Result<Store> {
        let _ = path;
        todo!()
    }
    pub fn open_in_memory() -> Result<Store> {
        todo!()
    }

    // ---- settings (key/value), e.g. library_root
    pub fn set_setting(&self, key: &str, value: &str, actor: &str) -> Result<()> {
        let _ = (key, value, actor);
        todo!()
    }
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        let _ = key;
        todo!()
    }

    // ---- library
    /// Inserts or refreshes an interview and its source inventory; logs added,
    /// removed and changed (hash) sources.
    pub fn upsert_interview(&self, entry: &InterviewEntry, actor: &str) -> Result<()> {
        let _ = (entry, actor);
        todo!()
    }
    pub fn interview(&self, interview_id: &str) -> Result<Option<InterviewEntry>> {
        let _ = interview_id;
        todo!()
    }
    pub fn set_primary_source(
        &self,
        interview_id: &str,
        rel_path: &str,
        actor: &str,
    ) -> Result<()> {
        let _ = (interview_id, rel_path, actor);
        todo!()
    }
    pub fn summaries(&self) -> Result<Vec<InterviewSummary>> {
        todo!()
    }

    // ---- codebook
    /// Reads `codes` from the thesis coding.db (opened read-only) and replaces the
    /// local codebook, keeping local inductive codes and `in_prompt` choices.
    pub fn import_codebook(&self, coding_db: &Path, actor: &str) -> Result<Codebook> {
        let _ = (coding_db, actor);
        todo!()
    }
    pub fn codebook(&self) -> Result<Codebook> {
        todo!()
    }
    /// New inductive code (family INDUCTIVE, origin inductive). Refuses duplicates.
    pub fn add_code(&self, code: &Code, actor: &str) -> Result<()> {
        let _ = (code, actor);
        todo!()
    }
    pub fn set_code_in_prompt(&self, code: &str, in_prompt: bool, actor: &str) -> Result<()> {
        let _ = (code, in_prompt, actor);
        todo!()
    }

    // ---- segmentation
    pub fn save_segmentation(&self, seg: &Segmentation, actor: &str) -> Result<()> {
        let _ = (seg, actor);
        todo!()
    }
    pub fn segmentation(&self, interview_id: &str) -> Result<Option<Segmentation>> {
        let _ = interview_id;
        todo!()
    }

    // ---- human coding
    /// Validates codes against the codebook (or NA), NA exclusivity, excerpt is a
    /// substring of the unit text; logs before/after.
    pub fn set_human_coding(
        &self,
        interview_id: &str,
        coding: &HumanCoding,
        actor: &str,
    ) -> Result<()> {
        let _ = (interview_id, coding, actor);
        todo!()
    }
    pub fn human_codings(&self, interview_id: &str) -> Result<BTreeMap<String, HumanCoding>> {
        let _ = interview_id;
        todo!()
    }
    pub fn human_coding_map(&self, interview_id: &str) -> Result<CodingMap> {
        let _ = interview_id;
        todo!()
    }
    pub fn guard(&self, interview_id: &str) -> Result<GuardStatus> {
        let _ = interview_id;
        todo!()
    }

    // ---- prompts and AI runs
    pub fn record_prompt(&self, part: &PromptPart, actor: &str) -> Result<()> {
        let _ = (part, actor);
        todo!()
    }
    pub fn prompts(&self, interview_id: &str) -> Result<Vec<PromptPart>> {
        let _ = interview_id;
        todo!()
    }
    pub fn create_ai_run(&self, interview_id: &str, meta: &AiRunMeta, actor: &str) -> Result<i64> {
        let _ = (interview_id, meta, actor);
        todo!()
    }
    /// Stores the raw answer verbatim (and its hash) with the parsed codings.
    /// Refused by the blind guard; refuses a part already received for the run.
    pub fn record_ai_output(
        &self,
        run_id: i64,
        raw: &str,
        parsed: &AiOutput,
        actor: &str,
    ) -> Result<()> {
        let _ = (run_id, raw, parsed, actor);
        todo!()
    }
    pub fn ai_runs(&self, interview_id: &str) -> Result<Vec<AiRun>> {
        let _ = interview_id;
        todo!()
    }
    /// Raw answers of a run, in the order received: (part, raw_text, sha256, imported_at).
    pub fn ai_raw_outputs(&self, run_id: i64) -> Result<Vec<(u32, String, String, String)>> {
        let _ = run_id;
        todo!()
    }

    // ---- arbitration
    /// Rationale must not be empty. A new decision on the same unit is appended;
    /// the latest one is current, earlier ones stay in the history.
    pub fn record_arbitration(
        &self,
        interview_id: &str,
        arb: &Arbitration,
        actor: &str,
    ) -> Result<()> {
        let _ = (interview_id, arb, actor);
        todo!()
    }
    pub fn arbitrations(&self, interview_id: &str) -> Result<BTreeMap<String, Arbitration>> {
        let _ = interview_id;
        todo!()
    }
    pub fn arbitration_history(&self, interview_id: &str) -> Result<Vec<Arbitration>> {
        let _ = interview_id;
        todo!()
    }

    // ---- audit
    /// Appends an event outside of a domain write (e.g. `export.package`).
    pub fn log(
        &self,
        actor: &str,
        action: &str,
        interview_id: Option<&str>,
        target: Option<&str>,
        detail: serde_json::Value,
    ) -> Result<AuditEvent> {
        let _ = (actor, action, interview_id, target, detail);
        todo!()
    }
    pub fn audit_events(&self, interview_id: Option<&str>) -> Result<Vec<AuditEvent>> {
        let _ = interview_id;
        todo!()
    }
    pub fn verify_chain(&self) -> Result<ChainCheck> {
        todo!()
    }
}
