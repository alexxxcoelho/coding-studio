// "Créer un code" from selected keywords: the new code dialog opens with the
// terms as name and the highlights that contain them as the passages to code.

import type { NewCodeTarget } from "@/components/coder/new-code-dialog"
import type { CodePrefill } from "./lexicon"
import type { Passage } from "./types"

/**
 * Resolves the highlights behind a prefill. The first one becomes the origin of
 * the code (the dialog creates the code from it), the others are coded after.
 * Highlights that disappeared or went stale since the table was built are left
 * out; null when none is left.
 */
export async function codeTargetFromTerms(
  prefill: CodePrefill,
  loadHighlights: (interviewId: string) => Promise<Passage[]>,
): Promise<{ interviewId: string; target: NewCodeTarget } | null> {
  const lists = await Promise.all(prefill.interviews.map((id) => loadHighlights(id)))
  const byId = new Map(lists.flat().filter((p) => p.kind === "highlight" && !p.stale).map((p) => [p.passage_id, p]))
  const found = prefill.passageIds.map((id) => byId.get(id)).filter((p): p is Passage => p !== undefined)
  if (found.length === 0) return null
  const [first, ...others] = found
  return {
    interviewId: first.interview_id,
    target: { unitId: first.unit_id, passageId: first.passage_id, range: null, text: first.text, name: prefill.name, alsoPassageIds: others.map((p) => p.passage_id) },
  }
}
