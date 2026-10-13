import { afterEach, beforeEach, describe, expect, it } from "vitest"
import {
  ACCENTS,
  ACCENT_KEY,
  THEME_KEY,
  TEXT_SIZE_KEY,
  SHORTCUTS_KEY,
  accentVars,
  applyAccent,
  applyStored,
  applyTheme,
  readAccent,
  readChoice,
  readShortcuts,
  readTextSize,
  resolveTheme,
  saveAccent,
  saveChoice,
  saveShortcuts,
  saveTextSize,
} from "./theme"

function stubMatchMedia(dark: boolean) {
  window.matchMedia = ((q: string) => ({ matches: dark && q.includes("dark"), addEventListener() {}, removeEventListener() {} })) as never
}

beforeEach(() => {
  localStorage.clear()
  document.documentElement.removeAttribute("style")
  document.documentElement.className = ""
  stubMatchMedia(false)
})
afterEach(() => localStorage.clear())

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
  it("applies the dark class from the stored choice or the system", () => {
    applyTheme("system")
    expect(document.documentElement.classList.contains("dark")).toBe(false)
    stubMatchMedia(true)
    applyTheme("system")
    expect(document.documentElement.classList.contains("dark")).toBe(true)
    applyTheme("light")
    expect(document.documentElement.classList.contains("dark")).toBe(false)
    expect(document.documentElement.style.colorScheme).toBe("light")
  })
  it("persists an explicit choice and forgets it on automatic", () => {
    saveChoice("dark")
    expect(localStorage.getItem(THEME_KEY)).toBe("dark")
    expect(document.documentElement.classList.contains("dark")).toBe(true)
    saveChoice("system")
    expect(localStorage.getItem(THEME_KEY)).toBeNull()
  })
  it("survives a storage that throws", () => {
    const real = Storage.prototype.setItem
    Storage.prototype.setItem = () => {
      throw new Error("quota")
    }
    try {
      expect(() => saveChoice("dark")).not.toThrow()
      expect(() => saveAccent("bleu")).not.toThrow()
      expect(document.documentElement.classList.contains("dark")).toBe(true)
    } finally {
      Storage.prototype.setItem = real
    }
  })
})

describe("accent", () => {
  it("has the eight accents of the design with light and dark values", () => {
    expect(Object.keys(ACCENTS)).toEqual(["graphite", "bleu", "violet", "rose", "rouge", "orange", "jaune", "vert"])
    expect(accentVars("bleu")).toEqual({ "--acL": "#007AFF", "--acLS": "#0062CC", "--acD": "#0A84FF", "--acDS": "#0070E0" })
    expect(accentVars("graphite")?.["--acLS"]).toBe("#636366")
  })
  it("leaves the stylesheet default (AccentColor) for the system accent", () => {
    expect(accentVars("system")).toBeNull()
    saveAccent("rouge")
    expect(document.documentElement.style.getPropertyValue("--acL")).toBe("#FF3B30")
    saveAccent("system")
    expect(document.documentElement.style.getPropertyValue("--acL")).toBe("")
    expect(document.documentElement.dataset.accent).toBe("system")
  })
  it("reads only valid stored accents", () => {
    expect(readAccent({ getItem: () => "vert" })).toBe("vert")
    expect(readAccent({ getItem: () => "magenta" })).toBe("system")
    expect(readAccent({ getItem: () => "toString" })).toBe("system")
    expect(readAccent({ getItem: () => { throw new Error("blocked") } })).toBe("system")
  })
  it("persists the accent", () => {
    saveAccent("violet")
    expect(localStorage.getItem(ACCENT_KEY)).toBe("violet")
    applyAccent("system")
    saveAccent("system")
    expect(localStorage.getItem(ACCENT_KEY)).toBeNull()
  })
})

describe("reading preferences", () => {
  it("defaults to medium text and visible shortcuts", () => {
    expect(readTextSize(localStorage)).toBe("medium")
    expect(readShortcuts(localStorage)).toBe(true)
  })
  it("persists and applies them", () => {
    saveTextSize("large")
    expect(localStorage.getItem(TEXT_SIZE_KEY)).toBe("large")
    expect(document.documentElement.style.getPropertyValue("--transcript-size")).toBe("17px")
    saveShortcuts(false)
    expect(localStorage.getItem(SHORTCUTS_KEY)).toBe("0")
    expect(document.documentElement.dataset.shortcuts).toBe("off")
  })
  it("applies everything stored at start-up", () => {
    localStorage.setItem(THEME_KEY, "dark")
    localStorage.setItem(ACCENT_KEY, "orange")
    localStorage.setItem(TEXT_SIZE_KEY, "small")
    applyStored()
    expect(document.documentElement.classList.contains("dark")).toBe(true)
    expect(document.documentElement.style.getPropertyValue("--acD")).toBe("#FF9F0A")
    expect(document.documentElement.style.getPropertyValue("--transcript-size")).toBe("13.5px")
  })
})
