import { describe, expect, it } from "vitest"

import { memoDirty, memoDraft, memoPayload } from "./memo"
import { cumulativeCodes, inOrder, interviewLabel, saturationTotals } from "./saturation"
import type { RqMemo, SaturationRow } from "./types"

const memo: RqMemo = {
  rq_id: "RQ1",
  documented: "Le dossier pèse.",
  interpretation: "Lecture prudente.",
  rivals: "Inertie",
  limits: "Deux interviews",
  units_for: ["ECO-07-U002", "ECO-07-U010"],
  units_against: ["RET-22-U001"],
  updated_at: "2026-10-12T10:00:00Z",
}

describe("research question memo", () => {
  it("round-trips a saved memo through the draft without change", () => {
    const d = memoDraft(memo)
    expect(memoDirty(d, memo)).toBe(false)
    expect(memoPayload(d)).toEqual({
      documented: memo.documented,
      interpretation: memo.interpretation,
      rivals: memo.rivals,
      limits: memo.limits,
      units_for: memo.units_for,
      units_against: memo.units_against,
    })
  })
  it("parses unit lists typed with commas, spaces or lines, deduplicated and sorted", () => {
    const d = { ...memoDraft(null), unitsFor: "ECO-07-U010, ECO-07-U002\nECO-07-U002", unitsAgainst: "" }
    expect(memoPayload(d).units_for).toEqual(["ECO-07-U002", "ECO-07-U010"])
    expect(memoDirty(d, null)).toBe(true)
  })
  it("starts blank when no memo exists", () => {
    expect(memoDirty(memoDraft(null), null)).toBe(false)
  })
})

const row = (order: number, codes: string[], fields: string[] = [], opened = 0, resolved = 0): SaturationRow => ({
  interview_id: `ECO-0${order}`,
  order,
  first_coded_at: "",
  codes_first_used: codes,
  field_codes_created: fields,
  labels_opened: opened,
  labels_resolved: resolved,
})

describe("saturation", () => {
  const rows = [row(2, ["B"]), row(1, ["A", "C"], ["X"], 2, 1), row(3, [])]
  it("orders interviews and accumulates first uses", () => {
    expect(inOrder(rows).map((r) => r.order)).toEqual([1, 2, 3])
    expect(cumulativeCodes(rows)).toEqual([2, 3, 3])
  })
  it("totals the columns", () => {
    expect(saturationTotals(rows)).toEqual({ codes: 3, fieldCodes: 1, labelsOpened: 2, labelsResolved: 1 })
  })
  it("labels an unattributed interview", () => {
    expect(interviewLabel("")).toBe("not attributed")
    expect(interviewLabel("ECO-07")).toBe("ECO-07")
  })
})
