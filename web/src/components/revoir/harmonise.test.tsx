import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { Harmonise } from "@/components/revoir/harmonise"
import type { HarmonisationItem } from "@/lib/review-api"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

const ID = "INT-01"
const W = `/api/interviews/${ID}/harmonisation`

const item = (n: number, over: Partial<HarmonisationItem> = {}): HarmonisationItem => ({
  item_id: n,
  interview_id: ID,
  unit_id: `${ID}-U00${n}`,
  text: `Texte du tour ${n}`,
  codes: [{ token: "READER_COST", code_id: "C-0002", name: "Coût des lecteurs" }],
  code_id: "C-0002",
  code_name: "Coût des lecteurs",
  cause: "merge",
  detail: "Fusion de codes dans le Codebook.",
  created_at: "2026-10-13T10:00:00Z",
  created_by: "alex",
  status: "open",
  resolved_at: null,
  resolved_by: null,
  resolution: null,
  note: null,
  ...over,
})

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
})
afterEach(cleanup)

describe("Harmoniser", () => {
  it("explains an empty list", async () => {
    mock.on("GET", W, { body: { n_open: 0, n_done: 0, items: [] } })
    renderWithClient(<Harmonise id={ID} />)
    expect(await screen.findByText("Rien à harmoniser pour cet entretien")).toBeInTheDocument()
  })

  it("lists the units to reread with their cause and current codes", async () => {
    mock.on("GET", W, { body: { n_open: 2, n_done: 0, items: [item(2), item(4, { cause: "new_subcode" })] } })
    renderWithClient(<Harmonise id={ID} />)
    const a = within(await screen.findByRole("article", { name: "Unité U002, Coût des lecteurs" }))
    expect(a.getByText("Fusion de codes")).toBeInTheDocument()
    expect(a.getByText("Texte du tour 2")).toBeInTheDocument()
    expect(a.getByText("Coût des lecteurs", { selector: ".tag" })).toBeInTheDocument()
    expect(screen.getByText("Nouveau sous-code")).toBeInTheDocument()
    expect(screen.getByRole("status")).toHaveTextContent("2 unités à relire")
  })

  it("confirms the coding with a note", async () => {
    mock.on("GET", W, { body: { n_open: 1, n_done: 0, items: [item(2)] } })
    mock.on("PUT", `${W}/2`, { body: item(2, { status: "done", resolution: "confirmed" }) })
    renderWithClient(<Harmonise id={ID} />)
    const a = within(await screen.findByRole("article", { name: /Unité U002/ }))
    fireEvent.change(a.getByLabelText("Note, facultative"), { target: { value: "reste juste" } })
    fireEvent.click(a.getByRole("button", { name: "Confirmer le codage" }))
    await waitFor(() => expect(mock.to("PUT", `${W}/2`)).toHaveLength(1))
    expect(mock.to("PUT", `${W}/2`)[0].body).toEqual({ resolution: "confirmed", note: "reste juste" })
  })

  it("records a correction made in the Coder", async () => {
    mock.on("GET", W, { body: { n_open: 1, n_done: 0, items: [item(2)] } })
    mock.on("PUT", `${W}/2`, { body: item(2, { status: "done", resolution: "corrected" }) })
    renderWithClient(<Harmonise id={ID} />)
    fireEvent.click(await screen.findByRole("button", { name: "J'ai corrigé le codage" }))
    await waitFor(() => expect(mock.to("PUT", `${W}/2`)).toHaveLength(1))
    expect(mock.to("PUT", `${W}/2`)[0].body).toEqual({ resolution: "corrected" })
  })

  it("shows a treated unit under Traitées with its resolution", async () => {
    mock.on("GET", W, { body: { n_open: 0, n_done: 1, items: [item(2, { status: "done", resolution: "corrected", note: "recodé", resolved_at: "2026-10-13T11:00:00Z", resolved_by: "alex" })] } })
    renderWithClient(<Harmonise id={ID} />)
    expect(await screen.findByText("Toute la liste est traitée.", { exact: false })).toBeInTheDocument()
    expect(screen.getByText("Rien à relire.")).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Traitées" }))
    expect(await screen.findByText("Codage corrigé")).toBeInTheDocument()
    expect(screen.getByText(/recodé/)).toBeInTheDocument()
  })
})
