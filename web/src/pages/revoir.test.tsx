import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { RevoirPage } from "@/pages/revoir"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { AiRun, ProviderEntry, PromptPart, ReviewAnswer, ReviewItem, Unit } from "@/lib/types"

const ID = "INT-01"
const I = `/api/interviews/${ID}`

const part = (n: number, of: number, batch: number): PromptPart => ({
  interview_id: ID,
  phase: 2,
  batch_id: batch,
  part: n,
  of,
  unit_ids: [`${ID}-U00${n}`],
  segmentation_hash: "h",
  codebook_hash: "cb1234567890",
  prompt_version: "CS-P04 v3.1",
  text: `prompt ${n}`,
  sha256: `sha${batch}${n}`.padEnd(64, "0"),
})

const meta = (label: string, extra: Partial<AiRun["meta"]> = {}): AiRun["meta"] => ({
  run_label: label,
  model_name: "modèle X",
  model_version: "",
  temperature: null,
  seed: null,
  context_length: null,
  batch_size: null,
  prompt_id_version: "",
  executed_at: new Date(2026, 9, 13, 9, 40).toISOString(),
  notes: null,
  batch_id: 2,
  ...extra,
})

const run = (over: Partial<AiRun> = {}): AiRun => ({
  run_id: 1,
  interview_id: ID,
  phase: 2,
  batch_id: 2,
  codings: {},
  parts_received: [1, 2],
  parts_expected: 2,
  complete: true,
  meta: meta("R1"),
  ...over,
})

const guard = (complete: boolean) => ({
  phase: 2,
  summary: { phase: 2, n_units: 2, n_human_coded: 2, human_complete: complete, n_ai_runs: 0, n_ai_runs_complete: 0, n_divergences: 0, n_arbitrated: 0, reconciled: false },
  guard: { phase: 2, complete, n_units: 2, missing: complete ? [] : [`${ID}-U002`] },
})

const entry = (id: string, kind: ProviderEntry["kind"], execution: ProviderEntry["description"]["execution"]): ProviderEntry => ({
  id,
  kind,
  description: { id, kind, sends: kind === "copy_paste" ? [] : ["prompt"], execution },
  configured: true,
  config: null,
})
const REGISTRY: ProviderEntry[] = [
  entry("copy_paste", "copy_paste", { kind: "local" }),
  entry("claude_cli", "cli", { kind: "hosted", vendor: "anthropic", endpoint: "https://api.anthropic.com" }),
]

const unit = (seq: number, role: Unit["role"], text: string): Unit => ({
  unit_id: `${ID}-U${String(seq).padStart(3, "0")}`,
  seq,
  speaker_raw: role === "interviewer" ? "INT" : "RESP",
  role,
  timestamp: "00:0" + seq,
  text,
})

const item = (seq: number, over: Partial<ReviewItem> = {}): ReviewItem => ({
  unit_id: `${ID}-U${String(seq).padStart(3, "0")}`,
  kind: "disagreement",
  human: ["COST"],
  human_current: ["COST"],
  revised_since_snapshot: false,
  ai: { R1: ["SKILLS"] },
  decision: null,
  unit: unit(seq, "respondent", `Réponse numéro ${seq}`),
  ...over,
})

const reviewAnswer = (items: ReviewItem[]): ReviewAnswer => ({
  runs: ["R1"],
  items,
  n_disagreements: items.filter((i) => i.kind === "disagreement").length,
  n_sampled: 0,
  n_decided: items.filter((i) => i.decision).length,
  seeds: [],
  snapshots: [{ run_label: "R1", snapshot_id: 4, sha256: "abcdef0123456789" }],
})

let mock: MockApi
let runs: AiRun[]
let review: ReviewAnswer

beforeEach(() => {
  runs = []
  review = { runs: [], items: [], n_disagreements: 0, n_sampled: 0, n_decided: 0, seeds: [], snapshots: [] }
  mock = mockApi()
  mock.on("GET", `${I}/phases/2`, () => ({ body: guard(true) }))
  mock.on("GET", `${I}/phases/2/prompts`, { body: [part(1, 1, 1), part(1, 2, 2), part(2, 2, 2)] })
  mock.on("GET", `${I}/phases/2/runs`, () => ({ body: runs }))
  mock.on("GET", `${I}/review`, () => ({ body: review }))
  mock.on("GET", "/api/providers", { body: REGISTRY })
  mock.on("GET", `${I}/snapshots`, { body: [{ snapshot_id: 4, interview_id: ID, batch_id: 2, phase: 2, segmentation_hash: "", codebook_hash: "", codings: {}, sha256: "", created_at: new Date(2026, 9, 13, 9, 12).toISOString() }] })
  mock.on("GET", `${I}/phases/1/units`, {
    body: [unit(1, "interviewer", "Combien de temps a pris le déploiement ?"), unit(2, "respondent", "Réponse numéro 2")].map((u) => ({ unit: u, coding: null, allowed: [] })),
  })
  mock.on("GET", "/api/codebook", { body: { codes: [] } })
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const change = (el: HTMLElement, value: string) => fireEvent.change(el, { target: { value } })

describe("Revue IA, generate the prompt", () => {
  it("warns that the human coding is frozen at that moment", async () => {
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByText(/fige votre codage de cet entretien/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Générer le prompt" })).toBeEnabled()
  })

  it("is disabled while relevant turns are not coded, and says which", async () => {
    mock.on("GET", `${I}/phases/2`, () => ({ body: guard(false) }))
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByRole("button", { name: "Générer le prompt" })).toBeDisabled()
    expect(screen.getByText(/1 tour pertinent pas encore codé/)).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "Continuer le codage" })).toBeInTheDocument()
  })

  it("lists the parts of the latest batch only, and counts the copies", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    const copy1 = await screen.findByRole("button", { name: "Copier la partie 1 sur 2" })
    expect(screen.getByRole("button", { name: "Copier la partie 2 sur 2" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Copier la partie 1 sur 1" })).toBeNull()
    expect(screen.getByRole("status")).toHaveTextContent("Aucune partie copiée.")
    fireEvent.click(copy1)
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("1 sur 2 parties copiées"))
    expect(writeText).toHaveBeenCalledWith("prompt 1")
    expect(screen.getByText(/13\.10 à 09:12/)).toBeInTheDocument()
  })

  it("generates with the batch size and shows the new parts", async () => {
    let made: PromptPart[] = []
    mock.on("GET", `${I}/phases/2/prompts`, () => ({ body: made }))
    mock.on("POST", `${I}/phases/2/prompts`, () => {
      made = [part(1, 1, 3)]
      return { body: made }
    })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    change(await screen.findByLabelText("Unités par partie"), "30")
    fireEvent.click(screen.getByRole("button", { name: "Générer le prompt" }))
    expect(await screen.findByRole("button", { name: "Copier la partie 1 sur 1" })).toBeInTheDocument()
    expect(mock.to("POST", `${I}/phases/2/prompts`)[0].body).toEqual({ batch_size: 30 })
  })
})

const PSEUDO = (over: Record<string, unknown> = {}) => ({
  enabled: true,
  digest: "d".repeat(64),
  categories: ["person"],
  n_list_entries: 2,
  entries: [{ kind: "person", original: "Anna Keller", pseudonym: "Personne 1", count: 3 }],
  free_text_covered: false,
  note: "La liste ne couvre pas le texte libre.",
  ...over,
})

describe("Revue IA, pseudonymisation", () => {
  it("says that the pseudonymisation is inactive", async () => {
    mock.on("GET", `${I}/pseudonymisation`, { body: PSEUDO({ enabled: false, entries: [] }) })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByText(/Pseudonymisation inactive : les noms partent tels quels/)).toBeInTheDocument()
  })

  it("lists the replaced names with their number of replacements", async () => {
    mock.on("GET", `${I}/pseudonymisation`, { body: PSEUDO() })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByText(/Pseudonymisation active : 1 nom remplacé avant envoi/)).toBeInTheDocument()
    expect(screen.getByText(/3 remplacements/)).toBeInTheDocument()
    expect(screen.getByText(/La liste ne couvre pas le texte libre/)).toBeInTheDocument()
  })
})

describe("Revue IA, answer import", () => {
  const groupOf = async (n: number) => within(await screen.findByRole("group", { name: `Réponse à la partie ${n} sur 2` }))

  it("needs the model name before importing", async () => {
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    const g = await groupOf(1)
    change(g.getByRole("textbox"), '{"codings":{}}')
    expect(g.getByRole("button", { name: "Importer la réponse" })).toBeDisabled()
    expect(screen.getByText(/Nommez le modèle utilisé/)).toBeInTheDocument()
  })

  it("records the run with the model, then imports the part", async () => {
    mock.on("POST", `${I}/phases/2/runs`, { body: run({ complete: false, parts_received: [], run_id: 7 }) })
    mock.on("POST", `${I}/phases/2/runs/7/outputs`, { body: run({ complete: false, parts_received: [1], run_id: 7 }) })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    change(await screen.findByLabelText(/^Modèle utilisé/), "claude-opus-5-5")
    const g = await groupOf(1)
    change(g.getByRole("textbox"), '{"codings":{}}')
    fireEvent.click(g.getByRole("button", { name: "Importer la réponse" }))
    await waitFor(() => expect(mock.to("POST", `${I}/phases/2/runs/7/outputs`)).toHaveLength(1))
    const created = mock.to("POST", `${I}/phases/2/runs`)[0].body as { model_name: string; run_label: string; batch_id: number }
    expect(created).toMatchObject({ model_name: "claude-opus-5-5", run_label: "R1", batch_id: 2 })
    expect(mock.to("POST", `${I}/phases/2/runs/7/outputs`)[0].body).toEqual({ raw: '{"codings":{}}', part: 1, of: 2 })
  })

  it("lists every validation problem and keeps the pasted text", async () => {
    mock.on("POST", `${I}/phases/2/runs`, { body: run({ complete: false, parts_received: [], run_id: 7 }) })
    mock.on("POST", `${I}/phases/2/runs/7/outputs`, {
      status: 400,
      body: {
        error: "the AI answer was rejected: 2 problem(s) found",
        kind: "validation",
        issues: [
          { location: "$.codings", message: "unit INT-01-U002 is missing" },
          { location: "$.codings.INT-01-U001", message: "unknown code ZZZ" },
        ],
      },
    })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    change(await screen.findByLabelText(/^Modèle utilisé/), "m")
    const g = await groupOf(1)
    change(g.getByRole("textbox"), "mauvaise réponse")
    fireEvent.click(g.getByRole("button", { name: "Importer la réponse" }))
    expect(await g.findByText("Réponse refusée : 2 problèmes. Rien n'a été importé.")).toBeInTheDocument()
    expect(g.getByText("unit INT-01-U002 is missing")).toBeInTheDocument()
    expect(g.getByText("unknown code ZZZ")).toBeInTheDocument()
    expect(g.getByRole("textbox")).toHaveValue("mauvaise réponse")
  })

  it("reuses the run left waiting instead of creating another", async () => {
    runs = [run({ complete: false, parts_received: [1], run_id: 7 })]
    mock.on("POST", `${I}/phases/2/runs/7/outputs`, { body: run({ run_id: 7 }) })
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    const g2 = await groupOf(2)
    change(g2.getByRole("textbox"), "{}")
    fireEvent.click(g2.getByRole("button", { name: "Importer la réponse" }))
    await waitFor(() => expect(mock.to("POST", `${I}/phases/2/runs/7/outputs`)).toHaveLength(1))
    expect(mock.to("POST", `${I}/phases/2/runs`)).toHaveLength(0)
    expect((await groupOf(1)).getByText("Importée.")).toBeInTheDocument()
  })

  it("says a complete run is enough, and offers another one on request", async () => {
    runs = [run()]
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByText(/Un run complet suffit/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Importer un autre run" }))
    expect(await screen.findByLabelText(/^Modèle utilisé/)).toBeInTheDocument()
  })
})

describe("Revue IA, run list", () => {
  it("shows a complete, a partial and a waiting run", async () => {
    runs = [
      run({ run_id: 1 }),
      run({ run_id: 2, complete: false, parts_received: [1], meta: meta("R2", { model_name: "autre" }) }),
      run({ run_id: 3, complete: false, parts_received: [], meta: meta("R3", { model_name: "vide" }) }),
    ]
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    const list = within(await screen.findByRole("region", { name: "Runs enregistrés" }))
    expect(list.getByText("R1, 13.10 09:40, modèle modèle X, 2 parties sur 2, complet")).toBeInTheDocument()
    expect(list.getByText("R2, 13.10 09:40, modèle autre, 1 partie sur 2, partiel, 1 partie sur 2")).toBeInTheDocument()
    expect(list.getByText("R3, 13.10 09:40, modèle vide, 0 partie sur 2, en attente de réponses")).toBeInTheDocument()
    // Only the complete run leads to the arbitration.
    expect(list.getAllByRole("link", { name: "Arbitrer" })).toHaveLength(1)
  })

  it("is empty before any run", async () => {
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByText("Aucun run pour l'instant.")).toBeInTheDocument()
  })

  it("states the review in the toolbar title", async () => {
    runs = [run()]
    review = reviewAnswer([item(2), item(3, { decision: { unit_id: `${ID}-U003`, phase: 2, choice: { type: "human" }, final_codes: ["COST"], rationale: "r", decided_by: "me", decided_at: "" } })])
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    expect(await screen.findByText("Revue · revue IA faite, 1 sur 2 décidées")).toBeInTheDocument()
  })
})

describe("Revue IA, provider run", () => {
  const RUN_URI = `${I}/batches/2/run`
  async function section() {
    renderWithClient(<RevoirPage id={ID} part="ai" />)
    return within(await screen.findByRole("region", { name: "Lancer avec un fournisseur" }))
  }

  it("keeps copy and paste first and offers no run with it", async () => {
    const s = await section()
    const select = (await s.findByLabelText(/^Fournisseur/)) as HTMLSelectElement
    await waitFor(() => expect(within(select).getAllByRole("option")).toHaveLength(2))
    expect(select.value).toBe("copy_paste")
    expect(s.queryByRole("button", { name: "Lancer" })).toBeNull()
  })

  it("runs the latest batch and shows the journal line", async () => {
    mock.on("POST", RUN_URI, () => {
      runs = [run()]
      return { body: { run: run(), journal_line: "command local, model hosted" } }
    })
    const s = await section()
    await waitFor(() => expect(within(s.getByLabelText(/^Fournisseur/)).getAllByRole("option")).toHaveLength(2))
    change(s.getByLabelText(/^Fournisseur/), "claude_cli")
    expect(s.getByText("command local, model hosted")).toBeInTheDocument()
    fireEvent.click(s.getByRole("button", { name: "Lancer" }))
    expect(await s.findByText(/Run R1 complet : 2 parties sur 2 importées/)).toBeInTheDocument()
    expect(mock.to("POST", RUN_URI)[0].body).toEqual({ provider_id: "claude_cli", run_label: "R1" })
    expect(mock.to("POST", /\/batches\/1\/run$/)).toHaveLength(0)
  })

  it("shows the refusal with the raw answer", async () => {
    mock.on("POST", RUN_URI, {
      status: 400,
      body: { error: "rejected", kind: "validation", issues: [{ location: "$", message: "no JSON object found" }], rejected: [{ part: 1, raw: "Désolé, je ne peux pas." }] },
    })
    const s = await section()
    await waitFor(() => expect(within(s.getByLabelText(/^Fournisseur/)).getAllByRole("option")).toHaveLength(2))
    change(s.getByLabelText(/^Fournisseur/), "claude_cli")
    fireEvent.click(s.getByRole("button", { name: "Lancer" }))
    expect(await s.findByText("Réponse refusée : 1 problème. Rien n'a été importé.")).toBeInTheDocument()
    fireEvent.click(s.getByRole("button", { name: "Afficher la réponse refusée de la partie 1" }))
    expect(s.getByText("Désolé, je ne peux pas.")).toBeInTheDocument()
  })
})

describe("Décisions, without review", () => {
  it("explains that the review is optional and links to the guide", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    expect(await screen.findByText("La revue IA est facultative")).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "Lire la section Revoir du guide" })).toHaveAttribute("href", "#/guide/revoir")
    expect(screen.getByRole("link", { name: "Préparer une revue IA" })).toHaveAttribute("href", "#/interviews/INT-01/ai")
  })

  it("says so when a run exists but is not complete", async () => {
    runs = [run({ complete: false, parts_received: [1] })]
    renderWithClient(<RevoirPage id={ID} part="review" />)
    expect(await screen.findByText("Aucun run complet pour l'instant")).toBeInTheDocument()
  })
})

describe("Décisions", () => {
  beforeEach(() => {
    runs = [run()]
    review = reviewAnswer([item(2), item(5)])
    mock.on("GET", `${I}/phases/1/units`, {
      body: [unit(1, "interviewer", "Combien de temps a pris le déploiement ?"), unit(2, "respondent", "Réponse numéro 2"), unit(5, "respondent", "Réponse numéro 5")].map((u) => ({ unit: u, coding: null, allowed: [] })),
    })
  })

  const card = async (name = "Unité U002") => within(await screen.findByRole("article", { name }))

  it("shows the unit with the interviewer question, and both readings", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    expect(c.getByText("Réponse numéro 2")).toBeInTheDocument()
    expect(await c.findByText(/Après l'intervieweur/)).toHaveTextContent("« Combien de temps a pris le déploiement ? »")
    const table = within(c.getByRole("table"))
    expect(table.getByText("COST")).toBeInTheDocument()
    expect(table.getByText("SKILLS")).toBeInTheDocument()
    expect(c.getByText("vous seul")).toBeInTheDocument()
    expect(c.getByText("modèle seul")).toBeInTheDocument()
    expect(c.getByText(/désaccord 1 sur 2/)).toBeInTheDocument()
  })

  it("never offers an interviewer turn in the queue", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    await card()
    const queue = within(screen.getByRole("complementary", { name: "File d'arbitrage" }))
    expect(queue.getAllByRole("button", { name: /^U\d+/ }).map((b) => b.textContent)).toEqual(["U002à décider", "U005à décider"])
  })

  it("chooses with the keys 1, 2 and 3", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    const radios = () => c.getAllByRole("radio") as HTMLInputElement[]
    expect(radios().map((r) => r.checked)).toEqual([true, false, false, false])
    fireEvent.keyDown(window, { key: "2" })
    expect(radios().map((r) => r.checked)).toEqual([false, true, false, false])
    fireEvent.keyDown(window, { key: "3" })
    expect(radios().map((r) => r.checked)).toEqual([false, false, true, false])
    fireEvent.keyDown(window, { key: "1" })
    expect(radios()[0].checked).toBe(true)
  })

  it("does not take the number keys while the reason is typed", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    fireEvent.keyDown(c.getByRole("textbox"), { key: "2" })
    expect((c.getAllByRole("radio") as HTMLInputElement[])[0].checked).toBe(true)
  })

  it("refuses to save a disagreement without a reason, and says why", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    fireEvent.keyDown(window, { key: "Enter" })
    expect(await c.findByRole("alert")).toHaveTextContent("Une justification est obligatoire")
    expect(mock.to("PUT", `${I}/review/${ID}-U002`)).toHaveLength(0)
  })

  it("requires the reason even to keep the human reading of a disagreement", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    expect(c.getByText("Justification, obligatoire pour un désaccord")).toBeInTheDocument()
    fireEvent.click(c.getByRole("button", { name: "Enregistrer et suivant" }))
    expect(await c.findByRole("alert")).toBeInTheDocument()
    expect(mock.to("PUT", /\/review\//)).toHaveLength(0)
  })

  it("saves the model's reading with its reason on Enter, then moves to the next card", async () => {
    mock.on("PUT", `${I}/review/${ID}-U002`, { body: { unit_id: `${ID}-U002`, phase: 2, choice: { type: "ai", run_label: "R1" }, final_codes: ["SKILLS"], rationale: "x", decided_by: "me", decided_at: "" } })
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    fireEvent.keyDown(window, { key: "2" })
    change(c.getByRole("textbox"), "  le modèle a raison  ")
    fireEvent.keyDown(c.getByRole("textbox"), { key: "Enter", ctrlKey: true })
    await waitFor(() => expect(mock.to("PUT", `${I}/review/${ID}-U002`)).toHaveLength(1))
    expect(mock.to("PUT", `${I}/review/${ID}-U002`)[0].body).toEqual({ choice: { type: "ai", run_label: "R1" }, final_codes: ["SKILLS"], rationale: "le modèle a raison" })
    expect(await screen.findByRole("article", { name: "Unité U005" })).toBeInTheDocument()
  })

  it("merges the two readings as a custom choice", async () => {
    mock.on("PUT", `${I}/review/${ID}-U002`, { body: {} })
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    fireEvent.keyDown(window, { key: "3" })
    change(c.getByRole("textbox"), "les deux")
    fireEvent.keyDown(window, { key: "Enter" })
    await waitFor(() => expect(mock.to("PUT", `${I}/review/${ID}-U002`)).toHaveLength(1))
    const body = mock.to("PUT", `${I}/review/${ID}-U002`)[0].body as { choice: unknown; final_codes: string[] }
    expect(body.choice).toEqual({ type: "custom" })
    expect([...body.final_codes].sort()).toEqual(["COST", "SKILLS"])
  })

  it("filters the queue between remaining and decided", async () => {
    review = reviewAnswer([item(2, { decision: { unit_id: `${ID}-U002`, phase: 2, choice: { type: "human" }, final_codes: ["COST"], rationale: "r", decided_by: "me", decided_at: "" } }), item(5)])
    renderWithClient(<RevoirPage id={ID} part="review" />)
    await card("Unité U005")
    const queue = within(screen.getByRole("complementary", { name: "File d'arbitrage" }))
    expect(queue.queryByRole("button", { name: /^U002/ })).toBeNull()
    fireEvent.click(queue.getByRole("button", { name: "Décidés" }))
    expect(queue.getByRole("button", { name: /^U002/ })).toBeInTheDocument()
    expect(queue.queryByRole("button", { name: /^U005/ })).toBeNull()
    expect(queue.getByText("1 / 2")).toBeInTheDocument()
  })

  it("confirms a sampled agreement without a reason", async () => {
    review = reviewAnswer([item(2, { kind: "agreement_sample", ai: { R1: ["COST"] } })])
    mock.on("PUT", `${I}/review/${ID}-U002`, { body: {} })
    renderWithClient(<RevoirPage id={ID} part="review" />)
    const c = await card()
    expect(c.getByText("Justification, facultative")).toBeInTheDocument()
    fireEvent.keyDown(window, { key: "Enter" })
    await waitFor(() => expect(mock.to("PUT", `${I}/review/${ID}-U002`)).toHaveLength(1))
    expect(mock.to("PUT", `${I}/review/${ID}-U002`)[0].body).toEqual({ choice: { type: "human" }, final_codes: ["COST"], rationale: "" })
  })
})

describe("Décisions, candidate concepts", () => {
  const CAND = {
    candidate_id: "c1",
    interview_id: ID,
    label: "Formation interne",
    description: "après le départ de l'intégrateur",
    run_label: "R1",
    unit_ids: [`${ID}-U002`],
    evidence: [{ unit_id: `${ID}-U002`, quote: "numéro" }],
    decision: null,
    history: [],
  }
  beforeEach(() => {
    mock.on("GET", `${I}/concept-candidates`, { body: [] })
    runs = [run()]
    review = reviewAnswer([item(2)])
    mock.on("GET", "/api/codes", {
      body: { codes: [{ code_id: "SKILLS", name: "Skills capacity", active: true, merged_into: null, system: false }], codebook_version: 1 },
    })
  })

  it("shows no section when the model proposed nothing", async () => {
    renderWithClient(<RevoirPage id={ID} part="review" />)
    await screen.findByRole("article", { name: "Unité U002" })
    expect(screen.queryByText("Concepts hors codebook")).toBeNull()
  })

  it("lists them in the queue and shows the motivating unit", async () => {
    mock.on("GET", `${I}/concept-candidates`, { body: [CAND] })
    renderWithClient(<RevoirPage id={ID} part="review" />)
    fireEvent.click(await screen.findByRole("button", { name: /Formation interne/ }))
    const c = within(await screen.findByRole("article", { name: "Concept Formation interne" }))
    expect(c.getByText(/Jamais appliqué sans décision motivée/)).toBeInTheDocument()
    expect(c.getByText("Réponse", { exact: false })).toBeInTheDocument()
    expect(c.getByText("numéro").tagName).toBe("MARK")
  })

  async function open() {
    mock.on("GET", `${I}/concept-candidates`, { body: [CAND] })
    renderWithClient(<RevoirPage id={ID} part="review" />)
    fireEvent.click(await screen.findByRole("button", { name: /Formation interne/ }))
    return within(await screen.findByRole("article", { name: "Concept Formation interne" }))
  }

  it("rejects only with a reason", async () => {
    mock.on("PUT", `${I}/concept-candidates/c1`, { body: CAND })
    const c = await open()
    fireEvent.click(c.getByRole("button", { name: "Écarter" }))
    fireEvent.click(c.getByRole("button", { name: "Enregistrer la décision" }))
    expect(await c.findByRole("alert")).toHaveTextContent("Une justification est obligatoire")
    expect(mock.to("PUT", `${I}/concept-candidates/c1`)).toHaveLength(0)
    change(c.getByRole("textbox"), "hors du sujet de l'étude")
    fireEvent.click(c.getByRole("button", { name: "Enregistrer la décision" }))
    await waitFor(() => expect(mock.to("PUT", `${I}/concept-candidates/c1`)).toHaveLength(1))
    expect(mock.to("PUT", `${I}/concept-candidates/c1`)[0].body).toEqual({ action: "reject", rationale: "hors du sujet de l'étude" })
  })

  it("accepts as a new code with its name", async () => {
    mock.on("PUT", `${I}/concept-candidates/c1`, { body: CAND })
    const c = await open()
    fireEvent.click(c.getByRole("button", { name: "Créer un code" }))
    change(c.getByLabelText("Nom du nouveau code"), "Formation interne")
    change(c.getByLabelText("Justification, obligatoire"), "revient dans trois unités")
    fireEvent.click(c.getByRole("button", { name: "Enregistrer la décision" }))
    await waitFor(() => expect(mock.to("PUT", `${I}/concept-candidates/c1`)).toHaveLength(1))
    expect(mock.to("PUT", `${I}/concept-candidates/c1`)[0].body).toEqual({ action: "create", new_code_name: "Formation interne", rationale: "revient dans trois unités" })
  })

  it("maps to an existing code, which must be chosen first", async () => {
    mock.on("PUT", `${I}/concept-candidates/c1`, { body: CAND })
    const c = await open()
    fireEvent.click(c.getByRole("button", { name: "Rattacher à un code existant" }))
    change(c.getByLabelText("Justification, obligatoire"), "même idée")
    fireEvent.click(c.getByRole("button", { name: "Enregistrer la décision" }))
    expect(await c.findByRole("alert")).toHaveTextContent("Choisissez le code")
    await waitFor(() => expect((c.getByLabelText("Code existant") as HTMLSelectElement).options.length).toBe(2))
    change(c.getByLabelText("Code existant"), "SKILLS")
    fireEvent.click(c.getByRole("button", { name: "Enregistrer la décision" }))
    await waitFor(() => expect(mock.to("PUT", `${I}/concept-candidates/c1`)).toHaveLength(1))
    expect(mock.to("PUT", `${I}/concept-candidates/c1`)[0].body).toEqual({ action: "map", code_id: "SKILLS", rationale: "même idée" })
  })

  const DECIDED = { action: "reject", code_id: null, rationale: "hors sujet", decided_at: "2026-10-13T10:00:00Z", decided_by: "alex" }

  it("shows a treated concept with its dated reason", async () => {
    mock.on("GET", `${I}/concept-candidates`, {
      body: [{ ...CAND, decision: DECIDED, history: [DECIDED] }],
    })
    renderWithClient(<RevoirPage id={ID} part="review" />)
    fireEvent.click(await screen.findByRole("button", { name: /Formation interne/ }))
    expect(await screen.findByText("Écarté")).toBeInTheDocument()
    expect(screen.getByText(/hors sujet/)).toBeInTheDocument()
  })
})
