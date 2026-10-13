// Cross-case matrix: research questions as row groups, cases as columns.

import type { Case, CaseFieldKey, CrossCaseAnswer, CrossCaseCell } from "./types"

export function cellFor(cells: CrossCaseCell[], rq: string, caseId: string): CrossCaseCell | null {
  return cells.find((c) => c.rq === rq && c.case_id === caseId) ?? null
}

/** What a matrix cell says. `undocumented`: the case has no attributed unit, so
 * nothing can be said about the question (also when the cell is missing).
 * `unmapped`: documented, but no code of the case maps to the question.
 * `coded`: documented, with codes. */
export type CellState = "undocumented" | "unmapped" | "coded"

export function cellState(cell: CrossCaseCell | null): CellState {
  if (!cell || !cell.documented) return "undocumented"
  return cell.codes.length ? "coded" : "unmapped"
}

/** Codes of a cell, most supported first (number of units), then by code. */
export function cellCodes(cell: CrossCaseCell | null): { code: string; units: string[]; n: number }[] {
  if (!cell) return []
  return cell.codes
    .map((c) => ({ ...c, n: c.units.length }))
    .sort((a, b) => b.n - a.n || a.code.localeCompare(b.code))
}

/** For one research question: the codes present in at least `min` cases,
 * with the case ids, to read convergence and contrast across cases. */
export function sharedCodes(answer: CrossCaseAnswer, rq: string, min = 2): { code: string; cases: string[] }[] {
  const by = new Map<string, Set<string>>()
  for (const cell of answer.cells.filter((c) => c.rq === rq))
    for (const c of cell.codes) {
      const s = by.get(c.code) ?? new Set<string>()
      if (c.units.length) s.add(cell.case_id)
      by.set(c.code, s)
    }
  return [...by.entries()]
    .filter(([, s]) => s.size >= min)
    .map(([code, s]) => ({ code, cases: [...s].sort() }))
    .sort((a, b) => b.cases.length - a.cases.length || a.code.localeCompare(b.code))
}

/** A case field is filled when it has text or evidence. */
export function fieldFilled(c: Case, key: CaseFieldKey): boolean {
  const f = c[key]
  return !!f && (!!f.text.trim() || f.evidence.length > 0)
}

export const FIELD_LABELS: Record<CaseFieldKey, { title: string; hint: string }> = {
  trigger: { title: "Déclencheur", hint: "Ce qui a mis le projet en mouvement" },
  alternatives: { title: "Alternatives", hint: "Les options envisagées, y compris ne rien faire" },
  constraints: { title: "Contraintes", hint: "Ce qui a limité le choix : loi, partenaires, budget, compétences" },
  choice: { title: "Choix", hint: "Ce qui a été décidé, et par qui" },
  result: { title: "Résultat", hint: "Ce qui s'est passé, dans les termes du répondant" },
}

/** Options offered by the case form. Other values are free text. */
export const DECISIONS = ["adopted", "deferred", "rejected"] as const
export const GRANULARITIES = ["item", "batch", "model"] as const
export const ACCOUNT_TYPES = ["first-hand", "reported", "ecosystem", "anticipation"] as const

/** Unit ids typed by the user, one per line or separated by commas, spaces or
 * semicolons. Deduplicated and sorted, as the server stores them. */
export function parseUnitIds(text: string): string[] {
  const ids = text
    .split(/[\s,;]+/)
    .map((s) => s.trim())
    .filter(Boolean)
  return [...new Set(ids)].sort()
}

/** Comma-separated free text list (e.g. carriers rejected), trimmed, without empties. */
export function parseList(text: string): string[] {
  return [...new Set(text.split(",").map((s) => s.trim()).filter(Boolean))]
}

/** Evidence ids of a field that are not among the case's attributed units. */
export function unattributedEvidence(c: Case, key: CaseFieldKey): string[] {
  const units = new Set(c.units)
  return c[key].evidence.filter((u) => !units.has(u))
}

/** Unit ids a refused prompt generation lists after its colon, e.g.
 * "3 units of the batch are not yet coded by the candidate in phase 2: A-U001, A-U002". */
export function uncodedUnitIds(message: string): string[] {
  const tail = message.includes(":") ? message.slice(message.indexOf(":") + 1) : ""
  return parseList(tail.replace(/\s+/g, ""))
}
