import { t } from "@/i18n"
import type { Code, ReviewAnswer } from "@/lib/types"

// Escape metadata; quote transcript lines without changing their wording.
const inline = (s: string) => s.replace(/\r?\n/g, " ").replace(/([\\`*_{}[\]()<>#+.!|~])/g, "\\$1")

export function disagreementMarkdown(id: string, review: ReviewAnswer, book: Code[] = []): string {
  const names = new Map(book.map(c => [c.code, c.label]))
  const codes = (values: string[]) => values.length
    ? values.map(code => inline(names.get(code) ? `${names.get(code)} (${code})` : code)).join("; ")
    : t("review_export.no_codes")
  const items = review.items.filter(i => i.kind === "disagreement")
    .slice().sort((a, b) => a.unit.seq - b.unit.seq || a.unit_id.localeCompare(b.unit_id))
  const lines = [`# ${t("review_export.title")} — ${inline(id)}`, "",
    `${t("review_export.total")}: ${items.length}`, "", t("review_export.reference"), ""]
  for (const item of items) {
    lines.push(`## ${inline(item.unit_id)}`, "",
      `${t("review_export.status")}: ${item.decision ? t("review_export.decided") : t("review_export.pending")}`, "",
      `${t("review_export.speaker")}: ${inline(item.unit.speaker_raw)}${item.unit.timestamp ? ` · ${inline(item.unit.timestamp)}` : ""}`, "",
      `### ${t("review_export.excerpt")}`, "",
      ...item.unit.text.split(/\r?\n/).map(line => `> ${line}`), "",
      `### ${t("review_export.researcher")}`, "", codes(item.human), "")
    if (item.revised_since_snapshot) lines.push(`**${t("review_export.current")}**`, "", codes(item.human_current), "")
    const runs = [...new Set([...review.runs, ...Object.keys(item.ai)])]
    for (const run of runs) lines.push(`### ${t("review_export.ai")} — ${inline(run)}`, "",
      Object.hasOwn(item.ai, run) ? codes(item.ai[run]) : t("review_export.missing"), "")
    if (item.decision) {
      lines.push(`### ${t("review_export.final")}`, "", codes(item.decision.final_codes), "")
      if (item.decision.rationale) lines.push(...item.decision.rationale.split(/\r?\n/).map(line => `> ${line}`), "")
    }
  }
  if (!items.length) lines.push(t("review_export.empty"), "")
  return lines.join("\n")
}

export function disagreementFilename(id: string): string {
  return `${id.replace(/[^a-zA-Z0-9_-]/g, "_") || "interview"}-disagreements.md`
}
