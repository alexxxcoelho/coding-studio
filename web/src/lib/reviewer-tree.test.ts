import { describe, expect, it } from "vitest"
import { checkState, exportTree, toggleFields, type ReviewerField } from "./reviewer-tree"

const fields: ReviewerField[] = [
  {id:"text",path:["Interviews","I01","U01","text"],value:"Quote",selected_by_default:true},
  {id:"speaker",path:["Interviews","I01","U01","speaker"],value:"Private name",selected_by_default:false},
  {id:"memo",path:["Interviews","I02","memo"],value:"Notes",selected_by_default:true},
]
describe("reviewer field tree", () => {
  it("keeps each field independently selectable under its object", () => {
    const tree = exportTree(fields)
    expect(tree).toHaveLength(1)
    expect(tree[0].ids).toEqual(["text","speaker","memo"])
    expect(tree[0].children[0].children[0].children.map(n => n.field?.id)).toEqual(["text","speaker"])
  })
  it("shows a partial parent after one field is excluded", () => {
    const all = toggleFields(new Set(), fields.map(f => f.id), true)
    const partial = toggleFields(all, ["speaker"], false)
    expect(checkState(fields.map(f => f.id), partial)).toBe("mixed")
    expect(all.has("speaker")).toBe(true)
    expect(partial.has("speaker")).toBe(false)
    expect(checkState(["text"],partial)).toBe(true)
    expect(checkState(["speaker"],partial)).toBe(false)
  })
})
