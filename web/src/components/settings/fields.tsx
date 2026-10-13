// Form pieces of the Settings page: labelled fields, native selects, inline
// errors. Errors are role="alert" so that a refusal is read out, never only
// shown as a toast.

import type { ReactNode, SelectHTMLAttributes } from "react"

import { FIELD, FIELD_AREA } from "@/components/ui/field-styles"
import { cn } from "@/lib/utils"

export const NATIVE_SELECT = FIELD
export const TEXTAREA = FIELD_AREA

/** A labelled control. The label wraps the control; the error is outside it,
 * so that it does not become part of the accessible name. */
export function Field({ label, error, children, className }: { label: string; error?: string; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1", className)}>
      <label className="flex min-w-0 flex-col gap-1 text-xs">
        <span className="text-muted-foreground">{label}</span>
        {children}
      </label>
      {error && (
        <p role="alert" className="text-destructive text-xs">
          {error}
        </p>
      )}
    </div>
  )
}

export function NativeSelect(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={cn(NATIVE_SELECT, props.className)} />
}

/** A list of refusals of the same kind, each read out as an alert. */
export function Alerts({ messages }: { messages: string[] }) {
  if (messages.length === 0) return null
  return (
    <div className="flex flex-col gap-1">
      {messages.map((m) => (
        <p key={m} role="alert" className="bg-danger-soft text-destructive rounded-md border border-current/20 px-3 py-2 text-sm">
          {m}
        </p>
      ))}
    </div>
  )
}

/** A chip with a remove button; the text is the value itself. */
export function Chip({ value, onRemove, removeLabel }: { value: string; onRemove: () => void; removeLabel: string }) {
  return (
    <span className="bg-muted inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs">
      <span>{value}</span>
      <button type="button" aria-label={removeLabel} onClick={onRemove} className="text-muted-foreground hover:text-foreground px-0.5">
        ×
      </button>
    </span>
  )
}
