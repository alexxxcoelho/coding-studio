// "Nouveau code depuis la sélection": a name in plain words, a one sentence
// definition, an optional parent. The code is created and applied to the
// selected passage in one call (docs/guide/fr/02_coder.md, action 5).

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { TEXTAREA } from "@/components/settings/fields"
import { ApiError, api } from "@/lib/api"
import { codeName, isApplicable } from "@/lib/code-tree"
import { invalidatePassages } from "@/lib/hooks"
import type { CodeNode } from "@/lib/types"

export interface NewCodeTarget {
  unitId: string
  /** A highlight to convert: the code is made from this passage. */
  passageId?: string
  /** UTF-16 offsets; absent for the whole turn. */
  range: { start: number; end: number } | null
  /** The text the code is made from, shown as a reminder. */
  text: string
}

export function NewCodeDialog({
  interviewId,
  target,
  codes,
  onClose,
}: {
  interviewId: string
  target: NewCodeTarget | null
  codes: CodeNode[]
  onClose: () => void
}) {
  const qc = useQueryClient()
  const [name, setName] = useState("")
  const [definition, setDefinition] = useState("")
  const [parent, setParent] = useState("")
  const [error, setError] = useState<string | null>(null)

  // A new target opens the form empty: adjusted during render, not in an effect.
  const [seen, setSeen] = useState(target)
  if (target !== seen) {
    setSeen(target)
    if (target) {
      setName("")
      setDefinition("")
      setParent("")
      setError(null)
    }
  }

  const create = useMutation({
    mutationFn: () =>
      api.createCodeFromPassage({
        name: name.trim(),
        definition: definition.trim(),
        parent_id: parent || null,
        ...(target!.passageId
          ? { passage_id: target!.passageId }
          : {
              passage: {
                interview_id: interviewId,
                unit_id: target!.unitId,
                ...(target!.range ? { start: target!.range.start, end: target!.range.end } : {}),
                offset_unit: "utf16" as const,
              },
            }),
      }),
    onSuccess: async (res) => {
      await invalidatePassages(qc, interviewId)
      toast.success(`Code « ${res.code.name} » créé et posé`)
      onClose()
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : String(e)),
  })

  const ready = name.trim() !== "" && definition.trim() !== ""
  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Nouveau code</DialogTitle>
          <DialogDescription>
            Le code est créé et posé sur {target?.passageId ? "le surlignage" : target?.range ? "la sélection" : "le tour"}, qui devient son origine et son premier exemple.
          </DialogDescription>
        </DialogHeader>
        {target?.text && (
          <blockquote className="small l2" style={{ margin: 0, borderLeft: "3px solid var(--sep)", paddingLeft: 10, maxHeight: 80, overflow: "auto" }}>
            {target.text}
          </blockquote>
        )}
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            if (ready) create.mutate()
          }}
        >
          <label className="flex flex-col gap-1 text-sm">
            <span className="hs">Nom</span>
            <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Ce que dit le répondant, en mots courants" />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="hs">Définition</span>
            <textarea className={TEXTAREA} rows={3} value={definition} onChange={(e) => setDefinition(e.target.value)} placeholder="Une phrase qui dit ce que le code inclut et ce qu'il exclut" />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="hs">Parent (facultatif)</span>
            <NativeSelect value={parent} onChange={(e) => setParent(e.target.value)}>
              <option value="">Aucun</option>
              {codes
                .filter((c) => isApplicable(c))
                .map((c) => (
                  <option key={c.code_id} value={c.code_id}>
                    {codeName(c)}
                  </option>
                ))}
            </NativeSelect>
          </label>
          {error && (
            <p role="alert" className="note-err">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Annuler
            </Button>
            <Button type="submit" disabled={!ready || create.isPending}>
              {create.isPending ? "Création…" : "Créer et poser"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
