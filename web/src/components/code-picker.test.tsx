import { useState } from "react"
import { describe, expect, it } from "vitest"
import { fireEvent, render, screen } from "@testing-library/react"
import { CodePicker } from "./code-picker"
import type { Code } from "@/lib/types"

const allowed: Code[] = [
  { code: "TECH", family: "TECH", label: "Technological context", definition: "tech", origin: "phase1", active: true, in_prompt: true },
  { code: "ORG", family: "ORG", label: "Organisational context", definition: "org", origin: "phase1", active: true, in_prompt: true },
]

function Harness({ initial = [] as string[] }) {
  const [v, setV] = useState(initial)
  return (
    <>
      <CodePicker allowed={allowed} value={v} onChange={setV} />
      <output data-testid="value">{v.join(",")}</output>
    </>
  )
}

describe("CodePicker", () => {
  it("offers only the allowed codes plus NA", () => {
    render(<Harness />)
    expect(screen.getAllByText("TECH").length).toBeGreaterThan(0)
    expect(screen.queryByText("ENV")).toBeNull()
    expect(screen.getByText("NA")).toBeInTheDocument()
  })
  it("toggles codes and keeps NA exclusive", () => {
    render(<Harness initial={["TECH"]} />)
    fireEvent.click(screen.getByText("ORG"))
    expect(screen.getByTestId("value")).toHaveTextContent("ORG,TECH")
    fireEvent.click(screen.getByText("NA"))
    expect(screen.getByTestId("value")).toHaveTextContent(/^NA$/)
  })
  it("filters by label as you type", () => {
    render(<Harness />)
    fireEvent.change(screen.getByPlaceholderText(/Filtrer les codes/), { target: { value: "organis" } })
    expect(screen.queryByText("Technological context")).toBeNull()
    expect(screen.getByText("Organisational context")).toBeInTheDocument()
  })
})

describe("CodePicker keyboard", () => {
  it("leaves Ctrl+Enter to the save shortcut", () => {
    render(<Harness initial={["TECH"]} />)
    const input = screen.getByPlaceholderText(/Filtrer les codes/)
    fireEvent.keyDown(input, { key: "Enter", ctrlKey: true })
    expect(screen.getByTestId("value")).toHaveTextContent("TECH")
  })
})
