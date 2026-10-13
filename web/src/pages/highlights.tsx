// Surlignages (design/macos/Lexique.dc.html, highlights part): the passages
// of the interview marked without a code, each to be coded with an existing
// code or to become a new code. The inspector is the lexical help: keyphrases of
// the selected highlight, keywords of all highlights, near passages.

import { useMemo, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { LexiconInspector } from "@/components/lexicon/inspector"
import { NewCodeDialog, type NewCodeTarget } from "@/components/coder/new-code-dialog"
import { shortId } from "@/components/coder/turn"
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
    onError: (e) => reportError(e, "Code non posé"),
  })
  const remove = useMutation({
    mutationFn: (p: Passage) => api.deletePassage(p.passage_id),
    onSuccess: () => invalidatePassages(qc, id),
    onError: (e) => reportError(e, "Surlignage non supprimé"),
  })

  if (passagesQ.isLoading || unitsQ.isLoading) return <Skeleton className="h-64 w-full" />
  return (
    <>
      <PageTitle title={id} sub={`Codage · surlignage · ${highlights.length} surlignage${highlights.length > 1 ? "s" : ""} sans code`} />
      <ToolbarActions>
        <StepsCapsule id={id} current="surlignages" highlights={highlights.length} />
        <label className="search">
          <SearchIcon />
          <input type="search" value={search} placeholder="Rechercher dans les surlignages" aria-label="Rechercher dans les surlignages" onChange={(e) => setSearch(e.target.value)} />
        </label>
      </ToolbarActions>
      <div className="content">
        <section className="tx" aria-label="Surlignages">
          <p className="small l2" style={{ margin: "0 12px 8px" }}>
            Un surlignage n'est pas un code : il reste visible et sert de citation candidate. Codez-le avec un code existant, ou faites-en un nouveau code dont il devient l'origine et le premier exemple.
          </p>
          {shown.length === 0 ? (
            <EmptyState
              title={highlights.length === 0 ? "Aucun surlignage sans code" : "Aucun surlignage ne correspond"}
              action={
                <Button asChild variant="outline">
                  <a href={interviewHref(id, "coding")}>Retourner à Coder</a>
                </Button>
              }
            >
              Dans Coder, sélectionnez un passage à la souris puis appuyez sur H, ou cliquez sur Surligner.
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
                      <span className="l3">{p.before}</span>
                      <span className="hl">{p.text}</span>
                      <span className="l3">{p.after}</span>
                    </p>
                    <div className="applied">
                      <NativeSelect
                        aria-label={`Coder ${shortId(p.unit_id)} avec un code existant`}
                        style={{ width: 200 }}
                        value=""
                        onChange={(e) => e.target.value && code.mutate({ p, code: e.target.value })}
                      >
                        <option value="">Coder avec…</option>
                        {usable.map((c) => (
                          <option key={c.code_id} value={c.code_id}>
                            {codeName(c)}
                          </option>
                        ))}
                      </NativeSelect>
                      <Button variant="outline" onClick={() => setTarget({ unitId: p.unit_id, passageId: p.passage_id, range: null, text: p.text })}>
                        Nouveau code depuis le surlignage…
                      </Button>
                      <Button variant="ghost" onClick={() => remove.mutate(p)}>
                        Supprimer
                      </Button>
                    </div>
                  </div>
                </article>
              )
            })
          )}
        </section>
        <aside className="insp insp-sticky" style={{ flexBasis: 340 }} aria-label="Lexique">
          <LexiconInspector
            passageId={selected?.passage_id ?? null}
            unitLabel={selected ? shortId(selected.unit_id) : null}
            onUseLabel={(label) => selected && setTarget({ unitId: selected.unit_id, passageId: selected.passage_id, range: null, text: selected.text, name: label })}
          />
        </aside>
      </div>
      <div className="keys" aria-label="Raccourcis" style={{ margin: "0 -1.25rem" }}>
        <span>
          <span className="kbd">H</span> surligner sans coder (dans Coder)
        </span>
      </div>
      <NewCodeDialog interviewId={id} target={target} codes={codes} onClose={() => setTarget(null)} />
    </>
  )
}
