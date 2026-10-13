// Lexique: the lexical workbench across the corpus (SPEC 4.4, design/macos/
// Lexique.dc.html). Keyword table of the highlights, near passages, keyword in
// context, term frequency per attribute value. Statistical and deterministic;
// no generative model is used. The highlights themselves are listed, per
// interview, on the Surlignages screen.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { PageTitle } from "@/components/bits"
import { KeywordTable } from "@/components/lexicon/keyword-table"
import { KwicSearch } from "@/components/lexicon/kwic-search"
import { PassageLexicon, SimilarPassages } from "@/components/lexicon/passage-panel"
import { GuideLink, ToolIntro } from "@/components/lexicon/parts"
import { TermFrequency } from "@/components/lexicon/term-frequency"
import { NativeSelect } from "@/components/settings/fields"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { shortId } from "@/components/coder/turn"
import { interviewHref } from "@/lib/router"
import { cn } from "@/lib/utils"

type Tool = "keywords" | "near" | "kwic" | "frequency"

const TOOLS: { key: Tool; label: string }[] = [
  { key: "keywords", get label() { return t("lexique.mots_cles") } },
  { key: "near", get label() { return t("common.passages_proches") } },
  { key: "kwic", get label() { return t("common.recherche_en_contexte") } },
  { key: "frequency", get label() { return t("lexique.frequences_par_attribut") } },
]

export function LexiquePage() {
  const [tool, setTool] = useState<Tool>("keywords")
  return (
    <>
      <PageTitle title={t("common.lexique")} sub={t("lexique.surlignages_et_lexique_statistique_e")} />
      <div className="flex flex-col gap-4" style={{ maxWidth: 1100 }}>
        <div className="r8">
          <div className="seg" role="tablist" aria-label={t("lexique.outils_du_lexique")}>
            {TOOLS.map((t) => (
              <button key={t.key} type="button" role="tab" aria-selected={tool === t.key} className={cn("s", tool === t.key && "on")} onClick={() => setTool(t.key)}>
                {t.label}
              </button>
            ))}
          </div>
          <GuideLink>{t("lexique.comment_lire_ces_tableaux")}</GuideLink>
        </div>
        {tool === "keywords" && <KeywordsTool />}
        {tool === "near" && <NearTool />}
        {tool === "kwic" && <KwicSearch />}
        {tool === "frequency" && <TermFrequency />}
      </div>
    </>
  )
}

function useInterviewIds() {
  return useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
}

function KeywordsTool() {
  const interviews = useInterviewIds()
  const [only, setOnly] = useState("")
  return (
    <section className="flex flex-col gap-2" aria-label={t("lexique.mots_cles_des_surlignages")}>
      <p className="small l2" style={{ margin: 0, maxWidth: "70ch" }}>
        {t("lexique.les_mots_regroupes_par_racine")}
      </p>
      <NativeSelect aria-label={t("lexique.limiter_a_un_entretien")} value={only} onChange={(e) => setOnly(e.target.value)} style={{ width: 220 }}>
        <option value="">{t("common.tous_les_entretiens")}</option>
        {(interviews.data ?? []).map((s) => (
          <option key={s.interview_id} value={s.interview_id}>
            {s.interview_id}
          </option>
        ))}
      </NativeSelect>
      <KeywordTable key={only} filter={only ? { interviewId: only } : undefined} />
    </section>
  )
}

function NearTool() {
  const interviews = useInterviewIds()
  const [id, setId] = useState("")
  const [pickedId, setPickedId] = useState<string | null>(null)
  const current = id || interviews.data?.[0]?.interview_id || ""
  const hl = useQuery({ queryKey: ["highlights", current], queryFn: () => api.highlights(current), enabled: current !== "" })
  const list = (hl.data ?? []).filter((p) => !p.stale)
  const selected = list.find((p) => p.passage_id === pickedId) ?? list[0] ?? null
  return (
    <section className="flex flex-col gap-3" aria-label={t("common.passages_proches")}>
      <p className="small l2" style={{ margin: 0, maxWidth: "70ch" }}>
        {t("lexique.choisissez_un_surlignage_le_lexique")}
      </p>
      <NativeSelect aria-label={t("common.entretien")} value={current} onChange={(e) => {
          setId(e.target.value)
          setPickedId(null)
        }} style={{ width: 220 }}>
        {(interviews.data ?? []).map((s) => (
          <option key={s.interview_id} value={s.interview_id}>
            {s.interview_id}
          </option>
        ))}
      </NativeSelect>
      {hl.isLoading && <Skeleton className="h-24 w-full" />}
      {hl.isSuccess && list.length === 0 && (
        <ToolIntro
          title={t("lexique.aucun_surlignage_dans_cet_entretien")}
          action={
            current ? (
              <a className="small" href={interviewHref(current, "coding")}>
                {t("lexique.ouvrir_coder")}
              </a>
            ) : undefined
          }
        >
          {t("lexique.les_passages_proches_se_calculent")}
        </ToolIntro>
      )}
      {list.length > 0 && (
        <div className="grid gap-4" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))" }}>
          <ul className="m-0 flex flex-col gap-1 p-0" aria-label={t("common.surlignages")}>
            {list.map((p) => (
              <li key={p.passage_id} style={{ listStyle: "none" }}>
                <button
                  type="button"
                  onClick={() => setPickedId(p.passage_id)}
                  aria-pressed={selected?.passage_id === p.passage_id}
                  className={cn("u resp w-full cursor-pointer border-0 bg-transparent text-left", selected?.passage_id === p.passage_id && "sel")}
                  style={{ gridTemplateColumns: "48px minmax(0, 1fr)", font: "inherit", color: "inherit" }}
                >
                  <span className="mono">{shortId(p.unit_id)}</span>
                  <span data-no-i18n>{p.text}</span>
                </button>
              </li>
            ))}
          </ul>
          <div className="flex flex-col gap-3">
            <PassageLexicon passageId={selected?.passage_id ?? null} />
            <div className="hs">{t("common.passages_proches")}{selected ? ` de ${shortId(selected.unit_id)}` : ""}</div>
            <SimilarPassages passageId={selected?.passage_id ?? null} />
          </div>
        </div>
      )}
    </section>
  )
}
