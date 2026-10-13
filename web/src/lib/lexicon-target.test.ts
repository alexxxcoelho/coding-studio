import { describe, expect, it } from "vitest"

import { codeTargetFromTerms } from "./lexicon-target"
import type { Passage } from "./types"

function hl(id: string, interview: string, unit: string, text: string): Passage {
  return { passage_id: id, interview_id: interview, unit_id: unit, text, kind: "highlight", stale: false, start: 0, end: text.length } as Passage
}

const BY_INTERVIEW: Record<string, Passage[]> = {
  "ECO-16": [hl("P1", "ECO-16", "ECO-16-U032", "tout le lot"), hl("P2", "ECO-16", "ECO-16-U040", "un rappel")],
  "ECO-32": [hl("P3", "ECO-32", "ECO-32-U061", "le lot reste"), { ...hl("P9", "ECO-32", "ECO-32-U070", "ancien"), stale: true }],
}
const load = async (id: string) => BY_INTERVIEW[id] ?? []

describe("codeTargetFromTerms", () => {
  it("opens the dialog on the first highlight, with the name and the other highlights", async () => {
    const r = await codeTargetFromTerms({ name: "lot, rappel", passageIds: ["P3", "P1", "P2"], interviews: ["ECO-16", "ECO-32"] }, load)
    expect(r?.interviewId).toBe("ECO-32")
    expect(r?.target).toMatchObject({ unitId: "ECO-32-U061", passageId: "P3", name: "lot, rappel", text: "le lot reste", range: null })
    expect(r?.target.alsoPassageIds).toEqual(["P1", "P2"])
  })
  it("drops the highlights that are gone or stale", async () => {
    const r = await codeTargetFromTerms({ name: "x", passageIds: ["P9", "P2", "P404"], interviews: ["ECO-16", "ECO-32"] }, load)
    expect(r?.target.passageId).toBe("P2")
    expect(r?.target.alsoPassageIds).toEqual([])
  })
  it("is null when no highlight is left", async () => {
    expect(await codeTargetFromTerms({ name: "x", passageIds: ["P404"], interviews: ["ECO-16"] }, load)).toBeNull()
  })
})
