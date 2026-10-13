// What the Coder screen derives from the units and the passages of an
// interview: the state of each turn, the filters, the counters, the text cut
// into pieces for display, and the rule of the end-of-interview panel.

import { LATER_ID } from "./code-tree"
import type { Passage, UnitRow } from "./types"

export type UnitState = "interviewer" | "non_pertinent" | "later" | "coded" | "remaining"

/** A turn as the screen sees it. */
export interface CoderUnit {
  unit_id: string
  seq: number
  role: "interviewer" | "respondent" | "unknown"
  speaker: string
  timestamp: string
  text: string
  /** The candidate marked the turn not relevant (phase 1 NA). */
  notRelevant: boolean
}

export function unitsFromRows(rows: UnitRow[]): CoderUnit[] {
  return rows.map((r) => ({
    unit_id: r.unit.unit_id,
    seq: r.unit.seq,
    role: r.unit.role,
    speaker: r.unit.speaker_raw,
    timestamp: r.unit.timestamp,
    text: r.unit.text,
    notRelevant: r.unit.role !== "interviewer" && (r.coding?.codes.length === 1 && r.coding.codes[0] === "NA"),
  }))
}

/** Interviewer turns are context only; every other turn is codable. */
export function isCodable(u: CoderUnit): boolean {
  return u.role !== "interviewer"
}

export function hasLater(p: Passage): boolean {
  return p.codings.some((c) => c.system || c.code_id === LATER_ID)
}

export function hasRealCode(p: Passage): boolean {
  return p.codings.some((c) => !c.system && c.code_id !== LATER_ID)
}

/**
 * State of a turn. Order of precedence: interviewer, not relevant, coded,
 * to review (marked "à revoir" with R, or parked under Plus tard), remaining. A turn with a code is coded even when a passage of it
 * is also parked; a turn with only highlights is still remaining.
 */
export function unitState(u: CoderUnit, passages: Passage[], marked = false): UnitState {
  if (!isCodable(u)) return "interviewer"
  if (u.notRelevant) return "non_pertinent"
  if (passages.some(hasRealCode)) return "coded"
  if (marked || passages.some(hasLater)) return "later"
  return "remaining"
}

export function groupByUnit(passages: Passage[]): Map<string, Passage[]> {
  const out = new Map<string, Passage[]>()
  for (const p of passages) {
    const list = out.get(p.unit_id) ?? []
    list.push(p)
    out.set(p.unit_id, list)
  }
  return out
}

export type CoderFilter = "all" | "remaining" | "later" | "non_pertinent" | "coded"

export const FILTER_LABELS: { key: CoderFilter; label: string }[] = [
  { key: "all", label: "Toutes" },
  { key: "remaining", label: "Restantes" },
  { key: "later", label: "À revoir" },
  { key: "non_pertinent", label: "Non pertinentes" },
  { key: "coded", label: "Codées" },
]

export interface Counts {
  respondent: number
  coded: number
  later: number
  non_pertinent: number
  remaining: number
  /** Coded, not relevant or to review: the turns the researcher has dealt with. */
  examined: number
  /** Turns of the "À revoir" filter: every marked turn, coded or not, and those parked under Plus tard. */
  review: number
}

export function countStates(units: CoderUnit[], byUnit: Map<string, Passage[]>, marks: ReadonlySet<string> = new Set()): Counts {
  const c: Counts = { respondent: 0, coded: 0, later: 0, non_pertinent: 0, remaining: 0, examined: 0, review: 0 }
  for (const u of units) {
    const s = unitState(u, byUnit.get(u.unit_id) ?? [], marks.has(u.unit_id))
    if (s === "interviewer") continue
    c.respondent++
    c[s]++
    if (inReviewFilter(u, s, marks)) c.review++
  }
  c.examined = c.coded + c.later + c.non_pertinent
  return c
}

/** A turn the "À revoir" filter lists: marked with R (whatever its coding), or parked under Plus tard. */
function inReviewFilter(u: CoderUnit, state: UnitState, marks: ReadonlySet<string>): boolean {
  return state !== "interviewer" && state !== "non_pertinent" && (state === "later" || marks.has(u.unit_id))
}

/** Turns shown for a filter. Interviewer turns stay in "all" and are hidden by the others (they are context, not work). */
export function applyFilter(units: CoderUnit[], byUnit: Map<string, Passage[]>, filter: CoderFilter, search = "", marks: ReadonlySet<string> = new Set()): CoderUnit[] {
  const q = search.trim().toLowerCase()
  return units.filter((u) => {
    if (q && !u.text.toLowerCase().includes(q) && !u.unit_id.toLowerCase().includes(q)) return false
    if (filter === "all") return true
    const state = unitState(u, byUnit.get(u.unit_id) ?? [], marks.has(u.unit_id))
    return filter === "later" ? inReviewFilter(u, state, marks) : state === filter
  })
}

/** The next or previous unit in the shown list that can be selected (all of them can: interviewer turns give context). */
export function neighbour(shown: CoderUnit[], current: string | null, dir: 1 | -1): string | null {
  if (shown.length === 0) return null
  const i = shown.findIndex((u) => u.unit_id === current)
  if (i === -1) return shown[dir === 1 ? 0 : shown.length - 1].unit_id
  const j = Math.min(shown.length - 1, Math.max(0, i + dir))
  return shown[j].unit_id
}

/** The unit to select first: the one asked for, else the first remaining respondent turn, else the first respondent turn. */
export function initialUnit(units: CoderUnit[], byUnit: Map<string, Passage[]>, asked: string | null): string | null {
  if (asked && units.some((u) => u.unit_id === asked)) return asked
  const first = units.find((u) => unitState(u, byUnit.get(u.unit_id) ?? []) === "remaining")
  return (first ?? units.find(isCodable) ?? units[0])?.unit_id ?? null
}

// ---------------------------------------------------------------- end of interview

/**
 * The "Fin d'entretien" panel opens when every respondent turn has been dealt
 * with: coded, not relevant or to review (docs/guide/fr/02_coder.md, action 11).
 * An interview without respondent turn never opens it.
 */
export function isEndOfInterview(counts: Counts): boolean {
  return counts.respondent > 0 && counts.remaining === 0
}

/** All items checked. The list informs and never locks. */
export function checklistComplete(items: { checked: boolean }[]): boolean {
  return items.length > 0 && items.every((i) => i.checked)
}

/** Items still to check, by label. */
export function checklistMissing(items: { label: string; checked: boolean }[]): string[] {
  return items.filter((i) => !i.checked).map((i) => i.label)
}

/** The checklist item that carries the end-of-interview memo note (id `end_memo`). */
export const END_MEMO_ITEM = "end_memo"

// ---------------------------------------------------------------- text pieces

export interface Piece {
  start: number
  end: number
  text: string
  passages: Passage[]
}

/**
 * Cuts a turn's text at every boundary of its passages (UTF-16 offsets) so
 * that each piece carries the passages covering it. A stale passage, or one
 * without UTF-16 offsets, is left out. Pieces are contiguous and cover the text.
 */
export function pieces(text: string, passages: Passage[]): Piece[] {
  const live = passages.filter((p) => !p.stale && p.start_utf16 !== undefined && p.end_utf16 !== undefined)
  const cuts = new Set<number>([0, text.length])
  for (const p of live) {
    cuts.add(Math.max(0, Math.min(text.length, p.start_utf16!)))
    cuts.add(Math.max(0, Math.min(text.length, p.end_utf16!)))
  }
  const sorted = [...cuts].sort((a, b) => a - b)
  const out: Piece[] = []
  for (let i = 0; i < sorted.length - 1; i++) {
    const start = sorted[i]
    const end = sorted[i + 1]
    if (end <= start) continue
    out.push({
      start,
      end,
      text: text.slice(start, end),
      passages: live.filter((p) => p.start_utf16! <= start && p.end_utf16! >= end),
    })
  }
  return out.length ? out : [{ start: 0, end: text.length, text, passages: [] }]
}

/** Passages that carry a real code, with the codes in order of first use: the stripes of the gutter. */
export function stripeCodes(passages: Passage[]): { code_id: string; name: string }[] {
  const seen = new Map<string, string>()
  for (const p of passages) {
    for (const c of p.codings) {
      if (c.system || c.code_id === LATER_ID) continue
      if (!seen.has(c.code_id)) seen.set(c.code_id, c.code_name)
    }
  }
  return [...seen].map(([code_id, name]) => ({ code_id, name }))
}

/** The passage a click or a key acts on: the whole-turn passage if any, else the first one that covers the offset. */
export function passageAt(passages: Passage[], offset: number): Passage | null {
  const hit = passages.filter((p) => !p.stale && p.start_utf16 !== undefined && p.start_utf16 <= offset && p.end_utf16! > offset)
  if (hit.length === 0) return null
  return [...hit].sort((a, b) => a.end_utf16! - a.start_utf16! - (b.end_utf16! - b.start_utf16!))[0]
}
