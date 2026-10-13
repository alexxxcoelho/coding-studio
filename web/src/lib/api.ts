// Typed client for the local Coding Studio API (docs/API.md).

import type { LibraryProgress } from "./pipeline"
import type {
  AgreementAnswer,
  AiRun,
  AiRunMeta,
  AppState,
  Arbitration,
  ArbitrationChoice,
  AuditEvent,
  Case,
  ChecklistView,
  CodeEditV1,
  CodeNode,
  CodeTreeAnswer,
  CodebookRevision,
  FromPassageBody,
  InterviewAttributes,
  LaterAnswer,
  MemoOverrides,
  MemoView,
  MergeAnswerV1,
  Passage,
  PassageContext,
  PassageCreateAnswer,
  ProjectConfig,
  ProjectPresets,
  TemplateReport,
  UnitMark,
  ChainCheck,
  Claim,
  ClaimInput,
  ClaimReview,
  Code,
  Codebook,
  CodeEdit,
  CodeUnit,
  CoConstructedKind,
  CodebookImportAnswer,
  ExportAnswer,
  FileView,
  FinalAnswer,
  HumanCoding,
  HumanSnapshot,
  InterviewDetail,
  InterviewEntry,
  InterviewSummary,
  IsolationReport,
  MergeAnswer,
  OAuthBegin,
  OAuthStatus,
  Phase,
  PhaseState,
  PromptPart,
  ProviderEntry,
  ProviderRunAnswer,
  ProviderRunBody,
  ProvidersConfig,
  RejectedAnswer,
  ResearchQuestion,
  ResolveAnswer,
  ReviewAnswer,
  RqMemo,
  SaturationRow,
  Segmentation,
  SpeakerRole,
  StudyConfig,
  ToClarify,
  UnitCorrection,
  UnitFlags,
  UnitRow,
  ValidationIssue,
  VerificationAnswer,
  VerificationState,
  WrittenFiles,
} from "./types"

export type ErrorKind = "validation" | "guard" | "not_found" | "internal" | "network"

/** Error answered by the server, or a network failure. */
export class ApiError extends Error {
  readonly status: number
  readonly kind: ErrorKind
  readonly issues: ValidationIssue[]
  /** Raw provider answers refused by the validation (400 of a provider run). */
  readonly rejected: RejectedAnswer[]

  constructor(status: number, kind: ErrorKind, message: string, issues: ValidationIssue[] = [], rejected: RejectedAnswer[] = []) {
    super(message)
    this.name = "ApiError"
    this.status = status
    this.kind = kind
    this.issues = issues
    this.rejected = rejected
  }

  /** A protocol lock refused the action (HTTP 409). */
  get isGuard(): boolean {
    return this.kind === "guard"
  }
}

const KINDS: ErrorKind[] = ["validation", "guard", "not_found", "internal"]

function kindFromStatus(status: number): ErrorKind {
  if (status === 409) return "guard"
  if (status === 404) return "not_found"
  if (status >= 400 && status < 500) return "validation"
  return "internal"
}

/** Builds an ApiError from a failed response body (JSON or text). */
export function toApiError(status: number, body: unknown): ApiError {
  if (body && typeof body === "object" && "error" in body) {
    const b = body as { error: unknown; kind?: unknown; issues?: unknown; rejected?: unknown }
    const kind = KINDS.includes(b.kind as ErrorKind) ? (b.kind as ErrorKind) : kindFromStatus(status)
    const issues = Array.isArray(b.issues) ? (b.issues as ValidationIssue[]) : []
    const rejected = Array.isArray(b.rejected) ? (b.rejected as RejectedAnswer[]) : []
    return new ApiError(status, kind, String(b.error), issues, rejected)
  }
  const text = typeof body === "string" && body.trim() ? body.trim() : `Request failed with status ${status}`
  return new ApiError(status, kindFromStatus(status), text)
}

declare global {
  interface Window {
    /** Origin of the API, injected by the desktop shell (Tauri) before the page loads. */
    __CS_API_BASE__?: string
  }
}

/**
 * Origin of the API, without a trailing slash. The desktop shell serves the UI
 * from its own origin (tauri://localhost, or the Vite dev server) and runs the
 * server on a loopback port that it announces in `window.__CS_API_BASE__`. Served
 * by the `coding-studio` binary or proxied by Vite, the base is empty: same origin.
 */
export function apiBase(w: { __CS_API_BASE__?: unknown } = window): string {
  const base = w.__CS_API_BASE__
  return typeof base === "string" ? base.trim().replace(/\/+$/, "") : ""
}

/** The URL of an API path such as `/api/state`. Every request goes through it. */
export function apiUrl(path: string, base: string = apiBase()): string {
  return `${base}${path}`
}

export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response
  try {
    res = await fetch(apiUrl(path), {
      method,
      headers: body === undefined ? undefined : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    throw new ApiError(0, "network", "The Coding Studio server cannot be reached. Is it still running?")
  }
  const text = await res.text()
  let parsed: unknown = text
  if (text) {
    try {
      parsed = JSON.parse(text)
    } catch {
      parsed = text
    }
  }
  if (!res.ok) throw toApiError(res.status, parsed)
  return (text ? parsed : null) as T
}

const enc = encodeURIComponent
const iv = (id: string) => `/api/interviews/${enc(id)}`
const ph = (id: string, p: Phase) => `${iv(id)}/phases/${p}`
/** AI pass, review and final coding exist in phase 2 only. */
const p2 = (id: string) => ph(id, 2)

export const api = {
  state: () => request<AppState>("GET", "/api/state"),
  pick: (kind: "folder" | "file", title: string) =>
    request<{ path: string | null }>("POST", "/api/pick", { kind, title }),
  scan: (root?: string) =>
    request<{ root: string; interviews: InterviewSummary[] }>("POST", "/api/library/scan", root ? { root } : {}),
  interviews: () => request<InterviewSummary[]>("GET", "/api/interviews"),
  interview: (id: string) => request<InterviewDetail>("GET", iv(id)),
  unitMarks: (id: string) => request<UnitMark[]>("GET", `${iv(id)}/marks`),
  markUnit: (id: string, unitId: string, note = "") => request<UnitMark>("PUT", `${iv(id)}/units/${enc(unitId)}/mark`, { note }),
  unmarkUnit: (id: string, unitId: string) => request<{ unit_id: string; cleared: boolean }>("DELETE", `${iv(id)}/units/${enc(unitId)}/mark`),
  libraryProgress: () => request<LibraryProgress>("GET", "/api/library/progress"),
  simple: (id: string) =>
    request<{ n_respondent_units: number; n_relevant: number; n_coded: number; done: boolean }>("GET", `${iv(id)}/simple`),
  file: (id: string, path: string) => request<FileView>("GET", `${iv(id)}/file?path=${enc(path)}`),
  setPrimary: (id: string, rel_path: string) => request<InterviewEntry>("POST", `${iv(id)}/primary`, { rel_path }),
  segment: (id: string, rel_path?: string) =>
    request<Segmentation>("POST", `${iv(id)}/segment`, rel_path ? { rel_path } : {}),

  phase: (id: string, p: Phase) => request<PhaseState>("GET", ph(id, p)),
  units: (id: string, p: Phase) => request<UnitRow[]>("GET", `${ph(id, p)}/units`),
  setCoding: (id: string, p: Phase, coding: HumanCoding) =>
    request<HumanCoding>("PUT", `${ph(id, p)}/coding/${enc(coding.unit_id)}`, coding),
  /** Phase 1 shorthand: `relevant: false` stores NA ("non pertinent"), `true` restores the turn. */
  setRelevant: (id: string, unitId: string, relevant: boolean) =>
    request<HumanCoding>("PUT", `${ph(id, 1)}/coding/${enc(unitId)}`, { relevant }),
  phase1Codebook: () => request<Codebook>("GET", "/api/codebook/phase1"),

  // ---- AI pass (phase 2)
  generatePrompts: (id: string, batch_size: number) => request<PromptPart[]>("POST", `${p2(id)}/prompts`, { batch_size }),
  prompts: (id: string) => request<PromptPart[]>("GET", `${p2(id)}/prompts`),
  runs: (id: string) => request<AiRun[]>("GET", `${p2(id)}/runs`),
  createRun: (id: string, meta: AiRunMeta) => request<AiRun>("POST", `${p2(id)}/runs`, meta),
  importOutput: (id: string, runId: number, raw: string, part?: number, of?: number) =>
    request<AiRun>("POST", `${p2(id)}/runs/${runId}/outputs`, { raw, part, of }),

  // ---- review (phase 2)
  agreement: (id: string) => request<AgreementAnswer>("GET", `${iv(id)}/agreement`),
  review: (id: string) => request<ReviewAnswer>("GET", `${iv(id)}/review`),
  snapshots: (id: string) => request<HumanSnapshot[]>("GET", `${iv(id)}/snapshots`),
  decide: (id: string, unitId: string, choice: ArbitrationChoice, final_codes: string[], rationale: string) =>
    request<Arbitration>("PUT", `${iv(id)}/review/${enc(unitId)}`, { choice, final_codes, rationale }),
  final: (id: string) => request<FinalAnswer>("GET", `${iv(id)}/final`),

  // ---- to clarify
  toClarify: (opts: { interviewId?: string; open?: boolean } = {}) => {
    const q = new URLSearchParams()
    if (opts.interviewId) q.set("interview_id", opts.interviewId)
    if (opts.open !== undefined) q.set("open", String(opts.open))
    const qs = q.toString()
    return request<ToClarify[]>("GET", `/api/to-clarify${qs ? `?${qs}` : ""}`)
  },
  addToClarify: (id: string, unitId: string, label: string, note: string) =>
    request<ToClarify>("POST", `${iv(id)}/units/${enc(unitId)}/to-clarify`, { label, note }),
  editToClarify: (tid: number, label: string, note: string) =>
    request<ToClarify>("PUT", `/api/to-clarify/${tid}`, { label, note }),
  resolveToClarify: (
    tid: number,
    body: { kind: "new_code" | "mapped"; code: string; rationale: string; all_with_label: boolean; new_code?: Code },
  ) => request<ResolveAnswer>("POST", `/api/to-clarify/${tid}/resolve`, body),

  // ---- codebook
  codebook: () => request<Codebook>("GET", "/api/codebook"),
  importCodebook: (coding_db?: string) =>
    request<Codebook>("POST", "/api/codebook/import", coding_db ? { coding_db } : {}),
  addCode: (code: Code) => request<Codebook>("POST", "/api/codebook/codes", code),
  editCode: (code: string, edit: CodeEdit) => request<Codebook>("PUT", `/api/codebook/codes/${enc(code)}`, edit),
  mergeCode: (code: string, into: string, rationale: string) =>
    request<MergeAnswer>("POST", `/api/codebook/codes/${enc(code)}/merge`, { into, rationale }),
  setActive: (code: string, active: boolean) =>
    request<Codebook>("POST", `/api/codebook/codes/${enc(code)}/active`, { active }),
  setInPrompt: (code: string, in_prompt: boolean) =>
    request<Codebook>("PUT", `/api/codebook/codes/${enc(code)}/in_prompt`, { in_prompt }),
  codeUnits: (code: string) => request<CodeUnit[]>("GET", `/api/codebook/codes/${enc(code)}/units`),
  exportCodebook: () => request<WrittenFiles>("POST", "/api/codebook/export"),

  /** v5: import by kind (csv, json or coding_db); the answer carries the counts. */
  importCodebookFrom: (kind: "csv" | "json" | "coding_db", path: string) =>
    request<CodebookImportAnswer>("POST", "/api/codebook/import", { kind, path }),

  // ---- study configuration (v5); a 409 guard is re-sent with force
  config: () => request<StudyConfig>("GET", "/api/config"),
  setConfig: (cfg: StudyConfig, force = false) =>
    request<StudyConfig>("PUT", force ? "/api/config?force=true" : "/api/config", cfg),

  // ---- providers (v5)
  providers: () => request<ProviderEntry[]>("GET", "/api/providers"),
  setProviders: (cfg: ProvidersConfig) => request<ProviderEntry[]>("PUT", "/api/providers", cfg),
  probeProvider: (id: string) => request<IsolationReport>("POST", `/api/providers/${enc(id)}/probe`),
  oauthBegin: (id: string) => request<OAuthBegin>("POST", `/api/providers/${enc(id)}/oauth/begin`, {}),
  oauthStatus: (id: string) => request<OAuthStatus>("GET", `/api/providers/${enc(id)}/oauth/status`),
  runWithProvider: (id: string, batchId: number, body: ProviderRunBody) =>
    request<ProviderRunAnswer>("POST", `${iv(id)}/batches/${batchId}/run`, body),

  // ---- research questions
  questions: () => request<ResearchQuestion[]>("GET", "/api/research-questions"),
  addQuestion: (id: string, text: string) => request<ResearchQuestion[]>("POST", "/api/research-questions", { id, text }),
  editQuestion: (id: string, text: string, ord?: number) =>
    request<ResearchQuestion[]>("PUT", `/api/research-questions/${enc(id)}`, ord === undefined ? { text } : { text, ord }),
  deleteQuestion: (qid: string) => request<ResearchQuestion[]>("DELETE", `/api/research-questions/${enc(qid)}`),

  // ---- cases
  cases: (id: string) => request<Case[]>("GET", `${iv(id)}/cases`),
  allCases: () => request<Case[]>("GET", "/api/cases"),
  addCase: (id: string, c: Partial<Case>) => request<Case>("POST", `${iv(id)}/cases`, c),
  editCase: (c: Case) => request<Case>("PUT", `/api/cases/${enc(c.case_id)}`, c),

  audit: (interviewId?: string) =>
    request<AuditEvent[]>("GET", interviewId ? `/api/audit?interview_id=${enc(interviewId)}` : "/api/audit"),
  verify: () => request<ChainCheck>("GET", "/api/audit/verify"),
  exportPackage: (id: string) => request<ExportAnswer>("POST", `${iv(id)}/export`),

  // ---- unit flags, corrections, verification (phase independent, per interview)
  flags: (id: string) => request<UnitFlags[]>("GET", `${iv(id)}/flags`),
  setFlags: (id: string, unitId: string, flags: string[], co_constructed_kind: CoConstructedKind | null) =>
    request<UnitFlags>("PUT", `${iv(id)}/units/${enc(unitId)}/flags`, { flags, co_constructed_kind }),
  corrections: (id: string, unitId: string) => request<UnitCorrection[]>("GET", `${iv(id)}/units/${enc(unitId)}/corrections`),
  correctUnit: (id: string, unitId: string, body: { text: string; role: SpeakerRole; reason: string }) =>
    request<UnitCorrection>("POST", `${iv(id)}/units/${enc(unitId)}/corrections`, body),
  verification: (id: string) => request<VerificationAnswer>("GET", `${iv(id)}/verification`),
  verifyUnit: (id: string, unitId: string, state: VerificationState, note: string | null) =>
    request<unknown>("PUT", `${iv(id)}/units/${enc(unitId)}/verification`, { state, note }),

  // ---- claims
  claims: (id: string) => request<Claim[]>("GET", `${iv(id)}/claims`),
  claimsReview: (id: string) => request<ClaimReview[]>("GET", `${iv(id)}/claims/review`),
  addClaim: (id: string, claim: ClaimInput) => request<Claim>("POST", `${iv(id)}/claims`, claim),
  editClaim: (claimId: number, claim: ClaimInput) => request<Claim>("PUT", `/api/claims/${claimId}`, claim),
  deleteClaim: (claimId: number) => request<{ deleted: number }>("DELETE", `/api/claims/${claimId}`),

  // ---- research question memos: a question without memo answers 404, read here as null
  rqMemo: (rq: string) =>
    request<RqMemo>("GET", `/api/research-questions/${enc(rq)}/memo`).catch((e: unknown) => {
      if (e instanceof ApiError && e.kind === "not_found") return null
      throw e
    }),
  setRqMemo: (rq: string, memo: Omit<RqMemo, "rq_id" | "updated_at">) =>
    request<RqMemo>("PUT", `/api/research-questions/${enc(rq)}/memo`, memo),

  // ---- saturation
  saturation: () => request<SaturationRow[]>("GET", "/api/saturation"),

  // ---- V1 passages (docs/API.md, "V1 lots 1 and 2"). Offsets are UTF-16 as a browser selection reports them.
  passages: (id: string, opts: { unitId?: string; code?: string; kind?: "highlight" | "coded" | "later" } = {}) => {
    const q = new URLSearchParams()
    if (opts.unitId) q.set("unit_id", opts.unitId)
    if (opts.code) q.set("code", opts.code)
    if (opts.kind) q.set("kind", opts.kind)
    const qs = q.toString()
    return request<Passage[]>("GET", `${iv(id)}/passages${qs ? `?${qs}` : ""}`)
  },
  createPassage: (id: string, body: PassageBody) =>
    request<PassageCreateAnswer>("POST", `${iv(id)}/passages`, { offset_unit: "utf16", ...body }),
  passage: (pid: string) => request<Passage>("GET", `/api/passages/${enc(pid)}`),
  deletePassage: (pid: string, reason?: string) =>
    request<{ deleted: string }>("DELETE", `/api/passages/${enc(pid)}${reason ? `?reason=${enc(reason)}` : ""}`),
  addCoding: (pid: string, code: string, note?: string) =>
    request<Passage>("POST", `/api/passages/${enc(pid)}/codings`, note === undefined ? { code } : { code, note }),
  removeCoding: (pid: string, code: string, reason?: string) =>
    request<Passage>("DELETE", `/api/passages/${enc(pid)}/codings/${enc(code)}${reason ? `?reason=${enc(reason)}` : ""}`),
  highlights: (id: string) => request<Passage[]>("GET", `${iv(id)}/highlights`),
  interviewLater: (id: string) => request<Passage[]>("GET", `${iv(id)}/later`),
  later: () => request<LaterAnswer>("GET", "/api/later"),

  // ---- V1 code tree
  codes: () => request<CodeTreeAnswer>("GET", "/api/codes"),
  codesBy: (attribute: string, value: string) =>
    request<CodeTreeAnswer>("GET", `/api/codes?attribute=${enc(attribute)}&value=${enc(value)}`),
  code: (id: string) => request<CodeNode>("GET", `/api/codes/${enc(id)}`),
  updateCode: (id: string, edit: CodeEditV1) => request<CodeNode>("PUT", `/api/codes/${enc(id)}`, edit),
  moveCode: (id: string, parent_id: string | null, rationale: string) =>
    request<CodeNode>("POST", `/api/codes/${enc(id)}/move`, { parent_id, rationale }),
  addAlias: (id: string, alias: string) => request<CodeNode>("POST", `/api/codes/${enc(id)}/aliases`, { alias }),
  removeAlias: (id: string, alias: string) => request<CodeNode>("DELETE", `/api/codes/${enc(id)}/aliases/${enc(alias)}`),
  mergeCodeV1: (id: string, into: string, rationale: string) =>
    request<MergeAnswerV1>("POST", `/api/codes/${enc(id)}/merge`, { into, rationale }),
  setCodeActive: (id: string, active: boolean) => request<CodeNode>("POST", `/api/codes/${enc(id)}/active`, { active }),
  createCodeFromPassage: (body: FromPassageBody) =>
    request<{ code: CodeNode; passage: Passage; codebook_version: number }>("POST", "/api/codes/from-passage", body),
  codePassages: (id: string, descendants = true) =>
    request<PassageContext[]>("GET", `/api/codes/${enc(id)}/passages?descendants=${descendants}`),
  codebookRevisions: () => request<{ codebook_version: number; revisions: CodebookRevision[] }>("GET", "/api/codebook/revisions"),

  // ---- V1 project configuration, attributes, memo, checklist, template
  projectConfig: () => request<ProjectConfig>("GET", "/api/project-config"),
  setProjectConfig: (cfg: ProjectConfig) => request<ProjectConfig>("PUT", "/api/project-config", cfg),
  projectPresets: () => request<ProjectPresets>("GET", "/api/project-config/presets"),
  attributes: (id: string) => request<InterviewAttributes>("GET", `${iv(id)}/attributes`),
  setAttributes: (id: string, values: Record<string, string | null>) =>
    request<InterviewAttributes>("PUT", `${iv(id)}/attributes`, { values }),
  memo: (id: string) => request<MemoView>("GET", `${iv(id)}/memo`),
  setMemoTemplate: (id: string, template_id: string) => request<MemoView>("PUT", `${iv(id)}/memo/template`, { template_id }),
  setMemoOverrides: (id: string, overrides: MemoOverrides) => request<MemoView>("PUT", `${iv(id)}/memo/overrides`, overrides),
  setMemoAnswers: (id: string, answers: Record<string, string>) => request<MemoView>("PUT", `${iv(id)}/memo/answers`, { answers }),
  checklist: (id: string) => request<ChecklistView>("GET", `${iv(id)}/checklist`),
  setChecklist: (id: string, checked: Record<string, boolean>, note?: string) =>
    request<ChecklistView>("PUT", `${iv(id)}/checklist`, note === undefined ? { checked } : { checked, note }),
  projectTemplate: () => request<unknown>("GET", "/api/project/template"),
  importTemplate: (template: unknown, dry_run: boolean) =>
    request<TemplateReport>("POST", "/api/project/template/import", { template, dry_run }),
}

/** Body of `POST /api/interviews/{id}/passages` (offsets in UTF-16 units unless `offset_unit` says otherwise). */
export interface PassageBody {
  unit_id: string
  start?: number
  end?: number
  offset_unit?: "utf16" | "scalar"
  text?: string
  code?: string
  note?: string
}
