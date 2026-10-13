// The retained themes in the order of the chapter. Drag a theme onto another to
// move it; the buttons do the same from the keyboard. The order of the list
// becomes the order of the results chapter.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import type { ThemeSummary } from "@/lib/analysis-api"
import { moveItem, readDrag, reorderByDrop, writeDrag } from "@/lib/analysis-dnd"
import { cn } from "@/lib/utils"

export function ThemeOrderList({ themes, selectedId, onSelect, onReorder }: { themes: ThemeSummary[]; selectedId: string | null; onSelect: (id: string) => void; onReorder: (ids: string[]) => void }) {
  const [overId, setOverId] = useState<string | null>(null)
  const ids = themes.map((t) => t.theme_id)
  return (
    <nav aria-label="Thèmes retenus" className="flex flex-col gap-1">
      <h2 className="hs px-1.5">Thèmes retenus</h2>
      <p className="text-muted-foreground px-1.5 text-[11.5px]">L'ordre de la liste devient l'ordre du chapitre des résultats. Glissez un thème pour le déplacer.</p>
      <ol className="flex flex-col gap-0.5">
        {themes.map((t, i) => (
          <li
            key={t.theme_id}
            draggable
            onDragStart={(e) => writeDrag(e.dataTransfer, { kind: "theme", id: t.theme_id })}
            onDragOver={(e) => {
              if (e.dataTransfer.types.includes("application/x-cs-theme")) {
                e.preventDefault()
                setOverId(t.theme_id)
              }
            }}
            onDragLeave={() => setOverId(null)}
            onDrop={(e) => {
              e.preventDefault()
              setOverId(null)
              const p = readDrag(e.dataTransfer, "theme")
              if (!p || p.id === t.theme_id) return
              const next = reorderByDrop(ids, p.id, t.theme_id)
              if (next !== ids) onReorder(next)
            }}
            className={cn("flex items-center gap-1 rounded-lg", t.theme_id === selectedId && "bg-fill2", overId === t.theme_id && "outline-2 outline-[var(--accent-c)]")}
          >
            <button type="button" onClick={() => onSelect(t.theme_id)} aria-current={t.theme_id === selectedId ? "true" : undefined} className="flex min-w-0 flex-1 items-baseline gap-2 px-2 py-1.5 text-left text-sm">
              <span className="text-muted-foreground tabular-nums">{i + 1}</span>
              <span className="font-medium">{t.title || "Thème sans titre"}</span>
            </button>
            <Button variant="ghost" size="icon-xs" aria-label={`Monter ${t.title}`} disabled={i === 0} onClick={() => onReorder(moveItem(ids, i, i - 1))}>
              ↑
            </Button>
            <Button variant="ghost" size="icon-xs" aria-label={`Descendre ${t.title}`} disabled={i === themes.length - 1} onClick={() => onReorder(moveItem(ids, i, i + 1))}>
              ↓
            </Button>
          </li>
        ))}
      </ol>
    </nav>
  )
}
