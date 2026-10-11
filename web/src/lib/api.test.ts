import { afterEach, describe, expect, it, vi } from "vitest"
import { ApiError, request, toApiError } from "./api"

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
