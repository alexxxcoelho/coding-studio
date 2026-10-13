// Correction of the text of one turn (what the audio really says). The words
// change, the passages on this turn are put back on the new text by the server,
// and those it cannot place for sure wait in the review.

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { TEXTAREA } from "@/components/settings/fields"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { refreshReview } from "@/lib/transcript-review"
import { v2 } from "@/lib/v2-api"
import type { Unit } from "@/lib/types"

type Turn = Pick<Unit, "unit_id" | "text" | "role">

export function CorrectTextDialog({ interviewId, unit, open, onClose, onReview }: { interviewId: string; unit: Turn | null; open: boolean; onClose: () => void; onReview: () => void }) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>{open && unit && <Form key={unit.unit_id} interviewId={interviewId} unit={unit} onClose={onClose} onReview={onReview} />}</DialogContent>
    </Dialog>
  )
}

function Form({ interviewId, unit, onClose, onReview }: { interviewId: string; unit: Turn; onClose: () => void; onReview: () => void }) {
  const qc = useQueryClient()
  const [text, setText] = useState(unit.text)
  const [reason, setReason] = useState("")
  const ready = text.trim() !== "" && text.trim() !== unit.text.trim() && reason.trim() !== ""
  const save = useMutation({
    mutationFn: () => api.correctUnit(interviewId, unit.unit_id, { text: text.trim(), role: unit.role, reason: reason.trim() }),
    onSuccess: async () => {
      await invalidateInterview(qc, interviewId)
      await refreshReview(qc, interviewId)
      const open = (await v2.review({ interview_id: interviewId, status: "needs_review" })).open
      onClose()
      if (open > 0) {
        toast.success(t("coder.correct_text.texte_corrige_n_passages_a", { n: open }), { action: { label: t("coder.correct_text.voir"), onClick: onReview } })
      } else {
        toast.success(t("coder.correct_text.texte_corrige_les_passages_ont"))
      }
    },
    onError: (e) => reportError(e, t("coder.correct_text.correction_refusee")),
  })
  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("coder.correct_text.corriger_le_texte_de_replace", { replace: unit.unit_id.replace(/^.*-(U\d+)$/, "$1") })}</DialogTitle>
        <DialogDescription>{t("coder.correct_text.le_texte_tel_qu_il")}</DialogDescription>
      </DialogHeader>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (ready) save.mutate()
        }}
      >
        <label className="flex flex-col gap-1 text-xs">
          <span className="l2">{t("coder.correct_text.texte")}</span>
          <textarea rows={7} className={`${TEXTAREA} verbatim`} value={text} onChange={(e) => setText(e.target.value)} aria-label={t("coder.correct_text.texte_corrige")} />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="l2">{t("coder.correct_text.motif_obligatoire")}</span>
          <textarea rows={2} className={TEXTAREA} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("coder.correct_text.ce_que_dit_l_audio")} aria-label={t("coder.correct_text.motif_de_la_correction")} />
        </label>
        <DialogFooter>
          <Button type="submit" disabled={!ready || save.isPending}>
            {save.isPending ? t("coder.correct_text.enregistrement") : t("coder.correct_text.enregistrer_la_correction")}
          </Button>
        </DialogFooter>
      </form>
    </>
  )
}
