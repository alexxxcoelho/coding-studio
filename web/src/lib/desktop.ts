// Tauri on macOS: the window carries a vibrancy material (window-vibrancy, see
// crates/cs-desktop) and the web layer leaves the sidebar translucent over it.

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
