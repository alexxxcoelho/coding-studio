// Window of the application: floating glass sidebar, unified toolbar, content,
// and the two overlays every screen shares (Réglages and the help drawer).

import { useCallback, useEffect, useState, type ReactNode } from "react"

import { HelpDrawer } from "@/components/help-drawer"
import { SettingsDialog } from "@/components/settings-dialog"
import { useSettingsDialog } from "@/components/use-settings-dialog"
import { Sidebar } from "@/components/sidebar"
import { Toolbar } from "@/components/toolbar"
import { ToolbarSlotsContext } from "@/components/toolbar-context"
import { useAppearance } from "@/lib/appearance"
import { screenForRoute } from "@/lib/guide"
import { isSettingsShortcut } from "@/lib/nav"
import { openInterviews, useOpenInterviews } from "@/lib/open-interviews"
import type { Route } from "@/lib/router"

const SIDEBAR_KEY = "cs-sidebar-hidden"

function readHidden(): boolean {
  try {
    return window.localStorage.getItem(SIDEBAR_KEY) === "1"
  } catch {
    return false
  }
}

export function AppShell({ route, children, pollMs }: { route: Route; children: ReactNode; pollMs?: number }) {
  useAppearance() // follows the system theme while on "Automatique"
  const settings = useSettingsDialog()
  const [helpOpen, setHelpOpen] = useState(false)
  const [hidden, setHidden] = useState(readHidden)
  const [titleEl, setTitleEl] = useState<HTMLElement | null>(null)
  const [actionsEl, setActionsEl] = useState<HTMLElement | null>(null)
  const openIds = useOpenInterviews()

  const interviewId = route.page === "interview" ? route.id : null
  useEffect(() => {
    if (interviewId) openInterviews.open(interviewId)
  }, [interviewId])

  const { setOpen: setSettingsOpen } = settings
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (!isSettingsShortcut(e)) return
      e.preventDefault()
      setSettingsOpen(true)
    }
    window.addEventListener("keydown", on)
    return () => window.removeEventListener("keydown", on)
  }, [setSettingsOpen])

  const toggleSidebar = useCallback(() => {
    setHidden((h) => {
      try {
        window.localStorage.setItem(SIDEBAR_KEY, h ? "0" : "1")
      } catch {
        // Storage unavailable: the choice lasts for this session only.
      }
      return !h
    })
  }, [])

  return (
    <div className="app-win bg-background flex min-h-svh gap-2 p-2" data-collapsed={hidden}>
      {!hidden && (
        <div className="sticky top-2 h-[calc(100svh-1rem)] shrink-0 self-start">
          <div className="h-full overflow-y-auto rounded-[14px]">
            <Sidebar route={route} openIds={openIds} onOpenSettings={() => settings.setOpen(true)} onToggle={toggleSidebar} />
          </div>
        </div>
      )}
      <div className="app-ct flex min-w-0 flex-1 flex-col">
        <ToolbarSlotsContext.Provider value={{ title: titleEl, actions: actionsEl }}>
          <div className="bg-background/85 sticky top-0 z-10 backdrop-blur-md">
            <Toolbar
              titleRef={setTitleEl}
              actionsRef={setActionsEl}
              onHelp={() => setHelpOpen(true)}
              onToggleSidebar={toggleSidebar}
              sidebarHidden={hidden}
            />
          </div>
          <main id="main" tabIndex={-1} className="min-w-0 flex-1 px-5 pt-1 pb-6 outline-none">
            {children}
          </main>
        </ToolbarSlotsContext.Provider>
      </div>
      <SettingsDialog open={settings.open} onOpenChange={settings.setOpen} tab={settings.tab} onTabChange={settings.setTab} pollMs={pollMs} />
      <HelpDrawer open={helpOpen} onOpenChange={setHelpOpen} screen={screenForRoute(route)} />
    </div>
  )
}
