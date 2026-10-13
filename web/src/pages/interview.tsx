import { useQuery } from "@tanstack/react-query"

import { Banner, EmptyState, PageTitle } from "@/components/bits"
import { AiPassTab } from "@/components/interview/ai-pass"
import { AuditTab } from "@/components/interview/audit-export"
import { CasesTab } from "@/components/interview/cases"
import { ReviewTab } from "@/components/interview/review"
import { SourcesTab } from "@/components/interview/sources"
import { VerificationTab } from "@/components/interview/verification"
import { Skeleton } from "@/components/ui/skeleton"
import { ToolbarActions } from "@/components/toolbar"
import { CoderPage, StepsCapsule } from "@/pages/coder"
import { HighlightsPage } from "@/pages/highlights"
import { api, ApiError } from "@/lib/api"
import { interviewHref, type Tab } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Phase } from "@/lib/types"

const TAB_LABELS: { tab: Tab; label: string }[] = [
  { tab: "sources", label: "Sources" },
  { tab: "coding", label: "Coder" },
  { tab: "ai", label: "Revue IA" },
  { tab: "review", label: "Décisions" },
  { tab: "cases", label: "Cas" },
  { tab: "verification", label: "Vérification" },
  { tab: "audit", label: "Audit et export" },
]

export function InterviewPage({ id, tab, phase, unit = null }: { id: string; tab: Tab; phase: Phase; unit?: string | null }) {
  // Coder and Surlignages are the screens of the single flow; the other tabs keep the earlier chrome until their lot.
  if (tab === "coding") return <CoderPage id={id} unit={unit} />
  if (tab === "highlights") return <HighlightsPage id={id} />
  return <OtherTabs id={id} tab={tab} phase={phase} />
}

function OtherTabs({ id, tab, phase }: { id: string; tab: Tab; phase: Phase }) {
  const detail = useQuery({ queryKey: ["interview", id], queryFn: () => api.interview(id) })
  const label = TAB_LABELS.find((t) => t.tab === tab)?.label ?? ""

  if (detail.isLoading) return <Skeleton className="h-40 w-full" />
  if (detail.error) {
    const nf = detail.error instanceof ApiError && detail.error.kind === "not_found"
    return (
      <>
        <PageTitle title={id} sub={label} />
        <EmptyState title={nf ? `Aucun entretien ${id} dans la bibliothèque` : "L'entretien n'a pas pu être chargé"}>
          {nf ? "Balayez de nouveau le dossier des entretiens depuis la Bibliothèque." : detail.error.message}
        </EmptyState>
      </>
    )
  }
  const d = detail.data!

  return (
    <div className="flex flex-col gap-4">
      <PageTitle title={d.entry.interview_id} sub={`${label} · ${d.entry.folder_name}`} />
      <ToolbarActions>
        <StepsCapsule id={id} current={tab === "ai" || tab === "review" ? "revoir" : null} />
      </ToolbarActions>

      <nav aria-label="Sections de l'entretien" className="seg self-start">
        {TAB_LABELS.map(({ tab: t, label: l }) => (
          <a key={t} href={interviewHref(id, t, t === "coding" ? phase : 1)} aria-current={t === tab ? "page" : undefined} className={cn("s", t === tab && "on")}>
            {l}
            {(t === "ai" || t === "review") && <span className="n">phase 2</span>}
          </a>
        ))}
      </nav>

      {d.entry.warnings.length > 0 && tab === "sources" && (
        <Banner tone="warn" title={`${d.entry.warnings.length} ${d.entry.warnings.length < 2 ? "avertissement" : "avertissements"} de balayage`}>
          <ul className="list-inside list-disc text-xs">
            {d.entry.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </Banner>
      )}

      {tab === "sources" && <SourcesTab detail={d} />}
      {tab === "ai" && <AiPassTab id={id} />}
      {tab === "review" && <ReviewTab id={id} />}
      {tab === "cases" && <CasesTab id={id} detail={d} />}
      {tab === "verification" && <VerificationTab id={id} />}
      {tab === "audit" && <AuditTab id={id} />}
    </div>
  )
}
