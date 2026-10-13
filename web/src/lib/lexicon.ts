// Pure logic of the Lexique screen: sorting and selecting in the keyword table,
// the code prefill made from selected terms, the frequency table by attribute
// value. No React here; the components render what these functions return.

import type { KeywordRow, TermFrequencyAnswer, TermFrequencyValue } from "./lexicon-api"
import { formatNumber, getLang, t } from "@/i18n"

export type SortKey = "term" | "passages" | "share" | "keyness" | "interviews"
export type SortDir = "asc" | "desc"

/** Direction a column opens with: words A to Z, numbers from the highest. */
export const DEFAULT_DIR: Record<SortKey, SortDir> = { term: "asc", passages: "desc", share: "desc", keyness: "desc", interviews: "desc" }

const collator = new Intl.Collator("fr", { sensitivity: "base" })

/** The words of a stem group, the commonest first, as shown in the table. */
export function termText(r: KeywordRow): string {
  return (r.forms.length > 0 ? r.forms : [r.stem]).join(", ")
}

/** The commonest surface form of a stem group. */
export function mainForm(r: KeywordRow): string {
  return r.forms[0] ?? r.stem
}

/** Share of the occurrences of the stem in the whole corpus that sit in the highlights. */
export function share(r: KeywordRow): number {
  return r.corpus_occurrences > 0 ? r.n_occurrences / r.corpus_occurrences : 0
}

function value(r: KeywordRow, key: Exclude<SortKey, "term">): number {
  switch (key) {
    case "passages":
      return r.n_passages
    case "share":
      return share(r)
    case "keyness":
      return r.keyness
    case "interviews":
      return r.n_interviews
  }
}

/** A sorted copy. Ties fall back on keyness (highest first), then on the stem. */
export function sortKeywords(rows: KeywordRow[], key: SortKey, dir: SortDir): KeywordRow[] {
  const sign = dir === "asc" ? 1 : -1
  return [...rows].sort((a, b) => {
    const primary = key === "term" ? collator.compare(mainForm(a), mainForm(b)) : value(a, key) - value(b, key)
    if (primary !== 0) return sign * primary
    return b.keyness - a.keyness || collator.compare(a.stem, b.stem)
  })
}

export function toggleStem(selected: ReadonlySet<string>, stem: string): Set<string> {
  const next = new Set(selected)
  if (!next.delete(stem)) next.add(stem)
  return next
}

/** Selects all the shown rows, or clears the selection when they are all selected. */
export function toggleAll(selected: ReadonlySet<string>, shown: KeywordRow[]): Set<string> {
  const everyone = shown.length > 0 && shown.every((r) => selected.has(r.stem))
  return everyone ? new Set() : new Set(shown.map((r) => r.stem))
}

export interface SelectionSummary {
  /** The highlights that carry at least one selected term, each once, in table order. */
  passageIds: string[]
  interviews: string[]
}

export function selectionSummary(rows: KeywordRow[], selected: ReadonlySet<string>): SelectionSummary {
  const passageIds: string[] = []
  const interviews: string[] = []
  const seenP = new Set<string>()
  const seenI = new Set<string>()
  for (const r of rows) {
    if (!selected.has(r.stem)) continue
    for (const p of r.passage_ids) {
      if (seenP.has(p)) continue
      seenP.add(p)
      passageIds.push(p)
    }
    for (const i of r.interviews) {
      if (seenI.has(i)) continue
      seenI.add(i)
      interviews.push(i)
    }
  }
  return { passageIds, interviews: interviews.sort(collator.compare) }
}

export interface CodePrefill extends SelectionSummary {
  /** Proposed name of the code: the commonest form of each selected term. */
  name: string
}

/** What "Créer un code" hands to the new code dialog; null when no term is selected. */
export function codeFromTerms(rows: KeywordRow[], selected: ReadonlySet<string>): CodePrefill | null {
  const picked = rows.filter((r) => selected.has(r.stem))
  if (picked.length === 0) return null
  return { name: picked.map(mainForm).join(", "), ...selectionSummary(rows, selected) }
}

const minus = "−"

/** A score with a decimal comma and a true minus sign. */
export function keynessText(n: number): string {
  const rounded = Math.round(n * 10) / 10
  // French keeps its decimal comma here (fr-CH would print a point).
  const plain = formatNumber(Math.abs(rounded), { minimumFractionDigits: 1, maximumFractionDigits: 1 })
  const s = getLang() === "fr" ? plain.replace(".", ",") : plain
  return rounded < 0 ? `${minus}${s}` : s
}

/** Similarity score from 0 to 1, two decimals. */
export function scoreText(n: number): string {
  return formatNumber(n, { minimumFractionDigits: 2, maximumFractionDigits: 2 })
}

export function valueLabel(v: string | null): string {
  return v === null || v === "" ? t("common.non_renseigne") : v
}

export interface FrequencyRow {
  label: string
  n_interviews: number
  n_words: number
  /** One cell per term of the table; null when the term never occurs for this value. */
  cells: (TermFrequencyValue | null)[]
}

export interface FrequencyTable {
  terms: string[]
  rows: FrequencyRow[]
}

/** The answer of the term frequency route as a table: a row per attribute value. */
export function frequencyTable(answer: TermFrequencyAnswer): FrequencyTable {
  if (answer.mode !== "terms") return { terms: [], rows: [] }
  const terms = answer.terms.map((t) => t.term)
  const byKey = new Map<string, FrequencyRow>()
  answer.terms.forEach((t, i) => {
    for (const v of t.by_value) {
      const key = v.value ?? ""
      let row = byKey.get(key)
      if (!row) {
        row = { label: valueLabel(v.value), n_interviews: v.n_interviews, n_words: v.n_words, cells: terms.map(() => null) }
        byKey.set(key, row)
      }
      row.cells[i] = v
    }
  })
  const rows = [...byKey.entries()]
    .sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : collator.compare(a, b)))
    .map(([, r]) => r)
  return { terms, rows }
}

/** Splits a comma separated list of terms typed by the researcher. */
export function parseTerms(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
}
