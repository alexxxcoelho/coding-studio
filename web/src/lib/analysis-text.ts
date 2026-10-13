// Wording helpers of the Analyser and Rédiger screens.

import type { PassageLink } from "./analysis-api"
import type { Case, ListItem } from "./types"

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n > 1 ? many : one}`
}

/** `ECO-07 U018` for a passage whose unit is `ECO-07-U018`. */
export function passageRef(p: { interview_id: string; unit_id: string }): string {
  const n = p.unit_id.includes("-U") ? p.unit_id.split("-U").pop() : p.unit_id
  return `${p.interview_id} U${n}`
}

/** The outcome of a documented decision in words, whether stored as `adopt` or `adopted`. */
export function outcomeLabel(decision: string | null): string {
  const d = (decision ?? "").toLowerCase()
  if (d.startsWith("adopt")) return "Adopter"
  if (d.startsWith("defer") || d.startsWith("différ")) return "Différer"
  if (d.startsWith("reject") || d.startsWith("refus")) return "Refuser"
  return decision || "Issue non renseignée"
}

/** A decision is documented only by the units attributed to it. */
export function isDocumented(c: Pick<Case, "units">): boolean {
  return c.units.length > 0
}

/** The chip of a link in the text: `ECO-07 U018 soutient`. */
export function linkChipLabel(link: Pick<PassageLink, "passage" | "link_type">, types: ListItem[]): string {
  return `${passageRef(link.passage)} ${types.find((t) => t.id === link.link_type)?.label ?? link.link_type}`
}
