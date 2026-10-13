import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { analysisApi } from "@/lib/analysis-api"
import { cases, codes, matrix, project, sheet, themePassages, themeSummaries } from "@/lib/analysis-fixtures"
import { mockApi, renderWithClient } from "@/test-utils"

import { AnalyserPage } from "./analyser"

vi.mock("@/lib/analysis-api", async (orig) => {
  const actual = await orig<typeof import("@/lib/analysis-api")>()
  return { ...actual, analysisApi: Object.fromEntries(Object.keys(actual.analysisApi).map((k) => [k, vi.fn()])) }
})

const fn = (name: keyof typeof analysisApi) => analysisApi[name] as unknown as Mock

function setup(projectConfig = project) {
  const api = mockApi()
    .on("GET", "/api/project-config", { body: projectConfig })
    .on("GET", "/api/codes", { body: { codebook_version: 3, codes } })
    .on("GET", "/api/research-questions", { body: [{ id: "Q1", text: "Pourquoi ?", ord: 1 }, { id: "Q2", text: "Comment ?", ord: 2 }] })
    .on("GET", "/api/cases", { body: cases })
    .on("GET", "/api/interviews", { body: [{ interview_id: "ECO-05" }] })
  fn("themes").mockResolvedValue(themeSummaries)
  fn("theme").mockResolvedValue(sheet())
  fn("themePassages").mockResolvedValue(themePassages)
  fn("matrix").mockResolvedValue(matrix)
  fn("matrixViews").mockResolvedValue([])
  fn("matrixCell").mockResolvedValue({
    row_key: "interview:RET-22",
    col_key: "theme:T-0002",
    row_label: "RET-22",
    col_label: "Qui supporte les coûts",
    value: "contradicted",
    summary: "Le groupe paie, le magasin paie",
    passages: [{ passage_id: "P-88", interview_id: "RET-22", unit_id: "RET-22-U088", text: "Un rappel, c'est tout le carton qui revient.", verification: "verified" }],
    n_interviews: 1,
  })
  return api
}

beforeEach(() => {
  vi.clearAllMocks()
})
afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("Analyser, themes", () => {
  it("lists the themes with title, status, codes and interviews", async () => {
    setup()
    renderWithClient(<AnalyserPage />)
    const first = await screen.findByRole("button", { name: /La non-adoption n'est pas toujours un échec/ })
    expect(within(first).getByText("Révisé")).toBeInTheDocument()
    expect(within(first).getByText("2 codes")).toBeInTheDocument()
    expect(within(first).getByText("9 entretiens")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /Qui supporte les coûts/ })).toBeInTheDocument()
    expect(within(screen.getByRole("button", { name: /Le groupe impose/ })).getByText("Retenu")).toBeInTheDocument()
  })

  it("opens the sheet of the first theme with its supporting codes", async () => {
    setup()
    renderWithClient(<AnalyserPage />)
    expect(await screen.findByRole("region", { name: "Fiche du thème" })).toBeInTheDocument()
    expect(screen.getByLabelText("Proposition")).toHaveValue("Non-adoption rationnelle plutôt que frein.")
    expect(within(screen.getByRole("region", { name: "Codes supports" })).getByText("Absence de besoin")).toBeInTheDocument()
    expect(screen.getByText("12 passages, 9 entretiens")).toBeInTheDocument()
  })

  it("creates a theme from a title", async () => {
    setup()
    fn("createTheme").mockResolvedValue(sheet({ theme_id: "T-0004", title: "Nouveau" }))
    renderWithClient(<AnalyserPage />)
    await screen.findByRole("region", { name: "Fiche du thème" })
    fireEvent.click(screen.getByRole("button", { name: "Nouveau thème" }))
    fireEvent.change(screen.getByLabelText("Titre du nouveau thème"), { target: { value: "Le prêt papier suffit" } })
    fireEvent.click(screen.getByRole("button", { name: "Créer le thème" }))
    await waitFor(() => expect(fn("createTheme")).toHaveBeenCalledWith("Le prêt papier suffit"))
  })

  it("adds a code dropped from the tree on the supporting codes", async () => {
    setup()
    fn("addThemeCode").mockResolvedValue(sheet())
    renderWithClient(<AnalyserPage />)
    const zone = await screen.findByRole("region", { name: "Codes supports" })
    const data = JSON.stringify({ kind: "code", id: "C-0002" })
    const dt = { types: ["application/x-cs-code"], getData: (k: string) => (k === "application/x-cs-code" ? data : "") }
    fireEvent.dragOver(zone, { dataTransfer: dt })
    fireEvent.drop(zone, { dataTransfer: dt })
    await waitFor(() => expect(fn("addThemeCode")).toHaveBeenCalledWith("T-0001", "C-0002"))
  })

  it("adds a code with the button of the tree, and not a code already there", async () => {
    setup()
    fn("addThemeCode").mockResolvedValue(sheet())
    renderWithClient(<AnalyserPage />)
    await screen.findByRole("region", { name: "Fiche du thème" })
    expect(screen.queryByRole("button", { name: "Ajouter Absence de besoin au thème" })).toBeNull()
    fireEvent.click(await screen.findByRole("button", { name: "Ajouter Lot ou unité au thème" }))
    await waitFor(() => expect(fn("addThemeCode")).toHaveBeenCalledWith("T-0001", "C-0002"))
  })
})

describe("Analyser, status and checks", () => {
  it("shows the study's checks and saves one", async () => {
    setup()
    fn("setThemeCheck").mockResolvedValue(sheet())
    renderWithClient(<AnalyserPage />)
    await screen.findByRole("region", { name: "Fiche du thème" })
    expect(screen.getByText("1 sur 3 faits")).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Contrôles avant « retenu »" }))
    fireEvent.click(screen.getByRole("checkbox", { name: "Le thème se distingue-t-il des autres ?" }))
    await waitFor(() => expect(fn("setThemeCheck")).toHaveBeenCalledWith("T-0001", "distinct", true))
  })

  it("does not retain a theme with checks left until a reason is written", async () => {
    setup()
    fn("setThemeStatus").mockResolvedValue(sheet({ status: "retained" }))
    renderWithClient(<AnalyserPage />)
    await screen.findByRole("region", { name: "Fiche du thème" })
    fireEvent.change(screen.getByLabelText("Statut"), { target: { value: "retained" } })
    expect(fn("setThemeStatus")).not.toHaveBeenCalled()
    const group = screen.getByRole("group", { name: "Retenir malgré les contrôles restants" })
    expect(within(group).getByText(/2 contrôles ne sont pas faits/)).toBeInTheDocument()
    const go = within(group).getByRole("button", { name: "Retenir malgré tout" })
    expect(go).toBeDisabled()
    fireEvent.change(within(group).getByLabelText("Raison pour retenir malgré les contrôles restants"), { target: { value: "Cas unique révélateur" } })
    fireEvent.click(go)
    await waitFor(() => expect(fn("setThemeStatus")).toHaveBeenCalledWith("T-0001", { status: "retained", override_reason: "Cas unique révélateur" }))
  })

  it("retains at once when every check is done", async () => {
    setup()
    fn("theme").mockResolvedValue(sheet({ checks: [{ check_id: "coherence", done: true, note: "" }, { check_id: "distinct", done: true, note: "" }, { check_id: "asserts", done: true, note: "" }] }))
    fn("setThemeStatus").mockResolvedValue(sheet({ status: "retained" }))
    renderWithClient(<AnalyserPage />)
    await screen.findByRole("region", { name: "Fiche du thème" })
    fireEvent.change(screen.getByLabelText("Statut"), { target: { value: "retained" } })
    await waitFor(() => expect(fn("setThemeStatus")).toHaveBeenCalledWith("T-0001", { status: "retained" }))
  })

  it("moves a theme back to candidate without a reason", async () => {
    setup()
    fn("theme").mockResolvedValue(sheet({ status: "retained" }))
    fn("setThemeStatus").mockResolvedValue(sheet({ status: "candidate" }))
    renderWithClient(<AnalyserPage />)
    await screen.findByRole("region", { name: "Fiche du thème" })
    fireEvent.change(screen.getByLabelText("Statut"), { target: { value: "candidate" } })
    await waitFor(() => expect(fn("setThemeStatus")).toHaveBeenCalledWith("T-0001", { status: "candidate" }))
  })
})

describe("Analyser, matrix", () => {
  async function openMatrix() {
    setup()
    renderWithClient(<AnalyserPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Matrice" }))
    return screen.findByRole("table", { name: "Matrice" })
  }

  it("shows closed value, count and summary in each cell", async () => {
    const table = await openMatrix()
    const c = within(table).getByRole("button", { name: "ECO-16, Absence de besoin" })
    expect(within(c).getByText("présent")).toBeInTheDocument()
    expect(within(c).getByText("3 passages")).toBeInTheDocument()
    expect(within(c).getByText("Argument exprimé, sans regret")).toBeInTheDocument()
    const unq = within(table).getByRole("button", { name: "ECO-16, Qui supporte les coûts" })
    expect(within(unq).getByText("à qualifier")).toBeInTheDocument()
    const contra = within(table).getByRole("button", { name: "RET-22, Qui supporte les coûts" })
    expect(within(contra).getByText("contredit")).toBeInTheDocument()
    expect(within(table).getByRole("button", { name: "RET-22, Absence de besoin" })).toHaveTextContent("aucun passage")
  })

  it("says 'pas encore codé' for an interview without final coding", async () => {
    const table = await openMatrix()
    expect(within(table).getByText("Pas encore codé, exclu de la matrice")).toBeInTheDocument()
  })

  it("starts with interviews by themes and recomputes when the columns change", async () => {
    await openMatrix()
    expect(fn("matrix")).toHaveBeenCalledWith({ rows: { kind: "interviews" }, cols: { kind: "themes" }, filter: null })
    fireEvent.change(screen.getByLabelText("Colonnes"), { target: { value: "dimension:who-pays" } })
    await waitFor(() => expect(fn("matrix")).toHaveBeenLastCalledWith({ rows: { kind: "interviews" }, cols: { kind: "dimension", id: "who-pays" }, filter: null }))
  })

  it("offers the attributes of the study as rows and as filter", async () => {
    await openMatrix()
    const rows = screen.getByLabelText("Lignes")
    expect(within(rows).getByRole("option", { name: "Type de bibliothèque" })).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText("Filtrer par attribut"), { target: { value: "sector" } })
    fireEvent.change(screen.getByLabelText("Valeur du filtre"), { target: { value: "scolaire" } })
    await waitFor(() => expect(fn("matrix")).toHaveBeenLastCalledWith({ rows: { kind: "interviews" }, cols: { kind: "themes" }, filter: { attribute: "sector", value: "scolaire" } }))
  })

  it("refuses the same list on both sides without calling the server", async () => {
    await openMatrix()
    const calls = fn("matrix").mock.calls.length
    fireEvent.change(screen.getByLabelText("Colonnes"), { target: { value: "interviews" } })
    expect(await screen.findByRole("alert")).toHaveTextContent("Les lignes et les colonnes ne peuvent pas être les mêmes.")
    expect(fn("matrix").mock.calls.length).toBe(calls)
  })

  it("opens the panel of a cell with its passages and saves the value", async () => {
    const table = await openMatrix()
    fn("setMatrixCell").mockResolvedValue({ row_key: "interview:RET-22", col_key: "theme:T-0002", value: "uncertain", summary: "", passages: [], n_interviews: 1, row_label: "", col_label: "" })
    fireEvent.click(within(table).getByRole("button", { name: "RET-22, Qui supporte les coûts" }))
    const panel = await screen.findByRole("complementary", { name: "Cellule RET-22, Qui supporte les coûts" })
    expect(await within(panel).findByText(/Un rappel, c'est tout le carton qui revient/)).toBeInTheDocument()
    expect(within(panel).getByRole("link", { name: "Ouvrir dans le Coder" })).toHaveAttribute("href", expect.stringContaining("RET-22"))
    expect(within(panel).getByText(/Non évoqué/)).toBeInTheDocument()
    fireEvent.change(within(panel).getByLabelText("Valeur de cellule"), { target: { value: "uncertain" } })
    await waitFor(() =>
      expect(fn("setMatrixCell")).toHaveBeenCalledWith({ row_key: "interview:RET-22", col_key: "theme:T-0002", value: "uncertain", summary: "Le groupe paie, le magasin paie" }),
    )
  })
})

describe("Analyser, other tabs", () => {
  it("lists the documented decisions and flags the one without unit", async () => {
    setup()
    renderWithClient(<AnalyserPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Décisions documentées" }))
    expect(await screen.findByText("Commune X, nouveau système de prêt")).toBeInTheDocument()
    expect(screen.getByText("non documentée")).toBeInTheDocument()
    expect(screen.getByText(/1 unité rattachée/)).toBeInTheDocument()
  })

  it("links the Explorer to the Lexique for keyword in context", async () => {
    setup()
    renderWithClient(<AnalyserPage />)
    fireEvent.click(await screen.findByRole("tab", { name: "Explorer" }))
    expect(screen.getAllByRole("link", { name: "Ouvrir le Lexique" })[0]).toHaveAttribute("href", "#/lexique")
    expect(screen.getByRole("region", { name: "Codes par attribut" })).toBeInTheDocument()
  })
})
