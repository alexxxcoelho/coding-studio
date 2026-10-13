// Wording helpers of the Analyser and Rédiger screens.

import type { PassageLink } from "./analysis-api"
import type { Case, ListItem } from "./types"
import { t } from "@/i18n"

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n > 1 ? many : one}`
}

/** `ECO-07 U018` for a passage whose unit is `ECO-07-U018`. */
export function passageRef(p: { interview_id: string; unit_id: string }): string {
  const n = p.unit_id.includes("-U") ? p.unit_id.split("-U").pop() : p.unit_id
  return `${p.interview_id} U${n}`
}

/** The outcome of a documented decision in words (`adopt`, `defer`, `reject`; the earlier `adopted` forms are understood). */
export function outcomeLabel(outcome: string | null): string {
  const d = (outcome ?? "").toLowerCase()
  if (d.startsWith("adopt")) return t("analysis_text.adopter")
  if (d.startsWith("defer") || d.startsWith("différ")) return t("common.differer")
  if (d.startsWith("reject") || d.startsWith("refus")) return t("analysis_text.refuser")
  return outcome || t("analysis_text.issue_non_renseignee")
}

/** A decision is documented only by the units attributed to it. */
export function isDocumented(c: Pick<Case, "units"> & { passages?: string[] }): boolean {
  return c.units.length > 0 || (c.passages?.length ?? 0) > 0
}

/** The chip of a link in the text: `ECO-07 U018 soutient`. */
export function linkChipLabel(link: Pick<PassageLink, "passage" | "link_type">, types: ListItem[]): string {
  return `${passageRef(link.passage)} ${types.find((t) => t.id === link.link_type)?.label ?? link.link_type}`
}
