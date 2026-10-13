// Code sets, families and the comparison helpers used across screens.

import type { Code, Toe } from "./types"
import { t } from "@/i18n"

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
  "OTHER",
]

const FAMILY_NAMES: Record<string, string> = {
  get TECH() { return t("codes.technologie") },
  get ORG() { return t("common.organisation") },
  get ENV() { return t("codes.environnement") },
  get OUTCOME() { return t("common.resultat") },
  get "TURNING-POINT"() { return t("codes.point_de_bascule") },
  get TRANSFER() { return t("codes.transfert") },
  get STAGE() { return t("common.etape") },
  get CONTEXT() { return t("codes.contexte") },
  get INDUCTIVE() { return t("codes.inductif") },
  get OTHER() { return t("codes.hors_contexte_toe") },
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
    case "OTHER":
      return "outcome"
    default:
      return "na"
  }
}

/** Family of a code string when the code object is not at hand. */
export function familyOf(code: string, book?: Code[]): string {
  const known = book?.find((c) => c.code === code)
  if (known) return known.family
  if (code === NA) return "NA"
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

/** Phase 1 single-key shortcuts: one letter per category, plus NA. */
export const PHASE1_HOTKEYS: Record<string, string> = {
  t: "TECH",
  o: "ORG",
  e: "ENV",
  c: "CONTEXT",
  n: NA,
}

/** Letter shown next to a code, when it has a shortcut in this phase. */
export function hotkeyFor(code: string, phase: number): string | null {
  if (phase !== 1) return null
  const hit = Object.entries(PHASE1_HOTKEYS).find(([, c]) => c === code)
  return hit ? hit[0].toUpperCase() : null
}

/** Code toggled by a key press, or null. Only in phase 1, only for a code
 * the unit allows (NA always). Case-insensitive; modifiers are the caller's
 * concern. */
export function codeForKey(key: string, phase: number, allowed: string[]): string | null {
  if (phase !== 1 || key.length !== 1) return null
  const code = PHASE1_HOTKEYS[key.toLowerCase()]
  if (!code) return null
  return code === NA || allowed.includes(code) ? code : null
}

/** Adds a code to a set (no removal). NA replaces everything; any other
 * code removes NA. Result is sorted. */
export function addCode(current: string[], code: string): string[] {
  if (code === NA) return [NA]
  if (current.includes(code)) return [...current].sort()
  return [...current.filter((c) => c !== NA), code].sort()
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

/** Family of a phase 1 context, by TOE letter. */
export const TOE_FAMILY: Record<Toe, string> = { T: "TECH", O: "ORG", E: "ENV" }
const TOE_NAME: Record<Toe, string> = { T: "technologique", O: "organisationnel", E: "environnemental" }

/** TOE attachment of a code: its `toe` field, else derived from its family. */
export function toeOf(code: Code): Toe | null {
  if (code.toe) return code.toe
  if (code.family === "TECH") return "T"
  if (code.family === "ORG") return "O"
  if (code.family === "ENV") return "E"
  return null
}

export interface ToeGroup {
  key: Toe | "OTHER"
  title: string
  /** The unit was pre-classified in this context in phase 1. */
  highlighted: boolean
  codes: Code[]
}

/** Phase 2 picker groups: the TOE contexts of the unit's phase 1
 * pre-classification first (highlighted), then the other contexts in T, O,
 * E order, then codes attached to no context. Codebook order within. */
export function groupByToe(codes: Code[], phase1: string[] = []): ToeGroup[] {
  const mine = (["T", "O", "E"] as Toe[]).filter((t) => phase1.includes(TOE_FAMILY[t]))
  const order: (Toe | "OTHER")[] = [...mine, ...(["T", "O", "E"] as Toe[]).filter((t) => !mine.includes(t)), "OTHER"]
  return order
    .map((key) => ({
      key,
      title: key === "OTHER" ? t("codes.hors_contexte_toe") : `Contexte ${TOE_NAME[key]}`,
      highlighted: key !== "OTHER" && mine.includes(key),
      codes: codes.filter((c) => (toeOf(c) ?? "OTHER") === key),
    }))
    .filter((g) => g.codes.length > 0)
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

/** Phase 2 codes the candidate can map a label to. */
export function phase2Codes(codes: Code[]): Code[] {
  return codes.filter((c) => c.active && !c.merged_into && c.origin !== "legacy" && c.origin !== "phase1")
}

/** Upper-case kebab key proposed from a label. */
export function codeKeyFrom(label: string): string {
  return label
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
}
