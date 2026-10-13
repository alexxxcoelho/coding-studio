import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react"

import { ReviewDialog } from "@/components/coder/review-dialog"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

const span = (text: string, s: number, e: number) => ({ unit_id: "INT-01-U003", start: s, end: e, start_utf16: s, end_utf16: e, text, unit_text: "Le coût monte" })
const ITEM = {
  projection_id: 7, interview_id: "INT-01", passage_id: "P-1", status: "needs_review", method: "diff", reason: "text_changed", from_version: 1, to_version: 2,
  old: span("coût", 3, 7), proposed: span("coût", 3, 7), final: null, current_unit_text: "Le coût monte vite", codes: [{ code_id: "C-1", code_name: "Coût" }],
  created_at: "", resolved_at: null, resolved_by: null, resolution_note: null,
}

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", /^\/api\/transcript\/review\?/, { body: { interview_id: "INT-01", status: "needs_review", open: 1, items: [ITEM] } })
  mock.on("GET", "/api/interviews/INT-01/transcript/versions", {
    body: { interview_id: "INT-01", current: 2, versions: [
      { version: 1, cause: "baseline", n_units: 9, target: null, reason: null, created_at: "2026-10-01T10:00:00Z", created_by: "alex", reanchored: 0, flagged: 0 },
      { version: 2, cause: "correction", n_units: 9, target: "INT-01-U003", reason: "faute", created_at: "2026-10-02T10:00:00Z", created_by: "alex", reanchored: 0, flagged: 1 },
    ] },
  })
  mock.on("POST", "/api/transcript/review/7/accept", { body: { ...ITEM, status: "accepted" } })
  mock.on("POST", "/api/transcript/review/7/detach", { body: { ...ITEM, status: "detached" } })
})
afterEach(() => cleanup())

describe("review dialog", () => {
  it("lists the versions with their cause and the items to review", async () => {
    renderWithClient(<ReviewDialog interviewId="INT-01" open onClose={() => {}} />)
    expect(await screen.findByText("Correction d'un tour")).toBeInTheDocument()
    expect(screen.getByText("Texte importé")).toBeInTheDocument()
    expect(await screen.findByText(/Emplacement proposé/)).toBeInTheDocument()
  })
  it("accepts and detaches", async () => {
    renderWithClient(<ReviewDialog interviewId="INT-01" open onClose={() => {}} />)
    fireEvent.click(await screen.findByRole("button", { name: "Accepter" }))
    await waitFor(() => expect(mock.to("POST", "/api/transcript/review/7/accept")).toHaveLength(1))
    fireEvent.click(screen.getByRole("button", { name: "Détacher" }))
    await waitFor(() => expect(mock.to("POST", "/api/transcript/review/7/detach")).toHaveLength(1))
  })
  it("disables Accepter when nothing is proposed", async () => {
    mock.on("GET", /^\/api\/transcript\/review\?/, { body: { interview_id: "INT-01", status: "needs_review", open: 1, items: [{ ...ITEM, proposed: null }] } })
    renderWithClient(<ReviewDialog interviewId="INT-01" open onClose={() => {}} />)
    expect(await screen.findByRole("button", { name: "Accepter" })).toBeDisabled()
  })
})
