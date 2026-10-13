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
import { dateTime, plural } from "@/lib/format"
import { keys } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { reviewApi, type HarmonisationCause, type HarmonisationItem, type HarmonisationResolution } from "@/lib/review-api"
import { unitHref } from "@/lib/router"

import "./revoir.css"

const GUIDE = "#/guide/revoir-harmoniser"

const CAUSE: Record<HarmonisationCause, string> = {
  merge: "Fusion de codes",
  redefine: "Définition modifiée",
  new_subcode: "Nouveau sous-code",
}

const RESOLUTION: Record<HarmonisationResolution, string> = {
  confirmed: "Codage confirmé",
  corrected: "Codage corrigé",
}

type Filter = "open" | "done" | "all"

const FILTERS: { key: Filter; label: string }[] = [
  { key: "open", label: "À relire" },
  { key: "done", label: "Traitées" },
  { key: "all", label: "Toutes" },
]

const shortUnit = (unitId: string) => `U${unitId.split("-U").pop()}`

export function Harmonise({ id }: { id: string }) {
  const [filter, setFilter] = useState<Filter>("open")
  const list = useQuery({ queryKey: keys.harmonisation(id), queryFn: () => reviewApi.harmonisation(id) })
  if (list.isLoading) return <Skeleton className="h-40 w-full" />
  if (list.isError) return <p role="alert" className="note-err">{list.error.message}</p>
  const w = list.data!
  const shown = w.items.filter((i) => filter === "all" || (filter === "open" ? i.status === "open" : i.status === "done"))

  return (
    <div className="page">
      <div className="rv-col">
        <p className="small l2" style={{ margin: "0 2px" }}>
          Quand deux codes sont fusionnés, qu'une définition change ou qu'un sous-code est créé, des unités déjà codées peuvent ne plus correspondre au nouveau sens. Relisez chacune, puis confirmez le codage ou corrigez-le. Aucun modèle n'intervient. <a href={GUIDE}>Lire cette étape du guide</a>.
        </p>

        {w.items.length === 0 ? (
          <EmptyState title="Rien à harmoniser pour cet entretien">
            La liste se remplit après une fusion, une nouvelle définition ou un nouveau sous-code dans le Codebook : les unités concernées de cet entretien apparaissent ici.
          </EmptyState>
        ) : (
          <>
            <div className="r8">
              <div className="seg" role="group" aria-label="Filtre de la liste">
                {FILTERS.map((f) => (
                  <button key={f.key} type="button" className={`s${filter === f.key ? " on" : ""}`} aria-pressed={filter === f.key} onClick={() => setFilter(f.key)}>
                    {f.label}
                  </button>
                ))}
              </div>
              <span className="small l2" role="status">
                {w.n_open === 0 ? "Toute la liste est traitée." : `${plural(w.n_open, "unité")} à relire`}, {plural(w.n_done, "traitée")}.
              </span>
            </div>
            {shown.length === 0 ? (
              <p className="gf">{filter === "open" ? "Rien à relire." : "Rien à afficher."}</p>
            ) : (
              shown.map((it) => <Item key={it.item_id} id={id} item={it} />)
            )}
          </>
        )}
      </div>
    </div>
  )
}

function Item({ id, item }: { id: string; item: HarmonisationItem }) {
  const qc = useQueryClient()
  const [note, setNote] = useState("")
  const open = item.status === "open"
  const save = useMutation({
    mutationFn: (resolution: HarmonisationResolution) => reviewApi.resolveHarmonisation(id, item.item_id, { resolution, note: note.trim() || undefined }),
    onSuccess: (_r, resolution) => {
      toast.success(`${shortUnit(item.unit_id)} : ${RESOLUTION[resolution].toLowerCase()}`)
      setNote("")
      void invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Unité non traitée"),
  })

  return (
    <article className="grp" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 8 }} aria-label={`Unité ${shortUnit(item.unit_id)}, ${item.code_name}`} data-state={item.status}>
      <div className="r8">
        <a className="mono b" href={unitHref(item.unit_id, 2)} title="Ouvrir dans Coder">
          {shortUnit(item.unit_id)}
        </a>
        <span className="tag">{CAUSE[item.cause]}</span>
        <span className="small l2">{item.code_name}</span>
      </div>
      {item.detail && <p className="small l2" style={{ margin: 0 }}>{item.detail}</p>}
      <p className="rv-quote verbatim" style={{ margin: 0 }}>{item.text}</p>
      <div className="r8">
        <span className="hs">Codage actuel</span>
        {item.codes.length === 0 ? (
          <span className="small l2">aucun code</span>
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
            Note, facultative
          </label>
          <Textarea id={`rv-hn-${item.item_id}`} rows={2} placeholder="Ce que vous avez constaté en relisant. Elle est datée et conservée." value={note} onChange={(e) => setNote(e.target.value)} />
          <div className="r8">
            <Button variant="default" disabled={save.isPending} onClick={() => save.mutate("confirmed")}>
              Confirmer le codage
            </Button>
            <Button variant="outline" asChild>
              <a href={unitHref(item.unit_id, 2)}>Corriger dans Coder</a>
            </Button>
            <Button variant="outline" disabled={save.isPending} onClick={() => save.mutate("corrected")}>
              J'ai corrigé le codage
            </Button>
          </div>
        </>
      ) : (
        <p className="small" style={{ margin: 0 }}>
          {item.resolution ? RESOLUTION[item.resolution] : "Traitée"}
          <span className="l2">
            {item.note ? `. ${item.note}` : ""} · {dateTime(item.resolved_at)}
            {item.resolved_by ? ` · ${item.resolved_by}` : ""}
          </span>
        </p>
      )}
    </article>
  )
}
