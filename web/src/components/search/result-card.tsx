// One turn found by the full-text search: where it is, whether it is coded, the
// text with the matches marked, the turns around it, and what to do with it:
// open it in the Coder, or code the selection (a match, or any words the
// reader selects) without leaving the screen.

import { useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeFinder } from "@/components/code-finder"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { useCodeName } from "@/lib/code-names"
import { invalidatePassages } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { unitHref } from "@/lib/router"
import { rangeOffsets } from "@/lib/selection"
import type { CodeNode } from "@/lib/types"
import type { SearchItem } from "@/lib/v2-api"
import { piecesAround } from "@/lib/v2-text"
import { cn } from "@/lib/utils"

const shortUnit = (id: string) => id.replace(/^.*-(U\d+)$/, "$1")
const brief = (t: string, n = 150) => (t.length > n ? `${t.slice(0, n - 1).trimEnd()}…` : t)

export function ResultCard({ item, codes, onCoded }: { item: SearchItem; codes: CodeNode[]; onCoded: () => void }) {
  const qc = useQueryClient()
  const nameOf = useCodeName()
  const textRef = useRef<HTMLSpanElement>(null)
  const ranges = item.matches.map((m) => ({ start: m.start_utf16, end: m.end_utf16 }))
  const [sel, setSel] = useState<{ start: number; end: number } | null>(null)
  const [open, setOpen] = useState(false)
  const chosen = sel ?? ranges[0] ?? null
  const chosenText = chosen ? item.text.slice(chosen.start, chosen.end) : ""
  const pieces = piecesAround(item.text, ranges)
  const interviewer = item.role === "interviewer"

  const code = useMutation({
    mutationFn: (c: CodeNode) => api.createPassage(item.interview_id, { unit_id: item.unit_id, start: chosen!.start, end: chosen!.end, code: c.code_id }),
    onSuccess: async (_, c) => {
      setOpen(false)
      await invalidatePassages(qc, item.interview_id)
      toast.success(t("search.result_card.brief_code_name", { brief: brief(chosenText, 50), name: c.name }))
      onCoded()
    },
    onError: (e) => reportError(e, t("search.result_card.passage_non_code")),
  })

  const onMouseUp = () => {
    const s = window.getSelection()
    const root = textRef.current
    if (!s || s.rangeCount === 0 || s.isCollapsed || !root) return
    const o = rangeOffsets(root, s.getRangeAt(0), item.text)
    if (o) setSel(o)
  }

  return (
    <article className="grp" style={{ padding: "10px 12px", display: "flex", flexDirection: "column", gap: 6 }} aria-label={`${item.interview_id} ${shortUnit(item.unit_id)}`} data-unit={item.unit_id}>
      <div className="r8">
        <a className="mono b" href={unitHref(item.unit_id)} title={t("common.ouvrir_dans_coder")}>
          {item.interview_id} {shortUnit(item.unit_id)}
        </a>
        <span className="small l2">{interviewer ? t("search.result_card.intervieweur") : t("search.result_card.repondant")}</span>
        <span className="badge">
          {t("search.result_card.n_occurrences", { n: item.matches.length })}
        </span>
        {item.coded ? (
          <span className="r8" aria-label={t("search.result_card.code")}>
            {item.code_ids.length === 0 && <span className="badge">{t("search.result_card.code_2")}</span>}
            {item.code_ids.map((id) => (
              <span key={id} className="tag">
                {nameOf(id)}
              </span>
            ))}
          </span>
        ) : (
          <span className="badge">{t("search.result_card.non_code")}</span>
        )}
        {item.coded && item.uncovered_matches > 0 && (
          <span className="small l2">
            {t("search.result_card.dont_n_occurrences_hors_passage", { n: item.uncovered_matches })}
          </span>
        )}
      </div>
      {item.previous && (
        <p className="small l2" style={{ margin: 0 }} data-no-i18n>
          <span className="mono">{shortUnit(item.previous.unit_id)}</span> {item.previous.speaker} : {brief(item.previous.text)}
        </p>
      )}
      <p className="verbatim" style={{ margin: 0, whiteSpace: "pre-wrap" }} data-no-i18n>
        <span ref={textRef} data-unit-text onMouseUp={onMouseUp}>
          {pieces.map((p, i) =>
            p.hit < 0 ? (
              p.text
            ) : (
              <mark
                key={i}
                className={cn("hl", chosen && ranges[p.hit] && chosen.start === ranges[p.hit].start && chosen.end === ranges[p.hit].end && "cur")}
                title={t("search.result_card.choisir_cette_occurrence")}
                onClick={() => setSel(ranges[p.hit])}
              >
                {p.text}
              </mark>
            ),
          )}
        </span>
      </p>
      {item.next && (
        <p className="small l2" style={{ margin: 0 }} data-no-i18n>
          <span className="mono">{shortUnit(item.next.unit_id)}</span> {item.next.speaker} : {brief(item.next.text)}
        </p>
      )}
      <div className="r8">
        <Button asChild variant="outline" size="sm">
          <a href={unitHref(item.unit_id)}>{t("common.ouvrir_dans_coder")}</a>
        </Button>
        {!interviewer && chosen && (
          <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
              <Button variant="outline" size="sm" title={t("search.result_card.coder_la_selection_brief", { brief: brief(chosenText, 60) })}>
                {t("common.coder_la_selection_2")}
              </Button>
            </PopoverTrigger>
            <PopoverContent align="start" className="w-80" aria-label={t("common.coder_la_selection")}>
              <span className="small l2">
                {t("search.result_card.selection_brief_cliquez_une_occurren", { brief: brief(chosenText, 80) })}
              </span>
              <CodeFinder codes={codes} applicableOnly autoFocus onPick={(c) => code.mutate(c)} label={t("search.result_card.code_a_poser_sur_la")} />
            </PopoverContent>
          </Popover>
        )}
        {sel && (
          <Button variant="ghost" size="sm" onClick={() => setSel(null)}>
            {t("search.result_card.revenir_a_la_premiere_occurrence")}
          </Button>
        )}
      </div>
    </article>
  )
}
