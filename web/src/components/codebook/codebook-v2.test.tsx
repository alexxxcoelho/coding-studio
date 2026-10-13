import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { CodeJournal } from "@/components/codebook/journal"
import { SplitDialog, splitProblem } from "@/components/codebook/split-dialog"
import { changedDecisions, WorklistDialog } from "@/components/codebook/worklist"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { CodeNode, Passage } from "@/lib/types"
import type { Worklist, WorklistItem } from "@/lib/v2-api"

function node(id: string, name: string, over: Partial<CodeNode> = {}): CodeNode {
  return {
    code_id: id, token: id, name, label: "", definition: `Def ${name}`, includes: "", excludes: "", parent_id: null, origin: "field", toe: null,
    research_questions: [], active: true, system: false, in_prompt: true, merged_into: null, version: 1, example: null, reason: null, memo: "",
    origin_passage_id: null, example_passage_id: null, created_at: null, aliases: [],
    counts: { passages: 3, interviews: 1, passages_rollup: 3, interviews_rollup: 1 }, ...over,
  } as CodeNode
}

const ORIGINAL = node("C-0001", "Stock")
const NEW1 = node("C-0008", "Écart mesuré")
const NEW2 = node("C-0009", "Confiance")

function passage(id: string, text: string): Passage {
  return { passage_id: id, interview_id: "INT-01", unit_id: `INT-01-U00${id.slice(-1)}`, start: 0, end: text.length, text, whole_unit: true, kind: "coded", stale: false, before: "", after: "", codings: [] } as unknown as Passage
}

function item(id: string, status: WorklistItem["status"] = "pending", targets: string[] = []): WorklistItem {
  return { passage: passage(id, `Texte ${id}`), status, targets, note: "", decided_at: null, decided_by: null, n_decisions: 0 }
}

function worklist(items: WorklistItem[], over: Partial<Worklist> = {}): Worklist {
  return {
    op_id: 1, original: ORIGINAL, new_codes: [NEW1, NEW2], rationale: "Deux idées", total: items.length,
    pending: items.filter((i) => i.status === "pending").length, assigned: 0, left: 0, gone: 0, undone: null, items, ...over,
  }
}

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
})
afterEach(() => cleanup())

describe("split", () => {
  it("refuses a single part, a repeated or used name and a missing reason", () => {
    const p = (name: string, definition = "d") => ({ name, definition, includes: "", excludes: "", parent: "" })
    const codes = [ORIGINAL, node("C-0002", "Coût")]
    expect(splitProblem([p("A")], codes, "C-0001", "x")).toMatch(/au moins deux/)
    expect(splitProblem([p("A"), p("a")], codes, "C-0001", "x")).toMatch(/deux fois/)
    expect(splitProblem([p("A"), p("coût")], codes, "C-0001", "x")).toMatch(/porte déjà/)
    expect(splitProblem([p("A"), p("B", "")], codes, "C-0001", "x")).toMatch(/Définissez/)
    expect(splitProblem([p("A"), p("B")], codes, "C-0001", " ")).toMatch(/motif/)
    expect(splitProblem([p("A"), p("B")], codes, "C-0001", "ok")).toBeNull()
  })

  it("sends the parts and the rationale, then hands over the operation", async () => {
    mock.on("POST", "/api/codes/C-0001/split", { body: { op_id: 4, original: ORIGINAL, new_codes: [NEW1, NEW2], worklist: {} } })
    let opened: number | null = null
    renderWithClient(<SplitDialog open code={ORIGINAL} codes={[ORIGINAL]} onClose={() => {}} onSplit={(id) => (opened = id)} />)
    fireEvent.change(screen.getByLabelText("Nom de la partie 1"), { target: { value: "Écart mesuré" } })
    fireEvent.change(screen.getByLabelText("Définition de la partie 1"), { target: { value: "Le chiffre" } })
    fireEvent.change(screen.getByLabelText("Nom de la partie 2"), { target: { value: "Confiance" } })
    fireEvent.change(screen.getByLabelText("Définition de la partie 2"), { target: { value: "Le sentiment" } })
    const go = screen.getByRole("button", { name: "Scinder et réaffecter les passages" })
    expect(go).toBeDisabled()
    fireEvent.change(screen.getByLabelText("Motif, obligatoire"), { target: { value: "Deux idées" } })
    expect(go).toBeEnabled()
    fireEvent.click(go)
    await waitFor(() => expect(opened).toBe(4))
    expect(mock.to("POST", "/api/codes/C-0001/split")[0].body).toEqual({
      parts: [
        { name: "Écart mesuré", definition: "Le chiffre" },
        { name: "Confiance", definition: "Le sentiment" },
      ],
      rationale: "Deux idées",
    })
  })
})

describe("worklist", () => {
  it("sends only the decisions that differ from what the server holds", () => {
    const items = [item("P-1"), item("P-2", "assigned", ["C-0008"]), item("P-3", "left"), item("P-4")]
    const drafts = {
      "P-1": { action: "assign" as const, targets: ["C-0008", "C-0009"] },
      "P-2": { action: "assign" as const, targets: ["C-0008"] },
      "P-3": { action: "leave" as const, targets: [] },
      "P-4": undefined,
    }
    expect(changedDecisions(items, drafts)).toEqual([{ passage_id: "P-1", action: "assign", targets: ["C-0008", "C-0009"] }])
  })

  it("decides with the keys and saves all at once", async () => {
    const wl = worklist([item("P-1"), item("P-2"), item("P-3")])
    mock.on("GET", "/api/splits/1/worklist", { body: wl })
    mock.on("PUT", "/api/splits/1/worklist", { body: worklist([item("P-1", "assigned", ["C-0008", "C-0009"]), item("P-2", "left"), item("P-3")]) })
    renderWithClient(<WorklistDialog opId={1} onClose={() => {}} />)
    const rows = await screen.findAllByRole("listitem")
    expect(rows).toHaveLength(3)
    rows[0].focus()
    fireEvent.keyDown(rows[0], { key: "1" })
    fireEvent.keyDown(rows[0], { key: "2" })
    fireEvent.keyDown(rows[0], { key: "ArrowDown" })
    fireEvent.keyDown(screen.getAllByRole("listitem")[1], { key: "l" })
    expect(within(screen.getAllByRole("listitem")[0]).getByRole("button", { name: /Écart mesuré/ })).toHaveAttribute("aria-pressed", "true")
    const save = screen.getByRole("button", { name: "Enregistrer 2 décisions" })
    fireEvent.click(save)
    await waitFor(() => expect(mock.to("PUT", "/api/splits/1/worklist")).toHaveLength(1))
    expect(mock.to("PUT", "/api/splits/1/worklist")[0].body).toEqual({
      decisions: [
        { passage_id: "P-1", action: "assign", targets: ["C-0008", "C-0009"] },
        { passage_id: "P-2", action: "leave" },
      ],
    })
  })

  it("explains an undone split", async () => {
    mock.on("GET", "/api/splits/1/worklist", { body: worklist([item("P-1")], { undone: { rationale: "x", at: "2026-10-13T10:00:00Z", by: "moi" } }) })
    renderWithClient(<WorklistDialog opId={1} onClose={() => {}} />)
    expect(await screen.findByText(/Cette scission a été annulée/)).toBeInTheDocument()
  })
})

describe("journal", () => {
  const op = (over = {}) => ({ op_id: 2, kind: "split", code_id: "C-0001", code_name: "Stock", other_ids: ["C-0008", "C-0009"], rationale: "Deux idées", created_at: "2026-10-13T10:00:00Z", created_by: "moi", undone: null, summary: { worklist: 4 }, ...over })

  it("undoes with a reason and says in plain French why it is refused", async () => {
    mock.on("GET", "/api/code-ops?code=C-0001", { body: { operations: [op()] } })
    mock.on("POST", "/api/code-ops/2/undo", { status: 409, body: { error: "operation 5 came after and touches the same codes: undo it first", kind: "guard" } })
    renderWithClient(<CodeJournal code="C-0001" onOpenWorklist={() => {}} />)
    fireEvent.click(await screen.findByRole("button", { name: /Annuler cette scission/ }))
    const dialog = await screen.findByRole("dialog")
    const go = within(dialog).getByRole("button", { name: "Annuler l'opération" })
    expect(go).toBeDisabled()
    fireEvent.change(within(dialog).getByLabelText("Motif de l'annulation, obligatoire"), { target: { value: "Erreur de découpage" } })
    fireEvent.click(go)
    expect(await within(dialog).findByText(/L'opération n° 5, plus récente, touche les mêmes codes/)).toBeInTheDocument()
    expect(mock.to("POST", "/api/code-ops/2/undo")[0].body).toEqual({ rationale: "Erreur de découpage" })
  })

  it("shows what was undone and opens the worklist of a split", async () => {
    mock.on("GET", "/api/code-ops?code=C-0001", { body: { operations: [op({ undone: { rationale: "Erreur", at: "2026-10-13T11:00:00Z", by: "moi" } })] } })
    let opened = 0
    renderWithClient(<CodeJournal code="C-0001" onOpenWorklist={(id) => (opened = id)} />)
    expect(await screen.findByText(/Annulée le/)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /Annuler cette scission/ })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Voir la liste" }))
    expect(opened).toBe(2)
  })
})
