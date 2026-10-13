import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react"

import { VerificationDialog } from "@/components/export/verification-dialog"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/interviews", { body: [{ interview_id: "INT-01", folder_name: "INT-01_A" }, { interview_id: "INT-02", folder_name: "INT-02_B" }] })
})
afterEach(cleanup)

describe("Paquet de vérification", () => {
  it("sends the chosen interviews and reports the folder and what was masked", async () => {
    mock.on("POST", "/api/export/verification", {
      body: {
        dir: "/tmp/exports/verification_x",
        files: [{ name: "README.md", sha256: "a".repeat(64), size: 10 }, { name: "persons.csv", sha256: "b".repeat(64), size: 20 }],
        manifest_sha256: "c".repeat(64),
        interview_ids: ["INT-02"],
        replacements: { person: 4, organisation: 1 },
        pseudonymisation_digest: "d".repeat(64),
        warnings: [],
      },
    })
    renderWithClient(<VerificationDialog open onClose={() => undefined} initialIds={["INT-02"]} />)
    const run = await screen.findByRole("button", { name: "Générer le paquet" })
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "INT-02" })).toBeChecked())
    expect(screen.getByRole("checkbox", { name: "INT-01" })).not.toBeChecked()
    fireEvent.click(run)
    const result = await screen.findByTestId("verification-result")
    expect(result).toHaveTextContent("/tmp/exports/verification_x")
    expect(result).toHaveTextContent("personnes : 4")
    expect(result).toHaveTextContent("organisations : 1")
    expect(mock.to("POST", "/api/export/verification")[0].body).toEqual({ interview_ids: ["INT-02"] })
  })

  it("cannot run with no interview chosen", async () => {
    renderWithClient(<VerificationDialog open onClose={() => undefined} />)
    expect(await screen.findByRole("button", { name: "Générer le paquet" })).toBeDisabled()
  })

  it("shows the refusal of the server", async () => {
    mock.on("POST", "/api/export/verification", { status: 400, body: { error: "a listed name remains in INT-01", kind: "validation" } })
    renderWithClient(<VerificationDialog open onClose={() => undefined} initialIds={["INT-01"]} />)
    fireEvent.click(await screen.findByRole("button", { name: "Générer le paquet" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("a listed name remains")
  })
})
