// Client of the V2 routes (docs/API.md, "V2 codes and search" and "V2 transcripts
// and audio"): code search, counts, split and undo, full-text search,
// co-occurrence, transcript versions and the review of projected passages, audio.

import { request } from "./api"
import type { CodeNode, Passage } from "./types"

const enc = encodeURIComponent

function query(params: Record<string, string | number | boolean | undefined | null | string[]>): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "" || v === false) continue
    if (Array.isArray(v)) {
      if (v.length) q.set(k, v.join(","))
      continue
    }
    q.set(k, String(v))
  }
  const s = q.toString()
  return s ? `?${s}` : ""
}

// ------------------------------------------------------------ code search

export type CodeMatchField = "name" | "alias" | "token" | "label" | "definition"

export interface CodeSearchItem {
  code_id: string
  name: string
  token: string
  definition: string
  parent_id: string | null
  /** Names of the ancestors, root first. */
  path: string[]
  active: boolean
  system: boolean
  passages: number
  interviews: number
  score: number
  matched: { field: CodeMatchField; text: string }
}

export interface CodeSearchAnswer {
  query: string
  total: number
  items: CodeSearchItem[]
}

// ------------------------------------------------------------- code stats

export interface AttributeStat {
  attribute_id: string
  label: string
  distinct_values: number
  values: { value: string; interviews: number }[]
  interviews_without_value: number
}

export interface StatBlock {
  passages: number
  interviews: number
  cases: number
  interview_ids: string[]
  case_ids: string[]
  attributes: AttributeStat[]
}

export interface CodeStats {
  code_id: string
  name: string
  own: StatBlock
  rollup: StatBlock
  filter: { attribute: string; value: string } | null
  note: string
}

// ------------------------------------------------------- split, journal

export interface SplitPartBody {
  name: string
  definition: string
  includes?: string
  excludes?: string
  parent_id?: string | null
}

export interface SplitAnswer {
  op_id: number
  original: CodeNode
  new_codes: CodeNode[]
  worklist: unknown
}

export type WorklistStatus = "pending" | "assigned" | "left"

export interface WorklistItem {
  passage: Passage
  status: WorklistStatus
  targets: string[]
  note: string
  decided_at: string | null
  decided_by: string | null
  n_decisions: number
}

export interface Worklist {
  op_id: number
  original: CodeNode
  new_codes: CodeNode[]
  rationale: string
  total: number
  pending: number
  assigned: number
  left: number
  gone: number
  undone: { rationale: string; at: string; by: string } | null
  items: WorklistItem[]
}

export interface WorklistDecision {
  passage_id: string
  action: "assign" | "leave"
  targets?: string[]
  note?: string
}

export interface CodeOp {
  op_id: number
  kind: "merge" | "split"
  code_id: string
  code_name: string
  other_ids: string[]
  rationale: string
  created_at: string
  created_by: string
  undone: { rationale: string; at: string; by: string } | null
  summary: Record<string, unknown>
}

export interface UndoReport {
  op_id: number
  kind: "merge" | "split"
  codings_restored: number
  codings_removed: number
  codings_moved_back: number
  unit_codings_restored: number
  decisions_restored: number
  skipped: unknown[]
}

// ------------------------------------------------------------ text search

export type SearchMode = "all_words" | "phrase"

export interface SearchParams {
  q: string
  mode?: SearchMode
  prefix?: boolean
  include_interviewer?: boolean
  interviews?: string[]
  attribute?: string
  value?: string
  coded?: "true" | "false" | ""
  code?: string
  descendants?: boolean
  limit?: number
  offset?: number
  context?: number
}

export interface SearchMatch {
  start: number
  end: number
  start_utf16: number
  end_utf16: number
}

export interface SearchNeighbour {
  unit_id: string
  role: string
  speaker: string
  text: string
}

export interface SearchItem {
  interview_id: string
  unit_id: string
  seq: number
  role: string
  speaker: string
  text: string
  matches: SearchMatch[]
  excerpt: {
    offset: number
    text: string
    highlights: { start: number; end: number }[]
    truncated_before: boolean
    truncated_after: boolean
  }
  previous: SearchNeighbour | null
  next: SearchNeighbour | null
  coded: boolean
  uncovered_matches: number
  passage_ids: string[]
  code_ids: string[]
}

export interface SearchResult {
  query: string
  mode: SearchMode
  words: string[]
  total_units: number
  total_hits: number
  truncated: boolean
  interviews: { interview_id: string; units: number; hits: number; mentioned: boolean }[]
  items: SearchItem[]
}

// ----------------------------------------------------------- co-occurrence

export type CoRelation = "same_passage" | "overlap" | "same_unit"

export interface CoParams {
  relation?: CoRelation
  group_by?: "interview" | "attribute" | ""
  attribute?: string
  codes?: string[]
  rollup?: boolean
  min_units?: number
  interviews?: string[]
}

export interface CoCell {
  a: string
  b: string
  units: number
  interviews: number
  passage_pairs: number
  jaccard: number
}

export interface CoGroup {
  key: string
  label: string
  interviews: number
  cells: CoCell[]
  diagonal: { code_id: string; units: number; interviews: number; passages: number }[]
}

export interface Cooccurrence {
  codes: { code_id: string; name: string; active: boolean }[]
  relation: CoRelation
  group_by: string | null
  attribute: string | null
  rollup: boolean
  groups: CoGroup[]
  note: string
}

export interface CoEvidenceItem {
  interview_id: string
  unit_id: string
  seq?: number
  speaker?: string
  text: string
  pairs: { a: string; b: string; overlap?: boolean | number }[]
  [k: string]: unknown
}

export interface CoEvidence {
  a: string
  b: string
  relation: CoRelation
  rollup: boolean
  n_units: number
  items: CoEvidenceItem[]
}

// ------------------------------------------------------ transcript versions

export interface TranscriptVersion {
  version: number
  cause: "baseline" | "segmentation" | "correction" | "preparation"
  text_sha256: string
  n_units: number
  target: string | null
  reason: string | null
  created_at: string
  created_by: string
  reanchored: number
  flagged: number
}

export interface TranscriptVersions {
  interview_id: string
  current: number
  versions: TranscriptVersion[]
}

export interface Span {
  unit_id: string
  start: number
  end: number
  start_utf16: number
  end_utf16: number
  text: string
  unit_text: string
}

export type ProjectionStatus = "needs_review" | "reanchored" | "accepted" | "adjusted" | "detached" | "superseded"
export type ProjectionReason = "" | "text_changed" | "unit_moved" | "unit_removed" | "unit_became_interviewer"

export interface ProjectionItem {
  projection_id: number
  interview_id: string
  passage_id: string
  status: ProjectionStatus
  method: "diff" | "moved_unit" | "search"
  reason: ProjectionReason | null
  from_version: number
  to_version: number
  old: Span
  proposed: Span | null
  final: Span | null
  current_unit_text: string
  codes: { code_id: string; code_name: string }[]
  created_at: string
  resolved_at: string | null
  resolved_by: string | null
  resolution_note: string | null
}

export interface ReviewAnswer {
  interview_id: string | null
  status: string
  open: number
  items: ProjectionItem[]
}

export interface AdjustBody {
  unit_id?: string
  start: number
  end: number
  offset_unit?: "scalar" | "utf16"
  note?: string
}

// ------------------------------------------------------------------ audio

export interface AudioUnit {
  unit_id: string
  seq: number
  role: string
  timestamp: string
  start_seconds: number | null
  audio_seconds: number | null
  end_seconds: number | null
}

export interface AudioOffset {
  offset_seconds: number
  note: string
  set: boolean
  updated_at: string | null
  updated_by: string | null
}

export interface AudioMap {
  interview_id: string
  has_audio: boolean
  files: { rel_path: string; size: number; mime: string; url: string }[]
  offset: AudioOffset
  timed_units: number
  units: AudioUnit[]
  warnings: string[]
}

export const v2 = {
  // codes
  codeSearch: (q: string, opts: { limit?: number; include_inactive?: boolean; include_system?: boolean } = {}) =>
    request<CodeSearchAnswer>("GET", `/api/codes/search${query({ q, ...opts })}`),
  codeStats: (id: string, filter?: { attribute: string; value: string }) =>
    request<CodeStats>("GET", `/api/codes/${enc(id)}/stats${query({ attribute: filter?.attribute, value: filter?.value })}`),
  splitCode: (id: string, parts: SplitPartBody[], rationale: string) =>
    request<SplitAnswer>("POST", `/api/codes/${enc(id)}/split`, { parts, rationale }),
  worklist: (opId: number, status?: WorklistStatus) =>
    request<Worklist>("GET", `/api/splits/${opId}/worklist${query({ status })}`),
  decideWorklist: (opId: number, decisions: WorklistDecision[]) =>
    request<Worklist>("PUT", `/api/splits/${opId}/worklist`, { decisions }),
  codeOps: (code?: string) => request<{ operations: CodeOp[] }>("GET", `/api/code-ops${query({ code })}`),
  undoCodeOp: (opId: number, rationale: string) =>
    request<{ report: UndoReport; codebook: unknown }>("POST", `/api/code-ops/${opId}/undo`, { rationale }),

  // text search and co-occurrence
  searchText: (p: SearchParams) =>
    request<SearchResult>(
      "GET",
      `/api/search/text${query({
        q: p.q,
        mode: p.mode,
        prefix: p.prefix,
        include_interviewer: p.include_interviewer,
        interviews: p.interviews,
        attribute: p.attribute,
        value: p.value,
        coded: p.coded,
        code: p.code,
        descendants: p.descendants,
        limit: p.limit,
        offset: p.offset,
        context: p.context,
      })}`,
    ),
  cooccurrence: (p: CoParams) =>
    request<Cooccurrence>(
      "GET",
      `/api/codes/cooccurrence${query({ relation: p.relation, group_by: p.group_by, attribute: p.attribute, codes: p.codes, rollup: p.rollup, min_units: p.min_units, interviews: p.interviews })}`,
    ),
  cooccurrencePassages: (a: string, b: string, p: { relation?: CoRelation; rollup?: boolean; interviews?: string[] } = {}) =>
    request<CoEvidence>("GET", `/api/codes/cooccurrence/passages${query({ a, b, relation: p.relation, rollup: p.rollup, interviews: p.interviews })}`),

  // transcript versions and review
  versions: (id: string) => request<TranscriptVersions>("GET", `/api/interviews/${enc(id)}/transcript/versions`),
  review: (opts: { interview_id?: string; status?: string } = {}) =>
    request<ReviewAnswer>("GET", `/api/transcript/review${query({ interview_id: opts.interview_id, status: opts.status })}`),
  acceptProjection: (pid: number, note?: string) => request<ProjectionItem>("POST", `/api/transcript/review/${pid}/accept`, note ? { note } : {}),
  adjustProjection: (pid: number, body: AdjustBody) => request<ProjectionItem>("POST", `/api/transcript/review/${pid}/adjust`, body),
  detachProjection: (pid: number, note?: string) => request<ProjectionItem>("POST", `/api/transcript/review/${pid}/detach`, note ? { note } : {}),

  // audio
  audio: (id: string) => request<AudioMap>("GET", `/api/interviews/${enc(id)}/audio`),
  setAudioOffset: (id: string, offset_seconds: number, note?: string) =>
    request<AudioOffset>("PUT", `/api/interviews/${enc(id)}/audio/offset`, note ? { offset_seconds, note } : { offset_seconds }),
}
