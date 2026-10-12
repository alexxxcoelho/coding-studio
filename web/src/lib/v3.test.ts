import { describe, expect, it } from "vitest"

import { groupByLabel, isCoded, knownLabels, labelKey } from "./clarify"
import { cellCodes, cellFor, fieldFilled, sharedCodes } from "./crosscase"
import { orderReview, rationaleRequired, reviewProgress } from "./review"
import type { Case, CrossCaseAnswer, ReviewItem, ToClarify, Unit } from "./types"

const tc = (id: number, iv: string, u: number, label: string, resolved = false): ToClarify => ({
  id,
  interview_id: iv,
  unit_id: `${iv}-U${String(u).padStart(3, "0")}`,
  label,
  note: "",
  created_at: "",
  created_by: "c",
  resolved: resolved ? { kind: "mapped", code: "X", at: "", by: "c", rationale: "r" } : null,
})

describe("to clarify", () => {
  it("groups labels across interviews, ignoring case and spacing", () => {
    const g = groupByLabel([tc(1, "ECO-07", 3, "Waiting for others"), tc(2, "RET-22", 1, "waiting  for others "), tc(3, "ECO-07", 1, "Pilot fatigue")])
    expect(g.map((x) => [x.label, x.items.length])).toEqual([["Waiting for others", 2], ["Pilot fatigue", 1]])
    expect(g[0].interviews).toEqual(["ECO-07", "RET-22"])
    expect(labelKey(" A  b ")).toBe("a b")
  })
  it("offers only open labels for autocomplete", () => {
    expect(knownLabels([tc(1, "A-1", 1, "open one"), tc(2, "A-1", 2, "done", true)])).toEqual(["open one"])
  })
  it("counts a unit as coded with codes or an open label", () => {
    expect(isCoded(null, [])).toBe(false)
    expect(isCoded(null, [tc(1, "A-1", 1, "x")])).toBe(true)
    expect(isCoded(null, [tc(1, "A-1", 1, "x", true)])).toBe(false)
    expect(isCoded({ unit_id: "u", codes: ["LEGAL"], subcode: null, modifier: null, ev_level: null, provenance: null, memo: null, excerpt: null })).toBe(true)
  })
})

const unit = (id: string): Unit => ({ unit_id: id, seq: 1, speaker_raw: "RESP", role: "respondent", timestamp: "00:01", text: "t" })
const ri = (id: string, kind: ReviewItem["kind"], decided = false): ReviewItem => ({
  unit_id: id,
  kind,
  human: ["A"],
  human_current: ["A"],
  revised_since_snapshot: false,
  ai: { R1: kind === "disagreement" ? ["B"] : ["A"] },
  decision: decided
    ? { unit_id: id, phase: 2, choice: { type: "human" }, final_codes: ["A"], rationale: "", decided_by: "c", decided_at: "" }
    : null,
  unit: unit(id),
})

describe("review", () => {
  const items = [ri("X-U010", "agreement_sample"), ri("X-U002", "disagreement", true), ri("X-U009", "disagreement"), ri("X-U001", "agreement_sample", true)]
  it("orders disagreements first, then sampled agreements, in unit order", () => {
    expect(orderReview(items).map((i) => i.unit_id)).toEqual(["X-U002", "X-U009", "X-U001", "X-U010"])
    expect(orderReview(items, true).map((i) => i.unit_id)).toEqual(["X-U009", "X-U010"])
  })
  it("reports progress; complete once every disagreement is decided", () => {
    expect(reviewProgress(items)).toEqual({ disagreements: 2, disagreementsDecided: 1, sampled: 2, sampledReviewed: 1, complete: false })
    expect(rationaleRequired(items[1])).toBe(true)
    expect(rationaleRequired(items[0])).toBe(false)
  })
})

const field = (text = "", evidence: string[] = []) => ({ text, contradictions: "", missing: "", evidence })
const kase = (id: string): Case => ({
  case_id: id,
  interview_id: id.replace(/-C\d+$/, ""),
  label: id,
  units: ["X-U001"],
  interviews: ["X"],
  decision: null,
  granularity: null,
  carrier_chosen: null,
  carriers_rejected: [],
  account_type: null,
  swiss_link: null,
  trigger: field("law"),
  alternatives: field(),
  constraints: field("", ["X-U001"]),
  choice: field(),
  result: field(),
  stage: null,
  outcome_status: null,
  notes: null,
})

describe("cross-case", () => {
  const answer: CrossCaseAnswer = {
    questions: [{ id: "RQ1", text: "Why", ord: 1 }],
    cases: [kase("ECO-07-C1"), kase("RET-22-C1")],
    cells: [
      { rq: "RQ1", case_id: "ECO-07-C1", documented: true, codes: [{ code: "LEGAL", units: ["u1"] }, { code: "COST", units: ["u2", "u3"] }] },
      { rq: "RQ1", case_id: "RET-22-C1", documented: true, codes: [{ code: "LEGAL", units: ["u9"] }, { code: "SKILLS", units: [] }] },
    ],
  }
  it("finds a cell and sorts its codes by support", () => {
    expect(cellCodes(cellFor(answer.cells, "RQ1", "ECO-07-C1")).map((c) => [c.code, c.n])).toEqual([["COST", 2], ["LEGAL", 1]])
    expect(cellFor(answer.cells, "RQ2", "ECO-07-C1")).toBeNull()
    expect(cellCodes(null)).toEqual([])
  })
  it("lists codes shared by several cases", () => {
    expect(sharedCodes(answer, "RQ1")).toEqual([{ code: "LEGAL", cases: ["ECO-07-C1", "RET-22-C1"] }])
  })
  it("treats a field as filled with text or evidence", () => {
    const c = kase("A-1-C1")
    expect(fieldFilled(c, "trigger")).toBe(true)
    expect(fieldFilled(c, "constraints")).toBe(true)
    expect(fieldFilled(c, "choice")).toBe(false)
  })
})
