// Pseudonymisation of what is sent to a model (docs/guide/fr/01_preparer.md,
// action 11): the switch and the list of names with their pseudonym, one list
// for the whole project. It is applied to every prompt and to every send to a
// provider, and reversed for display only. Free text typed by the researcher
// (memos, notes) is not covered, and the page says so.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Section } from "@/components/bits"
import { Alerts, NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { ApiError } from "@/lib/api"
import { shortHash } from "@/lib/format"
import { keys } from "@/lib/hooks"
import { entriesProblem, sameEntries } from "@/lib/pseudonymisation"
import { reviewApi, type Pseudonymisation, type PseudonymKind } from "@/lib/review-api"

interface Row {
  kind: PseudonymKind
  original: string
  pseudonym: string
}

const KIND_LABEL: Record<PseudonymKind, string> = { person: "Personne", organisation: "Organisation" }

export function PseudonymisationSection() {
  const q = useQuery({ queryKey: keys.projectPseudonymisation, queryFn: reviewApi.projectPseudonymisation })
  return (
    <Section title="Pseudonymisation avant envoi" aside="ce qui part vers un modèle">
      {q.isLoading ? <Skeleton className="h-32 w-full" /> : q.error ? <Alerts messages={[q.error.message]} /> : <Editor key={q.data!.digest} saved={q.data!} />}
    </Section>
  )
}

function Editor({ saved }: { saved: Pseudonymisation }) {
  const qc = useQueryClient()
  const stored: Row[] = saved.entries.map((e) => ({ kind: e.kind, original: e.original, pseudonym: e.pseudonym }))
  const [rows, setRows] = useState<Row[]>(stored)
  const [attempted, setAttempted] = useState(false)
  const [serverError, setServerError] = useState<string | null>(null)
  const dirty = !sameEntries(rows, stored)
  const problem = entriesProblem(rows)

  const save = useMutation({
    mutationFn: (body: { enabled?: boolean; entries?: Row[] }) => reviewApi.savePseudonymisation(body),
    onSuccess: (view, body) => {
      qc.setQueryData(keys.projectPseudonymisation, view)
      void qc.invalidateQueries({ queryKey: ["pseudonymisation"] })
      void qc.invalidateQueries({ queryKey: ["chain"] })
      setServerError(null)
      setAttempted(false)
      toast.success(body.enabled !== undefined ? (view.enabled ? "Pseudonymisation active" : "Pseudonymisation inactive") : "Liste enregistrée")
    },
    onError: (e) => setServerError(e instanceof ApiError ? e.message : String(e)),
  })

  const set = (i: number, patch: Partial<Row>) => setRows((r) => r.map((x, j) => (j === i ? { ...x, ...patch } : x)))
  const submit = () => {
    setAttempted(true)
    if (!problem) save.mutate({ entries: rows.map((r) => ({ ...r, original: r.original.trim(), pseudonym: r.pseudonym.trim() })) })
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={saved.enabled}
          aria-label="Remplacer les noms avant tout envoi à un modèle"
          disabled={save.isPending}
          onClick={() => save.mutate({ enabled: !saved.enabled })}
        />
        <span className="text-sm">
          <b>{saved.enabled ? "Pseudonymisation active" : "Pseudonymisation inactive"}</b>
          <span className="text-muted-foreground">
            {saved.enabled ? " : les noms de la liste sont remplacés dans chaque prompt et chaque envoi." : " : les noms partent tels quels dans un prompt."}
          </span>
        </span>
      </div>

      {rows.length > 0 && (
        <table className="tbl" aria-label="Liste des noms">
          <thead>
            <tr>
              <th style={{ width: 150 }}>Type</th>
              <th>Nom réel</th>
              <th>Pseudonyme</th>
              <th style={{ width: 80 }}>
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i}>
                <td>
                  <NativeSelect aria-label={`Type du nom ${i + 1}`} value={r.kind} onChange={(e) => set(i, { kind: e.target.value as PseudonymKind })}>
                    <option value="person">{KIND_LABEL.person}</option>
                    <option value="organisation">{KIND_LABEL.organisation}</option>
                  </NativeSelect>
                </td>
                <td>
                  <Input aria-label={`Nom réel ${i + 1}`} value={r.original} onChange={(e) => set(i, { original: e.target.value })} />
                </td>
                <td>
                  <Input aria-label={`Pseudonyme ${i + 1}`} value={r.pseudonym} onChange={(e) => set(i, { pseudonym: e.target.value })} />
                </td>
                <td>
                  <Button variant="ghost" aria-label={`Retirer le nom ${i + 1}`} onClick={() => setRows((x) => x.filter((_, j) => j !== i))}>
                    Retirer
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {rows.length === 0 && <p className="text-muted-foreground text-sm">La liste est vide. Ajoutez les noms de personnes et d'organisations qui figurent dans vos transcriptions.</p>}

      {attempted && problem && <Alerts messages={[problem]} />}
      {serverError && <Alerts messages={[serverError]} />}

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={() => setRows((r) => [...r, { kind: "person", original: "", pseudonym: "" }])}>
          Ajouter un nom
        </Button>
        <Button variant="default" disabled={!dirty || save.isPending} onClick={submit}>
          Enregistrer la liste
        </Button>
        {dirty && (
          <Button variant="ghost" onClick={() => setRows(stored)}>
            Annuler les changements
          </Button>
        )}
      </div>

      <p className="text-muted-foreground text-xs" data-testid="pseudo-digest">
        Liste de {saved.n_list_entries} nom{saved.n_list_entries > 1 ? "s" : ""}
        {saved.categories.length > 0 ? ` (${saved.categories.map((c) => (c === "person" ? "personnes" : c === "organisation" ? "organisations" : c)).join(", ")})` : ""}, empreinte SHA-256 <span className="mono">{shortHash(saved.digest, 16)}</span>, enregistrée avec chaque prompt. Le journal garde l'empreinte et les nombres, jamais les noms.
      </p>
      <p className="text-muted-foreground text-xs">
        <b>Le texte libre n'est pas couvert.</b> {saved.note}
      </p>
    </div>
  )
}
