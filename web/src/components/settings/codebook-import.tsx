// Codebook import: a file by path (or by the native dialog in the desktop
// app), by kind (csv, json or the legacy coding.db). The answer says how many
// codes were added, updated and absent.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { Section } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Alerts, Field, NativeSelect } from "@/components/settings/fields"
import { t } from "@/i18n"
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
    const picked = await api.pick("file", t("settings.codebook_import.choisir_le_fichier_du_codebook"))
    if (picked.path) setPath(picked.path)
  }

  return (
    <Section title={t("settings.codebook_import.import_du_codebook")} aside={t("settings.codebook_import.les_codes_sont_ajoutes_ou")}>
      <div className="flex flex-col gap-4">
        <div className="grid gap-3 sm:grid-cols-[12rem_1fr_auto] sm:items-end">
          <Field label={t("settings.codebook_import.type_d_import")}>
            <NativeSelect value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
              <option value="csv">CSV</option>
              <option value="json">JSON</option>
              <option value="coding_db">{t("settings.codebook_import.coding_db_de_la_these")}</option>
            </NativeSelect>
          </Field>
          <Field label={t("common.chemin_du_fichier")}>
            <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/chemin/vers/codebook.csv" className="font-mono text-xs" />
          </Field>
          <div className="flex gap-2">
            {state.data?.native_dialogs && (
              <Button variant="outline" onClick={choose}>
                {t("settings.codebook_import.choisir_un_fichier")}
              </Button>
            )}
            <Button disabled={!path.trim() || importing.isPending} onClick={() => importing.mutate()}>
              {importing.isPending ? "Import..." : t("settings.codebook_import.importer_le_codebook")}
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
