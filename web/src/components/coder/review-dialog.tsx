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
import { dateTime } from "@/lib/format"
import { rangeOffsets, type Offsets } from "@/lib/selection"
import { trKeys, useRefreshReview } from "@/lib/transcript-review"
import { v2, type ProjectionItem, type ProjectionReason, type TranscriptVersion } from "@/lib/v2-api"
import { explainReviewError, piecesAround } from "@/lib/v2-text"

const CAUSE: Record<TranscriptVersion["cause"], string> = {
  baseline: "Texte importé",
  segmentation: "Segmentation",
  correction: "Correction d'un tour",
}

const REASON: Record<string, string> = {
  text_changed: "Le texte du tour a changé.",
  unit_moved: "Le tour a changé de place.",
  unit_removed: "Le tour n'existe plus.",
  unit_became_interviewer: "Le tour est devenu un tour de l'intervieweur.",
}

export function reasonLabel(r: ProjectionReason | null): string {
  return (r && REASON[r]) || "Le texte a changé."
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
  const accept = useMutation({ mutationFn: () => v2.acceptProjection(item.projection_id), onSuccess: done("Emplacement proposé accepté"), onError: fail })
  const detach = useMutation({ mutationFn: () => v2.detachProjection(item.projection_id), onSuccess: done("Passage détaché"), onError: fail })
  const adjust = useMutation({
    mutationFn: () => v2.adjustProjection(item.projection_id, { start: sel!.start, end: sel!.end, offset_unit: "utf16" }),
    onSuccess: async () => {
      setAdjusting(false)
      await done("Passage ajusté")()
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
      <div className="small l2">Ancien passage</div>
      <Marked text={item.old.unit_text} range={{ start: item.old.start_utf16, end: item.old.end_utf16 }} label="Ancien texte du passage" />
      <div className="small l2">{adjusting ? "Texte actuel : sélectionnez le nouvel extrait" : "Texte actuel"}</div>
      {adjusting ? (
        <p className="verbatim" style={{ margin: 0, whiteSpace: "pre-wrap" }} data-no-i18n>
          <span ref={textRef} onMouseUp={onMouseUp} data-unit-text aria-label="Texte actuel du tour">
            {unitText}
          </span>
        </p>
      ) : (
        <Marked text={unitText} range={proposed} label="Texte actuel du tour" />
      )}
      {item.proposed ? (
        <p className="small l2" style={{ margin: 0 }}>
          Emplacement proposé : « {item.proposed.text} » (méthode : {item.method === "diff" ? "comparaison des textes" : item.method === "moved_unit" ? "tour déplacé" : "recherche du texte"}).
        </p>
      ) : (
        <p className="small l2" style={{ margin: 0 }}>
          Aucun emplacement ne convient : ajustez le passage à la main, ou détachez-le.
        </p>
      )}
      <div className="r8">
        {!adjusting && (
          <>
            <Button size="sm" disabled={busy || !item.proposed} onClick={() => accept.mutate()}>
              Accepter
            </Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => { setSel(null); setAdjusting(true) }}>
              Ajuster…
            </Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => detach.mutate()} title="Le passage sort des listes et des comptes ; son texte et ses codes restent dans l'historique.">
              Détacher
            </Button>
          </>
        )}
        {adjusting && (
          <>
            <Button size="sm" disabled={busy || !sel} onClick={() => adjust.mutate()}>
              Valider la sélection
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setAdjusting(false)}>
              Annuler
            </Button>
            <span className="small l2">{sel ? `« ${unitText.slice(sel.start, sel.end)} »` : "Sélectionnez des mots dans le texte."}</span>
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
    <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 6 }} aria-label="Versions du texte">
      {vs.map((v) => (
        <li key={v.version} className="grp" style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 2 }}>
          <div className="r8">
            <span className="b">Version {v.version}</span>
            {v.version === q.data?.current && <span className="badge">actuelle</span>}
            <span>{CAUSE[v.cause]}</span>
            {v.target && <span className="small l2 mono">{shortUnit(v.target)}</span>}
          </div>
          <span className="small l2">
            {dateTime(v.created_at)}, {v.created_by}, {v.n_units} tour{v.n_units > 1 ? "s" : ""}
            {v.reanchored > 0 && `, ${v.reanchored} passage${v.reanchored > 1 ? "s" : ""} replacé${v.reanchored > 1 ? "s" : ""} seul${v.reanchored > 1 ? "s" : ""}`}
            {v.flagged > 0 && `, ${v.flagged} à revoir`}
          </span>
          {v.reason && <span className="small l2">Motif : {v.reason}</span>}
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
          <DialogTitle>Versions du texte et passages à revoir</DialogTitle>
          <DialogDescription>
            Quand le texte d'un tour change, chaque passage codé sur ce tour est replacé. Ceux dont l'emplacement n'est pas sûr attendent votre décision.
          </DialogDescription>
        </DialogHeader>
        <div className="hs">À revoir{items.length > 0 ? ` (${items.length})` : ""}</div>
        {q.isLoading && <Skeleton className="h-20 w-full" />}
        {q.error && <Alerts messages={[q.error.message]} />}
        {q.data && items.length === 0 && <p className="small l2" style={{ margin: 0 }}>Aucun passage à revoir.</p>}
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 8 }} aria-label="Passages à revoir">
          {items.map((it) => (
            <ReviewCard key={it.projection_id} item={it} interviewId={interviewId} />
          ))}
        </ul>
        <div className="hs">Versions</div>
        <VersionList interviewId={interviewId} />
      </DialogContent>
    </Dialog>
  )
}
