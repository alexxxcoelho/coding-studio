// The tree of groups of the register (left of Personnes), laid out as the tree
// of the Codebook: one row per group with its count (distinct persons of the
// group and of everything below it), a group is moved by dragging it onto its
// future parent or onto "Toutes les personnes" (the root), and from its form.

import { useState } from "react"

import { t } from "@/i18n"
import { flattenGroups, subtreeIds, type GroupNode } from "@/lib/persons-api"
import { ALL, sameSelection, type GroupSelection } from "@/lib/persons-selection"
import { cn } from "@/lib/utils"

export function GroupTree({
  groups,
  nPersons,
  nUngrouped,
  selection,
  onSelect,
  onMove,
}: {
  groups: GroupNode[]
  nPersons: number
  nUngrouped: number
  selection: GroupSelection
  onSelect: (s: GroupSelection) => void
  /** Moves `groupId` under `parentId` (null: to the root). */
  onMove: (groupId: string, parentId: string | null) => void
}) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [dragId, setDragId] = useState<string | null>(null)
  const [dropId, setDropId] = useState<string | null>(null)
  const flat = flattenGroups(groups)
  const byId = new Map(flat.map((g) => [g.group_id, g]))

  // Rows in display order, without the children of a collapsed group.
  const rows: GroupNode[] = []
  const walk = (list: GroupNode[]) => {
    for (const n of list) {
      rows.push(n)
      if (!collapsed.has(n.group_id)) walk(n.children)
    }
  }
  walk(groups)

  const toggle = (id: string) =>
    setCollapsed((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })

  /** A group may be dropped on any group that is not itself or below it. */
  const canDrop = (target: string | null): boolean => {
    if (!dragId) return false
    if (target === null) return byId.get(dragId)?.parent_id !== null
    const node = byId.get(dragId)
    if (!node || node.group_id === target || node.parent_id === target) return false
    return !subtreeIds(node).includes(target)
  }
  const drop = (target: string | null) => {
    const id = dragId
    setDragId(null)
    setDropId(null)
    if (id && canDrop(target)) onMove(id, target)
  }
  const over = (e: React.DragEvent, target: string | null, key: string) => {
    if (canDrop(target)) {
      e.preventDefault()
      setDropId(key)
    }
  }

  return (
    <table className="tbl ol nostripe" aria-label={t("persons.group_tree")}>
      <thead>
        <tr>
          <th>{t("persons.groups")}</th>
          <th style={{ textAlign: "right" }}>{t("persons.count_header")}</th>
        </tr>
      </thead>
      <tbody>
        <tr
          className={cn("row-click", sameSelection(selection, ALL) && "sel", dropId === "root" && "drop")}
          data-state={sameSelection(selection, ALL) ? "selected" : undefined}
          aria-selected={sameSelection(selection, ALL)}
          tabIndex={0}
          onClick={() => onSelect(ALL)}
          onKeyDown={(e) => e.key === "Enter" && onSelect(ALL)}
          onDragOver={(e) => over(e, null, "root")}
          onDragLeave={() => setDropId(null)}
          onDrop={(e) => {
            e.preventDefault()
            drop(null)
          }}
        >
          <td>{t("persons.all_persons")}</td>
          <td style={{ textAlign: "right" }}>{nPersons}</td>
        </tr>
        {rows.map((g) => {
          const on = selection.kind === "group" && selection.id === g.group_id
          const open = !collapsed.has(g.group_id)
          const hasChildren = g.children.length > 0
          return (
            <tr
              key={g.group_id}
              className={cn("row-click", on && "sel", dropId === g.group_id && "drop", g.depth > 0 && "child")}
              data-state={on ? "selected" : undefined}
              aria-selected={on}
              tabIndex={0}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData("text/plain", g.group_id)
                setDragId(g.group_id)
              }}
              onDragEnd={() => {
                setDragId(null)
                setDropId(null)
              }}
              onDragOver={(e) => over(e, g.group_id, g.group_id)}
              onDragLeave={() => setDropId(null)}
              onDrop={(e) => {
                e.preventDefault()
                drop(g.group_id)
              }}
              onClick={() => onSelect({ kind: "group", id: g.group_id })}
              onKeyDown={(e) => e.key === "Enter" && onSelect({ kind: "group", id: g.group_id })}
            >
              <td>
                <span className="nm" style={{ paddingLeft: g.depth * 16 }}>
                  <button
                    type="button"
                    className="tri"
                    aria-label={hasChildren ? (open ? t("persons.collapse_group", { name: g.name }) : t("persons.expand_group", { name: g.name })) : undefined}
                    disabled={!hasChildren}
                    aria-hidden={!hasChildren || undefined}
                    tabIndex={hasChildren ? undefined : -1}
                    onClick={(e) => {
                      e.stopPropagation()
                      toggle(g.group_id)
                    }}
                  >
                    {hasChildren && (
                      <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor" aria-hidden="true">
                        <path d={open ? "M1.5 3h7L5 7.5z" : "M3 1.5v7L7.5 5z"} />
                      </svg>
                    )}
                  </button>
                  <span style={{ minWidth: 0, overflowWrap: "anywhere" }}>{g.name}</span>
                </span>
              </td>
              <td style={{ textAlign: "right" }} title={g.n_persons_total !== g.n_persons ? t("persons.count_direct", { n: g.n_persons, total: g.n_persons_total }) : undefined}>
                {g.n_persons_total}
                {g.n_persons_total !== g.n_persons && <span className="small l2"> ({g.n_persons})</span>}
              </td>
            </tr>
          )
        })}
        <tr
          className={cn("row-click", selection.kind === "none" && "sel")}
          data-state={selection.kind === "none" ? "selected" : undefined}
          aria-selected={selection.kind === "none"}
          tabIndex={0}
          onClick={() => onSelect({ kind: "none" })}
          onKeyDown={(e) => e.key === "Enter" && onSelect({ kind: "none" })}
        >
          <td>{t("persons.no_group")}</td>
          <td style={{ textAlign: "right" }}>{nUngrouped}</td>
        </tr>
      </tbody>
    </table>
  )
}
