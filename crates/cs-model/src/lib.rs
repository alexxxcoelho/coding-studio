//! Shared data model of Coding Studio.
//!
//! Every other crate speaks these types. They are plain data: no I/O, no
//! database, no network. Hashing lives here because the same canonical form
//! must be computed identically by the ingest, the store and the AI layers.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Code used when no code of the codebook applies to a unit. Exclusive: a unit
/// coded `NA` carries no other code.
pub const NA: &str = "NA";

/// Version of the deterministic segmentation rule. Stored with every
/// segmentation so a unit list can always be regenerated and compared.
pub const SEGMENTATION_RULE: &str = "turns-v1: one unit per continuous speaker turn, turns over 150 words split at sentence boundaries";

/// Maximum number of words per unit before a turn is split (same value as
/// `00_Tools/Scripts/segment_transcript.py`).
pub const MAX_WORDS: usize = 150;

// ------------------------------------------------------------------ errors

/// Errors that carry a meaning for the user interface. Wrapped in
/// `anyhow::Error` by the crates; the server downcasts to choose a status code.
#[derive(Debug, thiserror::Error)]
pub enum CsError {
    /// Input rejected (bad format, unknown code, excerpt not in the unit...). HTTP 400.
    #[error("{0}")]
    Validation(String),
    /// Protocol lock refused the action (blind coding guard, locked coding). HTTP 409.
    #[error("protocol guard: {0}")]
    Guard(String),
    /// Unknown interview, unit, run. HTTP 404.
    #[error("not found: {0}")]
    NotFound(String),
}

// ------------------------------------------------------------------ hashing

/// Lower-case hex SHA-256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Hash of a value's canonical JSON form. `serde_json` writes struct fields in
/// declaration order and `BTreeMap`/`BTreeSet` in key order, so the result is
/// stable across runs and platforms.
pub fn canonical_hash<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).expect("model types always serialise");
    sha256_hex(&json)
}

// ------------------------------------------------------------------ library

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// WebVTT from the conferencing platform (`<v Name>` voice tags).
    Vtt,
    /// Raw ASR text, one turn per line: `[hh:mm:ss] S1: ...` or `[mm:ss] [Speaker 1]: ...`.
    TimestampedText,
    /// `02_transcript_cleaned_diarised.md`: cleaned turns labelled INT / RESP.
    CleanedMarkdown,
    /// Budgie Echo markdown export: `**Speaker 2** — text`.
    BudgieMarkdown,
    /// Written answer received by email (no speaker turns, paragraphs).
    WrittenResponse,
    /// `04_parts/b_summary.md`: interview summary (read-only context, never sent to the AI).
    Summary,
    /// `04_parts/a_record_sheet.md` or `04_interview_record.md`.
    RecordSheet,
    /// `04_parts/c_toe_coding.md`: earlier first-pass coding table.
    ToeCoding,
    /// Anything else kept for the record (pipeline log, e-mails...).
    Other,
}

impl SourceKind {
    /// True when the file can be segmented into units.
    pub fn is_transcript(self) -> bool {
        matches!(
            self,
            SourceKind::Vtt
                | SourceKind::TimestampedText
                | SourceKind::CleanedMarkdown
                | SourceKind::BudgieMarkdown
                | SourceKind::WrittenResponse
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    /// Path relative to the interview folder, `/` separators.
    pub rel_path: String,
    pub kind: SourceKind,
    pub sha256: String,
    pub size: u64,
    /// Last modification time, ISO 8601 UTC, when the file system reports it.
    pub modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterviewEntry {
    /// Normalised ID, e.g. `ECO-33` (folder `ECO33_RomanEppenberger`).
    pub interview_id: String,
    pub folder_name: String,
    /// Absolute path of the interview folder.
    pub folder_path: String,
    pub sources: Vec<SourceFile>,
    /// `rel_path` of the transcript proposed for segmentation, if any.
    pub primary_source: Option<String>,
    /// Anything the scanner skipped or could not read, in plain words.
    pub warnings: Vec<String>,
}

// ------------------------------------------------------------- segmentation

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    Interviewer,
    Respondent,
    /// Label present but its role cannot be decided from the source (S1, Speaker 2).
    Unknown,
}

impl SpeakerRole {
    /// Short label used in units, prompts and exports.
    pub fn short(self) -> &'static str {
        match self {
            SpeakerRole::Interviewer => "INT",
            SpeakerRole::Respondent => "RESP",
            SpeakerRole::Unknown => "UNK",
        }
    }
}

/// One speaker turn as read from a source, before segmentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Label exactly as written in the source (`INT`, `Dos Santos Alexandre`, `S1`).
    pub speaker_raw: String,
    pub role: SpeakerRole,
    /// Start mark as written, normalised to `mm:ss` or `hh:mm:ss`; `P01`.. for written responses.
    pub timestamp: String,
    pub text: String,
}

/// One unit of analysis: the thing that is coded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    /// `<interview_id>-U<nnn>`, e.g. `ECO-16-U007`.
    pub unit_id: String,
    pub seq: u32,
    pub speaker_raw: String,
    pub role: SpeakerRole,
    pub timestamp: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segmentation {
    pub interview_id: String,
    pub source_rel_path: String,
    pub source_sha256: String,
    pub rule: String,
    pub units: Vec<Unit>,
    /// `canonical_hash(&units)`. Binds prompts, AI outputs and exports to this exact unit list.
    pub hash: String,
    /// Lines of the source that were not turned into units, with the reason.
    pub warnings: Vec<String>,
}

impl Segmentation {
    pub fn units_hash(units: &[Unit]) -> String {
        canonical_hash(&units)
    }
}

// ------------------------------------------------------------------ codebook

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Code {
    pub code: String,
    /// TECH | ORG | ENV | OUTCOME | TURNING-POINT | TRANSFER | STAGE | CONTEXT | INDUCTIVE
    pub family: String,
    pub label: String,
    pub definition: String,
    /// codebook | legacy | inductive | bookkeeping
    pub origin: String,
    pub active: bool,
    /// Whether the code is offered to the AI coder. Default: every active code
    /// whose origin is not `legacy`.
    pub in_prompt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Codebook {
    /// Where the codes came from, e.g. `05_Analysis/coding/coding.db`.
    pub source: String,
    pub codes: Vec<Code>,
}

impl Codebook {
    pub fn get(&self, code: &str) -> Option<&Code> {
        self.codes.iter().find(|c| c.code == code)
    }
    /// Codes sent to the AI coder, in codebook order.
    pub fn prompt_codes(&self) -> Vec<&Code> {
        self.codes
            .iter()
            .filter(|c| c.active && c.in_prompt)
            .collect()
    }
    /// Hash of the codes offered to the AI coder; stored with every prompt.
    pub fn prompt_hash(&self) -> String {
        canonical_hash(&self.prompt_codes())
    }
}

// ------------------------------------------------------------------- coding

/// A set of codes. `{"NA"}` means "no code applies"; the empty set means "not coded yet".
pub type CodeSet = BTreeSet<String>;

/// Codes per unit_id.
pub type CodingMap = BTreeMap<String, CodeSet>;

/// The candidate's coding of one unit.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HumanCoding {
    pub unit_id: String,
    pub codes: CodeSet,
    /// Facets of the code, comma separated, as in the coding files.
    pub subcode: Option<String>,
    /// ENABLER | BARRIER | CONTESTED | ...
    pub modifier: Option<String>,
    /// EV0..EV4
    pub ev_level: Option<String>,
    /// SPONTANEOUS | PROBED | DOCUMENT-DERIVED | RESEARCHER-INFERRED
    pub provenance: Option<String>,
    pub memo: Option<String>,
    /// Verbatim extract; must be an exact substring of the unit text.
    pub excerpt: Option<String>,
}

/// Checks the rules every code set must follow. Returns the problems found.
pub fn check_code_set(codes: &CodeSet) -> Vec<String> {
    let mut issues = Vec::new();
    if codes.is_empty() {
        issues.push("no code given; use NA when no code applies".to_string());
    }
    if codes.contains(NA) && codes.len() > 1 {
        issues.push("NA cannot be combined with other codes".to_string());
    }
    issues
}

// ------------------------------------------------------------------- AI runs

/// Parameters journalised for every AI run (P-04, after Fehring et al. 2026).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AiRunMeta {
    /// R1, R2, R3...
    pub run_label: String,
    pub model_name: String,
    pub model_version: String,
    pub temperature: Option<f64>,
    pub seed: Option<String>,
    pub context_length: Option<u32>,
    pub batch_size: Option<u32>,
    /// Prompt identifier and version, `cs_ai::PROMPT_VERSION` unless reworded (R2).
    pub prompt_id_version: String,
    /// When the candidate ran the prompt, ISO 8601.
    pub executed_at: String,
    pub notes: Option<String>,
}

/// One copy-paste prompt. Long interviews are split into parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPart {
    pub interview_id: String,
    pub part: u32,
    pub of: u32,
    pub unit_ids: Vec<String>,
    pub segmentation_hash: String,
    pub codebook_hash: String,
    pub prompt_version: String,
    pub text: String,
    pub sha256: String,
}

/// Validated content of one AI answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiOutput {
    pub interview_id: String,
    pub segmentation_hash: String,
    pub part: u32,
    pub of: u32,
    pub codings: CodingMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// `$` for the document, otherwise the unit_id or JSON path concerned.
    pub location: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRun {
    pub run_id: i64,
    pub interview_id: String,
    pub meta: AiRunMeta,
    pub codings: CodingMap,
    /// Parts received so far, e.g. [1, 2] of 3.
    pub parts_received: Vec<u32>,
    pub parts_expected: Option<u32>,
    /// Every unit of the segmentation has a coding.
    pub complete: bool,
}

// --------------------------------------------------------------- agreement

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeAgreement {
    pub code: String,
    pub n_a: usize,
    pub n_b: usize,
    /// Share of units where both coders agree on presence/absence, 0..1.
    pub raw: f64,
    /// Cohen's kappa on presence/absence; `None` when undefined (one level observed).
    pub kappa: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgreementReport {
    pub label_a: String,
    pub label_b: String,
    pub n_units: usize,
    pub n_exact: usize,
    pub per_code: Vec<CodeAgreement>,
    pub divergent_unit_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceKind {
    /// Human set differs from at least one AI run.
    HumanVsAi,
    /// Human agrees with some runs, but the AI runs disagree among themselves.
    AiRunsUnstable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divergence {
    pub unit_id: String,
    pub kind: DivergenceKind,
    pub human: CodeSet,
    /// Codes per run label.
    pub ai: BTreeMap<String, CodeSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ArbitrationChoice {
    Human,
    Ai { run_label: String },
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arbitration {
    pub unit_id: String,
    pub choice: ArbitrationChoice,
    pub final_codes: CodeSet,
    /// Mandatory, never empty.
    pub rationale: String,
    pub decided_by: String,
    pub decided_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalBasis {
    /// Human and every AI run gave the same set.
    Agreed,
    /// Divergence settled by an arbitration record.
    Arbitrated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalCoding {
    pub unit_id: String,
    pub codes: CodeSet,
    pub basis: FinalBasis,
}

// -------------------------------------------------------------------- audit

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub seq: i64,
    /// ISO 8601 UTC.
    pub ts: String,
    pub actor: String,
    /// Dotted verb, e.g. `library.scan`, `coding.set`, `ai.output.import`.
    pub action: String,
    pub interview_id: Option<String>,
    /// unit_id, run id, file... whatever the action is about.
    pub target: Option<String>,
    /// JSON payload: before/after values, hashes, counts.
    pub detail: serde_json::Value,
    pub prev_hash: String,
    pub hash: String,
}

impl AuditEvent {
    /// Hash that chains this event to the previous one. Any edit of a past
    /// event breaks every later hash, which `verify` reports.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_hash(
        prev_hash: &str,
        seq: i64,
        ts: &str,
        actor: &str,
        action: &str,
        interview_id: Option<&str>,
        target: Option<&str>,
        detail: &serde_json::Value,
    ) -> String {
        let payload = serde_json::json!([
            prev_hash,
            seq,
            ts,
            actor,
            action,
            interview_id,
            target,
            detail
        ]);
        canonical_hash(&payload)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainCheck {
    pub ok: bool,
    pub n_events: usize,
    /// seq of the first event whose hash does not match.
    pub first_bad_seq: Option<i64>,
}

// ---------------------------------------------------------- library summary

/// Where an interview stands in the pipeline, for the library view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterviewSummary {
    pub interview_id: String,
    pub folder_name: String,
    pub n_sources: usize,
    pub primary_source: Option<String>,
    pub n_units: usize,
    pub n_human_coded: usize,
    pub human_complete: bool,
    pub n_ai_runs: usize,
    pub n_ai_runs_complete: usize,
    pub n_divergences: usize,
    pub n_arbitrated: usize,
    pub n_warnings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hash_is_stable_and_order_insensitive_for_sets() {
        let a: CodeSet = ["TECH-ADV", "ORG-SKILLS"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let b: CodeSet = ["ORG-SKILLS", "TECH-ADV"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(canonical_hash(&a), canonical_hash(&b));
        assert_eq!(sha256_hex(b"abc").len(), 64);
    }

    #[test]
    fn code_set_rules() {
        let empty = CodeSet::new();
        assert_eq!(check_code_set(&empty).len(), 1);
        let na_mixed: CodeSet = ["NA", "TECH"].iter().map(|s| s.to_string()).collect();
        assert_eq!(check_code_set(&na_mixed).len(), 1);
        let ok: CodeSet = ["NA"].iter().map(|s| s.to_string()).collect();
        assert!(check_code_set(&ok).is_empty());
    }

    #[test]
    fn audit_hash_changes_with_any_field() {
        let d = serde_json::json!({"a": 1});
        let h1 = AuditEvent::compute_hash("0", 1, "t", "candidate", "x", None, None, &d);
        let h2 = AuditEvent::compute_hash("0", 1, "t", "candidate", "y", None, None, &d);
        assert_ne!(h1, h2);
    }
}
