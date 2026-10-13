// Client of the review routes of lot 4 (docs/API.md, "V1 lots 3 and 4", "Review
// (lot 4)"): candidate concepts, the pseudonymisation list of the project, the
// harmonisation worklist and the review state. The shapes are the ones the
// server answers; nothing here calls a model.

import { request } from "./api"

const enc = encodeURIComponent
const iv = (id: string) => `/api/interviews/${enc(id)}`

export type ConceptAction = "create" | "map" | "reject"

/** The dated, motivated decision on a candidate concept. */
export interface ConceptDecision {
  action: ConceptAction
  /** Code created or code attached to; null for a rejection. */
  code_id: string | null
  rationale: string
  decided_at: string
  decided_by: string
}

/** A quotation that motivates a candidate, word for word in its unit. */
export interface CandidateEvidence {
  unit_id: string
  quote: string
}

/** A concept the model reported as absent from the codebook. Never applied by itself. */
export interface ConceptCandidate {
  candidate_id: string
  interview_id: string
  label: string
  description: string | null
  run_label: string | null
  /** Units that motivate the proposal. */
  unit_ids: string[]
  evidence: CandidateEvidence[]
  /** The current decision, null until decided. */
  decision: ConceptDecision | null
  /** Every decision, oldest first; the last one is `decision`. */
  history: ConceptDecision[]
}

export interface ConceptDecisionBody {
  action: ConceptAction
  /** Existing code to attach to (`map`). */
  code_id?: string
  /** Name of the code to create (`create`). */
  new_code_name?: string
  rationale: string
}

export type PseudonymKind = "person" | "organisation"

/** One name of the list and its pseudonym. */
export interface PseudonymEntry {
  kind: PseudonymKind
  original: string
  pseudonym: string
  /** Replacements in an interview; absent on the list of the project. */
  count?: number
  /** On the list of the project: "person" when derived from an anonymised record, "manual" when typed by hand. */
  source?: "person" | "manual"
  /** The record a derived entry comes from. */
  person_id?: string | null
}

/** A manual entry left out because a record already replaces the same name. */
export interface ShadowedEntry {
  kind: PseudonymKind
  original: string
  pseudonym: string
  reason: string
}

/** The pseudonymisation list: of the project, or of an interview (the names found in it). */
export interface Pseudonymisation {
  /** The list in force: the manual switch or any anonymised record. */
  enabled: boolean
  /** The manual switch itself (absent on an older server). */
  enabled_manual?: boolean
  n_manual_entries?: number
  n_person_entries?: number
  shadowed_manual?: ShadowedEntry[]
  /** SHA-256 of the list, recorded with every prompt. */
  digest: string
  categories: string[]
  n_list_entries: number
  entries: PseudonymEntry[]
  /** Always false: memos and notes typed by the researcher are not replaced. */
  free_text_covered: boolean
  note: string
}

export interface PseudonymisationBody {
  enabled?: boolean
  /** Replaces the whole list. */
  entries?: { kind: PseudonymKind; original: string; pseudonym: string }[]
}

export type HarmonisationCause = "merge" | "redefine" | "new_subcode"
export type HarmonisationResolution = "confirmed" | "corrected"

export interface HarmonisationCode {
  token: string
  code_id: string
  name: string
}

/** One unit to reread after a change of the codebook. */
export interface HarmonisationItem {
  item_id: number
  interview_id: string
  unit_id: string
  text: string
  /** The codes of the unit now. */
  codes: HarmonisationCode[]
  /** The code the change concerns. */
  code_id: string
  code_name: string
  cause: HarmonisationCause
  detail: string
  created_at: string
  created_by: string
  status: "open" | "done"
  resolved_at: string | null
  resolved_by: string | null
  resolution: HarmonisationResolution | null
  note: string | null
}

export interface Worklist {
  n_open: number
  n_done: number
  items: HarmonisationItem[]
}

export type ReviewStateName = "not_done" | "in_progress" | "complete"

/** Where the review of an interview stands, with the sentence a report can use. */
export interface ReviewState {
  interview_id: string
  state: ReviewStateName
  /** Units in the scope of the review. */
  n_relevant: number
  /** Of those, the units a complete run answered. */
  n_covered: number
  coverage: number
  runs: string[]
  n_batches: number
  n_disagreements: number
  n_decided: number
  n_sample: number
  n_sample_reviewed: number
  n_candidates: number
  n_candidates_decided: number
  harmonisation_open: number
  harmonisation_done: number
  declaration: string
}

export interface CorpusReviewState {
  interviews: ReviewState[]
  n_interviews: number
  n_complete: number
  n_in_progress: number
  n_not_done: number
  declaration: string
}

export const reviewApi = {
  conceptCandidates: (id: string) => request<ConceptCandidate[]>("GET", `${iv(id)}/concept-candidates`),
  decideConcept: (id: string, candidateId: string, body: ConceptDecisionBody) =>
    request<ConceptCandidate>("PUT", `${iv(id)}/concept-candidates/${enc(candidateId)}`, body),

  /** The names of the list found in an interview, with their number of replacements. */
  pseudonymisation: (id: string) => request<Pseudonymisation>("GET", `${iv(id)}/pseudonymisation`),
  /** The whole list of the project. */
  projectPseudonymisation: () => request<Pseudonymisation>("GET", "/api/pseudonymisation"),
  savePseudonymisation: (body: PseudonymisationBody) => request<Pseudonymisation>("PUT", "/api/pseudonymisation", body),

  harmonisation: (id: string) => request<Worklist>("GET", `${iv(id)}/harmonisation`),
  allHarmonisation: () => request<Worklist>("GET", "/api/harmonisation"),
  resolveHarmonisation: (id: string, item: number, body: { resolution: HarmonisationResolution; note?: string }) =>
    request<HarmonisationItem>("PUT", `${iv(id)}/harmonisation/${item}`, body),

  reviewState: (id: string) => request<ReviewState>("GET", `${iv(id)}/review-state`),
  corpusReviewState: () => request<CorpusReviewState>("GET", "/api/review-state"),
}
