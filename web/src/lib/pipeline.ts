// Where an interview stands in the four steps of the Bibliothèque
// (Segmenté, Codé, Revue IA facultative, Arbitré), and the next step to take.

import type { ReviewState } from "./review-api"
import { interviewHref } from "./router"
import type { InterviewSummary } from "./types"

/** What the library progress says about an interview's respondent turns. */
export interface CodingProgress {
  /** Turns that are not the interviewer's. */
  respondent: number
  /** Respondent turns marked not relevant. */
  nonPertinent: number
  coded: number
  later: number
  /** Respondent turns marked "à revoir". */
  toReview: number
}

/** A row of `GET /api/library/progress`: every interview in one call. */
export interface LibraryRow {
  interview_id: string
  units: number
  respondent_units: number
  relevant: number
  not_relevant: number
  /** Relevant units coded in the phase 2 grid. */
  coded: number
  /** Respondent units carrying a passage with a real code: what the Coder shows as coded. */
  units_coded: number
  /** Respondent units parked under "Plus tard" and carrying no real code. */
  units_later: number
  to_review: number
  memo_answered: number
  memo_total: number
  review: { state: "none" | "in_progress" | "done"; disagreements: number; arbitrated: number; sample: number; sample_reviewed: number }
  arbitrated: boolean
  done: boolean
}

export interface LibraryProgress {
  interviews: LibraryRow[]
  totals: Record<string, number>
}

/** Progress of an interview from its row of the library progress. */
export function codingProgress(row: LibraryRow): CodingProgress {
  return { respondent: row.respondent_units, nonPertinent: row.not_relevant, coded: row.units_coded, later: row.units_later, toReview: row.to_review }
}

export function examined(p: CodingProgress): number {
  return p.coded + p.later + p.nonPertinent
}

export interface Step {
  key: "segmented" | "coded" | "ai" | "arbitrated"
  label: string
  /** 0 to 1, the width of the bar. */
  value: number
  /** A step the study may skip: drawn dashed while empty. */
  optional?: boolean
}

/** Label and bar of the optional review step, from the review state the server computes. The cell is narrow: the labels are short. */
function aiStep(ai: number, r: ReviewState | null | undefined): Pick<Step, "label" | "value" | "optional"> {
  const passes = (n: number) => `IA · ${n} passe${n > 1 ? "s" : ""}`
  if (!r) return { label: ai > 0 ? passes(ai) : "IA facultative", value: ai > 0 ? 1 : 0, optional: ai === 0 }
  if (r.state === "not_done") return { label: "IA facultative", value: 0, optional: true }
  if (r.state === "complete") return { label: passes(r.runs.length), value: 1 }
  if (r.runs.length === 0) return { label: "IA en attente", value: 0 }
  return { label: `IA · ${Math.round(r.coverage * 100)} % relus`, value: r.coverage }
}

export function pipelineSteps(s: InterviewSummary, p: CodingProgress | null, r?: ReviewState | null): Step[] {
  const segmented = s.n_units > 0
  const respondent = p?.respondent ?? 0
  const done = p ? p.coded + p.nonPertinent : 0
  const p2 = s.phase2
  const ai = p2.n_ai_runs_complete
  return [
    { key: "segmented", label: segmented ? "Segmenté" : "Segmentation", value: segmented ? 1 : 0 },
    {
      key: "coded",
      label: !segmented ? "Codage" : respondent > 0 && done >= respondent ? `Codé ${done}/${respondent}` : `Codage ${done}/${respondent}`,
      value: respondent > 0 ? Math.min(1, done / respondent) : 0,
    },
    { key: "ai", ...aiStep(ai, r) },
    {
      key: "arbitrated",
      label: p2.n_divergences > 0 ? `Arbitré ${p2.n_arbitrated}/${p2.n_divergences}` : "Arbitrage",
      value: p2.n_divergences > 0 ? p2.n_arbitrated / p2.n_divergences : 0,
    },
  ]
}

export type Stage = "new" | "in_progress" | "ready"

/** Prêt: every respondent turn examined and no open disagreement. Not segmented counts as en cours. */
export function stageOf(s: InterviewSummary, p: CodingProgress | null, r?: ReviewState | null): Stage {
  if (s.n_units === 0) return "new"
  if (!p) return "in_progress"
  const openDisagreements = Math.max(0, s.phase2.n_divergences - s.phase2.n_arbitrated)
  const remaining = p.respondent - examined(p)
  const toHarmonise = r?.harmonisation_open ?? 0
  return p.respondent > 0 && remaining <= 0 && openDisagreements === 0 && toHarmonise === 0 ? "ready" : "in_progress"
}

export type NextAction =
  | { kind: "segment"; label: string }
  | { kind: "link"; label: string; href: string }

export function nextAction(s: InterviewSummary, p: CodingProgress | null, r?: ReviewState | null): NextAction {
  const id = s.interview_id
  if (s.n_units === 0) return { kind: "segment", label: "Segmenter…" }
  const coding = interviewHref(id, "coding")
  if (!p || examined(p) === 0) return { kind: "link", label: "Commencer le codage", href: coding }
  const remaining = p.respondent - examined(p)
  if (remaining > 0) return { kind: "link", label: "Reprendre le codage", href: coding }
  const toHarmonise = r?.harmonisation_open ?? 0
  if (toHarmonise > 0) return { kind: "link", label: toHarmonise === 1 ? "Harmoniser la dernière unité" : `Harmoniser ${toHarmonise} unités`, href: interviewHref(id, "harmonise") }
  const open = s.phase2.n_divergences - s.phase2.n_arbitrated
  if (open > 0) return { kind: "link", label: open === 1 ? "Arbitrer le dernier" : `Arbitrer les ${open} restants`, href: interviewHref(id, "review") }
  if (s.phase2.n_ai_runs_complete === 0) return { kind: "link", label: "Lancer la revue IA", href: interviewHref(id, "ai") }
  return { kind: "link", label: "Voir le résultat", href: interviewHref(id, "cases") }
}

/** Search in the Bibliothèque: id and folder name. */
export function matchesInterview(s: InterviewSummary, q: string): boolean {
  const t = q.trim().toLowerCase()
  return !t || s.interview_id.toLowerCase().includes(t) || s.folder_name.toLowerCase().includes(t)
}
