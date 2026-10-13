// The matrix of Analyser: rows and columns chosen among the lists of the study,
// an attribute filter, saved views, cells that show a closed value, the passage
// count and a one-line summary, and the panel of the selected cell.

import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

import { AutoInput, PassageCard } from "@/components/analysis/shared"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { interviewAttributes } from "@/lib/attributes"
import { analysisApi, CELL_VALUES, type CellValue, type MatrixAxisEntry, type MatrixCell, type MatrixConfig, type MatrixView } from "@/lib/analysis-api"
import { akeys, useMatrix } from "@/lib/analysis-hooks"
import { axisOptions, axisToken, CELL_VALUE_LABEL, cellView, DEFAULT_MATRIX, matrixMax, parseAxisToken, validateMatrix } from "@/lib/matrix"
import { reportError } from "@/lib/notify"
import { unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { ProjectConfig } from "@/lib/types"

const HEAT = ["", "bg-[color-mix(in_srgb,var(--accent-c)_9%,transparent)]", "bg-[color-mix(in_srgb,var(--accent-c)_17%,transparent)]", "bg-[color-mix(in_srgb,var(--accent-c)_27%,transparent)]", "bg-[color-mix(in_srgb,var(--accent-c)_37%,transparent)]"]

export interface SelectedCell {
  row: MatrixAxisEntry
  col: MatrixAxisEntry
}

function MatrixControls({
  config,
  onChange,
  project,
  problem,
}: {
  config: MatrixConfig
  onChange: (c: MatrixConfig) => void
  project: ProjectConfig | undefined
  problem: string | null
}) {
  const qc = useQueryClient()
  const options = axisOptions(project)
  const views = useQuery({ queryKey: akeys.matrixViews, queryFn: analysisApi.matrixViews })
  const [naming, setNaming] = useState(false)
  const [name, setName] = useState("")
  const attribute = interviewAttributes(project?.attributes).find((a) => a.id === config.filter?.attribute)

  const side = (label: string, key: "rows" | "cols") => (
    <label className="flex items-center gap-1.5 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <NativeSelect
        aria-label={label}
        value={axisToken(config[key])}
        onChange={(e) => {
          const src = parseAxisToken(e.target.value)
          if (src) onChange({ ...config, [key]: src })
        }}
        style={{ width: 170 }}
      >
        {options.map((o) => (
          <option key={o.token} value={o.token}>
            {o.label}
          </option>
        ))}
      </NativeSelect>
    </label>
  )

  const saveView = async () => {
    const n = name.trim()
    if (!n) return
    try {
      await analysisApi.saveMatrixView(n, config)
      await qc.invalidateQueries({ queryKey: akeys.matrixViews })
      setNaming(false)
      setName("")
    } catch (e) {
      reportError(e, t("analysis.matrix_view.vue_non_enregistree"))
    }
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-3">
        {side(t("analysis.matrix_view.lignes"), "rows")}
        {side(t("analysis.matrix_view.colonnes"), "cols")}
        <label className="flex items-center gap-1.5 text-xs">
          <span className="text-muted-foreground">{t("analysis.matrix_view.filtrer_par_attribut")}</span>
          <NativeSelect
            aria-label={t("analysis.matrix_view.filtrer_par_attribut")}
            value={config.filter?.attribute ?? ""}
            onChange={(e) => onChange({ ...config, filter: e.target.value ? { attribute: e.target.value, value: "" } : null })}
            style={{ width: 150 }}
          >
            <option value="">{t("analysis.matrix_view.aucun_filtre")}</option>
            {interviewAttributes(project?.attributes).map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </NativeSelect>
        </label>
        {config.filter && (
          <label className="flex items-center gap-1.5 text-xs">
            <span className="text-muted-foreground">{t("common.valeur")}</span>
            {attribute && attribute.kind === "choice" ? (
              <NativeSelect aria-label={t("analysis.matrix_view.valeur_du_filtre")} value={config.filter.value} onChange={(e) => onChange({ ...config, filter: { attribute: config.filter!.attribute, value: e.target.value } })} style={{ width: 140 }}>
                <option value="">{t("common.choisir")}</option>
                {attribute.choices.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </NativeSelect>
            ) : (
              <Input aria-label={t("analysis.matrix_view.valeur_du_filtre")} value={config.filter.value} onChange={(e) => onChange({ ...config, filter: { attribute: config.filter!.attribute, value: e.target.value } })} style={{ width: 140 }} />
            )}
          </label>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {(views.data ?? []).length > 0 && (
          <label className="flex items-center gap-1.5 text-xs">
            <span className="text-muted-foreground">{t("analysis.matrix_view.vues_enregistrees")}</span>
            <NativeSelect
              aria-label={t("analysis.matrix_view.vues_enregistrees")}
              value=""
              onChange={(e) => {
                const v = (views.data ?? []).find((x: MatrixView) => x.view_id === e.target.value)
                if (v) onChange(v.config)
              }}
              style={{ width: 170 }}
            >
              <option value="">{t("analysis.matrix_view.appliquer_une_vue")}</option>
              {(views.data ?? []).map((v: MatrixView) => (
                <option key={v.view_id} value={v.view_id}>
                  {v.name}
                </option>
              ))}
            </NativeSelect>
          </label>
        )}
        {naming ? (
          <form
            className="flex items-center gap-1.5"
            onSubmit={(e) => {
              e.preventDefault()
              void saveView()
            }}
          >
            <Input aria-label={t("analysis.matrix_view.nom_de_la_vue")} autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t("analysis.matrix_view.nom_de_la_vue")} style={{ width: 170 }} />
            <Button type="submit" size="sm" disabled={!name.trim() || !!problem}>
              {t("common.enregistrer")}
            </Button>
            <Button type="button" variant="ghost" size="sm" onClick={() => setNaming(false)}>
              {t("common.annuler")}
            </Button>
          </form>
        ) : (
          <Button variant="outline" size="sm" onClick={() => setNaming(true)} disabled={!!problem}>
            {t("analysis.matrix_view.enregistrer_la_vue")}
          </Button>
        )}
        <Button variant="ghost" size="sm" onClick={() => onChange(DEFAULT_MATRIX)}>
          {t("analysis.matrix_view.reinitialiser")}
        </Button>
      </div>
    </div>
  )
}

function Cell({ cell, row, selected, max, onSelect, colLabel }: { cell: MatrixCell | undefined; row: MatrixAxisEntry; selected: boolean; max: number; onSelect: () => void; colLabel: string }) {
  const v = cellView(cell, row.coded, max)
  if (v.state === "not_coded") return null
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      aria-label={`${row.label}, ${colLabel}`}
      className={cn(
        "flex min-h-[66px] w-full flex-col gap-0.5 rounded-lg p-2 text-left",
        v.state === "none" ? "text-muted-foreground items-center justify-center text-xs shadow-[inset_0_0_0_.5px_var(--sep)]" : HEAT[v.heat] + " text-foreground",
        selected && "shadow-[0_0_0_2.5px_var(--accent-fill)]",
      )}
    >
      {v.state === "none" ? (
        v.label
      ) : (
        <>
          <span className="text-[13px] font-semibold">
            {v.label}
            {v.contradiction && <span className="text-warn ml-1.5 text-[11px]">{t("common.a_traiter")}</span>}
          </span>
          <span className="text-foreground/80 text-[11.5px]">{v.countText}</span>
          {v.summary && <span className="text-xs leading-snug">{v.summary}</span>}
        </>
      )}
    </button>
  )
}

function CellPanel({ selected, filter, onClose }: { selected: SelectedCell; filter: MatrixConfig["filter"]; onClose: () => void }) {
  const qc = useQueryClient()
  const { row, col } = selected
  const q = useQuery({ queryKey: akeys.cell(row.key, col.key, filter), queryFn: () => analysisApi.matrixCell(row.key, col.key, filter) })
  const write = async (value: CellValue | null, summary: string) => {
    try {
      const saved = await analysisApi.setMatrixCell({ row_key: row.key, col_key: col.key, value, summary })
      qc.setQueryData(akeys.cell(row.key, col.key, filter), saved)
      void qc.invalidateQueries({ queryKey: ["matrix"] })
    } catch (e) {
      reportError(e, t("analysis.matrix_view.cellule_non_enregistree"))
      throw e
    }
  }
  const d = q.data
  return (
    <aside className="grp flex flex-col gap-3 p-3" aria-label={`Cellule ${row.label}, ${col.label}`}>
      <div className="flex items-start justify-between gap-2">
        <div>
          <div className="hs">{t("analysis.matrix_view.cellule")}</div>
          <div className="text-sm font-semibold">
            <span className="mono">{row.label}</span> × {col.label}
          </div>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose}>
          {t("common.fermer")}
        </Button>
      </div>
      {q.isLoading || !d ? (
        <Skeleton className="h-24 w-full" />
      ) : (
        <>
          <label className="flex max-w-xs flex-col gap-1">
            <span className="hs">{t("analysis.matrix_view.valeur_de_cellule")}</span>
            <NativeSelect aria-label={t("analysis.matrix_view.valeur_de_cellule")} value={d.value ?? ""} onChange={(e) => void write((e.target.value || null) as CellValue | null, d.summary).catch(() => {})}>
              <option value="">{t("common.a_choisir")}</option>
              {CELL_VALUES.map((v) => (
                <option key={v} value={v}>
                  {CELL_VALUE_LABEL[v]}
                </option>
              ))}
            </NativeSelect>
          </label>
          <AutoInput label={t("analysis.matrix_view.resume_de_la_cellule")} value={d.summary} onSave={(v) => write(d.value, v)} placeholder={t("analysis.matrix_view.un_resume_d_une_ligne")} />
          <p className="text-muted-foreground text-[11.5px]">
            {t("analysis.matrix_view.le_nombre_de_passages_est", { items: t("common.n_passages", { n: d.passages.length }), items2: t("common.n_entretiens", { n: d.n_interviews }) })}
          </p>
          <div className="flex flex-col gap-2">
            {d.passages.length === 0 && <p className="text-muted-foreground text-sm">{t("analysis.matrix_view.aucun_passage_pour_cette_cellule")}</p>}
            {d.passages.map((p) => (
              <PassageCard key={p.passage_id} passage={p} foot={
                  <a href={unitHref(p.unit_id)} className="text-accent-text ml-auto text-[11.5px] hover:underline">
                    {t("common.ouvrir_dans_le_coder")}
                  </a>
                } />
            ))}
          </div>
        </>
      )}
    </aside>
  )
}

export function MatrixTab({ project }: { project: ProjectConfig | undefined }) {
  const [config, setConfig] = useState<MatrixConfig>(DEFAULT_MATRIX)
  const [selected, setSelected] = useState<SelectedCell | null>(null)
  const problem = validateMatrix(config, project)
  const q = useMatrix(config, project)
  const m = q.data
  const cellAt = new Map<string, MatrixCell>((m?.cells ?? []).map((c) => [`${c.row_key}|${c.col_key}`, c]))
  const max = matrixMax(m?.cells ?? [])

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <MatrixControls
        config={config}
        onChange={(c) => {
          setConfig(c)
          setSelected(null)
        }}
        project={project}
        problem={problem}
      />
      {problem && (
        <p role="alert" className="note-warn">
          {problem}
        </p>
      )}
      {!problem && q.isLoading && <Skeleton className="h-64 w-full" />}
      {q.error && <p className="text-destructive text-sm">{q.error.message}</p>}
      {m && (m.rows.length === 0 || m.cols.length === 0) && (
        <p className="text-muted-foreground text-sm">
          {m.cols.length === 0 ? t("analysis.matrix_view.aucune_colonne_a_afficher_creez") : t("analysis.matrix_view.aucune_ligne_a_afficher_avec")}
        </p>
      )}
      {m && m.rows.length > 0 && m.cols.length > 0 && (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[540px] border-separate border-spacing-1.5" aria-label={t("common.matrice")}>
            <thead>
              <tr>
                <th />
                {m.cols.map((c) => (
                  <th key={c.key} scope="col" className="text-muted-foreground px-1.5 pb-0.5 text-left text-[11.5px] font-medium">
                    {c.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {m.rows.map((r) => (
                <tr key={r.key}>
                  <th scope="row" className={cn("mono w-[90px] px-1.5 text-left font-semibold whitespace-nowrap", !r.coded && "text-muted-foreground")}>
                    {r.label}
                  </th>
                  {r.coded ? (
                    m.cols.map((c) => (
                      <td key={c.key}>
                        <Cell
                          cell={cellAt.get(`${r.key}|${c.key}`)}
                          row={r}
                          colLabel={c.label}
                          max={max}
                          selected={selected?.row.key === r.key && selected.col.key === c.key}
                          onSelect={() => setSelected({ row: r, col: c })}
                        />
                      </td>
                    ))
                  ) : (
                    <td colSpan={m.cols.length}>
                      <div className="text-muted-foreground flex min-h-10 items-center justify-center rounded-lg text-xs shadow-[inset_0_0_0_.5px_var(--sep)]">{t("analysis.matrix_view.pas_encore_code_exclu_de")}</div>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="text-muted-foreground text-[11.5px]">
        {t("analysis.matrix_view.un_resume_d_une_ligne_2")}
      </p>
      {selected && <CellPanel key={`${selected.row.key}|${selected.col.key}`} selected={selected} filter={config.filter} onClose={() => setSelected(null)} />}
    </div>
  )
}
