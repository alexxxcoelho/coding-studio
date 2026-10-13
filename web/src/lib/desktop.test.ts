import { describe, expect, it } from "vitest"
import { hasVibrancy, inTauri, isMacPlatform, markDesktop } from "./desktop"
import { MAX_OPEN, withOpened } from "./open-interviews"

describe("desktop detection", () => {
  it("detects the Tauri webview and macOS", () => {
    expect(inTauri({ __TAURI_INTERNALS__: {} })).toBe(true)
    expect(inTauri({})).toBe(false)
    expect(isMacPlatform({ platform: "MacIntel" })).toBe(true)
    expect(isMacPlatform({ platform: "Linux x86_64", userAgent: "X11" })).toBe(false)
  })
  it("applies vibrancy when the shell says so, or in Tauri on macOS only", () => {
    expect(hasVibrancy({ __CS_VIBRANCY__: true }, { platform: "Linux" })).toBe(true)
    expect(hasVibrancy({ __TAURI_INTERNALS__: {} }, { platform: "MacIntel" })).toBe(true)
    expect(hasVibrancy({ __TAURI_INTERNALS__: {} }, { platform: "Win32" })).toBe(false)
    expect(hasVibrancy({}, { platform: "MacIntel" })).toBe(false)
  })
  it("leaves the vibrancy flag off in a plain browser", () => {
    markDesktop()
    expect(document.documentElement.dataset.vibrancy).toBeUndefined()
  })
})

describe("open interviews", () => {
  it("adds first, keeps an open one in place and caps the list", () => {
    expect(withOpened(["B"], "A")).toEqual(["A", "B"])
    const same = ["A", "B"]
    expect(withOpened(same, "B")).toBe(same)
    const many = Array.from({ length: MAX_OPEN }, (_, i) => `I${i}`)
    expect(withOpened(many, "NEW")).toHaveLength(MAX_OPEN)
    expect(withOpened(many, "NEW")[0]).toBe("NEW")
  })
})
