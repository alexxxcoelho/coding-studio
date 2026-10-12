// Where an interview stands, and the next step to take.

import { interviewHref } from "./router"
import type { InterviewSummary } from "./types"

export interface NextStep {
  label: string
  href: string
  done: boolean
}

export function nextStep(s: InterviewSummary): NextStep {
  const id = s.interview_id
  const p1 = s.phase1
  const p2 = s.phase2
  if (s.n_units === 0) return { label: "Segment a transcript", href: interviewHref(id, "sources"), done: false }
  if (p1.n_human_coded === 0) return { label: `Pre-classify in phase 1 (0 of ${p1.n_units})`, href: interviewHref(id, "coding", 1), done: false }
  if (p2.n_units === 0)
    return { label: `Pre-classify more units (${p1.n_human_coded} of ${p1.n_units}, none relevant yet)`, href: interviewHref(id, "coding", 1), done: false }
  if (!p2.human_complete)
    return { label: `Code phase 2 (${p2.n_human_coded} of ${p2.n_units})`, href: interviewHref(id, "coding", 2), done: false }
  if (p2.n_ai_runs_complete === 0) return { label: "Run the AI challenge", href: interviewHref(id, "ai"), done: false }
  const open = p2.n_divergences - p2.n_arbitrated
  if (open > 0) return { label: `Review ${open} open disagreement${open === 1 ? "" : "s"}`, href: interviewHref(id, "review"), done: false }
  return { label: "Synthesise the cases, then export", href: interviewHref(id, "cases"), done: true }
}
