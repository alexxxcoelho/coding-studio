import { useQuery } from "@tanstack/react-query"

import { Banner, EmptyState } from "@/components/bits"
import { AiPassTab } from "@/components/interview/ai-pass"
import { AuditTab } from "@/components/interview/audit-export"
import { CasesTab } from "@/components/interview/cases"
import { CodingTab } from "@/components/interview/coding"
import { ReviewTab } from "@/components/interview/review"
import { SourcesTab } from "@/components/interview/sources"
import { VerificationTab } from "@/components/interview/verification"
import { Skeleton } from "@/components/ui/skeleton"
import { api, ApiError } from "@/lib/api"
import { interviewHref, type Tab } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Phase, PhaseSummary } from "@/lib/types"

const TAB_LABELS: { tab: Tab; label: string }[] = [
  { tab: "sources", label: "Sources" },
  { tab: "coding", label: "Coding" },
  { tab: "ai", label: "AI challenge" },
  { tab: "review", label: "Review" },
  { tab: "cases", label: "Cases" },
  { tab: "verification", label: "Verification" },
  { tab: "audit", label: "Audit and export" },
]

const PHASES: { phase: Phase; title: string; sub: string }[] = [
  { phase: 1, title: "Phase 1", sub: "TOE pre-classification" },
  { phase: 2, title: "Phase 2", sub: "Concrete codes" },
]

function phaseNote(s: PhaseSummary, phase: Phase): string {
  if (s.n_units === 0) return phase === 2 ? "no unit pre-classified yet" : "no unit"
  return `${s.n_human_coded} of ${s.n_units} coded${phase === 1 ? ", provisional" : ""}`
}

export function InterviewPage({ id, tab, phase, unit = null }: { id: string; tab: Tab; phase: Phase; unit?: string | null }) {
  const detail = useQuery({ queryKey: ["interview", id], queryFn: () => api.interview(id) })

  if (detail.isLoading) return <Skeleton className="h-40 w-full" />
  if (detail.error) {
    const nf = detail.error instanceof ApiError && detail.error.kind === "not_found"
    return (
      <EmptyState title={nf ? `No interview ${id} in the library` : "The interview could not be loaded"}>
        {nf ? "Scan the interview folder again from the Interviews page." : detail.error.message}
      </EmptyState>
    )
  }
  const d = detail.data!
  const phased = tab === "coding"
  const summaries = { 1: d.phase1, 2: d.phase2 } as const

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <a href="#/interviews" className="text-muted-foreground text-xs hover:underline">
            Interviews
          </a>
          <h1 className="flex items-baseline gap-3 text-xl font-semibold tracking-tight">
            <span className="ident text-xl">{d.entry.interview_id}</span>
            <span className="text-muted-foreground truncate text-sm font-normal">{d.entry.folder_name}</span>
          </h1>
        </div>
        {phased && (
          <div role="tablist" aria-label="Coding phase" className="bg-muted inline-flex rounded-lg p-1">
            {PHASES.map(({ phase: p, title, sub }) => {
              const active = p === phase
              return (
                <a
                  key={p}
                  role="tab"
                  aria-selected={active}
                  href={interviewHref(id, tab, p)}
                  className={cn(
                    "flex flex-col rounded-md px-3.5 py-1.5 text-left leading-tight",
                    active ? "bg-card shadow-sm" : "hover:bg-card/50",
                  )}
                >
                  <span className="text-sm font-medium">
                    {title}, {sub}
                  </span>
                  <span className="text-muted-foreground text-xs">{phaseNote(summaries[p], p)}</span>
                </a>
              )
            })}
          </div>
        )}
      </header>

      <nav aria-label="Interview sections" className="flex gap-1 border-b">
        {TAB_LABELS.map(({ tab: t, label }) => (
          <a
            key={t}
            href={interviewHref(id, t, t === "coding" ? phase : 1)}
            aria-current={t === tab ? "page" : undefined}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm",
              t === tab ? "border-primary text-foreground font-medium" : "text-muted-foreground hover:text-foreground border-transparent",
            )}
          >
            {label}
            {(t === "ai" || t === "review") && <span className="text-muted-foreground ml-1 text-xs font-normal">phase 2</span>}
          </a>
        ))}
      </nav>

      {d.entry.warnings.length > 0 && tab === "sources" && (
        <Banner tone="warn" title={`${d.entry.warnings.length} scan warning${d.entry.warnings.length > 1 ? "s" : ""}`}>
          <ul className="list-inside list-disc text-xs">
            {d.entry.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </Banner>
      )}

      {tab === "sources" && <SourcesTab detail={d} />}
      {tab === "coding" && <CodingTab key={phase} id={id} phase={phase} detail={d} initialUnit={unit} />}
      {tab === "ai" && <AiPassTab id={id} />}
      {tab === "review" && <ReviewTab id={id} />}
      {tab === "cases" && <CasesTab id={id} detail={d} />}
      {tab === "verification" && <VerificationTab id={id} />}
      {tab === "audit" && <AuditTab id={id} />}
    </div>
  )
}
