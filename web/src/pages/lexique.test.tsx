// The Lexique screen: route, sidebar entry, tools, and the Surlignages inspector.

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, within } from "@testing-library/react"

import { Sidebar } from "@/components/sidebar"
import { screenForRoute } from "@/lib/guide"
import { navKeyFor } from "@/lib/nav"
import { parseHash } from "@/lib/router"
import { LexiquePage } from "@/pages/lexique"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/codes", { body: { codes: [], codebook_version: 1 } })
  mock.on("GET", "/api/interviews", { body: [{ interview_id: "ECO-16" }, { interview_id: "RET-22" }] })
  mock.on("GET", "/api/project-config", { body: { attributes: [] } })
  mock.on("GET", "/api/state", { body: { codebook_size: 0 } })
  mock.on("GET", "/api/audit/verify", { body: { ok: true, n_events: 0 } })
  mock.on("GET", "/api/lexicon/keywords", { body: { scope: "highlights", n_passages: 0, n_terms_in_passages: 0, n_rows: 0, rows: [], note: "" } })
})
afterEach(() => cleanup())

describe("route", () => {
  it("parses #/lexique and keeps the Lexique entry of the sidebar active", () => {
    const r = parseHash("#/lexique")
    expect(r).toEqual({ page: "lexique" })
    expect(navKeyFor(r)).toBe("lexique")
    expect(screenForRoute(r)).toBe("coder")
  })

  it("has a sidebar link under Analyse", () => {
    renderWithClient(<Sidebar route={{ page: "lexique" }} openIds={[]} onOpenSettings={() => {}} onToggle={() => {}} />)
    const link = screen.getByRole("link", { name: "Lexique" })
    expect(link).toHaveAttribute("href", "#/lexique")
    expect(link).toHaveAttribute("aria-current", "page")
  })
})

describe("LexiquePage", () => {
  it("opens on the keyword table, with its empty state", async () => {
    renderWithClient(<LexiquePage />)
    expect(screen.getByRole("tab", { name: "Mots-clés" })).toHaveAttribute("aria-selected", "true")
    expect(await screen.findByText("Aucun surlignage à analyser")).toBeInTheDocument()
  })

  it("switches between the four tools, each with its explanation", async () => {
    mock.on("GET", "/api/interviews/ECO-16/highlights", { body: [] })
    renderWithClient(<LexiquePage />)

    fireEvent.click(screen.getByRole("tab", { name: "Recherche en contexte" }))
    expect(screen.getByRole("search", { name: "Recherche en contexte" })).toBeInTheDocument()
    expect(screen.getByText(/Casse et accents sont ignorés/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole("tab", { name: "Fréquences par attribut" }))
    expect(await screen.findByText("Aucun attribut défini")).toBeInTheDocument()

    fireEvent.click(screen.getByRole("tab", { name: "Passages proches" }))
    expect(await screen.findByText("Aucun surlignage dans cet entretien")).toBeInTheDocument()
    const intro = screen.getByTestId("tool-intro")
    expect(within(intro).getByRole("link", { name: "Lire le guide" })).toHaveAttribute("href", "#/guide/coder-lexique")
  })
})
