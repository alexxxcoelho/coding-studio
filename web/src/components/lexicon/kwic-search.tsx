// Keyword in context (SPEC F-54): every occurrence of a word or phrase in the
// respondent turns of all interviews, with the text on each side. Optional
// search by stem and filter on an interview attribute.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { ToolIntro } from "@/components/lexicon/parts"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { useProjectConfig } from "@/lib/hooks"
import { EMPTY } from "@/lib/lexicon-copy"
import { lexiconApi, type KwicHit } from "@/lib/lexicon-api"
import { interviewHref } from "@/lib/router"
import { cn } from "@/lib/utils"

const PAGE = 100

interface Submitted {
  q: string
  stem: boolean
  attribute: string
  value: string
}

/** One occurrence: where it is, the text before, the word, the text after. */
export function KwicRow({ hit, compact = false }: { hit: KwicHit; compact?: boolean }) {
  const short = hit.unit_id.replace(/^.*-(U\d+)$/, "$1")
  const where = (
    <a className="mono" href={interviewHref(hit.interview_id, "coding", 1, hit.unit_id)} title={t("common.ouvrir_dans_coder")}>
      {hit.interview_id} {short}
    </a>
  )
  const parts = (
    <>
      <span data-part="left" className="l2" style={compact ? undefined : { textAlign: "right", whiteSpace: "nowrap", overflow: "hidden", display: "flex", justifyContent: "flex-end" }}>
        {hit.left}
      </span>
      <mark data-part="match" className="hl" style={{ cursor: "default", fontWeight: 600 }}>
        {hit.match}
      </mark>
      <span data-part="right" className="l2" style={compact ? undefined : { whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
        {hit.right}
      </span>
    </>
  )
  if (compact) {
    return (
      <li className="grp" style={{ listStyle: "none", padding: "6px 8px", fontSize: 12.5, lineHeight: 1.4 }} data-testid="kwic-row" data-no-i18n>
        <div className="small">{where}</div>
        <div>
          {hit.left}
          {hit.left ? " " : ""}
          <mark data-part="match" className="hl" style={{ cursor: "default", fontWeight: 600 }}>
            {hit.match}
          </mark>
          {hit.right ? " " : ""}
          {hit.right}
        </div>
      </li>
    )
  }
  return (
    <li
      data-testid="kwic-row"
      data-no-i18n
      style={{ listStyle: "none", display: "grid", gridTemplateColumns: "120px minmax(0,1fr) auto minmax(0,1fr)", gap: 8, alignItems: "baseline", padding: "4px 6px", fontSize: 13 }}
    >
      {where}
      {parts}
    </li>
  )
}

export function KwicSearch({ compact = false }: { compact?: boolean }) {
  const cfg = useProjectConfig()
  const attributes = cfg.data?.attributes ?? []
  const [q, setQ] = useState("")
  const [stem, setStem] = useState(false)
  const [attribute, setAttribute] = useState("")
  const [value, setValue] = useState("")
  const [sent, setSent] = useState<Submitted | null>(null)
  const [limit, setLimit] = useState(PAGE)

  const def = attributes.find((a) => a.id === attribute)
  const filterReady = attribute !== "" && value.trim() !== ""
  const res = useQuery({
    queryKey: ["lexicon", "kwic", sent, limit],
    queryFn: () => lexiconApi.kwic(sent!.q, { stem: sent!.stem, attribute: sent!.attribute || undefined, value: sent!.value || undefined, limit }),
    enabled: sent !== null,
    placeholderData: (prev) => prev,
  })

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!q.trim()) return
    setLimit(PAGE)
    setSent({ q: q.trim(), stem, attribute: filterReady ? attribute : "", value: filterReady ? value.trim() : "" })
  }

  return (
    <div className="flex flex-col gap-3" data-testid="kwic-search">
      <form onSubmit={submit} className="flex flex-col gap-2" role="search" aria-label={t("common.recherche_en_contexte")}>
        <div className="r8">
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("lexicon.kwic_search.un_mot_ou_une_expression")} aria-label={t("lexicon.kwic_search.mot_ou_expression_a_chercher")} style={{ flex: 1, minWidth: 160 }} />
          <Button type="submit" disabled={!q.trim()}>
            {t("common.chercher")}
          </Button>
        </div>
        <div className="r8">
          <label className="r8 small">
            <input type="checkbox" checked={stem} onChange={(e) => setStem(e.target.checked)} style={{ accentColor: "var(--accent-fill)" }} />
            {t("lexicon.kwic_search.chercher_par_racine")}
          </label>
          {attributes.length > 0 && (
            <>
              <NativeSelect
                aria-label={t("common.filtrer_sur_un_attribut")}
                value={attribute}
                onChange={(e) => {
                  setAttribute(e.target.value)
                  setValue("")
                }}
                style={{ width: 170 }}
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
                  <NativeSelect aria-label={t("common.valeur_de_label", { label: def.label })} value={value} onChange={(e) => setValue(e.target.value)} style={{ width: 150 }}>
                    <option value="">{t("common.valeur_2")}</option>
                    {def.choices.map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                  </NativeSelect>
                ) : (
                  <Input aria-label={t("common.valeur_de_label", { label: def.label })} value={value} onChange={(e) => setValue(e.target.value)} placeholder={t("common.valeur")} style={{ width: 150 }} />
                ))}
            </>
          )}
        </div>
      </form>

      {sent === null && <ToolIntro title={EMPTY.kwic.title}>{EMPTY.kwic.text}</ToolIntro>}
      {sent !== null && res.isLoading && <Skeleton className="h-24 w-full" />}
      {res.isError && (
        <p role="alert" className="note-err">
          {res.error.message}
        </p>
      )}
      {res.data && res.data.total === 0 && <ToolIntro title={EMPTY.kwicNone.title}>{EMPTY.kwicNone.text}</ToolIntro>}
      {res.data && res.data.total > 0 && (
        <>
          <p className="small l2" style={{ margin: 0 }} aria-live="polite">
            {res.data.total} occurrence{res.data.total > 1 ? "s" : ""} dans {res.data.n_interviews} entretien{res.data.n_interviews > 1 ? "s" : ""}
            {res.data.stem ? ` · racines : ${res.data.query_tokens.join(", ")}` : ""}
          </p>
          <ul className={cn("m-0 flex flex-col p-0", compact ? "gap-1.5" : "gap-0")} aria-label={t("lexicon.kwic_search.occurrences")}>
            {res.data.hits.map((h) => (
              <KwicRow key={`${h.unit_id}:${h.start}`} hit={h} compact={compact} />
            ))}
          </ul>
          {res.data.hits.length < res.data.total && (
            <Button type="button" variant="outline" onClick={() => setLimit(limit + PAGE)}>
              {t("lexicon.kwic_search.afficher_plus_length_sur_total", { length: res.data.hits.length, total: res.data.total })}
            </Button>
          )}
        </>
      )}
    </div>
  )
}
