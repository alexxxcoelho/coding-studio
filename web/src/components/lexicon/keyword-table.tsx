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
import { api } from "@/lib/api"
import { EMPTY } from "@/lib/lexicon-copy"
import { lexiconApi, type KeywordRow, type LexFilter } from "@/lib/lexicon-api"
import { codeFromTerms, DEFAULT_DIR, keynessText, mainForm, sortKeywords, termText, toggleAll, toggleStem, type SortDir, type SortKey } from "@/lib/lexicon"
import { codeTargetFromTerms } from "@/lib/lexicon-target"
import { keys, useCodes } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"

const HEADERS: { key: SortKey; label: string; title: string; align?: "right" }[] = [
  { key: "term", label: "Mot-clé", title: "Forme la plus fréquente du groupe de racine" },
  { key: "passages", label: "Surlignages", title: "Nombre de surlignages qui contiennent le mot", align: "right" },
  { key: "share", label: "Dans le corpus", title: "Occurrences dans les surlignages sur occurrences dans tout le texte des enquêtés", align: "right" },
  { key: "keyness", label: "Score", title: "Log-vraisemblance (G2) du mot dans les surlignages contre le reste du texte des enquêtés. Négatif quand le mot y est plus rare.", align: "right" },
  { key: "interviews", label: "Entretiens", title: "Nombre d'entretiens dont un surlignage contient le mot", align: "right" },
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
        toast.error("Les surlignages de cette sélection ont changé. Le tableau est recalculé.")
        void q.refetch()
        return
      }
      setCreate(r)
    } catch (e) {
      reportError(e, "Surlignages non chargés")
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
              <input type="checkbox" checked={allOn} onChange={() => setSelected(toggleAll(selected, rows))} aria-label="Tout sélectionner" style={{ accentColor: "var(--accent-fill)" }} />
            </th>
            {headers.map((h) => (
              <th key={h.key} aria-sort={sort.key === h.key ? (sort.dir === "asc" ? "ascending" : "descending") : "none"} style={{ textAlign: h.align }} title={h.title}>
                <button type="button" onClick={() => sortBy(h.key)} className="cursor-pointer border-0 bg-transparent p-0 font-[inherit] text-[inherit]" style={{ color: "inherit" }}>
                  {h.label}
                  {sort.key === h.key ? (sort.dir === "asc" ? " ↑" : " ↓") : ""}
                </button>
              </th>
            ))}
            <th>Lesquels</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <KeywordRowView key={r.stem} row={r} checked={selected.has(r.stem)} compact={compact} onToggle={() => setSelected(toggleStem(selected, r.stem))} />
          ))}
        </tbody>
      </table>
      <p className="small l2" style={{ margin: 0 }}>
        {q.data!.n_passages} surlignage{q.data!.n_passages > 1 ? "s" : ""}, {q.data!.n_rows} mot{q.data!.n_rows > 1 ? "s" : ""}
        {q.data!.n_rows > rows.length ? ` (les ${rows.length} premiers sont affichés)` : ""}. Un comptage de mots, jamais une mesure d'importance.
      </p>
      <Button type="button" variant="default" disabled={!prefill || busy} onClick={() => void openCreate()}>
        {prefill ? `Créer un code · ${prefill.name}, ${prefill.passageIds.length} surlignage${prefill.passageIds.length > 1 ? "s" : ""}` : "Créer un code à partir des mots cochés"}
      </Button>
      <NewCodeDialog interviewId={create?.interviewId ?? ""} target={create?.target ?? null} codes={codesQ.data?.codes ?? []} onClose={() => setCreate(null)} />
    </div>
  )
}

function KeywordRowView({ row, checked, compact, onToggle }: { row: KeywordRow; checked: boolean; compact: boolean; onToggle: () => void }) {
  return (
    <tr data-stem={row.stem}>
      <td className="c">
        <input type="checkbox" checked={checked} onChange={onToggle} aria-label={mainForm(row)} style={{ accentColor: "var(--accent-fill)" }} />
      </td>
      <td title={`Racine : ${row.stem}`}>{termText(row)}</td>
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
