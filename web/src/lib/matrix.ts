// The configurable matrix (guide, "Régler la matrice" and "Lire une cellule"):
// rows and columns chosen among interviews, cases, attributes, themes, codes,
// focused codes (the codes that have sub-codes, or every code when none has)
// and the study's own dimensions; one closed value per cell, a passage
// count and a one-line summary. Pure functions: the screen only renders what
// they return.

import type { AxisSource, CellValue, MatrixCell, MatrixConfig } from "./analysis-api"
import type { ProjectConfig } from "./types"
import { t } from "@/i18n"
import { interviewAttributes } from "./attributes"

export const CELL_VALUE_LABEL: Record<CellValue, string> = {
  get present() { return t("matrix.present") },
  get not_mentioned() { return t("matrix.non_evoque") },
  get contradicted() { return t("matrix.contredit") },
  get uncertain() { return t("matrix.incertain") },
}

export const DEFAULT_MATRIX: MatrixConfig = { rows: { kind: "interviews" }, cols: { kind: "themes" }, filter: null }

export function axisToken(s: AxisSource): string {
  return s.id ? `${s.kind}:${s.id}` : s.kind
}

export function parseAxisToken(token: string): AxisSource | null {
  const [kind, ...rest] = token.split(":")
  const id = rest.join(":")
  switch (kind) {
    case "interviews":
    case "cases":
    case "themes":
    case "codes":
    case "focused_codes":
    case "dimensions":
      return id ? null : { kind }
    case "attribute":
      return id ? { kind, id } : null
    default:
      return null
  }
}

export interface AxisOption {
  token: string
  label: string
}

/** The lists a side of the matrix can show, in the order of the selector. */
export function axisOptions(cfg: ProjectConfig | undefined): AxisOption[] {
  const base: AxisOption[] = [
    { token: "interviews", label: t("common.entretiens") },
    { token: "cases", label: "Cas" },
    { token: "themes", label: t("common.themes") },
    { token: "codes", label: t("common.codes") },
    { token: "focused_codes", label: t("matrix.codes_focalises") },
  ]
  const attributes = interviewAttributes(cfg?.attributes).map((a) => ({ token: `attribute:${a.id}`, label: a.label }))
  const dimensions = (cfg?.matrix_dimensions ?? []).length > 0 ? [{ token: "dimensions", label: t("matrix.dimensions_de_l_etude") }] : []
  return [...base, ...attributes, ...dimensions]
}

/** A message when the setting cannot be computed, otherwise null. */
export function validateMatrix(c: MatrixConfig, cfg: ProjectConfig | undefined): string | null {
  if (axisToken(c.rows) === axisToken(c.cols)) return t("matrix.les_lignes_et_les_colonnes")
  const known = (s: AxisSource): string | null => {
    if (s.kind === "attribute" && !cfg?.attributes.some((a) => a.id === s.id)) return t("matrix.cet_attribut_n_existe_pas")
    if (s.kind === "dimensions" && !(cfg?.matrix_dimensions ?? []).length) return t("matrix.l_etude_ne_definit_aucune")
    return null
  }
  const own = known(c.rows) ?? known(c.cols)
  if (own) return own
  if (c.filter && !cfg?.attributes.some((a) => a.id === c.filter!.attribute)) return t("matrix.le_filtre_porte_sur_un")
  return null
}

export function countText(passages: number, interviews: number): string {
  if (passages === 0) return t("matrix.aucun_passage")
  const p = t("common.n_passages", { n: passages })
  // Passages and interviews are never mixed: seven passages in two interviews are not seven respondents.
  return interviews > 1 ? t("matrix.p_dans_interviews_entretiens", { p, interviews }) : p
}

export function matrixMax(cells: MatrixCell[]): number {
  return cells.reduce((m, c) => Math.max(m, c.n_passages), 0)
}

/** 0 for no passage, then four levels of the largest cell of the matrix. */
export function heatLevel(n: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (n <= 0 || max <= 0) return 0
  return Math.min(4, Math.max(1, Math.ceil((n / max) * 4))) as 1 | 2 | 3 | 4
}

export interface CellView {
  state: "not_coded" | "none" | "unqualified" | "qualified"
  label: string
  countText: string
  summary: string
  heat: 0 | 1 | 2 | 3 | 4
  contradiction: boolean
}

/** What a cell shows. A row that is not coded yet is never "non évoqué". */
export function cellView(cell: MatrixCell | undefined, rowCoded: boolean, max = 0): CellView {
  if (!rowCoded) return { state: "not_coded", label: t("matrix.pas_encore_code"), countText: "", summary: "", heat: 0, contradiction: false }
  const n = cell?.n_passages ?? 0
  const text = countText(n, cell?.n_interviews ?? 0)
  const heat = heatLevel(n, max || n)
  const summary = cell?.summary ?? ""
  if (cell?.value) {
    return { state: "qualified", label: CELL_VALUE_LABEL[cell.value], countText: text, summary, heat, contradiction: cell.value === "contradicted" }
  }
  if (n === 0) return { state: "none", label: t("matrix.aucun_passage"), countText: text, summary, heat: 0, contradiction: false }
  return { state: "unqualified", label: t("matrix.a_qualifier"), countText: text, summary, heat, contradiction: false }
}
