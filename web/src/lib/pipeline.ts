// Where an interview stands in the four steps of the Bibliothèque
// (Segmenté, Codé, Revue IA facultative, Arbitré), and the next step to take.

import { hasLater, hasRealCode } from "./coder-model"
import { interviewHref } from "./router"
import type { InterviewSummary, Passage } from "./types"

/** What the passages say about an interview's respondent turns. */
export interface CodingProgress {
  /** Turns that are not the interviewer's. */
  respondent: number
  /** Respondent turns marked not relevant. */
  nonPertinent: number
  coded: number
  later: number
}

/** Progress from the passages of an interview and the counts of its `/simple` view. */
export function codingProgress(passages: Passage[], simple: { n_respondent_units: number; n_relevant: number }): CodingProgress {
  const coded = new Set<string>()
  const later = new Set<string>()
  for (const p of passages) {
    if (p.stale) continue
    if (hasRealCode(p)) coded.add(p.unit_id)
    else if (hasLater(p)) later.add(p.unit_id)
  }
  for (const u of coded) later.delete(u)
  const nonPertinent = Math.max(0, simple.n_respondent_units - simple.n_relevant)
  return { respondent: simple.n_respondent_units, nonPertinent, coded: coded.size, later: later.size }
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

export function pipelineSteps(s: InterviewSummary, p: CodingProgress | null): Step[] {
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
    {
      key: "ai",
      label: ai > 0 ? `Revue IA · ${ai} passe${ai > 1 ? "s" : ""}` : "Revue IA facultative",
      value: ai > 0 ? 1 : 0,
      optional: ai === 0,
    },
    {
      key: "arbitrated",
      label: p2.n_divergences > 0 ? `Arbitré ${p2.n_arbitrated}/${p2.n_divergences}` : "Arbitrage",
      value: p2.n_divergences > 0 ? p2.n_arbitrated / p2.n_divergences : 0,
    },
  ]
}

export type Stage = "new" | "in_progress" | "ready"

/** Prêt: every respondent turn examined and no open disagreement. Not segmented counts as en cours. */
export function stageOf(s: InterviewSummary, p: CodingProgress | null): Stage {
  if (s.n_units === 0) return "new"
  if (!p) return "in_progress"
  const openDisagreements = Math.max(0, s.phase2.n_divergences - s.phase2.n_arbitrated)
  const remaining = p.respondent - examined(p)
  return p.respondent > 0 && remaining <= 0 && openDisagreements === 0 ? "ready" : "in_progress"
}

export type NextAction =
  | { kind: "segment"; label: string }
  | { kind: "link"; label: string; href: string }

export function nextAction(s: InterviewSummary, p: CodingProgress | null): NextAction {
  const id = s.interview_id
  if (s.n_units === 0) return { kind: "segment", label: "Segmenter…" }
  const coding = interviewHref(id, "coding")
  if (!p || examined(p) === 0) return { kind: "link", label: "Commencer le codage", href: coding }
  const remaining = p.respondent - examined(p)
  if (remaining > 0) return { kind: "link", label: "Reprendre le codage", href: coding }
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
