// Themes column of Analyser: title (a sentence), status, number of codes and of
// interviews, and the creation of a new theme.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { ThemeSummary } from "@/lib/analysis-api"
import { plural } from "@/lib/analysis-text"
import { STATUS_LABEL } from "@/lib/theme-status"
import { cn } from "@/lib/utils"

export function ThemeList({
  themes,
  selectedId,
  onSelect,
  onCreate,
  creating,
}: {
  themes: ThemeSummary[]
  selectedId: string | null
  onSelect: (id: string) => void
  onCreate: (title: string) => void
  creating?: boolean
}) {
  const [adding, setAdding] = useState(false)
  const [title, setTitle] = useState("")
  const submit = () => {
    const t = title.trim()
    if (!t) return
    onCreate(t)
    setTitle("")
    setAdding(false)
  }
  return (
    <aside className="flex min-w-0 flex-col gap-2" aria-label="Thèmes">
      <div className="flex items-center justify-between px-1.5">
        <h2 className="hs">Thèmes</h2>
        {!adding && (
          <Button variant="ghost" size="sm" onClick={() => setAdding(true)} disabled={creating}>
            Nouveau thème
          </Button>
        )}
      </div>
      {adding && (
        <form
          className="grp flex flex-col gap-2 p-2.5"
          onSubmit={(e) => {
            e.preventDefault()
            submit()
          }}
        >
          <label className="hs" htmlFor="new-theme-title">
            Titre du nouveau thème
          </label>
          <Input id="new-theme-title" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Une phrase qui affirme quelque chose" />
          <p className="text-muted-foreground text-[11.5px]">
            Un titre qui affirme, pas un sujet : « La non-adoption n'est pas toujours un échec : certaines petites bibliothèques n'identifient aucun besoin que le nouveau système de prêt réponde. »
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => setAdding(false)}>
              Annuler
            </Button>
            <Button type="submit" size="sm" disabled={!title.trim()}>
              Créer le thème
            </Button>
          </div>
        </form>
      )}
      {themes.length === 0 && !adding && (
        <p className="text-muted-foreground px-1.5 text-sm">
          Aucun thème pour l'instant. Un thème regroupe des codes et porte une interprétation : il se construit après le codage.
        </p>
      )}
      <ul className="flex flex-col gap-0.5">
        {themes.map((t) => {
          const on = t.theme_id === selectedId
          return (
            <li key={t.theme_id}>
              <button
                type="button"
                onClick={() => onSelect(t.theme_id)}
                aria-current={on ? "true" : undefined}
                className={cn("flex w-full flex-col gap-1 rounded-lg px-2.5 py-2 text-left text-sm", on ? "bg-fill2" : "hover:bg-fill")}
              >
                <span className="font-semibold">{t.title || "Thème sans titre"}</span>
                <span className="text-muted-foreground flex flex-wrap items-center gap-x-2 text-[11.5px]">
                  <span className="badge">{STATUS_LABEL[t.status]}</span>
                  <span>{plural(t.codes_count, "code")}</span>
                  <span>{plural(t.interviews_count, "entretien")}</span>
                </span>
              </button>
            </li>
          )
        })}
      </ul>
    </aside>
  )
}
