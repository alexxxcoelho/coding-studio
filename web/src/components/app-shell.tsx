import type { ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { BookMarked, FolderOpen, Grid3x3, HelpCircle, NotebookPen, ScrollText, Settings, ShieldAlert, ShieldCheck, TrendingUp } from "lucide-react"

import { ThemeSwitch } from "@/components/theme-switch"
import { api } from "@/lib/api"
import type { Route } from "@/lib/router"
import { cn } from "@/lib/utils"

const NAV = [
  { href: "#/interviews", page: "library", label: "Interviews", Icon: FolderOpen },
  { href: "#/to-clarify", page: "to-clarify", label: "To clarify", Icon: HelpCircle },
  { href: "#/codebook", page: "codebook", label: "Codebook", Icon: BookMarked },
  { href: "#/cross-case", page: "cross-case", label: "Cross-case", Icon: Grid3x3 },
  { href: "#/answers", page: "answers", label: "Answers", Icon: NotebookPen },
  { href: "#/saturation", page: "saturation", label: "Saturation", Icon: TrendingUp },
  { href: "#/settings", page: "settings", label: "Settings", Icon: Settings },
  { href: "#/audit", page: "audit", label: "Audit trail", Icon: ScrollText },
] as const

export function Mark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("border-primary inline-block size-[18px] rounded-full border-2", className)}
      style={{ boxShadow: "inset 0 0 0 3px var(--sidebar), inset 0 0 0 6px var(--primary)" }}
    />
  )
}

/** Number of open labels to clarify, shown in the sidebar. */
function OpenLabels() {
  const { data } = useQuery({ queryKey: ["to-clarify", "open"], queryFn: () => api.toClarify({ open: true }), refetchInterval: 60_000 })
  const n = data?.filter((t) => !t.resolved).length ?? 0
  if (!n) return null
  return <span className="text-inductive ml-auto text-xs tabular-nums">{n}</span>
}

function ChainBadge() {
  const { data, isError } = useQuery({ queryKey: ["chain"], queryFn: api.verify, refetchInterval: 30_000 })
  if (isError) return <span className="text-destructive text-xs">Server unreachable</span>
  if (!data) return null
  return (
    <a
      href="#/audit"
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs",
        data.ok ? "text-ok hover:bg-ok-soft" : "bg-danger-soft text-destructive",
      )}
      title="Every audit event is recomputed against the SHA-256 chain"
    >
      {data.ok ? <ShieldCheck className="size-3.5" /> : <ShieldAlert className="size-3.5" />}
      {data.ok ? `Audit chain intact, ${data.n_events} events` : `Audit chain broken at event ${data.first_bad_seq}`}
    </a>
  )
}

export function AppShell({ route, children }: { route: Route; children: ReactNode }) {
  const current = route.page === "interview" ? "library" : route.page
  return (
    <div className="grid min-h-svh grid-cols-[13.5rem_1fr]">
      <aside className="bg-sidebar text-sidebar-foreground border-sidebar-border sticky top-0 flex h-svh flex-col border-r">
        <a href="#/interviews" className="flex items-center gap-2 px-4 pt-4 pb-5 font-semibold">
          <Mark />
          Coding Studio
        </a>
        <nav aria-label="Main" className="flex flex-col gap-0.5 px-2">
          {NAV.map(({ href, page, label, Icon }) => (
            <a
              key={page}
              href={href}
              aria-current={current === page ? "page" : undefined}
              className={cn(
                "flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm",
                current === page
                  ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium"
                  : "text-sidebar-foreground/75 hover:bg-sidebar-accent/60",
              )}
            >
              <Icon className="size-4" aria-hidden />
              {label}
              {page === "to-clarify" && <OpenLabels />}
            </a>
          ))}
        </nav>
        <div className="mt-auto flex flex-col gap-3 px-4 pb-4">
          <ChainBadge />
          <ThemeSwitch />
        </div>
      </aside>
      <main id="main" tabIndex={-1} className="min-w-0 px-7 py-6 outline-none">
        {children}
      </main>
    </div>
  )
}
