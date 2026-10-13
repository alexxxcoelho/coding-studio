import { afterEach, describe, expect, it, vi } from "vitest"

import { analysisApi, fromNativeConfig, toNativeConfig, type MatrixConfig } from "./analysis-api"

afterEach(() => vi.unstubAllGlobals())

function stub(answer: unknown) {
  const fetchMock = vi.fn(async () => new Response(JSON.stringify(answer), { status: 200, headers: { "content-type": "application/json" } }))
  vi.stubGlobal("fetch", fetchMock)
  return fetchMock
}

const body = (m: ReturnType<typeof stub>) => JSON.parse((m.mock.calls[0] as unknown as [string, RequestInit])[1].body as string)
const url = (m: ReturnType<typeof stub>) => (m.mock.calls[0] as unknown as [string])[0]

describe("native matrix configuration", () => {
  it("sends one axis per side and the filter as a list of values", () => {
    const c: MatrixConfig = { rows: { kind: "interviews" }, cols: { kind: "attribute", id: "sector" }, filter: { attribute: "sector", value: "Santé" } }
    expect(toNativeConfig(c)).toEqual({
      rows: [{ kind: "interviews", ids: [] }],
      cols: [{ kind: "attribute", attribute: "sector" }],
      filters: [{ attribute: "sector", values: ["Santé"] }],
    })
  })

  it("keeps everything while the value of the filter is not chosen", () => {
    expect(toNativeConfig({ rows: { kind: "themes" }, cols: { kind: "codes" }, filter: { attribute: "sector", value: "" } }).filters).toEqual([])
  })

  it("reads a stored native view back as the first axis of each side", () => {
    const native = {
      rows: [{ kind: "focused_codes", ids: [] }, { kind: "themes", ids: [] }],
      cols: [{ kind: "dimensions", ids: [] }],
      filters: [{ attribute: "sector", values: ["Santé", "Industrie"] }],
    }
    expect(fromNativeConfig(native)).toEqual({ rows: { kind: "focused_codes" }, cols: { kind: "dimensions" }, filter: { attribute: "sector", value: "Santé" } })
  })
})

describe("matrix routes", () => {
  it("posts the native configuration to /api/matrix/compute and flattens the cells", async () => {
    const m = stub({
      rows: [{ key: "interview:INT-01", label: "INT-01", coded: true }],
      cols: [{ key: "theme:T-0001", label: "Thème", coded: true }],
      cells: [[{ row_key: "interview:INT-01", col_key: "theme:T-0001", value: "present", summary: "s", passages: 2, interviews: 1, manual: false }]],
      interviews_in_scope: ["INT-01"],
    })
    const r = await analysisApi.matrix({ rows: { kind: "interviews" }, cols: { kind: "themes" }, filter: null })
    expect(url(m)).toBe("/api/matrix/compute")
    expect(body(m)).toEqual({ rows: [{ kind: "interviews", ids: [] }], cols: [{ kind: "themes", ids: [] }], filters: [] })
    expect(r.cells).toEqual([{ row_key: "interview:INT-01", col_key: "theme:T-0001", value: "present", summary: "s", n_passages: 2, n_interviews: 1, manual: false }])
  })

  it("asks for a cell with the filter of the matrix", async () => {
    const m = stub({})
    await analysisApi.matrixCell("interview:INT-01", "theme:T-0001", { attribute: "sector", value: "Santé" })
    expect(url(m)).toBe("/api/matrix/cell?row=interview%3AINT-01&col=theme%3AT-0001&attribute=sector&value=Sant%C3%A9")
  })

  it("lists the saved views in the one-axis form, from native_config", async () => {
    stub([{ view_id: "V-1", name: "Par secteur", config: { rows: { kind: "interviews" } }, native_config: { rows: [{ kind: "interviews", ids: [] }], cols: [{ kind: "attribute", attribute: "sector" }], filters: [] } }])
    const views = await analysisApi.matrixViews()
    expect(views).toEqual([{ view_id: "V-1", name: "Par secteur", config: { rows: { kind: "interviews" }, cols: { kind: "attribute", id: "sector" }, filter: null } }])
  })
})

describe("gap routes", () => {
  it("creates a code with POST /api/codes", async () => {
    const m = stub({ code_id: "C-0009" })
    await analysisApi.createCode({ name: "Obligation légale", definition: "d", parent_id: null })
    expect(url(m)).toBe("/api/codes")
    expect(body(m)).toEqual({ name: "Obligation légale", definition: "d", parent_id: null })
  })

  it("undoes a creation, reads and writes the memo, qualifies a passage", async () => {
    let m = stub({ code_deleted: true, passage_deleted: true })
    await analysisApi.undoCreateCode("C-0009")
    expect(url(m)).toBe("/api/codes/C-0009/undo-create")
    m = stub({ code_id: "C-0009", memo: "x", history: [] })
    await analysisApi.setCodeMemo("C-0009", "x")
    expect(url(m)).toBe("/api/codes/C-0009/memo")
    expect(body(m)).toEqual({ text: "x" })
    m = stub({})
    await analysisApi.qualifyPassage("P-00001", { origin: "document", role: null })
    expect(url(m)).toBe("/api/passages/P-00001/qualification")
    expect(body(m)).toEqual({ origin: "document", role: null })
  })

  it("creates a decision with an outcome in the words of the guide", async () => {
    const m = stub({})
    await analysisApi.createDecision("INT-01", "Étiquetage", "defer")
    expect(url(m)).toBe("/api/interviews/INT-01/decisions")
    expect(body(m)).toEqual({ label: "Étiquetage", outcome: "defer" })
  })
})
