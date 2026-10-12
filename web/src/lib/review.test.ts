import { describe, expect, it } from "vitest"

import { candidateView } from "./review"
import type { ReviewItem } from "./types"

const item = (over: Partial<ReviewItem>): ReviewItem => ({
  unit_id: "ECO-07-U010",
  kind: "disagreement",
  human: ["LEGAL"],
  human_current: ["LEGAL"],
  revised_since_snapshot: false,
  ai: { R1: ["COST"] },
  decision: null,
  unit: { unit_id: "ECO-07-U010", seq: 10, speaker_raw: "RESP", role: "respondent", timestamp: "00:10", text: "t" },
  ...over,
})

describe("candidate view", () => {
  it("shows the frozen coding alone when the candidate has not revised it", () => {
    expect(candidateView(item({}))).toEqual({ frozen: ["LEGAL"], current: ["LEGAL"], revised: false })
  })
  it("shows the current coding next to the frozen one once revised", () => {
    const v = candidateView(item({ human_current: ["LEGAL", "COST"], revised_since_snapshot: true }))
    expect(v).toEqual({ frozen: ["LEGAL"], current: ["LEGAL", "COST"], revised: true })
  })
  it("keeps the frozen codes as the reference for the decision", () => {
    expect(candidateView(item({ human_current: [], revised_since_snapshot: true })).frozen).toEqual(["LEGAL"])
  })
})
