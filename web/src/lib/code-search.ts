// Code search for every picker (Coder list, selection menu, Codebook filter):
// the server's GET /api/codes/search answers by name, alias, old handle, label
// and definition and says which field matched. While the answer is on its way
// (or if it fails) the screen filters the codes it already holds, so that typing
// never waits for the network.

import { useEffect, useMemo, useState } from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"

import { buildRows, filterRows, rowsForIds, type TreeRow } from "./code-tree"
import type { CodeNode } from "./types"
import { v2, type CodeSearchItem } from "./v2-api"

export function useDebounced<T>(value: T, ms = 150): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms)
    return () => clearTimeout(t)
  }, [value, ms])
  return v
}

export interface CodeSearchRows {
  rows: TreeRow[]
  /** Why a code is in the list: the field that matched, by code id. Empty without a query or before the answer. */
  matched: Map<string, CodeSearchItem["matched"]>
  /** Names of the ancestors by code id (flat results show them). */
  paths: Map<string, string[]>
  /** True when the rows come from the server's answer to the query as typed. */
  fromServer: boolean
}

export interface CodeSearchOptions {
  /** `flat`: the results in rank order, one level (a picker). `tree`: matches under their ancestors, in codebook order (the Codebook). */
  layout?: "flat" | "tree"
  collapsed?: ReadonlySet<string>
  limit?: number
}

const NONE = new Map<string, never>()

export function useCodeSearchRows(codes: CodeNode[], query: string, opts: CodeSearchOptions = {}): CodeSearchRows {
  const { layout = "flat", collapsed, limit = 200 } = opts
  const typed = query.trim()
  const settled = useDebounced(typed, 150)
  const q = useQuery({
    queryKey: ["code-search", settled, limit],
    queryFn: () => v2.codeSearch(settled, { limit, include_inactive: true, include_system: true }),
    enabled: settled !== "",
    staleTime: 10_000,
    placeholderData: keepPreviousData,
  })
  const answer = q.data && q.data.query === typed ? q.data : null

  return useMemo<CodeSearchRows>(() => {
    if (typed === "") return { rows: buildRows(codes, collapsed), matched: NONE, paths: NONE, fromServer: false }
    if (!answer) return { rows: filterRows(codes, typed, collapsed), matched: NONE, paths: NONE, fromServer: false }
    const byId = new Map(codes.map((c) => [c.code_id, c]))
    const items = answer.items.filter((i) => byId.has(i.code_id))
    const matched = new Map(items.map((i) => [i.code_id, i.matched]))
    const paths = new Map(items.map((i) => [i.code_id, i.path]))
    const rows =
      layout === "tree"
        ? rowsForIds(codes, new Set(items.map((i) => i.code_id)))
        : items.map<TreeRow>((i) => ({ node: byId.get(i.code_id)!, depth: 0, hasChildren: false, parentId: null }))
    return { rows, matched, paths, fromServer: true }
  }, [codes, typed, answer, layout, collapsed])
}
