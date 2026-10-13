// Paramètres de l'étude (design/macos/Etude.dc.html): the study (name,
// description and coding rules sent to the model, research questions,
// interviewers, folder prefixes), the templates and lists of the study, the
// model template import and export, and the outputs that have no entry in the
// sidebar. The settings of the application (appearance, model providers) are in
// Réglages.

import { PageTitle, Section } from "@/components/bits"
import { CodebookImportSection } from "@/components/settings/codebook-import"
import { ProjectListsSection } from "@/components/settings/project-lists"
import { ProjectTemplateButtons } from "@/components/settings/project-template"
import { ResearchQuestionsEditor } from "@/components/settings/research-questions"
import { StudyConfigSection } from "@/components/settings/study-config"
import { ToolbarActions } from "@/components/toolbar"

const EXPORTS = [
  { href: "#/audit", label: "Journal d'audit", note: "Chaque action, en ajout seul, avec la chaîne SHA-256." },
  { href: "#/saturation", label: "Table de saturation", note: "Premier usage et création des codes, entretien par entretien." },
  { href: "#/cross-case", label: "Matrice transversale", note: "Les thèmes par entretien et par dimension." },
  { href: "#/to-clarify", label: "À clarifier", note: "Étiquettes provisoires posées en phase 2." },
]

export function SettingsPage() {
  return (
    <div className="flex flex-col gap-5" style={{ maxWidth: 820 }}>
      <PageTitle title="Paramètres de l'étude" sub="Enregistrés dans coding.db, avec l'étude" />
      <ToolbarActions>
        <ProjectTemplateButtons />
      </ToolbarActions>
      <StudyConfigSection />
      <Section title="Questions de recherche">
        <ResearchQuestionsEditor />
      </Section>
      <Section title="Modèles et listes de l'étude">
        <ProjectListsSection />
      </Section>
      <CodebookImportSection />
      <Section title="Exports et vérification" aside="ces vues sont des sorties, pas des étapes">
        <ul className="m-0 flex list-none flex-col gap-2 p-0">
          {EXPORTS.map((e) => (
            <li key={e.href} className="flex flex-wrap items-baseline gap-x-3">
              <a href={e.href} className="font-medium">
                {e.label}
              </a>
              <span className="text-label2 text-xs">{e.note}</span>
            </li>
          ))}
        </ul>
      </Section>
    </div>
  )
}
