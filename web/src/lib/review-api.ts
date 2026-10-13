// Endpoints of the review that exist on another branch only (candidate concepts
// and pseudonymisation). Each reader answers null when the server does not know
// the route (404), so that the screen hides the matching part and still works
// on a server without them.
//
// The shapes below are the ones this screen expects. They are to be checked
// against docs/API.md once the endpoints are merged.

import { ApiError, request } from "./api"

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
  decided_by?: string
}

/** A concept the model reported as absent from the codebook. Never applied by itself. */
export interface ConceptCandidate {
  candidate_id: string
  label: string
  description?: string | null
  run_label?: string | null
  /** Units that motivate the proposal. */
  unit_ids: string[]
  decision: ConceptDecision | null
}

export interface ConceptDecisionBody {
  action: ConceptAction
  /** Existing code to attach to (`map`). */
  code_id?: string
  /** Name of the code to create (`create`). */
  new_code_name?: string
  rationale: string
}

/** One substitution of the pseudonymisation applied to the prompt. */
export interface PseudonymEntry {
  /** "person" or "organisation". */
  kind: string
  original: string
  pseudonym: string
  count?: number
}

export interface PseudonymisationList {
  entries: PseudonymEntry[]
}

/** Runs `call`; a 404 (route unknown on this server, or no data) reads as null. */
export async function orNull<T>(call: () => Promise<T>): Promise<T | null> {
  try {
    return await call()
  } catch (e) {
    if (e instanceof ApiError && e.kind === "not_found") return null
    throw e
  }
}

export const reviewApi = {
  /** Candidate concepts of the interview; null when the server has no such route. */
  conceptCandidates: async (id: string): Promise<ConceptCandidate[] | null> => {
    const body = await orNull(() => request<unknown>("GET", `${iv(id)}/concept-candidates`))
    return Array.isArray(body) ? (body as ConceptCandidate[]) : null
  },
  decideConcept: (id: string, candidateId: string, body: ConceptDecisionBody) =>
    request<ConceptCandidate>("PUT", `${iv(id)}/concept-candidates/${enc(candidateId)}`, body),
  /** Names and organisations replaced in the prompt; null when the server has no such route. */
  pseudonymisation: async (id: string): Promise<PseudonymisationList | null> => {
    const body = await orNull(() => request<unknown>("GET", `${iv(id)}/pseudonymisation`))
    const entries = (body as PseudonymisationList | null)?.entries
    return Array.isArray(entries) ? { entries } : null
  },
}
