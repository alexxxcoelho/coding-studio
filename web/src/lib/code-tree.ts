// The code tree of /api/codes: nesting, filtering, colours and number keys.
// Pure functions; the screens only render what they return.

import type { CodeNode } from "./types"

/** The reserved system code "Plus tard". */
export const LATER_ID = "LATER"

/** Lower case without accents, for matching. */
export function fold(s: string): string {
  return s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase()
}

/** A code that can be applied to a passage: active, not merged away, not the system code. */
export function isApplicable(c: CodeNode): boolean {
  return c.active && !c.merged_into && !c.system && c.code_id !== LATER_ID
}

/** A code shown in the tree: merged codes are history, not tree members. */
export function isListed(c: CodeNode): boolean {
  return !c.merged_into
}

export interface TreeRow {
  node: CodeNode
  depth: number
  hasChildren: boolean
  /** True when a collapsed ancestor hides the row (rows are still returned when asked). */
  parentId: string | null
}

/**
 * Depth-first rows of the tree, in codebook order, children under their parent
 * and the system code last. A code whose parent is missing or merged away is
 * shown at the top level. Codes in `collapsed` hide their descendants.
 */
export function buildRows(codes: CodeNode[], collapsed: ReadonlySet<string> = new Set()): TreeRow[] {
  const listed = codes.filter(isListed)
  const ids = new Set(listed.map((c) => c.code_id))
  const children = new Map<string | null, CodeNode[]>()
  for (const c of listed) {
    const key = c.parent_id && ids.has(c.parent_id) && c.parent_id !== c.code_id ? c.parent_id : null
    const list = children.get(key) ?? []
    list.push(c)
    children.set(key, list)
  }
  const rows: TreeRow[] = []
  const seen = new Set<string>()
  const walk = (parent: string | null, depth: number) => {
    const list = children.get(parent) ?? []
    const ordered = parent === null ? [...list.filter((c) => !c.system), ...list.filter((c) => c.system)] : list
    for (const c of ordered) {
      if (seen.has(c.code_id)) continue
      seen.add(c.code_id)
      const kids = children.get(c.code_id) ?? []
      rows.push({ node: c, depth, hasChildren: kids.length > 0, parentId: parent })
      if (!collapsed.has(c.code_id)) walk(c.code_id, depth + 1)
    }
  }
  walk(null, 0)
  return rows
}

/** Text a filter looks into: name, label, token, definition, aliases. */
function haystack(c: CodeNode): string {
  return fold([c.name, c.label, c.token, c.definition, ...c.aliases.map((a) => a.alias)].join("\n"))
}

/**
 * Rows matching `query` (words, all required), with the ancestors of every
 * match so that a sub-code is never shown without its parent. An empty query
 * keeps the whole tree.
 */
export function filterRows(codes: CodeNode[], query: string, collapsed: ReadonlySet<string> = new Set()): TreeRow[] {
  const words = fold(query).split(/\s+/).filter(Boolean)
  if (words.length === 0) return buildRows(codes, collapsed)
  const byId = new Map(codes.map((c) => [c.code_id, c]))
  const keep = new Set<string>()
  for (const c of codes) {
    if (!isListed(c)) continue
    const h = haystack(c)
    if (!words.every((w) => h.includes(w))) continue
    let cur: CodeNode | undefined = c
    while (cur && !keep.has(cur.code_id)) {
      keep.add(cur.code_id)
      cur = cur.parent_id ? byId.get(cur.parent_id) : undefined
    }
  }
  // Searching opens the tree: collapsed state is ignored.
  return buildRows(codes.filter((c) => keep.has(c.code_id)), new Set<string>())
}

/** Ids of every descendant of `id`. */
export function descendantIds(codes: CodeNode[], id: string): Set<string> {
  const out = new Set<string>()
  const stack = [id]
  while (stack.length) {
    const cur = stack.pop()!
    for (const c of codes) {
      if (c.parent_id === cur && !out.has(c.code_id)) {
        out.add(c.code_id)
        stack.push(c.code_id)
      }
    }
  }
  return out
}

/** Parents a code may move under: not itself, not a descendant, applicable-or-not but never merged or the system code. */
export function possibleParents(codes: CodeNode[], id: string): CodeNode[] {
  const banned = descendantIds(codes, id)
  banned.add(id)
  return codes.filter((c) => isListed(c) && !c.system && !banned.has(c.code_id))
}

/** True when moving `id` under `parent` would make a cycle. */
export function wouldCycle(codes: CodeNode[], id: string, parent: string | null): boolean {
  if (parent === null) return false
  if (parent === id) return true
  return descendantIds(codes, id).has(parent)
}

// ------------------------------------------------------------- colours

/** The seven code colours of the design tokens (--c-*), in the order of macos.css. */
export const CODE_COLORS = ["legal", "partner", "retail", "cost", "proc", "need", "skill"] as const

/** Position of a code among the colours, stable for the life of its id (C-0007 is always the same colour). */
export function colorIndex(codeId: string): number {
  const m = /(\d+)\s*$/.exec(codeId)
  if (m) return (Number(m[1]) - 1 + CODE_COLORS.length * 1000) % CODE_COLORS.length
  let h = 0
  for (const ch of codeId) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return h % CODE_COLORS.length
}

/** CSS colour of a code. The system code is neutral. */
export function codeColor(codeId: string): string {
  if (codeId === LATER_ID) return "var(--label3)"
  return `var(--c-${CODE_COLORS[colorIndex(codeId)]})`
}

// ----------------------------------------------------------- number keys

/** The first nine applicable rows get the keys 1 to 9, in the order shown. */
export function numberKeys(rows: TreeRow[]): Map<string, number> {
  const out = new Map<string, number>()
  for (const r of rows) {
    if (out.size >= 9) break
    if (isApplicable(r.node)) out.set(r.node.code_id, out.size + 1)
  }
  return out
}

/** The code under key `n` (1 to 9) among the visible rows. */
export function codeForNumber(rows: TreeRow[], n: number): CodeNode | null {
  for (const [id, k] of numberKeys(rows)) if (k === n) return rows.find((r) => r.node.code_id === id)?.node ?? null
  return null
}

/** Best match for a typed search: the first applicable row of the filter. */
export function firstApplicable(rows: TreeRow[]): CodeNode | null {
  return rows.find((r) => isApplicable(r.node))?.node ?? null
}

/** Codes most used first, then codebook order: "top codes" of the selection menu. */
export function topCodes(codes: CodeNode[], n = 3): CodeNode[] {
  return codes
    .map((c, i) => ({ c, i }))
    .filter(({ c }) => isApplicable(c))
    .sort((a, b) => b.c.counts.passages_rollup - a.c.counts.passages_rollup || a.i - b.i)
    .slice(0, n)
    .map(({ c }) => c)
}

/** Name of a code for lists: its name, else its label, else its id. */
export function codeName(c: CodeNode): string {
  return c.name || c.label || c.code_id
}
