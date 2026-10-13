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
import { useCodeName } from "@/lib/code-names"
import { dateTime } from "@/lib/format"
import { v2, type CodeOp, type UndoReport } from "@/lib/v2-api"
import { explainCodeOpError } from "@/lib/v2-text"

/** One line of the journal, in words. */
export function opSentence(op: CodeOp, nameOf: (h: string) => string): string {
  const others = op.other_ids.map(nameOf)
  if (op.kind === "merge") return `« ${op.code_name} » fusionné dans « ${others[0] ?? "?"} »`
  return `« ${op.code_name} » scindé en ${others.map((n) => `« ${n} »`).join(", ")}`
}

/** What an undo put back, for the toast. */
export function undoSummary(r: UndoReport): string {
  const parts: string[] = []
  if (r.codings_restored) parts.push(`${r.codings_restored} codage${r.codings_restored > 1 ? "s" : ""} rétabli${r.codings_restored > 1 ? "s" : ""}`)
  if (r.codings_moved_back) parts.push(`${r.codings_moved_back} ramené${r.codings_moved_back > 1 ? "s" : ""} au code d'origine`)
  if (r.codings_removed) parts.push(`${r.codings_removed} retiré${r.codings_removed > 1 ? "s" : ""} des nouveaux codes`)
  if (r.unit_codings_restored) parts.push(`${r.unit_codings_restored} codage${r.unit_codings_restored > 1 ? "s" : ""} d'unité rétabli${r.unit_codings_restored > 1 ? "s" : ""}`)
  if (r.decisions_restored) parts.push(`${r.decisions_restored} décision${r.decisions_restored > 1 ? "s" : ""} de revue rétablie${r.decisions_restored > 1 ? "s" : ""}`)
  const done = parts.length ? parts.join(", ") : "Rien à rétablir"
  return r.skipped.length ? `${done}. ${r.skipped.length} élément${r.skipped.length > 1 ? "s" : ""} n'ont pas pu être rétablis : ils ont changé depuis.` : done
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
      {ops.length === 0 && <p className="small l2" style={{ margin: 0 }}>{empty ?? "Aucune fusion ni scission enregistrée."}</p>}
      <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 8 }} aria-label="Opérations">
        {ops.map((op) => (
          <li key={op.op_id} className="grp" style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 4 }} data-op={op.op_id}>
            <div className="r8" style={{ justifyContent: "space-between" }}>
              <span className="b">{op.kind === "merge" ? "Fusion" : "Scission"} n° {op.op_id}</span>
              {op.undone ? <span className="badge">annulée</span> : null}
            </div>
            <span>{opSentence(op, nameOf)}</span>
            <span className="small l2">
              {dateTime(op.created_at)}, {op.created_by}. Motif : {op.rationale}
            </span>
            {op.undone && (
              <span className="small l2">
                Annulée le {dateTime(op.undone.at)} par {op.undone.by}. Motif : {op.undone.rationale}
              </span>
            )}
            {op.kind === "split" && typeof op.summary.worklist === "number" && (
              <span className="small l2">
                {op.summary.worklist} passage{op.summary.worklist > 1 ? "s" : ""} dans la liste de réaffectation.
              </span>
            )}
            <div className="r8">
              {op.kind === "split" && (
                <Button variant="outline" size="sm" onClick={() => onOpenWorklist(op.op_id)}>
                  {op.undone ? "Voir la liste" : "Liste de réaffectation"}
                </Button>
              )}
              {!op.undone && (
                <Button variant="outline" size="sm" onClick={() => setUndoing(op)}>
                  Annuler cette {op.kind === "merge" ? "fusion" : "scission"}…
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
      toast.success(`Opération n° ${op.op_id} annulée`, { description: undoSummary(a.report) })
      onClose()
    },
    onError: (e) => setError(explainCodeOpError(e)),
  })
  return (
    <>
      <DialogHeader>
        <DialogTitle>Annuler la {op.kind === "merge" ? "fusion" : "scission"} n° {op.op_id}</DialogTitle>
        <DialogDescription>
          {opSentence(op, nameOf)}.{" "}
          {op.kind === "merge"
            ? "Le code redevient actif avec son parent, ses sous-codes, ses alias et ses codages. Ce qui a été modifié depuis n'est pas écrasé : cela est signalé."
            : "Les décisions de la liste sont défaites, les codages posés depuis sur les nouveaux codes reviennent au code d'origine, et les nouveaux codes sont retirés."}{" "}
          Si une opération plus récente touche les mêmes codes, elle doit être annulée d'abord.
        </DialogDescription>
      </DialogHeader>
      <Field label="Motif de l'annulation, obligatoire">
        <Input value={why} onChange={(e) => setWhy(e.target.value)} />
      </Field>
      {error && <Alerts messages={[error]} />}
      <DialogFooter>
        <Button variant="outline" onClick={onClose}>
          Garder l'opération
        </Button>
        <Button disabled={!why.trim() || run.isPending} onClick={() => run.mutate()}>
          Annuler l'opération
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
          <DialogTitle>Journal des fusions et scissions</DialogTitle>
          <DialogDescription>La plus récente en premier. Chaque opération garde son motif ; on peut l'annuler avec un autre motif tant qu'aucune opération plus récente ne touche les mêmes codes.</DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 420, overflow: "auto" }}>
          <CodeJournal onOpenWorklist={(id) => { onClose(); onOpenWorklist(id) }} />
        </div>
      </DialogContent>
    </Dialog>
  )
}
