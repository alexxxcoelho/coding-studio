// Codebook import: a file by path (or by the native dialog in the desktop
// app), by kind (csv, json or the legacy coding.db). The answer says how many
// codes were added, updated and absent.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { Section } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Alerts, Field, NativeSelect } from "@/components/settings/fields"
import { api, ApiError } from "@/lib/api"
import type { CodebookImportAnswer } from "@/lib/types"

type Kind = "csv" | "json" | "coding_db"

export function CodebookImportSection() {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const [kind, setKind] = useState<Kind>("csv")
  const [path, setPath] = useState("")
  const [result, setResult] = useState<CodebookImportAnswer["report"] | null>(null)

  const importing = useMutation({
    mutationFn: () => api.importCodebookFrom(kind, path.trim()),
    onSuccess: (answer) => {
      setResult(answer.report)
      qc.invalidateQueries({ queryKey: ["codebook"] })
      qc.invalidateQueries({ queryKey: ["state"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: () => setResult(null),
  })

  const choose = async () => {
    const picked = await api.pick("file", "Choisir le fichier du codebook")
    if (picked.path) setPath(picked.path)
  }

  return (
    <Section title="Import du codebook" aside="les codes sont ajoutés ou mis à jour, jamais supprimés">
      <div className="flex flex-col gap-4">
        <div className="grid gap-3 sm:grid-cols-[12rem_1fr_auto] sm:items-end">
          <Field label="Type d'import">
            <NativeSelect value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
              <option value="csv">CSV</option>
              <option value="json">JSON</option>
              <option value="coding_db">coding.db de la thèse</option>
            </NativeSelect>
          </Field>
          <Field label="Chemin du fichier">
            <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/chemin/vers/codebook.csv" className="font-mono text-xs" />
          </Field>
          <div className="flex gap-2">
            {state.data?.native_dialogs && (
              <Button variant="outline" onClick={choose}>
                Choisir un fichier
              </Button>
            )}
            <Button disabled={!path.trim() || importing.isPending} onClick={() => importing.mutate()}>
              {importing.isPending ? "Import..." : "Importer le codebook"}
            </Button>
          </div>
        </div>
        {result && (
          <p role="status" className="text-sm">
            {`${result.added} ajouté${result.added > 1 ? "s" : ""}, ${result.updated} mis à jour, ${result.absent} absent${result.absent > 1 ? "s" : ""}`}
          </p>
        )}
        {importing.error && <Alerts messages={[importing.error instanceof ApiError ? importing.error.message : String(importing.error)]} />}
      </div>
    </Section>
  )
}
