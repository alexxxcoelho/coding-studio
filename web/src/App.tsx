import { AppShell } from "@/components/app-shell"
import { useRoute } from "@/lib/router"
import { AuditPage } from "@/pages/audit"
import { CodebookPage } from "@/pages/codebook"
import { InterviewPage } from "@/pages/interview"
import { LibraryPage } from "@/pages/library"

export default function App() {
  const route = useRoute()
  return (
    <AppShell route={route}>
      {route.page === "library" && <LibraryPage />}
      {route.page === "interview" && <InterviewPage key={route.id} id={route.id} tab={route.tab} phase={route.phase} />}
      {route.page === "codebook" && <CodebookPage />}
      {route.page === "audit" && <AuditPage />}
    </AppShell>
  )
}
