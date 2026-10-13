import { describe, expect, it } from "vitest"

import {
  codeFromTerms,
  frequencyTable,
  keynessText,
  selectionSummary,
  sortKeywords,
  termText,
  toggleAll,
  toggleStem,
  valueLabel,
} from "./lexicon"
import type { KeywordRow, TermFrequencyAnswer } from "./lexicon-api"

function row(over: Partial<KeywordRow> & { stem: string }): KeywordRow {
  return {
    forms: [over.stem],
    n_passages: 1,
    n_interviews: 1,
    interviews: ["ECO-16"],
    passage_ids: [`P-${over.stem}`],
    n_occurrences: 1,
    corpus_occurrences: 10,
    keyness: 0,
    ...over,
  }
}

const ROWS: KeywordRow[] = [
  row({ stem: "lot", forms: ["lot", "lots"], n_passages: 11, n_interviews: 3, interviews: ["ECO-16", "ECO-32", "RET-22"], passage_ids: ["P1", "P2", "P3"], keyness: 25.4 }),
  row({ stem: "rappel", forms: ["rappel", "rappeler"], n_passages: 9, n_interviews: 2, interviews: ["ECO-16", "ECO-32"], passage_ids: ["P2", "P4"], keyness: 31.2 }),
  row({ stem: "cout", forms: ["coût", "coûter"], n_passages: 8, n_interviews: 3, interviews: ["ECO-16", "ECO-32", "RET-22"], passage_ids: ["P5"], keyness: 9.1 }),
  row({ stem: "revendeur", forms: ["revendeur"], n_passages: 9, n_interviews: 2, interviews: ["ECO-16", "ECO-32"], passage_ids: ["P6"], keyness: -2.5 }),
]

describe("sortKeywords", () => {
  it("sorts by keyness, highest first", () => {
    expect(sortKeywords(ROWS, "keyness", "desc").map((r) => r.stem)).toEqual(["rappel", "lot", "cout", "revendeur"])
  })
  it("sorts by keyness ascending", () => {
    expect(sortKeywords(ROWS, "keyness", "asc").map((r) => r.stem)).toEqual(["revendeur", "cout", "lot", "rappel"])
  })
  it("breaks a tie on the number of passages by keyness", () => {
    // rappel and revendeur both have 9 passages
    expect(sortKeywords(ROWS, "passages", "desc").map((r) => r.stem)).toEqual(["lot", "rappel", "revendeur", "cout"])
  })
  it("sorts the term column in French alphabetical order, accents ignored", () => {
    expect(sortKeywords(ROWS, "term", "asc").map((r) => r.stem)).toEqual(["cout", "lot", "rappel", "revendeur"])
  })
  it("sorts by the number of interviews", () => {
    expect(sortKeywords(ROWS, "interviews", "desc").map((r) => r.stem)).toEqual(["lot", "cout", "rappel", "revendeur"])
  })
  it("sorts by the share of the corpus occurrences found in the highlights", () => {
    const rows = [row({ stem: "a", n_occurrences: 1, corpus_occurrences: 10 }), row({ stem: "b", n_occurrences: 5, corpus_occurrences: 10 })]
    expect(sortKeywords(rows, "share", "desc").map((r) => r.stem)).toEqual(["b", "a"])
  })
  it("does not change its input", () => {
    const copy = ROWS.map((r) => r.stem)
    sortKeywords(ROWS, "term", "desc")
    expect(ROWS.map((r) => r.stem)).toEqual(copy)
  })
})

describe("selection", () => {
  it("adds then removes a stem", () => {
    const a = toggleStem(new Set<string>(), "lot")
    expect([...a]).toEqual(["lot"])
    expect(toggleStem(a, "lot").size).toBe(0)
    expect(a.size).toBe(1)
  })
  it("selects every shown row, or clears when all are selected", () => {
    const all = toggleAll(new Set<string>(), ROWS)
    expect(all.size).toBe(4)
    expect(toggleAll(all, ROWS).size).toBe(0)
    expect(toggleAll(new Set(["lot"]), ROWS).size).toBe(4)
  })
  it("summarises the highlights behind the selected terms without double counting", () => {
    const s = selectionSummary(ROWS, new Set(["lot", "rappel"]))
    expect(s.passageIds).toEqual(["P1", "P2", "P3", "P4"])
    expect(s.interviews).toEqual(["ECO-16", "ECO-32", "RET-22"])
  })
  it("ignores a selected stem that is no longer in the table", () => {
    expect(selectionSummary(ROWS, new Set(["inconnu"])).passageIds).toEqual([])
  })
})

describe("create code prefill", () => {
  it("names the code after the single selected term", () => {
    const p = codeFromTerms(ROWS, new Set(["rappel"]))
    expect(p?.name).toBe("rappel")
    expect(p?.passageIds).toEqual(["P2", "P4"])
  })
  it("joins several terms in table order, with their commonest form", () => {
    const p = codeFromTerms(ROWS, new Set(["rappel", "lot"]))
    expect(p?.name).toBe("lot, rappel")
    expect(p?.passageIds).toEqual(["P1", "P2", "P3", "P4"])
  })
  it("is null when nothing is selected", () => {
    expect(codeFromTerms(ROWS, new Set())).toBeNull()
  })
})

describe("labels", () => {
  it("shows the forms of a stem group", () => {
    expect(termText(ROWS[2])).toBe("coût, coûter")
  })
  it("formats the keyness with a decimal comma and a true minus", () => {
    expect(keynessText(31.24)).toBe("31,2")
    expect(keynessText(-2.5)).toBe("−2,5")
  })
  it("labels an empty attribute value", () => {
    expect(valueLabel(null)).toBe("Non renseigné")
    expect(valueLabel("PME")).toBe("PME")
  })
})

describe("frequencyTable", () => {
  const answer: TermFrequencyAnswer = {
    attribute: "secteur",
    mode: "terms",
    stem: true,
    note: "",
    terms: [
      {
        term: "rappel",
        by_value: [
          { value: "Vin", n_interviews: 4, n_interviews_with_term: 3, n_units_with_term: 7, n_occurrences: 9, n_words: 12000, per_1000_words: 0.75 },
          { value: null, n_interviews: 1, n_interviews_with_term: 0, n_units_with_term: 0, n_occurrences: 0, n_words: 3000, per_1000_words: 0 },
        ],
      },
      {
        term: "lot",
        by_value: [
          { value: "Vin", n_interviews: 4, n_interviews_with_term: 4, n_units_with_term: 11, n_occurrences: 15, n_words: 12000, per_1000_words: 1.25 },
          { value: "Bière", n_interviews: 2, n_interviews_with_term: 1, n_units_with_term: 1, n_occurrences: 1, n_words: 5000, per_1000_words: 0.2 },
        ],
      },
    ],
  }
  it("builds one row per value with a cell per term, the empty value last", () => {
    const t = frequencyTable(answer)
    expect(t.terms).toEqual(["rappel", "lot"])
    expect(t.rows.map((r) => r.label)).toEqual(["Bière", "Vin", "Non renseigné"])
    const vin = t.rows.find((r) => r.label === "Vin")!
    expect(vin.n_interviews).toBe(4)
    expect(vin.n_words).toBe(12000)
    expect(vin.cells.map((c) => c?.n_occurrences)).toEqual([9, 15])
    const biere = t.rows.find((r) => r.label === "Bière")!
    expect(biere.cells[0]).toBeNull()
  })
  it("is empty in the top terms mode", () => {
    expect(frequencyTable({ attribute: "secteur", mode: "top_terms", values: [] }).rows).toEqual([])
  })
})
