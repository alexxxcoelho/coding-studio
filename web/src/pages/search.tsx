// Recherche (Analyse): full-text search of the transcripts, on respondent turns
// (the interviewer's on request), by all the words or as an exact phrase, with
// filters on interviews, an interview attribute, the coded state and a code
// with its sub-codes. A way to find what is not coded yet.

import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { CodeFinder } from "@/components/code-finder"
import { EmptyState, PageTitle } from "@/components/bits"
import { ResultCard } from "@/components/search/result-card"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { codeColor } from "@/lib/code-tree"
import { useCodes, useProjectConfig } from "@/lib/hooks"
import type { CodeNode } from "@/lib/types"
import { v2, type SearchMode, type SearchParams } from "@/lib/v2-api"
import { cn } from "@/lib/utils"

const PAGE = 20

interface Form {
  q: string
  mode: SearchMode
  prefix: boolean
  interviewer: boolean
  interviews: string[]
  attribute: string
  value: string
  coded: "" | "true" | "false"
  code: CodeNode | null
  descendants: boolean
}

const FORM0: Form = { q: "", mode: "all_words", prefix: false, interviewer: false, interviews: [], attribute: "", value: "", coded: "", code: null, descendants: true }

/** The request for a form and a page: only the filters that are set. */
export function paramsOf(f: Form, page: number): SearchParams {
  return {
    q: f.q.trim(),
    mode: f.mode,
    prefix: f.prefix,
    include_interviewer: f.interviewer,
    interviews: f.interviews,
    attribute: f.attribute && f.value.trim() ? f.attribute : undefined,
    value: f.attribute && f.value.trim() ? f.value.trim() : undefined,
    coded: f.coded,
    code: f.code?.code_id,
    descendants: f.code ? f.descendants : undefined,
    limit: PAGE,
    offset: page * PAGE,
    context: 120,
  }
}

export function SearchPage() {
  const codesQ = useCodes()
  const cfgQ = useProjectConfig()
  const interviewsQ = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [form, setForm] = useState<Form>(FORM0)
  const [sent, setSent] = useState<Form | null>(null)
  const [page, setPage] = useState(0)
  const codes = useMemo(() => codesQ.data?.codes ?? [], [codesQ.data])
  const attributes = cfgQ.data?.attributes ?? []
  const def = attributes.find((a) => a.id === form.attribute)

  const res = useQuery({
    queryKey: ["search-text", sent, page],
    queryFn: () => v2.searchText(paramsOf(sent!, page)),
    enabled: sent !== null,
    placeholderData: (prev) => prev,
  })
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm((f) => ({ ...f, [k]: v }))
  const submit = (e?: React.FormEvent) => {
    e?.preventDefault()
    if (!form.q.trim()) return
    setPage(0)
    setSent({ ...form })
  }
  // A filter changed after a first search: run it again at once.
  const refine = <K extends keyof Form>(k: K, v: Form[K]) => {
    setForm((f) => ({ ...f, [k]: v }))
    if (sent) {
      setPage(0)
      setSent((s) => (s ? { ...s, [k]: v, q: form.q.trim() || s.q } : s))
    }
  }

  const data = res.data
  const total = data?.total_units ?? 0
  const pages = Math.max(1, Math.ceil(total / PAGE))

  return (
    <>
      <PageTitle title={t("common.recherche")} sub={t("search.dans_le_texte_des_entretiens")} />
      <div className="content">
        <section className="tree" aria-label={t("search.resultats_de_la_recherche")}>
          <form onSubmit={submit} role="search" aria-label={t("search.rechercher_dans_les_entretiens")} className="r8">
            <Input
              value={form.q}
              onChange={(e) => set("q", e.target.value)}
              placeholder={t("search.un_mot_plusieurs_mots_ou")}
              aria-label={t("search.texte_a_chercher")}
              style={{ flex: 1, minWidth: 220, height: 30 }}
              autoFocus
            />
            <Button type="submit" size="lg" disabled={!form.q.trim()}>
              {t("common.chercher")}
            </Button>
          </form>

          {sent === null && (
            <EmptyState title={t("search.cherchez_un_mot_pour_retrouver")}>
              {t("search.la_recherche_ignore_la_casse")}
            </EmptyState>
          )}
          {sent !== null && res.isLoading && <Skeleton className="h-40 w-full" />}
          {res.isError && (
            <p role="alert" className="note-err">
              {res.error.message}
            </p>
          )}
          {data && (
            <>
              <p className="small l2" style={{ margin: "0 2px" }} aria-live="polite" role="status">
                {data.total_hits === 0
                  ? t("search.aucune_occurrence")
                  : t("search.hits_summary", { hits: t("search.n_occurrences", { n: data.total_hits }), units: t("search.n_tours", { n: data.total_units }), interviews: t("common.n_entretiens", { n: data.interviews.filter((i) => i.mentioned).length }) })}
                {data.words.length > 0 && t("search.mots_cherches_join", { join: data.words.join(", ") })}
              </p>
              {data.total_hits === 0 && (
                <EmptyState title={t("search.rien_ne_correspond")}>{t("search.essayez_moins_de_mots_tous")}</EmptyState>
              )}
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }} aria-label={t("search.tours_trouves")}>
                {data.items.map((it) => (
                  <ResultCard key={it.unit_id + ":" + it.matches[0]?.start} item={it} codes={codes} onCoded={() => void res.refetch()} />
                ))}
              </div>
              {total > PAGE && (
                <nav className="r8" aria-label={t("search.pages_de_resultats")} style={{ justifyContent: "center" }}>
                  <Button variant="outline" disabled={page === 0} onClick={() => setPage(page - 1)}>
                    {t("common.precedent")}
                  </Button>
                  <span className="small l2">
                    {t("search.tours_v_a_min_sur", { v: page * PAGE + 1, min: Math.min(total, (page + 1) * PAGE), total })}
                  </span>
                  <Button variant="outline" disabled={page >= pages - 1} onClick={() => setPage(page + 1)}>
                    {t("common.suivant")}
                  </Button>
                </nav>
              )}
            </>
          )}
        </section>

        <aside className="insp insp-sticky" style={{ flexBasis: 300 }} aria-label={t("search.options_et_filtres")}>
          <div className="insp-in">
            <div className="hs">{t("search.mode")}</div>
            <div className="seg" role="radiogroup" aria-label={t("search.mode_de_recherche")} style={{ alignSelf: "stretch", display: "flex" }}>
              {(
                [
                  ["all_words", t("search.tous_les_mots")],
                  ["phrase", t("search.expression_exacte")],
                ] as const
              ).map(([k, label]) => (
                <button key={k} type="button" role="radio" aria-checked={form.mode === k} className={cn("s", form.mode === k && "on")} style={{ flex: 1 }} onClick={() => refine("mode", k)}>
                  {label}
                </button>
              ))}
            </div>
            <label className="r8 small">
              <input type="checkbox" checked={form.prefix} onChange={(e) => refine("prefix", e.target.checked)} style={{ accentColor: "var(--accent-fill)" }} />
              {t("search.debut_de_mot_pour_le")}
            </label>
            <label className="r8 small">
              <input type="checkbox" checked={form.interviewer} onChange={(e) => refine("interviewer", e.target.checked)} style={{ accentColor: "var(--accent-fill)" }} />
              {t("search.inclure_les_tours_de_l")}
            </label>

            <div className="hs">{t("common.codage")}</div>
            <div className="seg" role="radiogroup" aria-label={t("search.etat_de_codage")} style={{ alignSelf: "stretch", display: "flex" }}>
              {(
                [
                  ["", t("common.tous")],
                  ["true", t("search.codes")],
                  ["false", t("search.non_codes")],
                ] as const
              ).map(([k, label]) => (
                <button key={k} type="button" role="radio" aria-checked={form.coded === k} className={cn("s", form.coded === k && "on")} style={{ flex: 1 }} onClick={() => refine("coded", k)}>
                  {label}
                </button>
              ))}
            </div>

            <div className="hs">{t("common.code")}</div>
            {form.code ? (
              <div className="r8">
                <span className="tag">
                  <span className="sw" style={{ background: codeColor(form.code.code_id) }} />
                  {form.code.name}
                  <button type="button" aria-label={t("search.retirer_le_filtre_sur_le")} style={{ border: 0, background: "transparent", padding: 0, cursor: "pointer", color: "var(--label2)" }} onClick={() => refine("code", null)}>
                    ×
                  </button>
                </span>
                <label className="r8 small">
                  <input type="checkbox" checked={form.descendants} onChange={(e) => refine("descendants", e.target.checked)} style={{ accentColor: "var(--accent-fill)" }} />
                  {t("search.avec_les_sous_codes")}
                </label>
              </div>
            ) : (
              <CodeFinder codes={codes} onPick={(c) => refine("code", c)} label={t("search.filtrer_sur_un_code")} placeholder={t("search.filtrer_sur_un_code")} />
            )}

            {attributes.length > 0 && (
              <>
                <div className="hs">{t("search.attribut_de_l_entretien")}</div>
                <div className="r8">
                  <NativeSelect
                    aria-label={t("common.filtrer_sur_un_attribut")}
                    style={{ width: 140 }}
                    value={form.attribute}
                    onChange={(e) => {
                      setForm((f) => ({ ...f, attribute: e.target.value, value: "" }))
                    }}
                  >
                    <option value="">{t("common.tous_les_entretiens")}</option>
                    {attributes.map((a) => (
                      <option key={a.id} value={a.id}>
                        {a.label}
                      </option>
                    ))}
                  </NativeSelect>
                  {def &&
                    (def.kind === "choice" ? (
                      <NativeSelect aria-label={t("common.valeur_de_label", { label: def.label })} style={{ width: 120 }} value={form.value} onChange={(e) => refine("value", e.target.value)}>
                        <option value="">{t("common.valeur_2")}</option>
                        {def.choices.map((c) => (
                          <option key={c} value={c}>
                            {c}
                          </option>
                        ))}
                      </NativeSelect>
                    ) : (
                      <Input aria-label={t("common.valeur_de_label", { label: def.label })} style={{ width: 120 }} value={form.value} placeholder={t("common.valeur")} onChange={(e) => set("value", e.target.value)} onBlur={() => form.value.trim() && refine("value", form.value)} />
                    ))}
                </div>
              </>
            )}

            <div className="hs">{t("common.entretiens")}</div>
            <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 4 }} aria-label={t("common.entretiens")}>
              {(interviewsQ.data ?? []).map((s) => {
                const stat = data?.interviews.find((i) => i.interview_id === s.interview_id)
                const on = form.interviews.length === 0 || form.interviews.includes(s.interview_id)
                return (
                  <li key={s.interview_id}>
                    <label className="r8 small" style={{ justifyContent: "space-between", flexWrap: "nowrap" }}>
                      <span className="r8">
                        <input
                          type="checkbox"
                          checked={on}
                          aria-label={t("search.chercher_dans_interview_id", { interview_id: s.interview_id })}
                          onChange={(e) => {
                            const all = (interviewsQ.data ?? []).map((x) => x.interview_id)
                            const current = form.interviews.length === 0 ? all : form.interviews
                            const next = e.target.checked ? [...current, s.interview_id] : current.filter((x) => x !== s.interview_id)
                            refine("interviews", next.length === all.length ? [] : next)
                          }}
                          style={{ accentColor: "var(--accent-fill)" }}
                        />
                        {s.interview_id}
                      </span>
                      {stat && (
                        <span className="l2" title={t("search.occurrences_tours")}>
                          {t("search.hits_occ_n_tours", { n: stat.units, hits: stat.hits })}
                        </span>
                      )}
                    </label>
                  </li>
                )
              })}
            </ul>
            {sent && (
              <Button
                variant="outline"
                onClick={() => {
                  setForm({ ...FORM0, q: form.q })
                  setPage(0)
                  setSent({ ...FORM0, q: form.q })
                }}
              >
                {t("search.retirer_les_filtres")}
              </Button>
            )}
          </div>
        </aside>
      </div>
    </>
  )
}
