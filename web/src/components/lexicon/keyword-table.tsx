// Keyword table across the highlights: one row per stem group, with its
// frequency in the highlights against the corpus, its keyness score and the
// interviews concerned. Terms can be selected to create a code from them.
// A table with counts, never a word cloud (SPEC F-52).

import { useMemo, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { NewCodeDialog, type NewCodeTarget } from "@/components/coder/new-code-dialog"
import { ToolIntro } from "@/components/lexicon/parts"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { EMPTY } from "@/lib/lexicon-copy"
import { lexiconApi, type KeywordRow, type LexFilter } from "@/lib/lexicon-api"
import { codeFromTerms, DEFAULT_DIR, keynessText, mainForm, sortKeywords, termText, toggleAll, toggleStem, type SortDir, type SortKey } from "@/lib/lexicon"
import { codeTargetFromTerms } from "@/lib/lexicon-target"
import { keys, useCodes } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"

const HEADERS: { key: SortKey; label: string; title: string; align?: "right" }[] = [
  { key: "term", get label() { return t("lexicon.keyword_table.mot_cle") }, get title() { return t("lexicon.keyword_table.forme_la_plus_frequente_du") } },
  { key: "passages", get label() { return t("common.surlignages") }, get title() { return t("lexicon.keyword_table.nombre_de_surlignages_qui_contiennen") }, align: "right" },
  { key: "share", get label() { return t("lexicon.keyword_table.dans_le_corpus") }, get title() { return t("lexicon.keyword_table.occurrences_dans_les_surlignages_sur") }, align: "right" },
  { key: "keyness", get label() { return t("lexicon.keyword_table.score") }, get title() { return t("lexicon.keyword_table.log_vraisemblance_g2_du_mot") }, align: "right" },
  { key: "interviews", get label() { return t("common.entretiens") }, get title() { return t("lexicon.keyword_table.nombre_d_entretiens_dont_un") }, align: "right" },
]

export function KeywordTable({ filter, compact = false }: { filter?: LexFilter; compact?: boolean }) {
  const qc = useQueryClient()
  const codesQ = useCodes()
  const q = useQuery({ queryKey: ["lexicon", "keywords", filter ?? null], queryFn: () => lexiconApi.keywords(filter) })
  const [sort, setSort] = useState<{ key: SortKey; dir: SortDir }>({ key: "keyness", dir: "desc" })
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())
  const [create, setCreate] = useState<{ interviewId: string; target: NewCodeTarget | null } | null>(null)
  const [busy, setBusy] = useState(false)

  const rows = useMemo(() => sortKeywords(q.data?.rows ?? [], sort.key, sort.dir), [q.data, sort])
  const prefill = useMemo(() => codeFromTerms(rows, selected), [rows, selected])

  if (q.isLoading) return <Skeleton className="h-40 w-full" />
  if (q.isError) return <p role="alert" className="note-err">{q.error.message}</p>
  if (rows.length === 0) return <ToolIntro title={EMPTY.keywords.title}>{EMPTY.keywords.text}</ToolIntro>

  const sortBy = (key: SortKey) => setSort((s) => (s.key === key ? { key, dir: s.dir === "asc" ? "desc" : "asc" } : { key, dir: DEFAULT_DIR[key] }))
  const allOn = rows.every((r) => selected.has(r.stem))

  const openCreate = async () => {
    if (!prefill) return
    setBusy(true)
    try {
      const r = await codeTargetFromTerms(prefill, (id) => qc.fetchQuery({ queryKey: keys.highlights(id), queryFn: () => api.highlights(id), staleTime: 0 }))
      if (!r) {
        toast.error(t("lexicon.keyword_table.les_surlignages_de_cette_selection"))
        void q.refetch()
        return
      }
      setCreate(r)
    } catch (e) {
      reportError(e, t("lexicon.keyword_table.surlignages_non_charges"))
    } finally {
      setBusy(false)
    }
  }

  const headers = compact ? HEADERS.filter((h) => h.key !== "share" && h.key !== "keyness") : HEADERS
  return (
    <div className="flex flex-col gap-2" data-testid="keyword-table">
      <table className="kw tbl" style={{ fontSize: compact ? 12.5 : undefined }}>
        <thead>
          <tr>
            <th style={{ width: 22 }}>
              <input type="checkbox" checked={allOn} onChange={() => setSelected(toggleAll(selected, rows))} aria-label={t("lexicon.keyword_table.tout_selectionner")} style={{ accentColor: "var(--accent-fill)" }} />
            </th>
            {headers.map((h) => (
              <th key={h.key} aria-sort={sort.key === h.key ? (sort.dir === "asc" ? "ascending" : "descending") : "none"} style={{ textAlign: h.align }} title={h.title}>
                <button type="button" onClick={() => sortBy(h.key)} className="cursor-pointer border-0 bg-transparent p-0 font-[inherit] text-[inherit]" style={{ color: "inherit" }}>
                  {h.label}
                  {sort.key === h.key ? (sort.dir === "asc" ? " ↑" : " ↓") : ""}
                </button>
              </th>
            ))}
            <th>{t("lexicon.keyword_table.lesquels")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <KeywordRowView key={r.stem} row={r} checked={selected.has(r.stem)} compact={compact} onToggle={() => setSelected(toggleStem(selected, r.stem))} />
          ))}
        </tbody>
      </table>
      <p className="small l2" style={{ margin: 0 }}>
        {t("lexicon.keyword_table.n_surlignages", { n: q.data!.n_passages })}, {t("lexicon.keyword_table.n_mots", { n: q.data!.n_rows })}
        {q.data!.n_rows > rows.length ? t("lexicon.keyword_table.les_length_premiers_sont_affiches", { length: rows.length }) : ""}{t("lexicon.keyword_table.un_comptage_de_mots_jamais")}
      </p>
      <Button type="button" variant="default" disabled={!prefill || busy} onClick={() => void openCreate()}>
        {prefill ? t("lexicon.keyword_table.creer_un_code_name_n", { n: prefill.passageIds.length, name: prefill.name }) : t("lexicon.keyword_table.creer_un_code_a_partir")}
      </Button>
      <NewCodeDialog interviewId={create?.interviewId ?? ""} target={create?.target ?? null} codes={codesQ.data?.codes ?? []} onClose={() => setCreate(null)} />
    </div>
  )
}

function KeywordRowView({ row, checked, compact, onToggle }: { row: KeywordRow; checked: boolean; compact: boolean; onToggle: () => void }) {
  return (
    <tr data-stem={row.stem}>
      <td className="c">
        <input type="checkbox" checked={checked} onChange={onToggle} aria-label={mainForm(row)} data-no-i18n style={{ accentColor: "var(--accent-fill)" }} />
      </td>
      <td title={`Racine : ${row.stem}`} data-no-i18n>
        {termText(row)}
      </td>
      <td className="n" style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{row.n_passages}</td>
      {!compact && (
        <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
          {row.n_occurrences} / {row.corpus_occurrences}
        </td>
      )}
      {!compact && <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{keynessText(row.keyness)}</td>}
      <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{row.n_interviews}</td>
      <td className="l2">
        {row.interviews.map((id, i) => (
          <span key={id}>
            {i > 0 && ", "}
            <a href={interviewHref(id, "highlights")}>{id}</a>
          </span>
        ))}
      </td>
    </tr>
  )
}
