import type { ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { BookMarked, FolderOpen, ScrollText, ShieldAlert, ShieldCheck } from "lucide-react"

import { ThemeSwitch } from "@/components/theme-switch"
import { api } from "@/lib/api"
import type { Route } from "@/lib/router"
import { cn } from "@/lib/utils"

const NAV = [
  { href: "#/interviews", page: "library", label: "Interviews", Icon: FolderOpen },
  { href: "#/codebook", page: "codebook", label: "Codebook", Icon: BookMarked },
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
