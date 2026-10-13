// Rédiger (step 4): the results, theme by theme. Tabs: Par thème, Synthèse par
// question, Lecture par le cadre (only if the study has a framework), Audit
// final, Décisions documentées. Guide: docs/guide/fr/05_rediger.md and
// 06_exporter_et_auditer.md.

import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { toast } from "sonner"

import { DecisionsPanel } from "@/components/analysis/decisions"
import { TabBar } from "@/components/analysis/tab-bar"
import { PageTitle } from "@/components/bits"
import { ByThemeTab } from "@/components/writing/by-theme"
import { FinalAuditTab } from "@/components/writing/final-audit"
import { FrameworkTableTab } from "@/components/writing/framework-table"
import { QuestionSynthesisTab } from "@/components/writing/question-synthesis"
import { Capsule, CapsuleItem, ToolbarActions } from "@/components/toolbar"
import { Skeleton } from "@/components/ui/skeleton"
import { analysisApi } from "@/lib/analysis-api"
import { retainedThemes, useThemes } from "@/lib/analysis-hooks"
import { downloadText } from "@/lib/download"
import { useProjectConfig } from "@/lib/hooks"
import { reportError } from "@/lib/notify"

type Tab = "theme" | "question" | "framework" | "audit" | "decisions"

export function RedigerPage() {
  const projectQ = useProjectConfig()
  const themesQ = useThemes()
  const project = projectQ.data
  const hasFramework = !!project?.interpretive_framework
  const [picked, setPicked] = useState<Tab>("theme")
  // The framework tab does not exist without a framework.
  const tab: Tab = picked === "framework" && !hasFramework ? "theme" : picked

  const tabs: { id: Tab; label: string }[] = [
    { id: "theme", label: "Par thème" },
    { id: "question", label: "Synthèse par question" },
    ...(hasFramework ? [{ id: "framework" as const, label: "Lecture par le cadre" }] : []),
    { id: "audit", label: "Audit final" },
    { id: "decisions", label: "Décisions documentées" },
  ]

  const exportMd = useMutation({
    mutationFn: analysisApi.exportWriting,
    onSuccess: (r) => {
      const saved = r.markdown ? downloadText("resultats.md", r.markdown) : false
      toast.success(r.dir ? `Export Markdown écrit dans ${r.dir}.` : saved ? "Export Markdown téléchargé." : "Export Markdown prêt.")
    },
    onError: (e) => reportError(e, "Export impossible"),
  })

  const themes = themesQ.data ?? []
  return (
    <>
      <PageTitle title="Rédiger" sub="Les résultats, thème par thème" />
      <ToolbarActions>
        <TabBar tabs={tabs} value={tab} onChange={setPicked} label="Onglets de Rédiger" />
        <Capsule>
          <CapsuleItem onClick={() => exportMd.mutate()} disabled={exportMd.isPending}>
            Exporter en Markdown
          </CapsuleItem>
        </Capsule>
      </ToolbarActions>
      <div role="tabpanel" aria-label={tabs.find((t) => t.id === tab)?.label} className="pt-1">
        {(projectQ.isLoading || themesQ.isLoading) && tab !== "decisions" && <Skeleton className="h-96 w-full" />}
        {project && !themesQ.isLoading && tab === "theme" && <ByThemeTab themes={themes} project={project} />}
        {tab === "question" && <QuestionSynthesisTab />}
        {project?.interpretive_framework && tab === "framework" && <FrameworkTableTab themes={retainedThemes(themes)} framework={project.interpretive_framework} />}
        {project && tab === "audit" && <FinalAuditTab linkTypes={project.link_types} />}
        {tab === "decisions" && <DecisionsPanel />}
      </div>
    </>
  )
}
