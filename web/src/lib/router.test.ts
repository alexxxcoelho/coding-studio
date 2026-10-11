import { describe, expect, it } from "vitest"
import { interviewHref, parseHash } from "./router"

describe("router", () => {
  it("parses interview routes with phase", () => {
    expect(parseHash("#/interviews/ECO-07/coding?phase=2")).toEqual({ page: "interview", id: "ECO-07", tab: "coding", phase: 2 })
    expect(parseHash("#/interviews/ECO-07")).toEqual({ page: "interview", id: "ECO-07", tab: "sources", phase: 1 })
    expect(parseHash("#/codebook")).toEqual({ page: "codebook" })
    expect(parseHash("")).toEqual({ page: "library" })
  })
  it("round-trips hrefs", () => {
    expect(parseHash(interviewHref("RET-22", "reconciliation", 2))).toMatchObject({ id: "RET-22", tab: "reconciliation", phase: 2 })
  })
})
