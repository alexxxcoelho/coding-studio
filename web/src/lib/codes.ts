// Code sets, families and the comparison helpers used across screens.

import type { Code } from "./types"

export const NA = "NA"

export const MODIFIERS = [
  "ENABLER",
  "BARRIER",
  "CONTESTED",
  "ABSENT",
  "CONTEXT-DEPENDENT",
  "CHANGE-OVER-TIME",
  "INTERACTION",
  "NECESSARY-NOT-SUFFICIENT",
] as const

export const EV_LEVELS = ["EV0", "EV1", "EV2", "EV3", "EV4"] as const

export const PROVENANCES = ["SPONTANEOUS", "PROBED", "DOCUMENT-DERIVED", "RESEARCHER-INFERRED"] as const

/** Display order of families, TOE contexts first. */
export const FAMILY_ORDER = [
  "TECH",
  "ORG",
  "ENV",
  "OUTCOME",
  "TURNING-POINT",
  "TRANSFER",
  "STAGE",
  "CONTEXT",
  "INDUCTIVE",
]

const FAMILY_NAMES: Record<string, string> = {
  TECH: "Technology",
  ORG: "Organisation",
  ENV: "Environment",
  OUTCOME: "Outcome",
  "TURNING-POINT": "Turning point",
  TRANSFER: "Transfer",
  STAGE: "Stage",
  CONTEXT: "Context",
  INDUCTIVE: "Inductive",
}

export function familyName(family: string): string {
  return FAMILY_NAMES[family] ?? family
}

/** CSS variable key of a family colour (see index.css, --fam-*). */
export function familyKey(family: string): string {
  switch (family) {
    case "TECH":
      return "tech"
    case "ORG":
      return "org"
    case "ENV":
      return "env"
    case "OUTCOME":
      return "outcome"
    case "TURNING-POINT":
      return "turn"
    case "TRANSFER":
      return "transfer"
    case "STAGE":
      return "stage"
    case "CONTEXT":
      return "context"
    case "INDUCTIVE":
      return "inductive"
    default:
      return "na"
  }
}

/** Family of a code string when the code object is not at hand. */
export function familyOf(code: string, book?: Code[]): string {
  const known = book?.find((c) => c.code === code)
  if (known) return known.family
  if (code === NA) return "NA"
  if (code === "INDUCTIVE-PENDING") return "INDUCTIVE"
  if (code.startsWith("TECH")) return "TECH"
  if (code.startsWith("ORG")) return "ORG"
  if (code.startsWith("ENV")) return "ENV"
  if (code.startsWith("OUT")) return "OUTCOME"
  if (code.startsWith("TURN")) return "TURNING-POINT"
  if (code.startsWith("TRANSFER") || code === "SME-TRANSFER") return "TRANSFER"
  if (code.startsWith("STAGE")) return "STAGE"
  if (code === "CONTEXT" || code === "CASE-METADATA") return "CONTEXT"
  return "INDUCTIVE"
}

/** Toggles a code in a set. NA is exclusive: choosing it clears the rest,
 * choosing any other code removes NA. Result is sorted. */
export function toggleCode(current: string[], code: string): string[] {
  const has = current.includes(code)
  let next: string[]
  if (has) next = current.filter((c) => c !== code)
  else if (code === NA) next = [NA]
  else next = [...current.filter((c) => c !== NA), code]
  return [...next].sort()
}

/** Case-insensitive match on code, label or family name. */
export function matchesQuery(code: Code, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return [code.code, code.label, familyName(code.family), code.definition]
    .some((s) => s.toLowerCase().includes(q))
}

/** Groups codes by family in display order, keeping codebook order within. */
export function groupByFamily(codes: Code[]): { family: string; codes: Code[] }[] {
  const map = new Map<string, Code[]>()
  for (const c of codes) {
    const list = map.get(c.family) ?? []
    list.push(c)
    map.set(c.family, list)
  }
  const rank = (f: string) => {
    const i = FAMILY_ORDER.indexOf(f)
    return i === -1 ? FAMILY_ORDER.length : i
  }
  return [...map.entries()]
    .sort((a, b) => rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]))
    .map(([family, list]) => ({ family, codes: list }))
}

export type DiffMark = "shared" | "only"

/** Marks each code of `side` as shared with `other` or only on this side. */
export function diffCodes(side: string[], other: string[]): { code: string; mark: DiffMark }[] {
  const o = new Set(other)
  return [...side].sort().map((code) => ({ code, mark: o.has(code) ? "shared" : "only" }))
}

export function sameSet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false
  const s = new Set(a)
  return b.every((x) => s.has(x))
}
