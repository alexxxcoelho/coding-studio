// Revoir (design/macos/Revoir.dc.html, docs/guide/fr/03_revoir.md): the second
// step of an interview. Two parts in the toolbar: "Revue IA" (prompt, answers,
// runs) and "Décisions" (the arbitration). The review is never a lock.

import { useQuery } from "@tanstack/react-query"

import { Banner, PageTitle } from "@/components/bits"
import { Decisions } from "@/components/revoir/decisions"
import { Harmonise } from "@/components/revoir/harmonise"
import { RevueIa } from "@/components/revoir/revue-ia"
import { ToolbarActions } from "@/components/toolbar"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { reviewProgress } from "@/lib/review"
import { keys } from "@/lib/hooks"
import { reviewApi } from "@/lib/review-api"
import { reviewStateLabel, reviewStatusLabel } from "@/lib/review-model"
import { interviewHref } from "@/lib/router"
import { StepsCapsule } from "@/pages/coder"
import { cn } from "@/lib/utils"

export type RevoirPart = "ai" | "review" | "harmonise"

export function RevoirPage({ id, part }: { id: string; part: RevoirPart }) {
  const phase = useQuery({ queryKey: ["phase", id, 2], queryFn: () => api.phase(id, 2) })
  const prompts = useQuery({ queryKey: ["prompts", id, 2], queryFn: () => api.prompts(id) })
  const runs = useQuery({ queryKey: ["runs", id, 2], queryFn: () => api.runs(id) })
  const review = useQuery({ queryKey: ["review", id], queryFn: () => api.review(id) })
  const state = useQuery({ queryKey: keys.reviewState(id), queryFn: () => reviewApi.reviewState(id) })

  const items = review.data?.items ?? []
  const prog = reviewProgress(items)
  const complete = (runs.data ?? []).some((r) => r.complete)
  // The server computes the state (coverage, candidates, units to harmonise); the local count answers until it arrives.
  const status = state.data ? reviewStateLabel(state.data) : reviewStatusLabel(runs.data ?? [], complete ? { decided: prog.disagreementsDecided, total: prog.disagreements } : null)
  const toHarmonise = state.data?.harmonisation_open ?? 0
  const remaining = items.filter((i) => !i.decision).length

  const loading = phase.isLoading || prompts.isLoading || runs.isLoading || review.isLoading
  const failed = phase.error ?? prompts.error ?? runs.error ?? review.error

  return (
    <>
      <PageTitle title={id} sub={`Revue · ${status}`} />
      <ToolbarActions>
        <StepsCapsule id={id} current="revoir" />
        <nav className="cap" aria-label={t("revoir.volets_de_la_revue")}>
          <a className={cn("ci", part === "harmonise" && "on")} href={interviewHref(id, "harmonise")} aria-current={part === "harmonise" ? "page" : undefined}>
            {t("revoir.harmoniser")}{toHarmonise > 0 && <span className="n">{toHarmonise}</span>}
          </a>
          <a className={cn("ci", part === "ai" && "on")} href={interviewHref(id, "ai")} aria-current={part === "ai" ? "page" : undefined}>
            {t("common.revue_ia")}
          </a>
          <a className={cn("ci", part === "review" && "on")} href={interviewHref(id, "review")} aria-current={part === "review" ? "page" : undefined}>
            {t("revoir.decisions")}{remaining > 0 && <span className="n">{remaining}</span>}
          </a>
        </nav>
      </ToolbarActions>
      {part === "harmonise" ? (
        <Harmonise id={id} />
      ) : loading ? (
        <Skeleton className="h-64 w-full" />
      ) : failed ? (
        <Banner tone="danger" title={t("revoir.la_revue_n_a_pas")}>
          {failed.message}
        </Banner>
      ) : part === "ai" ? (
        <RevueIa id={id} guard={phase.data!.guard} prompts={prompts.data ?? []} runs={runs.data ?? []} />
      ) : (
        <Decisions id={id} review={review.data!} runList={runs.data ?? []} />
      )}
    </>
  )
}
