// Interface language: two flat catalogues (fr.json, en.json) with keys
// namespaced by screen ("coder.toolbar.revoir"), a `t(key, vars?)` function and
// a persisted language choice (localStorage "cs-lang", French by default).
//
// `t` is a plain function that reads the current language, so it can be called
// anywhere (components, helpers, toasts). The root component subscribes with
// `useLang()`, which re-renders the whole tree when the language changes.
//
// Plurals: a message with the variants `key_one` and `key_other` (no bare `key`)
// is chosen from the number `vars.n`. French counts 0 and 1 as singular,
// English only 1.

import en from "./en.json"
import fr from "./fr.json"

export type Lang = "fr" | "en"
export type Key = keyof typeof fr
export type Vars = Record<string, string | number | null | undefined>

export const LANG_KEY = "cs-lang"
export const LANGS: { id: Lang; label: string; locale: string }[] = [
  { id: "fr", label: "Français", locale: "fr-CH" },
  { id: "en", label: "English", locale: "en-GB" },
]

const CATALOGUES: Record<Lang, Record<string, string>> = { fr, en }

export function readLang(storage: Pick<Storage, "getItem"> | null): Lang {
  try {
    const v = storage?.getItem(LANG_KEY)
    return v === "en" ? "en" : "fr"
  } catch {
    return "fr"
  }
}

function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

let current: Lang = readLang(storage())
const listeners = new Set<() => void>()

export function getLang(): Lang {
  return current
}

/** Locale for dates and numbers: fr-CH or en-GB. */
export function locale(): string {
  return LANGS.find((l) => l.id === current)!.locale
}

function applyDocumentLang(): void {
  if (typeof document !== "undefined") document.documentElement.lang = current
}

/** Switches the language, remembers it and notifies the subscribers. */
export function setLang(lang: Lang, persist = true): void {
  if (lang === current) return
  current = lang
  if (persist) {
    try {
      if (lang === "fr") window.localStorage.removeItem(LANG_KEY)
      else window.localStorage.setItem(LANG_KEY, lang)
    } catch {
      // Storage unavailable: the choice lasts for this session only.
    }
  }
  applyDocumentLang()
  for (const l of listeners) l()
}

export function subscribeLang(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Plural category of `n` in `lang`: "one" or "other". */
export function pluralCategory(lang: Lang, n: number): "one" | "other" {
  if (lang === "fr") return Math.abs(n) < 2 ? "one" : "other"
  return n === 1 ? "one" : "other"
}

function lookup(lang: Lang, key: string, n: number | null): string | undefined {
  const cat = CATALOGUES[lang]
  if (n !== null && cat[key] === undefined) {
    const variant = cat[`${key}_${pluralCategory(lang, n)}`]
    if (variant !== undefined) return variant
  }
  return cat[key]
}

/** The message for `key` in the current language, `{name}` replaced from `vars`. */
export function t(key: Key, vars?: Vars): string {
  const n = vars && typeof vars.n === "number" ? vars.n : null
  const raw = lookup(current, key, n) ?? lookup("fr", key, n) ?? key
  if (!vars) return raw
  return raw.replace(/\{(\w+)\}/g, (whole, name: string) => (Object.hasOwn(vars, name) ? String(vars[name] ?? "") : whole))
}

/** Dates and numbers in the current language (fr-CH or en-GB). */
export function formatDateTime(iso: string | Date, options: Intl.DateTimeFormatOptions = { dateStyle: "medium", timeStyle: "short" }): string {
  return new Intl.DateTimeFormat(locale(), options).format(typeof iso === "string" ? new Date(iso) : iso)
}

export function formatNumber(n: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(locale(), options).format(n)
}

applyDocumentLang()
