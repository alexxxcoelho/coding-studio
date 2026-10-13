import { AppShell } from "@/components/app-shell"
import { CodeNamesProvider } from "@/lib/code-names"
import { useRoute } from "@/lib/router"
import { GuidePage } from "@/pages/guide"
import { AnalyserPage } from "@/pages/analyser"
import { RedigerPage } from "@/pages/rediger"
import { AuditPage } from "@/pages/audit"
import { CodebookPage } from "@/pages/codebook"
import { ToClarifyPage } from "@/pages/to-clarify"
import { InterviewPage } from "@/pages/interview"
import { LexiquePage } from "@/pages/lexique"
import { LibraryPage } from "@/pages/library"
import { SearchPage } from "@/pages/search"
import { SaturationPage } from "@/pages/saturation"
import { SettingsPage } from "@/pages/settings"

export default function App() {
  const route = useRoute()
  return (
    <CodeNamesProvider>
      <AppShell route={route}>
        {route.page === "library" && <LibraryPage />}
        {route.page === "interview" && <InterviewPage key={route.id} id={route.id} tab={route.tab} phase={route.phase} unit={route.unit} />}
        {route.page === "codebook" && <CodebookPage />}
        {route.page === "to-clarify" && <ToClarifyPage />}
        {route.page === "saturation" && <SaturationPage />}
        {route.page === "settings" && <SettingsPage />}
        {route.page === "audit" && <AuditPage />}
        {route.page === "analyser" && <AnalyserPage />}
        {route.page === "lexique" && <LexiquePage />}
        {route.page === "search" && <SearchPage />}
        {route.page === "rediger" && <RedigerPage />}
        {route.page === "guide" && <GuidePage anchor={route.anchor} />}
      </AppShell>
    </CodeNamesProvider>
  )
}
