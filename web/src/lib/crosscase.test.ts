import { describe, expect, it } from "vitest"

import { cellCodes, cellState, parseList, parseUnitIds, uncodedUnitIds, unattributedEvidence } from "./crosscase"
import type { Case, CrossCaseCell } from "./types"

const cell = (over: Partial<CrossCaseCell> = {}): CrossCaseCell => ({ rq: "RQ1", case_id: "ECO-07-C1", documented: true, codes: [], ...over })

describe("cell state", () => {
  it("marks a case without attributed units as not documented", () => {
    expect(cellState(cell({ documented: false }))).toBe("undocumented")
    expect(cellState(null)).toBe("undocumented")
    expect(cellCodes(cell({ documented: false }))).toEqual([])
  })
  it("tells a documented case with no mapped code from a coded one", () => {
    expect(cellState(cell())).toBe("unmapped")
    expect(cellState(cell({ codes: [{ code: "LEGAL", units: ["ECO-07-U001"] }] }))).toBe("coded")
  })
})

describe("unit ids typed in the case form", () => {
  it("accepts lines, commas, spaces and semicolons; drops duplicates and sorts", () => {
    expect(parseUnitIds("RET-22-U003\nECO-07-U012, ECO-07-U012 ;RET-22-U001")).toEqual(["ECO-07-U012", "RET-22-U001", "RET-22-U003"].sort())
    expect(parseUnitIds("  \n ,")).toEqual([])
  })
  it("reads a comma-separated free text list", () => {
    expect(parseList("Swisscom, , Sunrise,Swisscom")).toEqual(["Swisscom", "Sunrise"])
  })
})

const kase = (over: Partial<Case>): Case => ({
  case_id: "ECO-07-C1",
  interview_id: "ECO-07",
  label: "x",
  trigger: { text: "", contradictions: "", missing: "", evidence: [] },
  alternatives: { text: "", contradictions: "", missing: "", evidence: [] },
  constraints: { text: "", contradictions: "", missing: "", evidence: ["ECO-07-U001", "RET-22-U002"] },
  choice: { text: "", contradictions: "", missing: "", evidence: [] },
  result: { text: "", contradictions: "", missing: "", evidence: [] },
  units: ["ECO-07-U001"],
  interviews: ["ECO-07"],
  decision: null,
  granularity: null,
  carrier_chosen: null,
  carriers_rejected: [],
  account_type: null,
  swiss_link: null,
  stage: null,
  outcome_status: null,
  notes: null,
  ...over,
})

describe("evidence outside the attributed units", () => {
  it("lists the evidence ids that the case does not attribute", () => {
    expect(unattributedEvidence(kase({}), "constraints")).toEqual(["RET-22-U002"])
    expect(unattributedEvidence(kase({}), "trigger")).toEqual([])
  })
})

describe("refused prompt generation", () => {
  it("reads the uncoded unit ids from the server message", () => {
    const msg = "2 units of the batch are not yet coded by the candidate in phase 2: ECO-07-U004, ECO-07-U009"
    expect(uncodedUnitIds(msg)).toEqual(["ECO-07-U004", "ECO-07-U009"])
    expect(uncodedUnitIds("no colon here")).toEqual([])
  })
})
