// Tauri on macOS: the window carries a vibrancy material (window-vibrancy, see
// crates/cs-desktop) and the web layer leaves the sidebar translucent over it.
// The native menu talks to the web layer with a DOM event, see onDesktopMenu.

/** True inside the Tauri webview. */
export function inTauri(w: Pick<Window, never> | object = window): boolean {
  return "__TAURI_INTERNALS__" in w || "__TAURI__" in w
}

export function isMacPlatform(nav: { platform?: string; userAgent?: string } = navigator): boolean {
  return /Mac/.test(nav.platform || "") || /Macintosh/.test(nav.userAgent || "")
}

/** True when the desktop shell applied the sidebar material: the flag it sets, else Tauri on macOS. */
export function hasVibrancy(w: object = window, nav: { platform?: string; userAgent?: string } = navigator): boolean {
  if ((w as { __CS_VIBRANCY__?: unknown }).__CS_VIBRANCY__ === true) return true
  return inTauri(w) && isMacPlatform(nav)
}

/** Sets html[data-vibrancy="on"], which leaves the sidebar translucent over the window material. */
export function markDesktop(): void {
  if (hasVibrancy()) document.documentElement.dataset.vibrancy = "on"
}

/** Actions of the native menu (crates/cs-desktop/src/menu.rs) that the web layer carries out. */
export type MenuAction = "settings" | "toggle-sidebar" | "guide" | "import" | "export"

/** Name of the DOM event that the desktop shell dispatches on `window`; `detail` is the action. */
export const MENU_EVENT = "cs-menu"

/**
 * Calls `handler` for each menu action the desktop shell sends. Returns the
 * function that stops listening. Does nothing in a plain browser, where the
 * event never fires.
 */
export function onDesktopMenu(handler: (action: MenuAction) => void, target: Pick<Window, "addEventListener" | "removeEventListener"> = window): () => void {
  const on = (e: Event) => {
    const action = (e as CustomEvent<unknown>).detail
    if (typeof action === "string") handler(action as MenuAction)
  }
  target.addEventListener(MENU_EVENT, on)
  return () => target.removeEventListener(MENU_EVENT, on)
}

let pending: MenuAction | null = null

/** Remembers an action for a screen that is not mounted yet (Importer, from another screen). */
export function holdMenuAction(action: MenuAction): void {
  pending = action
}

/** True once if `action` was held; the screen that mounts then carries it out. */
export function takeMenuAction(action: MenuAction): boolean {
  if (pending !== action) return false
  pending = null
  return true
}

/**
 * Aligns the native window appearance (title bar, vibrancy material of the
 * sidebar) with the theme chosen in Réglages. Without it, "Clair" on a Mac in
 * dark mode leaves a dark sidebar material under light text. `null` follows
 * the system. Does nothing outside Tauri.
 */
export function syncNativeTheme(theme: "light" | "dark" | null, w: object = window): Promise<void> {
  if (!inTauri(w)) return Promise.resolve()
  return import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(theme))
    .catch(() => undefined)
}
