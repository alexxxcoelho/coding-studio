// Contract of the shell: sidebar sections and active entry, the keyboard
// shortcut of Réglages, the help drawer per screen, the toolbar slots.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { AppShell } from "@/components/app-shell"
import { PageTitle } from "@/components/bits"
import { openInterviews } from "@/lib/open-interviews"
import { appearance } from "@/lib/appearance"
import { parseHash, type Route } from "@/lib/router"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { InterviewSummary } from "@/lib/types"

let mock: MockApi

function summary(id: string): InterviewSummary {
  const phase = { phase: 1 as const, n_units: 0, n_human_coded: 0, human_complete: false, n_ai_runs: 0, n_ai_runs_complete: 0, n_divergences: 0, n_arbitrated: 0, reconciled: false }
  return { interview_id: id, folder_name: id, n_sources: 1, primary_source: null, n_units: 12, n_warnings: 0, phase1: { ...phase, n_units: 12, n_human_coded: 5 }, phase2: { ...phase, phase: 2 } }
}

beforeEach(() => {
  localStorage.clear()
  sessionStorage.clear()
  openInterviews.reset()
  appearance.reload()
  mock = mockApi()
  mock.on("GET", "/api/interviews", { body: [summary("ECO-07"), summary("RET-22")] })
  mock.on("GET", "/api/state", { body: { version: "0.1", db_path: "/data/cs.db", library_root: "/lib", coding_db: null, actor: "a", codebook_size: 31, native_dialogs: false, chain: { ok: true, n_events: 4, first_bad_seq: null } } })
  mock.on("GET", "/api/audit/verify", { body: { ok: true, n_events: 4, first_bad_seq: null } })
  mock.on("GET", "/api/providers", { body: [] })
})
afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function shell(hash: string, children: React.ReactNode = null) {
  const route: Route = parseHash(hash)
  return renderWithClient(<AppShell route={route}>{children}</AppShell>)
}

const nav = () => screen.getByRole("navigation", { name: "Barre latérale" })

describe("sidebar", () => {
  it("lists the three sections and their entries", async () => {
    shell("#/interviews")
    const n = within(nav())
    for (const h of ["Corpus", "Analyse", "Étude"]) expect(n.getByText(h)).toBeInTheDocument()
    for (const l of ["Bibliothèque", "Codebook", "Analyser", "Rédiger", "Paramètres de l'étude", "Guide"]) {
      expect(n.getByRole("link", { name: new RegExp(`^${l}`) })).toBeInTheDocument()
    }
    expect(n.getByRole("button", { name: "Réglages" })).toBeInTheDocument()
    expect(n.getByText("Pseudonymisation inactive")).toBeInTheDocument()
    // The count of the library comes from the API.
    expect(await n.findByRole("link", { name: "Bibliothèque 2" })).toBeInTheDocument()
  })

  it("marks the entry of the current route", () => {
    shell("#/codebook")
    expect(within(nav()).getByRole("link", { name: /^Codebook/ })).toHaveAttribute("aria-current", "page")
    expect(within(nav()).getByRole("link", { name: /^Bibliothèque/ })).not.toHaveAttribute("aria-current")
  })

  it("sends the old cross-case and answers links to Analyser and Rédiger", () => {
    shell("#/cross-case")
    expect(within(nav()).getByRole("link", { name: /^Analyser/ })).toHaveAttribute("aria-current", "page")
    cleanup()
    shell("#/answers")
    expect(within(nav()).getByRole("link", { name: /^Rédiger/ })).toHaveAttribute("aria-current", "page")
    cleanup()
    shell("#/audit")
    expect(within(nav()).getByRole("link", { name: /^Paramètres de l'étude/ })).toHaveAttribute("aria-current", "page")
  })

  it("shows the open interviews under Bibliothèque and marks the current one", async () => {
    shell("#/interviews/ECO-07/coding")
    const item = await within(nav()).findByRole("link", { name: /^ECO-07/ })
    expect(item).toHaveAttribute("aria-current", "page")
    expect(item).toHaveAttribute("href", "#/interviews/ECO-07/coding")
    expect(within(nav()).getByRole("link", { name: /^Bibliothèque/ })).not.toHaveAttribute("aria-current")
    expect(await within(nav()).findByText("5/12")).toBeInTheDocument()
  })

  it("closes an open interview from its entry", async () => {
    openInterviews.open("RET-22")
    shell("#/codebook")
    fireEvent.click(await within(nav()).findByRole("button", { name: "Fermer RET-22" }))
    expect(within(nav()).queryByRole("link", { name: /^RET-22/ })).toBeNull()
  })

  it("collapses and comes back from the toolbar", () => {
    shell("#/codebook")
    fireEvent.click(within(nav()).getByRole("button", { name: "Masquer la barre latérale" }))
    expect(screen.queryByRole("navigation", { name: "Barre latérale" })).toBeNull()
    expect(localStorage.getItem("cs-sidebar-hidden")).toBe("1")
    fireEvent.click(screen.getByRole("button", { name: "Afficher la barre latérale" }))
    expect(nav()).toBeInTheDocument()
  })
})

describe("Réglages", () => {
  it("opens with Cmd+, and with Ctrl+,", async () => {
    shell("#/codebook")
    expect(screen.queryByRole("dialog")).toBeNull()
    fireEvent.keyDown(window, { key: ",", metaKey: true })
    const d = await screen.findByRole("dialog", { name: /Réglages/ })
    expect(within(d).getByRole("tab", { name: "Général" })).toBeInTheDocument()
    expect(within(d).getByRole("tab", { name: "Fournisseurs IA" })).toBeInTheDocument()
    expect(within(d).getByRole("tab", { name: "Avancé" })).toBeInTheDocument()
    fireEvent.keyDown(document.body, { key: "Escape" })
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    fireEvent.keyDown(window, { key: ",", ctrlKey: true })
    expect(await screen.findByRole("dialog", { name: /Réglages/ })).toBeInTheDocument()
  })

  it("ignores a bare comma", () => {
    shell("#/codebook")
    fireEvent.keyDown(window, { key: "," })
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  it("opens from the sidebar and sets appearance, accent and text size", async () => {
    shell("#/codebook")
    fireEvent.click(within(nav()).getByRole("button", { name: "Réglages" }))
    const d = await screen.findByRole("dialog", { name: /Réglages/ })
    fireEvent.click(within(d).getByRole("radio", { name: "Sombre" }))
    expect(document.documentElement.classList.contains("dark")).toBe(true)
    expect(localStorage.getItem("cs-theme")).toBe("dark")
    fireEvent.click(within(d).getByRole("radio", { name: "Violet" }))
    expect(localStorage.getItem("cs-accent")).toBe("violet")
    expect(document.documentElement.style.getPropertyValue("--acL")).toBe("#AF52DE")
    expect(within(d).getByRole("radio", { name: "Violet" })).toHaveAttribute("aria-checked", "true")
    fireEvent.click(within(d).getByRole("radio", { name: "Grand" }))
    expect(document.documentElement.style.getPropertyValue("--transcript-size")).toBe("17px")
    fireEvent.click(within(d).getByRole("switch", { name: "Afficher les raccourcis" }))
    expect(document.documentElement.dataset.shortcuts).toBe("off")
  })

  it("offers the model providers in their tab", async () => {
    shell("#/codebook")
    fireEvent.keyDown(window, { key: ",", metaKey: true })
    const d = await screen.findByRole("dialog", { name: /Réglages/ })
    fireEvent.mouseDown(within(d).getByRole("tab", { name: "Fournisseurs IA" }), { button: 0 })
    fireEvent.keyDown(within(d).getByRole("tab", { name: "Fournisseurs IA" }), { key: "Enter" })
    expect(await within(d).findByRole("region", { name: "Fournisseurs de modèles" })).toBeInTheDocument()
  })

  it("shows the database path in Avancé", async () => {
    shell("#/codebook")
    fireEvent.keyDown(window, { key: ",", metaKey: true })
    const d = await screen.findByRole("dialog", { name: /Réglages/ })
    const tab = within(d).getByRole("tab", { name: "Avancé" })
    fireEvent.mouseDown(tab, { button: 0 })
    fireEvent.keyDown(tab, { key: "Enter" })
    expect(await within(d).findByText("/data/cs.db")).toBeInTheDocument()
    expect(within(d).getByRole("link", { name: "Ouvrir" })).toHaveAttribute("href", "#/audit")
  })
})

describe("help drawer", () => {
  async function helpLink(hash: string) {
    shell(hash)
    fireEvent.click(screen.getByRole("button", { name: "Aide pour cet écran" }))
    const d = await screen.findByRole("dialog")
    return within(d).getByRole("link", { name: "Ouvrir dans le guide" })
  }

  it("opens the section mapped to the screen", async () => {
    expect(await helpLink("#/interviews/ECO-07/coding")).toHaveAttribute("href", "#/guide/coder-parcourir")
    cleanup()
    expect(await helpLink("#/interviews/ECO-07/ai")).toHaveAttribute("href", "#/guide/revoir-prompt")
    cleanup()
    expect(await helpLink("#/interviews/ECO-07/review")).toHaveAttribute("href", "#/guide/revoir-arbitrer")
    cleanup()
    expect(await helpLink("#/interviews")).toHaveAttribute("href", "#/guide/preparer-bibliotheque")
    cleanup()
    expect(await helpLink("#/codebook")).toHaveAttribute("href", "#/guide/coder-codebook")
    cleanup()
    expect(await helpLink("#/rediger")).toHaveAttribute("href", "#/guide/rediger-theme")
    cleanup()
    expect(await helpLink("#/settings")).toHaveAttribute("href", "#/guide/preparer-reglages")
  })

  it("renders the section text with the anchor heading and scrolls to it", async () => {
    const scroll = vi.fn()
    Element.prototype.scrollIntoView = scroll
    shell("#/codebook")
    fireEvent.click(screen.getByRole("button", { name: "Aide pour cet écran" }))
    const d = await screen.findByRole("dialog")
    await waitFor(() => expect(d.querySelector("#coder-codebook")).not.toBeNull())
    expect(d.querySelector("#coder-codebook")?.textContent).toContain("Organiser l'arbre de codes")
    await waitFor(() => expect(scroll).toHaveBeenCalled())
    expect(within(d).getByText("Coder un entretien")).toBeInTheDocument()
  })

  it("closes when a link of the guide is followed", async () => {
    shell("#/codebook")
    fireEvent.click(screen.getByRole("button", { name: "Aide pour cet écran" }))
    const d = await screen.findByRole("dialog")
    fireEvent.click(within(d).getByRole("link", { name: "Ouvrir dans le guide" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
  })
})

describe("toolbar", () => {
  it("carries the title, subtitle and actions a page supplies", async () => {
    shell("#/codebook", <PageTitle title="Codebook" sub="31 codes" actions={<button type="button">Importer</button>} />)
    const bar = document.querySelector(".tb") as HTMLElement
    await waitFor(() => expect(within(bar).getByRole("heading", { name: "Codebook" })).toBeInTheDocument())
    expect(within(bar).getByText("31 codes")).toBeInTheDocument()
    expect(within(bar).getByRole("button", { name: "Importer" })).toBeInTheDocument()
    expect(within(bar).getByRole("button", { name: "Aide pour cet écran" })).toBeInTheDocument()
  })

  it("renders the same parts inline when there is no shell", () => {
    renderWithClient(<PageTitle title="Seul" sub="sans coque" />)
    expect(screen.getByRole("heading", { name: "Seul" })).toBeInTheDocument()
  })
})
