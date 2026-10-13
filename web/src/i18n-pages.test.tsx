// What the pages show, as plain text, dumped for every main screen in empty and
// error states. The French dump is a committed file: translating the interface
// must not change a character of it. (Run `vitest -u` to accept a deliberate change.)

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { act, cleanup, waitFor } from "@testing-library/react"

import App from "@/App"
import { HelpDrawer } from "@/components/help-drawer"
import { SettingsDialog } from "@/components/settings-dialog"
import { setLang } from "@/i18n"
import { mockApi, renderWithClient } from "@/test-utils"
import stop from "../scripts/french-stopwords.json"

const FRENCH = new Set<string>(stop.words)
const ACCENTS = new RegExp(`[${stop.accents}]`)

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
  passage_origins: [],
  passage_roles: [],
  matrix_dimensions: [],
  interpretive_framework: null,
}

export const ROUTES = [
  "#/",
  "#/codebook",
  "#/personnes",
  "#/to-clarify",
  "#/saturation",
  "#/settings",
  "#/audit",
  "#/analyser",
  "#/lexique",
  "#/search",
  "#/rediger",
  "#/guide",
  "#/guide/preparer",
  "#/interviews/INT-01/sources",
  "#/interviews/INT-01/coding",
  "#/interviews/INT-01/highlights",
  "#/interviews/INT-01/ai",
  "#/interviews/INT-01/review",
  "#/interviews/INT-01/harmonise",
  "#/interviews/INT-01/cases",
  "#/interviews/INT-01/verification",
  "#/interviews/INT-01/audit",
]

/** The visible text of `root` and the attributes read aloud or shown on hover, one item per line. */
export function dump(root: HTMLElement): string {
  const lines: string[] = []
  const walk = (node: Node) => {
    const el = node as HTMLElement
    if (["SCRIPT", "STYLE"].includes(el.tagName)) return
    let run = "" // adjacent text nodes read as one string
    const flush = () => {
      const text = run.replace(/\s+/g, " ").trim()
      if (text) lines.push(text)
      run = ""
    }
    for (const a of ["aria-label", "title", "placeholder", "alt"]) {
      const v = el.getAttribute?.(a)
      if (v) lines.push(`[${a}] ${v}`)
    }
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) run += child.textContent ?? ""
      else if (child.nodeType === Node.ELEMENT_NODE) {
        flush()
        walk(child)
      }
    }
    flush()
  }
  walk(root)
  return lines.join("\n")
}

async function settle(container: HTMLElement) {
  await waitFor(() => expect(container.querySelector(".animate-pulse")).toBeNull(), { timeout: 2000 }).catch(() => undefined)
  await act(async () => {
    await new Promise((r) => setTimeout(r, 30))
  })
}

beforeEach(() => {
  mockApi()
    .on("GET", /^\/api\/[a-z-]+(\?.*)?$/, { body: [] })
    .on("GET", "/api/config", { body: STUDY })
    .on("GET", "/api/pseudonymisation", { body: { enabled: true, digest: "d".repeat(64), categories: ["person"], n_list_entries: 1, entries: [{ kind: "person", original: "Anna", pseudonym: "Personne 1" }], free_text_covered: false, note: "Le texte libre n'est pas couvert." } })
    .on("GET", "/api/project-config", { body: PROJECT })
    .on("GET", "/api/state", { body: { native_dialogs: false, codebook_size: 0, db_path: "/tmp/etude.db", coding_db: null, library_root: null, version: "0.0.0" } })
    .on("GET", "/api/verify", { body: { ok: true, n_events: 3 } })
})
afterEach(() => {
  cleanup()
  window.location.hash = ""
})

/** Renders every screen and returns "== route ==" sections of their dumps. */
export async function dumpAll(): Promise<string> {
  const out: string[] = []
  for (const hash of ROUTES) {
    window.location.hash = hash
    const { container, unmount } = renderWithClient(<App />)
    await settle(container)
    out.push(`== ${hash} ==\n${dump(container)}`)
    unmount()
  }
  for (const tab of ["general", "providers", "advanced"] as const) {
    const { unmount } = renderWithClient(<SettingsDialog open onOpenChange={() => undefined} tab={tab} onTabChange={() => undefined} />)
    await settle(document.body)
    out.push(`== réglages ${tab} ==\n${dump(document.body)}`)
    unmount()
    cleanup()
  }
  for (const screen of ["coder", "analyser", "exporter", "codebook"]) {
    const { unmount } = renderWithClient(<HelpDrawer open onOpenChange={() => undefined} screen={screen} />)
    await settle(document.body)
    out.push(`== aide ${screen} ==\n${dump(document.body).slice(0, 1500)}`)
    unmount()
    cleanup()
  }
  return out.join("\n\n") + "\n"
}

describe("French interface text", () => {
  it("is unchanged on every main screen", async () => {
    await expect(await dumpAll()).toMatchFileSnapshot("./__snapshots__/i18n-pages.fr.txt")
  }, 120_000)
})

describe("English interface text", () => {
  afterEach(() => setLang("fr", false))

  it("leaves no French on any main screen", async () => {
    setLang("en", false)
    const text = await dumpAll()
    // Lines that come from the mocked server (study data, server messages) keep their language.
    const fromServer = /Personne 1|texte libre|Pilote/
    const found: string[] = []
    let inGuide = false
    for (const line of text.split("\n")) {
      if (line.startsWith("== ")) inGuide = /guide|aide/.test(line)
      // Section markers, the language names, and the guide prose (it quotes the former French labels) are not interface text.
      if (line.startsWith("== ") || line === "Français" || (inGuide && (line.length > 60 || line.includes("("))) || fromServer.test(line)) continue
      const words = line.toLowerCase().split(/[^\p{L}]+/u).filter((w) => FRENCH.has(w))
      if (words.length || ACCENTS.test(line)) found.push(`${line.slice(0, 120)} <- ${words.join(", ")}`)
    }
    expect(found).toEqual([])
  }, 120_000)

  it("follows the language switch live", async () => {
    window.location.hash = "#/codebook"
    const { container, unmount } = renderWithClient(<App />)
    await settle(container)
    const before = dump(container)
    await act(async () => setLang("en", false))
    await settle(container)
    const after = dump(container)
    expect(after).not.toEqual(before)
    expect(document.documentElement.lang).toBe("en")
    unmount()
  })
})
