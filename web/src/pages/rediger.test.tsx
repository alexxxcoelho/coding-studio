import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest"

import { analysisApi } from "@/lib/analysis-api"
import { audit, cases, claim, link, project, projectWithFramework, themePassages, themeSummaries, writing } from "@/lib/analysis-fixtures"
import { mockApi, renderWithClient } from "@/test-utils"

import { RedigerPage } from "./rediger"

vi.mock("@/lib/analysis-api", async (orig) => {
  const actual = await orig<typeof import("@/lib/analysis-api")>()
  return { ...actual, analysisApi: Object.fromEntries(Object.keys(actual.analysisApi).map((k) => [k, vi.fn()])) }
})

const fn = (name: keyof typeof analysisApi) => analysisApi[name] as unknown as Mock

const questions = [
  { id: "Q1", text: "Pourquoi les organisations adoptent-elles l'identification à l'unité ?", ord: 1 },
  { id: "Q2", text: "Comment ?", ord: 2 },
]

function setup(projectConfig = project) {
  mockApi()
    .on("GET", "/api/project-config", { body: projectConfig })
    .on("GET", "/api/research-questions", { body: questions })
    .on("GET", "/api/cases", { body: cases })
    .on("GET", "/api/interviews", { body: [] })
  fn("themes").mockResolvedValue(themeSummaries)
  fn("themeWriting").mockResolvedValue(writing({ parts: { ...writing().parts, mechanism_evidence: { text: "Le groupe exige le marquage.", nature: null, updated_at: null, links: [link(18, "mechanism_evidence")] } } }))
  fn("themePassages").mockResolvedValue(themePassages)
}

beforeEach(() => {
  vi.clearAllMocks()
})
afterEach(() => {
  vi.useRealTimers()
  cleanup()
  vi.unstubAllGlobals()
})

const drop = (el: HTMLElement, kind: "passage" | "theme", id: string) => {
  const mime = `application/x-cs-${kind}`
  const data = JSON.stringify({ kind, id })
  const dt = { types: [mime], getData: (k: string) => (k === mime ? data : "") }
  fireEvent.dragOver(el, { dataTransfer: dt })
  fireEvent.drop(el, { dataTransfer: dt })
}

describe("Rédiger, tabs", () => {
  it("has no framework tab when the study has no framework", async () => {
    setup()
    renderWithClient(<RedigerPage />)
    await screen.findByRole("navigation", { name: "Thèmes retenus" })
    const names = screen.getAllByRole("tab").map((t) => t.textContent)
    expect(names).toEqual(["Par thème", "Synthèse par question", "Audit final", "Décisions documentées"])
  })

  it("adds 'Lecture par le cadre' when the study has one", async () => {
    setup(projectWithFramework)
    renderWithClient(<RedigerPage />)
    await screen.findByRole("tab", { name: "Lecture par le cadre" })
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toContain("Lecture par le cadre")
  })
})

describe("Rédiger, par thème", () => {
  it("lists the retained themes only, in chapter order", async () => {
    setup()
    renderWithClient(<RedigerPage />)
    const nav = await screen.findByRole("navigation", { name: "Thèmes retenus" })
    const items = within(nav).getAllByRole("listitem")
    expect(items).toHaveLength(2)
    expect(items[0]).toHaveTextContent("Le groupe impose le marquage")
    expect(items[1]).toHaveTextContent("Qui supporte les coûts")
    expect(within(nav).queryByText(/La non-adoption/)).toBeNull()
  })

  it("shows the five parts of the theme in the guide's order, with the passages of the theme", async () => {
    setup()
    renderWithClient(<RedigerPage />)
    for (const [i, t] of ["Énoncé analytique", "Mécanisme et preuves", "Variations et cas divergents", "Explications concurrentes et limites", "Mini-synthèse"].entries()) {
      expect(await screen.findByLabelText(`${i + 1}. ${t}`)).toBeInTheDocument()
    }
    const list = screen.getByRole("region", { name: "Passages du thème" })
    expect(within(list).getByText("Extrait numéro 18.")).toBeInTheDocument()
    expect(within(list).getByText("contredit")).toBeInTheDocument()
    expect(within(list).getByText("Non vérifié à l'audio")).toBeInTheDocument()
  })

  it("moves a theme down and sends the order of every theme", async () => {
    setup()
    fn("reorderThemes").mockResolvedValue(themeSummaries)
    renderWithClient(<RedigerPage />)
    fireEvent.click(await screen.findByRole("button", { name: "Descendre Le groupe impose le marquage" }))
    await waitFor(() => expect(fn("reorderThemes")).toHaveBeenCalledWith(["T-0001", "T-0003", "T-0002"]))
  })

  it("reorders by dropping a theme on another", async () => {
    setup()
    fn("reorderThemes").mockResolvedValue(themeSummaries)
    renderWithClient(<RedigerPage />)
    const nav = await screen.findByRole("navigation", { name: "Thèmes retenus" })
    const [first, second] = within(nav).getAllByRole("listitem")
    void first
    drop(second, "theme", "T-0002")
    await waitFor(() => expect(fn("reorderThemes")).toHaveBeenCalledWith(["T-0001", "T-0003", "T-0002"]))
  })
})

describe("Rédiger, links", () => {
  it("shows a link as a chip with its type and its note", async () => {
    setup()
    renderWithClient(<RedigerPage />)
    const part = await screen.findByRole("region", { name: "Mécanisme et preuves" })
    expect(within(part).getByText("ECO-07 U018 soutient")).toBeInTheDocument()
    expect(within(part).getByLabelText("Type du lien ECO-07 U018")).toHaveValue("supports")
  })

  it("creates a link of the default type when a passage is dropped on a part", async () => {
    setup()
    fn("createLink").mockResolvedValue(link(61, "variations_divergent"))
    renderWithClient(<RedigerPage />)
    const part = await screen.findByRole("region", { name: "Variations et cas divergents" })
    drop(part, "passage", "P-61")
    await waitFor(() =>
      expect(fn("createLink")).toHaveBeenCalledWith({
        passage_id: "P-61",
        target: { kind: "theme_part", theme_id: "T-0002", part: "variations_divergent" },
        link_type: "supports",
        note: "",
      }),
    )
  })

  it("does not link a passage twice to the same part", async () => {
    setup()
    renderWithClient(<RedigerPage />)
    const part = await screen.findByRole("region", { name: "Mécanisme et preuves" })
    drop(part, "passage", "P-18")
    await new Promise((r) => setTimeout(r, 30))
    expect(fn("createLink")).not.toHaveBeenCalled()
  })

  it("links from the keyboard with the select of a passage", async () => {
    setup()
    fn("createLink").mockResolvedValue(link(61, "competing_limits"))
    renderWithClient(<RedigerPage />)
    const select = await screen.findByLabelText("Lier ECO-32 U061 à une partie")
    fireEvent.change(select, { target: { value: "competing_limits" } })
    await waitFor(() => expect(fn("createLink")).toHaveBeenCalledWith(expect.objectContaining({ passage_id: "P-61", target: { kind: "theme_part", theme_id: "T-0002", part: "competing_limits" } })))
  })

  it("changes the type of a link and removes it", async () => {
    setup()
    fn("updateLink").mockResolvedValue(link(18, "mechanism_evidence", "nuances"))
    fn("removeLink").mockResolvedValue({ removed: "L-18" })
    renderWithClient(<RedigerPage />)
    const part = await screen.findByRole("region", { name: "Mécanisme et preuves" })
    fireEvent.change(within(part).getByLabelText("Type du lien ECO-07 U018"), { target: { value: "nuances" } })
    await waitFor(() => expect(fn("updateLink")).toHaveBeenCalledWith("L-18", { link_type: "nuances" }))
    fireEvent.click(within(part).getByRole("button", { name: "Retirer le lien ECO-07 U018 soutient" }))
    await waitFor(() => expect(fn("removeLink")).toHaveBeenCalledWith("L-18"))
  })
})

describe("Rédiger, part editing autosave", () => {
  it("saves once, after the pause, with the last text", async () => {
    setup()
    fn("setThemePart").mockResolvedValue({ text: "x", nature: null, updated_at: "t", links: [] })
    renderWithClient(<RedigerPage />)
    const area = (await screen.findByLabelText("1. Énoncé analytique")) as HTMLTextAreaElement
    vi.useFakeTimers()
    fireEvent.change(area, { target: { value: "Le" } })
    fireEvent.change(area, { target: { value: "Le groupe" } })
    fireEvent.change(area, { target: { value: "Le groupe impose." } })
    await act(async () => {
      await vi.advanceTimersByTimeAsync(700)
    })
    expect(fn("setThemePart")).not.toHaveBeenCalled()
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200)
    })
    expect(fn("setThemePart")).toHaveBeenCalledTimes(1)
    expect(fn("setThemePart")).toHaveBeenCalledWith("T-0002", "analytical_statement", { text: "Le groupe impose." })
  })

  it("saves at once on blur", async () => {
    setup()
    fn("setThemePart").mockResolvedValue({ text: "x", nature: null, updated_at: "t", links: [] })
    renderWithClient(<RedigerPage />)
    const area = await screen.findByLabelText("5. Mini-synthèse")
    fireEvent.change(area, { target: { value: "Réponse partielle." } })
    fireEvent.blur(area)
    await waitFor(() => expect(fn("setThemePart")).toHaveBeenCalledWith("T-0002", "mini_synthesis", { text: "Réponse partielle." }))
  })

  it("saves the nature of the evidence of a part from the study's list", async () => {
    setup()
    fn("setThemePart").mockResolvedValue({ text: "Le groupe exige le marquage.", nature: "opinion", updated_at: "t", links: [] })
    renderWithClient(<RedigerPage />)
    const select = await screen.findByLabelText("Nature de la preuve : Mécanisme et preuves")
    expect(within(select).getByRole("option", { name: "Témoignage de première main" })).toBeInTheDocument()
    fireEvent.change(select, { target: { value: "opinion" } })
    await waitFor(() => expect(fn("setThemePart")).toHaveBeenCalledWith("T-0002", "mechanism_evidence", { nature: "opinion" }))
  })
})

describe("Rédiger, lecture par le cadre", () => {
  it("crosses the retained themes with the dimensions and saves a limit of evidence", async () => {
    setup(projectWithFramework)
    const reading = { theme_id: "T-0002", cells: {}, out_of_frame: false, out_of_frame_note: "", complementary_dimension: "", evidence_limit: "" }
    fn("frameworkReading").mockResolvedValue(reading)
    fn("setFrameworkReading").mockResolvedValue(reading)
    renderWithClient(<RedigerPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Lecture par le cadre" }))
    const table = await screen.findByRole("table", { name: "Lecture par le cadre" })
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Thème", "Technologie", "Organisation", "Environnement", "Hors cadre", "Dimension complémentaire", "Limite de preuve"])
    expect(within(table).getAllByRole("rowheader")).toHaveLength(2)
    const limit = await within(table).findByLabelText("Le groupe impose le marquage : limite de preuve")
    fireEvent.change(limit, { target: { value: "Un seul fabricant" } })
    fireEvent.blur(limit)
    await waitFor(() => expect(fn("setFrameworkReading")).toHaveBeenCalledWith("T-0002", expect.objectContaining({ evidence_limit: "Un seul fabricant" })))
  })
})

describe("Rédiger, synthèse par question", () => {
  it("shows the themes that answer, and saves the answer with the button", async () => {
    setup()
    fn("questionSynthesis").mockResolvedValue({
      rq_id: "Q1",
      text: questions[0].text,
      themes: [{ theme_id: "T-0002", title: "Le groupe impose le marquage", mini_synthesis: "Le groupe décide." }],
      answer: "",
      limits: "",
      updated_at: null,
      n_versions: 0,
    })
    fn("setQuestionSynthesis").mockResolvedValue({ rq_id: "Q1", text: "", themes: [], answer: "Ça dépend du groupe.", limits: "", updated_at: "2026-10-13T12:00:00Z", n_versions: 1 })
    renderWithClient(<RedigerPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Synthèse par question" }))
    expect(await screen.findByText("Le groupe décide.")).toBeInTheDocument()
    const save = screen.getByRole("button", { name: "Enregistrer" })
    expect(save).toBeDisabled()
    fireEvent.change(screen.getByLabelText("Réponse à la question"), { target: { value: "Ça dépend du groupe." } })
    fireEvent.click(save)
    await waitFor(() => expect(fn("setQuestionSynthesis")).toHaveBeenCalledWith("Q1", { answer: "Ça dépend du groupe.", limits: "" }))
  })
})

describe("Rédiger, audit final", () => {
  async function openAudit(claims = [claim(1), claim(2, { gaps: [{ kind: "unverified_passage", message: "Le passage ECO-07 U018 n'est pas vérifié à l'audio." }] }), claim(3)]) {
    setup()
    fn("latestFinalAudit").mockResolvedValue(audit(claims))
    renderWithClient(<RedigerPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Audit final" }))
    await screen.findByText("Le groupe exige le marquage 1.")
  }

  it("draws three claims with the whole chain", async () => {
    setup()
    fn("latestFinalAudit").mockResolvedValue(null)
    fn("drawFinalAudit").mockResolvedValue(audit([claim(1), claim(2), claim(3)]))
    renderWithClient(<RedigerPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Audit final" }))
    fireEvent.click(await screen.findByRole("button", { name: "Tirer trois affirmations" }))
    const card = await screen.findByRole("article", { name: "Affirmation : Le groupe exige le marquage 1." })
    expect(fn("drawFinalAudit")).toHaveBeenCalledWith(3)
    expect(within(card).getByText("Le groupe impose le marquage")).toBeInTheDocument()
    expect(within(card).getByText("Absence de besoin")).toBeInTheDocument()
    expect(within(card).getByText("Extrait numéro 18.")).toBeInTheDocument()
    expect(within(card).getByText("Version de l'entretien : v2")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Relancer un tirage" })).toBeInTheDocument()
  })

  it("records 'étayée' for a claim with a complete chain", async () => {
    await openAudit()
    fn("setAuditVerdict").mockResolvedValue(audit([]))
    const card = screen.getByRole("article", { name: "Affirmation : Le groupe exige le marquage 1." })
    const save = within(card).getByRole("button", { name: "Enregistrer le verdict" })
    expect(save).toBeDisabled()
    fireEvent.click(within(card).getByLabelText("étayée"))
    fireEvent.click(save)
    await waitFor(() => expect(fn("setAuditVerdict")).toHaveBeenCalledWith(4, "theme_part:T-0002:mechanism_evidence1", "supported", ""))
  })

  it("will not call a claim with a missing link 'étayée'", async () => {
    await openAudit()
    const card = screen.getByRole("article", { name: "Affirmation : Le groupe exige le marquage 2." })
    expect(within(card).getByText("Le passage ECO-07 U018 n'est pas vérifié à l'audio.")).toBeInTheDocument()
    fireEvent.click(within(card).getByLabelText("étayée"))
    expect(within(card).getByText(/corrigez-le avant de conclure/)).toBeInTheDocument()
    expect(within(card).getByRole("button", { name: "Enregistrer le verdict" })).toBeDisabled()
  })

  it("needs a note to nuance or withdraw a claim", async () => {
    await openAudit()
    fn("setAuditVerdict").mockResolvedValue(audit([]))
    const card = screen.getByRole("article", { name: "Affirmation : Le groupe exige le marquage 2." })
    fireEvent.click(within(card).getByLabelText("à nuancer"))
    const save = within(card).getByRole("button", { name: "Enregistrer le verdict" })
    expect(save).toBeDisabled()
    fireEvent.change(within(card).getByLabelText("Note du verdict"), { target: { value: "Dire « plusieurs »" } })
    expect(save).toBeEnabled()
    fireEvent.click(save)
    await waitFor(() => expect(fn("setAuditVerdict")).toHaveBeenCalledWith(4, "theme_part:T-0002:mechanism_evidence2", "to_nuance", "Dire « plusieurs »"))
  })

  it("counts the verdicts and says the result goes to the export", async () => {
    const v = { verdict: "supported" as const, note: "", at: "t" }
    await openAudit([claim(1, { verdict: v }), claim(2, { verdict: { ...v, verdict: "to_withdraw", note: "x" } }), claim(3, { verdict: v })])
    expect(screen.getByRole("status")).toHaveTextContent("3 sur 3 jugées")
    expect(screen.getByRole("status")).toHaveTextContent("Le résultat va dans l'export")
  })
})

describe("Rédiger, export", () => {
  it("exports the written parts as Markdown", async () => {
    setup()
    fn("exportWriting").mockResolvedValue({ dir: "exports/2026-10-13", files: [], markdown: "# Résultats" })
    renderWithClient(<RedigerPage />)
    fireEvent.click(await screen.findByRole("button", { name: "Exporter en Markdown" }))
    await waitFor(() => expect(fn("exportWriting")).toHaveBeenCalledTimes(1))
  })
})

