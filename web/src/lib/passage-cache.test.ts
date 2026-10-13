import { describe, expect, it } from "vitest"
import { isWholePassage, appliedCodes, unitCodes, findSpan, hasCode, withCoding, withoutCoding, withoutPassage, withServerPassage } from "./passage-cache"
import type { Passage } from "./types"

const TEXT = "Six mois pour les deux premiers magasins."
const code = { code_id: "C-0001", name: "Formation", system: false }
const other = { code_id: "C-0002", name: "Matériel", system: false }

describe("withCoding", () => {
  it("adds a whole-turn passage when no selection", () => {
    const out = withCoding([], { unitId: "U1" }, TEXT, "I", code)
    expect(out).toHaveLength(1)
    expect(out[0]).toMatchObject({ unit_id: "U1", whole_unit: true, start_utf16: 0, end_utf16: TEXT.length, kind: "coded", text: TEXT })
    expect(out[0].codings.map((c) => c.code_id)).toEqual(["C-0001"])
  })
  it("adds a partial passage with the selected text", () => {
    const out = withCoding([], { unitId: "U1", start: 0, end: 8 }, TEXT, "I", code)
    expect(out[0]).toMatchObject({ whole_unit: false, text: "Six mois" })
  })
  it("a selection of the whole text is the whole turn", () => {
    expect(withCoding([], { unitId: "U1", start: 0, end: TEXT.length }, TEXT, "I", code)[0].whole_unit).toBe(true)
  })
  it("puts a second code on the same span, not a second passage", () => {
    const a = withCoding([], { unitId: "U1", start: 0, end: 8 }, TEXT, "I", code)
    const b = withCoding(a, { unitId: "U1", start: 0, end: 8 }, TEXT, "I", other)
    expect(b).toHaveLength(1)
    expect(b[0].codings.map((c) => c.code_id)).toEqual(["C-0001", "C-0002"])
  })
  it("a code already there is a no-op", () => {
    const a = withCoding([], { unitId: "U1" }, TEXT, "I", code)
    expect(withCoding(a, { unitId: "U1" }, TEXT, "I", code)).toBe(a)
  })
  it("makes a highlight without code, and coding it later turns it into a coded passage", () => {
    const h = withCoding([], { unitId: "U1", start: 4, end: 8 }, TEXT, "I", null)
    expect(h[0]).toMatchObject({ kind: "highlight", codings: [] })
    const c = withCoding(h, { unitId: "U1", start: 4, end: 8 }, TEXT, "I", code)
    expect(c).toHaveLength(1)
    expect(c[0].kind).toBe("coded")
  })
  it("does not mutate its input", () => {
    const a: Passage[] = []
    withCoding(a, { unitId: "U1" }, TEXT, "I", code)
    expect(a).toEqual([])
  })
})

describe("removal and replacement", () => {
  const base = withCoding(withCoding([], { unitId: "U1" }, TEXT, "I", code), { unitId: "U1" }, TEXT, "I", other)
  it("removing the last code leaves a highlight", () => {
    const id = base[0].passage_id
    const one = withoutCoding(base, id, "C-0001")
    expect(one[0].kind).toBe("coded")
    const none = withoutCoding(one, id, "C-0002")
    expect(none[0]).toMatchObject({ kind: "highlight", codings: [] })
  })
  it("removes a passage", () => {
    expect(withoutPassage(base, base[0].passage_id)).toEqual([])
  })
  it("replaces a temporary passage by the server's", () => {
    const tmp = base[0]
    const real = { ...tmp, passage_id: "P-00007" }
    const out = withServerPassage(base, real, tmp.passage_id)
    expect(out.map((p) => p.passage_id)).toEqual(["P-00007"])
  })
})

describe("lookups", () => {
  const list = withCoding(withCoding([], { unitId: "U1" }, TEXT, "I", code), { unitId: "U1", start: 0, end: 8 }, TEXT, "I", other)
  it("finds a span", () => {
    expect(findSpan(list, { unitId: "U1" }, TEXT.length)?.whole_unit).toBe(true)
    expect(findSpan(list, { unitId: "U1", start: 0, end: 8 }, TEXT.length)?.whole_unit).toBe(false)
    expect(findSpan(list, { unitId: "U1", start: 1, end: 8 }, TEXT.length)).toBeUndefined()
    expect(findSpan(list, { unitId: "U2" }, TEXT.length)).toBeUndefined()
  })
  it("tells the codes applied to the span, or to the turn", () => {
    expect([...appliedCodes(list, { unitId: "U1" }, TEXT.length)]).toEqual(["C-0001"])
    expect([...appliedCodes(list, { unitId: "U1", start: 0, end: 8 }, TEXT.length)]).toEqual(["C-0002"])
    expect([...unitCodes(list, "U1")]).toEqual(["C-0001", "C-0002"])
    expect(hasCode(list[0], "C-0001")).toBe(true)
  })
})

describe("whole turn", () => {
  it("is read from the offsets, since the server flags any passage starting at 0", () => {
    const partial = { ...withCoding([], { unitId: "U1", start: 0, end: 8 }, TEXT, "I", code)[0], whole_unit: true }
    expect(isWholePassage(partial, TEXT.length)).toBe(false)
    expect(isWholePassage({ start_utf16: 0, end_utf16: TEXT.length }, TEXT.length)).toBe(true)
    expect(isWholePassage({ start_utf16: undefined, end_utf16: undefined }, TEXT.length)).toBe(false)
  })
  it("finds the whole-turn passage and the partial one at 0 apart", () => {
    const list = [
      { ...withCoding([], { unitId: "U1", start: 0, end: 8 }, TEXT, "I", code)[0], whole_unit: true },
      { ...withCoding([], { unitId: "U1" }, TEXT, "I", other)[0], passage_id: "P-2" },
    ]
    expect(findSpan(list, { unitId: "U1" }, TEXT.length)?.passage_id).toBe("P-2")
    expect(findSpan(list, { unitId: "U1", start: 0, end: 8 }, TEXT.length)?.codings[0].code_id).toBe("C-0001")
  })
})
