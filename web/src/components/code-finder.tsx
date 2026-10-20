// A search box over the codes with its result list: the picker of the Recherche
// screen (code filter, "Coder la sélection"). It uses GET /api/codes/search through
// useCodeSearchRows and says which field matched.

import { useId, useRef, useState, type CSSProperties } from "react"

import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { useCodeSearchRows } from "@/lib/code-search"
import { codeColor, codeName, isApplicable } from "@/lib/code-tree"
import type { CodeNode } from "@/lib/types"
import { matchedLine } from "@/lib/v2-text"

export function CodeFinder({
  codes,
  onPick,
  applicableOnly = false,
  placeholder = t("common.chercher_un_code"),
  label = t("common.chercher_un_code"),
  autoFocus,
  max = 8,
}: {
  codes: CodeNode[]
  onPick: (c: CodeNode) => void
  /** Only the codes that can be put on a passage. */
  applicableOnly?: boolean
  placeholder?: string
  label?: string
  autoFocus?: boolean
  max?: number
}) {
  const listId = useId()
  const [active, setActive] = useState(0)
  const [q, setQ] = useState("")
  const found = useCodeSearchRows(codes, q, { limit: 30 })
  const list = found.rows
    .map((r) => r.node)
    .filter((c) => !c.system && (applicableOnly ? isApplicable(c) : !c.merged_into))
    .slice(0, q.trim() ? max : 6)
  const input = useRef<HTMLInputElement>(null)
  const pick = (c: CodeNode) => {
    onPick(c)
    setQ("")
    setActive(0)
    input.current?.focus()
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}>
      <Input
        ref={input}
        value={q}
        autoFocus={autoFocus}
        onChange={(e) => { setQ(e.target.value); setActive(0) }}
        role="combobox"
        aria-expanded={true}
        aria-autocomplete="list"
        aria-controls={listId}
        aria-activedescendant={list.length ? `${listId}-${Math.min(active, list.length - 1)}` : undefined}
        placeholder={placeholder}
        aria-label={label}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing || e.metaKey || e.ctrlKey || e.altKey) return
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault()
            e.stopPropagation()
            const next = Math.max(0, Math.min(list.length - 1, active + (e.key === "ArrowDown" ? 1 : -1)))
            setActive(next)
            document.getElementById(`${listId}-${next}`)?.scrollIntoView?.({ block: "nearest" })
          }
          if (e.key === "Enter" && list[Math.min(active, list.length - 1)]) {
            e.preventDefault()
            e.stopPropagation()
            pick(list[Math.min(active, list.length - 1)])
          }
        }}
      />
      <ul id={listId} role="listbox" className="cl" aria-label={t("code_finder.codes_trouves")} style={{ padding: 0, maxHeight: "55vh", overflowY: "auto" }}>
        {list.length === 0 && <li className="l2" style={{ display: "block", cursor: "default" }}>{t("common.aucun_code_ne_correspond")}</li>}
        {list.map((c, index) => {
          const why = matchedLine(found.matched.get(c.code_id), 60)
          const path = found.paths.get(c.code_id) ?? []
          return (
            <li
              key={c.code_id}
              id={`${listId}-${index}`}
              role="option"
              aria-selected={index === Math.min(active, list.length - 1)}
              tabIndex={0}
              aria-label={codeName(c)}
              onMouseMove={() => setActive(index)}
              style={{ background: index === Math.min(active, list.length - 1) ? "var(--fill)" : undefined, outline: index === Math.min(active, list.length - 1) ? "1px solid var(--tint)" : undefined, "--k": codeColor(c.code_id), gridTemplateColumns: "10px minmax(0,1fr)" } as CSSProperties}
              onClick={() => pick(c)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault()
                  pick(c)
                }
              }}
            >
              <span className="sw" style={{ marginTop: 3 }} />
              <div>
                <div className="nm">
                  {codeName(c)}
                  {path.length > 0 && <span className="l3 small" style={{ fontWeight: 400 }}> {t("common.dans_join", { join: path.join(" › ") })}</span>}
                </div>
                {why && <div className="df">{why}</div>}
              </div>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
