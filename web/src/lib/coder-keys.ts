// Keyboard of the Coder screen (docs/guide/fr/02_coder.md, action 3; SPEC F-47).
// One pure function maps a key event to an action; the screen runs it.

export type CoderAction =
  | { type: "code"; n: number }
  | { type: "notRelevant" }
  | { type: "review" }
  | { type: "highlight" }
  | { type: "detail" }
  | { type: "next" }
  | { type: "prev" }
  | { type: "undo" }
  | { type: "search" }
  | { type: "newCode" }
  | { type: "escape" }

export interface KeyLike {
  key: string
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  shiftKey?: boolean
  isComposing?: boolean
  defaultPrevented?: boolean
}

export interface KeyContext {
  /** Focus is in a text field, a select, a text area or an editable element. */
  typing: boolean
  /** A dialog, menu or drawer is open and owns the keyboard. */
  modalOpen?: boolean
}

/** True for the elements that take text. */
export function isTypingTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false
  if (el.isContentEditable) return true
  const tag = el.tagName
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT"
}

/**
 * Action of a key press, or null. Keys that type text are left alone in a
 * field. Cmd or Ctrl with Z undoes and with N makes a code; every other
 * modified key is ignored so that the browser and the system keep theirs.
 */
export function resolveKey(e: KeyLike, ctx: KeyContext): CoderAction | null {
  if (e.isComposing || e.defaultPrevented) return null
  const mod = !!(e.metaKey || e.ctrlKey)
  const key = e.key
  if (mod && !e.altKey) {
    if (ctx.modalOpen || ctx.typing) return null
    if (key.toLowerCase() === "z" && !e.shiftKey) return { type: "undo" }
    if (key.toLowerCase() === "n" && !e.shiftKey) return { type: "newCode" }
    return null
  }
  if (e.altKey || mod) return null
  if (key === "Escape") return { type: "escape" }
  if (ctx.typing || ctx.modalOpen) return null
  if (key === "ArrowDown" || key === "Enter") return { type: "next" }
  if (key === "ArrowUp") return { type: "prev" }
  if (e.shiftKey) return null
  if (/^[1-9]$/.test(key)) return { type: "code", n: Number(key) }
  switch (key.toLowerCase()) {
    case "n":
      return { type: "notRelevant" }
    case "r":
      return { type: "review" }
    case "h":
      return { type: "highlight" }
    case "d":
      return { type: "detail" }
    case "/":
      return { type: "search" }
    default:
      return null
  }
}
