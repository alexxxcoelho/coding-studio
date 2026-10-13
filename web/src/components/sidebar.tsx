// Floating glass sidebar of design/macos: Corpus (Bibliothèque and the open
// interviews), Analyse, Étude, then the footer with the pseudonymisation note
// and Réglages.

import type { ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { ShieldAlert, ShieldCheck, X } from "lucide-react"

import {
  AnalyserIcon,
  CodebookIcon,
  GearIcon,
  GuideIcon,
  LibraryIcon,
  RedigerIcon,
  ShieldIcon,
  SidebarIcon,
  StudyIcon,
} from "@/components/icons"
import { api } from "@/lib/api"
import { isMac, navKeyFor } from "@/lib/nav"
import { openInterviews } from "@/lib/open-interviews"
import { interviewHref, type Route } from "@/lib/router"
import type { InterviewSummary } from "@/lib/types"
import { cn } from "@/lib/utils"

function NavItem({ href, on, icon, children, count, sub }: { href: string; on: boolean; icon?: ReactNode; children: ReactNode; count?: ReactNode; sub?: boolean }) {
  return (
    <a href={href} className={cn("it", on && "on", sub && "sub")} aria-current={on ? "page" : undefined}>
      {icon}
      {children}
      {count !== undefined && count !== null && (
        <>
          {" "}
          <span className="n">{count}</span>
        </>
      )}
    </a>
  )
}

function progress(s: InterviewSummary | undefined): string | null {
  if (!s) return null
  const p = s.phase2.n_units > 0 ? s.phase2 : s.phase1
  return p.n_units > 0 ? `${p.n_human_coded}/${p.n_units}` : null
}

function ChainNote() {
  const { data, isError } = useQuery({ queryKey: ["chain"], queryFn: api.verify, refetchInterval: 30_000 })
  if (isError) return <div className="sb-note text-destructive">Serveur injoignable</div>
  if (!data) return null
  return (
    <a
      href="#/audit"
      className={cn("sb-note no-underline", data.ok ? "" : "text-destructive")}
      title="Chaque événement du journal d'audit est recalculé contre la chaîne SHA-256"
      style={data.ok ? { color: "var(--label2)" } : undefined}
    >
      {data.ok ? <ShieldCheck className="size-3.5" aria-hidden /> : <ShieldAlert className="size-3.5" aria-hidden />}
      {data.ok ? `Journal intact, ${data.n_events} événements` : `Journal rompu à l'événement ${data.first_bad_seq}`}
    </a>
  )
}

export function Sidebar({
  route,
  openIds,
  onOpenSettings,
  onToggle,
}: {
  route: Route
  openIds: string[]
  onOpenSettings: () => void
  onToggle: () => void
}) {
  const active = navKeyFor(route)
  const interviews = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const byId = new Map((interviews.data ?? []).map((s) => [s.interview_id, s]))
  const current = route.page === "interview" ? route.id : null

  return (
    <nav className="sb flex h-full w-[232px] shrink-0 flex-col gap-px px-2.5 pt-3 pb-2.5" aria-label="Barre latérale">
      <div className="sb-top flex items-center justify-end px-1.5" data-tauri-drag-region>
        <button type="button" className="sb-toggle" onClick={onToggle} aria-label="Masquer la barre latérale" title="Masquer la barre latérale">
          <SidebarIcon />
        </button>
      </div>

      <div className="sb-h" style={{ paddingTop: 2 }}>
        Corpus
      </div>
      <NavItem href="#/interviews" on={active === "library" && current === null} icon={<LibraryIcon />} count={interviews.data ? interviews.data.length : undefined}>
        Bibliothèque
      </NavItem>
      {openIds.map((id) => (
        <div key={id} className="group/open relative">
          <NavItem href={interviewHref(id, "coding")} on={current === id} sub count={progress(byId.get(id))} icon={<span className="dot" style={{ background: current === id ? "var(--accent-c)" : "var(--label3)", width: 8, height: 8, borderRadius: "50%", display: "inline-block", flex: "none" }} />}>
            {id}
          </NavItem>
          <button
            type="button"
            aria-label={`Fermer ${id}`}
            title={`Fermer ${id}`}
            className="text-label2 hover:bg-fill2 absolute top-1/2 right-1 hidden size-4 -translate-y-1/2 cursor-pointer place-items-center rounded-full border-0 bg-transparent p-0 group-hover/open:grid focus-visible:grid"
            onClick={() => {
              openInterviews.close(id)
              if (current === id) window.location.hash = "#/interviews"
            }}
          >
            <X className="size-3" aria-hidden />
          </button>
        </div>
      ))}

      <div className="sb-h">Analyse</div>
      <NavItem href="#/codebook" on={active === "codebook"} icon={<CodebookIcon />} count={state.data && state.data.codebook_size > 0 ? state.data.codebook_size : undefined}>
        Codebook
      </NavItem>
      <NavItem href="#/analyser" on={active === "analyser"} icon={<AnalyserIcon />}>
        Analyser
      </NavItem>
      <NavItem href="#/rediger" on={active === "rediger"} icon={<RedigerIcon />}>
        Rédiger
      </NavItem>

      <div className="sb-h">Étude</div>
      <NavItem href="#/settings" on={active === "settings"} icon={<StudyIcon />}>
        Paramètres de l'étude
      </NavItem>
      <NavItem href="#/guide" on={active === "guide"} icon={<GuideIcon />}>
        Guide
      </NavItem>

      <div className="mt-auto flex flex-col gap-px pt-2">
        <div
          className="sb-note"
          title="Les textes ne quittent jamais la machine sans un geste explicite. Le remplacement des noms avant envoi arrive avec la revue (F-12)."
        >
          <ShieldIcon size={14} />
          Pseudonymisation inactive
        </div>
        <ChainNote />
        <button type="button" className="it" onClick={onOpenSettings} aria-label="Réglages">
          <GearIcon />
          Réglages
          <span className="n" aria-hidden>
            {isMac() ? "⌘," : "Ctrl+,"}
          </span>
        </button>
      </div>
    </nav>
  )
}
