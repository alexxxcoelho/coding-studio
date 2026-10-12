// Code picker on shadcn Command: type to filter, Enter toggles, NA exclusive.
// Phase 1: the five pre-classification categories. Phase 2: every concrete
// code, grouped by TOE attachment with the unit's phase 1 contexts first.

import { Check } from "lucide-react"

import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { CodeChip } from "@/components/bits"
import { familyName, groupByFamily, groupByToe, hotkeyFor, matchesQuery, NA, toggleCode } from "@/lib/codes"
import { cn } from "@/lib/utils"
import type { Code } from "@/lib/types"

export function CodePicker({
  allowed,
  value,
  onChange,
  disabled,
  autoFocus,
  className,
  phase,
  phase1Contexts,
}: {
  allowed: Code[]
  value: string[]
  onChange: (next: string[]) => void
  disabled?: boolean
  autoFocus?: boolean
  className?: string
  /** Shows the single-key shortcut of each code (phase 1). */
  phase?: number
  /** Phase 2: the unit's phase 1 contexts, listed first. Enables TOE grouping. */
  phase1Contexts?: string[] | null
}) {
  const key = (code: string) => {
    const k = phase ? hotkeyFor(code, phase) : null
    return k ? (
      <kbd className="text-muted-foreground ml-auto rounded border px-1.5 font-mono text-[0.7rem]" aria-label={`Shortcut ${k}`}>
        {k}
      </kbd>
    ) : null
  }
  const toeMode = phase === 2 || phase1Contexts !== undefined
  const groups = toeMode
    ? groupByToe(allowed, phase1Contexts ?? []).map((g) => ({
        id: g.key,
        heading: g.highlighted ? `${g.title}, pre-classified` : g.title,
        highlighted: g.highlighted,
        codes: g.codes,
      }))
    : groupByFamily(allowed).map((g) => ({ id: g.family, heading: familyName(g.family), highlighted: false, codes: g.codes }))
  const toggle = (code: string) => {
    if (!disabled) onChange(toggleCode(value, code))
  }
  return (
    <Command
      className={cn("rounded-md border", className)}
      // Ctrl/Cmd+Enter saves the unit; keep cmdk from reading it as a toggle.
      onKeyDown={(e) => {
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) e.preventDefault()
      }}
      filter={(itemValue, search) => {
        if (itemValue === NA) return NA.toLowerCase().includes(search.toLowerCase()) || "not applicable".includes(search.toLowerCase()) ? 1 : 0
        const code = allowed.find((c) => c.code === itemValue)
        return code && matchesQuery(code, search) ? 1 : 0
      }}
    >
      <CommandInput placeholder="Type to filter codes, Enter to toggle" autoFocus={autoFocus} disabled={disabled} />
      <CommandList className="max-h-[22rem]">
        <CommandEmpty>No allowed code matches.</CommandEmpty>
        {groups.map(({ id, heading, highlighted, codes }) => (
          <CommandGroup key={id} heading={heading} className={cn(highlighted && "bg-accent/40 [&_[cmdk-group-heading]]:text-foreground")}>
            {codes.map((c) => {
              const on = value.includes(c.code)
              const rqs = c.research_questions ?? []
              const tip = [c.definition, rqs.length ? `Research questions: ${rqs.join(", ")}` : ""].filter(Boolean).join("\n")
              return (
                <CommandItem key={c.code} value={c.code} onSelect={() => toggle(c.code)} disabled={disabled} aria-selected={on} title={tip}>
                  <Check className={cn("size-3.5", on ? "opacity-100" : "opacity-0")} aria-hidden />
                  <CodeChip code={c.code} family={c.family} />
                  <span className="text-muted-foreground truncate text-xs">{c.label}</span>
                  {toeMode && c.origin === "field" && (
                    <span className="text-inductive ml-auto shrink-0 rounded border border-current/30 px-1 text-[0.65rem]" title="Code created from the field">
                      field
                    </span>
                  )}
                  {key(c.code)}
                </CommandItem>
              )
            })}
          </CommandGroup>
        ))}
        <CommandGroup heading="No code applies">
          <CommandItem value={NA} onSelect={() => toggle(NA)} disabled={disabled}>
            <Check className={cn("size-3.5", value.includes(NA) ? "opacity-100" : "opacity-0")} aria-hidden />
            <CodeChip code={NA} family="NA" />
            <span className="text-muted-foreground text-xs">Not applicable, clears the other codes</span>
            {key(NA)}
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </Command>
  )
}
