import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { PersonsPage } from "@/pages/persons"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

const person = (over: Record<string, unknown> = {}) => ({
  person_id: "P-0001",
  kind: "person",
  name: "Anna Roth",
  organisation: "Lectis",
  function: "Directrice",
  email: "",
  phone: "",
  notes: "",
  consent_status: "given",
  consent_date: "2026-03-02",
  consent_document: "",
  anonymise: true,
  pseudonym: "Répondant 01",
  aliases: ["Anna"],
  attributes: {},
  group_ids: ["G-0002"],
  created_at: "",
  updated_at: "",
  ...over,
})

const node = (over: Record<string, unknown>) => ({ description: "", created_at: "", updated_at: "", n_interviews_total: 0, children: [], ...over })

const TREE = {
  groups: [
    node({
      group_id: "G-0001",
      name: "Détaillants",
      parent_id: null,
      path: "Détaillants",
      depth: 0,
      n_persons: 0,
      n_persons_total: 2,
      children: [node({ group_id: "G-0002", name: "Suisse romande", parent_id: "G-0001", path: "Détaillants/Suisse romande", depth: 1, n_persons: 2, n_persons_total: 2 })],
    }),
  ],
  n_groups: 2,
  n_persons: 3,
  n_ungrouped_persons: 1,
}

const LIST = {
  n: 3,
  persons: [
    { ...person(), n_interviews: 1 },
    { ...person({ person_id: "P-0002", name: "Marc Bovet", organisation: "", function: "", anonymise: false, pseudonym: "Répondant 02", aliases: [], consent_status: "", group_ids: ["G-0002"] }), n_interviews: 0 },
    { ...person({ person_id: "P-0003", kind: "organisation", name: "Lectis", pseudonym: "Organisation 01", aliases: [], group_ids: [] }), n_interviews: 1 },
  ],
}

const DETAIL = {
  ...person(),
  groups: [{ group_id: "G-0002", name: "Suisse romande", path: "Détaillants/Suisse romande" }],
  interviews: [{ interview_id: "INT-01", role: "respondent", folder_name: "INT-01_Example", attributes: {} }],
  attribute_definitions: [],
  interview_attribute_definitions: [],
}

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock
    .on("GET", "/api/person-groups", { body: TREE })
    .on("GET", /^\/api\/persons(\?.*)?$/, { body: LIST })
    .on("GET", "/api/persons/P-0001", { body: DETAIL })
    .on("GET", "/api/interviews", { body: [{ interview_id: "INT-01", folder_name: "INT-01_Example" }, { interview_id: "INT-02", folder_name: "INT-02_Exemple" }] })
    .on("GET", "/api/verify", { body: { ok: true, n_events: 1 } })
})
afterEach(cleanup)

describe("Personnes", () => {
  it("shows the tree with its roll-up counts, the list and the fiche of the first record", async () => {
    renderWithClient(<PersonsPage />)
    const tree = await screen.findByRole("table", { name: "Arbre des groupes" })
    expect(within(tree).getByText("Détaillants")).toBeInTheDocument()
    expect(within(tree).getByText("Sans groupe")).toBeInTheDocument()
    const list = await screen.findByRole("table", { name: "Liste des personnes" })
    expect(within(list).getByText("Marc Bovet")).toBeInTheDocument()
    expect(within(list).getByText("Non anonymisée")).toBeInTheDocument()
    expect(await screen.findByLabelText("Nom")).toHaveValue("Anna Roth")
    expect(screen.getByRole("switch", { name: "Anonymiser" })).toHaveAttribute("aria-checked", "true")
    expect(screen.getByRole("link", { name: "Ouvrir l'entretien INT-01" })).toHaveAttribute("href", "#/interviews/INT-01/sources")
  })

  it("filters by the selected group and its sub-groups", async () => {
    renderWithClient(<PersonsPage />)
    const tree = await screen.findByRole("table", { name: "Arbre des groupes" })
    fireEvent.click(within(tree).getByText("Détaillants"))
    await waitFor(() => expect(mock.to("GET", "/api/persons?group_id=G-0001").length).toBeGreaterThan(0))
  })

  it("filters by anonymisation and by interview", async () => {
    renderWithClient(<PersonsPage />)
    await screen.findByRole("table", { name: "Liste des personnes" })
    fireEvent.change(screen.getByLabelText("Filtrer par anonymisation"), { target: { value: "no" } })
    await waitFor(() => expect(mock.to("GET", "/api/persons?anonymise=false").length).toBeGreaterThan(0))
    fireEvent.change(screen.getByLabelText("Filtrer par entretien"), { target: { value: "INT-02" } })
    await waitFor(() => expect(mock.to("GET", "/api/persons?interview_id=INT-02&anonymise=false").length).toBeGreaterThan(0))
  })

  it("turns the anonymisation off from the fiche", async () => {
    mock.on("PUT", "/api/persons/P-0001", { body: person({ anonymise: false }) })
    renderWithClient(<PersonsPage />)
    fireEvent.click(await screen.findByRole("switch", { name: "Anonymiser" }))
    await waitFor(() => expect(mock.to("PUT", "/api/persons/P-0001")).toHaveLength(1))
    expect(mock.to("PUT", "/api/persons/P-0001")[0].body).toEqual({ anonymise: false })
  })

  it("adds an alias and explains a name conflict in plain words", async () => {
    mock.on("PUT", "/api/persons/P-0001", {
      status: 400,
      body: { error: 'the name "Bovet" is used by P-0001 and by P-0002', kind: "validation" },
    })
    renderWithClient(<PersonsPage />)
    fireEvent.change(await screen.findByPlaceholderText("Nouvel alias"), { target: { value: "Bovet" } })
    fireEvent.click(screen.getByRole("button", { name: "Ajouter" }))
    expect((await screen.findAllByRole("alert"))[0]).toHaveTextContent("« Bovet » est déjà pris")
    expect(mock.to("PUT", "/api/persons/P-0001")[0].body).toEqual({ aliases: ["Anna", "Bovet"] })
  })

  it("explains a group created twice", async () => {
    mock.on("POST", "/api/person-groups", { status: 400, body: { error: 'a group named "Détaillants" already exists at this level', kind: "validation" } })
    renderWithClient(<PersonsPage />)
    fireEvent.click(await screen.findByRole("button", { name: "+ Groupe" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.change(within(dialog).getByLabelText("Nom du groupe"), { target: { value: "Détaillants" } })
    fireEvent.click(within(dialog).getByRole("button", { name: "Créer le groupe" }))
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("existe déjà à ce niveau")
  })

  it("asks for a group to take over a non-empty group before deleting it", async () => {
    mock.on("DELETE", "/api/person-groups/G-0001?reassign_to=G-0002", { body: { deleted: "G-0001" } })
    renderWithClient(<PersonsPage />)
    const tree = await screen.findByRole("table", { name: "Arbre des groupes" })
    fireEvent.click(within(tree).getByText("Détaillants"))
    fireEvent.click(screen.getByRole("button", { name: "Supprimer" }))
    const dialog = await screen.findByRole("dialog")
    const confirm = within(dialog).getByRole("button", { name: "Supprimer le groupe" })
    expect(confirm).toBeDisabled()
    // The group itself and its sub-group below it cannot take the members over: nothing to choose here.
    expect(within(dialog).getByText(/Aucun autre groupe ne peut les reprendre/)).toBeInTheDocument()
  })

  it("moves a group by dragging it onto another", async () => {
    mock.on("PUT", "/api/person-groups/G-0002", { body: {} })
    renderWithClient(<PersonsPage />)
    const tree = await screen.findByRole("table", { name: "Arbre des groupes" })
    const child = within(tree).getByText("Suisse romande").closest("tr")!
    const root = within(tree).getByText("Toutes les personnes").closest("tr")!
    fireEvent.dragStart(child, { dataTransfer: { setData: () => undefined } })
    fireEvent.dragOver(root)
    fireEvent.drop(root)
    await waitFor(() => expect(mock.to("PUT", "/api/person-groups/G-0002")).toHaveLength(1))
    expect(mock.to("PUT", "/api/person-groups/G-0002")[0].body).toEqual({ parent_id: null })
  })
})
