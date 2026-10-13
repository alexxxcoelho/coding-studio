import { describe, expect, it } from "vitest"
import { attributeDefsProblem, attributeErrors, attributePatch, attributeSummary, checkAttribute, cleanDef, slugId, validDate } from "./attributes"
import type { AttributeDef } from "./types"

const def = (id: string, kind: AttributeDef["kind"], extra: Partial<AttributeDef> = {}): AttributeDef => ({
  id,
  label: id,
  kind,
  choices: [],
  shared_with_model: false,
  ...extra,
})

describe("checkAttribute", () => {
  it("clears on an empty value", () => {
    expect(checkAttribute(def("a", "text"), "   ")).toEqual({ ok: true, value: null })
  })
  it("trims a text and caps it at 2000 characters", () => {
    expect(checkAttribute(def("a", "text"), "  Directrice  ")).toEqual({ ok: true, value: "Directrice" })
    expect(checkAttribute(def("a", "text"), "x".repeat(2001)).ok).toBe(false)
    expect(checkAttribute(def("a", "text"), "x".repeat(2000)).ok).toBe(true)
  })
  it("accepts only a value of the list", () => {
    const d = def("v", "choice", { choices: ["A", "B"] })
    expect(checkAttribute(d, "B")).toEqual({ ok: true, value: "B" })
    expect(checkAttribute(d, "C").ok).toBe(false)
  })
  it("reads a number with a comma and stores a point", () => {
    expect(checkAttribute(def("n", "number"), "12,5")).toEqual({ ok: true, value: "12.5" })
    expect(checkAttribute(def("n", "number"), "-3")).toEqual({ ok: true, value: "-3" })
    expect(checkAttribute(def("n", "number"), "douze").ok).toBe(false)
    expect(checkAttribute(def("n", "number"), "1,2,3").ok).toBe(false)
    expect(checkAttribute(def("n", "number"), "1e5").ok).toBe(false)
  })
  it("reads a date as AAAA-MM-JJ with a real day", () => {
    expect(checkAttribute(def("d", "date"), "2026-10-13").ok).toBe(true)
    expect(checkAttribute(def("d", "date"), "13.10.2026").ok).toBe(false)
    expect(validDate("2024-02-29")).toBe(true)
    expect(validDate("2026-02-29")).toBe(false)
    expect(validDate("2026-13-01")).toBe(false)
    expect(validDate("2026-04-31")).toBe(false)
  })
})

describe("draft to patch", () => {
  const defs = [def("sector", "choice", { choices: ["Retail", "Eco"] }), def("role", "text"), def("size", "number")]
  it("lists errors by attribute", () => {
    expect(attributeErrors(defs, { sector: "Autre", role: "x", size: "12" })).toEqual({ sector: "Valeur absente de la liste" })
    expect(attributeErrors(defs, { size: "beaucoup" })).toEqual({ size: "Nombre attendu" })
  })
  it("sends only what changed, null to clear", () => {
    expect(attributePatch(defs, { sector: "Retail", role: "CEO" }, { sector: "Retail", role: "", size: "4,5" })).toEqual({ role: null, size: "4.5" })
  })
  it("is null when nothing changed or a value is wrong", () => {
    expect(attributePatch(defs, { role: "CEO" }, { role: "CEO" })).toBeNull()
    expect(attributePatch(defs, {}, { sector: "Autre" })).toBeNull()
  })
  it("summarises a definition", () => {
    expect(attributeSummary(def("v", "choice", { label: "Version du guide", choices: ["A", "B"] }))).toBe("Version du guide (liste A, B)")
    expect(attributeSummary(def("r", "text", { label: "Rôle" }))).toBe("Rôle (texte)")
  })
})

describe("definition editor", () => {
  it("proposes a unique slug", () => {
    expect(slugId("Version du guide", [])).toBe("version-du-guide")
    expect(slugId("Rôle du répondant", ["role-du-repondant"])).toBe("role-du-repondant-2")
    expect(slugId("???", [])).toBe("element")
  })
  it("refuses what the server refuses", () => {
    expect(attributeDefsProblem([def("a", "choice")])).toMatch(/au moins une valeur/)
    expect(attributeDefsProblem([def("a", "text", { choices: ["x"] })])).toMatch(/seule une liste/)
    expect(attributeDefsProblem([def("a", "text"), def("a", "date")])).toMatch(/Identifiant en double/)
    expect(attributeDefsProblem([def("a", "text", { label: "Rôle" }), def("b", "text", { label: "rôle" })])).toMatch(/Nom en double/)
    expect(attributeDefsProblem([def("A b", "text")])).toMatch(/Identifiant invalide/)
    expect(attributeDefsProblem([def("a", "text", { label: " " })])).toMatch(/pas de nom/)
    expect(attributeDefsProblem([def("a", "choice", { choices: ["x", "x"] })])).toMatch(/en double/)
    expect(attributeDefsProblem([def("a", "choice", { choices: ["x"] }), def("b", "text")])).toBeNull()
  })
  it("cleans choices", () => {
    expect(cleanDef(def("a", "choice", { label: " L ", choices: [" A ", "", "B"] }))).toMatchObject({ label: "L", choices: ["A", "B"] })
    expect(cleanDef(def("a", "text", { choices: ["x"] })).choices).toEqual([])
  })
})
