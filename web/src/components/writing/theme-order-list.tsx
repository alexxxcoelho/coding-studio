// The retained themes in the order of the chapter. Drag a theme onto another to
// move it; the buttons do the same from the keyboard. The order of the list
// becomes the order of the results chapter.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import { t } from "@/i18n"
import type { ThemeSummary } from "@/lib/analysis-api"
import { moveItem, readDrag, reorderByDrop, writeDrag } from "@/lib/analysis-dnd"
import { cn } from "@/lib/utils"

export function ThemeOrderList({ themes, selectedId, onSelect, onReorder }: { themes: ThemeSummary[]; selectedId: string | null; onSelect: (id: string) => void; onReorder: (ids: string[]) => void }) {
  const [overId, setOverId] = useState<string | null>(null)
  const ids = themes.map((tt) => tt.theme_id)
  return (
    <nav aria-label={t("writing.theme_order_list.themes_retenus")} className="flex flex-col gap-1">
      <h2 className="hs px-1.5">{t("writing.theme_order_list.themes_retenus")}</h2>
      <p className="text-muted-foreground px-1.5 text-[11.5px]">{t("writing.theme_order_list.l_ordre_de_la_liste")}</p>
      <ol className="flex flex-col gap-0.5">
        {themes.map((tt, i) => (
          <li
            key={tt.theme_id}
            draggable
            onDragStart={(e) => writeDrag(e.dataTransfer, { kind: "theme", id: tt.theme_id })}
            onDragOver={(e) => {
              if (e.dataTransfer.types.includes("application/x-cs-theme")) {
                e.preventDefault()
                setOverId(tt.theme_id)
              }
            }}
            onDragLeave={() => setOverId(null)}
            onDrop={(e) => {
              e.preventDefault()
              setOverId(null)
              const p = readDrag(e.dataTransfer, "theme")
              if (!p || p.id === tt.theme_id) return
              const next = reorderByDrop(ids, p.id, tt.theme_id)
              if (next !== ids) onReorder(next)
            }}
            className={cn("flex items-center gap-1 rounded-lg", tt.theme_id === selectedId && "bg-fill2", overId === tt.theme_id && "outline-2 outline-[var(--accent-c)]")}
          >
            <button type="button" onClick={() => onSelect(tt.theme_id)} aria-current={tt.theme_id === selectedId ? "true" : undefined} className="flex min-w-0 flex-1 items-baseline gap-2 px-2 py-1.5 text-left text-sm">
              <span className="text-muted-foreground tabular-nums">{i + 1}</span>
              <span className="font-medium">{tt.title || t("common.theme_sans_titre")}</span>
            </button>
            <Button variant="ghost" size="icon-xs" aria-label={`Monter ${tt.title}`} disabled={i === 0} onClick={() => onReorder(moveItem(ids, i, i - 1))}>
              ↑
            </Button>
            <Button variant="ghost" size="icon-xs" aria-label={`Descendre ${tt.title}`} disabled={i === themes.length - 1} onClick={() => onReorder(moveItem(ids, i, i + 1))}>
              ↓
            </Button>
          </li>
        ))}
      </ol>
    </nav>
  )
}
