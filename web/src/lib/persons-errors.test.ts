import { describe, expect, it } from "vitest"

import { ApiError } from "./api"
import { interviewAttributes, personAttributes } from "./attributes"
import { templateCsv } from "./persons-csv"
import { plainError } from "./persons-errors"
import { parseHash, personHref } from "./router"

const err = (status: number, message: string) => new ApiError(status, status === 409 ? "guard" : "validation", message)

describe("plainError", () => {
  it("explains a name used by two records", () => {
    const m = plainError(err(400, 'the name "Anna Roth" is used by P-0001 and by P-0002'))
    expect(m).toContain("« Anna Roth » est déjà pris")
    expect(m).toContain("P-0001")
    expect(m).toContain("P-0002")
  })
  it("explains a pseudonym that puts a name back", () => {
    expect(plainError(err(400, 'the pseudonym "Anna 1" of P-0003 contains a listed name or alias'))).toContain("remettrait un nom réel")
  })
  it("explains an unflagged record whose name another record replaces", () => {
    const m = plainError(err(400, 'P-0004 is not anonymised but its name "Anna" contains a name or alias that another record replaces'))
    expect(m).toContain("P-0004 n'est pas anonymisée")
    expect(m).toContain("Anonymisez cette fiche")
  })
  it("explains the refused deletions (409)", () => {
    expect(plainError(err(409, "P-0001 is linked to 2 interview(s); unlink it first or delete with its links"))).toContain("liée à 2 entretien(s)")
    expect(plainError(err(409, "group G-0001 is not empty (1 sub-group(s), 3 person(s)); move them or give a group to reassign them to"))).toContain("1 sous-groupe(s) et 3 personne(s)")
  })
  it("explains a group name that exists and a cycle", () => {
    expect(plainError(err(400, 'a group named "Clients" already exists at this level'))).toContain("« Clients » existe déjà à ce niveau")
    expect(plainError(err(400, "G-0001 cannot be moved under G-0002: it would create a cycle"))).toContain("ni sous l'un de ses sous-groupes")
  })
  it("keeps an unknown message as the server wrote it", () => {
    expect(plainError(err(400, "something new"))).toBe("something new")
  })
})

describe("attribute scopes", () => {
  const defs = [
    { id: "sector", label: "Secteur", kind: "text" as const, choices: [], shared_with_model: false },
    { id: "size", label: "Taille", kind: "text" as const, choices: [], shared_with_model: false, scope: "person" as const },
  ]
  it("separates the attributes of the interviews from those of the register", () => {
    expect(interviewAttributes(defs).map((d) => d.id)).toEqual(["sector"])
    expect(personAttributes(defs).map((d) => d.id)).toEqual(["size"])
  })
})

describe("persons route and template", () => {
  it("parses the register and a record", () => {
    expect(parseHash("#/personnes")).toEqual({ page: "persons", id: null })
    expect(parseHash("#/personnes/P-0007")).toEqual({ page: "persons", id: "P-0007" })
    expect(personHref("P-0007")).toBe("#/personnes/P-0007")
  })
  it("writes the header row with a column per person attribute", () => {
    expect(templateCsv([])).toMatch(/^person_id,name,kind,organisation,function,email,phone,notes,aliases,groups,anonymise,pseudonym,interviews,consent_status,consent_date,consent_document\n$/)
    expect(templateCsv(["size"]).trim().endsWith(",attr:size")).toBe(true)
  })
})
