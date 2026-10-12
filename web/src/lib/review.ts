// Review of the AI challenge: order and progress.

import { sameSet } from "./codes"
import type { ReviewItem } from "./types"

const unitOrder = (a: ReviewItem, b: ReviewItem) => a.unit_id.localeCompare(b.unit_id, undefined, { numeric: true })

/** Disagreements first, then the sampled agreements, each in unit order.
 * With `openOnly`, decided items are left out. */
export function orderReview(items: ReviewItem[], openOnly = false): ReviewItem[] {
  const keep = openOnly ? items.filter((i) => !i.decision) : items
  return [
    ...keep.filter((i) => i.kind === "disagreement").sort(unitOrder),
    ...keep.filter((i) => i.kind !== "disagreement").sort(unitOrder),
  ]
}

export interface ReviewProgress {
  disagreements: number
  disagreementsDecided: number
  sampled: number
  sampledReviewed: number
  /** Every disagreement has a decision (sampled agreements are optional). */
  complete: boolean
}

export function reviewProgress(items: ReviewItem[]): ReviewProgress {
  const dis = items.filter((i) => i.kind === "disagreement")
  const smp = items.filter((i) => i.kind !== "disagreement")
  const disDone = dis.filter((i) => i.decision).length
  return {
    disagreements: dis.length,
    disagreementsDecided: disDone,
    sampled: smp.length,
    sampledReviewed: smp.filter((i) => i.decision).length,
    complete: disDone === dis.length,
  }
}

/** A rationale is required for a disagreement, optional for a sampled agreement. */
export function rationaleRequired(item: ReviewItem): boolean {
  return item.kind === "disagreement"
}

/** What the review shows for the candidate's coding of an item: the frozen
 * snapshot (the reference the decision is taken against), and the current
 * coding when the candidate has revised it since the snapshot. */
export interface CandidateView {
  frozen: string[]
  current: string[]
  revised: boolean
}

export function candidateView(item: ReviewItem): CandidateView {
  const current = item.human_current ?? item.human
  const revised = item.revised_since_snapshot || !sameSet(item.human, current)
  return { frozen: item.human, current, revised }
}
