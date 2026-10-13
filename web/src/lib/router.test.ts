import { describe, expect, it } from "vitest"
import { interviewHref, parseHash, unitHref } from "./router"

describe("router", () => {
  it("parses interview routes with phase", () => {
    expect(parseHash("#/interviews/ECO-07/coding?phase=2")).toEqual({ page: "interview", id: "ECO-07", tab: "coding", phase: 2, unit: null })
    expect(parseHash("#/interviews/ECO-07")).toEqual({ page: "interview", id: "ECO-07", tab: "sources", phase: 1, unit: null })
    expect(parseHash("#/codebook")).toEqual({ page: "codebook" })
    expect(parseHash("")).toEqual({ page: "library" })
  })
  it("round-trips hrefs", () => {
    expect(parseHash(interviewHref("RET-22", "review"))).toMatchObject({ id: "RET-22", tab: "review", phase: 2 })
  })
})

describe("router v3", () => {
  it("forces phase 2 on the AI and review tabs and maps old reconciliation links", () => {
    expect(parseHash("#/interviews/ECO-07/ai")).toMatchObject({ tab: "ai", phase: 2 })
    expect(parseHash("#/interviews/ECO-07/reconciliation?phase=1")).toMatchObject({ tab: "review", phase: 2 })
  })
  it("carries a unit to select and new pages", () => {
    expect(parseHash(unitHref("ECO-07-U003"))).toMatchObject({ id: "ECO-07", tab: "coding", phase: 2, unit: "ECO-07-U003" })
    expect(parseHash("#/to-clarify")).toEqual({ page: "to-clarify" })
    expect(parseHash("#/cross-case")).toEqual({ page: "cross-case" })
  })
})

describe("router v4", () => {
  it("parses the verification tab and the answers and saturation pages", () => {
    expect(parseHash(interviewHref("ECO-07", "verification"))).toMatchObject({ tab: "verification", phase: 1 })
    expect(parseHash("#/answers")).toEqual({ page: "answers" })
    expect(parseHash("#/saturation")).toEqual({ page: "saturation" })
  })
})

describe("router v5", () => {
  it("parses the settings page", () => {
    expect(parseHash("#/settings")).toEqual({ page: "settings" })
  })
})
