// Typed client for the local Coding Studio API (docs/API.md).

import type {
  AgreementAnswer,
  AiRun,
  AiRunMeta,
  AppState,
  Arbitration,
  ArbitrationChoice,
  AuditEvent,
  Case,
  ChainCheck,
  Code,
  Codebook,
  CodeEdit,
  CodeUnit,
  CrossCaseAnswer,
  ExportAnswer,
  FileView,
  FinalAnswer,
  HumanCoding,
  InterviewDetail,
  InterviewEntry,
  InterviewSummary,
  MergeAnswer,
  Phase,
  PhaseState,
  PromptPart,
  ResearchQuestion,
  ResolveAnswer,
  ReviewAnswer,
  Segmentation,
  ToClarify,
  UnitRow,
  ValidationIssue,
  WrittenFiles,
} from "./types"

export type ErrorKind = "validation" | "guard" | "not_found" | "internal" | "network"

/** Error answered by the server, or a network failure. */
export class ApiError extends Error {
  readonly status: number
  readonly kind: ErrorKind
  readonly issues: ValidationIssue[]

  constructor(status: number, kind: ErrorKind, message: string, issues: ValidationIssue[] = []) {
    super(message)
    this.name = "ApiError"
    this.status = status
    this.kind = kind
    this.issues = issues
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
    const b = body as { error: unknown; kind?: unknown; issues?: unknown }
    const kind = KINDS.includes(b.kind as ErrorKind) ? (b.kind as ErrorKind) : kindFromStatus(status)
    const issues = Array.isArray(b.issues) ? (b.issues as ValidationIssue[]) : []
    return new ApiError(status, kind, String(b.error), issues)
  }
  const text = typeof body === "string" && body.trim() ? body.trim() : `Request failed with status ${status}`
  return new ApiError(status, kindFromStatus(status), text)
}

export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, {
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
  file: (id: string, path: string) => request<FileView>("GET", `${iv(id)}/file?path=${enc(path)}`),
  setPrimary: (id: string, rel_path: string) => request<InterviewEntry>("POST", `${iv(id)}/primary`, { rel_path }),
  segment: (id: string, rel_path?: string) =>
    request<Segmentation>("POST", `${iv(id)}/segment`, rel_path ? { rel_path } : {}),

  phase: (id: string, p: Phase) => request<PhaseState>("GET", ph(id, p)),
  units: (id: string, p: Phase) => request<UnitRow[]>("GET", `${ph(id, p)}/units`),
  setCoding: (id: string, p: Phase, coding: HumanCoding) =>
    request<HumanCoding>("PUT", `${ph(id, p)}/coding/${enc(coding.unit_id)}`, coding),
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
  crossCase: () => request<CrossCaseAnswer>("GET", "/api/cross-case"),

  audit: (interviewId?: string) =>
    request<AuditEvent[]>("GET", interviewId ? `/api/audit?interview_id=${enc(interviewId)}` : "/api/audit"),
  verify: () => request<ChainCheck>("GET", "/api/audit/verify"),
  exportPackage: (id: string) => request<ExportAnswer>("POST", `${iv(id)}/export`),
}
