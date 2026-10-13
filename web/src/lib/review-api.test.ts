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
  const view = { enabled: true, digest: "x", categories: ["person"], n_list_entries: 1, entries: [{ kind: "person", original: "Anna", pseudonym: "P1", count: 2 }], free_text_covered: false, note: "n" }
  it("reads the names found in an interview", async () => {
    mock.on("GET", "/api/interviews/X/pseudonymisation", { body: view })
    expect((await reviewApi.pseudonymisation("X")).entries[0].count).toBe(2)
  })
  it("reads and replaces the list of the project", async () => {
    mock.on("GET", "/api/pseudonymisation", { body: view })
    mock.on("PUT", "/api/pseudonymisation", { body: view })
    expect((await reviewApi.projectPseudonymisation()).enabled).toBe(true)
    await reviewApi.savePseudonymisation({ enabled: false, entries: [{ kind: "organisation", original: "Lectis", pseudonym: "Fournisseur A" }] })
    expect(mock.to("PUT", "/api/pseudonymisation")[0].body).toEqual({ enabled: false, entries: [{ kind: "organisation", original: "Lectis", pseudonym: "Fournisseur A" }] })
  })
})

describe("harmonisation and review state", () => {
  it("reads the worklist and resolves an item", async () => {
    mock.on("GET", "/api/interviews/X/harmonisation", { body: { n_open: 1, n_done: 0, items: [{ item_id: 4, status: "open" }] } })
    mock.on("PUT", "/api/interviews/X/harmonisation/4", { body: { item_id: 4, status: "done", resolution: "confirmed" } })
    expect((await reviewApi.harmonisation("X")).n_open).toBe(1)
    await reviewApi.resolveHarmonisation("X", 4, { resolution: "confirmed", note: "vu" })
    expect(mock.to("PUT", "/api/interviews/X/harmonisation/4")[0].body).toEqual({ resolution: "confirmed", note: "vu" })
  })
  it("reads the state of an interview and of the corpus", async () => {
    mock.on("GET", "/api/interviews/X/review-state", { body: { interview_id: "X", state: "in_progress" } })
    mock.on("GET", "/api/review-state", { body: { interviews: [], n_interviews: 0 } })
    expect((await reviewApi.reviewState("X")).state).toBe("in_progress")
    expect((await reviewApi.corpusReviewState()).n_interviews).toBe(0)
  })
  it("lets a missing interview through", async () => {
    await expect(reviewApi.reviewState("Z")).rejects.toThrow("unmocked")
  })
})
