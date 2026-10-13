import { afterEach, describe, expect, it } from "vitest"
import { coversWholeText, normalizeSelection, offsetInRoot, rangeOffsets } from "./selection"

function mount(html: string): HTMLElement {
  const el = document.createElement("div")
  el.innerHTML = html
  document.body.appendChild(el)
  return el
}

afterEach(() => {
  document.body.innerHTML = ""
})

describe("rangeOffsets", () => {
  it("counts from the start of the text element across wrapped pieces", () => {
    const root = mount(`<span>Six mois pour les deux premiers magasins, </span><span class="mk">surtout à cause de la formation</span><span>.</span>`)
    const range = document.createRange()
    range.setStart(root.children[1].firstChild!, 0)
    range.setEnd(root.children[1].firstChild!, "surtout à cause".length)
    const text = root.textContent!
    const o = rangeOffsets(root, range)!
    expect(text.slice(o.start, o.end)).toBe("surtout à cause")
    expect(o.start).toBe("Six mois pour les deux premiers magasins, ".length)
  })

  it("spans several text nodes", () => {
    const root = mount(`<span>abc</span><span>def</span><span>ghi</span>`)
    const range = document.createRange()
    range.setStart(root.children[0].firstChild!, 1)
    range.setEnd(root.children[2].firstChild!, 2)
    expect(rangeOffsets(root, range)).toEqual({ start: 1, end: 8 })
  })

  it("counts UTF-16 units: an emoji is two", () => {
    const root = mount(`<span>Café 😀 prêt, </span><span>oui</span>`)
    const text = root.textContent!
    const range = document.createRange()
    range.setStart(root.children[1].firstChild!, 0)
    range.setEnd(root.children[1].firstChild!, 3)
    const o = rangeOffsets(root, range)!
    expect(o).toEqual({ start: text.indexOf("oui"), end: text.indexOf("oui") + 3 })
    expect(text.indexOf("oui")).toBe("Café 😀 prêt, ".length)
    expect("Café 😀 prêt, ".length).toBe(14) // 13 characters, 14 UTF-16 units
  })

  it("handles a point between elements", () => {
    const root = mount(`<span>abc</span><span>def</span>`)
    const range = document.createRange()
    range.setStart(root, 1)
    range.setEnd(root, 2)
    expect(rangeOffsets(root, range)).toEqual({ start: 3, end: 6 })
  })

  it("handles a range that ends after the last child", () => {
    const root = mount(`<span>abc</span><span>def</span>`)
    const range = document.createRange()
    range.setStart(root.children[0].firstChild!, 1)
    range.setEnd(root, 2)
    expect(rangeOffsets(root, range)).toEqual({ start: 1, end: 6 })
  })

  it("is null for a point outside the element", () => {
    const root = mount(`<span>abc</span>`)
    const other = mount(`<span>xyz</span>`)
    const range = document.createRange()
    range.setStart(root.firstChild!.firstChild!, 0)
    range.setEnd(other.firstChild!.firstChild!, 2)
    expect(rangeOffsets(root, range)).toBeNull()
    expect(offsetInRoot(root, other, 0)).toBeNull()
  })

  it("is null for a collapsed range", () => {
    const root = mount(`<span>abc</span>`)
    const range = document.createRange()
    range.setStart(root.firstChild!.firstChild!, 1)
    range.collapse(true)
    expect(rangeOffsets(root, range)).toBeNull()
  })

  it("treats a start after the end as empty", () => {
    const root = mount(`<span>abcdef</span>`)
    const range = document.createRange()
    range.setStart(root.firstChild!.firstChild!, 5)
    range.setEnd(root.firstChild!.firstChild!, 2)
    // A Range is always ordered by the DOM: start 5 > end 2 collapses at 2.
    expect(rangeOffsets(root, range)).toBeNull()
  })
})

describe("normalizeSelection", () => {
  const text = "  Oui, 😀 trois semaines.  "
  it("trims white space at both ends", () => {
    expect(normalizeSelection(text, 0, text.length)).toEqual({ start: 2, end: text.length - 2 })
  })
  it("does not cut an emoji in two", () => {
    const i = text.indexOf("😀")
    expect(normalizeSelection(text, i + 1, i + 4)).toEqual({ start: i, end: i + 4 })
    expect(normalizeSelection(text, i, i + 1)).toEqual({ start: i, end: i + 2 })
  })
  it("is null when only white space is selected", () => {
    expect(normalizeSelection("a   b", 1, 4)).toBeNull()
  })
  it("clamps to the text", () => {
    expect(normalizeSelection("abc", -4, 99)).toEqual({ start: 0, end: 3 })
  })
  it("knows a whole-text selection", () => {
    expect(coversWholeText("abc", { start: 0, end: 3 })).toBe(true)
    expect(coversWholeText("abc", { start: 1, end: 3 })).toBe(false)
  })
})
