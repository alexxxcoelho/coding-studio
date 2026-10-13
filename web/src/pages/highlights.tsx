// Surlignages (design/macos/Lexique.dc.html, highlights part): the passages
// of the interview marked without a code, each to be coded with an existing
// code or to become a new code. The inspector is the lexical help: keyphrases of
// the selected highlight, keywords of all highlights, near passages.

import { useMemo, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { LexiconInspector } from "@/components/lexicon/inspector"
import { NewCodeDialog, type NewCodeTarget } from "@/components/coder/new-code-dialog"
import { shortId } from "@/components/coder/turn"
import { t } from "@/i18n"
import { StepsCapsule } from "@/pages/coder"
import { EmptyState, PageTitle } from "@/components/bits"
import { SearchIcon } from "@/components/icons"
import { NativeSelect } from "@/components/settings/fields"
import { ToolbarActions } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { codeName, isApplicable } from "@/lib/code-tree"
import { invalidatePassages, useCodes, usePassages, useUnits } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"
import type { Passage } from "@/lib/types"

export function HighlightsPage({ id }: { id: string }) {
  const qc = useQueryClient()
  const passagesQ = usePassages(id)
  const unitsQ = useUnits(id)
  const codesQ = useCodes()
  const [search, setSearch] = useState("")
  const [target, setTarget] = useState<NewCodeTarget | null>(null)
  const [pickedId, setPickedId] = useState<string | null>(null)

  const codes = useMemo(() => codesQ.data?.codes ?? [], [codesQ.data])
  const usable = codes.filter(isApplicable)
  const units = useMemo(() => new Map((unitsQ.data ?? []).map((r) => [r.unit.unit_id, r.unit])), [unitsQ.data])
  const order = useMemo(() => new Map((unitsQ.data ?? []).map((r, i) => [r.unit.unit_id, i])), [unitsQ.data])
  const highlights = useMemo(
    () =>
      (passagesQ.data ?? [])
        .filter((p) => p.kind === "highlight" && !p.stale)
        .sort((a, b) => (order.get(a.unit_id) ?? 0) - (order.get(b.unit_id) ?? 0) || a.start - b.start),
    [passagesQ.data, order],
  )
  const selected = highlights.find((p) => p.passage_id === pickedId) ?? highlights[0] ?? null
  const q = search.trim().toLowerCase()
  const shown = q ? highlights.filter((p) => p.text.toLowerCase().includes(q) || p.unit_id.toLowerCase().includes(q)) : highlights

  const code = useMutation({
    mutationFn: (v: { p: Passage; code: string }) => api.addCoding(v.p.passage_id, v.code),
    onSuccess: () => invalidatePassages(qc, id),
    onError: (e) => reportError(e, t("common.code_non_pose")),
  })
  const remove = useMutation({
    mutationFn: (p: Passage) => api.deletePassage(p.passage_id),
    onSuccess: () => invalidatePassages(qc, id),
    onError: (e) => reportError(e, t("highlights.surlignage_non_supprime")),
  })

  if (passagesQ.isLoading || unitsQ.isLoading) return <Skeleton className="h-64 w-full" />
  return (
    <>
      <PageTitle title={id} sub={t("highlights.codage_surlignage_n_surlignages_sans", { n: highlights.length })} />
      <ToolbarActions>
        <StepsCapsule id={id} current="surlignages" highlights={highlights.length} />
        <label className="search">
          <SearchIcon />
          <input type="search" value={search} placeholder={t("highlights.rechercher_dans_les_surlignages")} aria-label={t("highlights.rechercher_dans_les_surlignages")} onChange={(e) => setSearch(e.target.value)} />
        </label>
      </ToolbarActions>
      <div className="content">
        <section className="tx" aria-label={t("common.surlignages")}>
          <p className="small l2" style={{ margin: "0 12px 8px" }}>
            {t("highlights.un_surlignage_n_est_pas")}
          </p>
          {shown.length === 0 ? (
            <EmptyState
              title={highlights.length === 0 ? t("highlights.aucun_surlignage_sans_code") : t("highlights.aucun_surlignage_ne_correspond")}
              action={
                <Button asChild variant="outline">
                  <a href={interviewHref(id, "coding")}>{t("common.retourner_a_coder")}</a>
                </Button>
              }
            >
              {t("highlights.dans_coder_selectionnez_un_passage")}
            </EmptyState>
          ) : (
            shown.map((p) => {
              const unit = units.get(p.unit_id)
              return (
                <article
                  key={p.passage_id}
                  className={`u resp${selected?.passage_id === p.passage_id ? " sel" : ""}`}
                  style={{ gridTemplateColumns: "64px minmax(0, 1fr)" }}
                  aria-current={selected?.passage_id === p.passage_id ? "true" : undefined}
                  onClick={() => setPickedId(p.passage_id)}
                >
                  <div className="meta">
                    <a className="mono" href={interviewHref(id, "coding", 1, p.unit_id)}>
                      {shortId(p.unit_id)}
                    </a>
                    <span>{unit?.timestamp}</span>
                  </div>
                  <div className="txt">
                    <p>
                      <span className="l2">{p.before}</span>
                      <span className="hl">{p.text}</span>
                      <span className="l2">{p.after}</span>
                    </p>
                    <div className="applied">
                      <NativeSelect
                        aria-label={t("highlights.coder_shortid_avec_un_code", { shortId: shortId(p.unit_id) })}
                        style={{ width: 200 }}
                        value=""
                        onChange={(e) => e.target.value && code.mutate({ p, code: e.target.value })}
                      >
                        <option value="">{t("common.coder_avec")}</option>
                        {usable.map((c) => (
                          <option key={c.code_id} value={c.code_id}>
                            {codeName(c)}
                          </option>
                        ))}
                      </NativeSelect>
                      <Button variant="outline" onClick={() => setTarget({ unitId: p.unit_id, passageId: p.passage_id, range: null, text: p.text })}>
                        {t("highlights.nouveau_code_depuis_le_surlignage")}
                      </Button>
                      <Button variant="ghost" onClick={() => remove.mutate(p)}>
                        {t("common.supprimer")}
                      </Button>
                    </div>
                  </div>
                </article>
              )
            })
          )}
        </section>
        <aside className="insp insp-sticky" style={{ flexBasis: 340 }} aria-label={t("common.lexique")}>
          <LexiconInspector
            passageId={selected?.passage_id ?? null}
            unitLabel={selected ? shortId(selected.unit_id) : null}
            onUseLabel={(label) => selected && setTarget({ unitId: selected.unit_id, passageId: selected.passage_id, range: null, text: selected.text, name: label })}
          />
        </aside>
      </div>
      <div className="keys" aria-label={t("common.raccourcis")} style={{ margin: "0 -1.25rem" }}>
        <span>
          <span className="kbd">H</span> {t("highlights.surligner_sans_coder_dans_coder")}
        </span>
      </div>
      <NewCodeDialog interviewId={id} target={target} codes={codes} onClose={() => setTarget(null)} />
    </>
  )
}
