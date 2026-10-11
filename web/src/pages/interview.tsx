import { useQuery } from "@tanstack/react-query"

import { Banner, EmptyState } from "@/components/bits"
import { AiPassTab } from "@/components/interview/ai-pass"
import { AuditTab } from "@/components/interview/audit-export"
import { CodingTab } from "@/components/interview/coding"
import { ReconciliationTab } from "@/components/interview/reconciliation"
import { SourcesTab } from "@/components/interview/sources"
import { Skeleton } from "@/components/ui/skeleton"
import { api, ApiError } from "@/lib/api"
import { interviewHref, type Tab } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Phase, PhaseSummary } from "@/lib/types"

const TAB_LABELS: { tab: Tab; label: string; phased: boolean }[] = [
  { tab: "sources", label: "Sources", phased: false },
  { tab: "coding", label: "Coding", phased: true },
  { tab: "ai", label: "AI pass", phased: true },
  { tab: "reconciliation", label: "Reconciliation", phased: true },
  { tab: "audit", label: "Audit and export", phased: false },
]

const PHASES: { phase: Phase; title: string; sub: string }[] = [
  { phase: 1, title: "Phase 1", sub: "TOE contexts" },
  { phase: 2, title: "Phase 2", sub: "Study-specific codes" },
]

function phaseNote(s: PhaseSummary, open: boolean): string {
  if (!open) return "opens once phase 1 is reconciled"
  if (s.reconciled) return "reconciled"
  if (s.n_units === 0) return "no unit in scope"
  return `${s.n_human_coded} of ${s.n_units} coded`
}

export function InterviewPage({ id, tab, phase }: { id: string; tab: Tab; phase: Phase }) {
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
  const phased = TAB_LABELS.find((t) => t.tab === tab)?.phased ?? false
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
              const open = p === 1 || d.phase1.reconciled
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
                    !open && !active && "opacity-60",
                  )}
                >
                  <span className="text-sm font-medium">
                    {title}, {sub}
                  </span>
                  <span className="text-muted-foreground text-xs">{phaseNote(summaries[p], open)}</span>
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
            href={interviewHref(id, t, phase)}
            aria-current={t === tab ? "page" : undefined}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm",
              t === tab ? "border-primary text-foreground font-medium" : "text-muted-foreground hover:text-foreground border-transparent",
            )}
          >
            {label}
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
      {tab === "coding" && <CodingTab id={id} phase={phase} detail={d} />}
      {tab === "ai" && <AiPassTab id={id} phase={phase} />}
      {tab === "reconciliation" && <ReconciliationTab id={id} phase={phase} />}
      {tab === "audit" && <AuditTab id={id} />}
    </div>
  )
}
