// The menu that opens under a text selection (design/macos/Coder.dc.html,
// .menu.pop): search a code, the top codes, Surligner, Citer, and a new code
// made from the selection.

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react"

import { SearchIcon } from "@/components/icons"
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

  return (
    <div ref={ref} className="menu" role="menu" aria-label="Coder la sélection" style={{ position: "absolute", left: x, top: y, width: 270, zIndex: 20 }}>
      <label className="mi" style={{ color: "var(--label2)", cursor: "text" }}>
        <SearchIcon />
        <input
          ref={input}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Coder la sélection…"
          aria-label="Coder la sélection"
          onKeyDown={(e) => {
            if (e.key === "Enter" && list[0]) {
              e.preventDefault()
              onPick(list[0])
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
          Aucun code ne correspond
        </div>
      )}
      {list.map((c) => (
        <button key={c.code_id} type="button" role="menuitem" className="mi" style={{ "--k": codeColor(c.code_id), height: found.matched.get(c.code_id) && matchedLine(found.matched.get(c.code_id)) ? 36 : undefined } as CSSProperties} onClick={() => onPick(c)}>
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
        Surligner<span className="r">H</span>
      </button>
      <button type="button" role="menuitem" className="mi" onClick={onQuote}>
        Citer…<span className="r">D</span>
      </button>
      <button type="button" role="menuitem" className="mi" onClick={onNewCode}>
        Nouveau code depuis la sélection…<span className="r">{isMac() ? "⌘N" : "Ctrl+N"}</span>
      </button>
    </div>
  )
}
