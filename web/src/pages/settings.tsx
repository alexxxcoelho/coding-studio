// Paramètres de l'étude (design/macos/Etude.dc.html): the study (name,
// description and coding rules sent to the model, research questions,
// interviewers, folder prefixes), the templates and lists of the study, the
// model template import and export, and the outputs that have no entry in the
// sidebar. The settings of the application (appearance, model providers) are in
// Réglages.

import { useState } from "react"
import { Anonymizer } from "@/components/anonymizer"

import { PageTitle, Section } from "@/components/bits"
import { VerificationDialog } from "@/components/export/verification-dialog"
import { CodebookImportSection } from "@/components/settings/codebook-import"
import { ProjectListsSection } from "@/components/settings/project-lists"
import { ProjectTemplateButtons } from "@/components/settings/project-template"
import { ResearchQuestionsEditor } from "@/components/settings/research-questions"
import { StudyConfigSection } from "@/components/settings/study-config"
import { ToolbarActions } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { t } from "@/i18n"

const EXPORTS = [
  { href: "#/audit", get label() { return t("common.journal_d_audit") }, get note() { return t("settings.chaque_action_en_ajout_seul") } },
  { href: "#/saturation", get label() { return t("settings.table_de_saturation") }, get note() { return t("settings.premier_usage_et_creation_des") } },
  { href: "#/cross-case", get label() { return t("settings.matrice_transversale") }, get note() { return t("settings.les_themes_par_entretien_et") } },
  { href: "#/to-clarify", get label() { return t("common.a_clarifier") }, get note() { return t("settings.etiquettes_provisoires_posees_en_pha") } },
]

export function SettingsPage() {
  const [verification, setVerification] = useState(false)
  return (
    <div className="flex flex-col gap-5" style={{ maxWidth: 820 }}>
      <PageTitle title={t("common.parametres_de_l_etude")} sub={t("settings.enregistres_dans_coding_db_avec")} />
      <ToolbarActions>
        <ProjectTemplateButtons />
      </ToolbarActions>
      <StudyConfigSection />
      <Section title={t("common.questions_de_recherche")}>
        <ResearchQuestionsEditor />
      </Section>
      <Section title={t("common.modeles_et_listes_de_l")}>
        <ProjectListsSection />
      </Section>
      <Section title={t("anonymizer.title")}><p className="text-sm mb-3">{t("anonymizer.local")}</p><Anonymizer /></Section>
      <CodebookImportSection />
      <Section title={t("settings.exports_et_verification")} aside={t("settings.ces_vues_sont_des_sorties")}>
        <ul className="m-0 flex list-none flex-col gap-2 p-0">
          {EXPORTS.map((e) => (
            <li key={e.href} className="flex flex-wrap items-baseline gap-x-3">
              <a href={e.href} className="font-medium">
                {e.label}
              </a>
              <span className="text-label2 text-xs">{e.note}</span>
            </li>
          ))}
          <li className="flex flex-wrap items-baseline gap-x-3">
            <Button variant="outline" onClick={() => setVerification(true)}>
              {t("persons.verification_action")}
            </Button>
            <span className="text-label2 text-xs">{t("persons.verification_note_short")}</span>
          </li>
        </ul>
      </Section>
      {verification && <VerificationDialog open onClose={() => setVerification(false)} />}
    </div>
  )
}
