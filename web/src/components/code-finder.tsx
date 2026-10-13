// A search box over the codes with its result list: the picker of the Recherche
// screen (code filter, "Coder la sélection"). It uses GET /api/codes/search through
// useCodeSearchRows and says which field matched.

import { useRef, useState, type CSSProperties } from "react"

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
  const [q, setQ] = useState("")
  const found = useCodeSearchRows(codes, q, { limit: 30 })
  const list = found.rows
    .map((r) => r.node)
    .filter((c) => !c.system && (applicableOnly ? isApplicable(c) : !c.merged_into))
    .slice(0, q.trim() ? max : 6)
  const input = useRef<HTMLInputElement>(null)
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}>
      <Input
        ref={input}
        value={q}
        autoFocus={autoFocus}
        onChange={(e) => setQ(e.target.value)}
        placeholder={placeholder}
        aria-label={label}
        onKeyDown={(e) => {
          if (e.key === "Enter" && list[0]) {
            e.preventDefault()
            onPick(list[0])
          }
        }}
      />
      <ul className="cl" aria-label={t("code_finder.codes_trouves")} style={{ padding: 0 }}>
        {list.length === 0 && <li className="l2" style={{ display: "block", cursor: "default" }}>{t("common.aucun_code_ne_correspond")}</li>}
        {list.map((c) => {
          const why = matchedLine(found.matched.get(c.code_id), 60)
          const path = found.paths.get(c.code_id) ?? []
          return (
            <li
              key={c.code_id}
              role="button"
              tabIndex={0}
              aria-label={codeName(c)}
              style={{ "--k": codeColor(c.code_id), gridTemplateColumns: "10px minmax(0,1fr)" } as CSSProperties}
              onClick={() => onPick(c)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault()
                  onPick(c)
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
