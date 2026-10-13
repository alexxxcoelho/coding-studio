// The tools of the Lexique screen: keyword table (sort, select, create a code),
// keyword in context rows, near passages, term frequency, empty states.

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { KeywordTable } from "@/components/lexicon/keyword-table"
import { KwicRow, KwicSearch } from "@/components/lexicon/kwic-search"
import { PassageLexicon, SimilarPassages } from "@/components/lexicon/passage-panel"
import { TermFrequency } from "@/components/lexicon/term-frequency"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { KeywordRow, KwicHit } from "@/lib/lexicon-api"
import type { CodeNode, Passage } from "@/lib/types"

function kw(over: Partial<KeywordRow> & { stem: string }): KeywordRow {
  return { forms: [over.stem], n_passages: 1, n_interviews: 1, interviews: ["ECO-16"], passage_ids: [], n_occurrences: 1, corpus_occurrences: 10, keyness: 0, ...over }
}
const ROWS: KeywordRow[] = [
  kw({ stem: "lot", forms: ["lot", "lots"], n_passages: 3, n_interviews: 2, interviews: ["ECO-16", "ECO-32"], passage_ids: ["P1", "P2", "P3"], n_occurrences: 5, corpus_occurrences: 40, keyness: 25.4 }),
  kw({ stem: "rappel", forms: ["rappel", "rappeler"], n_passages: 2, n_interviews: 1, passage_ids: ["P2", "P4"], n_occurrences: 4, corpus_occurrences: 12, keyness: 31.2 }),
  kw({ stem: "cout", forms: ["coût"], n_passages: 5, n_interviews: 2, interviews: ["ECO-16", "ECO-32"], passage_ids: ["P5"], keyness: 9.1 }),
]

function hl(id: string, interview: string, unit: string, text: string): Passage {
  return { passage_id: id, interview_id: interview, unit_id: unit, text, kind: "highlight", stale: false, start: 0, end: text.length, codings: [] } as unknown as Passage
}
const CODE = { code_id: "C1", name: "Rappel de lot", label: "", active: true, system: false, merged_into: null, parent_id: null } as unknown as CodeNode

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/codes", { body: { codes: [CODE], codebook_version: 1 } })
  mock.on("GET", "/api/project-config", {
    body: { attributes: [{ id: "secteur", label: "Secteur", kind: "choice", choices: ["Vin", "Bière"], shared_with_model: false }] },
  })
})
afterEach(() => cleanup())

const termOrder = () => screen.getAllByRole("row").slice(1).map((r) => r.getAttribute("data-stem"))

describe("KeywordTable", () => {
  beforeEach(() => {
    mock.on("GET", "/api/lexicon/keywords", { body: { scope: "highlights", n_passages: 5, n_terms_in_passages: 30, n_rows: 3, rows: ROWS, note: "" } })
  })

  it("lists the terms by keyness, with stem group, frequency against the corpus and interviews", async () => {
    renderWithClient(<KeywordTable />)
    await screen.findByText("lot, lots")
    expect(termOrder()).toEqual(["rappel", "lot", "cout"])
    const lot = screen.getByText("lot, lots").closest("tr")!
    expect(within(lot).getByText("5 / 40")).toBeInTheDocument()
    expect(within(lot).getByText("25,4")).toBeInTheDocument()
    expect(within(lot).getByRole("link", { name: "ECO-32" })).toHaveAttribute("href", "#/interviews/ECO-32/highlights")
  })

  it("sorts by a column, and reverses on a second click", async () => {
    renderWithClient(<KeywordTable />)
    await screen.findByText("lot, lots")
    fireEvent.click(screen.getByRole("button", { name: "Surlignages" }))
    expect(termOrder()).toEqual(["cout", "lot", "rappel"])
    fireEvent.click(screen.getByRole("button", { name: /^Surlignages/ }))
    expect(termOrder()).toEqual(["rappel", "lot", "cout"])
    fireEvent.click(screen.getByRole("button", { name: "Mot-clé" }))
    expect(termOrder()).toEqual(["cout", "lot", "rappel"])
    expect(screen.getByRole("columnheader", { name: /Mot-clé/ })).toHaveAttribute("aria-sort", "ascending")
  })

  it("selects terms and offers to create a code from them", async () => {
    renderWithClient(<KeywordTable />)
    await screen.findByText("lot, lots")
    const create = screen.getByRole("button", { name: /Créer un code/ })
    expect(create).toBeDisabled()
    fireEvent.click(screen.getByRole("checkbox", { name: "lot" }))
    fireEvent.click(screen.getByRole("checkbox", { name: "rappel" }))
    // table order is by keyness: rappel, lot
    expect(screen.getByRole("button", { name: "Créer un code · rappel, lot, 4 surlignages" })).toBeEnabled()
    fireEvent.click(screen.getByRole("checkbox", { name: "Tout sélectionner" }))
    expect(screen.getByRole("button", { name: "Créer un code · rappel, lot, coût, 5 surlignages" })).toBeInTheDocument()
  })

  it("opens the new code dialog with the terms as name, and codes the other highlights", async () => {
    mock.on("GET", "/api/interviews/ECO-16/highlights", { body: [hl("P1", "ECO-16", "ECO-16-U032", "tout le lot"), hl("P2", "ECO-16", "ECO-16-U040", "un rappel")] })
    mock.on("GET", "/api/interviews/ECO-32/highlights", { body: [hl("P3", "ECO-32", "ECO-32-U061", "le lot reste")] })
    mock.on("POST", "/api/codes/from-passage", { body: { code: { code_id: "C9", name: "lot" }, passage: { passage_id: "P1" }, codebook_version: 2 } })
    mock.on("POST", /\/api\/passages\/P\d\/codings/, { body: {} })
    renderWithClient(<KeywordTable />)
    await screen.findByText("lot, lots")
    fireEvent.click(screen.getByRole("checkbox", { name: "lot" }))
    fireEvent.click(screen.getByRole("button", { name: "Créer un code · lot, 3 surlignages" }))

    const dialog = await screen.findByRole("dialog")
    expect(within(dialog).getByPlaceholderText(/mots courants/)).toHaveValue("lot")
    expect(within(dialog).getByText("tout le lot")).toBeInTheDocument()
    fireEvent.change(within(dialog).getByPlaceholderText(/phrase qui dit/), { target: { value: "Le lot concerné par un rappel." } })
    fireEvent.click(within(dialog).getByRole("button", { name: "Créer et poser" }))

    await waitFor(() => expect(mock.to("POST", "/api/codes/from-passage")).toHaveLength(1))
    expect(mock.to("POST", "/api/codes/from-passage")[0].body).toMatchObject({ name: "lot", passage_id: "P1" })
    await waitFor(() => expect(mock.to("POST", /\/codings$/)).toHaveLength(2))
    expect(mock.to("POST", "/api/passages/P2/codings")[0].body).toEqual({ code: "C9" })
    expect(mock.to("POST", "/api/passages/P3/codings")[0].body).toEqual({ code: "C9" })
  })
})

describe("KeywordTable empty", () => {
  it("explains what the table does and links to the guide", async () => {
    mock.on("GET", "/api/lexicon/keywords", { body: { scope: "highlights", n_passages: 0, n_terms_in_passages: 0, n_rows: 0, rows: [], note: "" } })
    renderWithClient(<KeywordTable />)
    expect(await screen.findByText("Aucun surlignage à analyser")).toBeInTheDocument()
    expect(screen.getByText(/jamais une mesure|n'utilise aucune IA générative/)).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "Lire le guide" })).toHaveAttribute("href", "#/guide/coder-lexique")
  })
})

const HIT: KwicHit = {
  interview_id: "ECO-16",
  unit_id: "ECO-16-U032",
  seq: 32,
  role: "respondent",
  start: 20,
  end: 25,
  left: "Aujourd'hui on rappelle tout le ",
  match: "lot",
  right: ", on ne sait pas quelles bouteilles",
}

describe("KwicRow", () => {
  it("renders the left context, the term and the right context", () => {
    renderWithClient(<KwicRow hit={HIT} />)
    const row = screen.getByTestId("kwic-row")
    expect(row.querySelector('[data-part="left"]')).toHaveTextContent("Aujourd'hui on rappelle tout le")
    expect(row.querySelector('[data-part="match"]')).toHaveTextContent("lot")
    expect(row.querySelector('[data-part="right"]')).toHaveTextContent(", on ne sait pas quelles bouteilles")
    expect(within(row).getByRole("link", { name: "ECO-16 U032" })).toHaveAttribute("href", "#/interviews/ECO-16/coding?unit=ECO-16-U032")
  })
  it("renders the same parts in the compact layout", () => {
    renderWithClient(<KwicRow hit={HIT} compact />)
    expect(screen.getByTestId("kwic-row").querySelector("mark")).toHaveTextContent("lot")
    expect(screen.getByTestId("kwic-row")).toHaveTextContent("tout le lot , on ne sait pas")
  })
})

describe("KwicSearch", () => {
  it("explains the tool before any search", () => {
    renderWithClient(<KwicSearch />)
    expect(screen.getByText("Recherche en contexte", { selector: "p" })).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "Lire le guide" })).toBeInTheDocument()
    expect(mock.to("GET", /kwic/)).toHaveLength(0)
  })

  it("searches by stem with an attribute filter and lists the occurrences", async () => {
    mock.on("GET", /\/api\/lexicon\/kwic/, {
      body: { query: "rappel", stem: true, query_tokens: ["rappel"], total: 1, n_interviews: 1, by_interview: [{ interview_id: "ECO-16", n: 1 }], hits: [HIT], offset: 0 },
    })
    renderWithClient(<KwicSearch />)
    const select = await screen.findByLabelText("Filtrer sur un attribut")
    fireEvent.change(screen.getByLabelText("Mot ou expression à chercher"), { target: { value: "rappel" } })
    fireEvent.click(screen.getByLabelText("Chercher par racine"))
    fireEvent.change(select, { target: { value: "secteur" } })
    fireEvent.change(screen.getByLabelText("Valeur de Secteur"), { target: { value: "Vin" } })
    fireEvent.click(screen.getByRole("button", { name: "Chercher" }))

    expect(await screen.findByText("1 occurrence dans 1 entretien · racines : rappel")).toBeInTheDocument()
    expect(screen.getByTestId("kwic-row")).toBeInTheDocument()
    const url = new URL(mock.to("GET", /kwic/)[0].path, "http://x")
    expect(Object.fromEntries(url.searchParams)).toMatchObject({ q: "rappel", stem: "true", attribute: "secteur", value: "Vin" })
  })

  it("does not send a filter with an attribute but no value", async () => {
    mock.on("GET", /\/api\/lexicon\/kwic/, { body: { query: "x", stem: false, query_tokens: ["x"], total: 0, n_interviews: 0, by_interview: [], hits: [], offset: 0 } })
    renderWithClient(<KwicSearch />)
    fireEvent.change(await screen.findByLabelText("Filtrer sur un attribut"), { target: { value: "secteur" } })
    fireEvent.change(screen.getByLabelText("Mot ou expression à chercher"), { target: { value: "x" } })
    fireEvent.click(screen.getByRole("button", { name: "Chercher" }))
    await screen.findByText("Aucune occurrence")
    const url = new URL(mock.to("GET", /kwic/)[0].path, "http://x")
    expect(url.searchParams.has("attribute")).toBe(false)
    expect(url.searchParams.has("value")).toBe(false)
  })
})

describe("passage lexicon", () => {
  it("asks for a selected passage when there is none", () => {
    renderWithClient(<PassageLexicon passageId={null} />)
    expect(screen.getByText("Aucun passage sélectionné")).toBeInTheDocument()
  })

  it("shows keyphrases, stems and the provisional label, which the researcher can use", async () => {
    mock.on("GET", "/api/passages/P1/lexical", {
      body: { language: "fr", keyphrases: [{ phrase: "tout le lot", score: 0.01, count: 1 }, { phrase: "bouteilles vendues", score: 0.02, count: 1 }], stems: [{ stem: "rappel", forms: ["rappelle"], count: 1 }, { stem: "lot", forms: ["lot"], count: 2 }], label: "lot, rappel", provisional: true, note: "" },
    })
    const used: string[] = []
    renderWithClient(<PassageLexicon passageId="P1" onUseLabel={(l) => used.push(l)} />)
    expect(await screen.findByText("bouteilles vendues")).toBeInTheDocument()
    expect(screen.getByText("rappel, lot")).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Utiliser « lot, rappel » comme libellé provisoire" }))
    expect(used).toEqual(["lot, rappel"])
  })
})

describe("SimilarPassages", () => {
  it("lists the near passages with score and shared terms, and codes one with a code", async () => {
    mock.on("GET", /\/api\/passages\/P1\/similar/, {
      body: {
        passage_id: "P1",
        items: [
          { interview_id: "ECO-32", unit_id: "ECO-32-U061", seq: 61, score: 0.42, shared_terms: [{ stem: "lot", form: "lot", weight: 0.3 }], text: "Le lot reste l'unité de contrôle.", coded_with: [{ code_id: "C2", name: "Traçabilité" }] },
        ],
        note: "Proximité lexicale.",
      },
    })
    mock.on("POST", "/api/interviews/ECO-32/passages", { body: { passage: hl("P7", "ECO-32", "ECO-32-U061", "x"), created: true } })
    renderWithClient(<SimilarPassages passageId="P1" />)
    const card = await screen.findByRole("article", { name: "ECO-32 U061" })
    expect(within(card).getByText("0,42")).toBeInTheDocument()
    expect(within(card).getByText("lot")).toBeInTheDocument()
    expect(within(card).getByText("Traçabilité")).toBeInTheDocument()
    expect(within(card).getByRole("link", { name: "Ouvrir dans Coder" })).toHaveAttribute("href", "#/interviews/ECO-32/coding?unit=ECO-32-U061")
    await within(card).findByRole("option", { name: "Rappel de lot" })
    fireEvent.change(within(card).getByLabelText("Coder ECO-32 U061 avec un code"), { target: { value: "C1" } })
    await waitFor(() => expect(mock.to("POST", "/api/interviews/ECO-32/passages")).toHaveLength(1))
    expect(mock.to("POST", "/api/interviews/ECO-32/passages")[0].body).toMatchObject({ unit_id: "ECO-32-U061", code: "C1" })
  })

  it("explains an empty result", async () => {
    mock.on("GET", /\/api\/passages\/P1\/similar/, { body: { passage_id: "P1", items: [], note: "" } })
    renderWithClient(<SimilarPassages passageId="P1" />)
    expect(await screen.findByText("Aucun passage proche")).toBeInTheDocument()
    expect(screen.getByText(/ne voit ni les synonymes ni les autres langues/)).toBeInTheDocument()
  })
})

describe("TermFrequency", () => {
  it("explains the tool before any count", async () => {
    renderWithClient(<TermFrequency />)
    expect(await screen.findByText("Fréquence par valeur d'attribut")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Compter" })).toBeDisabled()
  })

  it("counts terms per attribute value, in a table", async () => {
    mock.on("GET", /\/api\/lexicon\/term-frequency/, {
      body: {
        attribute: "secteur",
        mode: "terms",
        stem: true,
        note: "",
        terms: [{ term: "rappel", by_value: [{ value: "Vin", n_interviews: 4, n_interviews_with_term: 3, n_units_with_term: 7, n_occurrences: 9, n_words: 12000, per_1000_words: 0.75 }, { value: null, n_interviews: 1, n_interviews_with_term: 0, n_units_with_term: 0, n_occurrences: 0, n_words: 3000, per_1000_words: 0 }] }],
      },
    })
    renderWithClient(<TermFrequency />)
    await screen.findByRole("option", { name: "Secteur" })
    fireEvent.change(screen.getByLabelText("Attribut"), { target: { value: "secteur" } })
    fireEvent.change(screen.getByLabelText("Mots à compter"), { target: { value: " rappel , " } })
    fireEvent.click(screen.getByRole("button", { name: "Compter" }))
    const table = await screen.findByRole("table", { name: "Fréquence des mots par valeur" })
    expect(within(table).getByText("Vin")).toBeInTheDocument()
    expect(within(table).getByText("Non renseigné")).toBeInTheDocument()
    expect(within(table).getByText("(0,8 pour 1000)")).toBeInTheDocument()
    const url = new URL(mock.to("GET", /term-frequency/)[0].path, "http://x")
    expect(Object.fromEntries(url.searchParams)).toMatchObject({ attribute: "secteur", terms: "rappel", stem: "true" })
  })
})
