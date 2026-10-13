// Term frequency per value of an interview attribute: a small table, never a
// word cloud. With words, their occurrences per 1000 words in each group of
// interviews; without, the most frequent words of each group.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { ToolIntro } from "@/components/lexicon/parts"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { formatNumber, t } from "@/i18n"
import { useProjectConfig } from "@/lib/hooks"
import { EMPTY } from "@/lib/lexicon-copy"
import { frequencyTable, parseTerms, valueLabel } from "@/lib/lexicon"
import { lexiconApi } from "@/lib/lexicon-api"

interface Submitted {
  attribute: string
  terms: string[]
  stem: boolean
}

const num = (n: number) => formatNumber(n)
const per1000 = (n: number) => formatNumber(n, { minimumFractionDigits: 1, maximumFractionDigits: 1 })

export function TermFrequency() {
  const cfg = useProjectConfig()
  const attributes = cfg.data?.attributes ?? []
  const [attribute, setAttribute] = useState("")
  const [raw, setRaw] = useState("")
  const [stem, setStem] = useState(true)
  const [sent, setSent] = useState<Submitted | null>(null)

  const res = useQuery({
    queryKey: ["lexicon", "frequency", sent],
    queryFn: () => lexiconApi.termFrequency(sent!.attribute, { terms: sent!.terms, stem: sent!.stem }),
    enabled: sent !== null,
  })

  if (cfg.isSuccess && attributes.length === 0) {
    return <ToolIntro title={EMPTY.noAttribute.title}>{EMPTY.noAttribute.text}</ToolIntro>
  }
  const table = res.data ? frequencyTable(res.data) : null
  return (
    <div className="flex flex-col gap-3" data-testid="term-frequency">
      <form
        className="r8"
        aria-label={t("lexicon.term_frequency.frequence_par_attribut")}
        onSubmit={(e) => {
          e.preventDefault()
          if (attribute) setSent({ attribute, terms: parseTerms(raw), stem })
        }}
      >
        <NativeSelect aria-label={t("common.attribut")} value={attribute} onChange={(e) => setAttribute(e.target.value)} style={{ width: 180 }}>
          <option value="">{t("common.attribut_2")}</option>
          {attributes.map((a) => (
            <option key={a.id} value={a.id}>
              {a.label}
            </option>
          ))}
        </NativeSelect>
        <Input value={raw} onChange={(e) => setRaw(e.target.value)} placeholder={t("lexicon.term_frequency.mots_separes_par_des_virgules")} aria-label={t("lexicon.term_frequency.mots_a_compter")} style={{ flex: 1, minWidth: 200 }} />
        <label className="r8 small">
          <input type="checkbox" checked={stem} onChange={(e) => setStem(e.target.checked)} style={{ accentColor: "var(--accent-fill)" }} />
          {t("lexicon.term_frequency.par_racine")}
        </label>
        <Button type="submit" disabled={!attribute}>
          {t("lexicon.term_frequency.compter")}
        </Button>
      </form>

      {sent === null && <ToolIntro title={EMPTY.frequency.title}>{EMPTY.frequency.text}</ToolIntro>}
      {sent !== null && res.isLoading && <Skeleton className="h-24 w-full" />}
      {res.isError && (
        <p role="alert" className="note-err">
          {res.error.message}
        </p>
      )}

      {table && res.data?.mode === "terms" && (
        <>
          <table className="tbl" aria-label={t("lexicon.term_frequency.frequence_des_mots_par_valeur")}>
            <thead>
              <tr>
                <th>{attributes.find((a) => a.id === res.data!.attribute)?.label ?? res.data.attribute}</th>
                <th style={{ textAlign: "right" }}>{t("common.entretiens")}</th>
                <th style={{ textAlign: "right" }}>{t("lexicon.term_frequency.mots")}</th>
                {table.terms.map((t) => (
                  <th key={t} style={{ textAlign: "right" }}>
                    {t}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((r) => (
                <tr key={r.label}>
                  <td>{r.label}</td>
                  <td style={{ textAlign: "right" }}>{r.n_interviews}</td>
                  <td style={{ textAlign: "right" }}>{num(r.n_words)}</td>
                  {r.cells.map((c, i) => (
                    <td key={table.terms[i]} style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }} title={c ? t("lexicon.term_frequency.n_interviews_with_term_entretien", { n_interviews_with_term: c.n_interviews_with_term, n_interviews: c.n_interviews, n_units_with_term: c.n_units_with_term }) : undefined}>
                      {c ? (
                        <>
                          {c.n_occurrences} <span className="l2 small">{t("lexicon.term_frequency.per1000_pour_1000", { per1000: per1000(c.per_1000_words) })}</span>
                        </>
                      ) : (
                        <span className="l3">0</span>
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
          <p className="small l2" style={{ margin: 0 }}>
            {res.data.note || t("lexicon.term_frequency.occurrences_et_occurrences_pour_1000")}
          </p>
        </>
      )}

      {res.data?.mode === "top_terms" && (
        <table className="tbl" aria-label={t("lexicon.term_frequency.mots_les_plus_frequents_par")}>
          <thead>
            <tr>
              <th>{attributes.find((a) => a.id === res.data!.attribute)?.label ?? res.data.attribute}</th>
              <th>{t("lexicon.term_frequency.mots_les_plus_frequents")}</th>
            </tr>
          </thead>
          <tbody>
            {res.data.values.map((v) => (
              <tr key={v.value ?? ""}>
                <td>{valueLabel(v.value)}</td>
                <td>
                  {v.terms.length === 0 ? (
                    <span className="l3">{t("lexicon.term_frequency.aucun_mot")}</span>
                  ) : (
                    v.terms.map((t, i) => (
                      <span key={t.stem} title={`${t.n_interviews} entretien(s)`}>
                        {i > 0 && ", "}
                        {t.form} <span className="l2 small">{t.n_occurrences}</span>
                      </span>
                    ))
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}
