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
import { t } from "@/i18n"
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

const KIND_LABEL: Record<PseudonymKind, string> = { get person() { return t("settings.pseudonymisation.personne") }, get organisation() { return t("common.organisation") } }

export function PseudonymisationSection() {
  const q = useQuery({ queryKey: keys.projectPseudonymisation, queryFn: reviewApi.projectPseudonymisation })
  return (
    <Section title={t("settings.pseudonymisation.pseudonymisation_avant_envoi")} aside={t("settings.pseudonymisation.ce_qui_part_vers_un")}>
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
      toast.success(body.enabled !== undefined ? (view.enabled ? t("common.pseudonymisation_active") : t("common.pseudonymisation_inactive")) : t("settings.pseudonymisation.liste_enregistree"))
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
          aria-label={t("settings.pseudonymisation.remplacer_les_noms_avant_tout")}
          disabled={save.isPending}
          onClick={() => save.mutate({ enabled: !saved.enabled })}
        />
        <span className="text-sm">
          <b>{saved.enabled ? t("common.pseudonymisation_active") : t("common.pseudonymisation_inactive")}</b>
          <span className="text-muted-foreground">
            {saved.enabled ? t("settings.pseudonymisation.les_noms_de_la_liste") : t("settings.pseudonymisation.les_noms_partent_tels_quels")}
          </span>
        </span>
      </div>

      {rows.length > 0 && (
        <table className="tbl" aria-label={t("settings.pseudonymisation.liste_des_noms")}>
          <thead>
            <tr>
              <th style={{ width: 150 }}>{t("common.type")}</th>
              <th>{t("settings.pseudonymisation.nom_reel")}</th>
              <th>{t("settings.pseudonymisation.pseudonyme")}</th>
              <th style={{ width: 80 }}>
                <span className="sr-only">{t("common.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i}>
                <td>
                  <NativeSelect aria-label={t("settings.pseudonymisation.type_du_nom_v", { v: i + 1 })} value={r.kind} onChange={(e) => set(i, { kind: e.target.value as PseudonymKind })}>
                    <option value="person">{KIND_LABEL.person}</option>
                    <option value="organisation">{KIND_LABEL.organisation}</option>
                  </NativeSelect>
                </td>
                <td>
                  <Input aria-label={t("settings.pseudonymisation.nom_reel_v", { v: i + 1 })} value={r.original} onChange={(e) => set(i, { original: e.target.value })} />
                </td>
                <td>
                  <Input aria-label={`Pseudonyme ${i + 1}`} value={r.pseudonym} onChange={(e) => set(i, { pseudonym: e.target.value })} />
                </td>
                <td>
                  <Button variant="ghost" aria-label={t("settings.pseudonymisation.retirer_le_nom_v", { v: i + 1 })} onClick={() => setRows((x) => x.filter((_, j) => j !== i))}>
                    {t("common.retirer")}
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {rows.length === 0 && <p className="text-muted-foreground text-sm">{t("settings.pseudonymisation.la_liste_est_vide_ajoutez")}</p>}

      {attempted && problem && <Alerts messages={[problem]} />}
      {serverError && <Alerts messages={[serverError]} />}

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={() => setRows((r) => [...r, { kind: "person", original: "", pseudonym: "" }])}>
          {t("settings.pseudonymisation.ajouter_un_nom")}
        </Button>
        <Button variant="default" disabled={!dirty || save.isPending} onClick={submit}>
          {t("settings.pseudonymisation.enregistrer_la_liste")}
        </Button>
        {dirty && (
          <Button variant="ghost" onClick={() => setRows(stored)}>
            {t("common.annuler_les_changements")}
          </Button>
        )}
      </div>

      <p className="text-muted-foreground text-xs" data-testid="pseudo-digest">
        {t("settings.pseudonymisation.liste_de_n_noms", { n: saved.n_list_entries })}
        {saved.categories.length > 0 ? ` (${saved.categories.map((c) => (c === "person" ? "personnes" : c === "organisation" ? "organisations" : c)).join(", ")})` : ""}{t("settings.pseudonymisation.empreinte_sha_256")} <span className="mono">{shortHash(saved.digest, 16)}</span>{t("settings.pseudonymisation.enregistree_avec_chaque_prompt_le")}
      </p>
      <p className="text-muted-foreground text-xs">
        <b>{t("settings.pseudonymisation.le_texte_libre_n_est")}</b> {saved.note}
      </p>
    </div>
  )
}
