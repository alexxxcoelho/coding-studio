// Checks of the pseudonymisation list before it is sent (the server checks them
// again: docs/API.md, "V1 lots 3 and 4").

import { t } from "@/i18n"
interface Row {
  kind: string
  original: string
  pseudonym: string
}

const norm = (s: string) => s.trim().toLocaleLowerCase("fr")

/** The first problem of the list in the study's words, or null. A blank row is a problem, not silently dropped. */
export function entriesProblem(rows: Row[]): string | null {
  const names = new Set<string>()
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i]
    if (norm(r.original) === "") return t("pseudonymisation.le_nom_reel_de_la", { v: i + 1 })
    if (norm(r.pseudonym) === "") return t("pseudonymisation.le_pseudonyme_de_trim_est", { trim: r.original.trim() })
    if (norm(r.original) === norm(r.pseudonym)) return t("pseudonymisation.trim_ne_change_pas_le", { trim: r.original.trim() })
    if (names.has(norm(r.original))) return t("pseudonymisation.trim_figure_deux_fois_dans", { trim: r.original.trim() })
    names.add(norm(r.original))
  }
  for (const r of rows) {
    if (names.has(norm(r.pseudonym))) return t("pseudonymisation.le_pseudonyme_trim_est_lui", { trim: r.pseudonym.trim() })
  }
  return null
}

export function sameEntries(a: Row[], b: Row[]): boolean {
  return a.length === b.length && a.every((r, i) => r.kind === b[i].kind && r.original === b[i].original && r.pseudonym === b[i].pseudonym)
}
