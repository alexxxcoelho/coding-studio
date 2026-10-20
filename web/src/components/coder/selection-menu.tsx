// The menu that opens under a text selection (design/macos/Coder.dc.html,
// .menu.pop): search a code, the top codes, Surligner, Citer, and a new code
// made from the selection.

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react"

import { SearchIcon } from "@/components/icons"
import { t } from "@/i18n"
import { codeColor, codeName, isApplicable, topCodes } from "@/lib/code-tree"
import { useCodeSearchRows } from "@/lib/code-search"
import { matchedLine } from "@/lib/v2-text"
import { isMac } from "@/lib/nav"
import type { CodeNode } from "@/lib/types"

export function SelectionMenu({
  codes,
  keyOf,
  x,
  y,
  onPick,
  onHighlight,
  onQuote,
  onNewCode,
  onClose,
}: {
  codes: CodeNode[]
  /** Number key of a code in the inspector list, when it has one. */
  keyOf: (id: string) => number | undefined
  x: number
  y: number
  onPick: (c: CodeNode) => void
  onHighlight: () => void
  onQuote: () => void
  onNewCode: () => void
  onClose: () => void
}) {
  const [active, setActive] = useState(0)
  const [q, setQ] = useState("")
  const ref = useRef<HTMLDivElement>(null)
  const input = useRef<HTMLInputElement>(null)

  const found = useCodeSearchRows(codes, q, { limit: 20 })
  const list = useMemo(() => {
    if (q.trim()) return found.rows.map((r) => r.node).filter(isApplicable).slice(0, 8)
    return topCodes(codes, 3)
  }, [codes, q, found.rows])

  useEffect(() => {
    input.current?.focus()
    const away = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose()
    }
    document.addEventListener("mousedown", away)
    return () => document.removeEventListener("mousedown", away)
  }, [onClose])

  const pick = (c: CodeNode) => {
    onPick(c)
    setQ("")
    setActive(0)
    input.current?.focus()
  }

  return (
    <div ref={ref} className="menu" role="menu" aria-label={t("common.coder_la_selection")} style={{ position: "absolute", left: x, top: y, width: 270, zIndex: 20 }}>
      <label className="mi" style={{ color: "var(--label2)", cursor: "text" }}>
        <SearchIcon />
        <input
          ref={input}
          value={q}
          onChange={(e) => { setQ(e.target.value); setActive(0) }}
          placeholder={t("common.coder_la_selection_2")}
          aria-label={t("common.coder_la_selection")}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing || e.metaKey || e.ctrlKey || e.altKey) return
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault()
              e.stopPropagation()
              setActive(Math.max(0, Math.min(list.length - 1, active + (e.key === "ArrowDown" ? 1 : -1))))
            }
            if (e.key === "Enter" && list[Math.min(active, list.length - 1)]) {
              e.preventDefault()
              e.stopPropagation()
              pick(list[Math.min(active, list.length - 1)])
            }
            if (e.key === "Escape") {
              e.preventDefault()
              e.stopPropagation()
              onClose()
            }
          }}
          style={{ border: 0, outline: 0, background: "transparent", font: "inherit", color: "var(--label)", flex: 1, minWidth: 0 }}
        />
        <span className="r">/</span>
      </label>
      <div className="msep" />
      {list.length === 0 && (
        <div className="mi" style={{ color: "var(--label2)", cursor: "default" }}>
          {t("coder.selection_menu.aucun_code_ne_correspond")}
        </div>
      )}
      {list.map((c, index) => (
        <button key={c.code_id} type="button" role="menuitem" className="mi" onMouseMove={() => setActive(index)} style={{ background: index === Math.min(active, list.length - 1) ? "var(--fill)" : undefined, "--k": codeColor(c.code_id), height: found.matched.get(c.code_id) && matchedLine(found.matched.get(c.code_id)) ? 36 : undefined } as CSSProperties} onClick={() => pick(c)}>
          <span className="sw" style={{ background: "var(--k)" }} />
          <span style={{ display: "flex", flexDirection: "column", alignItems: "flex-start", minWidth: 0, lineHeight: 1.15 }}>
            <span>{codeName(c)}</span>
            {matchedLine(found.matched.get(c.code_id), 40) && <span className="small" style={{ opacity: 0.8 }}>{matchedLine(found.matched.get(c.code_id), 40)}</span>}
          </span>
          <span className="r">{keyOf(c.code_id) ?? ""}</span>
        </button>
      ))}
      <div className="msep" />
      <button type="button" role="menuitem" className="mi" onClick={onHighlight}>
        {t("coder.selection_menu.surligner")}<span className="r">H</span>
      </button>
      <button type="button" role="menuitem" className="mi" onClick={onQuote}>
        {t("coder.selection_menu.citer")}<span className="r">D</span>
      </button>
      <button type="button" role="menuitem" className="mi" onClick={onNewCode}>
        {t("common.nouveau_code_depuis_la_selection")}<span className="r">{isMac() ? "⌘N" : "Ctrl+N"}</span>
      </button>
    </div>
  )
}
