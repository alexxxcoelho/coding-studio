// Client of the lexical workbench (docs/API.md, "Lexical workbench (lot 3)").
// Deterministic and local: tokenisation, Snowball stems, YAKE, TF-IDF, KWIC.
// No generative model is called by any of these routes.

import { request } from "./api"

export type LexLanguage = "fr" | "en" | "de"
export type Speakers = "respondent" | "interviewer" | "all"

/** Filter on interviews: one interview, an attribute value, or both (intersection). */
export interface LexFilter {
  interviewId?: string
  attribute?: string
  value?: string
}

export interface LexiconConfig {
  language: LexLanguage
  interview_languages: Record<string, LexLanguage>
}

export interface Keyphrase {
  phrase: string
  /** YAKE score: lower is better. */
  score: number
  count: number
}

export interface StemGroup {
  stem: string
  forms: string[]
  count: number
}

export interface PassageLexical {
  language: LexLanguage
  keyphrases: Keyphrase[]
  stems: StemGroup[]
  /** Provisional name the researcher may use or ignore. */
  label: string
  provisional: true
  note: string
}

export interface SharedTerm {
  stem: string
  form: string
  weight: number
}

export interface CodeRef {
  code_id: string
  name: string
}

export interface SimilarItem {
  interview_id: string
  unit_id: string
  seq: number
  score: number
  shared_terms: SharedTerm[]
  text: string
  coded_with: CodeRef[]
}

export interface SimilarAnswer {
  passage_id: string
  items: SimilarItem[]
  note: string
}

export interface KeywordRow {
  stem: string
  forms: string[]
  n_passages: number
  n_interviews: number
  interviews: string[]
  passage_ids: string[]
  /** Occurrences in the passages of the table. */
  n_occurrences: number
  /** Occurrences in the respondent text of the whole corpus. */
  corpus_occurrences: number
  /** Log-likelihood (G2) against the rest of the respondent text; negative when rarer. */
  keyness: number
}

export interface KeywordsAnswer {
  scope: "highlights" | "passages"
  n_passages: number
  n_terms_in_passages: number
  n_rows: number
  rows: KeywordRow[]
  note: string
}

export interface KwicHit {
  interview_id: string
  unit_id: string
  seq: number
  role: "respondent" | "interviewer"
  /** Unicode scalar positions in the unit text, end excluded. */
  start: number
  end: number
  left: string
  match: string
  right: string
}

export interface KwicAnswer {
  query: string
  stem: boolean
  query_tokens: string[]
  total: number
  n_interviews: number
  by_interview: { interview_id: string; n: number }[]
  hits: KwicHit[]
  offset: number
}

export interface TermFrequencyValue {
  /** Null for the interviews where the attribute is empty. */
  value: string | null
  n_interviews: number
  n_interviews_with_term: number
  n_units_with_term: number
  n_occurrences: number
  n_words: number
  per_1000_words: number
}

export interface TermFrequencyRow {
  term: string
  by_value: TermFrequencyValue[]
}

export interface TopTerm {
  stem: string
  form: string
  n_occurrences: number
  n_interviews: number
}

export type TermFrequencyAnswer =
  | { attribute: string; mode: "terms"; stem: boolean; terms: TermFrequencyRow[]; note: string }
  | { attribute: string; mode: "top_terms"; values: { value: string | null; terms: TopTerm[] }[] }

export interface LexiconStatus {
  config: LexiconConfig
  languages: LexLanguage[]
  index: { n_interviews: number; n_units: number; n_respondent_units: number; n_words: number; n_stems: number }
  fingerprint: string
  note: string
}

const enc = encodeURIComponent

function query(params: Record<string, string | number | boolean | undefined | null>): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "" || v === false) continue
    q.set(k, String(v))
  }
  const s = q.toString()
  return s ? `?${s}` : ""
}

function filterParams(f: LexFilter | undefined) {
  return { interview_id: f?.interviewId, attribute: f?.attribute, value: f?.value }
}

export const lexiconApi = {
  config: () => request<LexiconConfig>("GET", "/api/lexicon/config"),
  status: () => request<LexiconStatus>("GET", "/api/lexicon/status"),

  /** Keyphrases, stems and provisional label of a passage. */
  passage: (pid: string) => request<PassageLexical>("GET", `/api/passages/${enc(pid)}/lexical`),

  /** Units close to a passage, with the codes they already carry. */
  similar: (pid: string, opts: LexFilter & { k?: number; minScore?: number; withoutCode?: string } = {}) =>
    request<SimilarAnswer>(
      "GET",
      `/api/passages/${enc(pid)}/similar${query({ k: opts.k, min_score: opts.minScore, without_code: opts.withoutCode, ...filterParams(opts) })}`,
    ),

  /** Keyword table across the highlights (default) or all the passages. */
  keywords: (opts: LexFilter & { scope?: "highlights" | "passages"; minPassages?: number; limit?: number } = {}) =>
    request<KeywordsAnswer>(
      "GET",
      `/api/lexicon/keywords${query({ scope: opts.scope, min_passages: opts.minPassages, limit: opts.limit, ...filterParams(opts) })}`,
    ),

  /** Keyword in context. The interviewer turns are left out unless `speakers` says otherwise. */
  kwic: (q: string, opts: LexFilter & { stem?: boolean; width?: number; speakers?: Speakers; limit?: number; offset?: number } = {}) =>
    request<KwicAnswer>(
      "GET",
      `/api/lexicon/kwic${query({ q, stem: opts.stem, width: opts.width, speakers: opts.speakers, limit: opts.limit, offset: opts.offset, ...filterParams(opts) })}`,
    ),

  /** Frequency of terms per value of an attribute; without `terms`, the most frequent terms per value. */
  termFrequency: (attribute: string, opts: { terms?: string[]; stem?: boolean; speakers?: Speakers; top?: number } = {}) =>
    request<TermFrequencyAnswer>(
      "GET",
      `/api/lexicon/term-frequency${query({ attribute, terms: opts.terms?.join(","), stem: opts.stem, speakers: opts.speakers, top: opts.top })}`,
    ),
}
