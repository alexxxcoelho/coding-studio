import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render, screen, within } from "@testing-library/react"

import { GuidePage } from "@/pages/guide"

afterEach(cleanup)

describe("guide page", () => {
  it("opens the summary by default, with the sommaire of sections", () => {
    render(<GuidePage anchor={null} />)
    const toc = screen.getByRole("navigation", { name: "Sommaire" })
    expect(within(toc).getByRole("link", { name: /Préparer l'étude/ })).toHaveAttribute("href", "#/guide/preparer")
    expect(screen.getByRole("heading", { level: 1, name: "Guide de Coding Studio" })).toHaveAttribute("id", "sommaire")
  })

  it("opens the section that owns a heading anchor and lists its headings", () => {
    render(<GuidePage anchor="coder-detail" />)
    expect(screen.getByRole("heading", { level: 1, name: "Coder un entretien" })).toBeInTheDocument()
    expect(document.getElementById("coder-detail")).not.toBeNull()
    const toc = screen.getByRole("navigation", { name: "Sommaire" })
    expect(within(toc).getByRole("link", { name: "Pourquoi cette étape" })).toHaveAttribute("href", "#/guide/coder-pourquoi")
  })

  it("rewrites the links between files into routes of the app", () => {
    render(<GuidePage anchor={null} />)
    const article = document.querySelector("article") as HTMLElement
    const link = within(article).getAllByRole("link", { name: "Préparer" })[0]
    expect(link).toHaveAttribute("href", "#/guide/preparer-bibliotheque")
  })
})
