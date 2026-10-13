import { describe, expect, it } from "vitest"

import { ApiError } from "./api"
import {
  cellOf,
  clock,
  excerptRanges,
  explainCodeOpError,
  heatAlpha,
  matchedLine,
  maxUnits,
  parseOffset,
  piecesAround,
  scalarToUtf16,
  toggleTarget,
  unitAt,
} from "./v2-text"
import type { AudioUnit, CoGroup } from "./v2-api"

describe("matched line of the code search", () => {
  it("names the field that matched, except for the name", () => {
    expect(matchedLine({ field: "name", text: "Coût" })).toBeNull()
    expect(matchedLine({ field: "alias", text: "compétences" })).toBe("Alias : compétences")
    expect(matchedLine({ field: "definition", text: "x".repeat(100) }, 20)).toBe(`Définition : ${"x".repeat(19)}…`)
    expect(matchedLine(undefined)).toBeNull()
  })
})

describe("refusals in French", () => {
  it("explains the 409 of an undo", () => {
    const e = (m: string, s = 409) => new ApiError(s, s === 409 ? "guard" : "not_found", m)
    expect(explainCodeOpError(e("operation 3 was already undone"))).toMatch(/déjà été annulée/)
    expect(explainCodeOpError(e("operation 5 came after and touches the same codes: undo it first"))).toMatch(/n° 5.*Annulez-la d'abord/)
    expect(explainCodeOpError(e("not found: code operation 9", 404))).toMatch(/avant l'existence du journal/)
    expect(explainCodeOpError(e("an undo needs a rationale", 400))).toBe("Le motif est obligatoire.")
  })
})

describe("pieces of a text around its matches", () => {
  it("cuts around ranges and skips bad ones", () => {
    const p = piecesAround("abcdefgh", [{ start: 2, end: 4 }, { start: 3, end: 5 }, { start: 6, end: 20 }, { start: 5, end: 7 }])
    expect(p.map((x) => x.text)).toEqual(["ab", "cd", "e", "fg", "h"])
    expect(p.map((x) => x.hit)).toEqual([-1, 0, -1, 3, -1])
  })
  it("converts scalar positions to UTF-16 around an emoji", () => {
    expect(scalarToUtf16("😀 ab", 2)).toBe(3)
    expect(excerptRanges("😀 ab", [{ start: 2, end: 4 }])).toEqual([{ start: 3, end: 5 }])
  })
})

describe("audio clock", () => {
  it("prints minutes and hours", () => {
    expect(clock(75.4)).toBe("1:15")
    expect(clock(3725)).toBe("1:02:05")
    expect(clock(null)).toBe("--:--")
  })
  it("reads an offset in seconds or as a clock", () => {
    expect(parseOffset("2,5")).toBe(2.5)
    expect(parseOffset("-3")).toBe(-3)
    expect(parseOffset("1:30")).toBe(90)
    expect(parseOffset("-0:10")).toBe(-10)
    expect(parseOffset("abc")).toBeNull()
    expect(parseOffset("999999")).toBeNull()
  })
  const u = (id: string, a: number | null, e: number | null): AudioUnit => ({ unit_id: id, seq: 0, role: "respondent", timestamp: "", start_seconds: a, audio_seconds: a, end_seconds: e })
  it("finds the unit being heard", () => {
    const units = [u("U1", 1, 6), u("U2", null, null), u("U3", 6, 25), u("U4", 25, null)]
    expect(unitAt(units, 0.5)).toBeNull()
    expect(unitAt(units, 1)).toBe("U1")
    expect(unitAt(units, 5.99)).toBe("U1")
    expect(unitAt(units, 6)).toBe("U3")
    expect(unitAt(units, 400)).toBe("U4")
  })
})

describe("co-occurrence colours", () => {
  it("steps the accent from 8 to 50 percent", () => {
    expect(heatAlpha(0, 10)).toBe(0)
    expect(heatAlpha(1, 10)).toBe(0.08)
    expect(heatAlpha(10, 10)).toBe(0.5)
    expect(heatAlpha(5, 10)).toBe(0.28)
  })
  it("finds a cell in either order and the largest count", () => {
    const g: CoGroup = { key: "all", label: "all", interviews: 2, diagonal: [], cells: [{ a: "A", b: "B", units: 3, interviews: 1, passage_pairs: 3, jaccard: 0.5 }] }
    expect(cellOf(g, "B", "A")?.units).toBe(3)
    expect(cellOf(g, "A", "C")).toBeNull()
    expect(maxUnits([g])).toBe(3)
  })
})

describe("keyboard decisions of the worklist", () => {
  it("toggles targets and falls back to undecided", () => {
    let d = toggleTarget(undefined, "C-1")
    expect(d).toEqual({ action: "assign", targets: ["C-1"] })
    d = toggleTarget(d, "C-2")
    expect(d?.targets).toEqual(["C-1", "C-2"])
    d = toggleTarget(d, "C-1")
    d = toggleTarget(d, "C-2")
    expect(d).toBeUndefined()
    expect(toggleTarget({ action: "leave", targets: [] }, "C-3")).toEqual({ action: "assign", targets: ["C-3"] })
  })
})
