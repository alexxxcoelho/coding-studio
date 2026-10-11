import { describe, expect, it } from "vitest"
import { readChoice, resolveTheme } from "./theme"

describe("theme", () => {
  it("follows the system unless a choice is stored", () => {
    expect(resolveTheme("system", true)).toBe("dark")
    expect(resolveTheme("system", false)).toBe("light")
    expect(resolveTheme("light", true)).toBe("light")
    expect(resolveTheme("dark", false)).toBe("dark")
  })
  it("reads only valid stored values", () => {
    expect(readChoice({ getItem: () => "dark" })).toBe("dark")
    expect(readChoice({ getItem: () => "purple" })).toBe("system")
    expect(readChoice({ getItem: () => { throw new Error("blocked") } })).toBe("system")
    expect(readChoice(null)).toBe("system")
  })
})
