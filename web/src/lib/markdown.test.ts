import { describe, expect, it } from "vitest"
import { renderMarkdown, resolveGuideHref } from "./markdown"

describe("guide markdown", () => {
  it("turns {#id} heading attributes into ids and removes them from the text", () => {
    const { html, headings } = renderMarkdown("# Guide {#sommaire}\n\n## Ce que couvre ce guide {#sommaire-portee}\n\nTexte.")
    expect(html).toContain('<h1 id="sommaire">Guide</h1>')
    expect(html).toContain('<h2 id="sommaire-portee">Ce que couvre ce guide</h2>')
    expect(html).not.toContain("{#")
    expect(headings).toEqual([
      { level: 1, id: "sommaire", text: "Guide" },
      { level: 2, id: "sommaire-portee", text: "Ce que couvre ce guide" },
    ])
  })
  it("leaves headings without attribute without id", () => {
    expect(renderMarkdown("## Sans ancre").html).toContain("<h2>Sans ancre</h2>")
  })
  it("keeps inline markup inside a heading with an attribute", () => {
    expect(renderMarkdown("### 1. Créer le **projet** {#p}").html).toContain('<h3 id="p">1. Créer le <strong>projet</strong></h3>')
  })
  it("rewrites links between guide files into in-app routes", () => {
    const { html } = renderMarkdown("Voir [Coder](02_coder.md#coder-parcourir) et [Préparer](01_preparer.md).")
    expect(html).toContain('href="#/guide/coder-parcourir"')
    expect(html).toContain('href="#/guide/preparer"')
  })
  it("rewrites same-file anchors and keeps external links external", () => {
    expect(resolveGuideHref("#coder-detail")).toBe("#/guide/coder-detail")
    expect(resolveGuideHref("./03_revoir.md#revoir-prompt")).toBe("#/guide/revoir-prompt")
    expect(resolveGuideHref("fr/04_analyser.md")).toBe("#/guide/analyser")
    expect(resolveGuideHref("https://example.org/x")).toBe("https://example.org/x")
    const { html } = renderMarkdown("[ext](https://example.org)")
    expect(html).toContain('target="_blank"')
    expect(html).toContain('rel="noopener noreferrer"')
  })
  it("renders lists and emphasis", () => {
    const { html } = renderMarkdown("1. un\n2. deux\n\n**Dans Coding Studio.** suite")
    expect(html).toContain("<ol>")
    expect(html).toContain("<strong>Dans Coding Studio.</strong>")
  })
})
