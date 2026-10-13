// Optimistic edits of the passage list held by TanStack Query. The server
// answers with the real passage; until then the screen shows this guess, and
// rolls back to the saved list if the call fails.

import type { CodeNode, Passage, PassageCoding } from "./types"

export interface Target {
  unitId: string
  /** UTF-16 offsets; absent when the whole turn is meant. */
  start?: number
  end?: number
}

/**
 * True when the passage covers the whole turn. Computed from the offsets: the
 * server's `whole_unit` flag is true for any passage that starts at 0 (API gap).
 */
export function isWholePassage(p: Pick<Passage, "start_utf16" | "end_utf16">, unitLength: number): boolean {
  return p.start_utf16 === 0 && p.end_utf16 === unitLength
}

/** The passage that already has exactly this span (the server treats the same span as the same passage). */
export function findSpan(passages: Passage[], t: Target, unitLength: number): Passage | undefined {
  const whole = t.start === undefined || t.end === undefined || (t.start === 0 && t.end === unitLength)
  return passages.find((p) => {
    if (p.unit_id !== t.unitId || p.stale) return false
    if (whole) return isWholePassage(p, unitLength)
    return p.start_utf16 === t.start && p.end_utf16 === t.end
  })
}

export function hasCode(p: Passage, codeId: string): boolean {
  return p.codings.some((c) => c.code_id === codeId)
}

let tmp = 0

function localCoding(code: Pick<CodeNode, "code_id" | "name" | "system">, note = ""): PassageCoding {
  return { coding_id: -(++tmp), code_id: code.code_id, code_name: code.name, system: code.system, note, created_at: "", created_by: "" }
}

/** List after coding the target with `code` (or marking a highlight when `code` is null). */
export function withCoding(
  passages: Passage[],
  t: Target,
  unitText: string,
  interviewId: string,
  code: Pick<CodeNode, "code_id" | "name" | "system"> | null,
  note = "",
): Passage[] {
  const existing = findSpan(passages, t, unitText.length)
  if (existing) {
    if (!code || hasCode(existing, code.code_id)) return passages
    return passages.map((p) =>
      p === existing ? { ...p, kind: "coded", codings: [...p.codings, localCoding(code, note)] } : p,
    )
  }
  const whole = t.start === undefined || t.end === undefined
  const start = whole ? 0 : t.start!
  const end = whole ? unitText.length : t.end!
  const fresh: Passage = {
    passage_id: `tmp-${++tmp}`,
    interview_id: interviewId,
    unit_id: t.unitId,
    start,
    end,
    start_utf16: start,
    end_utf16: end,
    text: unitText.slice(start, end),
    whole_unit: whole || (start === 0 && end === unitText.length),
    segmentation_hash: "",
    anchor_digest: "",
    created_at: "",
    created_by: "",
    kind: code ? "coded" : "highlight",
    stale: false,
    before: "",
    after: "",
    codings: code ? [localCoding(code, note)] : [],
  }
  return [...passages, fresh]
}

/** List after removing a code from a passage; a passage left without code stays as a highlight. */
export function withoutCoding(passages: Passage[], passageId: string, codeId: string): Passage[] {
  return passages.map((p) => {
    if (p.passage_id !== passageId) return p
    const codings = p.codings.filter((c) => c.code_id !== codeId)
    return { ...p, codings, kind: codings.length ? "coded" : "highlight" }
  })
}

export function withoutPassage(passages: Passage[], passageId: string): Passage[] {
  return passages.filter((p) => p.passage_id !== passageId)
}

/** Replaces the passage with the server's version (or adds it). */
export function withServerPassage(passages: Passage[], fresh: Passage, replacing?: string): Passage[] {
  const without = passages.filter((p) => p.passage_id !== fresh.passage_id && p.passage_id !== replacing)
  return [...without, fresh]
}

/** Passages of one turn that carry the code (any span). */
export function passagesWithCode(passages: Passage[], unitId: string, codeId: string): Passage[] {
  return passages.filter((p) => p.unit_id === unitId && hasCode(p, codeId))
}

/** Codes present on exactly the target span (whole turn when no offsets): what the code list shows as applied and what a key toggles. */
export function appliedCodes(passages: Passage[], t: Target, unitLength: number): Set<string> {
  const out = new Set<string>()
  findSpan(passages, t, unitLength)?.codings.forEach((c) => out.add(c.code_id))
  return out
}

/** Codes on any passage of the turn, in order of first use (the tags under the turn). */
export function unitCodes(passages: Passage[], unitId: string): Set<string> {
  const out = new Set<string>()
  for (const p of passages) {
    if (p.unit_id !== unitId || p.stale) continue
    p.codings.forEach((c) => out.add(c.code_id))
  }
  return out
}
