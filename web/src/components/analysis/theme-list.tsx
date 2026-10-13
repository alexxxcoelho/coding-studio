// Themes column of Analyser: title (a sentence), status, number of codes and of
// interviews, and the creation of a new theme.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import type { ThemeSummary } from "@/lib/analysis-api"
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
    const tt = title.trim()
    if (!tt) return
    onCreate(tt)
    setTitle("")
    setAdding(false)
  }
  return (
    <aside className="flex min-w-0 flex-col gap-2" aria-label={t("common.themes")}>
      <div className="flex items-center justify-between px-1.5">
        <h2 className="hs">{t("common.themes")}</h2>
        {!adding && (
          <Button variant="ghost" size="sm" onClick={() => setAdding(true)} disabled={creating}>
            {t("analysis.theme_list.nouveau_theme")}
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
            {t("analysis.theme_list.titre_du_nouveau_theme")}
          </label>
          <Input id="new-theme-title" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder={t("analysis.theme_list.une_phrase_qui_affirme_quelque")} />
          <p className="text-muted-foreground text-[11.5px]">
            {t("analysis.theme_list.un_titre_qui_affirme_pas")}
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => setAdding(false)}>
              {t("common.annuler")}
            </Button>
            <Button type="submit" size="sm" disabled={!title.trim()}>
              {t("analysis.theme_list.creer_le_theme")}
            </Button>
          </div>
        </form>
      )}
      {themes.length === 0 && !adding && (
        <p className="text-muted-foreground px-1.5 text-sm">
          {t("analysis.theme_list.aucun_theme_pour_l_instant")}
        </p>
      )}
      <ul className="flex flex-col gap-0.5">
        {themes.map((tt) => {
          const on = tt.theme_id === selectedId
          return (
            <li key={tt.theme_id}>
              <button
                type="button"
                onClick={() => onSelect(tt.theme_id)}
                aria-current={on ? "true" : undefined}
                className={cn("flex w-full flex-col gap-1 rounded-lg px-2.5 py-2 text-left text-sm", on ? "bg-fill2" : "hover:bg-fill")}
              >
                <span className="font-semibold">{tt.title || t("common.theme_sans_titre")}</span>
                <span className="text-muted-foreground flex flex-wrap items-center gap-x-2 text-[11.5px]">
                  <span className="badge">{STATUS_LABEL[tt.status]}</span>
                  <span>{t("common.n_codes", { n: tt.codes_count })}</span>
                  <span>{t("common.n_entretiens", { n: tt.interviews_count })}</span>
                </span>
              </button>
            </li>
          )
        })}
      </ul>
    </aside>
  )
}
