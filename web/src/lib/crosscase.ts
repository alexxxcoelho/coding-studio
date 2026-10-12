// Cross-case matrix: research questions as row groups, cases as columns.

import type { Case, CaseFieldKey, CrossCaseAnswer, CrossCaseCell } from "./types"

export function cellFor(cells: CrossCaseCell[], rq: string, caseId: string): CrossCaseCell | null {
  return cells.find((c) => c.rq === rq && c.case_id === caseId) ?? null
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
  trigger: { title: "Trigger", hint: "What set the project in motion" },
  alternatives: { title: "Alternatives", hint: "The options considered, including doing nothing" },
  constraints: { title: "Constraints", hint: "What limited the choice: law, partners, budget, skills" },
  choice: { title: "Choice", hint: "What was decided, by whom" },
  result: { title: "Result", hint: "What happened, in the respondent's terms" },
}
