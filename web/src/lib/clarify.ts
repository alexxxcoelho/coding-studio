// Labels to clarify: grouping across interviews, and the unit "is coded" rule.

import type { HumanCoding, ToClarify } from "./types"

export interface LabelGroup {
  /** Normalised key: trimmed, case-insensitive. */
  key: string
  /** Label as first written. */
  label: string
  items: ToClarify[]
  interviews: string[]
}

export function labelKey(label: string): string {
  return label.trim().replace(/\s+/g, " ").toLowerCase()
}

/** Groups open labels by label (case and spacing ignored), largest group
 * first, then alphabetically. Items keep their unit order. */
export function groupByLabel(items: ToClarify[]): LabelGroup[] {
  const map = new Map<string, LabelGroup>()
  for (const it of items) {
    const key = labelKey(it.label)
    const g = map.get(key) ?? { key, label: it.label.trim(), items: [], interviews: [] }
    g.items.push(it)
    if (!g.interviews.includes(it.interview_id)) g.interviews.push(it.interview_id)
    map.set(key, g)
  }
  for (const g of map.values()) g.items.sort((a, b) => a.unit_id.localeCompare(b.unit_id, undefined, { numeric: true }))
  return [...map.values()].sort((a, b) => b.items.length - a.items.length || a.label.localeCompare(b.label))
}

/** Distinct open labels, for autocomplete, most used first. */
export function knownLabels(items: ToClarify[]): string[] {
  return groupByLabel(items.filter((i) => !i.resolved)).map((g) => g.label)
}

/** Phase 2: a unit counts as coded when it has codes or an open label. */
export function isCoded(coding: HumanCoding | null | undefined, toClarify: ToClarify[] = []): boolean {
  return !!coding?.codes.length || toClarify.some((t) => !t.resolved)
}
