import { describe, expect, it } from "vitest"

import type { MatrixCell } from "./analysis-api"
import { axisOptions, axisToken, cellView, countText, DEFAULT_MATRIX, heatLevel, matrixMax, parseAxisToken, validateMatrix } from "./matrix"
import type { ProjectConfig } from "./types"

const config = {
  attributes: [
    { id: "sector", label: "Secteur", kind: "choice", choices: ["détail", "industrie"], shared_with_model: false },
    { id: "size", label: "Taille", kind: "number", choices: [], shared_with_model: false },
  ],
  matrix_dimensions: [{ id: "qui-impose", label: "Qui impose" }],
} as unknown as ProjectConfig

const cell = (p: Partial<MatrixCell> = {}): MatrixCell => ({ row_key: "interview:A", col_key: "theme:T-1", value: null, summary: "", n_passages: 0, n_interviews: 0, ...p })

describe("axis tokens", () => {
  it("round-trip every kind", () => {
    for (const s of [
      { kind: "interviews" },
      { kind: "cases" },
      { kind: "themes" },
      { kind: "codes" },
      { kind: "focused_codes" },
      { kind: "dimensions" },
      { kind: "attribute", id: "sector" },
    ] as const) {
      expect(parseAxisToken(axisToken(s))).toEqual(s)
    }
  })
  it("rejects what it does not know", () => {
    expect(parseAxisToken("planets")).toBeNull()
    expect(parseAxisToken("attribute:")).toBeNull()
  })
})

describe("axisOptions", () => {
  it("offers interviews, cases, themes, codes, focused codes, each attribute and the dimensions of the study", () => {
    const labels = axisOptions(config).map((o) => o.label)
    expect(labels).toEqual(["Entretiens", "Cas", "Thèmes", "Codes", "Codes focalisés", "Secteur", "Taille", "Dimensions de l'étude"])
  })
  it("works before the configuration is loaded", () => {
    expect(axisOptions(undefined).map((o) => o.token)).toEqual(["interviews", "cases", "themes", "codes", "focused_codes"])
  })
})

describe("validateMatrix", () => {
  it("accepts the default view", () => {
    expect(validateMatrix(DEFAULT_MATRIX, config)).toBeNull()
  })
  it("refuses the same list on both sides", () => {
    expect(validateMatrix({ ...DEFAULT_MATRIX, cols: { kind: "interviews" } }, config)).toMatch(/mêmes/)
  })
  it("refuses an attribute or dimensions the study does not define", () => {
    expect(validateMatrix({ ...DEFAULT_MATRIX, rows: { kind: "attribute", id: "nope" } }, config)).toMatch(/attribut/)
    expect(validateMatrix({ ...DEFAULT_MATRIX, cols: { kind: "dimensions" } }, { ...config, matrix_dimensions: [] })).toMatch(/dimension/)
  })
  it("refuses a filter on an unknown attribute", () => {
    expect(validateMatrix({ ...DEFAULT_MATRIX, filter: { attribute: "x", value: "y" } }, config)).toMatch(/attribut/)
    expect(validateMatrix({ ...DEFAULT_MATRIX, filter: { attribute: "sector", value: "détail" } }, config)).toBeNull()
  })
})

describe("cellView", () => {
  it("says 'pas encore codé' for a row without coding, never 'non évoqué'", () => {
    const v = cellView(cell(), false)
    expect(v.state).toBe("not_coded")
    expect(v.label).toBe("pas encore codé")
  })
  it("shows 'aucun passage' when nothing is attached and no value was chosen", () => {
    expect(cellView(cell(), true)).toMatchObject({ state: "none", label: "aucun passage", heat: 0 })
    expect(cellView(undefined, true).state).toBe("none")
  })
  it("asks for a value when passages exist without one", () => {
    const v = cellView(cell({ n_passages: 3, n_interviews: 1 }), true)
    expect(v).toMatchObject({ state: "unqualified", label: "à qualifier", countText: "3 passages" })
  })
  it("shows the closed value, the count and the summary", () => {
    const v = cellView(cell({ value: "contradicted", n_passages: 2, n_interviews: 1, summary: "Le groupe paie" }), true)
    expect(v).toMatchObject({ state: "qualified", label: "contredit", summary: "Le groupe paie", contradiction: true })
  })
  it("keeps a researcher's 'non évoqué' even with no passage", () => {
    expect(cellView(cell({ value: "not_mentioned" }), true)).toMatchObject({ state: "qualified", label: "non évoqué" })
  })
})

describe("countText", () => {
  it("separates passages and interviews", () => {
    expect(countText(1, 1)).toBe("1 passage")
    expect(countText(7, 2)).toBe("7 passages dans 2 entretiens")
    expect(countText(0, 0)).toBe("aucun passage")
  })
})

describe("heat", () => {
  it("scales the count to four levels of the largest cell", () => {
    expect(heatLevel(0, 8)).toBe(0)
    expect(heatLevel(1, 8)).toBe(1)
    expect(heatLevel(4, 8)).toBe(2)
    expect(heatLevel(6, 8)).toBe(3)
    expect(heatLevel(8, 8)).toBe(4)
    expect(heatLevel(5, 0)).toBe(0)
  })
  it("finds the largest count", () => {
    expect(matrixMax([cell({ n_passages: 2 }), cell({ n_passages: 9 })])).toBe(9)
    expect(matrixMax([])).toBe(0)
  })
})
