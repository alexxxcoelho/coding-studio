// Co-occurrence of codes: for each pair of codes, the number of units where both
// are found, by one of three relations, for all the interviews or grouped by
// interview or by an attribute. Cells are tinted with one accent in five
// alpha steps and always print their number; a click lists the passages.

import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { EmptyState } from "@/components/bits"
import { t } from "@/i18n"
import { useProjectConfig } from "@/lib/hooks"
import { unitHref } from "@/lib/router"
import { v2, type CoParams, type CoRelation } from "@/lib/v2-api"
import { cellOf, heatAlpha, maxUnits } from "@/lib/v2-text"
import { cn } from "@/lib/utils"

const RELATIONS: { id: CoRelation; label: string; hint: string }[] = [
  { id: "same_passage", get label() { return t("analysis.cooccurrence.meme_passage") }, get hint() { return t("analysis.cooccurrence.les_deux_codes_sont_poses") } },
  { id: "overlap", get label() { return t("analysis.cooccurrence.passages_qui_se_recouvrent") }, get hint() { return t("analysis.cooccurrence.deux_passages_codes_differemment_se") } },
  { id: "same_unit", get label() { return t("analysis.cooccurrence.meme_tour_de_parole") }, get hint() { return t("analysis.cooccurrence.les_deux_codes_sont_dans") } },
]

interface Drill {
  a: string
  b: string
  aName: string
  bName: string
  group: string
}

export function CooccurrenceTab() {
  const cfg = useProjectConfig()
  const attributes = cfg.data?.attributes ?? []
  const [relation, setRelation] = useState<CoRelation>("same_passage")
  const [groupBy, setGroupBy] = useState<"" | "interview" | "attribute">("")
  const [attribute, setAttribute] = useState("")
  const [rollup, setRollup] = useState(false)
  const [minUnits, setMinUnits] = useState(1)
  const [groupKey, setGroupKey] = useState("")
  const [drill, setDrill] = useState<Drill | null>(null)

  const params: CoParams = { relation, group_by: groupBy, attribute: groupBy === "attribute" ? attribute : undefined, rollup, min_units: minUnits }
  const ready = groupBy !== "attribute" || attribute !== ""
  const q = useQuery({ queryKey: ["cooccurrence", params], queryFn: () => v2.cooccurrence(params), enabled: ready, placeholderData: (p) => p })
  const data = q.data
  const group = data?.groups.find((g) => g.key === groupKey) ?? data?.groups[0]
  const max = useMemo(() => (data ? maxUnits(data.groups) : 0), [data])
  const codes = data?.codes ?? []
  // Codes with no pair above the threshold in this group stay out of the table.
  const shown = useMemo(() => {
    if (!group) return []
    return codes.filter((c) => group.cells.some((x) => x.a === c.code_id || x.b === c.code_id) || group.diagonal.some((d) => d.code_id === c.code_id && d.units > 0))
  }, [codes, group])

  const evidence = useQuery({
    queryKey: ["cooccurrence-passages", drill?.a, drill?.b, relation, rollup],
    queryFn: () => v2.cooccurrencePassages(drill!.a, drill!.b, { relation, rollup }),
    enabled: drill !== null,
  })

  return (
    <div className="flex flex-col gap-3" style={{ padding: "6px 2px" }}>
      <div className="r8" role="group" aria-label={t("analysis.cooccurrence.reglages_de_la_co_occurrence")}>
        <label className="r8 small">
          {t("analysis.cooccurrence.relation")}
          <NativeSelect aria-label={t("analysis.cooccurrence.relation")} value={relation} style={{ width: 210 }} onChange={(e) => setRelation(e.target.value as CoRelation)}>
            {RELATIONS.map((r) => (
              <option key={r.id} value={r.id}>
                {r.label}
              </option>
            ))}
          </NativeSelect>
        </label>
        <label className="r8 small">
          {t("analysis.cooccurrence.grouper_par")}
          <NativeSelect
            aria-label={t("analysis.cooccurrence.grouper_par")}
            value={groupBy}
            style={{ width: 150 }}
            onChange={(e) => {
              setGroupBy(e.target.value as typeof groupBy)
              setGroupKey("")
            }}
          >
            <option value="">{t("analysis.cooccurrence.aucun_tout")}</option>
            <option value="interview">{t("common.entretien")}</option>
            <option value="attribute">{t("common.attribut")}</option>
          </NativeSelect>
        </label>
        {groupBy === "attribute" && (
          <NativeSelect
            aria-label={t("analysis.cooccurrence.attribut_de_regroupement")}
            value={attribute}
            style={{ width: 150 }}
            onChange={(e) => {
              setAttribute(e.target.value)
              setGroupKey("")
            }}
          >
            <option value="">{t("common.attribut_2")}</option>
            {attributes.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </NativeSelect>
        )}
        <label className="r8 small">
          <input type="checkbox" checked={rollup} onChange={(e) => setRollup(e.target.checked)} style={{ accentColor: "var(--accent-fill)" }} />
          {t("analysis.cooccurrence.avec_les_sous_codes")}
        </label>
        <label className="r8 small">
          {t("analysis.cooccurrence.seuil")}
          <input
            type="number"
            min={1}
            aria-label={t("analysis.cooccurrence.nombre_minimal_de_tours")}
            value={minUnits}
            onChange={(e) => setMinUnits(Math.max(1, Number(e.target.value) || 1))}
            className="field"
            style={{ width: 56, height: 26 }}
          />
          {t("analysis.cooccurrence.tours_au_moins")}
        </label>
      </div>
      <p className="small l2" style={{ margin: 0 }}>
        {RELATIONS.find((r) => r.id === relation)?.hint}
      </p>

      {data && data.groups.length > 1 && (
        <label className="r8 small">
          {t("analysis.cooccurrence.groupe")}
          <NativeSelect aria-label={t("analysis.cooccurrence.groupe_affiche")} value={group?.key ?? ""} style={{ width: 220 }} onChange={(e) => setGroupKey(e.target.value)}>
            {data.groups.map((g) => (
              <option key={g.key} value={g.key}>
                {t("analysis.cooccurrence.label_n_entretiens", { n: g.interviews, label: g.label })}
              </option>
            ))}
          </NativeSelect>
        </label>
      )}

      {!ready && <EmptyState title={t("analysis.cooccurrence.choisissez_un_attribut")}>{t("analysis.cooccurrence.les_entretiens_seront_regroupes_selo")}</EmptyState>}
      {ready && q.isLoading && <Skeleton className="h-40 w-full" />}
      {q.isError && (
        <p role="alert" className="note-err">
          {q.error.message}
        </p>
      )}
      {data && shown.length === 0 && <EmptyState title={t("analysis.cooccurrence.aucune_co_occurrence")}>{t("analysis.cooccurrence.aucun_couple_de_codes_n")}</EmptyState>}
      {group && shown.length > 0 && (
        <div style={{ overflowX: "auto" }}>
          <table className="tbl" aria-label={t("analysis.cooccurrence.co_occurrences_nombre_de_tours")} style={{ width: "auto" }}>
            <thead>
              <tr>
                <th scope="col">{t("common.code")}</th>
                {shown.map((c) => (
                  <th key={c.code_id} scope="col" style={{ textAlign: "center", maxWidth: 110, whiteSpace: "normal" }}>
                    {c.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {shown.map((r) => (
                <tr key={r.code_id}>
                  <th scope="row" style={{ whiteSpace: "normal", maxWidth: 190, fontSize: 13, fontWeight: 500, color: "var(--label)" }}>
                    {r.name}
                  </th>
                  {shown.map((c) => {
                    if (r.code_id === c.code_id) {
                      const d = group.diagonal.find((x) => x.code_id === r.code_id)
                      return (
                        <td key={c.code_id} style={{ textAlign: "center", color: "var(--label2)" }} title={t("analysis.cooccurrence.tours_ou_ce_code_est")}>
                          {d?.units ?? 0}
                        </td>
                      )
                    }
                    const cell = cellOf(group, r.code_id, c.code_id)
                    const n = cell?.units ?? 0
                    const alpha = heatAlpha(n, max)
                    return (
                      <td
                        key={c.code_id}
                        style={{
                          textAlign: "center",
                          padding: 2,
                          fontVariantNumeric: "tabular-nums",
                        }}
                      >
                        {n > 0 ? (
                          <button
                            type="button"
                            className={cn("heat-cell")}
                            style={{ background: `color-mix(in srgb, var(--accent-fill) ${Math.round(alpha * 100)}%, transparent)` }}
                            aria-label={`${r.name} et ${c.name} : ${n} tour${n > 1 ? "s" : ""}, ${cell!.interviews} entretien${cell!.interviews > 1 ? "s" : ""}`}
                            onClick={() => setDrill({ a: r.code_id, b: c.code_id, aName: r.name, bName: c.name, group: group.label })}
                          >
                            {n}
                          </button>
                        ) : (
                          <span className="l2">0</span>
                        )}
                      </td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {data?.note && <p className="small l2" style={{ margin: 0 }}>{data.note}</p>}

      {drill && (
        <section className="grp" style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }} aria-label={t("analysis.cooccurrence.passages_de_la_cellule")}>
          <div className="r8" style={{ justifyContent: "space-between" }}>
            <strong>
              {t("analysis.cooccurrence.aname_et_bname", { aName: drill.aName, bName: drill.bName })}
            </strong>
            <Button variant="ghost" size="sm" onClick={() => setDrill(null)}>
              {t("common.fermer")}
            </Button>
          </div>
          {evidence.isLoading && <Skeleton className="h-16 w-full" />}
          {evidence.data && (
            <>
              <p className="small l2" style={{ margin: 0 }}>
                {t("analysis.cooccurrence.n_tours_au_total", { n: evidence.data.n_units })}
                {groupBy ? t("analysis.cooccurrence.tous_les_groupes_confondus") : ""}.
              </p>
              {evidence.data.items.map((it) => (
                <div key={it.unit_id} style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                  <a className="mono small" href={unitHref(it.unit_id)}>
                    {it.interview_id} {it.unit_id.replace(/^.*-(U\d+)$/, "$1")}
                  </a>
                  <p className="verbatim" style={{ margin: 0 }} data-no-i18n>
                    {it.text}
                  </p>
                </div>
              ))}
            </>
          )}
        </section>
      )}
    </div>
  )
}
