// Light / dark theme. "system" follows prefers-color-scheme; an explicit
// choice is kept in localStorage. index.html applies it before first paint.

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

export function applyTheme(choice: ThemeChoice): void {
  const dark = resolveTheme(choice, window.matchMedia("(prefers-color-scheme: dark)").matches) === "dark"
  const root = document.documentElement
  root.classList.toggle("dark", dark)
  root.style.colorScheme = dark ? "dark" : "light"
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
