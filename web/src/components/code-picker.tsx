// Code picker on shadcn Command: type to filter, Enter toggles, grouped by
// family, only the codes allowed for the unit, NA exclusive.

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
import { familyName, groupByFamily, hotkeyFor, matchesQuery, NA, toggleCode } from "@/lib/codes"
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
}: {
  allowed: Code[]
  value: string[]
  onChange: (next: string[]) => void
  disabled?: boolean
  autoFocus?: boolean
  className?: string
  /** Shows the single-key shortcut of each code (phase 1). */
  phase?: number
}) {
  const key = (code: string) => {
    const k = phase ? hotkeyFor(code, phase) : null
    return k ? (
      <kbd className="text-muted-foreground ml-auto rounded border px-1.5 font-mono text-[0.7rem]" aria-label={`Shortcut ${k}`}>
        {k}
      </kbd>
    ) : null
  }
  const groups = groupByFamily(allowed)
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
        {groups.map(({ family, codes }) => (
          <CommandGroup key={family} heading={familyName(family)}>
            {codes.map((c) => {
              const on = value.includes(c.code)
              return (
                <CommandItem key={c.code} value={c.code} onSelect={() => toggle(c.code)} disabled={disabled} aria-selected={on}>
                  <Check className={cn("size-3.5", on ? "opacity-100" : "opacity-0")} aria-hidden />
                  <CodeChip code={c.code} family={c.family} />
                  <span className="text-muted-foreground truncate text-xs" title={c.definition}>
                    {c.label}
                  </span>
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
