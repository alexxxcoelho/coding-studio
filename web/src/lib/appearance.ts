// Reactive access to the appearance choices (theme, accent, reading prefs).
// Writers persist through lib/theme.ts and notify subscribers.

import { useEffect, useSyncExternalStore } from "react"

import {
  applyTheme,
  readAccent,
  readChoice,
  readShortcuts,
  readTextSize,
  saveAccent,
  saveChoice,
  saveShortcuts,
  saveTextSize,
  type AccentId,
  type TextSize,
  type ThemeChoice,
} from "@/lib/theme"

export interface Appearance {
  theme: ThemeChoice
  accent: AccentId
  textSize: TextSize
  showShortcuts: boolean
}

function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

function load(): Appearance {
  const s = storage()
  return { theme: readChoice(s), accent: readAccent(s), textSize: readTextSize(s), showShortcuts: readShortcuts(s) }
}

let state: Appearance = load()
const listeners = new Set<() => void>()

function set(patch: Partial<Appearance>) {
  state = { ...state, ...patch }
  listeners.forEach((l) => l())
}

export const appearance = {
  get: () => state,
  /** Re-reads storage (tests, or after another window changed it). */
  reload: () => set(load()),
  setTheme(theme: ThemeChoice) {
    saveChoice(theme)
    set({ theme })
  },
  setAccent(accent: AccentId) {
    saveAccent(accent)
    set({ accent })
  },
  setTextSize(textSize: TextSize) {
    saveTextSize(textSize)
    set({ textSize })
  },
  setShowShortcuts(showShortcuts: boolean) {
    saveShortcuts(showShortcuts)
    set({ showShortcuts })
  },
}

function subscribe(cb: () => void) {
  listeners.add(cb)
  return () => {
    listeners.delete(cb)
  }
}

export function useAppearance(): Appearance {
  const a = useSyncExternalStore(subscribe, appearance.get)
  // While on "system", follow live changes of the OS setting.
  useEffect(() => {
    if (a.theme !== "system" || typeof window.matchMedia !== "function") return
    const mq = window.matchMedia("(prefers-color-scheme: dark)")
    const on = () => applyTheme("system")
    mq.addEventListener("change", on)
    return () => mq.removeEventListener("change", on)
  }, [a.theme])
  return a
}
