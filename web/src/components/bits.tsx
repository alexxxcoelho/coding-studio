// Small shared pieces: code chips, the coding rail, banners, empty states,
// error reporting.

import { useId, type ReactNode } from "react"
import { AlertTriangle, CheckCircle2, Info, Lock, OctagonX } from "lucide-react"

import { ToolbarActions, ToolbarTitle } from "@/components/toolbar"
import { familyKey, familyOf, FAMILY_ORDER, NA, type DiffMark } from "@/lib/codes"
import { cn } from "@/lib/utils"
import type { Code } from "@/lib/types"

export function CodeChip({
  code,
  family,
  mark,
  title,
  className,
}: {
  code: string
  family?: string
  mark?: DiffMark
  title?: string
  className?: string
}) {
  const fam = family ?? familyOf(code)
  return (
    <span
      className={cn("chip", className)}
      data-mark={mark}
      title={title}
      style={{ ["--fam" as string]: `var(--fam-${code === NA ? "na" : familyKey(fam)})` }}
    >
      {code}
    </span>
  )
}

export function ChipList({ codes, book, empty = "none" }: { codes: string[]; book?: Code[]; empty?: string }) {
  if (!codes.length) return <span className="text-muted-foreground text-xs">{empty}</span>
  return (
    <span className="inline-flex flex-wrap gap-1">
      {codes.map((c) => (
        <CodeChip key={c} code={c} family={familyOf(c, book)} />
      ))}
    </span>
  )
}

/** A provisional label to clarify (dashed chip). */
export function PendingChip({ label, note, className }: { label: string; note?: string | null; className?: string }) {
  return (
    <span className={cn("chip-pending", className)} title={note ? `To clarify: ${note}` : "To clarify"}>
      ? {label}
    </span>
  )
}

/** Margin rail: one band per family the unit carries; hatched when uncoded. */
export function CodeRail({ codes, book }: { codes: string[] | null | undefined; book?: Code[] }) {
  if (!codes || !codes.length) return <span className="rail" data-state="empty" aria-hidden />
  const fams = [...new Set(codes.map((c) => (c === NA ? "NA" : familyOf(c, book))))].sort(
    (a, b) => FAMILY_ORDER.indexOf(a) - FAMILY_ORDER.indexOf(b),
  )
  return (
    <span className="rail" aria-hidden>
      {fams.map((f) => (
        <span key={f} style={{ background: `var(--fam-${f === "NA" ? "na" : familyKey(f)})` }} />
      ))}
    </span>
  )
}

type Tone = "info" | "warn" | "danger" | "ok" | "lock"

const TONES: Record<Tone, { cls: string; Icon: typeof Info }> = {
  info: { cls: "bg-accent text-accent-foreground border-primary/20", Icon: Info },
  warn: { cls: "bg-warn-soft text-warn border-warn/25", Icon: AlertTriangle },
  danger: { cls: "bg-danger-soft text-destructive border-destructive/25", Icon: OctagonX },
  ok: { cls: "bg-ok-soft text-ok border-ok/25", Icon: CheckCircle2 },
  lock: { cls: "bg-muted text-foreground border-border", Icon: Lock },
}

export function Banner({
  tone = "info",
  title,
  children,
  action,
  className,
}: {
  tone?: Tone
  title: ReactNode
  children?: ReactNode
  action?: ReactNode
  className?: string
}) {
  const { cls, Icon } = TONES[tone]
  return (
    <div role="status" className={cn("flex items-start gap-3 rounded-md border px-3.5 py-2.5 text-sm", cls, className)}>
      <Icon className="mt-0.5 size-4 shrink-0" aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="font-medium">{title}</div>
        {children && <div className="mt-0.5 opacity-90">{children}</div>}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  )
}

export function EmptyState({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="border-border bg-card rounded-md border border-dashed px-6 py-10">
      <p className="text-base font-medium">{title}</p>
      {children && <p className="text-muted-foreground mt-1 max-w-prose text-sm">{children}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  )
}

/** Page title: rendered in the unified toolbar of the shell, inline without one. */
export function PageTitle({ title, sub, actions }: { title: ReactNode; sub?: ReactNode; actions?: ReactNode }) {
  return (
    <>
      <ToolbarTitle title={title} sub={sub} />
      {actions && <ToolbarActions>{actions}</ToolbarActions>}
    </>
  )
}

export function Section({ title, aside, children, className }: { title: ReactNode; aside?: ReactNode; children: ReactNode; className?: string }) {
  // Named by its title, so that it is a labelled region for assistive tech.
  const titleId = useId()
  return (
    <section aria-labelledby={titleId} className={cn("bg-card rounded-md border", className)}>
      <header className="flex items-center justify-between gap-3 border-b px-4 py-2.5">
        <h2 id={titleId} className="text-sm font-semibold">
          {title}
        </h2>
        {aside && <div className="text-muted-foreground text-xs">{aside}</div>}
      </header>
      <div className="p-4">{children}</div>
    </section>
  )
}

export function Stat({ label, value, tone }: { label: string; value: ReactNode; tone?: "ok" | "warn" }) {
  return (
    <div className="min-w-0">
      <div className="text-muted-foreground text-xs">{label}</div>
      <div className={cn("text-sm font-semibold tabular-nums", tone === "ok" && "text-ok", tone === "warn" && "text-warn")}>{value}</div>
    </div>
  )
}
