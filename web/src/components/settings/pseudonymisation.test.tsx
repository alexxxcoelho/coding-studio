import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react"

import { PseudonymisationSection } from "@/components/settings/pseudonymisation"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

const view = (over: Record<string, unknown> = {}) => ({
  enabled: false,
  digest: "a".repeat(64),
  categories: ["person"],
  n_list_entries: 1,
  entries: [{ kind: "person", original: "Claire Vuilleumier", pseudonym: "Personne A" }],
  free_text_covered: false,
  note: "Elle ne couvre ni une autre forme d'un nom ni le texte libre.",
  ...over,
})

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/pseudonymisation", { body: view() })
})
afterEach(cleanup)

describe("Pseudonymisation section", () => {
  it("shows the switch, the list, the digest and the free text warning", async () => {
    renderWithClient(<PseudonymisationSection />)
    expect(await screen.findByText("Pseudonymisation inactive")).toBeInTheDocument()
    expect(screen.getByLabelText("Nom réel 1")).toHaveValue("Claire Vuilleumier")
    expect(screen.getByTestId("pseudo-digest")).toHaveTextContent("aaaaaaaaaaaaaaaa")
    expect(screen.getByText("Le texte libre n'est pas couvert.")).toBeInTheDocument()
  })

  it("switches it on without touching the list", async () => {
    mock.on("PUT", "/api/pseudonymisation", { body: view({ enabled: true }) })
    renderWithClient(<PseudonymisationSection />)
    fireEvent.click(await screen.findByRole("switch", { name: /Remplacer les noms/ }))
    await waitFor(() => expect(mock.to("PUT", "/api/pseudonymisation")).toHaveLength(1))
    expect(mock.to("PUT", "/api/pseudonymisation")[0].body).toEqual({ enabled: true })
    expect(await screen.findByText("Pseudonymisation active")).toBeInTheDocument()
  })

  it("adds a name and saves the whole list", async () => {
    mock.on("PUT", "/api/pseudonymisation", { body: view() })
    renderWithClient(<PseudonymisationSection />)
    await screen.findByLabelText("Nom réel 1")
    expect(screen.getByRole("button", { name: "Enregistrer la liste" })).toBeDisabled()
    fireEvent.click(screen.getByRole("button", { name: "Ajouter un nom" }))
    fireEvent.change(screen.getByLabelText("Type du nom 2"), { target: { value: "organisation" } })
    fireEvent.change(screen.getByLabelText("Nom réel 2"), { target: { value: "Lectis" } })
    fireEvent.change(screen.getByLabelText("Pseudonyme 2"), { target: { value: "Fournisseur A" } })
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer la liste" }))
    await waitFor(() => expect(mock.to("PUT", "/api/pseudonymisation")).toHaveLength(1))
    expect(mock.to("PUT", "/api/pseudonymisation")[0].body).toEqual({
      entries: [
        { kind: "person", original: "Claire Vuilleumier", pseudonym: "Personne A" },
        { kind: "organisation", original: "Lectis", pseudonym: "Fournisseur A" },
      ],
    })
  })

  it("refuses a duplicate before calling the server", async () => {
    renderWithClient(<PseudonymisationSection />)
    await screen.findByLabelText("Nom réel 1")
    fireEvent.click(screen.getByRole("button", { name: "Ajouter un nom" }))
    fireEvent.change(screen.getByLabelText("Nom réel 2"), { target: { value: "claire vuilleumier" } })
    fireEvent.change(screen.getByLabelText("Pseudonyme 2"), { target: { value: "Personne B" } })
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer la liste" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("figure deux fois")
    expect(mock.to("PUT", "/api/pseudonymisation")).toHaveLength(0)
  })

  it("shows the server refusal", async () => {
    mock.on("PUT", "/api/pseudonymisation", { status: 400, body: { error: "duplicate name", kind: "validation" } })
    renderWithClient(<PseudonymisationSection />)
    await screen.findByLabelText("Nom réel 1")
    fireEvent.change(screen.getByLabelText("Pseudonyme 1"), { target: { value: "Personne Z" } })
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer la liste" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("duplicate name")
  })

  it("lists the entries derived from the records apart, read-only, and sends only the manual ones", async () => {
    mock.on("GET", "/api/pseudonymisation", {
      body: view({
        enabled: true,
        enabled_manual: false,
        n_manual_entries: 1,
        n_person_entries: 2,
        n_list_entries: 3,
        shadowed_manual: [{ kind: "person", original: "Anna", pseudonym: "X", reason: "managed by P-0001" }],
        entries: [
          { kind: "person", original: "Anna Roth", pseudonym: "Répondant 01", source: "person", person_id: "P-0001" },
          { kind: "person", original: "Anna", pseudonym: "Répondant 01", source: "person", person_id: "P-0001" },
          { kind: "person", original: "Claire Vuilleumier", pseudonym: "Personne A", source: "manual", person_id: null },
        ],
      }),
    })
    mock.on("PUT", "/api/pseudonymisation", { body: view() })
    renderWithClient(<PseudonymisationSection />)
    const derived = await screen.findByRole("table", { name: "Noms issus des fiches de Personnes" })
    expect(derived).toHaveTextContent("Anna Roth")
    expect(screen.getAllByRole("link", { name: /Ouvrir la fiche P-0001/ })[0]).toHaveAttribute("href", "#/personnes/P-0001")
    expect(screen.getByLabelText("Nom réel 1")).toHaveValue("Claire Vuilleumier")
    expect(screen.queryByLabelText("Nom réel 2")).toBeNull()
    expect(screen.getByText(/déjà géré par la fiche P-0001/)).toBeInTheDocument()
    expect(screen.getByRole("switch", { name: /Remplacer les noms/ })).toHaveAttribute("aria-checked", "false")
    expect(screen.getByText(/la liste reste active/)).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText("Pseudonyme 1"), { target: { value: "Personne Z" } })
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer la liste" }))
    await waitFor(() => expect(mock.to("PUT", "/api/pseudonymisation")).toHaveLength(1))
    expect(mock.to("PUT", "/api/pseudonymisation")[0].body).toEqual({ entries: [{ kind: "person", original: "Claire Vuilleumier", pseudonym: "Personne Z" }] })
  })
})
