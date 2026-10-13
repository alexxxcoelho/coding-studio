// The journal of merges and splits (F-39): who did what and why, and the undo
// of an operation, with its own reason. A refusal says in plain words what is in
// the way.

import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { toast } from "sonner"

import { useRefreshAfterCodeOp } from "@/components/codebook/refresh"
import { Alerts, Field } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { useCodeName } from "@/lib/code-names"
import { dateTime } from "@/lib/format"
import { v2, type CodeOp, type UndoReport } from "@/lib/v2-api"
import { explainCodeOpError } from "@/lib/v2-text"

/** One line of the journal, in words. */
export function opSentence(op: CodeOp, nameOf: (h: string) => string): string {
  const others = op.other_ids.map(nameOf)
  if (op.kind === "merge") return t("codebook.journal.code_name_fusionne_dans_v", { code_name: op.code_name, v: others[0] ?? "?" })
  return t("codebook.journal.code_name_scinde_en_join", { code_name: op.code_name, join: others.map((n) => `« ${n} »`).join(", ") })
}

/** What an undo put back, for the toast. */
export function undoSummary(r: UndoReport): string {
  const parts: string[] = []
  if (r.codings_restored) parts.push(t("codebook.journal.n_codages_retablis", { n: r.codings_restored }))
  if (r.codings_moved_back) parts.push(t("codebook.journal.n_ramenes_au_code_d", { n: r.codings_moved_back }))
  if (r.codings_removed) parts.push(t("codebook.journal.n_retires_des_nouveaux_codes", { n: r.codings_removed }))
  if (r.unit_codings_restored) parts.push(t("codebook.journal.n_codages_d_unite_retablis", { n: r.unit_codings_restored }))
  if (r.decisions_restored) parts.push(t("codebook.journal.n_decisions_de_revue_retablies", { n: r.decisions_restored }))
  const done = parts.length ? parts.join(", ") : t("codebook.journal.rien_a_retablir")
  return r.skipped.length ? t("codebook.journal.done_n_elements_n_ont", { n: r.skipped.length, done }) : done
}

export function CodeJournal({ code, onOpenWorklist, empty }: { code?: string; onOpenWorklist: (opId: number) => void; empty?: string }) {
  const nameOf = useCodeName()
  const q = useQuery({ queryKey: ["code-ops", code ?? "all"], queryFn: () => v2.codeOps(code) })
  const [undoing, setUndoing] = useState<CodeOp | null>(null)
  const ops = [...(q.data?.operations ?? [])].reverse()

  if (q.isLoading) return <Skeleton className="h-16 w-full" />
  if (q.error) return <Alerts messages={[q.error.message]} />
  return (
    <>
      {ops.length === 0 && <p className="small l2" style={{ margin: 0 }}>{empty ?? t("codebook.journal.aucune_fusion_ni_scission_enregistre")}</p>}
      <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 8 }} aria-label={t("codebook.journal.operations")}>
        {ops.map((op) => (
          <li key={op.op_id} className="grp" style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 4 }} data-op={op.op_id}>
            <div className="r8" style={{ justifyContent: "space-between" }}>
              <span className="b">{op.kind === "merge" ? "Fusion" : "Scission"} n° {op.op_id}</span>
              {op.undone ? <span className="badge">{t("codebook.journal.annulee")}</span> : null}
            </div>
            <span>{opSentence(op, nameOf)}</span>
            <span className="small l2">
              {t("codebook.journal.datetime_created_by_motif_rationale", { dateTime: dateTime(op.created_at), created_by: op.created_by, rationale: op.rationale })}
            </span>
            {op.undone && (
              <span className="small l2">
                {t("codebook.journal.annulee_le_datetime_par_by", { dateTime: dateTime(op.undone.at), by: op.undone.by, rationale: op.undone.rationale })}
              </span>
            )}
            {op.kind === "split" && typeof op.summary.worklist === "number" && (
              <span className="small l2">
                {t("codebook.journal.n_passages_dans_la_liste", { n: op.summary.worklist })}
              </span>
            )}
            <div className="r8">
              {op.kind === "split" && (
                <Button variant="outline" size="sm" onClick={() => onOpenWorklist(op.op_id)}>
                  {op.undone ? t("codebook.journal.voir_la_liste") : t("codebook.journal.liste_de_reaffectation")}
                </Button>
              )}
              {!op.undone && (
                <Button variant="outline" size="sm" onClick={() => setUndoing(op)}>
                  {t("codebook.journal.annuler_cette")} {op.kind === "merge" ? "fusion" : "scission"}…
                </Button>
              )}
            </div>
          </li>
        ))}
      </ul>
      <UndoDialog op={undoing} onClose={() => setUndoing(null)} />
    </>
  )
}

function UndoDialog({ op, onClose }: { op: CodeOp | null; onClose: () => void }) {
  return (
    <Dialog open={op !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>{op && <UndoBody key={op.op_id} op={op} onClose={onClose} />}</DialogContent>
    </Dialog>
  )
}

function UndoBody({ op, onClose }: { op: CodeOp; onClose: () => void }) {
  const nameOf = useCodeName()
  const refresh = useRefreshAfterCodeOp()
  const [why, setWhy] = useState("")
  const [error, setError] = useState<string | null>(null)
  const run = useMutation({
    mutationFn: () => v2.undoCodeOp(op.op_id, why.trim()),
    onSuccess: async (a) => {
      await refresh()
      toast.success(t("codebook.journal.operation_n_op_id_annulee", { op_id: op.op_id }), { description: undoSummary(a.report) })
      onClose()
    },
    onError: (e) => setError(explainCodeOpError(e)),
  })
  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("codebook.journal.annuler_la")} {op.kind === "merge" ? "fusion" : "scission"} n° {op.op_id}</DialogTitle>
        <DialogDescription>
          {opSentence(op, nameOf)}.{" "}
          {op.kind === "merge"
            ? t("codebook.journal.le_code_redevient_actif_avec")
            : t("codebook.journal.les_decisions_de_la_liste")}{" "}{t("codebook.journal.si_une_operation_plus_recente")}
        </DialogDescription>
      </DialogHeader>
      <Field label={t("codebook.journal.motif_de_l_annulation_obligatoire")}>
        <Input value={why} onChange={(e) => setWhy(e.target.value)} />
      </Field>
      {error && <Alerts messages={[error]} />}
      <DialogFooter>
        <Button variant="outline" onClick={onClose}>
          {t("codebook.journal.garder_l_operation")}
        </Button>
        <Button disabled={!why.trim() || run.isPending} onClick={() => run.mutate()}>
          {t("codebook.journal.annuler_l_operation")}
        </Button>
      </DialogFooter>
    </>
  )
}

/** The whole journal in a dialog (toolbar of the Codebook). */
export function JournalDialog({ open, onClose, onOpenWorklist }: { open: boolean; onClose: () => void; onOpenWorklist: (opId: number) => void }) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("codebook.journal.journal_des_fusions_et_scissions")}</DialogTitle>
          <DialogDescription>{t("codebook.journal.la_plus_recente_en_premier")}</DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 420, overflow: "auto" }}>
          <CodeJournal onOpenWorklist={(id) => { onClose(); onOpenWorklist(id) }} />
        </div>
      </DialogContent>
    </Dialog>
  )
}
