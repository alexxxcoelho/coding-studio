// Passages of a corrected transcript that the researcher must look at again:
// the open items of the review, per interview, shared by the Coder toolbar, the
// Bibliothèque and the review dialog.

import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query"

import { invalidatePassages } from "./hooks"
import { v2 } from "./v2-api"

export const trKeys = {
  open: ["transcript-review", "open"] as const,
  items: (id: string) => ["transcript-review", "items", id] as const,
  versions: (id: string) => ["transcript-review", "versions", id] as const,
}

/** Number of passages to review per interview (only interviews with some). */
export function useReviewCounts(): Map<string, number> {
  const q = useQuery({ queryKey: trKeys.open, queryFn: () => v2.review({ status: "needs_review" }), staleTime: 15_000 })
  const out = new Map<string, number>()
  for (const it of q.data?.items ?? []) out.set(it.interview_id, (out.get(it.interview_id) ?? 0) + 1)
  return out
}

export function refreshReview(qc: QueryClient, interviewId: string): Promise<unknown> {
  return Promise.all([qc.invalidateQueries({ queryKey: ["transcript-review"] }), invalidatePassages(qc, interviewId)])
}

export function useRefreshReview(interviewId: string) {
  const qc = useQueryClient()
  return () => refreshReview(qc, interviewId)
}
