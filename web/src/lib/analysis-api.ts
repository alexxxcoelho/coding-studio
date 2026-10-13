// Client of the Analyser and Rédiger screens (docs/SPEC.md 4.7, 4.8, 4.9).
//
// Everything the two screens ask of the server goes through `analysisApi`, and
// every shape they rely on is declared here. The routes are those of
// `crates/cs-app/src/v1_analysis.rs` (docs/API.md, "V1 lot 5 and gaps"); the
// closed lists (statuses, cell values, parts, verdicts, origins, roles,
// outcomes) are those of `cs-model/src/analysis.rs`. The matrix goes through
// the native endpoint `POST /api/matrix/compute`: the screens keep a one-axis
// setting per side, converted here (`toNativeConfig`, `fromNativeConfig`).

import { request } from "./api"
import type { Case, CodeNode, Passage } from "./types"

const enc = encodeURIComponent

// ------------------------------------------------------------ closed lists

export const THEME_STATUSES = ["candidate", "revised", "retained"] as const
export type ThemeStatus = (typeof THEME_STATUSES)[number]

export const CELL_VALUES = ["present", "not_mentioned", "contradicted", "uncertain"] as const
export type CellValue = (typeof CELL_VALUES)[number]

export const THEME_PARTS = [
  { key: "analytical_statement", title: "Énoncé analytique" },
  { key: "mechanism_evidence", title: "Mécanisme et preuves" },
  { key: "variations_divergent", title: "Variations et cas divergents" },
  { key: "competing_limits", title: "Explications concurrentes et limites" },
  { key: "mini_synthesis", title: "Mini-synthèse" },
] as const
export type ThemePartKey = (typeof THEME_PARTS)[number]["key"]

export const AUDIT_VERDICTS = ["supported", "to_nuance", "to_withdraw"] as const
export type AuditVerdict = (typeof AUDIT_VERDICTS)[number]

export type VerificationBadge = "verified" | "unverified" | "unusable"

// ------------------------------------------------------------------ themes

/** A row of the themes list. The title is a sentence (an analytical proposition). */
export interface ThemeSummary {
  theme_id: string
  title: string
  status: ThemeStatus
  /** Position in the chapter. */
  ord: number
  /** Supporting codes, distinct. */
  codes_count: number
  /** Distinct interviews carrying at least one passage of the supporting codes. */
  interviews_count: number
  /** Distinct passages of the supporting codes (descendants included). */
  passages_count?: number
  research_questions: string[]
  checks_done?: number
  checks_total?: number
}

export interface ThemeCheckState {
  check_id: string
  /** The project's wording of the check. */
  label?: string
  done: boolean
  note: string
}

export interface ThemeCode {
  code_id: string
  name: string
  passages: number
  interviews: number
}

export interface ThemeSheet extends ThemeSummary {
  proposition: string
  conditions: string
  alternative_interpretation: string
  limits: string
  decision_memo: string
  codes: ThemeCode[]
  /** Case ids that support the theme / that diverge from it. */
  supporting_cases: string[]
  divergent_cases: string[]
  checks: ThemeCheckState[]
  /** What the server finds missing in the sheet, in words. */
  warnings?: string[]
  updated_at: string
}

/** The text fields of the sheet, as sent by a save. */
export type ThemeSheetEdit = Partial<
  Pick<ThemeSheet, "title" | "proposition" | "conditions" | "alternative_interpretation" | "limits" | "decision_memo" | "supporting_cases" | "divergent_cases" | "research_questions">
>

export interface ThemeHistoryEntry {
  kind: "created" | "title" | "status"
  before: string | null
  after: string | null
  reason: string
  override: boolean
  at: string
  by: string
}

/** A passage of a theme: carried by one of its supporting codes (descendants included). */
export interface PassageBrief {
  passage_id: string
  interview_id: string
  unit_id: string
  text: string
  /** Exact audio verification state of the unit; null when not asked. */
  verification: VerificationBadge | null
  /** True when the text of the interview changed since the passage was anchored. */
  stale?: boolean
}

export interface ThemePassage extends PassageBrief {
  code_ids: string[]
  /** Value of the cell (interview, theme) of the matrix, when the researcher set one. */
  cell_value: CellValue | null
}

/** Status change. `override_reason` is required when a check is not done and the target is retained. */
export interface StatusChange {
  status: ThemeStatus
  reason?: string
  override_reason?: string
}

// ---------------------------------------------------------------- framework

export interface FrameworkCellReading {
  explains: boolean
  note: string
}

/** Reading of a theme by the framework of the study (Analyser and Rédiger only). */
export interface FrameworkReading {
  theme_id: string
  cells: Record<string, FrameworkCellReading>
  out_of_frame: boolean
  out_of_frame_note: string
  complementary_dimension: string
  evidence_limit: string
}

export interface FrameworkTable {
  rows: (FrameworkReading & { title: string })[]
}

// ------------------------------------------------------------------- matrix

/**
 * `focused_codes` lists the codes that have sub-codes, or every code when none has any;
 * `dimensions` lists the dimensions of the study.
 */
export type AxisKind = "interviews" | "cases" | "attribute" | "themes" | "codes" | "focused_codes" | "dimensions"

/** What one side of the matrix lists. `id` is the attribute id of an `attribute` side. */
export interface AxisSource {
  kind: AxisKind
  id?: string
}

export interface MatrixConfig {
  rows: AxisSource
  cols: AxisSource
  /** Keeps the interviews whose attribute has this value. */
  filter: { attribute: string; value: string } | null
}

export interface MatrixView {
  view_id: string
  name: string
  config: MatrixConfig
}

export interface MatrixAxisEntry {
  /** Stable key, for example `interview:ECO-07`, `theme:T-0001`. */
  key: string
  label: string
  /** False for an interview that has no final coding yet: its cells read "pas encore codé". */
  coded: boolean
}

export interface MatrixCell {
  row_key: string
  col_key: string
  /** Set by the researcher; null until then. */
  value: CellValue | null
  summary: string
  n_passages: number
  n_interviews: number
  /** True when passages were attached to the cell by hand. */
  manual?: boolean
}

export interface MatrixResult {
  rows: MatrixAxisEntry[]
  cols: MatrixAxisEntry[]
  cells: MatrixCell[]
  interviews_in_scope?: string[]
}

export interface CellPassage extends PassageBrief {
  speaker?: string
}

export interface CellDetail {
  row_key: string
  col_key: string
  row_label: string
  col_label: string
  value: CellValue | null
  summary: string
  passages: CellPassage[]
  n_interviews: number
  manual?: boolean
}

export interface CellEdit {
  row_key: string
  col_key: string
  value: CellValue | null
  summary: string
}

// -------------------------------------------------------------------- links

export type LinkTarget = { kind: "theme_part"; theme_id: string; part: ThemePartKey } | { kind: "rq_part"; rq_id: string }

/** A typed link between a passage and a part of the text. */
export interface PassageLink {
  link_id: string
  passage: PassageBrief
  target: LinkTarget
  link_type: string
  note: string
}

export interface LinkCreate {
  passage_id: string
  target: LinkTarget
  link_type: string
  note?: string
}

// ------------------------------------------------------------------ writing

export interface PartState {
  text: string
  nature_label?: string | null
  /** Saved versions of the part; earlier ones are kept. */
  n_versions?: number
  /** Id of the evidence nature, from the project list; null until chosen. */
  nature: string | null
  updated_at: string | null
  links: PassageLink[]
}

export interface ThemeWriting {
  theme_id: string
  title: string
  status: ThemeStatus
  research_questions: string[]
  parts: Record<ThemePartKey, PartState>
}

/** A save of a part: what is sent changes, what is left out stays as it is. */
export interface PartEdit {
  text?: string
  nature?: string | null
}

export interface QuestionSynthesis {
  rq_id: string
  text: string
  /** Themes that answer the question, with their mini-synthesis. */
  themes: { theme_id: string; title: string; mini_synthesis: string }[]
  answer: string
  limits: string
  updated_at: string | null
  /** Earlier versions are kept. */
  n_versions: number
  links?: PassageLink[]
}

export interface SynthesisEdit {
  answer: string
  limits: string
}

export interface WritingExport {
  dir: string | null
  files: { name: string; sha256: string; size: number }[]
  markdown: string
}

// -------------------------------------------------------------- final audit

export type GapKind = "no_link" | "unverified_passage" | "stale_passage" | "no_text"

export interface AuditGap {
  kind: GapKind
  message: string
}

export interface AuditChainLink {
  link_id: string
  link_type: string
  note: string
  passage: PassageBrief
  /** Version of the interview text the passage was anchored on. */
  interview_version: string | null
}

export interface AuditClaim {
  claim_key: string
  /** The whole written part the sentence is drawn from. */
  text?: string
  chain_sha256?: string
  /** For example "Mécanisme et preuves". */
  part_title: string
  /** The sentence drawn from the written text. */
  sentence: string
  theme: { theme_id: string; title: string } | null
  rq_id: string | null
  codes: { code_id: string; name: string }[]
  links: AuditChainLink[]
  gaps: AuditGap[]
  verdict: { verdict: AuditVerdict; note: string; at: string } | null
}

export interface FinalAudit {
  audit_id: number
  seed: string
  n: number
  created_at: string
  /** Claims the draw could choose from. */
  eligible?: number
  claims: AuditClaim[]
}

// ------------------------------------------------- native matrix (compute)

interface NativeAxis {
  kind: string
  ids?: string[]
  attribute?: string | null
  status?: string | null
}
interface NativeConfig {
  rows: NativeAxis[]
  cols: NativeAxis[]
  filters: { attribute: string; values: string[] }[]
}
interface NativeCell {
  row_key: string
  col_key: string
  value: CellValue | null
  summary: string
  passages: number
  interviews: number
  manual: boolean
}
interface NativeMatrix {
  rows: MatrixAxisEntry[]
  cols: MatrixAxisEntry[]
  cells: NativeCell[][]
  interviews_in_scope: string[]
}
interface StoredView {
  view_id: string
  name: string
  native_config: NativeConfig
}

function toNativeAxis(a: AxisSource): NativeAxis {
  return a.kind === "attribute" ? { kind: "attribute", attribute: a.id ?? null } : { kind: a.kind, ids: a.id ? [a.id] : [] }
}

function fromNativeAxis(a: NativeAxis | undefined): AxisSource {
  if (!a) return { kind: "interviews" }
  if (a.kind === "attribute") return { kind: "attribute", id: a.attribute ?? undefined }
  return { kind: a.kind as AxisKind }
}

/** The one-axis setting of the screens as the native configuration of the server. */
export function toNativeConfig(c: MatrixConfig): NativeConfig {
  return {
    rows: [toNativeAxis(c.rows)],
    cols: [toNativeAxis(c.cols)],
    // A filter whose value is not chosen yet keeps everything.
    filters: c.filter && c.filter.value ? [{ attribute: c.filter.attribute, values: [c.filter.value] }] : [],
  }
}

/** The first axis of each side, and the first value of the first filter: what the screens can show. */
export function fromNativeConfig(n: NativeConfig): MatrixConfig {
  const f = n.filters[0]
  return { rows: fromNativeAxis(n.rows[0]), cols: fromNativeAxis(n.cols[0]), filter: f && f.values[0] !== undefined ? { attribute: f.attribute, value: f.values[0] } : null }
}

function fromStoredView(v: StoredView): MatrixView {
  return { view_id: v.view_id, name: v.name, config: fromNativeConfig(v.native_config) }
}

function fromNativeMatrix(m: NativeMatrix): MatrixResult {
  return {
    rows: m.rows,
    cols: m.cols,
    interviews_in_scope: m.interviews_in_scope,
    cells: m.cells.flat().map((c) => ({
      row_key: c.row_key,
      col_key: c.col_key,
      value: c.value,
      summary: c.summary,
      n_passages: c.passages,
      n_interviews: c.interviews,
      manual: c.manual,
    })),
  }
}

// -------------------------------------------------------- decisions, gaps

export const DECISION_OUTCOMES = [
  { value: "adopt", label: "Adopter" },
  { value: "defer", label: "Différer" },
  { value: "reject", label: "Refuser" },
] as const
export type DecisionOutcome = (typeof DECISION_OUTCOMES)[number]["value"]

/** A documented decision as `/api/decisions` answers: a case, with the words of the guide beside it. */
export interface DecisionRecord extends Case {
  decision_id: string
  name: string
  /** `adopt`, `defer` or `reject`; null until chosen. */
  outcome: DecisionOutcome | null
  outcome_label: string | null
  /** Passages that document the decision. */
  passages: string[]
  characteristics: Record<string, string>
}

export const PASSAGE_ORIGINS = [
  { value: "lived_experience", label: "Expérience vécue" },
  { value: "project", label: "Projet" },
  { value: "opinion", label: "Opinion" },
  { value: "document", label: "Document" },
] as const

export const PASSAGE_ROLES = [
  { value: "trigger", label: "Déclencheur" },
  { value: "retrospective-justification", label: "Justification après coup" },
  { value: "enabling-condition", label: "Condition facilitatrice" },
  { value: "constraint", label: "Contrainte" },
  { value: "undetermined", label: "Indéterminé" },
] as const

/** Answer of `GET/PUT /api/codes/{id}/memo`: the memo and its append-only history. */
export interface CodeMemo {
  code_id: string
  memo: string
  history: { memo_id: number; text: string; at: string; by: string }[]
}

export interface UndoCreate {
  code_deleted: boolean
  passage_deleted: boolean
}

// ---------------------------------------------------------------- the client

export const analysisApi = {
  // ---- themes
  themes: () => request<ThemeSummary[]>("GET", "/api/themes"),
  createTheme: (title: string) => request<ThemeSheet>("POST", "/api/themes", { title }),
  theme: (id: string) => request<ThemeSheet>("GET", `/api/themes/${enc(id)}`),
  updateTheme: (id: string, edit: ThemeSheetEdit) => request<ThemeSheet>("PUT", `/api/themes/${enc(id)}`, edit),
  /** 409 when the target is retained and a check is not done without `override_reason`. */
  setThemeStatus: (id: string, change: StatusChange) => request<ThemeSheet>("POST", `/api/themes/${enc(id)}/status`, change),
  setThemeCheck: (id: string, checkId: string, done: boolean, note = "") =>
    request<ThemeSheet>("PUT", `/api/themes/${enc(id)}/checks/${enc(checkId)}`, { done, note }),
  addThemeCode: (id: string, codeId: string) => request<ThemeSheet>("POST", `/api/themes/${enc(id)}/codes`, { code_id: codeId }),
  removeThemeCode: (id: string, codeId: string) => request<ThemeSheet>("DELETE", `/api/themes/${enc(id)}/codes/${enc(codeId)}`),
  themePassages: (id: string) => request<ThemePassage[]>("GET", `/api/themes/${enc(id)}/passages`),
  themeHistory: (id: string) => request<ThemeHistoryEntry[]>("GET", `/api/themes/${enc(id)}/history`),
  /** Order of the chapter: every theme id, in the new order. */
  reorderThemes: (ids: string[]) => request<ThemeSummary[]>("PUT", "/api/themes/order", { ids }),

  // ---- framework reading
  frameworkReading: (id: string) => request<FrameworkReading>("GET", `/api/themes/${enc(id)}/framework`),
  setFrameworkReading: (id: string, reading: Omit<FrameworkReading, "theme_id">) =>
    request<FrameworkReading>("PUT", `/api/themes/${enc(id)}/framework`, reading),
  frameworkTable: () => request<FrameworkTable>("GET", "/api/writing/framework"),

  // ---- matrix
  matrixViews: async () => (await request<StoredView[]>("GET", "/api/matrix/views")).map(fromStoredView),
  saveMatrixView: async (name: string, config: MatrixConfig) =>
    fromStoredView(await request<StoredView>("POST", "/api/matrix/views", { name, config: toNativeConfig(config) })),
  deleteMatrixView: (viewId: string) => request<{ deleted: string }>("DELETE", `/api/matrix/views/${enc(viewId)}`),
  matrix: async (config: MatrixConfig) => fromNativeMatrix(await request<NativeMatrix>("POST", "/api/matrix/compute", toNativeConfig(config))),
  /** `filter` is the one of the matrix, so that the counts of the cell are those of the grid. */
  matrixCell: (rowKey: string, colKey: string, filter?: MatrixConfig["filter"]) =>
    request<CellDetail>(
      "GET",
      `/api/matrix/cell?row=${enc(rowKey)}&col=${enc(colKey)}${filter ? `&attribute=${enc(filter.attribute)}&value=${enc(filter.value)}` : ""}`,
    ),
  setMatrixCell: (edit: CellEdit) => request<CellDetail>("PUT", "/api/matrix/cell", edit),
  /** Passages attached by hand to a cell of a study dimension (no code to compute them from). */
  attachCellPassage: (rowKey: string, colKey: string, passageId: string) =>
    request<CellDetail>("POST", "/api/matrix/cell/passages", { row_key: rowKey, col_key: colKey, passage_id: passageId }),
  detachCellPassage: (rowKey: string, colKey: string, passageId: string) =>
    request<CellDetail>("DELETE", `/api/matrix/cell/passages/${enc(passageId)}?row=${enc(rowKey)}&col=${enc(colKey)}`),

  // ---- writing, per theme
  themeWriting: (id: string) => request<ThemeWriting>("GET", `/api/writing/themes/${enc(id)}`),
  setThemePart: (id: string, part: ThemePartKey, edit: PartEdit) =>
    request<PartState>("PUT", `/api/writing/themes/${enc(id)}/parts/${enc(part)}`, edit),

  // ---- typed links
  createLink: (body: LinkCreate) => request<PassageLink>("POST", "/api/links", body),
  updateLink: (linkId: string, edit: { link_type?: string; note?: string }) =>
    request<PassageLink>("PUT", `/api/links/${enc(linkId)}`, edit),
  removeLink: (linkId: string, reason = "") =>
    request<{ removed: string }>("DELETE", `/api/links/${enc(linkId)}${reason ? `?reason=${enc(reason)}` : ""}`),

  // ---- synthesis per research question
  questionSynthesis: (rqId: string) => request<QuestionSynthesis>("GET", `/api/writing/questions/${enc(rqId)}`),
  setQuestionSynthesis: (rqId: string, edit: SynthesisEdit) =>
    request<QuestionSynthesis>("PUT", `/api/writing/questions/${enc(rqId)}`, edit),

  // ---- final audit
  latestFinalAudit: () => request<FinalAudit | null>("GET", "/api/final-audit/latest"),
  drawFinalAudit: (n = 3, seed?: string) => request<FinalAudit>("POST", "/api/final-audit", seed ? { n, seed } : { n }),
  setAuditVerdict: (auditId: number, claimKey: string, verdict: AuditVerdict, note: string) =>
    request<FinalAudit>("POST", `/api/final-audit/${auditId}/verdicts`, { claim_key: claimKey, verdict, note }),

  // ---- documented decisions
  decisions: () => request<DecisionRecord[]>("GET", "/api/decisions"),
  decision: (id: string) => request<DecisionRecord>("GET", `/api/decisions/${enc(id)}`),
  updateDecision: (id: string, body: Partial<DecisionRecord>) => request<DecisionRecord>("PUT", `/api/decisions/${enc(id)}`, body),
  createDecision: (interviewId: string, name: string, outcome: DecisionOutcome) =>
    request<DecisionRecord>("POST", `/api/interviews/${enc(interviewId)}/decisions`, { label: name, outcome }),

  // ---- codes and passages (gaps 2, 6, 7, 8)
  createCode: (body: { name: string; definition: string; includes?: string; excludes?: string; parent_id?: string | null }) =>
    request<CodeNode>("POST", "/api/codes", body),
  codeMemo: (id: string) => request<CodeMemo>("GET", `/api/codes/${enc(id)}/memo`),
  setCodeMemo: (id: string, text: string) => request<CodeMemo>("PUT", `/api/codes/${enc(id)}/memo`, { text }),
  undoCreateCode: (id: string) => request<UndoCreate>("POST", `/api/codes/${enc(id)}/undo-create`),
  qualifyPassage: (pid: string, q: { origin?: string | null; role?: string | null }) =>
    request<Passage>("PUT", `/api/passages/${enc(pid)}/qualification`, q),

  // ---- export
  exportWriting: () => request<WritingExport>("POST", "/api/writing/export", { format: "markdown" }),
}
