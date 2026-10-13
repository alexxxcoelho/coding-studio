import { describe, expect, it } from "vitest"

import { entriesProblem, sameEntries } from "./pseudonymisation"

const row = (original: string, pseudonym: string, kind = "person") => ({ kind, original, pseudonym })

describe("entriesProblem", () => {
  it("accepts an empty list and a clean one", () => {
    expect(entriesProblem([])).toBeNull()
    expect(entriesProblem([row("Claire Vuilleumier", "Personne A"), row("Lectis", "Fournisseur A", "organisation")])).toBeNull()
  })
  it("refuses a blank name or pseudonym", () => {
    expect(entriesProblem([row(" ", "P")])).toMatch(/ligne 1/)
    expect(entriesProblem([row("Anna", "")])).toMatch(/pseudonyme de « Anna » est vide/)
  })
  it("refuses a name that does not change", () => {
    expect(entriesProblem([row("Anna", "anna")])).toMatch(/ne change pas/)
  })
  it("refuses a duplicate, case ignored", () => {
    expect(entriesProblem([row("Anna", "P1"), row("ANNA", "P2")])).toMatch(/deux fois/)
  })
  it("refuses a pseudonym that is itself a name of the list", () => {
    expect(entriesProblem([row("Anna", "Berta"), row("Berta", "P2")])).toMatch(/lui-même un nom/)
  })
})

describe("sameEntries", () => {
  it("compares the rows in order", () => {
    expect(sameEntries([row("a", "b")], [row("a", "b")])).toBe(true)
    expect(sameEntries([row("a", "b")], [row("a", "c")])).toBe(false)
    expect(sameEntries([], [row("a", "b")])).toBe(false)
  })
})
