// Small pieces shared by the Analyser and Rédiger screens: autosaved fields,
// passage rows, wording helpers.

import { useId, type DragEvent, type ReactNode } from "react"

import type { PassageBrief, VerificationBadge } from "@/lib/analysis-api"
import { passageRef } from "@/lib/analysis-text"
import { SAVE_STATUS_LABEL, useAutosavedText, type SaveStatus } from "@/lib/autosave"
import { unitHref } from "@/lib/router"
import { writeDrag } from "@/lib/analysis-dnd"
import { cn } from "@/lib/utils"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"

const VERIF_LABEL: Record<VerificationBadge, string> = {
  verified: "Vérifié à l'audio",
  unverified: "Non vérifié à l'audio",
  unusable: "Inutilisable",
}

export function VerificationLabel({ state }: { state: VerificationBadge | null }) {
  if (!state) return null
  return <span className={cn("text-[11.5px]", state === "verified" ? "text-ok" : "text-warn")}>{VERIF_LABEL[state]}</span>
}

export function SaveBadge({ status }: { status: SaveStatus }) {
  if (status === "idle") return null
  return (
    <span role="status" className={cn("text-[11.5px]", status === "error" ? "text-destructive" : "text-muted-foreground")}>
      {SAVE_STATUS_LABEL[status]}
    </span>
  )
}

/** A textarea that saves itself after a pause, on blur and on leaving. */
export function AutoTextarea({
  label,
  hint,
  value,
  onSave,
  rows = 3,
  placeholder,
  className,
  serif,
  hideLabel,
  extra,
}: {
  label: string
  hint?: ReactNode
  value: string
  onSave: (v: string) => Promise<unknown>
  rows?: number
  placeholder?: string
  className?: string
  serif?: boolean
  hideLabel?: boolean
  extra?: ReactNode
}) {
  const f = useAutosavedText(value, onSave)
  const id = useId()
  return (
    <div className={cn("flex min-w-0 flex-col gap-1", className)}>
      <div className="flex items-center justify-between gap-2">
        <label className={cn("hs", hideLabel && "sr-only")} htmlFor={id}>
          {label}
        </label>
        {extra}
      </div>
      <Textarea
        id={id}
        value={f.value}
        rows={rows}
        placeholder={placeholder}
        onChange={(e) => f.onChange(e.target.value)}
        onBlur={() => void f.flush()}
        className={cn("text-sm", serif && "font-serif text-base leading-relaxed")}
      />
      <div className="flex items-center justify-between gap-2 empty:hidden">
        {hint && <span className="text-muted-foreground text-[11.5px]">{hint}</span>}
        <span className="ml-auto">
          <SaveBadge status={f.status} />
        </span>
      </div>
    </div>
  )
}

/** A one-line field that saves itself, same rules as {@link AutoTextarea}. */
export function AutoInput({
  label,
  value,
  onSave,
  placeholder,
  className,
  hideLabel,
}: {
  label: string
  value: string
  onSave: (v: string) => Promise<unknown>
  placeholder?: string
  className?: string
  hideLabel?: boolean
}) {
  const f = useAutosavedText(value, onSave)
  const id = useId()
  return (
    <div className={cn("flex min-w-0 flex-col gap-1", className)}>
      <label className={cn("hs", hideLabel && "sr-only")} htmlFor={id}>
        {label}
      </label>
      <Input
        id={id}
        value={f.value}
        placeholder={placeholder}
        onChange={(e) => f.onChange(e.target.value)}
        onBlur={() => void f.flush()}
        onKeyDown={(e) => {
          if (e.key === "Enter") void f.flush()
        }}
      />
      <SaveBadge status={f.status} />
    </div>
  )
}

/** A passage as a card: reference, quote, verification, a link to open it in the Coder. */
export function PassageCard({
  passage,
  draggable,
  top,
  foot,
  className,
}: {
  passage: PassageBrief
  draggable?: boolean
  top?: ReactNode
  foot?: ReactNode
  className?: string
}) {
  const onDragStart = (e: DragEvent) => writeDrag(e.dataTransfer, { kind: "passage", id: passage.passage_id })
  return (
    <div
      className={cn("grp flex flex-col gap-1.5 p-2.5 text-sm", draggable && "cursor-grab active:cursor-grabbing", className)}
      draggable={draggable}
      onDragStart={draggable ? onDragStart : undefined}
      data-passage={passage.passage_id}
    >
      <div className="flex flex-wrap items-center gap-2">
        <a href={unitHref(passage.unit_id)} className="mono font-semibold hover:underline" title="Ouvrir dans le Coder">
          {passageRef(passage)}
        </a>
        {top}
      </div>
      <q className="leading-snug [quotes:none]">{passage.text}</q>
      <div className="flex flex-wrap items-center gap-2">
        <VerificationLabel state={passage.verification} />
        {foot}
      </div>
    </div>
  )
}
