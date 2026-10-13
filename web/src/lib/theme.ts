import { syncNativeTheme } from "./desktop"
// Appearance: light / dark / automatic, and the accent colour. "system" follows
// prefers-color-scheme (appearance) or the CSS keyword AccentColor (accent).
// Choices are kept in localStorage; index.html applies them before first paint
// with the same keys and values.

export type ThemeChoice = "system" | "light" | "dark"
export const THEME_KEY = "cs-theme"

export function readChoice(storage: Pick<Storage, "getItem"> | null): ThemeChoice {
  try {
    const v = storage?.getItem(THEME_KEY)
    return v === "light" || v === "dark" ? v : "system"
  } catch {
    return "system"
  }
}

export function resolveTheme(choice: ThemeChoice, systemDark: boolean): "light" | "dark" {
  if (choice === "system") return systemDark ? "dark" : "light"
  return choice
}

function systemPrefersDark(): boolean {
  try {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
  } catch {
    return false
  }
}

export function applyTheme(choice: ThemeChoice): void {
  const dark = resolveTheme(choice, systemPrefersDark()) === "dark"
  const root = document.documentElement
  root.classList.toggle("dark", dark)
  root.style.colorScheme = dark ? "dark" : "light"
  void syncNativeTheme(choice === "system" ? null : choice === "dark" ? "dark" : "light")
}

export function saveChoice(choice: ThemeChoice): void {
  try {
    if (choice === "system") localStorage.removeItem(THEME_KEY)
    else localStorage.setItem(THEME_KEY, choice)
  } catch {
    // Storage unavailable: the choice lasts for this session only.
  }
  applyTheme(choice)
}

// ------------------------------------------------------------------ accent

export type AccentId = "system" | "graphite" | "bleu" | "violet" | "rose" | "rouge" | "orange" | "jaune" | "vert"
export const ACCENT_KEY = "cs-accent"

interface AccentValues {
  label: string
  /** Light mode: accent, fill (buttons, selection), in this order; then dark mode. */
  light: string
  lightFill: string
  dark: string
  darkFill: string
}

/** The eight accents of the design files, with their light and dark values. */
export const ACCENTS: Record<Exclude<AccentId, "system">, AccentValues> = {
  graphite: { label: "Graphite", light: "#8C8C8C", lightFill: "#636366", dark: "#98989D", darkFill: "#636366" },
  bleu: { label: "Bleu", light: "#007AFF", lightFill: "#0062CC", dark: "#0A84FF", darkFill: "#0070E0" },
  violet: { label: "Violet", light: "#AF52DE", lightFill: "#8E3BBF", dark: "#BF5AF2", darkFill: "#9A44C9" },
  rose: { label: "Rose", light: "#FF2D55", lightFill: "#D1123F", dark: "#FF375F", darkFill: "#D1123F" },
  rouge: { label: "Rouge", light: "#FF3B30", lightFill: "#D12A20", dark: "#FF453A", darkFill: "#D12A20" },
  orange: { label: "Orange", light: "#FF9500", lightFill: "#A85A00", dark: "#FF9F0A", darkFill: "#A85A00" },
  jaune: { label: "Jaune", light: "#FFCC00", lightFill: "#8A6D00", dark: "#FFD60A", darkFill: "#8A6D00" },
  vert: { label: "Vert", light: "#28CD41", lightFill: "#1A7F2B", dark: "#32D74B", darkFill: "#1A7F2B" },
}

export const ACCENT_IDS: AccentId[] = ["system", "graphite", "bleu", "violet", "rose", "rouge", "orange", "jaune", "vert"]

export function readAccent(storage: Pick<Storage, "getItem"> | null): AccentId {
  try {
    const v = storage?.getItem(ACCENT_KEY)
    return v && Object.hasOwn(ACCENTS, v) ? (v as AccentId) : "system"
  } catch {
    return "system"
  }
}

/** CSS custom properties to set on the root for an accent; null for "system" (the stylesheet default). */
export function accentVars(id: AccentId): Record<string, string> | null {
  if (id === "system") return null
  const a = ACCENTS[id]
  return { "--acL": a.light, "--acLS": a.lightFill, "--acD": a.dark, "--acDS": a.darkFill }
}

export function applyAccent(id: AccentId): void {
  const root = document.documentElement
  const vars = accentVars(id)
  for (const name of ["--acL", "--acLS", "--acD", "--acDS"]) {
    if (vars) root.style.setProperty(name, vars[name])
    else root.style.removeProperty(name)
  }
  root.dataset.accent = id
}

export function saveAccent(id: AccentId): void {
  try {
    if (id === "system") localStorage.removeItem(ACCENT_KEY)
    else localStorage.setItem(ACCENT_KEY, id)
  } catch {
    // Storage unavailable: the choice lasts for this session only.
  }
  applyAccent(id)
}

// ------------------------------------------------------------ reading prefs

export type TextSize = "small" | "medium" | "large"
export const TEXT_SIZE_KEY = "cs-text-size"
export const SHORTCUTS_KEY = "cs-shortcuts"

/** Transcript text size as a CSS length, exposed as --transcript-size. */
export const TEXT_SIZES: Record<TextSize, { label: string; css: string }> = {
  small: { label: "Petit", css: "13.5px" },
  medium: { label: "Moyen", css: "15px" },
  large: { label: "Grand", css: "17px" },
}

export function readTextSize(storage: Pick<Storage, "getItem"> | null): TextSize {
  try {
    const v = storage?.getItem(TEXT_SIZE_KEY)
    return v === "small" || v === "large" ? v : "medium"
  } catch {
    return "medium"
  }
}

export function readShortcuts(storage: Pick<Storage, "getItem"> | null): boolean {
  try {
    return storage?.getItem(SHORTCUTS_KEY) !== "0"
  } catch {
    return true
  }
}

export function applyTextSize(size: TextSize): void {
  document.documentElement.style.setProperty("--transcript-size", TEXT_SIZES[size].css)
}

export function applyShortcuts(show: boolean): void {
  document.documentElement.dataset.shortcuts = show ? "on" : "off"
}

function write(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key)
    else localStorage.setItem(key, value)
  } catch {
    // Storage unavailable: the choice lasts for this session only.
  }
}

export function saveTextSize(size: TextSize): void {
  write(TEXT_SIZE_KEY, size === "medium" ? null : size)
  applyTextSize(size)
}

export function saveShortcuts(show: boolean): void {
  write(SHORTCUTS_KEY, show ? null : "0")
  applyShortcuts(show)
}

/** Applies every stored appearance choice. Called once at start-up. */
export function applyStored(): void {
  const s = (() => {
    try {
      return window.localStorage
    } catch {
      return null
    }
  })()
  applyTheme(readChoice(s))
  applyAccent(readAccent(s))
  applyTextSize(readTextSize(s))
  applyShortcuts(readShortcuts(s))
}
