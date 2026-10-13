// Lecture par le cadre: retained themes against the dimensions of the framework
// of the study. What the framework explains, what it does not, what stays
// outside it, a complementary dimension and the limit of the evidence. The tab
// exists only if the study has a framework.

import { FrameworkRow } from "@/components/analysis/framework-reading"
import { EmptyState } from "@/components/bits"
import type { ThemeSummary } from "@/lib/analysis-api"
import type { Framework } from "@/lib/types"

export function FrameworkTableTab({ themes, framework }: { themes: ThemeSummary[]; framework: Framework }) {
  if (themes.length === 0) {
    return <EmptyState title="Aucun thème retenu">Le tableau croise les thèmes retenus et les dimensions du cadre. Retenez d'abord des thèmes dans Analyser.</EmptyState>
  }
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <p className="text-muted-foreground max-w-prose text-sm">
        Pour chaque thème retenu : ce que le cadre explique, ce qu'il n'explique pas, ce qui reste hors cadre. Ajoutez une dimension complémentaire, que le cadre ne contient pas (par exemple qui paie), et une limite de preuve. Une part hors cadre est normale ; si tout rentre sans reste, relisez.
      </p>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-sm" aria-label="Lecture par le cadre">
          <thead>
            <tr>
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                Thème
              </th>
              {framework.dimensions.map((d) => (
                <th key={d.id} scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold" title={d.definition}>
                  {d.label}
                </th>
              ))}
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                Hors cadre
              </th>
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                Dimension complémentaire
              </th>
              <th scope="col" className="border-b px-3 py-2 text-left text-xs font-semibold">
                Limite de preuve
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
