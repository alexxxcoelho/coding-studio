import { describe, expect, it } from "vitest"
import { disagreementFilename, disagreementMarkdown } from "./review-markdown"
import type { ReviewAnswer, ReviewItem } from "./types"

const item = (seq: number, extra: Partial<ReviewItem> = {}): ReviewItem => ({
  unit_id: `INT-U${seq}`, kind: "disagreement", human: ["FROZEN"], human_current: ["NOW"],
  revised_since_snapshot: true, ai: {R1: ["AI_ONE"], R2: []}, decision: null,
  unit: {unit_id: `INT-U${seq}`, seq, speaker_raw: "Participant", role: "respondent", timestamp: "00:01", text: `Excerpt ${seq}\n\nSecond line`}, ...extra,
})
const review = (items: ReviewItem[]): ReviewAnswer => ({runs: ["R1", "R2", "R3"], items, n_disagreements: 0, n_sampled: 0, n_decided: 0, snapshots: [], seeds: []})

describe("disagreement Markdown", () => {
  it("preserves frozen researcher choices, separates current choices and model runs, and excludes agreements", () => {
    const result = disagreementMarkdown("INT", review([item(9), item(2), item(3, {kind: "sampled_agreement" as ReviewItem["kind"], unit: {...item(3).unit, text: "EXCLUDED"}})]))
    expect(result.indexOf("INT-U2")).toBeLessThan(result.indexOf("INT-U9"))
    expect(result).toContain("FROZEN")
    expect(result).toContain("NOW")
    expect(result).toContain("AI\\_ONE")
    expect(result).toContain("— R2\n\nAucun code")
    expect(result).toContain("— R3\n\nRéponse absente")
    expect(result).toContain("> Excerpt 2\n> \n> Second line")
    expect(result).not.toContain("EXCLUDED")
  })
  it("handles an empty review and sanitises filenames", () => {
    expect(disagreementMarkdown("INT", review([]))).toContain("Aucun désaccord.")
    expect(disagreementFilename("../../INT/01")).toBe("______INT_01-disagreements.md")
  })
})
