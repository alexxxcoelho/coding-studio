// Lecture par le cadre: retained themes against the dimensions of the framework
// of the study. What the framework explains, what it does not, what stays
// outside it, a complementary dimension and the limit of the evidence. The tab
// exists only if the study has a framework.

import { FrameworkRow } from "@/components/analysis/framework-reading"
import { EmptyState } from "@/components/bits"
import { t } from "@/i18n"
import type { ThemeSummary } from "@/lib/analysis-api"
import type { Framework } from "@/lib/types"

export function FrameworkTableTab({ themes, framework }: { themes: ThemeSummary[]; framework: Framework }) {
  if (themes.length === 0) {
    return <EmptyState title={t("writing.framework_table.aucun_theme_retenu")}>{t("writing.framework_table.le_tableau_croise_les_themes")}</EmptyState>
  }
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <p className="text-muted-foreground max-w-prose text-sm">
        {t("writing.framework_table.pour_chaque_theme_retenu_ce")}
      </p>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-sm" aria-label={t("common.lecture_par_le_cadre")}>
          <thead>
            <tr>
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                {t("common.theme")}
              </th>
              {framework.dimensions.map((d) => (
                <th key={d.id} scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold" title={d.definition}>
                  {d.label}
                </th>
              ))}
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                {t("common.hors_cadre")}
              </th>
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                {t("writing.framework_table.dimension_complementaire")}
              </th>
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                {t("common.limite_de_preuve")}
              </th>
            </tr>
          </thead>
          <tbody>
            {themes.map((t) => (
              <FrameworkRow key={t.theme_id} themeId={t.theme_id} title={t.title} framework={framework} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
