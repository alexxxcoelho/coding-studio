// One turn of the transcript (design/macos/Coder.dc.html, .u): gutter stripes,
// number and time, the text in pieces, the codes applied under it.

import { useRef, type CSSProperties } from "react"

import { codeColor } from "@/lib/code-tree"
import { hasLater, hasRealCode, pieces, stripeCodes, unitState, type CoderUnit } from "@/lib/coder-model"
import { isWholePassage } from "@/lib/passage-cache"
import { rangeOffsets } from "@/lib/selection"
import type { Passage } from "@/lib/types"
import { cn } from "@/lib/utils"

/** Marker for the pending selection, drawn like a passage until it is coded. */
export const SELECTION_ID = "__selection"

export function selectionPassage(unitId: string, start: number, end: number): Passage {
  return {
    passage_id: SELECTION_ID,
    interview_id: "",
    unit_id: unitId,
    start,
    end,
    start_utf16: start,
    end_utf16: end,
    text: "",
    whole_unit: false,
    segmentation_hash: "",
    anchor_digest: "",
    created_at: "",
    created_by: "",
    kind: "highlight",
    stale: false,
    before: "",
    after: "",
    codings: [],
  }
}

/** "INT-01-U042" shows as "U042". */
export function shortId(unitId: string): string {
  const m = /(U\d+)$/.exec(unitId)
  return m ? m[1] : unitId
}

export interface TurnProps {
  unit: CoderUnit
  passages: Passage[]
  selected: boolean
  /** Name of a code by id, for the tags. */
  codeName: (id: string, fallback: string) => string
  pending: { start: number; end: number } | null
  currentPassage: string | null
  onSelect: () => void
  /** The reader selected text inside the turn (offsets in UTF-16 units of the turn text). */
  onRange: (range: { start: number; end: number }, rect: DOMRect) => void
  onPassage: (p: Passage) => void
}

export function Turn({ unit, passages, selected, codeName, pending, currentPassage, onSelect, onRange, onPassage }: TurnProps) {
  const textRef = useRef<HTMLSpanElement>(null)
  const state = unitState(unit, passages)
  const interviewer = state === "interviewer"
  const stripes = stripeCodes(passages)
  const inline = passages.filter((p) => !isWholePassage(p, unit.text.length))
  const all = pending ? [...inline, selectionPassage(unit.unit_id, pending.start, pending.end)] : inline
  const parts = pieces(unit.text, all)
  const codes = stripes
  const later = passages.some(hasLater)

  const handleMouseUp = () => {
    if (interviewer) return
    const sel = window.getSelection()
    const root = textRef.current
    if (!sel || sel.rangeCount === 0 || sel.isCollapsed || !root) return
    const range = sel.getRangeAt(0)
    const o = rangeOffsets(root, range, unit.text)
    if (!o) return
    onRange(o, range.getBoundingClientRect())
    sel.removeAllRanges()
  }

  return (
    <div
      className={cn("u", interviewer ? "int" : "resp", selected && "sel", state === "non_pertinent" && "na")}
      data-unit={unit.unit_id}
      data-state={state}
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
    >
      <div className="gut" aria-hidden="true">
        {codes.map((c) => (
          <i key={c.code_id} style={{ "--k": codeColor(c.code_id) } as CSSProperties} />
        ))}
      </div>
      <div className="meta">
        <span className={cn("mono", selected && "b")}>{shortId(unit.unit_id)}</span>
        <span>{unit.timestamp}</span>
      </div>
      <div className="txt" style={{ position: "relative" }}>
        {interviewer && <span className="who">INT</span>}
        <p style={{ display: interviewer ? "inline" : "block" }}>
          <span ref={textRef} data-unit-text onMouseUp={handleMouseUp}>
            {parts.map((piece) => {
              if (piece.passages.length === 0) return piece.text
              const coded = piece.passages.find(hasRealCode)
              const isSel = piece.passages.some((p) => p.passage_id === SELECTION_ID)
              const first = coded ?? piece.passages.find((p) => p.passage_id !== SELECTION_ID) ?? piece.passages[0]
              if (isSel && !coded) {
                return (
                  <span key={piece.start} className="seltext">
                    {piece.text}
                  </span>
                )
              }
              const color = coded ? codeColor(coded.codings.find((c) => !c.system)!.code_id) : undefined
              return (
                <span
                  key={piece.start}
                  className={cn(coded ? "mk" : "hl", currentPassage === first.passage_id && "cur", isSel && "seltext")}
                  style={color ? ({ "--k": color } as CSSProperties) : undefined}
                  data-passage={first.passage_id}
                  title={coded ? coded.codings.filter((c) => !c.system).map((c) => c.code_name).join(", ") : "Surlignage"}
                  onClick={(e) => {
                    if (window.getSelection()?.isCollapsed === false) return
                    e.stopPropagation()
                    onSelect()
                    onPassage(first)
                  }}
                >
                  {piece.text}
                </span>
              )
            })}
          </span>
        </p>
        {!interviewer && (
          <div className="applied">
            {codes.map((c) => (
              <span key={c.code_id} className="tag k" style={{ "--k": codeColor(c.code_id) } as CSSProperties}>
                <span className="sw" />
                {codeName(c.code_id, c.name)}
              </span>
            ))}
            {later && <span className="badge">à revoir</span>}
            {state === "non_pertinent" && <span className="badge">non pertinent</span>}
            {state === "remaining" && <span className="badge">non codée</span>}
            {selected && (
              <span className="hint">
                <span>
                  <span className="kbd">N</span> non pertinent
                </span>
                <span>
                  <span className="kbd">R</span> à revoir
                </span>
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
