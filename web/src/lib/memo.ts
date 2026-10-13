// Memo of a research question: four written parts and two unit lists
// (supporting and opposing units), edited as text and saved as arrays.

import { parseUnitIds } from "./crosscase"
import type { RqMemo } from "./types"

export interface MemoDraft {
  documented: string
  interpretation: string
  rivals: string
  limits: string
  /** One unit id per line or separated by commas, as typed. */
  unitsFor: string
  unitsAgainst: string
}

export const MEMO_PARTS = [
  { key: "documented", title: "What the data document", hint: "The passages and cases that bear on the question, in the respondents' terms." },
  { key: "interpretation", title: "Interpretation", hint: "What the documented passages mean for the question. Say where the reading is thin." },
  { key: "rivals", title: "Rival explanations", hint: "Other readings the data also allow, and what would tell them apart." },
  { key: "limits", title: "Limits", hint: "What the data cannot show for this question, and who was not heard." },
] as const

export function emptyMemoDraft(): MemoDraft {
  return { documented: "", interpretation: "", rivals: "", limits: "", unitsFor: "", unitsAgainst: "" }
}

export function memoDraft(memo: RqMemo | null): MemoDraft {
  if (!memo) return emptyMemoDraft()
  return {
    documented: memo.documented,
    interpretation: memo.interpretation,
    rivals: memo.rivals,
    limits: memo.limits,
    unitsFor: memo.units_for.join("\n"),
    unitsAgainst: memo.units_against.join("\n"),
  }
}

/** Body of the PUT: unit lists parsed, deduplicated and sorted as the server stores them. */
export function memoPayload(d: MemoDraft): Omit<RqMemo, "rq_id" | "updated_at"> {
  return {
    documented: d.documented.trim(),
    interpretation: d.interpretation.trim(),
    rivals: d.rivals.trim(),
    limits: d.limits.trim(),
    units_for: parseUnitIds(d.unitsFor),
    units_against: parseUnitIds(d.unitsAgainst),
  }
}

/** True when the draft would change the saved memo. */
export function memoDirty(d: MemoDraft, memo: RqMemo | null): boolean {
  const next = memoPayload(d)
  const base = memo ?? { documented: "", interpretation: "", rivals: "", limits: "", units_for: [], units_against: [] }
  return (
    next.documented !== base.documented.trim() ||
    next.interpretation !== base.interpretation.trim() ||
    next.rivals !== base.rivals.trim() ||
    next.limits !== base.limits.trim() ||
    next.units_for.join() !== [...base.units_for].sort().join() ||
    next.units_against.join() !== [...base.units_against].sort().join()
  )
}
