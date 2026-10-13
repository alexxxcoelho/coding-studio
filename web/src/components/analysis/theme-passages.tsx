// The passages of a theme: those carried by its supporting codes (descendants
// included), with their interview, unit, extract and cell value. In Rédiger the
// cards are the source of the drags that create typed links.

import { PassageCard } from "@/components/analysis/shared"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import type { PassageLink, ThemePassage } from "@/lib/analysis-api"
import { passageRef } from "@/lib/analysis-text"
import { useThemePassages } from "@/lib/analysis-hooks"
import { CELL_VALUE_LABEL } from "@/lib/matrix"
import { THEME_PARTS } from "@/lib/analysis-api"

export function ThemePassageList({
  themeId,
  draggable,
  links = [],
  onLinkTo,
}: {
  themeId: string
  draggable?: boolean
  /** The links of the theme, to tell which passages are already used. */
  links?: PassageLink[]
  /** The keyboard way to link: choose a part for a passage. */
  onLinkTo?: (passage: ThemePassage, part: (typeof THEME_PARTS)[number]["key"]) => void
}) {
  const q = useThemePassages(themeId)
  if (q.isLoading) return <Skeleton className="h-32 w-full" />
  if (q.error) return <p className="text-destructive text-sm">{q.error.message}</p>
  const passages = q.data ?? []
  const interviews = new Set(passages.map((p) => p.interview_id)).size
  if (passages.length === 0) return <p className="text-muted-foreground text-sm">{t("analysis.theme_passages.aucun_passage_ajoutez_des_codes")}</p>
  const used = (id: string) => links.filter((l) => l.passage.passage_id === id).length
  return (
    <div className="flex flex-col gap-2">
      <p className="text-muted-foreground text-[11.5px]">
        {t("analysis.theme_passages.items_dans_items2", { items: t("common.n_passages", { n: passages.length }), items2: t("common.n_entretiens", { n: interviews }) })}
        {draggable && t("analysis.theme_passages.glissez_un_passage_dans_une")}
      </p>
      {passages.map((p) => (
        <PassageCard
          key={p.passage_id}
          passage={p}
          draggable={draggable}
          top={
            p.cell_value ? <span className="badge">{CELL_VALUE_LABEL[p.cell_value]}</span> : undefined
          }
          foot={
            <>
              {used(p.passage_id) > 0 && <span className="text-muted-foreground text-[11.5px]">{t("analysis.theme_passages.cite_used_fois", { used: used(p.passage_id) })}</span>}
              {onLinkTo && (
                <select
                  className="ctl-field ml-auto h-[22px] rounded-md px-1 text-xs"
                  aria-label={t("analysis.theme_passages.lier_passageref_a_une_partie", { passageRef: passageRef(p) })}
                  value=""
                  onChange={(e) => {
                    if (e.target.value) onLinkTo(p, e.target.value as (typeof THEME_PARTS)[number]["key"])
                  }}
                >
                  <option value="">{t("analysis.theme_passages.lier_a")}</option>
                  {THEME_PARTS.map((part) => (
                    <option key={part.key} value={part.key}>
                      {part.title}
                    </option>
                  ))}
                </select>
              )}
            </>
          }
        />
      ))}
    </div>
  )
}
