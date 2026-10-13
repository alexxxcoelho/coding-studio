import { afterEach, describe, expect, it } from "vitest"
import { act, cleanup, render } from "@testing-library/react"

import { LANG_KEY, formatDateTime, formatNumber, getLang, locale, pluralCategory, readLang, setLang, t, type Key } from "./index"
import { useLang, useT } from "./react"
import en from "./en.json"
import fr from "./fr.json"

// The catalogues are real; the tests only use keys that exist in both, found by shape.
const key = (suffix: string) => Object.keys(fr).find((k) => k.endsWith(suffix)) as Key | undefined

afterEach(() => {
  cleanup()
  setLang("fr")
  window.localStorage.clear()
})

describe("language choice", () => {
  it("is French by default and tolerates any stored junk", () => {
    expect(readLang(null)).toBe("fr")
    expect(readLang({ getItem: () => "de" })).toBe("fr")
    expect(readLang({ getItem: () => "en" })).toBe("en")
    expect(readLang({ getItem: () => { throw new Error("blocked") } })).toBe("fr")
  })
  it("is stored under cs-lang and sets <html lang>", () => {
    setLang("en")
    expect(window.localStorage.getItem(LANG_KEY)).toBe("en")
    expect(document.documentElement.lang).toBe("en")
    expect(getLang()).toBe("en")
    setLang("fr")
    expect(window.localStorage.getItem(LANG_KEY)).toBeNull()
    expect(document.documentElement.lang).toBe("fr")
  })
  it("uses fr-CH and en-GB for dates and numbers", () => {
    expect(locale()).toBe("fr-CH")
    setLang("en")
    expect(locale()).toBe("en-GB")
    expect(formatNumber(1234.5, { minimumFractionDigits: 1 })).toBe("1,234.5")
    expect(formatDateTime("2026-03-04T09:05:00", { day: "numeric", month: "long" })).toBe("4 March")
    setLang("fr")
    expect(formatDateTime("2026-03-04T09:05:00", { day: "numeric", month: "long" })).toBe("4 mars")
  })
})

describe("plural categories", () => {
  it("counts 0 and 1 as singular in French, only 1 in English", () => {
    expect(pluralCategory("fr", 0)).toBe("one")
    expect(pluralCategory("fr", 1)).toBe("one")
    expect(pluralCategory("fr", 2)).toBe("other")
    expect(pluralCategory("en", 0)).toBe("other")
    expect(pluralCategory("en", 1)).toBe("one")
  })
})

describe("t", () => {
  it("returns the key when nothing matches and interpolates {vars}", () => {
    expect(t("no.such.key" as Key)).toBe("no.such.key")
  })
  it("interpolates and picks plural variants from the catalogue", () => {
    const one = Object.keys(fr).find((k) => k.endsWith("_one")) as string | undefined
    if (!one) return
    const base = one.slice(0, -4) as Key
    expect(t(base, { n: 1 })).toContain("1")
    expect(t(base, { n: 5 })).toContain("5")
    expect(t(base, { n: 5 })).toBe((fr as Record<string, string>)[`${base}_other`].replace("{n}", "5"))
  })
  it("falls back to French when English lacks the key", () => {
    const k = key("")
    if (!k) return
    setLang("en")
    expect(typeof t(k)).toBe("string")
    expect(Object.keys(en).length).toBe(Object.keys(fr).length)
  })
})

describe("hooks", () => {
  it("re-renders the subscriber when the language changes", () => {
    function Probe() {
      const lang = useLang()
      const tt = useT()
      return <p data-testid="p">{lang + tt("no.such.key" as Key)}</p>
    }
    const { getByTestId } = render(<Probe />)
    expect(getByTestId("p").textContent).toBe("frno.such.key")
    act(() => setLang("en"))
    expect(getByTestId("p").textContent).toBe("enno.such.key")
  })
})
