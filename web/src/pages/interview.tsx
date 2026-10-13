import { useQuery } from "@tanstack/react-query"

import { Banner, EmptyState, PageTitle } from "@/components/bits"
import { AuditTab } from "@/components/interview/audit-export"
import { CasesTab } from "@/components/interview/cases"
import { SourcesTab } from "@/components/interview/sources"
import { VerificationTab } from "@/components/interview/verification"
import { Skeleton } from "@/components/ui/skeleton"
import { ToolbarActions } from "@/components/toolbar"
import { t } from "@/i18n"
import { CoderPage, StepsCapsule } from "@/pages/coder"
import { HighlightsPage } from "@/pages/highlights"
import { RevoirPage } from "@/pages/revoir"
import { api, ApiError } from "@/lib/api"
import { interviewHref, type Tab } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Phase } from "@/lib/types"

const TAB_LABELS: { tab: Tab; label: string }[] = [
  { tab: "sources", get label() { return t("interview.sources") } },
  { tab: "coding", get label() { return t("common.coder") } },
  { tab: "cases", label: "Cas" },
  { tab: "verification", get label() { return t("common.verification") } },
  { tab: "audit", get label() { return t("interview.audit_et_export") } },
]

export function InterviewPage({ id, tab, phase, unit = null }: { id: string; tab: Tab; phase: Phase; unit?: string | null }) {
  // Coder, Surlignages and Revoir are the screens of the single flow; the other tabs keep the earlier chrome until their lot.
  if (tab === "coding") return <CoderPage id={id} unit={unit} />
  if (tab === "highlights") return <HighlightsPage id={id} />
  if (tab === "ai" || tab === "review" || tab === "harmonise") return <RevoirPage id={id} part={tab} />
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
        <EmptyState title={nf ? t("interview.aucun_entretien_id_dans_la", { id }) : t("common.l_entretien_n_a_pas")}>
          {nf ? t("interview.balayez_de_nouveau_le_dossier") : detail.error.message}
        </EmptyState>
      </>
    )
  }
  const d = detail.data!

  return (
    <div className="flex flex-col gap-4">
      <PageTitle title={d.entry.interview_id} sub={`${label} · ${d.entry.folder_name}`} />
      <ToolbarActions>
        <StepsCapsule id={id} current={null} />
      </ToolbarActions>

      <nav aria-label={t("interview.sections_de_l_entretien")} className="seg self-start">
        {TAB_LABELS.map(({ tab: t, label: l }) => (
          <a key={t} href={interviewHref(id, t, t === "coding" ? phase : 1)} aria-current={t === tab ? "page" : undefined} className={cn("s", t === tab && "on")}>
            {l}
          </a>
        ))}
      </nav>

      {d.entry.warnings.length > 0 && tab === "sources" && (
        <Banner tone="warn" title={t("interview.n_avertissements_de_balayage", { n: d.entry.warnings.length })}>
          <ul className="list-inside list-disc text-xs">
            {d.entry.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </Banner>
      )}

      {tab === "sources" && <SourcesTab detail={d} />}
      {tab === "cases" && <CasesTab id={id} detail={d} />}
      {tab === "verification" && <VerificationTab id={id} />}
      {tab === "audit" && <AuditTab id={id} />}
    </div>
  )
}
