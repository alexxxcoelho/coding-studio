// From a browser selection to passage offsets.
//
// The API takes positions in UTF-16 code units of the unit text when the
// request says `offset_unit: "utf16"` (docs/API.md, "Passages"). A unit's text
// is rendered as plain text nodes only (no markup inside it other than
// <span>s that wrap pieces of it), so the offset of a point is the number of
// UTF-16 units of text that precede it inside the text element.

export interface Offsets {
  start: number
  end: number
}

/** Number of UTF-16 units of text before the point (node, offset) inside `root`, or null if the point is outside it. */
export function offsetInRoot(root: Node, node: Node, offset: number): number | null {
  if (!root.contains(node)) return null
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  let total = 0
  if (node.nodeType === Node.TEXT_NODE) {
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (n === node) return total + offset
      total += (n as Text).data.length
    }
    return null
  }
  // The point is between children of an element: count the text of the children before it.
  const target = node.childNodes[offset] ?? null
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (target) {
      const pos = target.compareDocumentPosition(n)
      // n is at or after the target (it follows or is contained by it)
      if (n === target || pos & Node.DOCUMENT_POSITION_FOLLOWING || pos & Node.DOCUMENT_POSITION_CONTAINED_BY) return total
    } else if (!node.contains(n) && (node.compareDocumentPosition(n) & Node.DOCUMENT_POSITION_FOLLOWING)) {
      return total
    }
    total += (n as Text).data.length
  }
  return total
}

/** Offsets of a range inside the text element `root`, whitespace at both edges trimmed; null when empty or outside. */
export function rangeOffsets(root: HTMLElement, range: Range, text: string = root.textContent ?? ""): Offsets | null {
  const a = offsetInRoot(root, range.startContainer, range.startOffset)
  const b = offsetInRoot(root, range.endContainer, range.endOffset)
  if (a === null || b === null) return null
  return normalizeSelection(text, Math.min(a, b), Math.max(a, b))
}

/** Trims whitespace at both ends of [start, end) and moves each end off the middle of a surrogate pair. Null when nothing is left. */
export function normalizeSelection(text: string, start: number, end: number): Offsets | null {
  let s = Math.max(0, Math.min(start, text.length))
  let e = Math.max(0, Math.min(end, text.length))
  while (s < e && /\s/.test(text[s])) s++
  while (e > s && /\s/.test(text[e - 1])) e--
  if (s < text.length && isLowSurrogate(text.charCodeAt(s)) && s > 0 && isHighSurrogate(text.charCodeAt(s - 1))) s--
  if (e > 0 && e < text.length && isHighSurrogate(text.charCodeAt(e - 1)) && isLowSurrogate(text.charCodeAt(e))) e++
  return e > s ? { start: s, end: e } : null
}

const isHighSurrogate = (c: number) => c >= 0xd800 && c <= 0xdbff
const isLowSurrogate = (c: number) => c >= 0xdc00 && c <= 0xdfff

/** True when the offsets cover the whole text (the passage is then the turn itself). */
export function coversWholeText(text: string, o: Offsets): boolean {
  return o.start === 0 && o.end === text.length
}
