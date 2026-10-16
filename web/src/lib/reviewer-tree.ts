export interface ReviewerField { id: string; path: string[]; value: unknown; selected_by_default: boolean }
export interface ReviewerCatalogue { snapshot: string; fields: ReviewerField[]; file_ids: Record<string, number>; pseudonym_rules: number; coverage?: { interviews: number; transcripts: number } }
export interface ExportNode { key: string; label: string; ids: string[]; children: ExportNode[]; field?: ReviewerField }

export function exportTree(fields: ReviewerField[]): ExportNode[] {
  const root: ExportNode = { key: "", label: "", ids: [], children: [] }
  const nodes = new Map<string, ExportNode>([["", root]])
  for (const field of fields) {
    let parent = root
    for (let i = 0; i < field.path.length; i++) {
      const key = JSON.stringify(field.path.slice(0, i + 1))
      let node = nodes.get(key)
      if (!node) {
        node = { key, label: field.path[i], ids: [], children: [] }
        nodes.set(key, node); parent.children.push(node)
      }
      node.ids.push(field.id); parent = node
    }
    parent.field = field
  }
  for (const field of fields) {
    if (field.path[0] === "Source files" && field.path.at(-1) === "name" && typeof field.value === "string") {
      const node = nodes.get(JSON.stringify(field.path.slice(0,-1)))
      if (node) node.label = field.value
    }
  }
  return root.children
}

export function checkState(ids: string[], selected: Set<string>): boolean | "mixed" {
  const count = ids.reduce((sum, id) => sum + Number(selected.has(id)), 0)
  return count === 0 ? false : count === ids.length ? true : "mixed"
}

export function toggleFields(selected: Set<string>, ids: string[], on: boolean): Set<string> {
  const result = new Set(selected)
  for (const id of ids) { if (on) result.add(id); else result.delete(id) }
  return result
}
