import { describe, expect, it } from "vitest"
import {
  applyFilter,
  checklistComplete,
  checklistMissing,
  countStates,
  groupByUnit,
  initialUnit,
  isEndOfInterview,
  neighbour,
  passageAt,
  pieces,
  stripeCodes,
  unitState,
  unitsFromRows,
  type CoderUnit,
} from "./coder-model"
import type { Passage, PassageCoding, UnitRow } from "./types"

const coding = (code_id: string, name = code_id, system = false): PassageCoding => ({
  coding_id: 1,
  code_id,
  code_name: name,
  system,
  note: "",
  created_at: "",
  created_by: "",
})

function passage(unit_id: string, start: number, end: number, codings: PassageCoding[] = [], extra: Partial<Passage> = {}): Passage {
  return {
    passage_id: `P-${unit_id}-${start}-${end}`,
    interview_id: "ECO-01",
    unit_id,
    start,
    end,
    start_utf16: start,
    end_utf16: end,
    text: "",
    whole_unit: false,
    segmentation_hash: "",
    anchor_digest: "",
    created_at: "",
    created_by: "",
    kind: codings.length ? "coded" : "highlight",
    stale: false,
    before: "",
    after: "",
    codings,
    ...extra,
  }
}

const unit = (id: string, role: CoderUnit["role"], extra: Partial<CoderUnit> = {}): CoderUnit => ({
  unit_id: id,
  seq: 0,
  role,
  speaker: role === "interviewer" ? "INT" : "RESP",
  timestamp: "",
  text: `Texte ${id}`,
  notRelevant: false,
  ...extra,
})

const units: CoderUnit[] = [
  unit("U1", "interviewer"),
  unit("U2", "respondent"),
  unit("U3", "interviewer"),
  unit("U4", "respondent"),
  unit("U5", "respondent", { notRelevant: true }),
  unit("U6", "respondent"),
]
const passages = [passage("U2", 0, 7, [coding("C-0001")]), passage("U4", 0, 7, [coding("LATER", "Plus tard", true)]), passage("U6", 0, 3)]
const byUnit = groupByUnit(passages)

describe("unitsFromRows", () => {
  it("reads NA as not relevant for respondent turns only", () => {
    const row = (id: string, role: "interviewer" | "respondent", codes: string[]): UnitRow =>
      ({ unit: { unit_id: id, seq: 1, speaker_raw: "X", role, timestamp: "", text: "t" }, coding: { unit_id: id, codes } }) as UnitRow
    const out = unitsFromRows([row("A", "interviewer", ["NA"]), row("B", "respondent", ["NA"]), row("C", "respondent", ["CONTEXT"])])
    expect(out.map((u) => u.notRelevant)).toEqual([false, true, false])
  })
})

describe("unitState", () => {
  it("tells the five states", () => {
    const state = (id: string) => unitState(units.find((u) => u.unit_id === id)!, byUnit.get(id) ?? [])
    expect(state("U1")).toBe("interviewer")
    expect(state("U2")).toBe("coded")
    expect(state("U4")).toBe("later")
    expect(state("U5")).toBe("non_pertinent")
    expect(state("U6")).toBe("remaining") // a highlight is not a code
  })
  it("a code wins over a parked passage on the same turn", () => {
    const both = [passage("U2", 0, 3, [coding("LATER", "Plus tard", true)]), passage("U2", 3, 7, [coding("C-0002")])]
    expect(unitState(units[1], both)).toBe("coded")
  })
})

describe("counts and filters", () => {
  it("counts respondent turns only", () => {
    expect(countStates(units, byUnit)).toEqual({ respondent: 4, coded: 1, later: 1, non_pertinent: 1, remaining: 1, examined: 3 })
  })
  it("filters by state; all keeps the interviewer turns", () => {
    expect(applyFilter(units, byUnit, "all")).toHaveLength(6)
    expect(applyFilter(units, byUnit, "remaining").map((u) => u.unit_id)).toEqual(["U6"])
    expect(applyFilter(units, byUnit, "later").map((u) => u.unit_id)).toEqual(["U4"])
    expect(applyFilter(units, byUnit, "non_pertinent").map((u) => u.unit_id)).toEqual(["U5"])
    expect(applyFilter(units, byUnit, "coded").map((u) => u.unit_id)).toEqual(["U2"])
  })
  it("searches the text and the id", () => {
    expect(applyFilter(units, byUnit, "all", "texte u4").map((u) => u.unit_id)).toEqual(["U4"])
    expect(applyFilter(units, byUnit, "all", "u5").map((u) => u.unit_id)).toEqual(["U5"])
  })
})

describe("navigation", () => {
  it("moves in the shown list and stops at the ends", () => {
    expect(neighbour(units, "U2", 1)).toBe("U3")
    expect(neighbour(units, "U2", -1)).toBe("U1")
    expect(neighbour(units, "U1", -1)).toBe("U1")
    expect(neighbour(units, "U6", 1)).toBe("U6")
  })
  it("falls back to the ends when the current unit is not shown", () => {
    expect(neighbour(units, "UX", 1)).toBe("U1")
    expect(neighbour(units, null, -1)).toBe("U6")
    expect(neighbour([], "U1", 1)).toBeNull()
  })
  it("opens on the asked unit, else the first remaining respondent turn", () => {
    expect(initialUnit(units, byUnit, "U4")).toBe("U4")
    expect(initialUnit(units, byUnit, "nope")).toBe("U6")
    expect(initialUnit(units, new Map(), null)).toBe("U2")
    expect(initialUnit([], new Map(), null)).toBeNull()
  })
})

describe("end of interview and checklist", () => {
  it("opens when no respondent turn remains: coded, not relevant or to review", () => {
    expect(isEndOfInterview(countStates(units, byUnit))).toBe(false)
    const done = groupByUnit([...passages, passage("U6", 0, 3, [coding("C-0003")])])
    expect(isEndOfInterview(countStates(units, done))).toBe(true)
  })
  it("never opens for an interview without respondent turn", () => {
    expect(isEndOfInterview(countStates([unit("U1", "interviewer")], new Map()))).toBe(false)
  })
  it("a checklist is complete when every item is checked, and never when empty", () => {
    expect(checklistComplete([{ checked: true }, { checked: true }])).toBe(true)
    expect(checklistComplete([{ checked: true }, { checked: false }])).toBe(false)
    expect(checklistComplete([])).toBe(false)
    expect(checklistMissing([{ label: "a", checked: true }, { label: "b", checked: false }])).toEqual(["b"])
  })
})

describe("pieces", () => {
  const text = "Six mois pour les deux premiers magasins, surtout à cause de la formation."
  it("returns the whole text when nothing covers it", () => {
    expect(pieces(text, [])).toEqual([{ start: 0, end: text.length, text, passages: [] }])
  })
  it("cuts at every boundary and keeps overlaps", () => {
    const a = passage("U", 0, 10, [coding("C-0001")])
    const b = passage("U", 5, 20, [coding("C-0002")])
    const out = pieces(text, [a, b])
    expect(out.map((p) => [p.start, p.end, p.passages.length])).toEqual([
      [0, 5, 1],
      [5, 10, 2],
      [10, 20, 1],
      [20, text.length, 0],
    ])
    expect(out.map((p) => p.text).join("")).toBe(text)
  })
  it("skips stale passages and passages without UTF-16 offsets", () => {
    const stale = passage("U", 0, 10, [], { stale: true })
    const bare = passage("U", 0, 10, [], { start_utf16: undefined, end_utf16: undefined })
    expect(pieces(text, [stale, bare])).toHaveLength(1)
  })
  it("clamps offsets to the text", () => {
    const out = pieces("abc", [passage("U", 1, 99)])
    expect(out.map((p) => p.text)).toEqual(["a", "bc"])
  })
})

describe("stripes and hit test", () => {
  it("lists the real codes once, in order of first use, without the system code", () => {
    const list = [passage("U", 0, 3, [coding("C-0002", "B")]), passage("U", 3, 6, [coding("C-0001", "A"), coding("C-0002", "B"), coding("LATER", "Plus tard", true)])]
    expect(stripeCodes(list)).toEqual([
      { code_id: "C-0002", name: "B" },
      { code_id: "C-0001", name: "A" },
    ])
  })
  it("picks the narrowest passage at an offset", () => {
    const wide = passage("U", 0, 20)
    const narrow = passage("U", 5, 8)
    expect(passageAt([wide, narrow], 6)).toBe(narrow)
    expect(passageAt([wide, narrow], 12)).toBe(wide)
    expect(passageAt([wide, narrow], 30)).toBeNull()
  })
})
