// Settings: study configuration, codebook import and model providers. Each
// part is a region of its own; a refusal of the server is shown inside the
// region that made the request.

import { PageTitle } from "@/components/bits"
import { CodebookImportSection } from "@/components/settings/codebook-import"
import { ProvidersSection } from "@/components/settings/model-providers"
import { StudyConfigSection } from "@/components/settings/study-config"

export function SettingsPage({ pollMs = 1500 }: { pollMs?: number }) {
  return (
    <div className="flex flex-col gap-5">
      <PageTitle title="Settings" sub="The study configuration, the codebook import and the model providers of this database." />
      <StudyConfigSection />
      <CodebookImportSection />
      <ProvidersSection pollMs={pollMs} />
    </div>
  )
}
