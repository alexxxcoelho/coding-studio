// Split a code (F-39): two or more new codes, each with a name and a definition,
// and the reason. Nothing moves yet: the passages of the original wait in a
// worklist where each one is assigned to one or more of the new codes, or left.

import { useMemo, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { toast } from "sonner"

import { useRefreshAfterCodeOp } from "@/components/codebook/refresh"
import { Alerts, Field, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { fold, isApplicable } from "@/lib/code-tree"
import type { CodeNode } from "@/lib/types"
import { v2, type SplitPartBody } from "@/lib/v2-api"
import { explainCodeOpError } from "@/lib/v2-text"

interface PartDraft {
  name: string
  definition: string
  includes: string
  excludes: string
  /** "" keeps the parent of the original. */
  parent: string
}

const blank = (): PartDraft => ({ name: "", definition: "", includes: "", excludes: "", parent: "" })

/** The first thing wrong with the parts, in the researcher's words, or null. */
export function splitProblem(parts: PartDraft[], codes: CodeNode[], originalId: string, rationale: string): string | null {
  if (parts.length < 2) return t("common.une_scission_donne_au_moins")
  const names = new Set<string>()
  for (const [i, p] of parts.entries()) {
    const n = fold(p.name.trim())
    if (!n) return t("codebook.split_dialog.donnez_un_nom_a_la", { v: i + 1 })
    if (names.has(n)) return t("codebook.split_dialog.le_nom_trim_est_donne", { trim: p.name.trim() })
    names.add(n)
    if (codes.some((c) => c.code_id !== originalId && !c.merged_into && fold(c.name) === n)) return t("codebook.split_dialog.un_code_porte_deja_le", { trim: p.name.trim() })
    if (!p.definition.trim()) return t("codebook.split_dialog.definissez_la_partie_v_en", { v: i + 1 })
  }
  if (!rationale.trim()) return t("codebook.split_dialog.le_motif_est_obligatoire_il")
  return null
}

export function SplitDialog({
  open,
  code,
  codes,
  onClose,
  onSplit,
}: {
  open: boolean
  code: CodeNode
  codes: CodeNode[]
  onClose: () => void
  /** Called with the number of the operation, to open its worklist. */
  onSplit: (opId: number) => void
}) {
  const refresh = useRefreshAfterCodeOp()
  const [parts, setParts] = useState<PartDraft[]>([blank(), blank()])
  const [rationale, setRationale] = useState("")
  const [error, setError] = useState<string | null>(null)
  const parents = useMemo(() => codes.filter((c) => isApplicable(c) && c.code_id !== code.code_id), [codes, code.code_id])
  const problem = splitProblem(parts, codes, code.code_id, rationale)
  const set = (i: number, patch: Partial<PartDraft>) => setParts((l) => l.map((p, j) => (j === i ? { ...p, ...patch } : p)))

  const run = useMutation({
    mutationFn: () => {
      const body: SplitPartBody[] = parts.map((p) => ({
        name: p.name.trim(),
        definition: p.definition.trim(),
        ...(p.includes.trim() ? { includes: p.includes.trim() } : {}),
        ...(p.excludes.trim() ? { excludes: p.excludes.trim() } : {}),
        ...(p.parent ? { parent_id: p.parent } : {}),
      }))
      return v2.splitCode(code.code_id, body, rationale.trim())
    },
    onSuccess: async (a) => {
      await refresh()
      toast.success(t("codebook.split_dialog.name_est_scinde_en_length", { name: code.name, length: a.new_codes.length }))
      setParts([blank(), blank()])
      setRationale("")
      setError(null)
      onClose()
      onSplit(a.op_id)
    },
    onError: (e) => setError(explainCodeOpError(e)),
  })

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("codebook.split_dialog.scinder_name", { name: code.name })}</DialogTitle>
          <DialogDescription>
            {t("codebook.split_dialog.le_code_reste_actif_avec", { n: code.counts.passages })}
          </DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 380, overflow: "auto", display: "flex", flexDirection: "column", gap: 10, paddingBottom: 4 }}>
          {parts.map((p, i) => (
            <div key={i} className="grp" style={{ padding: 10, display: "flex", flexDirection: "column", gap: 8 }} role="group" aria-label={`Partie ${i + 1}`}>
              <div className="r8" style={{ justifyContent: "space-between" }}>
                <span className="hs">{t("codebook.split_dialog.partie_v", { v: i + 1 })}</span>
                {parts.length > 2 && (
                  <Button variant="ghost" size="sm" aria-label={t("codebook.split_dialog.retirer_la_partie_v", { v: i + 1 })} onClick={() => setParts((l) => l.filter((_, j) => j !== i))}>
                    {t("common.retirer")}
                  </Button>
                )}
              </div>
              <Field label={t("common.nom")}>
                <Input aria-label={t("codebook.split_dialog.nom_de_la_partie_v", { v: i + 1 })} value={p.name} onChange={(e) => set(i, { name: e.target.value })} />
              </Field>
              <Field label={t("common.definition")}>
                <textarea aria-label={t("codebook.split_dialog.definition_de_la_partie_v", { v: i + 1 })} className={TEXTAREA} rows={2} value={p.definition} onChange={(e) => set(i, { definition: e.target.value })} />
              </Field>
              <div className="r8" style={{ alignItems: "flex-start" }}>
                <Field label={t("codebook.split_dialog.inclut_facultatif")} className="flex-1">
                  <textarea aria-label={t("codebook.split_dialog.inclut_partie_v", { v: i + 1 })} className={TEXTAREA} rows={2} value={p.includes} onChange={(e) => set(i, { includes: e.target.value })} />
                </Field>
                <Field label={t("codebook.split_dialog.exclut_facultatif")} className="flex-1">
                  <textarea aria-label={t("codebook.split_dialog.exclut_partie_v", { v: i + 1 })} className={TEXTAREA} rows={2} value={p.excludes} onChange={(e) => set(i, { excludes: e.target.value })} />
                </Field>
              </div>
              <Field label={t("common.parent")}>
                <NativeSelect aria-label={t("codebook.split_dialog.parent_de_la_partie_v", { v: i + 1 })} value={p.parent} onChange={(e) => set(i, { parent: e.target.value })}>
                  <option value="">{t("codebook.split_dialog.celui_du_code_d_origine")}</option>
                  {parents.map((c) => (
                    <option key={c.code_id} value={c.code_id}>
                      {c.name}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
            </div>
          ))}
          <div>
            <Button variant="outline" onClick={() => setParts((l) => [...l, blank()])}>
              {t("codebook.split_dialog.ajouter_une_partie")}
            </Button>
          </div>
        </div>
        <Field label={t("common.motif_obligatoire")}>
          <Input value={rationale} onChange={(e) => setRationale(e.target.value)} placeholder={t("codebook.split_dialog.pourquoi_ce_code_recouvre_deux")} />
        </Field>
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.annuler")}
          </Button>
          <Button disabled={!!problem || run.isPending} title={problem ?? undefined} onClick={() => run.mutate()}>
            {t("codebook.split_dialog.scinder_et_reaffecter_les_passages")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
