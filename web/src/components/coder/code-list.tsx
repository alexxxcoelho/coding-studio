// The Codes tab of the Coder inspector: a searchable tree of codes with their
// short definition and the number keys of the first nine, applied codes
// checked (design/macos/Coder.dc.html, .cl).

import type { CSSProperties, RefObject } from "react"
import { Check } from "lucide-react"

import { Input } from "@/components/ui/input"
import { codeColor, codeName, isApplicable, type TreeRow } from "@/lib/code-tree"
import type { CodeNode } from "@/lib/types"
import { cn } from "@/lib/utils"

/** First sentence of a definition, for the list. */
export function shortDefinition(d: string, max = 80): string {
  const first = d.split(/(?<=[.!?])\s/)[0] ?? d
  return first.length > max ? `${first.slice(0, max - 1).trimEnd()}…` : first
}

export function CodeList({
  rows,
  keys,
  applied,
  query,
  onQuery,
  searchRef,
  onPick,
  onPickLater,
  laterApplied,
  onEnter,
}: {
  rows: TreeRow[]
  keys: Map<string, number>
  applied: Set<string>
  query: string
  onQuery: (q: string) => void
  searchRef: RefObject<HTMLInputElement | null>
  onPick: (c: CodeNode) => void
  onPickLater: () => void
  laterApplied: boolean
  /** Enter in the search box: apply the first code of the list. */
  onEnter: () => void
}) {
  return (
    <>
      <div className="insp-in" style={{ paddingBottom: 6 }}>
        <Input
          ref={searchRef}
          type="text"
          placeholder="Chercher un code"
          aria-label="Chercher un code"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault()
              onEnter()
            }
          }}
        />
      </div>
      <ul className="cl" aria-label="Codes">
        {rows.length === 0 && <li className="l2" style={{ display: "block", cursor: "default" }}>Aucun code ne correspond.</li>}
        {rows.map(({ node, depth }) => {
          const on = applied.has(node.code_id)
          const usable = isApplicable(node)
          const system = node.system
          const k = keys.get(node.code_id)
          return (
            <li
              key={node.code_id}
              className={cn(on && "on", !usable && !system && "off")}
              style={{ "--k": codeColor(node.code_id), paddingLeft: 8 + depth * 18 } as CSSProperties}
              role="button"
              tabIndex={0}
              aria-pressed={system ? laterApplied : on}
              aria-label={`${codeName(node)}${k ? `, touche ${k}` : ""}`}
              onClick={() => (system ? onPickLater() : usable && onPick(node))}
              onKeyDown={(e) => {
                if (e.key === " " || e.key === "Enter") {
                  e.preventDefault()
                  if (system) onPickLater()
                  else if (usable) onPick(node)
                }
              }}
            >
              <span className="kbd" aria-hidden>
                {k ?? ""}
              </span>
              <span className="sw" style={{ marginTop: 3 }} />
              <div>
                <div className="nm">{codeName(node)}</div>
                {node.definition && <div className="df">{shortDefinition(node.definition)}</div>}
              </div>
              {(system ? laterApplied : on) ? (
                <Check className="size-3.5" style={{ color: "var(--accent-text)", marginTop: 2 }} role="img" aria-label="Appliqué" strokeWidth={2.2} />
              ) : (
                <span />
              )}
            </li>
          )
        })}
      </ul>
    </>
  )
}
