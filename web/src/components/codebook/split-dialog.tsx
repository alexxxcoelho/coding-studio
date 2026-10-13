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
  if (parts.length < 2) return "Une scission donne au moins deux nouveaux codes."
  const names = new Set<string>()
  for (const [i, p] of parts.entries()) {
    const n = fold(p.name.trim())
    if (!n) return `Donnez un nom à la partie ${i + 1}.`
    if (names.has(n)) return `Le nom « ${p.name.trim()} » est donné deux fois.`
    names.add(n)
    if (codes.some((c) => c.code_id !== originalId && !c.merged_into && fold(c.name) === n)) return `Un code porte déjà le nom « ${p.name.trim()} ».`
    if (!p.definition.trim()) return `Définissez la partie ${i + 1} en une phrase.`
  }
  if (!rationale.trim()) return "Le motif est obligatoire : il reste dans le journal."
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
      toast.success(`« ${code.name} » est scindé en ${a.new_codes.length} codes`)
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
          <DialogTitle>Scinder « {code.name} »</DialogTitle>
          <DialogDescription>
            Le code reste actif avec ses {code.counts.passages} passage{code.counts.passages > 1 ? "s" : ""}. Rien ne bouge avant vos décisions : ils attendent dans une liste, où vous choisirez pour chacun un ou plusieurs des nouveaux codes, ou de le laisser.
          </DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 380, overflow: "auto", display: "flex", flexDirection: "column", gap: 10, paddingBottom: 4 }}>
          {parts.map((p, i) => (
            <div key={i} className="grp" style={{ padding: 10, display: "flex", flexDirection: "column", gap: 8 }} role="group" aria-label={`Partie ${i + 1}`}>
              <div className="r8" style={{ justifyContent: "space-between" }}>
                <span className="hs">Partie {i + 1}</span>
                {parts.length > 2 && (
                  <Button variant="ghost" size="sm" aria-label={`Retirer la partie ${i + 1}`} onClick={() => setParts((l) => l.filter((_, j) => j !== i))}>
                    Retirer
                  </Button>
                )}
              </div>
              <Field label="Nom">
                <Input aria-label={`Nom de la partie ${i + 1}`} value={p.name} onChange={(e) => set(i, { name: e.target.value })} />
              </Field>
              <Field label="Définition">
                <textarea aria-label={`Définition de la partie ${i + 1}`} className={TEXTAREA} rows={2} value={p.definition} onChange={(e) => set(i, { definition: e.target.value })} />
              </Field>
              <div className="r8" style={{ alignItems: "flex-start" }}>
                <Field label="Inclut, facultatif" className="flex-1">
                  <textarea aria-label={`Inclut, partie ${i + 1}`} className={TEXTAREA} rows={2} value={p.includes} onChange={(e) => set(i, { includes: e.target.value })} />
                </Field>
                <Field label="Exclut, facultatif" className="flex-1">
                  <textarea aria-label={`Exclut, partie ${i + 1}`} className={TEXTAREA} rows={2} value={p.excludes} onChange={(e) => set(i, { excludes: e.target.value })} />
                </Field>
              </div>
              <Field label="Parent">
                <NativeSelect aria-label={`Parent de la partie ${i + 1}`} value={p.parent} onChange={(e) => set(i, { parent: e.target.value })}>
                  <option value="">Celui du code d'origine</option>
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
              Ajouter une partie
            </Button>
          </div>
        </div>
        <Field label="Motif, obligatoire">
          <Input value={rationale} onChange={(e) => setRationale(e.target.value)} placeholder="Pourquoi ce code recouvre deux idées" />
        </Field>
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Annuler
          </Button>
          <Button disabled={!!problem || run.isPending} title={problem ?? undefined} onClick={() => run.mutate()}>
            Scinder et réaffecter les passages
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
