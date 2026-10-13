import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react"

import { InterviewPersons } from "@/components/library/persons-section"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

const p = (id: string, name: string, over: Record<string, unknown> = {}) => ({
  person_id: id, kind: "person", name, organisation: "", function: "", email: "", phone: "", notes: "", consent_status: "", consent_date: "",
  consent_document: "", anonymise: true, pseudonym: `Répondant ${id.slice(-2)}`, aliases: [], attributes: {}, group_ids: [], created_at: "", updated_at: "", n_interviews: 0, ...over,
})

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock
    .on("GET", "/api/interviews/INT-01/persons", { body: { interview_id: "INT-01", persons: [{ ...p("P-0001", "Anna Roth"), role: "respondent" }, { ...p("P-0002", "Marc Bovet", { anonymise: false }), role: "interviewer" }] } })
    .on("GET", "/api/persons", { body: { n: 3, persons: [p("P-0001", "Anna Roth"), p("P-0002", "Marc Bovet"), p("P-0003", "Eva Lanz")] } })
})
afterEach(cleanup)

describe("Personnes of an interview fiche", () => {
  it("lists the linked persons with their role and the pseudonym the model sees", async () => {
    renderWithClient(<InterviewPersons interviewId="INT-01" />)
    expect(await screen.findByRole("link", { name: "Anna Roth" })).toHaveAttribute("href", "#/personnes/P-0001")
    expect(screen.getByText(/pour le modèle : « Répondant 01 »/)).toBeInTheDocument()
    expect(screen.getByText(/Non anonymisée/)).toBeInTheDocument()
  })

  it("links a person that is not yet linked, with a role, and removes a link", async () => {
    mock.on("POST", "/api/interviews/INT-01/persons", { body: { interview_id: "INT-01", persons: [] } })
    mock.on("DELETE", "/api/interviews/INT-01/persons/P-0002", { body: { interview_id: "INT-01", persons: [] } })
    renderWithClient(<InterviewPersons interviewId="INT-01" />)
    await screen.findByRole("link", { name: "Anna Roth" })
    const pick = screen.getByLabelText("Personne à lier")
    expect(Array.from((pick as HTMLSelectElement).options).map((o) => o.text)).toEqual(["Ajouter une personne", "Eva Lanz"])
    fireEvent.change(pick, { target: { value: "P-0003" } })
    fireEvent.change(screen.getByLabelText("Rôle à donner"), { target: { value: "co_respondent" } })
    fireEvent.click(screen.getByRole("button", { name: "Lier" }))
    await waitFor(() => expect(mock.to("POST", "/api/interviews/INT-01/persons")).toHaveLength(1))
    expect(mock.to("POST", "/api/interviews/INT-01/persons")[0].body).toEqual({ person_id: "P-0003", role: "co_respondent" })
    fireEvent.click(screen.getByRole("button", { name: "Retirer Marc Bovet de cet entretien" }))
    await waitFor(() => expect(mock.to("DELETE", "/api/interviews/INT-01/persons/P-0002")).toHaveLength(1))
  })
})
