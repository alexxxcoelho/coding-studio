import { describe, expect, it } from "vitest"

import type { PassageLink, LinkTarget } from "./analysis-api"
import { decodeDrag, encodeDrag, MIME_CODE, MIME_PASSAGE, moveItem, planCodeAdd, planLink, readDrag, reorderByDrop, writeDrag } from "./analysis-dnd"

const target: LinkTarget = { kind: "theme_part", theme_id: "T-0001", part: "mechanism_evidence" }
const brief = { passage_id: "P-1", interview_id: "ECO-07", unit_id: "ECO-07-U018", text: "Le lot suffit.", verification: null }
const link = (over: Partial<PassageLink> = {}): PassageLink => ({ link_id: "L-1", passage: brief, target, link_type: "supports", note: "", ...over })
const types = [
  { id: "supports", label: "soutient" },
  { id: "nuances", label: "nuance" },
]

function fakeTransfer() {
  const store = new Map<string, string>()
  return {
    setData: (k: string, v: string) => void store.set(k, v),
    getData: (k: string) => store.get(k) ?? "",
    types: [] as string[],
    effectAllowed: "",
    store,
  }
}

describe("drag payloads", () => {
  it("round-trip a passage and a code", () => {
    expect(decodeDrag(encodeDrag({ kind: "passage", id: "P-1" }), "passage")).toEqual({ kind: "passage", id: "P-1" })
    expect(decodeDrag(encodeDrag({ kind: "code", id: "C-0003" }), "code")).toEqual({ kind: "code", id: "C-0003" })
  })
  it("refuses a payload of another kind, garbage and nothing", () => {
    expect(decodeDrag(encodeDrag({ kind: "code", id: "C-1" }), "passage")).toBeNull()
    expect(decodeDrag("not json", "passage")).toBeNull()
    expect(decodeDrag("", "code")).toBeNull()
    expect(decodeDrag(null, "code")).toBeNull()
  })
  it("is written under a custom type and under text/plain, and read back", () => {
    const dt = fakeTransfer()
    writeDrag(dt, { kind: "passage", id: "P-9" })
    expect(dt.store.has(MIME_PASSAGE)).toBe(true)
    expect(dt.store.has("text/plain")).toBe(true)
    expect(readDrag(dt, "passage")).toEqual({ kind: "passage", id: "P-9" })
    expect(readDrag(dt, "code")).toBeNull()
    const other = fakeTransfer()
    writeDrag(other, { kind: "code", id: "C-2" })
    expect(other.store.has(MIME_CODE)).toBe(true)
  })
})

describe("planLink", () => {
  it("creates a link of the default type with an empty note", () => {
    const plan = planLink({ passageId: "P-2", target, links: [link()], linkTypes: types })
    expect(plan).toEqual({ action: "create", body: { passage_id: "P-2", target, link_type: "supports", note: "" } })
  })
  it("takes the first type of the study list, whatever its name", () => {
    const plan = planLink({ passageId: "P-2", target, links: [], linkTypes: [{ id: "mecanisme", label: "mécanisme" }] })
    expect(plan.action === "create" && plan.body.link_type).toBe("mecanisme")
  })
  it("does not link the same passage twice to the same part", () => {
    const plan = planLink({ passageId: "P-1", target, links: [link()], linkTypes: types })
    expect(plan).toMatchObject({ action: "duplicate" })
  })
  it("allows the same passage on another part, another theme or a question", () => {
    const other: LinkTarget = { kind: "theme_part", theme_id: "T-0001", part: "variations_divergent" }
    expect(planLink({ passageId: "P-1", target: other, links: [link()], linkTypes: types }).action).toBe("create")
    const t2: LinkTarget = { kind: "theme_part", theme_id: "T-0002", part: "mechanism_evidence" }
    expect(planLink({ passageId: "P-1", target: t2, links: [link()], linkTypes: types }).action).toBe("create")
    expect(planLink({ passageId: "P-1", target: { kind: "rq_part", rq_id: "Q1" }, links: [link()], linkTypes: types }).action).toBe("create")
  })
  it("refuses when the study has no link type", () => {
    expect(planLink({ passageId: "P-2", target, links: [], linkTypes: [] })).toMatchObject({ action: "refused" })
  })
})

describe("planCodeAdd", () => {
  it("adds a code once", () => {
    expect(planCodeAdd(["C-1"], "C-2")).toBe("add")
    expect(planCodeAdd(["C-1"], "C-1")).toBe("present")
  })
})

describe("reordering", () => {
  it("moves an item", () => {
    expect(moveItem(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"])
    expect(moveItem(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"])
    expect(moveItem(["a", "b"], 1, 9)).toEqual(["a", "b"])
  })
  it("reorders by dropping a theme on another: it takes the place of the target", () => {
    expect(reorderByDrop(["t1", "t2", "t3"], "t1", "t3")).toEqual(["t2", "t3", "t1"])
    expect(reorderByDrop(["t1", "t2", "t3"], "t3", "t1")).toEqual(["t3", "t1", "t2"])
  })
  it("keeps the order when the drop changes nothing", () => {
    expect(reorderByDrop(["t1", "t2"], "t1", "t1")).toEqual(["t1", "t2"])
    expect(reorderByDrop(["t1", "t2"], "x", "t1")).toEqual(["t1", "t2"])
  })
})
