// The tabs of Analyser and Rédiger, as a capsule of the toolbar.

import { Capsule, CapsuleItem } from "@/components/toolbar"

export function TabBar<T extends string>({ tabs, value, onChange, label }: { tabs: { id: T; label: string }[]; value: T; onChange: (id: T) => void; label: string }) {
  return (
    <Capsule label={label} role="tablist">
      {tabs.map((t) => (
        <CapsuleItem key={t.id} role="tab" aria-selected={t.id === value} aria-pressed={undefined} on={t.id === value} onClick={() => onChange(t.id)}>
          {t.label}
        </CapsuleItem>
      ))}
    </Capsule>
  )
}
