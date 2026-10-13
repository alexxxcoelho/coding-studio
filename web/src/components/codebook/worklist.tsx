// The reassignment worklist of a split: every passage that carried the original
// code, to assign to one or more of the new codes or to leave where it is.
// Decisions are made with the keyboard and saved together (all or nothing).

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { toast } from "sonner"

import { useRefreshAfterCodeOp } from "@/components/codebook/refresh"
import { Alerts } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { codeColor } from "@/lib/code-tree"
import { isMac } from "@/lib/nav"
import { unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import { v2, type Worklist, type WorklistDecision, type WorklistItem, type WorklistStatus } from "@/lib/v2-api"
import { explainCodeOpError, toggleTarget, type Draft } from "@/lib/v2-text"

type Show = "pending" | "done" | "all"

const SHOW: { id: Show; label: string }[] = [
  { id: "pending", get label() { return t("codebook.worklist.a_traiter") } },
  { id: "done", get label() { return t("codebook.worklist.traites") } },
  { id: "all", get label() { return t("common.tous") } },
]

const shortUnit = (id: string) => id.replace(/^.*-(U\d+)$/, "$1")

/** What the server holds for an item, as a draft (undefined while pending). */
export function savedDraft(it: WorklistItem): Draft | undefined {
  if (it.status === "assigned") return { action: "assign", targets: it.targets }
  if (it.status === "left") return { action: "leave", targets: [] }
  return undefined
}

const same = (a: Draft | undefined, b: Draft | undefined) => (a?.action ?? "") === (b?.action ?? "") && (a?.targets ?? []).join("|") === (b?.targets ?? []).join("|")

/** The decisions to send: the items whose draft differs from what the server holds. */
export function changedDecisions(items: WorklistItem[], drafts: Record<string, Draft | undefined>): WorklistDecision[] {
  const out: WorklistDecision[] = []
  for (const it of items) {
    const d = drafts[it.passage.passage_id]
    if (!d || same(d, savedDraft(it))) continue
    out.push(d.action === "leave" ? { passage_id: it.passage.passage_id, action: "leave" } : { passage_id: it.passage.passage_id, action: "assign", targets: d.targets })
  }
  return out
}

export function WorklistDialog({ opId, onClose }: { opId: number | null; onClose: () => void }) {
  return (
    <Dialog open={opId !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-4xl" aria-describedby="worklist-help">
        {opId !== null && <WorklistBody key={opId} opId={opId} onClose={onClose} />}
      </DialogContent>
    </Dialog>
  )
}

function WorklistBody({ opId, onClose }: { opId: number; onClose: () => void }) {
  const refresh = useRefreshAfterCodeOp()
  const q = useQuery({ queryKey: ["split-worklist", opId], queryFn: () => v2.worklist(opId) })
  const [drafts, setDrafts] = useState<Record<string, Draft | undefined>>({})
  const [show, setShow] = useState<Show>("pending")
  const [error, setError] = useState<string | null>(null)
  const [focus, setFocus] = useState(0)
  const refs = useRef<(HTMLDivElement | null)[]>([])
  const wl = q.data

  const items = useMemo(() => wl?.items ?? [], [wl])
  const draftOf = (it: WorklistItem) => (it.passage.passage_id in drafts ? drafts[it.passage.passage_id] : savedDraft(it))
  // The filter follows what the server holds, so that a row does not leave the list under the cursor when it is decided.
  const visible = items.filter((it) => (show === "all" ? true : show === "pending" ? it.status === "pending" : it.status !== "pending"))
  const nSaved = items.filter((it) => it.status !== "pending").length
  const effective: Record<string, Draft | undefined> = {}
  for (const it of items) effective[it.passage.passage_id] = draftOf(it)
  const pending = changedDecisions(items, effective)
  const noLeft = items.filter((it) => !effective[it.passage.passage_id]).length
  const locked = !!wl?.undone

  const save = useMutation({
    mutationFn: () => v2.decideWorklist(opId, pending),
    onSuccess: async (w) => {
      setDrafts({})
      setError(null)
      await refresh()
      toast.success(t("codebook.worklist.n_decisions_enregistrees", { n: pending.length }))
      if (w.pending === 0) toast.info(t("codebook.worklist.la_liste_est_vide_chaque"))
    },
    onError: (e) => setError(explainCodeOpError(e)),
  })

  const setDraft = (id: string, d: Draft | undefined) => setDrafts((m) => ({ ...m, [id]: d }))
  const move = (to: number) => {
    const n = Math.max(0, Math.min(visible.length - 1, to))
    setFocus(n)
    refs.current[n]?.focus()
  }

  // Keep the focused row inside the list when the list shrinks after a decision.
  useEffect(() => {
    if (focus > visible.length - 1) setFocus(Math.max(0, visible.length - 1))
  }, [visible.length, focus])

  if (q.isLoading || !wl) {
    return (
      <>
        <DialogHeader>
          <DialogTitle>{t("codebook.worklist.reaffectation_des_passages")}</DialogTitle>
          <DialogDescription id="worklist-help">{t("codebook.worklist.chargement_de_la_liste")}</DialogDescription>
        </DialogHeader>
        <Skeleton className="h-40 w-full" />
      </>
    )
  }

  const newCodes = wl.new_codes
  const onKey = (e: React.KeyboardEvent, it: WorklistItem, i: number) => {
    if (e.target instanceof HTMLElement && (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA")) return
    if (locked) return
    const id = it.passage.passage_id
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault()
      if (pending.length && !save.isPending) save.mutate()
      return
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return
    if (e.key === "ArrowDown" || e.key === "j") {
      e.preventDefault()
      move(i + 1)
    } else if (e.key === "ArrowUp" || e.key === "k") {
      e.preventDefault()
      move(i - 1)
    } else if (/^[1-9]$/.test(e.key) && Number(e.key) <= newCodes.length) {
      e.preventDefault()
      setDraft(id, toggleTarget(draftOf(it), newCodes[Number(e.key) - 1].code_id))
    } else if (e.key === "l" || e.key === "L") {
      e.preventDefault()
      setDraft(id, { action: "leave", targets: [] })
    } else if (e.key === "Backspace" || e.key === "Delete") {
      e.preventDefault()
      setDraft(id, undefined)
    }
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("codebook.worklist.reaffecter_les_passages_de_name", { name: wl.original.name })}</DialogTitle>
        <DialogDescription id="worklist-help">
          {wl.rationale ? `Motif : ${wl.rationale}. ` : ""}{t("codebook.worklist.pour_chaque_passage_posez_un", { name: wl.original.name })}
        </DialogDescription>
      </DialogHeader>

      <div className="r8" style={{ justifyContent: "space-between" }}>
        <div className="seg" role="tablist" aria-label={t("codebook.worklist.passages_de_la_liste")}>
          {SHOW.map((s) => (
            <button key={s.id} type="button" role="tab" aria-selected={show === s.id} className={cn("s", show === s.id && "on")} onClick={() => setShow(s.id)}>
              {s.label}
              <span className="n">{s.id === "pending" ? items.length - nSaved : s.id === "done" ? nSaved : items.length}</span>
            </button>
          ))}
        </div>
        <span className="small l2" aria-live="polite">
          {t("codebook.worklist.progress", { assigned: t("codebook.worklist.n_reaffectes", { n: wl.assigned }), left: t("codebook.worklist.n_laisses", { n: wl.left }), pending: wl.pending, total: wl.total })}
          {wl.gone > 0 ? t("codebook.worklist.n_supprimes_depuis", { n: wl.gone }) : ""}
        </span>
      </div>

      <div className="r8">
        <span className="hs">{t("codebook.worklist.touches_des_nouveaux_codes")}</span>
        {newCodes.map((c, i) => (
          <span key={c.code_id} className="tag k" style={{ "--k": codeColor(c.code_id) } as CSSProperties}>
            <span className="kbd">{i + 1}</span>
            <span className="sw" />
            {c.name}
          </span>
        ))}
      </div>

      {locked && <p className="note-warn">{t("codebook.worklist.cette_scission_a_ete_annulee")}</p>}

      <div role="list" aria-label={t("codebook.worklist.passages_a_reaffecter")} style={{ maxHeight: 360, overflow: "auto", display: "flex", flexDirection: "column", gap: 6 }}>
        {visible.length === 0 && (
          <p className="small l2" style={{ margin: "8px 2px" }}>
            {show === "pending" ? t("codebook.worklist.aucun_passage_a_traiter") : t("codebook.worklist.aucun_passage_dans_ce_filtre")}
          </p>
        )}
        {visible.map((it, i) => {
          const d = draftOf(it)
          const id = it.passage.passage_id
          const dirty = !same(d, savedDraft(it))
          const status: WorklistStatus = d ? (d.action === "leave" ? "left" : "assigned") : "pending"
          return (
            <div
              key={id}
              ref={(el) => {
                refs.current[i] = el
              }}
              role="listitem"
              tabIndex={i === focus ? 0 : -1}
              aria-label={`${it.passage.interview_id} ${shortUnit(it.passage.unit_id)}, ${status === "pending" ? t("common.a_traiter") : status === "left" ? t("codebook.worklist.laisse") : t("codebook.worklist.reaffecte")}`}
              className="grp"
              data-passage={id}
              style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 6, outlineOffset: -2 }}
              onFocus={() => setFocus(i)}
              onKeyDown={(e) => onKey(e, it, i)}
            >
              <div className="r8">
                <a className="mono b" href={unitHref(it.passage.unit_id)} title={t("common.ouvrir_dans_coder")} onClick={onClose}>
                  {it.passage.interview_id} {shortUnit(it.passage.unit_id)}
                </a>
                <span className="badge">{status === "pending" ? t("common.a_traiter") : status === "left" ? t("codebook.worklist.laisse") : t("codebook.worklist.reaffecte")}</span>
                {dirty && <span className="badge">{t("codebook.worklist.non_enregistre")}</span>}
              </div>
              <p className="verbatim" style={{ margin: 0, fontSize: 14 }}>
                {it.passage.text}
              </p>
              <div className="r8">
                {newCodes.map((c, k) => {
                  const on = d?.action === "assign" && d.targets.includes(c.code_id)
                  return (
                    <Button key={c.code_id} variant={on ? "default" : "outline"} size="sm" aria-pressed={on} disabled={locked} onClick={() => setDraft(id, toggleTarget(d, c.code_id))}>
                      <span className="kbd" aria-hidden style={on ? { background: "rgba(255,255,255,.25)", color: "inherit" } : undefined}>{k + 1}</span>
                      {c.name}
                    </Button>
                  )
                })}
                <Button variant={d?.action === "leave" ? "default" : "outline"} size="sm" aria-pressed={d?.action === "leave"} disabled={locked} onClick={() => setDraft(id, d?.action === "leave" ? undefined : { action: "leave", targets: [] })}>
                  <span className="kbd" aria-hidden style={d?.action === "leave" ? { background: "rgba(255,255,255,.25)", color: "inherit" } : undefined}>L</span>
                  {t("codebook.worklist.laisser")}
                </Button>
              </div>
            </div>
          )
        })}
      </div>

      {error && <Alerts messages={[error]} />}
      <div className="r8" style={{ justifyContent: "space-between" }}>
        <span className="small l2">
          <span className="kbd">↑ ↓</span> {t("codebook.worklist.passage")} <span className="kbd">{t("codebook.worklist.one_to_n", { n: Math.max(1, newCodes.length) })}</span> {t("codebook.worklist.poser_ou_retirer_un_code")} <span className="kbd">L</span> {t("codebook.worklist.laisser_2")} <span className="kbd">{t("codebook.worklist.suppr")}</span> {t("codebook.worklist.effacer")}{" "}
          <span className="kbd">{isMac() ? "⌘" : "Ctrl+"}{t("codebook.worklist.entree")}</span> {t("codebook.worklist.enregistrer")}
        </span>
        <span className="r8">
          <Button
            variant="outline"
            disabled={locked || noLeft === 0}
            onClick={() => setDrafts((m) => ({ ...m, ...Object.fromEntries(items.filter((it) => !effective[it.passage.passage_id]).map((it) => [it.passage.passage_id, { action: "leave", targets: [] } as Draft])) }))}
          >
            {t("codebook.worklist.laisser_tous_les_restants")}
          </Button>
          <Button disabled={locked || pending.length === 0 || save.isPending} onClick={() => save.mutate()}>
            {t("common.enregistrer")} {pending.length > 0 ? t("codebook.worklist.n_decisions", { n: pending.length }) : t("codebook.worklist.les_decisions")}
          </Button>
        </span>
      </div>
    </>
  )
}

export type { Worklist }
