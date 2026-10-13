// TypeScript mirror of crates/cs-model/src/lib.rs (serde, snake_case).
// Sets (BTreeSet) arrive as sorted arrays.

export type Phase = 1 | 2

export type SourceKind =
  | "vtt"
  | "timestamped_text"
  | "cleaned_markdown"
  | "budgie_markdown"
  | "written_response"
  | "docx"
  | "pdf"
  | "summary"
  | "record_sheet"
  | "toe_coding"
  | "other"

export interface SourceFile {
  rel_path: string
  kind: SourceKind
  sha256: string
  size: number
  modified: string | null
}

export interface InterviewEntry {
  interview_id: string
  folder_name: string
  folder_path: string
  sources: SourceFile[]
  primary_source: string | null
  warnings: string[]
}

export type SpeakerRole = "interviewer" | "respondent" | "unknown"
export const SPEAKER_ROLES: readonly SpeakerRole[] = ["interviewer", "respondent", "unknown"]

export interface Unit {
  unit_id: string
  seq: number
  speaker_raw: string
  role: SpeakerRole
  timestamp: string
  text: string
  /** Set by the server when the current text or role comes from a correction. */
  corrected?: boolean
}

export interface Segmentation {
  interview_id: string
  source_rel_path: string
  source_sha256: string
  rule: string
  units: Unit[]
  hash: string
  warnings: string[]
}

/** TOE attachment of a concrete code. */
export type Toe = "T" | "O" | "E"

/** literature | field | legacy (phase 1 categories carry "phase1"). */
export type CodeOrigin = "literature" | "field" | "legacy" | "phase1" | string

export interface Code {
  code: string
  /** Display family: TECH, ORG, ENV from the TOE attachment, otherwise OTHER. */
  family: string
  label: string
  definition: string
  origin: CodeOrigin
  active: boolean
  in_prompt: boolean
  toe?: Toe | null
  research_questions?: string[]
  merged_into?: string | null
  version?: number
  example?: string | null
  reason?: string | null
  created_at?: string | null
}

export interface Codebook {
  source: string
  codes: Code[]
}

export interface HumanCoding {
  unit_id: string
  codes: string[]
  subcode: string | null
  modifier: string | null
  ev_level: string | null
  provenance: string | null
  memo: string | null
  excerpt: string | null
  /** Set by the server: changed after an AI answer covered the unit. Ignored on input. */
  revised_after_ai?: boolean
}

export interface AiRunMeta {
  run_label: string
  model_name: string
  model_version: string
  temperature: number | null
  seed: string | null
  context_length: number | null
  batch_size: number | null
  prompt_id_version: string
  executed_at: string
  notes: string | null
  /** Prompt batch the run answers; null = the latest batch. */
  batch_id?: number | null
  /** Set when a provider ran the batch (v5); absent for pasted runs. */
  provider?: RunProviderRecord | null
}

export interface PromptPart {
  interview_id: string
  phase: Phase
  batch_id?: number
  /** Codes offered in this batch (snapshot). */
  codes?: string[]
  part: number
  of: number
  unit_ids: string[]
  segmentation_hash: string
  codebook_hash: string
  prompt_version: string
  text: string
  sha256: string
}

export interface ValidationIssue {
  location: string
  message: string
}

export interface AiRun {
  run_id: number
  interview_id: string
  phase: Phase
  meta: AiRunMeta
  batch_id?: number
  unit_ids?: string[]
  codings: Record<string, string[]>
  parts_received: number[]
  parts_expected: number | null
  complete: boolean
}

export interface CodeAgreement {
  code: string
  n_a: number
  n_b: number
  raw: number
  kappa: number | null
}

export interface AgreementReport {
  label_a: string
  label_b: string
  n_units: number
  n_exact: number
  per_code: CodeAgreement[]
  divergent_unit_ids: string[]
}

export type DivergenceKind = "human_vs_ai" | "ai_runs_unstable"

export interface Divergence {
  unit_id: string
  kind: DivergenceKind
  human: string[]
  ai: Record<string, string[]>
}

export type ArbitrationChoice =
  | { type: "human" }
  | { type: "ai"; run_label: string }
  | { type: "custom" }

export interface Arbitration {
  unit_id: string
  phase: Phase
  choice: ArbitrationChoice
  final_codes: string[]
  rationale: string
  decided_by: string
  decided_at: string
}

export type FinalBasis = "candidate" | "reviewed" | "agreed" | "arbitrated"

export interface FinalCoding {
  unit_id: string
  codes: string[]
  basis: FinalBasis
}

export interface Resolution {
  kind: "new_code" | "mapped"
  code: string
  at: string
  by: string
  rationale: string | null
}

/** A provisional label put on a unit, waiting to become or join a code. */
export interface ToClarify {
  id: number
  interview_id: string
  unit_id: string
  label: string
  note: string
  created_at: string
  created_by: string
  resolved: Resolution | null
  /** Filled by the list endpoint. */
  unit_text?: string
}

export interface ResearchQuestion {
  id: string
  text: string
  ord: number
}

export interface CaseField {
  text: string
  /** Where the data disagree, in the respondent's terms. */
  contradictions: string
  /** What the data do not tell, and would be needed to. */
  missing: string
  /** Unit ids that illustrate the text; each must be among the case's `units`. */
  evidence: string[]
}

export const CASE_FIELDS = ["trigger", "alternatives", "constraints", "choice", "result"] as const
export type CaseFieldKey = (typeof CASE_FIELDS)[number]

export interface Case {
  case_id: string
  /** The interview the case was created from. */
  interview_id: string
  label: string
  trigger: CaseField
  alternatives: CaseField
  constraints: CaseField
  choice: CaseField
  result: CaseField
  /** Units attributed to the case, from any interview. The cross-case matrix uses only these. */
  units: string[]
  /** Read-only: interviews derived by the server from `units` and `interview_id`. */
  interviews: string[]
  /** adopted | deferred | rejected, or free text. */
  decision: string | null
  /** item | batch | model, or free text. */
  granularity: string | null
  carrier_chosen: string | null
  carriers_rejected: string[]
  /** first-hand | reported | ecosystem | anticipation, or free text. */
  account_type: string | null
  swiss_link: string | null
  stage: string | null
  outcome_status: string | null
  notes: string | null
  updated_at?: string | null
}

export type ReviewKind = "disagreement" | "agreement_sample"

export interface ReviewItem {
  unit_id: string
  kind: ReviewKind
  /** Frozen snapshot of the candidate's coding, taken when the batch was generated. */
  human: string[]
  /** The candidate's coding now. Equals `human` unless revised since the snapshot. */
  human_current: string[]
  revised_since_snapshot: boolean
  ai: Record<string, string[]>
  decision: Arbitration | null
  unit: Unit
  history?: Arbitration[]
}

/** Pointer to the frozen human reference of one run. */
export interface ReviewSnapshotRef {
  run_label: string
  snapshot_id: number
  sha256: string
}

/** Frozen candidate coding of a batch, taken when its prompts were generated. */
export interface HumanSnapshot {
  snapshot_id: number
  interview_id: string
  batch_id: number
  phase: Phase
  segmentation_hash: string
  codebook_hash: string
  /** unit_id -> sorted codes (empty when the unit only carried an open label). */
  codings: Record<string, string[]>
  sha256: string
  created_at: string
}

export interface ReviewAnswer {
  runs: string[]
  items: ReviewItem[]
  n_disagreements: number
  n_sampled: number
  n_decided: number
  seeds: { run_label: string; seed: string | number }[]
  snapshots: ReviewSnapshotRef[]
}

export interface CrossCaseCell {
  rq: string
  case_id: string
  /** false when the case has no attributed unit; then `codes` is empty. */
  documented: boolean
  codes: { code: string; units: string[] }[]
}

export interface CrossCaseAnswer {
  questions: ResearchQuestion[]
  cases: Case[]
  cells: CrossCaseCell[]
}

/** A unit carrying a given code, anywhere in the corpus. */
export interface CodeUnit {
  interview_id: string
  unit: Unit
  coding: HumanCoding | null
}

export interface WrittenFiles {
  files: { path: string; sha256: string; size: number }[]
}

export interface MergeAnswer {
  codebook: Codebook
  rewritten_codings: number
  rewritten_decisions: number
}

export interface ResolveAnswer {
  resolved: ToClarify[]
  codebook: Codebook
}

/** Fields of a code the candidate may edit (omitted = unchanged). */
export interface CodeEdit {
  label?: string
  definition?: string
  toe?: Toe | null
  research_questions?: string[]
  example?: string | null
  reason?: string | null
  in_prompt?: boolean
}

export interface AuditEvent {
  seq: number
  ts: string
  actor: string
  action: string
  interview_id: string | null
  target: string | null
  detail: unknown
  prev_hash: string
  hash: string
}

export interface ChainCheck {
  ok: boolean
  n_events: number
  first_bad_seq: number | null
}

export interface PhaseSummary {
  phase: Phase
  n_units: number
  n_human_coded: number
  human_complete: boolean
  n_ai_runs: number
  n_ai_runs_complete: number
  n_divergences: number
  n_sampled?: number
  n_arbitrated: number
  n_to_clarify?: number
  reconciled: boolean
}

export interface InterviewSummary {
  interview_id: string
  folder_name: string
  n_sources: number
  primary_source: string | null
  n_units: number
  n_warnings: number
  phase1: PhaseSummary
  phase2: PhaseSummary
}

export interface GuardStatus {
  phase: Phase
  complete: boolean
  n_units: number
  missing: string[]
}

// ---------------------------------------------------------------- answers

export interface AppState {
  version: string
  db_path: string
  library_root: string | null
  coding_db: string | null
  actor: string
  codebook_size: number
  chain: ChainCheck
  native_dialogs: boolean
}

export interface InterviewDetail {
  entry: InterviewEntry
  segmentation: Segmentation | null
  phase1: PhaseSummary
  phase2: PhaseSummary
}

export interface FileView {
  path: string
  kind: SourceKind
  sha256: string
  changed_since_scan: boolean
  text: string
}

export interface PhaseState {
  phase: Phase
  summary: PhaseSummary
  guard: GuardStatus
}

export interface UnitRow {
  unit: Unit
  coding: HumanCoding | null
  allowed: Code[]
  /** Phase 2: the unit's current phase 1 contexts. */
  phase1?: string[] | null
  /** Phase 2: false when phase 1 now says NA (coding kept, excluded from new prompts). */
  in_scope?: boolean
  to_clarify?: ToClarify[]
  /** Phase 2: qualifications of the statement(s) inside the unit. */
  claims?: Claim[]
  /** Phase 2: current flags of the unit (subset of UNIT_FLAGS). */
  flags?: string[]
  /** Phase 2: state as computed by the server; `stale` when the unit changed since. */
  verification?: { state: VerificationState; stale: boolean }
}

// ------------------------------------------------- verification, flags, claims

/** Closed lists, mirrored from crates/cs-model/src/lib.rs. */
export const UNIT_FLAGS = ["VERIFY", "SECOND-HAND", "UNFINISHED", "CO-CONSTRUCTED"] as const
export type UnitFlag = (typeof UNIT_FLAGS)[number]
export const CO_CONSTRUCTED_KINDS = ["assent", "content"] as const
export type CoConstructedKind = (typeof CO_CONSTRUCTED_KINDS)[number]
export const VERIFICATION_STATES = ["unverified", "verified", "unusable"] as const
export type VerificationState = (typeof VERIFICATION_STATES)[number]
export const CLAIM_ROLES = ["trigger", "retrospective-justification", "enabling-condition", "constraint", "undetermined"] as const
export type ClaimRole = (typeof CLAIM_ROLES)[number]
/** Reasons a unit is a verification target. */
export type VerificationReason = "quoted" | "flagged" | "disagreement" | "case" | "sample"

export interface UnitFlags {
  interview_id: string
  unit_id: string
  flags: string[]
  co_constructed_kind: CoConstructedKind | null
  updated_at: string
}

export interface UnitCorrection {
  correction_id: number
  interview_id: string
  unit_id: string
  text: string
  role: SpeakerRole
  reason: string
  created_at: string
  created_by: string
}

export interface VerificationStatus {
  unit_id: string
  state: VerificationState
  stale: boolean
  at: string | null
  by: string | null
  note: string | null
}

export interface VerificationTarget {
  unit_id: string
  reasons: VerificationReason[]
}

export interface VerificationCoverage {
  n_units: number
  n_verified: number
  n_unusable: number
  n_stale: number
  n_targets: number
  n_targets_verified: number
}

export interface VerificationAnswer {
  status: VerificationStatus[]
  targets: VerificationTarget[]
  coverage: VerificationCoverage
}

/** A qualification of a statement inside a unit. Excerpt is an exact substring of the unit text. */
export interface Claim {
  claim_id: number
  interview_id: string
  unit_id: string
  excerpt: string
  codes: string[]
  ev_level: string | null
  provenance: string | null
  /** One of CLAIM_ROLES (typed as string so a future server value still renders). */
  role: string | null
  memo: string | null
  created_at: string
  updated_at: string
}

/** Body of a claim create or edit: server-set fields may be omitted. */
export type ClaimInput = Pick<Claim, "unit_id" | "excerpt" | "codes" | "ev_level" | "provenance" | "role" | "memo"> &
  Partial<Pick<Claim, "claim_id" | "interview_id">>

export interface ClaimReview {
  claim_id: number
  reason: string
}

export interface RqMemo {
  rq_id: string
  documented: string
  interpretation: string
  rivals: string
  limits: string
  units_for: string[]
  units_against: string[]
  updated_at: string
}

export interface SaturationRow {
  interview_id: string
  order: number
  first_coded_at: string
  codes_first_used: string[]
  field_codes_created: string[]
  labels_opened: number
  labels_resolved: number
}

export interface AgreementAnswer {
  reports: AgreementReport[]
  markdown: string
}

export interface DivergenceItem {
  divergence: Divergence
  unit: Unit
  arbitration: Arbitration | null
  history: Arbitration[]
  allowed: Code[]
}

export interface DivergencesAnswer {
  items: DivergenceItem[]
  n_divergences: number
  n_decided: number
  runs: string[]
}

export interface FinalAnswer {
  finals: FinalCoding[]
  uncoded: string[]
  n_decided: number
  n_open_disagreements: number
}

export interface ExportAnswer {
  dir: string
  files: { name: string; sha256: string; size: number }[]
  manifest_sha256?: string
  /** Coding status at export time (crates/cs-app/src/export.rs, CodingStatus). */
  status?: {
    n_units_phase1: number
    n_coded_phase1: number
    n_units_phase2: number
    n_coded_phase2: number
    n_runs: number
    n_complete_runs: number
    n_disagreements: number
    n_sampled: number
    n_decided: number
    n_revised_after_ai: number
    n_to_clarify_open: number
    n_cases: number
  }
}

// ------------------------------------------------- study configuration (v5)

export type FrameworkPreset = "toe" | "tam_utaut" | "none" | "custom"

export interface Category {
  code: string
  label: string
  definition: string
  /** TOE letter that phase 2 codes attached to the category carry, or null. */
  attachment: string | null
}

export interface PromptTemplate {
  version: string
  header: string
  codebook_intro: string
  units_intro: string
  footer: string
}

export interface StudyConfig {
  framework: FrameworkPreset
  phase1_categories: Category[]
  id_prefixes: string[]
  interviewers: string[]
  max_words: number
  prompt_template: PromptTemplate
  study_name: string
}

/** `POST /api/codebook/import` answers the codebook plus a report of the import. */
export interface CodebookImportAnswer extends Codebook {
  report: { added: number; updated: number; absent: number }
}

// ----------------------------------------------------------- providers (v5)

export type Execution = { kind: "local" } | { kind: "hosted"; vendor: string; endpoint: string }
export type PromptVia = "stdin" | "arg"
export type Preset = "openai" | "zai" | "custom"
export type ProviderKind = "copy_paste" | "cli" | "open_ai_compatible" | "oauth_chat_gpt"

export interface CliProviderConfig {
  type: "cli"
  id: string
  command: string
  args: string[]
  prompt_via: PromptVia
  disable_flags: string[]
  declared_execution: Execution
  model_label: string | null
}

export interface OpenAiProviderConfig {
  type: "open_ai_compatible"
  id: string
  preset: Preset
  base_url: string | null
  /** Name of the environment variable that holds the key, never the key. */
  api_key_env: string
  model: string
}

export interface OAuthProviderConfig {
  type: "oauth_chat_gpt"
  id: string
  profile_id: string | null
  model: string | null
  token_dir: string | null
}

export type ProviderConfig = CliProviderConfig | OpenAiProviderConfig | OAuthProviderConfig

export interface ProvidersConfig {
  providers: ProviderConfig[]
}

export interface ProviderDescription {
  id: string
  kind: ProviderKind
  sends: string[]
  execution: Execution
}

/** One row of `GET /api/providers`; `copy_paste` is first and has no config. */
export interface ProviderEntry {
  id: string
  kind: ProviderKind
  description: ProviderDescription
  configured: boolean
  config: ProviderConfig | null
}

export interface IsolationReport {
  ok: boolean
  cwd_empty: boolean
  env_scrubbed: boolean
  instruction_files_absent: boolean
  probe_answer: string
}

/** Body of `POST /api/interviews/{id}/batches/{batch_id}/run`. */
export interface ProviderRunBody {
  provider_id: string
  run_label: string
  model?: string
  temperature?: number
  seed?: number
  max_tokens?: number
}

export interface RunProviderRecord {
  id: string
  execution: Execution
  parameters_exposed: string[]
  parameters_not_exposed: string[]
  parts: { part: number; request_sha256: string; started_at: string; finished_at: string }[]
}

export interface ProviderRunAnswer {
  run: AiRun
  /** "command local, model hosted", "local" or "hosted (vendor, endpoint)". */
  journal_line: string
}

/** A raw answer refused by the validation, returned in the 400 body. */
export interface RejectedAnswer {
  part: number
  raw: string
}

export interface OAuthBegin {
  url: string
  state: string
}

export interface OAuthStatus {
  connected: boolean
  pending: boolean
  profile: { id: string; email: string } | null
  error: string | null
}

// ------------------------------------------------ V1 lots 1 and 2 (docs/API.md)

/** A code applied to a passage. `system` is true for the reserved "Plus tard". */
export interface PassageCoding {
  coding_id: number
  code_id: string
  code_name: string
  system: boolean
  note: string
  created_at: string
  created_by: string
}

/** A selected part of a unit. Offsets count Unicode characters; the `_utf16` forms are those of a browser selection. */
/** A respondent unit marked "à revoir" (R in the Coder). */
export interface UnitMark {
  interview_id: string
  unit_id: string
  mark: "to_review"
  note: string
  at: string
  by: string
}

export interface Passage {
  passage_id: string
  interview_id: string
  unit_id: string
  start: number
  end: number
  start_utf16?: number
  end_utf16?: number
  text: string
  whole_unit: boolean
  segmentation_hash: string
  anchor_digest: string
  created_at: string
  created_by: string
  kind: "highlight" | "coded"
  stale: boolean
  stale_reason?: string
  before: string
  after: string
  codings: PassageCoding[]
  /** How the passage came to be said (`PASSAGE_ORIGINS`); null until qualified. */
  origin?: string | null
  /** Role of the passage in a decision (`PASSAGE_ROLES`); null until qualified. */
  role?: string | null
}

export interface PassageCreateAnswer {
  passage: Passage
  created: boolean
}

export interface CodeAlias {
  alias: string
  reason: string
  created_at: string
}

export interface CodeCounts {
  passages: number
  interviews: number
  passages_rollup: number
  interviews_rollup: number
}

/** A code of the tree. `code_id` is stable; `token` is the legacy upper-case handle. */
export interface CodeNode {
  code_id: string
  token: string
  name: string
  label: string
  definition: string
  includes: string
  excludes: string
  parent_id: string | null
  origin: string
  toe: string | null
  research_questions: string[]
  active: boolean
  system: boolean
  in_prompt: boolean
  merged_into: string | null
  version: number
  example: string | null
  reason: string | null
  /** The memo of the code, apart from the reason of its last change. */
  memo?: string
  origin_passage_id: string | null
  example_passage_id: string | null
  created_at: string | null
  aliases: CodeAlias[]
  counts: CodeCounts
}

export interface CodeTreeAnswer {
  codebook_version: number
  filter?: unknown
  codes: CodeNode[]
}

export interface CodeEditV1 {
  name?: string
  label?: string
  definition?: string
  includes?: string
  excludes?: string
  research_questions?: string[]
  example?: string | null
  reason?: string | null
}

export interface MergeAnswerV1 {
  outcome: { moved_passage_codings: number; deduplicated_passage_codings: number }
  merged: string
  code: CodeNode
}

export interface FromPassageBody {
  name: string
  definition: string
  includes?: string
  excludes?: string
  parent_id?: string | null
  passage_id?: string
  passage?: { interview_id: string; unit_id: string; start?: number; end?: number; offset_unit?: "utf16" | "scalar" }
}

export interface PassageContext {
  passage: Passage
  speaker: string
  previous: { unit_id: string; speaker: string; text: string } | null
  next: { unit_id: string; speaker: string; text: string } | null
  other_codes: string[]
}

export interface CodebookRevision {
  rev: number
  ts: string
  actor: string
  reason: string
  target: string
}

export type AttributeKind = "text" | "choice" | "number" | "date"

export interface AttributeDef {
  id: string
  label: string
  kind: AttributeKind
  choices: string[]
  shared_with_model: boolean
}

export interface MemoQuestionDef {
  id: string
  text: string
}

export interface MemoTemplate {
  id: string
  name: string
  default: boolean
  version: number
  questions: MemoQuestionDef[]
}

export interface ListItem {
  id: string
  label: string
}

export interface FrameworkDimension {
  id: string
  label: string
  definition: string
}

export interface Framework {
  preset: "toe" | "tam_utaut" | "custom"
  dimensions: FrameworkDimension[]
}

export interface ProjectConfig {
  attributes: AttributeDef[]
  memo_templates: MemoTemplate[]
  checklist: ListItem[]
  theme_checks: ListItem[]
  evidence_natures: ListItem[]
  link_types: ListItem[]
  matrix_dimensions: ListItem[]
  interpretive_framework: Framework | null
  memo_history?: unknown[]
}

export interface ProjectPresets {
  frameworks: { toe: Framework; tam_utaut: Framework }
  defaults: Partial<ProjectConfig>
}

export interface InterviewAttributes {
  interview_id: string
  definitions: AttributeDef[]
  values: Record<string, string>
}

export interface MemoAnswerV1 {
  text: string
  at: string
}

export interface MemoQuestionView {
  id: string
  text: string
  source: "template" | "added"
  reworded: boolean
  answer: MemoAnswerV1 | null
}

export interface MemoOverrides {
  added: { id?: string; text: string }[]
  removed: string[]
  reworded: Record<string, string>
}

export interface MemoView {
  interview_id: string
  template_id: string
  template_name: string
  template_version: number
  current_version: number
  outdated: boolean
  questions: MemoQuestionView[]
  removed: MemoQuestionView[]
  orphan_answers: Record<string, MemoAnswerV1>
  overrides: MemoOverrides
  saved: boolean
}

export interface ChecklistItemView {
  id: string
  label: string
  checked: boolean
  checked_at: string | null
}

export interface ChecklistView {
  interview_id: string
  items: ChecklistItemView[]
  note: string
  complete: boolean
}

export interface TemplateChange {
  part: string
  action: string
  detail: string
}

export interface TemplateReport {
  dry_run: boolean
  applied: boolean
  changes: TemplateChange[]
  problems: string[]
}

export interface LaterAnswer {
  summary: { interview_id: string; n: number }[]
  items: Passage[]
}
