// Versions of an interview's text and the review of the passages that a
// correction put in doubt: the old reading, the new text, the span the server
// proposes, and three gestures: Accepter, Ajuster (select the span in the
// current text) or Détacher.

import { useRef, useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { toast } from "sonner"

import { Alerts } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { dateTime } from "@/lib/format"
import { rangeOffsets, type Offsets } from "@/lib/selection"
import { trKeys, useRefreshReview } from "@/lib/transcript-review"
import { v2, type ProjectionItem, type ProjectionReason, type TranscriptVersion } from "@/lib/v2-api"
import { explainReviewError, piecesAround } from "@/lib/v2-text"

const CAUSE: Record<TranscriptVersion["cause"], string> = {
  get baseline() { return t("coder.review_dialog.texte_importe") },
  get segmentation() { return t("common.segmentation") },
  get correction() { return t("coder.review_dialog.correction_d_un_tour") },
}

const REASON: Record<string, string> = {
  get text_changed() { return t("coder.review_dialog.le_texte_du_tour_a") },
  get unit_moved() { return t("coder.review_dialog.le_tour_a_change_de") },
  get unit_removed() { return t("coder.review_dialog.le_tour_n_existe_plus") },
  get unit_became_interviewer() { return t("coder.review_dialog.le_tour_est_devenu_un") },
}

export function reasonLabel(r: ProjectionReason | null): string {
  return (r && REASON[r]) || t("coder.review_dialog.le_texte_a_change")
}

const shortUnit = (id: string) => id.replace(/^.*-(U\d+)$/, "$1")

function Marked({ text, range, label }: { text: string; range: { start: number; end: number } | null; label: string }) {
  const pieces = range ? piecesAround(text, [range]) : [{ text, hit: -1 }]
  return (
    <p className="verbatim" style={{ margin: 0, whiteSpace: "pre-wrap" }} aria-label={label} data-no-i18n>
      {pieces.map((p, i) => (p.hit < 0 ? p.text : <mark key={i} className="hl">{p.text}</mark>))}
    </p>
  )
}

function ReviewCard({ item, interviewId }: { item: ProjectionItem; interviewId: string }) {
  const refresh = useRefreshReview(interviewId)
  const [adjusting, setAdjusting] = useState(false)
  const [sel, setSel] = useState<Offsets | null>(null)
  const textRef = useRef<HTMLSpanElement>(null)
  const done = (msg: string) => async () => {
    await refresh()
    toast.success(msg)
  }
  const fail = (e: unknown) => toast.error(explainReviewError(e))
  const accept = useMutation({ mutationFn: () => v2.acceptProjection(item.projection_id), onSuccess: done(t("coder.review_dialog.emplacement_propose_accepte")), onError: fail })
  const detach = useMutation({ mutationFn: () => v2.detachProjection(item.projection_id), onSuccess: done(t("coder.review_dialog.passage_detache")), onError: fail })
  const adjust = useMutation({
    mutationFn: () => v2.adjustProjection(item.projection_id, { start: sel!.start, end: sel!.end, offset_unit: "utf16" }),
    onSuccess: async () => {
      setAdjusting(false)
      await done(t("coder.review_dialog.passage_ajuste"))()
    },
    onError: fail,
  })
  const busy = accept.isPending || detach.isPending || adjust.isPending
  const unitText = item.current_unit_text
  const proposed = item.proposed ? { start: item.proposed.start_utf16, end: item.proposed.end_utf16 } : null
  const onMouseUp = () => {
    const s = window.getSelection()
    if (!s || s.rangeCount === 0 || s.isCollapsed || !textRef.current) return
    const o = rangeOffsets(textRef.current, s.getRangeAt(0), unitText)
    if (o) setSel(o)
  }

  return (
    <li className="grp" style={{ padding: "10px 12px", display: "flex", flexDirection: "column", gap: 6 }} data-projection={item.projection_id}>
      <div className="r8">
        <span className="mono b">
          {shortUnit(item.old.unit_id)}
        </span>
        <span className="small l2">{reasonLabel(item.reason)}</span>
        {item.codes.map((c) => (
          <span key={c.code_id} className="tag">
            {c.code_name}
          </span>
        ))}
      </div>
      <div className="small l2">{t("coder.review_dialog.ancien_passage")}</div>
      <Marked text={item.old.unit_text} range={{ start: item.old.start_utf16, end: item.old.end_utf16 }} label={t("coder.review_dialog.ancien_texte_du_passage")} />
      <div className="small l2">{adjusting ? t("coder.review_dialog.texte_actuel_selectionnez_le_nouvel") : t("coder.review_dialog.texte_actuel")}</div>
      {adjusting ? (
        <p className="verbatim" style={{ margin: 0, whiteSpace: "pre-wrap" }} data-no-i18n>
          <span ref={textRef} onMouseUp={onMouseUp} data-unit-text aria-label={t("coder.review_dialog.texte_actuel_du_tour")}>
            {unitText}
          </span>
        </p>
      ) : (
        <Marked text={unitText} range={proposed} label={t("coder.review_dialog.texte_actuel_du_tour")} />
      )}
      {item.proposed ? (
        <p className="small l2" style={{ margin: 0 }}>
          {t("coder.review_dialog.emplacement_propose_text_methode", { text: item.proposed.text })} {item.method === "diff" ? t("coder.review_dialog.comparaison_des_textes") : item.method === "moved_unit" ? t("coder.review_dialog.tour_deplace") : t("coder.review_dialog.recherche_du_texte")}).
        </p>
      ) : (
        <p className="small l2" style={{ margin: 0 }}>
          {t("coder.review_dialog.aucun_emplacement_ne_convient_ajuste")}
        </p>
      )}
      <div className="r8">
        {!adjusting && (
          <>
            <Button size="sm" disabled={busy || !item.proposed} onClick={() => accept.mutate()}>
              {t("coder.review_dialog.accepter")}
            </Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => { setSel(null); setAdjusting(true) }}>
              {t("coder.review_dialog.ajuster")}
            </Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => detach.mutate()} title={t("coder.review_dialog.le_passage_sort_des_listes")}>
              {t("coder.review_dialog.detacher")}
            </Button>
          </>
        )}
        {adjusting && (
          <>
            <Button size="sm" disabled={busy || !sel} onClick={() => adjust.mutate()}>
              {t("coder.review_dialog.valider_la_selection")}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setAdjusting(false)}>
              {t("common.annuler")}
            </Button>
            <span className="small l2">{sel ? `« ${unitText.slice(sel.start, sel.end)} »` : t("coder.review_dialog.selectionnez_des_mots_dans_le")}</span>
          </>
        )}
      </div>
    </li>
  )
}

function VersionList({ interviewId }: { interviewId: string }) {
  const q = useQuery({ queryKey: trKeys.versions(interviewId), queryFn: () => v2.versions(interviewId) })
  if (q.isLoading) return <Skeleton className="h-16 w-full" />
  if (q.error) return <Alerts messages={[q.error.message]} />
  const vs = [...(q.data?.versions ?? [])].reverse()
  return (
    <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 6 }} aria-label={t("coder.review_dialog.versions_du_texte")}>
      {vs.map((v) => (
        <li key={v.version} className="grp" style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 2 }}>
          <div className="r8">
            <span className="b">{t("coder.review_dialog.version_version", { version: v.version })}</span>
            {v.version === q.data?.current && <span className="badge">{t("coder.review_dialog.actuelle")}</span>}
            <span>{CAUSE[v.cause]}</span>
            {v.target && <span className="small l2 mono">{shortUnit(v.target)}</span>}
          </div>
          <span className="small l2">
            {t("coder.review_dialog.datetime_created_by_n_tours", { n: v.n_units, dateTime: dateTime(v.created_at), created_by: v.created_by })}
            {v.reanchored > 0 && t("coder.review_dialog.n_passages_replaces_seuls", { n: v.reanchored })}
            {v.flagged > 0 && t("coder.review_dialog.flagged_a_revoir", { flagged: v.flagged })}
          </span>
          {v.reason && <span className="small l2">{t("coder.review_dialog.motif_reason", { reason: v.reason })}</span>}
        </li>
      ))}
    </ul>
  )
}

export function ReviewDialog({ interviewId, open, onClose }: { interviewId: string; open: boolean; onClose: () => void }) {
  const q = useQuery({
    queryKey: trKeys.items(interviewId),
    queryFn: () => v2.review({ interview_id: interviewId, status: "needs_review" }),
    enabled: open,
  })
  const items = q.data?.items ?? []
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent style={{ maxWidth: 680, maxHeight: "86vh", overflowY: "auto" }}>
        <DialogHeader>
          <DialogTitle>{t("common.versions_du_texte_et_passages")}</DialogTitle>
          <DialogDescription>
            {t("coder.review_dialog.quand_le_texte_d_un")}
          </DialogDescription>
        </DialogHeader>
        <div className="hs">{t("common.a_revoir")}{items.length > 0 ? ` (${items.length})` : ""}</div>
        {q.isLoading && <Skeleton className="h-20 w-full" />}
        {q.error && <Alerts messages={[q.error.message]} />}
        {q.data && items.length === 0 && <p className="small l2" style={{ margin: 0 }}>{t("coder.review_dialog.aucun_passage_a_revoir")}</p>}
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 8 }} aria-label={t("coder.review_dialog.passages_a_revoir")}>
          {items.map((it) => (
            <ReviewCard key={it.projection_id} item={it} interviewId={interviewId} />
          ))}
        </ul>
        <div className="hs">{t("common.versions")}</div>
        <VersionList interviewId={interviewId} />
      </DialogContent>
    </Dialog>
  )
}
