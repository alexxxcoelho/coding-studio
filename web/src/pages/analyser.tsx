import { ReviewerExport } from "@/components/reviewer-export"
// Analyser (step 3): from codes to themes, then comparison of the interviews.
// Tabs: Thèmes (list, sheet, code tree to drag from), Matrice, Explorer,
// Décisions documentées. Guide: docs/guide/fr/04_analyser.md.

import { useMemo, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { CooccurrenceTab } from "@/components/analysis/cooccurrence"
import { DecisionsPanel } from "@/components/analysis/decisions"
import { ExplorerTab } from "@/components/analysis/explorer"
import { CodeSource } from "@/components/analysis/code-source"
import { MatrixTab } from "@/components/analysis/matrix-view"
import { TabBar } from "@/components/analysis/tab-bar"
import { ThemeList } from "@/components/analysis/theme-list"
import { ThemeSheetView } from "@/components/analysis/theme-sheet"
import { EmptyState, PageTitle } from "@/components/bits"
import { ToolbarActions } from "@/components/toolbar"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { request } from "@/lib/api"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { analysisApi } from "@/lib/analysis-api"
import { akeys, invalidateThemes, useCasesAll, useQuestions, useTheme, useThemes } from "@/lib/analysis-hooks"
import { useCodes, useProjectConfig } from "@/lib/hooks"
import { reportError } from "@/lib/notify"

type Tab = "themes" | "matrix" | "explorer" | "cooccurrence" | "decisions"
const TABS: { id: Tab; label: string }[] = [
  { id: "themes", get label() { return t("common.themes") } },
  { id: "matrix", get label() { return t("common.matrice") } },
  { id: "explorer", get label() { return t("analyser.explorer") } },
  { id: "cooccurrence", label: "Co-occurrence" },
  { id: "decisions", get label() { return t("common.decisions_documentees") } },
]

function ThemesTab() {
  const qc = useQueryClient()
  const themesQ = useThemes()
  const projectQ = useProjectConfig()
  const codesQ = useCodes()
  const questionsQ = useQuestions()
  const casesQ = useCasesAll()
  const [pickedId, setPickedId] = useState<string | null>(null)

  const themes = useMemo(() => [...(themesQ.data ?? [])].sort((a, b) => a.ord - b.ord), [themesQ.data])
  const selectedId = themes.some((t) => t.theme_id === pickedId) ? pickedId : (themes[0]?.theme_id ?? null)
  const sheetQ = useTheme(selectedId)

  const create = useMutation({
    mutationFn: (title: string) => analysisApi.createTheme(title),
    onSuccess: (sheet) => {
      qc.setQueryData(akeys.theme(sheet.theme_id), sheet)
      void invalidateThemes(qc)
      setPickedId(sheet.theme_id)
    },
    onError: (e) => reportError(e, t("analyser.theme_non_cree")),
  })
  const addCode = useMutation({
    mutationFn: (codeId: string) => analysisApi.addThemeCode(selectedId!, codeId),
    onSuccess: (sheet) => {
      qc.setQueryData(akeys.theme(sheet.theme_id), sheet)
      void invalidateThemes(qc, sheet.theme_id)
    },
    onError: (e) => reportError(e, t("common.code_non_ajoute")),
  })

  if (themesQ.isLoading || projectQ.isLoading) return <Skeleton className="h-96 w-full" />
  if (themesQ.error) return <EmptyState title={t("analyser.themes_indisponibles")}>{themesQ.error.message}</EmptyState>

  const codes = codesQ.data?.codes ?? []
  const selected = themes.find((t) => t.theme_id === selectedId)
  const inTheme = new Set((sheetQ.data?.codes ?? []).map((c) => c.code_id))

  return (
    <div className="grid min-w-0 gap-6 lg:grid-cols-[250px_minmax(0,1fr)_250px]">
      <ThemeList themes={themes} selectedId={selectedId} onSelect={setPickedId} onCreate={(t) => create.mutate(t)} creating={create.isPending} />
      <div className="min-w-0">
        {selected && projectQ.data ? (
          <ThemeSheetView key={selected.theme_id} themeId={selected.theme_id} project={projectQ.data} questions={questionsQ.data ?? []} cases={casesQ.data ?? []} />
        ) : (
          <EmptyState title={t("analyser.aucun_theme_selectionne")}>
            {t("analyser.un_theme_regroupe_des_codes")}
          </EmptyState>
        )}
      </div>
      <CodeSource codes={codes} inTheme={inTheme} onAdd={(id) => addCode.mutate(id)} disabled={!selected} />
    </div>
  )
}

export function AnalyserPage() {
  const projectQ = useProjectConfig()
  const [reviewerOpen, setReviewerOpen] = useState(false)
  const [tab, setTab] = useState<Tab>("themes")
  const exportReport = useMutation({
    mutationFn: () => request<{ dir: string }>("POST", "/api/analysis/export"),
    onSuccess: (r) => toast.success(t("analyser.export_report_saved", { dir: r.dir })),
    onError: (e) => reportError(e, t("rediger.export_impossible")),
  })
  return (
    <>
      <ReviewerExport open={reviewerOpen} onOpenChange={setReviewerOpen} />
      <PageTitle title={t("common.analyser")} sub={t("analyser.des_codes_aux_themes_puis")} />
      <ToolbarActions>
        <Button variant="outline" onClick={() => setReviewerOpen(true)}>{t("reviewer.title")}</Button>
        <Button variant="outline" onClick={() => exportReport.mutate()} disabled={exportReport.isPending} title={t("analyser.export_report_help")}>{t("analyser.export_report")}</Button>
        <TabBar tabs={TABS} value={tab} onChange={setTab} label={t("analyser.onglets_d_analyser")} />
      </ToolbarActions>
      <div role="tabpanel" aria-label={TABS.find((t) => t.id === tab)?.label} className="pt-1">
        {tab === "themes" && <ThemesTab />}
        {tab === "matrix" && <MatrixTab project={projectQ.data} />}
        {tab === "explorer" && <ExplorerTab project={projectQ.data} />}
        {tab === "cooccurrence" && <CooccurrenceTab />}
        {tab === "decisions" && <DecisionsPanel />}
      </div>
    </>
  )
}
