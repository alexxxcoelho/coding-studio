import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react"

import { CooccurrenceTab } from "@/components/analysis/cooccurrence"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/project", { body: { attributes: [] } })
  mock.on("GET", /^\/api\/codes\/cooccurrence\?/, {
    body: {
      codes: [
        { code_id: "C-1", name: "Coût", active: true },
        { code_id: "C-2", name: "Confiance", active: true },
      ],
      relation: "same_passage", group_by: null, attribute: null, rollup: false, note: "",
      groups: [{ key: "all", label: "Tous", interviews: 2, cells: [{ a: "C-1", b: "C-2", units: 3, interviews: 2, passage_pairs: 3, jaccard: 0.5 }], diagonal: [{ code_id: "C-1", units: 5, interviews: 2, passages: 5 }, { code_id: "C-2", units: 4, interviews: 2, passages: 4 }] }],
    },
  })
  mock.on("GET", /^\/api\/codes\/cooccurrence\/passages/, {
    body: { a: "C-1", b: "C-2", relation: "same_passage", rollup: false, n_units: 1, items: [{ interview_id: "INT-01", unit_id: "INT-01-U003", text: "Le coût me rassure", pairs: [] }] },
  })
})
afterEach(() => cleanup())

describe("co-occurrence", () => {
  it("prints the counts and lists the passages of a cell", async () => {
    renderWithClient(<CooccurrenceTab />)
    const cell = await screen.findAllByRole("button", { name: /Coût et Confiance : 3 tours/ })
    expect(cell[0]).toHaveTextContent("3")
    fireEvent.click(cell[0])
    await waitFor(() => expect(screen.getByText("Le coût me rassure")).toBeInTheDocument())
    expect(mock.to("GET", /cooccurrence\/passages/)[0].path).toContain("a=C-1")
  })
})
