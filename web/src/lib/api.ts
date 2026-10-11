// Typed client for the local Coding Studio API (docs/API.md).

import type {
  AgreementAnswer,
  AiRun,
  AiRunMeta,
  AppState,
  Arbitration,
  ArbitrationChoice,
  AuditEvent,
  ChainCheck,
  Code,
  Codebook,
  DivergencesAnswer,
  ExportAnswer,
  FileView,
  FinalAnswer,
  HumanCoding,
  InterviewDetail,
  InterviewEntry,
  InterviewSummary,
  Phase,
  PhaseState,
  PromptPart,
  Segmentation,
  UnitRow,
  ValidationIssue,
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

  generatePrompts: (id: string, p: Phase, batch_size: number) =>
    request<PromptPart[]>("POST", `${ph(id, p)}/prompts`, { batch_size }),
  prompts: (id: string, p: Phase) => request<PromptPart[]>("GET", `${ph(id, p)}/prompts`),
  runs: (id: string, p: Phase) => request<AiRun[]>("GET", `${ph(id, p)}/runs`),
  createRun: (id: string, p: Phase, meta: AiRunMeta) => request<AiRun>("POST", `${ph(id, p)}/runs`, meta),
  importOutput: (id: string, p: Phase, runId: number, raw: string, part?: number, of?: number) =>
    request<AiRun>("POST", `${ph(id, p)}/runs/${runId}/outputs`, { raw, part, of }),

  agreement: (id: string, p: Phase) => request<AgreementAnswer>("GET", `${ph(id, p)}/agreement`),
  divergences: (id: string, p: Phase) => request<DivergencesAnswer>("GET", `${ph(id, p)}/divergences`),
  arbitrate: (id: string, p: Phase, unitId: string, choice: ArbitrationChoice, final_codes: string[], rationale: string) =>
    request<Arbitration>("PUT", `${ph(id, p)}/arbitration/${enc(unitId)}`, { choice, final_codes, rationale }),
  final: (id: string, p: Phase) => request<FinalAnswer>("GET", `${ph(id, p)}/final`),

  codebook: () => request<Codebook>("GET", "/api/codebook"),
  importCodebook: (coding_db?: string) =>
    request<Codebook>("POST", "/api/codebook/import", coding_db ? { coding_db } : {}),
  addCode: (code: Code) => request<Codebook>("POST", "/api/codebook/codes", code),
  setInPrompt: (code: string, in_prompt: boolean) =>
    request<Codebook>("PUT", `/api/codebook/codes/${enc(code)}/in_prompt`, { in_prompt }),

  audit: (interviewId?: string) =>
    request<AuditEvent[]>("GET", interviewId ? `/api/audit?interview_id=${enc(interviewId)}` : "/api/audit"),
  verify: () => request<ChainCheck>("GET", "/api/audit/verify"),
  exportPackage: (id: string) => request<ExportAnswer>("POST", `${iv(id)}/export`),
}
