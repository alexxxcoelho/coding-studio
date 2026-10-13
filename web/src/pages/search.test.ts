import { describe, expect, it } from "vitest"

import { paramsOf } from "./search"

const base = { q: "  coût ", mode: "phrase" as const, prefix: false, interviewer: false, interviews: [], attribute: "", value: "", coded: "" as const, code: null, descendants: true }

describe("paramsOf", () => {
  it("trims the query and sets the page offset", () => {
    const p = paramsOf(base, 2)
    expect(p.q).toBe("coût")
    expect(p.offset).toBe(40)
    expect(p.attribute).toBeUndefined()
    expect(p.descendants).toBeUndefined()
  })
  it("keeps an attribute only with a value", () => {
    expect(paramsOf({ ...base, attribute: "role" }, 0).attribute).toBeUndefined()
    expect(paramsOf({ ...base, attribute: "role", value: "Lecteur" }, 0)).toMatchObject({ attribute: "role", value: "Lecteur" })
  })
})
