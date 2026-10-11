// TypeScript mirror of crates/cs-model/src/lib.rs (serde, snake_case).
// Sets (BTreeSet) arrive as sorted arrays.

export type Phase = 1 | 2

export type SourceKind =
  | "vtt"
  | "timestamped_text"
  | "cleaned_markdown"
  | "budgie_markdown"
  | "written_response"
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

export interface Unit {
  unit_id: string
  seq: number
  speaker_raw: string
  role: SpeakerRole
  timestamp: string
  text: string
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

export interface Code {
  code: string
  family: string
  label: string
  definition: string
  origin: string
  active: boolean
  in_prompt: boolean
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
}

export interface PromptPart {
  interview_id: string
  phase: Phase
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

export type FinalBasis = "agreed" | "arbitrated"

export interface FinalCoding {
  unit_id: string
  codes: string[]
  basis: FinalBasis
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
  n_arbitrated: number
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
  phase2_started: boolean
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
  reconciled: boolean
  frozen: boolean
  locked: boolean
}

export interface UnitRow {
  unit: Unit
  coding: HumanCoding | null
  allowed: Code[]
  phase1_final: string[] | null
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
  reconciled: boolean
  finals: FinalCoding[]
  pending: string[]
}

export interface ExportAnswer {
  dir: string
  files: { name: string; sha256: string; size: number }[]
  manifest_sha256?: string
  phases?: {
    phase: Phase
    n_units: number
    n_runs: number
    n_complete_runs: number
    n_divergences: number
    reconciled: boolean
    pending_units: string[] | number
  }[]
}
