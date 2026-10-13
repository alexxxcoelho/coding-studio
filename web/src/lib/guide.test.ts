import { describe, expect, it } from "vitest"
import { guideSections, helpTarget, screenForRoute, sectionForAnchor, sectionSource } from "./guide"

describe("guide map", () => {
  it("resolves a screen to the section and anchor of guide.json", () => {
    expect(helpTarget("coder")).toMatchObject({ section: "coder", anchor: "coder-parcourir" })
    expect(helpTarget("revoir-ia")).toMatchObject({ section: "revoir", anchor: "revoir-prompt" })
    expect(helpTarget("bibliotheque")).toMatchObject({ section: "preparer", anchor: "preparer-bibliotheque" })
    expect(helpTarget("reglages")).toMatchObject({ section: "preparer", anchor: "preparer-reglages" })
  })
  it("falls back to the guide summary for an unknown screen", () => {
    expect(helpTarget("inconnu")).toMatchObject({ section: "sommaire", anchor: "sommaire" })
  })
  it("finds the section holding a heading anchor", () => {
    expect(sectionForAnchor("coder-detail")?.id).toBe("coder")
    expect(sectionForAnchor("exporter")?.id).toBe("exporter")
    expect(sectionForAnchor("nope")).toBeNull()
  })
  it("maps every route to a screen the guide knows", () => {
    expect(screenForRoute({ page: "library" })).toBe("bibliotheque")
    expect(screenForRoute({ page: "codebook" })).toBe("codebook")
    expect(screenForRoute({ page: "analyser" })).toBe("analyser")
    expect(screenForRoute({ page: "rediger" })).toBe("rediger")
    expect(screenForRoute({ page: "settings" })).toBe("reglages")
    expect(screenForRoute({ page: "guide", anchor: null })).toBe("guide")
    expect(screenForRoute({ page: "interview", id: "A", tab: "coding", phase: 1, unit: null })).toBe("coder")
    expect(screenForRoute({ page: "interview", id: "A", tab: "ai", phase: 2, unit: null })).toBe("revoir-ia")
    expect(screenForRoute({ page: "interview", id: "A", tab: "review", phase: 2, unit: null })).toBe("revoir-decisions")
    expect(screenForRoute({ page: "audit" })).toBe("exporter")
  })
  it("has the markdown source of each section", () => {
    for (const s of guideSections()) expect(sectionSource(s.id)).toContain(`{#${s.anchor}}`)
  })
})
