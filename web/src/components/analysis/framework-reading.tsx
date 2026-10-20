import {TableRow} from '@/components/ui/table'
// Reading of the themes by the interpretive framework of the study, if it has
// one. It appears in Analyser (a selector on the theme sheet) and in Rédiger (a
// table) and nowhere else: never in the coding screens nor in the codebook.

import { useFrameworkEditor, type FrameworkBody as Body } from "@/components/analysis/use-framework-editor"
import { AutoInput } from "@/components/analysis/shared"
import { Checkbox } from "@/components/ui/checkbox"
import { t } from "@/i18n"
import type { Framework } from "@/lib/types"

const withCell = (r: Body, dim: string, change: Partial<{ explains: boolean; note: string }>): Body => ({
  ...r,
  cells: { ...r.cells, [dim]: { explains: r.cells[dim]?.explains ?? false, note: r.cells[dim]?.note ?? "", ...change } },
})

/** The selector of the theme sheet: the dimensions of the framework and "hors cadre". */
export function FrameworkChecks({ themeId, framework }: { themeId: string; framework: Framework }) {
  const { reading, patch } = useFrameworkEditor(themeId)
  return (
    <fieldset className="flex flex-col gap-1.5">
      <legend className="hs mb-1">{t("common.lecture_par_le_cadre")}</legend>
      <div className="flex flex-wrap gap-x-4 gap-y-1.5">
        {framework.dimensions.map((d) => (
          <label key={d.id} className="flex items-center gap-1.5 text-sm" title={d.definition}>
            <Checkbox
              checked={reading.cells[d.id]?.explains ?? false}
              aria-label={t("analysis.framework_reading.label_le_theme_l_explique", { label: d.label })}
              onCheckedChange={(v) => void patch((r) => withCell(r, d.id, { explains: v === true })).catch(() => {})}
            />
            {d.label}
          </label>
        ))}
        <label className="flex items-center gap-1.5 text-sm">
          <Checkbox
            checked={reading.out_of_frame}
            aria-label={t("common.hors_cadre")}
            onCheckedChange={(v) => void patch((r) => ({ ...r, out_of_frame: v === true })).catch(() => {})}
          />
          {t("common.hors_cadre")}
        </label>
      </div>
      <p className="text-muted-foreground text-[11.5px]">
        {t("analysis.framework_reading.cochez_ce_que_le_theme")}
      </p>
    </fieldset>
  )
}

/** One row of the framework table of Rédiger: a retained theme read through each dimension. */
export function FrameworkRow({ themeId, title, framework }: { themeId: string; title: string; framework: Framework }) {
  const { reading, patch } = useFrameworkEditor(themeId)
  const save = <K extends keyof Body>(key: K) => (v: Body[K]) => patch((r) => ({ ...r, [key]: v }))
  return (
    <TableRow className="align-top">
      <th scope="row" className="min-w-48 border-b px-3 py-2 text-left text-sm font-medium">
        {title}
      </th>
      {framework.dimensions.map((d) => (
        <td key={d.id} className="min-w-44 border-b px-3 py-2">
          <label className="mb-1 flex items-center gap-1.5 text-sm">
            <Checkbox
              checked={reading.cells[d.id]?.explains ?? false}
              aria-label={t("analysis.framework_reading.title_label_le_cadre_l", { title, label: d.label })}
              onCheckedChange={(v) => void patch((r) => withCell(r, d.id, { explains: v === true })).catch(() => {})}
            />
            {t("analysis.framework_reading.explique")}
          </label>
          <AutoInput
            label={t("analysis.framework_reading.title_label_ce_que_le", { title, label: d.label })}
            hideLabel
            placeholder={t("analysis.framework_reading.ce_qui_echappe")}
            value={reading.cells[d.id]?.note ?? ""}
            onSave={(v) => patch((r) => withCell(r, d.id, { note: v }))}
          />
        </td>
      ))}
      <td className="min-w-44 border-b px-3 py-2">
        <label className="mb-1 flex items-center gap-1.5 text-sm">
          <Checkbox
            checked={reading.out_of_frame}
            aria-label={`${title} : hors cadre`}
            onCheckedChange={(v) => void patch((r) => ({ ...r, out_of_frame: v === true })).catch(() => {})}
          />
          {t("common.hors_cadre")}
        </label>
        <AutoInput label={t("analysis.framework_reading.title_ce_qui_reste_hors", { title })} hideLabel placeholder={t("analysis.framework_reading.ce_qui_reste_hors_cadre")} value={reading.out_of_frame_note} onSave={save("out_of_frame_note")} />
      </td>
      <td className="min-w-44 border-b px-3 py-2">
        <AutoInput label={t("analysis.framework_reading.title_dimension_complementaire", { title })} hideLabel placeholder={t("analysis.framework_reading.par_exemple_qui_paie")} value={reading.complementary_dimension} onSave={save("complementary_dimension")} />
      </td>
      <td className="min-w-44 border-b px-3 py-2">
        <AutoInput label={t("analysis.framework_reading.title_limite_de_preuve", { title })} hideLabel placeholder={t("common.limite_de_preuve")} value={reading.evidence_limit} onSave={save("evidence_limit")} />
      </td>
    </TableRow>
  )
}
