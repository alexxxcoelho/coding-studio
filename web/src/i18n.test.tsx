// The interface is French: no common English UI word may appear in the text
// the pages render (empty, loading and error states included). The word list
// is shared with the visual check (e2e/visual.py); code, identifiers and
// verbatim interview text are skipped like there.

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, waitFor } from "@testing-library/react"
import type { ReactElement } from "react"

import words from "../e2e/english-words.json"
import { AuditPage } from "@/pages/audit"
import { CodebookPage } from "@/pages/codebook"
import { GuidePage } from "@/pages/guide"
import { LibraryPage } from "@/pages/library"
import { SaturationPage } from "@/pages/saturation"
import { SettingsPage } from "@/pages/settings"
import { ToClarifyPage } from "@/pages/to-clarify"
import { mockApi, renderWithClient } from "@/test-utils"

const WORDS = new Set(words.words)
const ALLOWED = new Set(words.allow.map((w) => w.toLowerCase()))

/** English words found in the visible text of `root`, outside the skipped subtrees. */
function englishIn(root: HTMLElement): string[] {
  const found = new Set<string>()
  const skip = words.skip
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  const scan = (text: string) => {
    for (const w of text.match(/[A-Za-zÀ-ÿ']+/g) ?? []) {
      if (w === w.toUpperCase() || ALLOWED.has(w.toLowerCase())) continue
      if (WORDS.has(w.toLowerCase())) found.add(w.toLowerCase())
    }
  }
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (n.parentElement?.closest(skip)) continue
    scan(n.textContent ?? "")
  }
  for (const el of root.querySelectorAll<HTMLElement>("[placeholder],[aria-label],[title]")) {
    if (el.closest(skip) && !el.matches("input,textarea")) continue
    for (const a of ["placeholder", "aria-label", "title"]) scan(el.getAttribute(a) ?? "")
  }
  return [...found].sort()
}

const STUDY = {
  framework: "none",
  phase1_categories: [],
  id_prefixes: ["INT"],
  interviewers: [],
  max_words: 150,
  prompt_template: { version: "v1", header: "", codebook_intro: "", units_intro: "", footer: "" },
  study_name: "Pilote",
}
const PROJECT = {
  attributes: [],
  memo_templates: [],
  checklist: [],
  theme_checks: [],
  evidence_natures: [],
  link_types: [],
  matrix_dimensions: [],
  interpretive_framework: null,
}

const PAGES: [string, () => ReactElement][] = [
  ["Bibliothèque", () => <LibraryPage />],
  ["Livre de codes", () => <CodebookPage />],
  ["Réglages de l'étude", () => <SettingsPage />],
  ["Guide", () => <GuidePage anchor={null} />],
  ["Journal d'audit", () => <AuditPage />],
  ["Saturation", () => <SaturationPage />],
  ["À clarifier", () => <ToClarifyPage />],
]

beforeEach(() => {
  // Every list is empty; anything else answers 404, so the error states render too.
  mockApi()
    .on("GET", /^\/api\/[a-z-]+(\?.*)?$/, { body: [] })
    .on("GET", "/api/config", { body: STUDY })
    .on("GET", "/api/pseudonymisation", { body: { enabled: true, digest: "d".repeat(64), categories: ["person"], n_list_entries: 1, entries: [{ kind: "person", original: "Anna", pseudonym: "Personne 1" }], free_text_covered: false, note: "Le texte libre n'est pas couvert." } })
    .on("GET", "/api/project-config", { body: PROJECT })
    .on("GET", "/api/state", { body: { native_dialogs: false, codebook_size: 0 } })
})
afterEach(cleanup)

describe("French interface", () => {
  it("covers the word list", () => {
    expect(WORDS.size).toBeGreaterThan(50)
  })
  it("detects English text and skips code", () => {
    const div = document.createElement("div")
    div.innerHTML = '<p>Cancel and save</p><code>copy delete</code><button aria-label="Close dialog">x</button><p>OK, Coding Studio</p>'
    expect(englishIn(div)).toEqual(["and", "cancel", "close", "save"])
  })
  for (const [name, page] of PAGES) {
    it(`${name} shows no English UI word`, async () => {
      const { container } = renderWithClient(page())
      await waitFor(() => expect(container.querySelector(".animate-pulse")).toBeNull(), { timeout: 2000 }).catch(() => undefined)
      expect(englishIn(container)).toEqual([])
    })
  }
})
