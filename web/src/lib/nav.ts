// Navigation helpers of the shell.

import type { Route } from "@/lib/router"

/** Which sidebar entry is active for a route. */
export type NavKey = "library" | "codebook" | "lexique" | "search" | "analyser" | "rediger" | "settings" | "guide"

export function navKeyFor(route: Route): NavKey {
  switch (route.page) {
    case "library":
    case "interview":
    case "to-clarify":
      return "library"
    case "codebook":
      return "codebook"
    case "lexique":
      return "lexique"
    case "search":
      return "search"
    case "analyser":
    case "saturation":
      return "analyser"
    case "rediger":
      return "rediger"
    case "settings":
    case "audit":
      return "settings"
    case "guide":
      return "guide"
  }
}

export function isMac(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
}

/** True for the shortcut that opens Réglages: Cmd+, on macOS, Ctrl+, elsewhere. */
export function isSettingsShortcut(e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): boolean {
  return e.key === "," && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey
}

