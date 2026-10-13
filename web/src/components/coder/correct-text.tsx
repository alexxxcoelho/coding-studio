// Correction of the text of one turn (what the audio really says). The words
// change, the passages on this turn are put back on the new text by the server,
// and those it cannot place for sure wait in the review.

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { TEXTAREA } from "@/components/settings/fields"
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
        toast.success(`Texte corrigé. ${open} passage${open > 1 ? "s" : ""} à revoir.`, { action: { label: "Voir", onClick: onReview } })
      } else {
        toast.success("Texte corrigé. Les passages ont suivi seuls.")
      }
    },
    onError: (e) => reportError(e, "Correction refusée"),
  })
  return (
    <>
      <DialogHeader>
        <DialogTitle>Corriger le texte de {unit.unit_id.replace(/^.*-(U\d+)$/, "$1")}</DialogTitle>
        <DialogDescription>Le texte tel qu'il doit se lire. L'ancien texte reste dans les versions, et les passages codés sur ce tour sont replacés.</DialogDescription>
      </DialogHeader>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (ready) save.mutate()
        }}
      >
        <label className="flex flex-col gap-1 text-xs">
          <span className="l2">Texte</span>
          <textarea rows={7} className={`${TEXTAREA} verbatim`} value={text} onChange={(e) => setText(e.target.value)} aria-label="Texte corrigé" />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="l2">Motif, obligatoire</span>
          <textarea rows={2} className={TEXTAREA} value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Ce que dit l'audio, et à quel moment" aria-label="Motif de la correction" />
        </label>
        <DialogFooter>
          <Button type="submit" disabled={!ready || save.isPending}>
            {save.isPending ? "Enregistrement…" : "Enregistrer la correction"}
          </Button>
        </DialogFooter>
      </form>
    </>
  )
}
