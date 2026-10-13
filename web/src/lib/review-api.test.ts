import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { mockApi, type MockApi } from "@/test-utils"
import { reviewApi } from "./review-api"

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
})
afterEach(() => vi.unstubAllGlobals())

describe("candidate concepts", () => {
  it("reads the list", async () => {
    mock.on("GET", "/api/interviews/X/concept-candidates", { body: [{ candidate_id: "c1", label: "Formation", unit_ids: ["X-U001"], decision: null }] })
    expect(await reviewApi.conceptCandidates("X")).toHaveLength(1)
  })
  it("reads a 404 as an absent feature", async () => {
    expect(await reviewApi.conceptCandidates("X")).toBeNull()
  })
  it("reads an answer that is not a list as an absent feature", async () => {
    mock.on("GET", "/api/interviews/X/concept-candidates", { body: "<html></html>" })
    expect(await reviewApi.conceptCandidates("X")).toBeNull()
  })
  it("lets a server failure through", async () => {
    mock.on("GET", "/api/interviews/X/concept-candidates", { status: 500, body: { error: "boom", kind: "internal" } })
    await expect(reviewApi.conceptCandidates("X")).rejects.toThrow("boom")
  })
  it("sends the decision with its reason", async () => {
    mock.on("PUT", "/api/interviews/X/concept-candidates/c1", { body: { candidate_id: "c1", label: "F", unit_ids: [], decision: null } })
    await reviewApi.decideConcept("X", "c1", { action: "map", code_id: "SKILLS", rationale: "même idée" })
    expect(mock.to("PUT", "/api/interviews/X/concept-candidates/c1")[0].body).toEqual({ action: "map", code_id: "SKILLS", rationale: "même idée" })
  })
})

describe("pseudonymisation", () => {
  it("reads the entries", async () => {
    mock.on("GET", "/api/interviews/X/pseudonymisation", { body: { entries: [{ kind: "person", original: "Anna", pseudonym: "P1" }] } })
    expect((await reviewApi.pseudonymisation("X"))?.entries).toHaveLength(1)
  })
  it("reads a 404 as an absent feature", async () => {
    expect(await reviewApi.pseudonymisation("X")).toBeNull()
  })
})
