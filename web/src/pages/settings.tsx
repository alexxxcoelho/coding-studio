// Paramètres de l'étude: the study configuration and the codebook import. The
// settings of the application (appearance, model providers) are in Réglages.
// Each part is a region of its own; a refusal of the server is shown inside the
// region that made the request.

import { PageTitle, Section } from "@/components/bits"
import { CodebookImportSection } from "@/components/settings/codebook-import"
import { StudyConfigSection } from "@/components/settings/study-config"

const EXPORTS = [
  { href: "#/audit", label: "Journal d'audit", note: "Chaque action, en ajout seul, avec la chaîne SHA-256." },
  { href: "#/saturation", label: "Saturation", note: "Premier usage et création des codes, entretien par entretien." },
  { href: "#/to-clarify", label: "À clarifier", note: "Étiquettes provisoires posées en phase 2." },
]

export function SettingsPage() {
  return (
    <div className="flex flex-col gap-5">
      <PageTitle title="Paramètres de l'étude" sub="Configuration de l'étude et import du codebook de cette base" />
      <StudyConfigSection />
      <CodebookImportSection />
      <Section title="Exports et vérification" aside="pages sans entrée dans la barre latérale">
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
