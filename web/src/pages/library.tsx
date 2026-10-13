// Bibliothèque (design/macos/Main.dc.html): the interviews with their four
// steps, filters En cours / Prêts / Tous, rescan, search, and the Fiche of the
// selected interview in the inspector.

import { useMemo, useState } from "react"
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, EmptyState, PageTitle } from "@/components/bits"
import { Fiche } from "@/components/library/fiche"
import { ImportDialog } from "@/components/library/import-dialog"
import { RescanIcon } from "@/components/icons"
import { Capsule, CapsuleItem, ToolbarActions, ToolbarSearch } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { keys } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { codingProgress, matchesInterview, nextAction, pipelineSteps, stageOf, type CodingProgress, type Stage } from "@/lib/pipeline"
import { interviewHref, navigate } from "@/lib/router"
import type { InterviewSummary } from "@/lib/types"
import { cn } from "@/lib/utils"

type Filter = "progress" | "ready" | "all"

function useProgress(rows: InterviewSummary[]): Map<string, CodingProgress> {
  const segmented = rows.filter((r) => r.n_units > 0)
  const passages = useQueries({
    queries: segmented.map((r) => ({ queryKey: keys.passages(r.interview_id), queryFn: () => api.passages(r.interview_id) })),
  })
  const simple = useQueries({
    queries: segmented.map((r) => ({ queryKey: ["simple", r.interview_id], queryFn: () => api.simple(r.interview_id) })),
  })
  const out = new Map<string, CodingProgress>()
  segmented.forEach((r, i) => {
    const p = passages[i]?.data
    const s = simple[i]?.data
    if (p && s) out.set(r.interview_id, codingProgress(p, s))
  })
  return out
}

function basename(path: string | undefined): string {
  if (!path) return ""
  return path.slice(path.lastIndexOf("/") + 1)
}

export function LibraryPage() {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const list = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [filter, setFilter] = useState<Filter>("progress")
  const [search, setSearch] = useState("")
  const [picked, setPicked] = useState<string | null>(null)
  const [importOpen, setImportOpen] = useState(false)

  const rows = useMemo(() => list.data ?? [], [list.data])
  const progress = useProgress(rows)
  const stage = (s: InterviewSummary): Stage => stageOf(s, progress.get(s.interview_id) ?? null)
  const counts = {
    progress: rows.filter((s) => stage(s) !== "ready").length,
    ready: rows.filter((s) => stage(s) === "ready").length,
    all: rows.length,
  }
  const shown = rows.filter((s) => {
    if (!matchesInterview(s, search)) return false
    if (filter === "progress") return stage(s) !== "ready"
    if (filter === "ready") return stage(s) === "ready"
    return true
  })
  const selectedId = picked && shown.some((s) => s.interview_id === picked) ? picked : (shown[0]?.interview_id ?? null)
  const selected = rows.find((s) => s.interview_id === selectedId) ?? null

  const rescan = useMutation({
    mutationFn: () => api.scan(),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["interviews"] })
      qc.invalidateQueries({ queryKey: ["state"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
      toast.success(`Dossier scanné : ${res.interviews.length} entretien${res.interviews.length > 1 ? "s" : ""}`)
    },
    onError: (e) => reportError(e, "Le scan a échoué"),
  })
  const segment = useMutation({
    mutationFn: (id: string) => api.segment(id),
    onSuccess: (seg, id) => {
      qc.invalidateQueries({ queryKey: ["interviews"] })
      qc.invalidateQueries({ queryKey: ["interview", id] })
      qc.invalidateQueries({ queryKey: keys.units(id) })
      qc.invalidateQueries({ queryKey: ["chain"] })
      toast.success(`${id} segmenté : ${seg.units.length} unités`)
    },
    onError: (e) => reportError(e, "Segmentation refusée"),
  })

  const sub = state.data
    ? `${rows.length} entretien${rows.length > 1 ? "s" : ""}${state.data.db_path ? ` · ${basename(state.data.db_path)} enregistré` : ""}`
    : " "
  const ready = counts.ready

  return (
    <>
      <PageTitle title="Bibliothèque" sub={sub} />
      <ToolbarActions>
        <Capsule label="Filtrer les entretiens" role="tablist">
          <CapsuleItem role="tab" aria-selected={filter === "progress"} on={filter === "progress"} count={counts.progress} onClick={() => setFilter("progress")}>
            En cours
          </CapsuleItem>
          <CapsuleItem role="tab" aria-selected={filter === "ready"} on={filter === "ready"} count={counts.ready} onClick={() => setFilter("ready")}>
            Prêts
          </CapsuleItem>
          <CapsuleItem role="tab" aria-selected={filter === "all"} on={filter === "all"} count={counts.all} onClick={() => setFilter("all")}>
            Tous
          </CapsuleItem>
        </Capsule>
        <Capsule>
          <CapsuleItem aria-label="Rescanner le dossier" title="Rescanner le dossier" disabled={rescan.isPending} onClick={() => rescan.mutate()}>
            <RescanIcon />
          </CapsuleItem>
          <span className="vs" />
          <CapsuleItem onClick={() => setImportOpen(true)}>Importer des transcriptions…</CapsuleItem>
        </Capsule>
        {ready > 0 && (
          <Button asChild size="lg">
            <a href="#/analyser" style={{ color: "var(--on-accent)" }}>
              Analyser {ready} entretien{ready > 1 ? "s" : ""}
            </a>
          </Button>
        )}
        <ToolbarSearch value={search} onChange={setSearch} />
      </ToolbarActions>

      <div className="content">
        <section style={{ flex: "999 1 560px", minWidth: 0, padding: "6px 18px 16px", overflowX: "auto" }} aria-label="Entretiens">
          {state.data && state.data.codebook_size === 0 && (
            <Banner
              tone="warn"
              className="mb-3"
              title="Le codebook est vide"
              action={
                <Button size="sm" variant="outline" asChild>
                  <a href="#/codebook">Ouvrir le codebook</a>
                </Button>
              }
            >
              Importez une palette de départ ou créez vos codes depuis les passages.
            </Banner>
          )}
          {list.isLoading ? (
            <div className="flex flex-col gap-2">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          ) : rows.length === 0 ? (
            <EmptyState
              title="Aucun entretien pour l'instant"
              action={<Button onClick={() => setImportOpen(true)}>Importer des transcriptions…</Button>}
            >
              Choisissez le dossier qui contient un sous-dossier par entretien, nommé avec son identifiant, puis lancez le scan.
            </EmptyState>
          ) : shown.length === 0 ? (
            <EmptyState title="Aucun entretien dans ce filtre">Changez de filtre ou de recherche.</EmptyState>
          ) : (
            <table className="tbl">
              <thead>
                <tr>
                  <th>Entretien</th>
                  <th>Unités</th>
                  <th>Avancement</th>
                  <th style={{ textAlign: "right" }}>Prochaine étape</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((s) => {
                  const p = progress.get(s.interview_id) ?? null
                  const next = nextAction(s, p)
                  const on = s.interview_id === selectedId
                  return (
                    <tr
                      key={s.interview_id}
                      className={cn("row-click", on && "sel")}
                      data-state={on ? "selected" : undefined}
                      aria-selected={on}
                      onClick={() => setPicked(s.interview_id)}
                      onDoubleClick={() => s.n_units > 0 && navigate(interviewHref(s.interview_id, "coding"))}
                    >
                      <td>
                        <div className="b mono" style={{ fontSize: 13, fontWeight: 600 }}>
                          {s.interview_id}
                        </div>
                        <div className="small l2">
                          {s.folder_name}
                          {s.n_warnings > 0 && `, ${s.n_warnings} avertissement${s.n_warnings > 1 ? "s" : ""}`}
                        </div>
                      </td>
                      <td>
                        {s.n_units > 0 ? (
                          <>
                            {s.n_units}
                            {p && <div className="small l2">{p.respondent} répondant</div>}
                          </>
                        ) : (
                          <span className="l2">Non segmenté</span>
                        )}
                      </td>
                      <td>
                        <div className="pipe">
                          {pipelineSteps(s, p).map((st) => (
                            <div key={st.key}>
                              <i className={cn("bar", st.optional && "opt")}>{!st.optional && <b style={{ width: `${st.value * 100}%` }} />}</i>
                              <span title={st.label}>{st.label}</span>
                            </div>
                          ))}
                        </div>
                      </td>
                      <td style={{ textAlign: "right" }}>
                        {next.kind === "segment" ? (
                          <Button
                            variant="outline"
                            disabled={segment.isPending}
                            onClick={(e) => {
                              e.stopPropagation()
                              segment.mutate(s.interview_id)
                            }}
                          >
                            {next.label}
                          </Button>
                        ) : (
                          <Button asChild variant="outline">
                            <a href={next.href} onClick={(e) => e.stopPropagation()}>
                              {next.label}
                            </a>
                          </Button>
                        )}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
          {state.data && (
            <p className="small l2" style={{ margin: "12px 10px 0" }}>
              Dossier <span className="mono">{state.data.library_root ?? "non choisi"}</span>
              {rows.length > shown.length && ` · ${rows.length - shown.length} autre${rows.length - shown.length > 1 ? "s" : ""} sous « Tous »`}
            </p>
          )}
        </section>
        {selected && (
          <aside className="insp insp-sticky" style={{ flexBasis: 300 }} aria-label={`Fiche ${selected.interview_id}`}>
            <Fiche key={selected.interview_id} summary={selected} progress={progress.get(selected.interview_id) ?? null} />
          </aside>
        )}
      </div>
      <ImportDialog open={importOpen} onOpenChange={setImportOpen} />
    </>
  )
}
