// Floating glass sidebar of design/macos: Corpus (Bibliothèque and the open
// interviews), Analyse, Étude, then the footer with the pseudonymisation note
// and Réglages.

import type { ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { Search, TextSearch, Users, X } from "lucide-react"

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
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { useCodes } from "@/lib/hooks"
import { usePersonGroups } from "@/lib/persons-api"
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

/** Mandatory masking is independent of the retired optional switch. */
function PseudoNote() {
  return <a href="#/settings" className="sb-note no-underline" style={{color:"var(--label2)"}} title={t("anonymizer.sidebar_help")}><ShieldIcon size={14}/>{t("anonymizer.sidebar")}</a>
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
  const codes = useCodes()
  const codeCount = codes.data?.codes?.filter((c) => c.active && !c.system && !c.merged_into).length
  const persons = usePersonGroups()
  const byId = new Map((interviews.data ?? []).map((s) => [s.interview_id, s]))
  const current = route.page === "interview" ? route.id : null

  return (
    <nav className="sb flex h-full w-[232px] shrink-0 flex-col gap-px px-2.5 pt-3 pb-2.5" aria-label={t("sidebar.barre_laterale")}>
      <div className="sb-top flex items-center justify-end px-1.5" data-tauri-drag-region>
        <button type="button" className="sb-toggle" onClick={onToggle} aria-label={t("sidebar.masquer_la_barre_laterale")} title={t("sidebar.masquer_la_barre_laterale")}>
          <SidebarIcon />
        </button>
      </div>

      <div className="sb-h" style={{ paddingTop: 2 }}>
        {t("sidebar.corpus")}
      </div>
      <NavItem href="#/interviews" on={active === "library" && current === null} icon={<LibraryIcon />} count={interviews.data ? interviews.data.length : undefined}>
        {t("common.bibliotheque")}
      </NavItem>
      {openIds.map((id) => (
        <div key={id} className="group/open relative">
          <NavItem href={interviewHref(id, "coding")} on={current === id} sub count={progress(byId.get(id))} icon={<span className="dot" style={{ background: current === id ? "var(--accent-c)" : "var(--label3)", width: 8, height: 8, borderRadius: "50%", display: "inline-block", flex: "none" }} />}>
            {id}
          </NavItem>
          <button
            type="button"
            aria-label={t("sidebar.close_interview", { id })}
            title={t("sidebar.close_interview", { id })}
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

      <NavItem href="#/personnes" on={active === "persons"} icon={<Users className="size-4" aria-hidden />} count={persons.data && persons.data.n_persons > 0 ? persons.data.n_persons : undefined}>
        {t("persons.title")}
      </NavItem>

      <div className="sb-h">{t("sidebar.analyse")}</div>
      <NavItem href="#/codebook" on={active === "codebook"} icon={<CodebookIcon />} count={codeCount || undefined}>
        {t("common.codebook")}
      </NavItem>
      <NavItem href="#/lexique" on={active === "lexique"} icon={<Search className="size-4" aria-hidden />}>
        {t("common.lexique")}
      </NavItem>
      <NavItem href="#/recherche" on={active === "search"} icon={<TextSearch className="size-4" aria-hidden />}>
        {t("common.recherche")}
      </NavItem>
      <NavItem href="#/analyser" on={active === "analyser"} icon={<AnalyserIcon />}>
        {t("common.analyser")}
      </NavItem>
      <NavItem href="#/rediger" on={active === "rediger"} icon={<RedigerIcon />}>
        {t("common.rediger")}
      </NavItem>

      <div className="sb-h">{t("common.etude")}</div>
      <NavItem href="#/settings" on={active === "settings"} icon={<StudyIcon />}>
        {t("common.parametres_de_l_etude")}
      </NavItem>
      <NavItem href="#/guide" on={active === "guide"} icon={<GuideIcon />}>
        {t("common.guide")}
      </NavItem>

      <div className="mt-auto flex flex-col gap-px pt-2">
        <PseudoNote />
        <button type="button" className="it" onClick={onOpenSettings} aria-label={t("sidebar.reglages")}>
          <GearIcon />
          {t("sidebar.reglages")}
          <span className="n" aria-hidden>
            {isMac() ? "⌘," : "Ctrl+,"}
          </span>
        </button>
      </div>
    </nav>
  )
}
