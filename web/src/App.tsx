import { AppShell } from "@/components/app-shell"
import { useRoute } from "@/lib/router"
import { AuditPage } from "@/pages/audit"
import { CodebookPage } from "@/pages/codebook"
import { CrossCasePage } from "@/pages/cross-case"
import { ToClarifyPage } from "@/pages/to-clarify"
import { InterviewPage } from "@/pages/interview"
import { LibraryPage } from "@/pages/library"

export default function App() {
  const route = useRoute()
  return (
    <AppShell route={route}>
      {route.page === "library" && <LibraryPage />}
      {route.page === "interview" && <InterviewPage key={route.id} id={route.id} tab={route.tab} phase={route.phase} unit={route.unit} />}
      {route.page === "codebook" && <CodebookPage />}
      {route.page === "to-clarify" && <ToClarifyPage />}
      {route.page === "cross-case" && <CrossCasePage />}
      {route.page === "audit" && <AuditPage />}
    </AppShell>
  )
}
