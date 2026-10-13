//! Shared data model of Coding Studio.
//!
//! Every other crate speaks these types. They are plain data: no I/O, no
//! database, no network. Hashing lives here because the same canonical form
//! must be computed identically by the ingest, the store and the AI layers.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod config;
pub mod phase;
pub mod reconcile;

pub use config::{Category, FrameworkPreset, PromptTemplate, StudyConfig};
pub use phase::{PHASE_1, PHASE_2};

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
    /// Budgie Echo markdown export: `**Speaker 2** (em dash) text`.
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
    /// True when the current text or role comes from a unit correction, not from
    /// the segmentation. Set by the store; ignored on input.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub corrected: bool,
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
    /// Display family: TECH, ORG or ENV from the TOE attachment, otherwise
    /// OTHER (phase 1 categories keep their own name).
    pub family: String,
    pub label: String,
    pub definition: String,
    /// literature (from the codebook and the literature) | field (created from
    /// the interviews) | legacy (family-level codes of the first passes, kept
    /// for the record) | phase1 (the provisional TOE categories).
    pub origin: String,
    /// TOE attachment: "T", "O", "E", or none when the code is not attached.
    #[serde(default)]
    pub toe: Option<String>,
    pub active: bool,
    /// Whether the code is offered to the AI coder. Default: every active code
    /// whose origin is not `legacy`.
    pub in_prompt: bool,
    /// Research questions the code bears on (ids of `ResearchQuestion`).
    #[serde(default)]
    pub research_questions: Vec<String>,
    /// Set when the code was merged into another one; it is then inactive.
    #[serde(default)]
    pub merged_into: Option<String>,
    /// Incremented on every edit of the code.
    #[serde(default = "one")]
    pub version: u32,
    /// An example segment or quote.
    #[serde(default)]
    pub example: Option<String>,
    /// Why the code was created (field codes).
    #[serde(default)]
    pub reason: Option<String>,
    /// Creation time, ISO 8601, for codes created in the tool.
    #[serde(default)]
    pub created_at: Option<String>,
}

fn one() -> u32 {
    1
}

/// TOE letters accepted on a code.
pub const TOE_LETTERS: [&str; 3] = ["T", "O", "E"];

/// Display family of a TOE attachment: TECH, ORG, ENV, or OTHER.
pub fn family_for_toe(toe: Option<&str>) -> &'static str {
    match toe {
        Some("T") => "TECH",
        Some("O") => "ORG",
        Some("E") => "ENV",
        _ => "OTHER",
    }
}

/// TOE letter of a phase 1 category (TECH, ORG, ENV), if any.
pub fn toe_of_context(category: &str) -> Option<&'static str> {
    match category {
        "TECH" => Some("T"),
        "ORG" => Some("O"),
        "ENV" => Some("E"),
        _ => None,
    }
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
    /// Set by the store when the coding was changed after an AI answer covering
    /// the unit had been imported. Ignored on input.
    #[serde(default)]
    pub revised_after_ai: bool,
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

/// The candidate's phase 2 coding of every unit of a prompt batch, frozen when
/// the batch was generated. The reference of every comparison with that batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanSnapshot {
    pub snapshot_id: i64,
    pub interview_id: String,
    pub batch_id: i64,
    pub phase: u8,
    pub segmentation_hash: String,
    pub codebook_hash: String,
    /// unit_id -> sorted codes (empty when the unit only carried an open label).
    pub codings: CodingMap,
    /// SHA-256 of the canonical JSON of `codings`.
    pub sha256: String,
    pub created_at: String,
}

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
    /// Prompt batch the run answers; the latest batch when omitted.
    #[serde(default)]
    pub batch_id: Option<i64>,
    /// The provider that produced the answers. Absent for a run whose answers
    /// were pasted by hand, so that their serialisation is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderRecord>,
}

/// Where a provider runs, as declared in its configuration (never guessed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderExecution {
    Local,
    Hosted { vendor: String, endpoint: String },
}

/// What the journal keeps of a provider run: which provider, where it runs,
/// which parameters it could and could not fix, and when each prompt part was
/// sent. Never holds a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: String,
    pub execution: ProviderExecution,
    pub parameters_exposed: Vec<String>,
    pub parameters_not_exposed: Vec<String>,
    /// One entry per prompt part, in part order.
    pub parts: Vec<ProviderPartRecord>,
}

/// One call of a provider: the prompt part it sent (its SHA-256 as stored in
/// the prompt), and the instants that bracket the call (RFC 3339 UTC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPartRecord {
    pub part: u32,
    pub request_sha256: String,
    pub started_at: String,
    pub finished_at: String,
}

/// One copy-paste prompt. Long interviews are split into parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPart {
    pub interview_id: String,
    /// Always 2: only the detailed codes are challenged by the AI.
    pub phase: u8,
    /// Generation the part belongs to (all parts generated together share it);
    /// assigned by the store when the parts are recorded.
    #[serde(default)]
    pub batch_id: i64,
    /// Codes offered in the part (snapshot), the reference for validating the answer.
    #[serde(default)]
    pub codes: Vec<String>,
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
    pub phase: u8,
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
    pub phase: u8,
    pub meta: AiRunMeta,
    /// Prompt batch answered by the run.
    pub batch_id: i64,
    /// Units of the batch, in order.
    pub unit_ids: Vec<String>,
    pub codings: CodingMap,
    /// Parts received so far, e.g. [1, 2] of 3.
    pub parts_received: Vec<u32>,
    pub parts_expected: Option<u32>,
    /// Every unit of the batch has a coding.
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
    #[serde(default = "default_phase")]
    pub phase: u8,
    pub choice: ArbitrationChoice,
    pub final_codes: CodeSet,
    /// Mandatory, never empty.
    pub rationale: String,
    pub decided_by: String,
    pub decided_at: String,
}

fn default_phase() -> u8 {
    PHASE_1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalBasis {
    /// The candidate's current coding (no review decision on the unit).
    Candidate,
    /// A review decision of the candidate.
    Reviewed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalCoding {
    pub unit_id: String,
    pub codes: CodeSet,
    pub basis: FinalBasis,
}

// ------------------------------------------------------------------ review

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    /// The candidate's codes differ from at least one complete AI run.
    Disagreement,
    /// The candidate and the AI agree; drawn at random for a second look.
    AgreementSample,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub unit_id: String,
    pub kind: ReviewKind,
    pub human: CodeSet,
    /// The candidate's current coding (may differ from `human`, the snapshot).
    #[serde(default)]
    pub human_current: CodeSet,
    #[serde(default)]
    pub revised_since_snapshot: bool,
    /// Codes per run label (complete runs).
    pub ai: BTreeMap<String, CodeSet>,
    pub decision: Option<Arbitration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSnapshotRef {
    pub run_label: String,
    pub snapshot_id: i64,
    pub sha256: String,
}

// -------------------------------------------------------------- to clarify

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionKind {
    /// A new code was created from the label (origin field).
    NewCode,
    /// The label was mapped to an existing code.
    Mapped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub kind: ResolutionKind,
    pub code: String,
    pub at: String,
    pub by: String,
    pub rationale: Option<String>,
}

/// A provisional label put on a unit while the right code is not yet known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToClarify {
    pub id: i64,
    pub interview_id: String,
    pub unit_id: String,
    pub label: String,
    pub note: String,
    pub created_at: String,
    pub created_by: String,
    pub resolved: Option<Resolution>,
}

// ------------------------------------------- verification, claims, memos, saturation

/// Closed list of unit flags.
pub const UNIT_FLAGS: [&str; 4] = ["VERIFY", "SECOND-HAND", "UNFINISHED", "CO-CONSTRUCTED"];
/// Required with the CO-CONSTRUCTED flag, refused otherwise.
pub const CO_CONSTRUCTED_KINDS: [&str; 2] = ["assent", "content"];
/// `unverified` is computed (no row, or a stale row); only the other two are stored.
pub const VERIFICATION_STATES: [&str; 3] = ["unverified", "verified", "unusable"];
pub const EV_LEVELS: [&str; 5] = ["EV0", "EV1", "EV2", "EV3", "EV4"];
pub const PROVENANCES: [&str; 4] = [
    "SPONTANEOUS",
    "PROBED",
    "DOCUMENT-DERIVED",
    "RESEARCHER-INFERRED",
];
pub const CLAIM_ROLES: [&str; 5] = [
    "trigger",
    "retrospective-justification",
    "enabling-condition",
    "constraint",
    "undetermined",
];

/// Flags set on a unit (current state, editable).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UnitFlags {
    pub interview_id: String,
    pub unit_id: String,
    /// Subset of `UNIT_FLAGS`.
    pub flags: Vec<String>,
    /// `assent` | `content`, set only with `CO-CONSTRUCTED`.
    pub co_constructed_kind: Option<String>,
    pub updated_at: String,
}

/// One correction of a unit's text or role (append-only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCorrection {
    pub correction_id: i64,
    pub interview_id: String,
    pub unit_id: String,
    pub text: String,
    pub role: SpeakerRole,
    pub reason: String,
    pub created_at: String,
    pub created_by: String,
}

/// Current verification status of a unit, computed from the latest row.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VerificationStatus {
    pub unit_id: String,
    /// One of `VERIFICATION_STATES`.
    pub state: String,
    /// True when the row no longer matches the current text or role.
    pub stale: bool,
    pub at: Option<String>,
    pub by: Option<String>,
    pub note: Option<String>,
}

/// A unit that must be verified, with the reasons it was selected.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VerificationTarget {
    pub unit_id: String,
    /// `quoted`, `flagged`, `disagreement`, `case`, `sample`.
    pub reasons: Vec<String>,
}

/// Verification counts for one interview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VerificationCoverage {
    pub n_units: usize,
    pub n_verified: usize,
    pub n_unusable: usize,
    pub n_stale: usize,
    pub n_targets: usize,
    pub n_targets_verified: usize,
}

/// A qualification of a statement inside a unit (editable).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Claim {
    #[serde(default)]
    pub claim_id: i64,
    #[serde(default)]
    pub interview_id: String,
    pub unit_id: String,
    /// Exact substring of the unit's current text.
    pub excerpt: String,
    /// Subset of the unit's current phase 2 codes; may be empty.
    #[serde(default)]
    pub codes: CodeSet,
    /// One of `EV_LEVELS`.
    #[serde(default)]
    pub ev_level: Option<String>,
    /// One of `PROVENANCES`.
    #[serde(default)]
    pub provenance: Option<String>,
    /// One of `CLAIM_ROLES`.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub memo: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Memo of one research question, written across interviews.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RqMemo {
    pub rq_id: String,
    #[serde(default)]
    pub documented: String,
    #[serde(default)]
    pub interpretation: String,
    #[serde(default)]
    pub rivals: String,
    #[serde(default)]
    pub limits: String,
    /// Unit ids supporting the answer; must exist in their interview's current segmentation.
    #[serde(default)]
    pub units_for: Vec<String>,
    /// Unit ids against the answer; same rule.
    #[serde(default)]
    pub units_against: Vec<String>,
    #[serde(default)]
    pub updated_at: String,
}

/// One interview's row of the saturation table (provisional, see export header).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaturationRow {
    pub interview_id: String,
    pub order: u32,
    pub first_coded_at: String,
    /// Codes whose first use in the corpus is in this interview.
    pub codes_first_used: Vec<String>,
    /// Field codes created while this interview was in play.
    pub field_codes_created: Vec<String>,
    pub labels_opened: u32,
    pub labels_resolved: u32,
}

// --------------------------------------------------- questions and cases

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchQuestion {
    /// Short id, e.g. RQ1 or SQ1.
    pub id: String,
    pub text: String,
    pub ord: u32,
}

/// One field of a case synthesis, backed by units.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CaseField {
    pub text: String,
    #[serde(default)]
    pub contradictions: String,
    #[serde(default)]
    pub missing: String,
    /// unit_ids, from any interview, that illustrate the text; each must be among the case's attributed `units`.
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// Synthesis of one case (one identifiable project) told in an interview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    /// e.g. ECO-05-C1; generated when empty on creation.
    #[serde(default)]
    pub case_id: String,
    #[serde(default)]
    pub interview_id: String,
    pub label: String,
    #[serde(default)]
    pub trigger: CaseField,
    #[serde(default)]
    pub alternatives: CaseField,
    #[serde(default)]
    pub constraints: CaseField,
    #[serde(default)]
    pub choice: CaseField,
    #[serde(default)]
    pub result: CaseField,
    /// Units attributed to the case, from any interview, sorted and deduplicated. The cross-case matrix uses only these.
    #[serde(default)]
    pub units: Vec<String>,
    /// Read-only: interview ids derived from `units` plus `interview_id`, sorted.
    #[serde(default)]
    pub interviews: Vec<String>,
    /// adopted | deferred | rejected, or free text.
    #[serde(default)]
    pub decision: Option<String>,
    /// item | batch | model, or free text.
    #[serde(default)]
    pub granularity: Option<String>,
    #[serde(default)]
    pub carrier_chosen: Option<String>,
    #[serde(default)]
    pub carriers_rejected: Vec<String>,
    /// first-hand | reported | ecosystem | anticipation, or free text.
    #[serde(default)]
    pub account_type: Option<String>,
    #[serde(default)]
    pub swiss_link: Option<String>,
    /// STAGE-* of the codebook, free text otherwise.
    #[serde(default)]
    pub stage: Option<String>,
    /// STATUS-* of the codebook, free text otherwise.
    #[serde(default)]
    pub outcome_status: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub updated_at: String,
}

impl Case {
    /// The five synthesis fields with their names, in reading order.
    pub fn fields(&self) -> [(&'static str, &CaseField); 5] {
        [
            ("trigger", &self.trigger),
            ("alternatives", &self.alternatives),
            ("constraints", &self.constraints),
            ("choice", &self.choice),
            ("result", &self.result),
        ]
    }
}

/// Codes of one research question found in one case, with their units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossCaseCode {
    pub code: String,
    pub units: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossCaseCell {
    pub rq: String,
    pub case_id: String,
    /// false when the case has no attributed unit; then `codes` is empty.
    #[serde(default)]
    pub documented: bool,
    pub codes: Vec<CrossCaseCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossCase {
    pub questions: Vec<ResearchQuestion>,
    pub cases: Vec<Case>,
    pub cells: Vec<CrossCaseCell>,
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

/// Progress of one coding phase of an interview. In phase 1 only the unit and
/// coding counts are used.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PhaseSummary {
    pub phase: u8,
    /// Units in scope: every unit in phase 1; in phase 2 the units whose
    /// current phase 1 category is not NA.
    pub n_units: usize,
    /// Units coded (phase 2: codes or an open label to clarify).
    pub n_human_coded: usize,
    pub human_complete: bool,
    pub n_ai_runs: usize,
    pub n_ai_runs_complete: usize,
    /// Disagreements between the candidate and a complete AI run.
    pub n_divergences: usize,
    /// Agreements drawn for review.
    pub n_sampled: usize,
    /// Review decisions recorded (disagreements and sampled agreements).
    pub n_arbitrated: usize,
    /// Open labels to clarify.
    pub n_to_clarify: usize,
    /// At least one complete AI run and every disagreement decided.
    pub reconciled: bool,
}

/// Where an interview stands in the pipeline, for the library view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterviewSummary {
    pub interview_id: String,
    pub folder_name: String,
    pub n_sources: usize,
    pub primary_source: Option<String>,
    pub n_units: usize,
    pub n_warnings: usize,
    pub phase1: PhaseSummary,
    pub phase2: PhaseSummary,
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
