import { ReviewerExport } from "@/components/reviewer-export"
// Bibliothèque (design/macos/Main.dc.html): the interviews with their four
// steps, filters En cours / Prêts / Tous, rescan, search, and the Fiche of the
// selected interview in the inspector.

import { Download } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, EmptyState, PageTitle } from "@/components/bits"
import { VerificationDialog } from "@/components/export/verification-dialog"
import { Fiche } from "@/components/library/fiche"
import { ImportDialog } from "@/components/library/import-dialog"
import { RescanIcon } from "@/components/icons"
import { Capsule, CapsuleItem, ToolbarActions, ToolbarSearch } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { onMenuAction } from "@/lib/desktop"
import { reviewApi, type ReviewState } from "@/lib/review-api"
import { keys } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { codingProgress, matchesInterview, nextAction, pipelineSteps, stageOf, type CodingProgress, type Stage } from "@/lib/pipeline"
import { interviewHref, navigate } from "@/lib/router"
import type { InterviewSummary } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useReviewCounts } from "@/lib/transcript-review"

type Filter = "progress" | "ready" | "all"

/** Progress of every interview, in one call. */
function useProgress(rows: InterviewSummary[]): Map<string, CodingProgress> {
  const q = useQuery({ queryKey: keys.libraryProgress, queryFn: api.libraryProgress, enabled: rows.some((r) => r.n_units > 0) })
  const out = new Map<string, CodingProgress>()
  for (const r of q.data?.interviews ?? []) {
    if (rows.some((s) => s.interview_id === r.interview_id && s.n_units > 0)) out.set(r.interview_id, codingProgress(r))
  }
  return out
}

/** Review state of every interview, in one call: the AI step of the pipeline and the units left to harmonise. */
function useReviewStates(): Map<string, ReviewState> {
  const q = useQuery({ queryKey: keys.corpusReviewState, queryFn: reviewApi.corpusReviewState })
  return new Map((q.data?.interviews ?? []).map((r) => [r.interview_id, r]))
}

function basename(path: string | undefined): string {
  if (!path) return ""
  return path.slice(path.lastIndexOf("/") + 1)
}

export function LibraryPage() {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const list = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [reviewerOpen, setReviewerOpen] = useState(false)
  const [filter, setFilter] = useState<Filter>("progress")
  const [search, setSearch] = useState("")
  const [picked, setPicked] = useState<string | null>(null)
  const [importOpen, setImportOpen] = useState(false)
  const [verificationOpen, setVerificationOpen] = useState(false)
  // Fichier > Importer des transcriptions... of the desktop menu.
  useEffect(() => onMenuAction("import", () => setImportOpen(true)), [])

  const rows = useMemo(() => list.data ?? [], [list.data])
  const progress = useProgress(rows)
  const reviews = useReviewStates()
  const reviewCounts = useReviewCounts()
  const stage = (s: InterviewSummary): Stage => stageOf(s, progress.get(s.interview_id) ?? null, reviews.get(s.interview_id))
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
      qc.invalidateQueries({ queryKey: keys.libraryProgress })
      qc.invalidateQueries({ queryKey: keys.corpusReviewState })
      toast.success(t("library.dossier_scanne_n_entretiens", { n: res.interviews.length }))
    },
    onError: (e) => reportError(e, t("common.le_scan_a_echoue")),
  })
  const segment = useMutation({
    mutationFn: (id: string) => api.segment(id),
    onSuccess: (seg, id) => {
      qc.invalidateQueries({ queryKey: ["interviews"] })
      qc.invalidateQueries({ queryKey: ["interview", id] })
      qc.invalidateQueries({ queryKey: keys.units(id) })
      qc.invalidateQueries({ queryKey: ["chain"] })
      qc.invalidateQueries({ queryKey: keys.libraryProgress })
      qc.invalidateQueries({ queryKey: keys.corpusReviewState })
      toast.success(t("library.id_segmente_length_unites", { id, length: seg.units.length }))
    },
    onError: (e) => reportError(e, t("common.segmentation_refusee")),
  })

  const sub = state.data
    ? `${t("common.n_entretiens", { n: rows.length })}${state.data.db_path ? t("library.basename_enregistre", { basename: basename(state.data.db_path) }) : ""}`
    : " "
  const ready = counts.ready

  return (
    <>
      <ReviewerExport open={reviewerOpen} onOpenChange={setReviewerOpen} />
      <PageTitle title={t("common.bibliotheque")} sub={sub} />
      <ToolbarActions>
        <Capsule label={t("library.filtrer_les_entretiens")} role="tablist">
          <CapsuleItem role="tab" aria-selected={filter === "progress"} on={filter === "progress"} count={counts.progress} onClick={() => setFilter("progress")}>
            {t("library.en_cours")}
          </CapsuleItem>
          <CapsuleItem role="tab" aria-selected={filter === "ready"} on={filter === "ready"} count={counts.ready} onClick={() => setFilter("ready")}>
            {t("library.prets")}
          </CapsuleItem>
          <CapsuleItem role="tab" aria-selected={filter === "all"} on={filter === "all"} count={counts.all} onClick={() => setFilter("all")}>
            {t("common.tous")}
          </CapsuleItem>
        </Capsule>
        <Capsule>
          <CapsuleItem aria-label={t("library.rescanner_le_dossier")} title={t("library.rescanner_le_dossier")} disabled={rescan.isPending} onClick={() => rescan.mutate()}>
            <RescanIcon />
          </CapsuleItem>
          <span className="vs" />
          <CapsuleItem onClick={() => setImportOpen(true)}>{t("library.importer_des_transcriptions")}</CapsuleItem>
          <span className="vs" />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <CapsuleItem aria-label={t("persons.export_menu")} title={t("persons.export_menu")} aria-haspopup="menu">
                <Download size={15} aria-hidden="true" />
              </CapsuleItem>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setReviewerOpen(true)}>{t("reviewer.title")}</DropdownMenuItem>
              <DropdownMenuItem onSelect={() => setVerificationOpen(true)}>{t("persons.verification_action")}</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </Capsule>
        {ready > 0 && (
          <Button asChild size="lg">
            <a href="#/analyser" style={{ color: "var(--on-accent)" }}>
              {t("library.analyser_n_entretiens", { n: ready })}
            </a>
          </Button>
        )}
        <ToolbarSearch value={search} onChange={setSearch} />
      </ToolbarActions>

      <div className="content">
        <section style={{ flex: "999 1 560px", minWidth: 0, padding: "6px 18px 16px", overflowX: "auto" }} aria-label={t("common.entretiens")}>
          {state.data && state.data.codebook_size === 0 && (
            <Banner
              tone="warn"
              className="mb-3"
              title={t("common.le_codebook_est_vide")}
              action={
                <Button size="sm" variant="outline" asChild>
                  <a href="#/codebook">{t("library.ouvrir_le_codebook")}</a>
                </Button>
              }
            >
              {t("library.importez_une_palette_de_depart")}
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
              title={t("library.aucun_entretien_pour_l_instant")}
              action={<Button onClick={() => setImportOpen(true)}>{t("library.importer_des_transcriptions")}</Button>}
            >
              {t("library.choisissez_le_dossier_qui_contient")}
            </EmptyState>
          ) : shown.length === 0 ? (
            <EmptyState title={t("library.aucun_entretien_dans_ce_filtre")}>{t("library.changez_de_filtre_ou_de")}</EmptyState>
          ) : (
            <table className="tbl">
              <thead>
                <tr>
                  <th>{t("common.entretien")}</th>
                  <th>{t("library.unites")}</th>
                  <th>{t("library.avancement")}</th>
                  <th style={{ textAlign: "right" }}>{t("library.prochaine_etape")}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((s) => {
                  const p = progress.get(s.interview_id) ?? null
                  const review = reviews.get(s.interview_id)
                  const next = nextAction(s, p, review)
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
                          {s.n_warnings > 0 && t("library.n_avertissements", { n: s.n_warnings })}
                        </div>
                      </td>
                      <td>
                        {s.n_units > 0 ? (
                          <>
                            {s.n_units}
                            {p && <div className="small l2">{t("library.respondent_repondant", { respondent: p.respondent })}</div>}
                          </>
                        ) : (
                          <span className="l2">{t("library.non_segmente")}</span>
                        )}
                      </td>
                      <td>
                        <div className="pipe">
                          {pipelineSteps(s, p, review).map((st) => (
                            <div key={st.key}>
                              <i className={cn("bar", st.optional && "opt")}>{!st.optional && <b style={{ width: `${st.value * 100}%` }} />}</i>
                              <span title={st.label}>{st.label}</span>
                            </div>
                          ))}
                        </div>
                        {(reviewCounts.get(s.interview_id) ?? 0) > 0 && (
                          <div className="small" style={{ marginTop: 4 }}>
                            <a href={interviewHref(s.interview_id, "coding")} onClick={(e) => e.stopPropagation()} className="badge" title={t("library.passages_dont_l_emplacement_est")}>
                              {t("library.get_passages_a_revoir", { n: (reviewCounts.get(s.interview_id) ?? 0), get: reviewCounts.get(s.interview_id) })}
                            </a>
                          </div>
                        )}
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
              {t("library.dossier")} <span className="mono">{state.data.library_root ?? t("common.non_choisi")}</span>
              {rows.length > shown.length && t("library.n_autres_sous_tous", { n: rows.length - shown.length })}
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
      {verificationOpen && <VerificationDialog open onClose={() => setVerificationOpen(false)} initialIds={selectedId ? [selectedId] : []} />}
    </>
  )
}
