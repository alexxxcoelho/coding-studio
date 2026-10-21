// Rédiger (step 4): the results, theme by theme. Tabs: Par thème, Synthèse par
// question, Lecture par le cadre (only if the study has a framework), Audit
// final, Décisions documentées. Guide: docs/guide/fr/05_rediger.md and
// 06_exporter_et_auditer.md.

import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { toast } from "sonner"

import { JsonImportDialog } from "@/components/json-import-dialog"
import { request } from "@/lib/api"
import { DecisionsPanel } from "@/components/analysis/decisions"
import { TabBar } from "@/components/analysis/tab-bar"
import { PageTitle } from "@/components/bits"
import { ByThemeTab } from "@/components/writing/by-theme"
import { FinalAuditTab } from "@/components/writing/final-audit"
import { FrameworkTableTab } from "@/components/writing/framework-table"
import { QuestionSynthesisTab } from "@/components/writing/question-synthesis"
import { Capsule, CapsuleItem, ToolbarActions } from "@/components/toolbar"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
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
  const [importOpen,setImportOpen]=useState(false)
  const [picked, setPicked] = useState<Tab>("theme")
  // The framework tab does not exist without a framework.
  const tab: Tab = picked === "framework" && !hasFramework ? "theme" : picked

  const tabs: { id: Tab; label: string }[] = [
    { id: "theme", label: t("rediger.par_theme") },
    { id: "question", label: t("rediger.synthese_par_question") },
    ...(hasFramework ? [{ id: "framework" as const, label: t("common.lecture_par_le_cadre") }] : []),
    { id: "audit", label: t("rediger.audit_final") },
    { id: "decisions", label: t("common.decisions_documentees") },
  ]

  const exportMd = useMutation({
    mutationFn: analysisApi.exportWriting,
    onSuccess: (r) => {
      const saved = r.markdown ? downloadText("resultats.md", r.markdown) : false
      toast.success(r.dir ? t("rediger.export_markdown_ecrit_dans_dir", { dir: r.dir }) : saved ? t("rediger.export_markdown_telecharge") : t("rediger.export_markdown_pret"))
    },
    onError: (e) => reportError(e, t("rediger.export_impossible")),
  })

  const themes = themesQ.data ?? []
  return (
    <>
      <JsonImportDialog open={importOpen} onOpenChange={setImportOpen} endpoint="/api/writing/json/import" title={t("writing.json.import")} description={t("writing.json.help")} replacementLabel={n=>t("writing.json.replace",{n})}/>
      <PageTitle title={t("common.rediger")} sub={t("rediger.les_resultats_theme_par_theme")} />
      <ToolbarActions>
        <TabBar tabs={tabs} value={tab} onChange={setPicked} label={t("rediger.onglets_de_rediger")} />
      </ToolbarActions>
      <div className="flex justify-end">
        <Capsule>
          <CapsuleItem onClick={async()=>{try{const doc=await request<unknown>("GET","/api/writing/json");if(!downloadText("writing.json",JSON.stringify(doc,null,2)+"\n","application/json;charset=utf-8"))throw new Error(t("rediger.export_impossible"))}catch(e){reportError(e,t("rediger.export_impossible"))}}}>{t("writing.json.export")}</CapsuleItem>
          <CapsuleItem onClick={()=>setImportOpen(true)}>{t("writing.json.import")}</CapsuleItem>
          <CapsuleItem onClick={() => exportMd.mutate()} disabled={exportMd.isPending}>
            {t("rediger.exporter_en_markdown")}
          </CapsuleItem>
        </Capsule>
      </div>
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
