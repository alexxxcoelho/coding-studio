// Unified toolbar of design/macos: title and subtitle at the left, then the
// capsule groups and buttons a page supplies, a search field and the "?" help
// button. The shell renders the bar; a page fills it with <ToolbarTitle>,
// <ToolbarActions>, <Capsule> and <ToolbarSearch>. Without a shell (tests, a
// page rendered on its own) the same parts render inline.

import { useContext, type ReactNode } from "react"
import { createPortal } from "react-dom"

import { SearchIcon, SidebarIcon } from "@/components/icons"
import { ToolbarSlotsContext } from "@/components/toolbar-context"
import { t } from "@/i18n"
import { cn } from "@/lib/utils"

/** The bar itself. `titleRef` and `actionsRef` hand the slot elements to the provider. */
export function Toolbar({
  titleRef,
  actionsRef,
  onHelp,
  onToggleSidebar,
  sidebarHidden,
}: {
  titleRef: (el: HTMLElement | null) => void
  actionsRef: (el: HTMLElement | null) => void
  onHelp: () => void
  onToggleSidebar?: () => void
  sidebarHidden?: boolean
}) {
  return (
    <header className="tb" data-tauri-drag-region>
      {sidebarHidden && onToggleSidebar && (
        <button type="button" className="sb-toggle" onClick={onToggleSidebar} aria-label={t("toolbar.afficher_la_barre_laterale")} title={t("toolbar.afficher_la_barre_laterale")}>
          <SidebarIcon />
        </button>
      )}
      <div className="ttl" ref={titleRef} />
      <div className="contents" ref={actionsRef} />
      <button type="button" className="help" onClick={onHelp} aria-label={t("toolbar.aide_pour_cet_ecran")} title={t("toolbar.aide_pour_cet_ecran")}>
        ?
      </button>
    </header>
  )
}

/** Title and subtitle of the page, in the toolbar. */
export function ToolbarTitle({ title, sub }: { title: ReactNode; sub?: ReactNode }) {
  const slots = useContext(ToolbarSlotsContext)
  const subText = typeof sub === "string" ? sub : undefined
  const body = (
    <>
      <h1 className="m-0 text-[15px] leading-[1.2] font-bold">{title}</h1>
      {sub && <span title={subText}>{sub}</span>}
    </>
  )
  if (!slots) return <div className="ttl mb-4">{body}</div>
  return slots.title ? createPortal(body, slots.title) : null
}

/** Buttons, capsules and fields of the page, in the toolbar after the title. */
export function ToolbarActions({ children }: { children: ReactNode }) {
  const slots = useContext(ToolbarSlotsContext)
  if (!slots) return <div className="mb-4 flex flex-wrap items-center gap-2">{children}</div>
  return slots.actions ? createPortal(children, slots.actions) : null
}

/** A group of toolbar items in a glass capsule. */
export function Capsule({ children, label, role }: { children: ReactNode; label?: string; role?: string }) {
  return (
    <div className="cap" aria-label={label} role={role}>
      {children}
    </div>
  )
}

export function CapsuleItem({
  on,
  count,
  className,
  children,
  ...rest
}: { on?: boolean; count?: ReactNode } & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" className={cn("ci", on && "on", className)} aria-pressed={on} {...rest}>
      {children}
      {count !== undefined && <span className="n">{count}</span>}
    </button>
  )
}

export function ToolbarSearch({
  value,
  onChange,
  placeholder = "Rechercher",
}: {
  value: string
  onChange: (v: string) => void
  placeholder?: string
}) {
  return (
    <label className="search">
      <SearchIcon />
      <input type="search" value={value} placeholder={placeholder} aria-label={placeholder} onChange={(e) => onChange(e.target.value)} />
    </label>
  )
}
