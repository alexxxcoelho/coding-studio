import { afterEach, describe, expect, it, vi } from "vitest"
import { ApiError, apiBase, apiUrl, request, toApiError } from "./api"

describe("toApiError", () => {
  it("keeps the server kind, message and issues", () => {
    const e = toApiError(400, { error: "rejected", kind: "validation", issues: [{ location: "$", message: "bad" }] })
    expect(e.kind).toBe("validation")
    expect(e.message).toBe("rejected")
    expect(e.issues).toHaveLength(1)
  })
  it("maps a guard refusal", () => {
    const e = toApiError(409, { error: "protocol guard: blind coding guard", kind: "guard" })
    expect(e.isGuard).toBe(true)
  })
  it("falls back on the status when the body is not JSON", () => {
    expect(toApiError(404, "nope").kind).toBe("not_found")
    expect(toApiError(500, "").message).toContain("500")
    expect(toApiError(409, { error: "x", kind: "weird" }).kind).toBe("guard")
  })
})

describe("request", () => {
  afterEach(() => vi.unstubAllGlobals())
  it("returns parsed JSON on success", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ a: 1 }), { status: 200 })))
    await expect(request("GET", "/api/x")).resolves.toEqual({ a: 1 })
  })
  it("throws ApiError on failure", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ error: "locked", kind: "guard" }), { status: 409 })))
    await expect(request("PUT", "/api/x", {})).rejects.toMatchObject({ kind: "guard", message: "locked" })
  })
  it("reports a network failure in plain words", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("Failed to fetch") }))
    const err = (await request("GET", "/api/x").catch((e: unknown) => e)) as ApiError
    expect(err).toBeInstanceOf(ApiError)
    expect(err.kind).toBe("network")
  })
})

describe("provider run refusals", () => {
  it("keeps the rejected raw answers of a 400", () => {
    const e = toApiError(400, {
      error: "the AI answer was rejected: 1 problem(s) found",
      kind: "validation",
      issues: [{ location: "$", message: "no JSON object found" }],
      rejected: [{ part: 2, raw: "sorry" }],
    })
    expect(e.rejected).toEqual([{ part: 2, raw: "sorry" }])
    expect(toApiError(400, { error: "x", kind: "validation" }).rejected).toEqual([])
  })
})

describe("API base", () => {
  it("is empty (same origin) unless the desktop shell announces one", () => {
    expect(apiBase({})).toBe("")
    expect(apiBase({ __CS_API_BASE__: 7 })).toBe("")
    expect(apiUrl("/api/state")).toBe("/api/state")
  })
  it("uses the loopback origin of the desktop shell, without a trailing slash", () => {
    expect(apiBase({ __CS_API_BASE__: "http://127.0.0.1:50123/" })).toBe("http://127.0.0.1:50123")
    expect(apiUrl("/api/state", "http://127.0.0.1:50123")).toBe("http://127.0.0.1:50123/api/state")
  })
  it("prefixes every request", async () => {
    const fetchMock = vi.fn(async (_url: string) => new Response("{}", { status: 200 }))
    vi.stubGlobal("fetch", fetchMock)
    ;(window as { __CS_API_BASE__?: string }).__CS_API_BASE__ = "http://127.0.0.1:9"
    try {
      await request("GET", "/api/state")
      expect(fetchMock.mock.calls[0]![0]).toBe("http://127.0.0.1:9/api/state")
    } finally {
      delete (window as { __CS_API_BASE__?: string }).__CS_API_BASE__
      vi.unstubAllGlobals()
    }
  })
})
