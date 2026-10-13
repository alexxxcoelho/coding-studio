// The code tree as a source of drags: a code is dragged into the theme sheet to
// become one of its supporting codes. A button does the same for the keyboard.

import { useMemo, useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { writeDrag } from "@/lib/analysis-dnd"
import { buildRows, codeColor, filterRows, isApplicable } from "@/lib/code-tree"
import type { CodeNode } from "@/lib/types"

export function CodeSource({ codes, inTheme, onAdd, disabled }: { codes: CodeNode[]; inTheme: ReadonlySet<string>; onAdd: (codeId: string) => void; disabled?: boolean }) {
  const [query, setQuery] = useState("")
  const rows = useMemo(() => (query.trim() ? filterRows(codes, query) : buildRows(codes)), [codes, query])
  return (
    <aside className="flex min-w-0 flex-col gap-2" aria-label={t("common.arbre_des_codes")}>
      <div className="flex flex-col gap-1.5 px-1.5">
        <h2 className="hs">{t("common.codes")}</h2>
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("common.filtrer_les_codes")} aria-label={t("common.filtrer_les_codes")} />
        <p className="text-muted-foreground text-[11.5px]">
          {t("analysis.code_source.glissez_un_code_dans_la")}
        </p>
      </div>
      <ul className="flex flex-col">
        {rows.map(({ node: c, depth }) => {
          const usable = isApplicable(c)
          const already = inTheme.has(c.code_id)
          return (
            <li
              key={c.code_id}
              draggable={usable && !disabled}
              onDragStart={(e) => writeDrag(e.dataTransfer, { kind: "code", id: c.code_id })}
              className="hover:bg-fill flex items-center gap-2 rounded-md py-1 pr-1.5 text-sm"
              style={{ paddingLeft: 8 + depth * 14, opacity: c.active ? 1 : 0.5, cursor: usable && !disabled ? "grab" : undefined }}
              data-code={c.code_id}
            >
              <span className="sw" style={{ ["--k" as string]: codeColor(c.code_id) }} />
              <span className="min-w-0 flex-1 truncate" title={c.definition || c.name}>
                {c.name}
              </span>
              <span className="text-muted-foreground text-[11.5px] tabular-nums" title={t("analysis.code_source.passages_sous_codes_compris")}>
                {c.counts.passages_rollup}
              </span>
              {usable &&
                (already ? (
                  <span className="text-muted-foreground w-6 text-center text-xs" title={t("analysis.code_source.deja_dans_le_theme")}>
                    ✓
                  </span>
                ) : (
                  <Button variant="ghost" size="icon-xs" aria-label={t("analysis.code_source.ajouter_name_au_theme", { name: c.name })} disabled={disabled} onClick={() => onAdd(c.code_id)}>
                    +
                  </Button>
                ))}
            </li>
          )
        })}
      </ul>
      {rows.length === 0 && <p className="text-muted-foreground px-1.5 text-sm">{t("common.aucun_code_ne_correspond")}</p>}
    </aside>
  )
}
