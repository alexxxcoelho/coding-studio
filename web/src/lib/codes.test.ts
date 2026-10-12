import { describe, expect, it } from "vitest"
import { codeForKey, diffCodes, familyOf, groupByFamily, hotkeyFor, matchesQuery, NA, sameSet, toggleCode } from "./codes"
import type { Code } from "./types"

const code = (c: string, family: string, label = c): Code => ({
  code: c, family, label, definition: `${label} definition`, origin: "codebook", active: true, in_prompt: true,
})

describe("toggleCode", () => {
  it("adds and removes, sorted", () => {
    expect(toggleCode(["TECH"], "ENV")).toEqual(["ENV", "TECH"])
    expect(toggleCode(["ENV", "TECH"], "TECH")).toEqual(["ENV"])
  })
  it("keeps NA exclusive both ways", () => {
    expect(toggleCode(["ENV", "TECH"], NA)).toEqual([NA])
    expect(toggleCode([NA], "ORG")).toEqual(["ORG"])
    expect(toggleCode([NA], NA)).toEqual([])
  })
})

describe("filtering and grouping", () => {
  const book = [code("ORG-SKILLS", "ORG", "Skills"), code("TECH-ADV", "TECH", "Relative advantage"), code("OUT-REALISED", "OUTCOME")]
  it("matches code, label and family name, case-insensitive", () => {
    expect(matchesQuery(book[1], "relative")).toBe(true)
    expect(matchesQuery(book[0], "organisation")).toBe(true)
    expect(matchesQuery(book[0], "tech")).toBe(false)
    expect(matchesQuery(book[2], "  ")).toBe(true)
  })
  it("groups TOE families first", () => {
    expect(groupByFamily(book).map((g) => g.family)).toEqual(["TECH", "ORG", "OUTCOME"])
  })
  it("infers a family from the code string", () => {
    expect(familyOf("ENV-PARTNER")).toBe("ENV")
    expect(familyOf("INDUCTIVE-PENDING")).toBe("INDUCTIVE")
    expect(familyOf(NA)).toBe("NA")
  })
})

describe("diffCodes", () => {
  it("marks codes on one side only", () => {
    expect(diffCodes(["TECH", "ORG"], ["TECH"])).toEqual([
      { code: "ORG", mark: "only" },
      { code: "TECH", mark: "shared" },
    ])
    expect(sameSet(["A", "B"], ["B", "A"])).toBe(true)
    expect(sameSet(["A"], ["A", "B"])).toBe(false)
  })
})

describe("phase 1 hotkeys", () => {
  const p1 = ["TECH", "ORG", "ENV", "INDUCTIVE-PENDING"]
  it("maps T O E I N to the four categories and NA, case-insensitive", () => {
    expect(codeForKey("t", 1, p1)).toBe("TECH")
    expect(codeForKey("O", 1, p1)).toBe("ORG")
    expect(codeForKey("e", 1, p1)).toBe("ENV")
    expect(codeForKey("i", 1, p1)).toBe("INDUCTIVE-PENDING")
    expect(codeForKey("n", 1, p1)).toBe(NA)
  })
  it("ignores other keys, phase 2 and codes the unit does not allow", () => {
    expect(codeForKey("x", 1, p1)).toBeNull()
    expect(codeForKey("Enter", 1, p1)).toBeNull()
    expect(codeForKey("t", 2, ["TECH-ADV"])).toBeNull()
    expect(codeForKey("t", 1, ["ORG"])).toBeNull()
    expect(codeForKey("n", 1, [])).toBe(NA)
  })
  it("labels codes with their key in phase 1 only", () => {
    expect(hotkeyFor("ENV", 1)).toBe("E")
    expect(hotkeyFor(NA, 1)).toBe("N")
    expect(hotkeyFor("ENV", 2)).toBeNull()
  })
  it("pressing T then N leaves only NA, then O replaces NA", () => {
    let codes = toggleCode([], codeForKey("t", 1, p1)!)
    codes = toggleCode(codes, codeForKey("n", 1, p1)!)
    expect(codes).toEqual([NA])
    codes = toggleCode(codes, codeForKey("o", 1, p1)!)
    expect(codes).toEqual(["ORG"])
  })
})
