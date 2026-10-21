// Revoir, part "Harmoniser" (docs/guide/fr/03_revoir.md, action 1): after a merge,
// a new definition or a new sub-code, the units already coded that may no longer
// fit are listed here. Each one is reread, then the coding is confirmed or
// corrected. No model is called.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { EmptyState } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { getLang, t } from "@/i18n"
import { dateTime } from "@/lib/format"
import { keys } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { reviewApi, type HarmonisationCause, type HarmonisationItem, type HarmonisationResolution } from "@/lib/review-api"
import { unitHref } from "@/lib/router"

import "./revoir.css"

const GUIDE = "#/guide/revoir-harmoniser"

const CAUSE: Record<HarmonisationCause, string> = {
  get merge() { return t("revoir.harmonise.fusion_de_codes") },
  get redefine() { return t("revoir.harmonise.definition_modifiee") },
  get new_subcode() { return t("revoir.harmonise.nouveau_sous_code") },
}

const RESOLUTION: Record<HarmonisationResolution, string> = {
  get confirmed() { return t("revoir.harmonise.codage_confirme") },
  get corrected() { return t("revoir.harmonise.codage_corrige") },
}

type Filter = "open" | "done" | "all"

const FILTERS: { key: Filter; label: string }[] = [
  { key: "open", get label() { return t("revoir.harmonise.a_relire") } },
  { key: "done", get label() { return t("revoir.harmonise.traitees") } },
  { key: "all", get label() { return t("common.toutes") } },
]

const shortUnit = (unitId: string) => `U${unitId.split("-U").pop()}`

export function Harmonise({ id }: { id: string }) {
  const qc = useQueryClient()
  const [notes, setNotes] = useState<Record<number,string>>({})
  const [filter, setFilter] = useState<Filter>("open")
  const list = useQuery({ queryKey: keys.harmonisation(id), queryFn: () => reviewApi.harmonisation(id) })
  const confirmAll = useMutation({
    mutationFn: () => reviewApi.confirmHarmonisation(id, (list.data?.items ?? []).filter(i=>i.status==="open").map(i=>({item_id:i.item_id,note:notes[i.item_id]?.trim()||undefined}))),
    onSuccess: async result => {
      setNotes({})
      toast.success(t("revoir.harmonise.bulk_done",{n:result.confirmed}))
      await invalidateInterview(qc,id)
    },
    onError: e => {reportError(e,t("revoir.harmonise.unite_non_traitee"));void invalidateInterview(qc,id)},
  })
  if (list.isLoading) return <Skeleton className="h-40 w-full" />
  if (list.isError) return <p role="alert" className="note-err">{list.error.message}</p>
  const w = list.data!
  const shown = w.items.filter((i) => filter === "all" || (filter === "open" ? i.status === "open" : i.status === "done"))

  return (
    <div className="page">
      <div className="rv-col">
        <p className="small l2" style={{ margin: "0 2px" }}>
          {t("revoir.harmonise.quand_deux_codes_sont_fusionnes")} <a href={GUIDE}>{t("revoir.harmonise.lire_cette_etape_du_guide")}</a>.
        </p>

        {w.items.length === 0 ? (
          <EmptyState title={t("revoir.harmonise.rien_a_harmoniser_pour_cet")}>
            {t("revoir.harmonise.la_liste_se_remplit_apres")}
          </EmptyState>
        ) : (
          <>
            <div className="r8">
              <div className="seg" role="group" aria-label={t("revoir.harmonise.filtre_de_la_liste")}>
                {FILTERS.map((f) => (
                  <button key={f.key} type="button" className={`s${filter === f.key ? " on" : ""}`} aria-pressed={filter === f.key} onClick={() => setFilter(f.key)}>
                    {f.label}
                  </button>
                ))}
              </div>
              <Button disabled={w.n_open===0||confirmAll.isPending} title={t("revoir.harmonise.bulk_help")} onClick={()=>confirmAll.mutate()}>{t("revoir.harmonise.confirm_all")} ({w.n_open})</Button>
              <span className="small l2" role="status">
                {w.n_open === 0 ? t("revoir.harmonise.toute_la_liste_est_traitee") : t("revoir.harmonise.items_a_relire", { items: t("common.n_unites", { n: w.n_open }) })}, {t("revoir.harmonise.n_traitees", { n: w.n_done })}.
              </span>
            </div>
            {shown.length === 0 ? (
              <p className="gf">{filter === "open" ? t("revoir.harmonise.rien_a_relire") : t("common.rien_a_afficher")}</p>
            ) : (
              shown.map((it) => <Item key={it.item_id} id={id} item={it} note={notes[it.item_id]??""} onNote={note=>setNotes(old=>({...old,[it.item_id]:note}))} batchPending={confirmAll.isPending} />)
            )}
          </>
        )}
      </div>
    </div>
  )
}

function Item({ id, item, note, onNote, batchPending }: { id: string; item: HarmonisationItem; note:string; onNote:(note:string)=>void; batchPending:boolean }) {
  const qc = useQueryClient()
  const open = item.status === "open"
  const save = useMutation({
    mutationFn: (resolution: HarmonisationResolution) => reviewApi.resolveHarmonisation(id, item.item_id, { resolution, note: note.trim() || undefined }),
    onSuccess: (_r, resolution) => {
      toast.success(`${shortUnit(item.unit_id)} : ${RESOLUTION[resolution].toLowerCase()}`)
      onNote("")
      void invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, t("revoir.harmonise.unite_non_traitee")),
  })

  return (
    <article className="grp" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 8 }} aria-label={t("revoir.harmonise.unite_shortunit_code_name", { shortUnit: shortUnit(item.unit_id), code_name: item.code_name })} data-state={item.status}>
      <div className="r8">
        <a className="mono b" href={unitHref(item.unit_id, 2)} title={t("common.ouvrir_dans_coder")}>
          {shortUnit(item.unit_id)}
        </a>
        <span className="tag">{CAUSE[item.cause]}</span>
        <span className="small l2">{item.code_name}</span>
      </div>
      {item.detail && <p className="small l2" style={{ margin: 0 }}>{item.detail.charAt(0).toLocaleUpperCase(getLang())}{item.detail.slice(1)}</p>}
      <p className="rv-quote verbatim" style={{ margin: 0 }}>{item.text}</p>
      <div className="r8">
        <span className="hs">{t("revoir.harmonise.codage_actuel")}</span>
        {item.codes.length === 0 ? (
          <span className="small l2">{t("common.aucun_code")}</span>
        ) : (
          item.codes.map((c) => (
            <span key={c.code_id} className="tag">
              {c.name}
            </span>
          ))
        )}
      </div>

      {open ? (
        <>
          <label className="hs" htmlFor={`rv-hn-${item.item_id}`}>
            {t("revoir.harmonise.note_facultative")}
          </label>
          <Textarea id={`rv-hn-${item.item_id}`} rows={2} placeholder={t("revoir.harmonise.ce_que_vous_avez_constate")} value={note} disabled={batchPending} onChange={(e) => onNote(e.target.value)} />
          <div className="r8">
            <Button variant="default" disabled={save.isPending||batchPending} onClick={() => save.mutate("confirmed")}>
              {t("revoir.harmonise.confirmer_le_codage")}
            </Button>
            <Button variant="outline" asChild>
              <a href={unitHref(item.unit_id, 2)}>{t("revoir.harmonise.corriger_dans_coder")}</a>
            </Button>
            <Button variant="outline" disabled={save.isPending||batchPending} onClick={() => save.mutate("corrected")}>
              {t("revoir.harmonise.j_ai_corrige_le_codage")}
            </Button>
          </div>
        </>
      ) : (
        <p className="small" style={{ margin: 0 }}>
          {item.resolution ? RESOLUTION[item.resolution] : t("revoir.harmonise.traitee")}
          <span className="l2">
            {item.note ? `. ${item.note}` : ""} · {dateTime(item.resolved_at)}
            {item.resolved_by ? ` · ${item.resolved_by}` : ""}
          </span>
        </p>
      )}
    </article>
  )
}
