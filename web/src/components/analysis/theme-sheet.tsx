// The sheet of a theme: the file that defends it (guide, "Remplir la fiche de
// thème"). Proposition, research questions, supporting codes (dropped from the
// code tree), supporting and divergent cases, conditions, alternative
// interpretation, limits, status with the checks, framework reading and the
// decision memo. The text fields save themselves.

import { useState, type DragEvent } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { FrameworkChecks } from "@/components/analysis/framework-reading"
import { AutoInput, AutoTextarea } from "@/components/analysis/shared"
import { ThemePassageList } from "@/components/analysis/theme-passages"
import { Chip, NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { t } from "@/i18n"
import { analysisApi, THEME_STATUSES, type ThemeSheet, type ThemeSheetEdit, type ThemeStatus } from "@/lib/analysis-api"
import { carries, planCodeAdd, readDrag } from "@/lib/analysis-dnd"
import { akeys, invalidateThemes, useTheme } from "@/lib/analysis-hooks"
import { reportError } from "@/lib/notify"
import { checksProgress, planStatusChange, STATUS_LABEL } from "@/lib/theme-status"
import { cn } from "@/lib/utils"
import type { Case, ProjectConfig, ResearchQuestion } from "@/lib/types"

type TextKey = "proposition" | "conditions" | "alternative_interpretation" | "limits" | "decision_memo"

function caseLabel(cases: Case[], id: string): string {
  const c = cases.find((x) => x.case_id === id)
  return c?.label ? `${c.label} (${id})` : id
}

function CaseList({ label, ids, cases, onChange }: { label: string; ids: string[]; cases: Case[]; onChange: (ids: string[]) => void }) {
  const left = cases.filter((c) => !ids.includes(c.case_id))
  return (
    <div className="flex flex-col gap-1.5">
      <span className="hs">{label}</span>
      <div className="flex flex-wrap items-center gap-1.5">
        {ids.length === 0 && <span className="text-muted-foreground text-sm">{t("common.aucun")}</span>}
        {ids.map((id) => (
          <Chip key={id} value={caseLabel(cases, id)} removeLabel={`Retirer ${caseLabel(cases, id)}`} onRemove={() => onChange(ids.filter((x) => x !== id))} />
        ))}
        {left.length > 0 && (
          <NativeSelect aria-label={t("analysis.theme_sheet.ajouter_un_cas_label", { label })} value="" onChange={(e) => e.target.value && onChange([...ids, e.target.value])} style={{ width: 150 }}>
            <option value="">{t("analysis.theme_sheet.ajouter_un_cas")}</option>
            {left.map((c) => (
              <option key={c.case_id} value={c.case_id}>
                {caseLabel(cases, c.case_id)}
              </option>
            ))}
          </NativeSelect>
        )}
      </div>
    </div>
  )
}

export function ThemeSheetView({
  themeId,
  project,
  questions,
  cases,
  codeNames,
}: {
  themeId: string
  project: ProjectConfig
  questions: ResearchQuestion[]
  cases: Case[]
  /** Names of the codes by id, for the chips. */
  codeNames?: ReadonlyMap<string, string>
}) {
  const qc = useQueryClient()
  const q = useTheme(themeId)
  const [checksOpen, setChecksOpen] = useState(false)
  const [overriding, setOverriding] = useState(false)
  const [overrideReason, setOverrideReason] = useState("")
  const [dropping, setDropping] = useState(false)

  const apply = (sheet: ThemeSheet) => {
    qc.setQueryData(akeys.theme(themeId), sheet)
    void invalidateThemes(qc, themeId)
  }
  const run = async (call: () => Promise<ThemeSheet>, failure: string): Promise<boolean> => {
    try {
      apply(await call())
      return true
    } catch (e) {
      reportError(e, failure)
      return false
    }
  }
  const save = (edit: ThemeSheetEdit) => async () => {
    try {
      apply(await analysisApi.updateTheme(themeId, edit))
    } catch (e) {
      reportError(e, t("analysis.theme_sheet.fiche_non_enregistree"))
      throw e
    }
  }
  const saveText = (key: TextKey) => (v: string) => save({ [key]: v })()

  if (q.isLoading) return <Skeleton className="h-96 w-full" />
  if (q.error || !q.data) return <p className="text-destructive text-sm">{q.error?.message ?? t("analysis.theme_sheet.theme_introuvable")}</p>
  const sheet = q.data

  const checks = project.theme_checks.map((c) => ({ check_id: c.id, label: c.label, done: sheet.checks.find((s) => s.check_id === c.id)?.done ?? false }))
  const progress = checksProgress(checks)

  const changeStatus = async (to: ThemeStatus, override?: string) => {
    const plan = planStatusChange({ from: sheet.status, to, checks, overrideReason: override })
    if (plan.reason === "same") return
    if (!plan.allowed) {
      setOverriding(true)
      setChecksOpen(true)
      return
    }
    const ok = await run(() => analysisApi.setThemeStatus(themeId, { status: to, ...(plan.needsOverride ? { override_reason: override!.trim() } : {}) }), t("analysis.theme_sheet.statut_refuse"))
    if (ok) {
      setOverriding(false)
      setOverrideReason("")
    }
  }

  const onDropCode = async (e: DragEvent) => {
    e.preventDefault()
    setDropping(false)
    const p = readDrag(e.dataTransfer, "code")
    if (!p) return
    if (planCodeAdd(sheet.codes.map((c) => c.code_id), p.id) === "present") {
      toast.info(t("analysis.theme_sheet.ce_code_est_deja_dans"))
      return
    }
    await run(() => analysisApi.addThemeCode(themeId, p.id), t("common.code_non_ajoute"))
  }

  const rqs = [...questions].sort((a, b) => a.ord - b.ord)
  const nInterviews = sheet.interviews_count

  return (
    <div className="flex min-w-0 flex-col gap-4" aria-label={t("analysis.theme_sheet.fiche_du_theme")} role="region">
      <AutoInput
        label={t("analysis.theme_sheet.titre_du_theme")}
        value={sheet.title}
        onSave={(v) => save({ title: v })()}
        className="[&_input]:h-8 [&_input]:text-[15px] [&_input]:font-semibold"
      />
      <p className="text-muted-foreground -mt-2 text-[11.5px]">{t("analysis.theme_sheet.le_titre_est_une_proposition")}</p>

      <AutoTextarea label={t("analysis.theme_sheet.proposition")} value={sheet.proposition} onSave={saveText("proposition")} rows={4} hint={t("analysis.theme_sheet.deux_a_quatre_phrases_remplissez")} />

      <fieldset className="flex flex-col gap-1.5">
        <legend className="hs mb-1">{t("common.questions_de_recherche")}</legend>
        {rqs.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("analysis.theme_sheet.aucune_question_de_recherche_definis")}</p>
        ) : (
          <div className="flex flex-wrap gap-x-4 gap-y-1.5">
            {rqs.map((rq) => (
              <label key={rq.id} className="flex items-center gap-1.5 text-sm" title={rq.text}>
                <Checkbox
                  checked={sheet.research_questions.includes(rq.id)}
                  aria-label={`Question ${rq.id}`}
                  onCheckedChange={(v) =>
                    void run(
                      () =>
                        analysisApi.updateTheme(themeId, {
                          research_questions: v === true ? [...sheet.research_questions, rq.id] : sheet.research_questions.filter((x) => x !== rq.id),
                        }),
                      t("analysis.theme_sheet.questions_non_enregistrees"),
                    )
                  }
                />
                {rq.id}
              </label>
            ))}
          </div>
        )}
      </fieldset>

      <section
        aria-label={t("analysis.theme_sheet.codes_supports")}
        className={cn("flex flex-col gap-1.5 rounded-lg p-2.5 transition-colors", dropping ? "bg-accent-fill/15 outline-2 outline-dashed outline-[var(--accent-c)]" : "grp")}
        onDragOver={(e) => {
          if (carries(e.dataTransfer, "code")) {
            e.preventDefault()
            setDropping(true)
          }
        }}
        onDragLeave={() => setDropping(false)}
        onDrop={onDropCode}
      >
        <div className="flex items-baseline justify-between gap-2">
          <h3 className="hs">{t("analysis.theme_sheet.codes_supports")}</h3>
          <span className="text-muted-foreground text-[11.5px]">
            {t("common.n_codes", { n: sheet.codes.length })}, {t("common.n_entretiens", { n: nInterviews })}
          </span>
        </div>
        {sheet.codes.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("analysis.theme_sheet.glissez_ici_des_codes_de")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {sheet.codes.map((c) => (
              <li key={c.code_id} className="flex items-center gap-2 text-sm">
                <span className="min-w-0 flex-1 truncate">{c.name || codeNames?.get(c.code_id) || c.code_id}</span>
                <span className="text-muted-foreground text-[11.5px]">
                  {t("common.n_passages", { n: c.passages })}, {t("common.n_entretiens", { n: c.interviews })}
                </span>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("analysis.theme_sheet.retirer_name_du_theme", { name: c.name })}
                  onClick={() => void run(() => analysisApi.removeThemeCode(themeId, c.code_id), t("analysis.theme_sheet.code_non_retire"))}
                >
                  ×
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <CaseList label={t("analysis.theme_sheet.cas_qui_etayent")} ids={sheet.supporting_cases} cases={cases} onChange={(ids) => void run(() => analysisApi.updateTheme(themeId, { supporting_cases: ids }), t("analysis.theme_sheet.cas_non_enregistres"))} />
      <CaseList label={t("analysis.theme_sheet.cas_divergents")} ids={sheet.divergent_cases} cases={cases} onChange={(ids) => void run(() => analysisApi.updateTheme(themeId, { divergent_cases: ids }), t("analysis.theme_sheet.cas_non_enregistres"))} />

      <AutoTextarea label={t("analysis.theme_sheet.conditions")} value={sheet.conditions} onSave={saveText("conditions")} rows={2} hint={t("analysis.theme_sheet.par_exemple_un_petit_volume")} />
      <AutoTextarea label={t("analysis.theme_sheet.interpretation_alternative")} value={sheet.alternative_interpretation} onSave={saveText("alternative_interpretation")} rows={2} />
      <AutoTextarea label={t("common.limites")} value={sheet.limits} onSave={saveText("limits")} rows={2} />

      {project.interpretive_framework && <FrameworkChecks themeId={themeId} framework={project.interpretive_framework} />}

      <section className="flex flex-col gap-2" aria-label={t("analysis.theme_sheet.statut_et_controles")}>
        <div className="flex flex-wrap items-center gap-2">
          <label className="hs" htmlFor="theme-status">
            {t("analysis.theme_sheet.statut")}
          </label>
          <NativeSelect id="theme-status" value={sheet.status} onChange={(e) => void changeStatus(e.target.value as ThemeStatus)} style={{ width: 130 }}>
            {THEME_STATUSES.map((s) => (
              <option key={s} value={s}>
                {STATUS_LABEL[s]}
              </option>
            ))}
          </NativeSelect>
          <Button variant="outline" size="sm" aria-expanded={checksOpen} onClick={() => setChecksOpen((o) => !o)}>
            {t("analysis.theme_sheet.controles_avant_retenu")}
          </Button>
          <span className="text-muted-foreground text-[11.5px]">
            {t("analysis.theme_sheet.done_sur_total_faits", { done: progress.done, total: progress.total })}
          </span>
        </div>
        <p className="text-muted-foreground text-[11.5px]">{t("analysis.theme_sheet.les_controles_aident_a_decider")}</p>
        {checksOpen && (
          <ul className="grp flex flex-col gap-1.5 p-2.5" aria-label={t("analysis.theme_sheet.controles_avant_retenu")}>
            {checks.length === 0 && <li className="text-muted-foreground text-sm">{t("analysis.theme_sheet.aucun_controle_defini_dans_les")}</li>}
            {checks.map((c) => (
              <li key={c.check_id}>
                <label className="flex items-start gap-2 text-sm">
                  <Checkbox
                    className="mt-0.5"
                    checked={c.done}
                    aria-label={c.label}
                    onCheckedChange={(v) => void run(() => analysisApi.setThemeCheck(themeId, c.check_id, v === true), t("analysis.theme_sheet.controle_non_enregistre"))}
                  />
                  <span>{c.label}</span>
                </label>
              </li>
            ))}
          </ul>
        )}
        {overriding && (
          <div role="group" aria-label={t("analysis.theme_sheet.retenir_malgre_les_controles_restant")} className="note-warn flex-col items-stretch">
            <p className="text-sm">
              {progress.total - progress.done > 1 ? t("analysis.theme_sheet.v_controles_ne_sont_pas", { v: progress.total - progress.done }) : t("analysis.theme_sheet.un_controle_n_est_pas")} {t("analysis.theme_sheet.pour_retenir_quand_meme_ce")}
            </p>
            <Textarea aria-label={t("analysis.theme_sheet.raison_pour_retenir_malgre_les")} rows={2} value={overrideReason} onChange={(e) => setOverrideReason(e.target.value)} />
            <div className="flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => setOverriding(false)}>
                {t("common.annuler")}
              </Button>
              <Button size="sm" disabled={!overrideReason.trim()} onClick={() => void changeStatus("retained", overrideReason)}>
                {t("analysis.theme_sheet.retenir_malgre_tout")}
              </Button>
            </div>
          </div>
        )}
      </section>

      <AutoTextarea label={t("analysis.theme_sheet.memo_de_decision")} value={sheet.decision_memo} onSave={saveText("decision_memo")} rows={3} hint={t("analysis.theme_sheet.explique_en_quelques_lignes_pourquoi")} />

      <details className="grp p-2.5">
        <summary className="hs cursor-pointer">{t("common.passages_du_theme")}</summary>
        <div className="mt-2">
          <ThemePassageList themeId={themeId} />
        </div>
      </details>
    </div>
  )
}
