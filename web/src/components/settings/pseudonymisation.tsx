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
import { personHref } from "@/lib/router"
import { reviewApi, type Pseudonymisation, type PseudonymEntry, type PseudonymKind, type ShadowedEntry } from "@/lib/review-api"

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
  // The list in force has two parts: the entries derived from the anonymised records of Personnes
  // (read-only here, edited in their fiche) and the entries typed by hand, the only ones this editor changes.
  const derived = saved.entries.filter((e) => e.source === "person")
  const stored: Row[] = saved.entries.filter((e) => e.source !== "person").map((e) => ({ kind: e.kind, original: e.original, pseudonym: e.pseudonym }))
  const manualOn = saved.enabled_manual ?? saved.enabled
  const nDerived = saved.n_person_entries ?? derived.length
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
          aria-checked={manualOn}
          aria-label={t("settings.pseudonymisation.remplacer_les_noms_avant_tout")}
          disabled={save.isPending}
          onClick={() => save.mutate({ enabled: !manualOn })}
        />
        <span className="text-sm">
          <b>{saved.enabled ? t("common.pseudonymisation_active") : t("common.pseudonymisation_inactive")}</b>
          <span className="text-muted-foreground">
            {saved.enabled ? t("settings.pseudonymisation.les_noms_de_la_liste") : t("settings.pseudonymisation.les_noms_partent_tels_quels")}
          </span>
        </span>
      </div>
      {saved.enabled && !manualOn && <p className="text-muted-foreground text-xs">{t("persons.pseudo_on_by_records", { n: nDerived })}</p>}

      <DerivedEntries entries={derived} shadowed={saved.shadowed_manual ?? []} />

      {(derived.length > 0 || rows.length > 0) && <h3 className="text-sm font-semibold">{t("persons.manual_entries")}</h3>}
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
      {rows.length === 0 && (
        <p className="text-muted-foreground text-sm">{derived.length > 0 ? t("persons.manual_entries_empty") : t("settings.pseudonymisation.la_liste_est_vide_ajoutez")}</p>
      )}

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
        {saved.categories.length > 0 ? ` (${saved.categories.map((c) => (c === "person" ? t("settings.pseudonymisation.personnes") : c === "organisation" ? t("settings.pseudonymisation.organisations") : c)).join(", ")})` : ""}{t("settings.pseudonymisation.empreinte_sha_256")} <span className="mono">{shortHash(saved.digest, 16)}</span>{t("settings.pseudonymisation.enregistree_avec_chaque_prompt_le")}
      </p>
      {nDerived > 0 && <p className="text-muted-foreground text-xs">{t("persons.list_parts", { derived: nDerived, manual: saved.n_manual_entries ?? rows.length })}</p>}
      <p className="text-muted-foreground text-xs">
        <b>{t("settings.pseudonymisation.le_texte_libre_n_est")}</b> {saved.note}
      </p>
    </div>
  )
}

/** The entries that come from anonymised records: read-only, each linked to its fiche. */
function DerivedEntries({ entries, shadowed }: { entries: PseudonymEntry[]; shadowed: ShadowedEntry[] }) {
  if (entries.length === 0 && shadowed.length === 0) return null
  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-sm font-semibold">{t("persons.derived_entries")}</h3>
      <p className="text-muted-foreground text-xs">{t("persons.derived_help")}</p>
      {entries.length > 0 && (
        <table className="tbl" aria-label={t("persons.derived_table")}>
          <thead>
            <tr>
              <th style={{ width: 150 }}>{t("common.type")}</th>
              <th>{t("settings.pseudonymisation.nom_reel")}</th>
              <th>{t("settings.pseudonymisation.pseudonyme")}</th>
              <th style={{ width: 150 }}>{t("persons.source_record")}</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={`${e.person_id}:${e.original}`}>
                <td>{KIND_LABEL[e.kind]}</td>
                <td>{e.original}</td>
                <td>{e.pseudonym}</td>
                <td>
                  {e.person_id ? (
                    <a href={personHref(e.person_id)} aria-label={t("persons.open_record", { id: e.person_id, name: e.original })}>
                      {e.person_id}
                    </a>
                  ) : (
                    ""
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {shadowed.length > 0 && (
        <div className="flex flex-col gap-1">
          <p className="text-muted-foreground text-xs">{t("persons.shadowed_help")}</p>
          <ul className="m-0 list-none p-0 text-xs">
            {shadowed.map((s) => (
              <li key={s.original}>
                {s.original} {"\u2192"} {s.pseudonym}
                <span className="text-muted-foreground"> ({shadowReason(s.reason)})</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

/** The reason the server gives for a manual entry that does not apply, in plain words. */
function shadowReason(reason: string): string {
  const managed = /^managed by (P-\d+)$/.exec(reason)
  if (managed) return t("persons.shadowed_managed", { id: managed[1] })
  const flag = /^(P-\d+) is not anonymised$/.exec(reason)
  if (flag) return t("persons.shadowed_not_anonymised", { id: flag[1] })
  return reason
}
