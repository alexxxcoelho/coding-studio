// Where an interview stands, and the next step to take.

import { interviewHref } from "./router"
import type { InterviewSummary, Phase, PhaseSummary } from "./types"

export interface NextStep {
  label: string
  href: string
  done: boolean
}

function phaseStep(id: string, phase: Phase, s: PhaseSummary): NextStep | null {
  const name = `phase ${phase}`
  if (!s.human_complete)
    return { label: `Code ${name} (${s.n_human_coded} of ${s.n_units})`, href: interviewHref(id, "coding", phase), done: false }
  if (s.n_ai_runs_complete === 0)
    return { label: `Run the AI pass of ${name}`, href: interviewHref(id, "ai", phase), done: false }
  if (!s.reconciled)
    return {
      label: `Reconcile ${name} (${s.n_divergences - s.n_arbitrated} pending)`,
      href: interviewHref(id, "reconciliation", phase),
      done: false,
    }
  return null
}

export function nextStep(s: InterviewSummary): NextStep {
  const id = s.interview_id
  if (s.n_units === 0) return { label: "Segment a transcript", href: interviewHref(id, "sources"), done: false }
  const p1 = phaseStep(id, 1, s.phase1)
  if (p1) return p1
  if (s.phase2.n_units === 0)
    return { label: "Phase 1 reconciled, no relevant unit for phase 2", href: interviewHref(id, "audit"), done: true }
  const p2 = phaseStep(id, 2, s.phase2)
  if (p2) return p2
  return { label: "Both phases reconciled: export the package", href: interviewHref(id, "audit"), done: true }
}
