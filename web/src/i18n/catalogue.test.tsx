import { afterEach, describe, expect, it } from "vitest"
import { act, cleanup, fireEvent, screen } from "@testing-library/react"

import { SettingsDialog } from "@/components/settings-dialog"
import { guideSections, helpTarget, sectionSource } from "@/lib/guide"
import { resolveGuideHref } from "@/lib/markdown"
import { mockApi, renderWithClient } from "@/test-utils"
import { LANG_KEY, getLang, setLang, t } from "./index"
import en from "./en.json"
import fr from "./fr.json"

afterEach(() => {
  cleanup()
  setLang("fr")
  window.localStorage.clear()
})

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",")

describe("catalogues", () => {
  it("have the same keys and the same placeholders", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(fr).sort())
    for (const [k, v] of Object.entries(fr)) expect(placeholders((en as Record<string, string>)[k]), k).toBe(placeholders(v))
  })
  it("pick the plural variant of the language", () => {
    expect(t("common.n_codes", { n: 0 })).toBe("0 code")
    expect(t("common.n_codes", { n: 2 })).toBe("2 codes")
    setLang("en")
    expect(t("common.n_codes", { n: 0 })).toBe("0 codes")
    expect(t("common.n_codes", { n: 1 })).toBe("1 code")
    expect(t("common.percent", { value: 12 })).toBe("12%")
  })
})

describe("guide in English", () => {
  it("serves the English sections, screens and markdown", () => {
    setLang("en")
    expect(guideSections().map((s) => s.id)).toEqual(guideSections("fr").map((s) => s.id))
    expect(helpTarget("coder").file).toMatch(/02_code\.md$/)
    for (const s of guideSections()) expect(sectionSource(s.id)).toContain(`{#${s.anchor}}`)
    const english = sectionSource("coder")
    setLang("fr")
    expect(english).not.toEqual(sectionSource("coder"))
  })
  it("resolves a link between guide files in the current language", () => {
    setLang("en")
    expect(resolveGuideHref("00_contents.md")).toMatch(/^#\/guide\//)
    expect(resolveGuideHref("en/00_contents.md")).toBe(resolveGuideHref("00_contents.md"))
  })
})

describe("language select", () => {
  it("switches the interface live and remembers the choice", async () => {
    mockApi().on("GET", /^\/api\//, { body: {} })
    renderWithClient(<SettingsDialog open onOpenChange={() => undefined} tab="general" onTabChange={() => undefined} />)
    const select = await screen.findByLabelText("Langue de l'interface")
    expect(getLang()).toBe("fr")
    await act(async () => {
      fireEvent.change(select, { target: { value: "en" } })
    })
    expect(getLang()).toBe("en")
    expect(window.localStorage.getItem(LANG_KEY)).toBe("en")
    expect(await screen.findByLabelText("Interface language")).toHaveValue("en")
  })
})
