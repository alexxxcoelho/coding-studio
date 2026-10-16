import { describe, expect, it } from "vitest"
import {
  buildRows,
  codeColor,
  codeForNumber,
  colorIndex,
  descendantIds,
  filterRows,
  firstApplicable,
  numberKeys,
  possibleParents,
  topCodes,
  wouldCycle,
} from "./code-tree"
import type { CodeNode } from "./types"

function node(id: string, name: string, extra: Partial<CodeNode> = {}): CodeNode {
  return {
    code_id: id,
    token: name.toUpperCase().replace(/\W+/g, "-"),
    name,
    label: name,
    definition: "",
    includes: "",
    excludes: "",
    parent_id: null,
    origin: "field",
    toe: null,
    research_questions: [],
    active: true,
    system: false,
    in_prompt: true,
    merged_into: null,
    version: 1,
    example: null,
    reason: null,
    origin_passage_id: null,
    example_passage_id: null,
    created_at: null,
    aliases: [],
    counts: { passages: 0, interviews: 0, passages_rollup: 0, interviews_rollup: 0 },
    ...extra,
  }
}

const LATER = node("LATER", "Plus tard", { system: true, token: "LATER" })
const tree = [
  node("C-0001", "Obligation légale"),
  node("C-0002", "Demande des partenaires", { definition: "Un client ou un fournisseur l'exige" }),
  node("C-0003", "Référencement chez un détaillant", { parent_id: "C-0002", aliases: [{ alias: "listing", reason: "synonym", created_at: "" }] }),
  node("C-0004", "Mandat fournisseur", { parent_id: "C-0002" }),
  node("C-0005", "Coût perçu comme un frein"),
  node("C-0006", "Ancien code", { merged_into: "C-0005", active: false }),
  LATER,
]

describe("buildRows", () => {
  it("nests children under their parent and puts the system code last", () => {
    const rows = buildRows([LATER, ...tree.slice(0, 6)])
    expect(rows.map((r) => [r.node.code_id, r.depth])).toEqual([
      ["C-0001", 0],
      ["C-0002", 0],
      ["C-0003", 1],
      ["C-0004", 1],
      ["C-0005", 0],
      ["LATER", 0],
    ])
    expect(rows[1].hasChildren).toBe(true)
    expect(rows[0].hasChildren).toBe(false)
  })
  it("hides the descendants of a collapsed code", () => {
    const rows = buildRows(tree, new Set(["C-0002"]))
    expect(rows.map((r) => r.node.code_id)).toEqual(["C-0001", "C-0002", "C-0005", "LATER"])
  })
  it("shows a code whose parent is missing at the top level", () => {
    const rows = buildRows([node("C-0009", "Orphelin", { parent_id: "C-0404" })])
    expect(rows).toHaveLength(1)
    expect(rows[0].depth).toBe(0)
  })
  it("survives a cycle in the data", () => {
    const rows = buildRows([node("C-0001", "A", { parent_id: "C-0002" }), node("C-0002", "B", { parent_id: "C-0001" })])
    expect(rows.length).toBeLessThanOrEqual(2)
  })
})

describe("filterRows", () => {
  it("matches name and definition, ignoring historical aliases without accents or case", () => {
    expect(filterRows(tree, "OBLIGATION legale").map((r) => r.node.code_id)).toEqual(["C-0001"])
    expect(filterRows(tree, "listing")).toEqual([])
    expect(filterRows(tree, "fournisseur l'exige").map((r) => r.node.code_id)).toEqual(["C-0002"])
  })
  it("keeps the ancestors of a match", () => {
    const rows = filterRows(tree, "mandat")
    expect(rows.map((r) => [r.node.code_id, r.depth])).toEqual([
      ["C-0002", 0],
      ["C-0004", 1],
    ])
  })
  it("requires every word", () => {
    expect(filterRows(tree, "coût frein").map((r) => r.node.code_id)).toEqual(["C-0005"])
    expect(filterRows(tree, "coût mandat")).toEqual([])
  })
  it("returns the whole tree for an empty query and never lists merged codes", () => {
    expect(filterRows(tree, "  ").map((r) => r.node.code_id)).not.toContain("C-0006")
    expect(filterRows(tree, "ancien")).toEqual([])
  })
})

describe("moves", () => {
  it("knows descendants", () => {
    expect([...descendantIds(tree, "C-0002")].sort()).toEqual(["C-0003", "C-0004"])
  })
  it("refuses a cycle and offers neither itself nor its descendants as parent", () => {
    expect(wouldCycle(tree, "C-0002", "C-0003")).toBe(true)
    expect(wouldCycle(tree, "C-0002", "C-0002")).toBe(true)
    expect(wouldCycle(tree, "C-0003", "C-0001")).toBe(false)
    expect(wouldCycle(tree, "C-0003", null)).toBe(false)
    expect(possibleParents(tree, "C-0002").map((c) => c.code_id)).toEqual(["C-0001", "C-0005"])
  })
})

describe("colours", () => {
  it("is stable per id and cycles over the seven tokens", () => {
    expect(colorIndex("C-0001")).toBe(0)
    expect(colorIndex("C-0008")).toBe(0)
    expect(codeColor("C-0004")).toBe("var(--c-cost)")
    expect(codeColor("C-0011")).toBe(codeColor("C-0004"))
    expect(codeColor("LATER")).toBe("var(--label3)")
  })
})

describe("number keys", () => {
  it("numbers the first nine applicable visible codes and skips the system and merged ones", () => {
    const rows = buildRows(tree)
    const keys = numberKeys(rows)
    expect([...keys.entries()]).toEqual([
      ["C-0001", 1],
      ["C-0002", 2],
      ["C-0003", 3],
      ["C-0004", 4],
      ["C-0005", 5],
    ])
    expect(codeForNumber(rows, 3)?.code_id).toBe("C-0003")
    expect(codeForNumber(rows, 6)).toBeNull()
  })
  it("follows the filter: keys apply to the visible codes", () => {
    const rows = filterRows(tree, "mandat")
    expect(codeForNumber(rows, 1)?.code_id).toBe("C-0002")
    expect(codeForNumber(rows, 2)?.code_id).toBe("C-0004")
  })
  it("stops at nine", () => {
    const many = Array.from({ length: 12 }, (_, i) => node(`C-${String(i + 1).padStart(4, "0")}`, `Code ${i}`))
    expect(numberKeys(buildRows(many)).size).toBe(9)
  })
  it("picks the first applicable row for a typed search", () => {
    expect(firstApplicable(filterRows(tree, "coût"))?.code_id).toBe("C-0005")
    expect(firstApplicable([])).toBeNull()
  })
})

describe("topCodes", () => {
  it("orders by roll-up passages then codebook order", () => {
    const withCounts = tree.map((c) => (c.code_id === "C-0005" ? { ...c, counts: { ...c.counts, passages_rollup: 9 } } : c))
    expect(topCodes(withCounts, 2).map((c) => c.code_id)).toEqual(["C-0005", "C-0001"])
  })
})
