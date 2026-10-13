// Logic of the Revoir screen (docs/guide/fr/03_revoir.md, SPEC F-63 to F-73):
// the decision draft of one card, its keyboard, the queue, the states of the
// runs and the candidate concepts. Pure functions; the screen only draws them.

import { sameSet } from "./codes"
import type { KeyLike } from "./coder-keys"
import type { AiRun, ArbitrationChoice, PromptPart, ReviewItem, Unit, ValidationIssue } from "./types"
import type { ReviewState } from "./review-api"
import { t } from "@/i18n"

// ---------------------------------------------------------------- decisions

/** What the researcher picks on a card. `merge` and `other` both go to the server as a custom choice. */
export type DecisionKind = "human" | "ai" | "merge" | "other"

export interface DecisionDraft {
  kind: DecisionKind
  /** The run read by "prendre celui du modèle" and by "fusionner". */
  runLabel: string | null
  /** Codes picked for "autre code". */
  custom: string[]
  rationale: string
}

/** The choices of a card, in the order of the keys 1 to 4. A sampled agreement is confirmed or corrected. */
export function availableChoices(item: ReviewItem): DecisionKind[] {
  return item.kind === "disagreement" ? ["human", "ai", "merge", "other"] : ["human", "other"]
}

export function newDraft(item: ReviewItem, runs: string[]): DecisionDraft {
  return { kind: "human", runLabel: runs[0] ?? null, custom: item.human, rationale: "" }
}

const union = (a: string[], b: string[]) => [...new Set([...a, ...b])]

/** The draft that reproduces a stored decision, for editing it; null when there is none. */
export function draftFromDecision(item: ReviewItem, runs: string[]): DecisionDraft | null {
  const d = item.decision
  if (!d) return null
  const base = { runLabel: runs[0] ?? null, rationale: d.rationale }
  if (d.choice.type === "human") return { ...base, kind: "human", custom: item.human }
  if (d.choice.type === "ai") return { ...base, kind: "ai", runLabel: d.choice.run_label, custom: d.final_codes }
  const merged = runs.find((r) => sameSet(union(item.human, item.ai[r] ?? []), d.final_codes))
  if (merged) return { ...base, kind: "merge", runLabel: merged, custom: d.final_codes }
  return { ...base, kind: "other", custom: d.final_codes }
}

export function finalCodesOf(item: ReviewItem, draft: DecisionDraft): string[] {
  const ai = draft.runLabel ? (item.ai[draft.runLabel] ?? []) : []
  switch (draft.kind) {
    case "human":
      return item.human
    case "ai":
      return ai
    case "merge":
      return union(item.human, ai)
    case "other":
      return draft.custom
  }
}

export function toChoice(draft: DecisionDraft): ArbitrationChoice {
  if (draft.kind === "human") return { type: "human" }
  if (draft.kind === "ai") return { type: "ai", run_label: draft.runLabel ?? "" }
  return { type: "custom" }
}

/**
 * A reason is written for every disagreement (the server refuses it otherwise)
 * and for any final reading that departs from the frozen human one. Confirming
 * a sampled agreement needs none.
 */
export function rationaleNeeded(item: ReviewItem, draft: DecisionDraft): boolean {
  return item.kind === "disagreement" || !sameSet(finalCodesOf(item, draft), item.human)
}

export type DraftProblem = "no_codes" | "rationale" | null

export function draftProblem(item: ReviewItem, draft: DecisionDraft): DraftProblem {
  if (finalCodesOf(item, draft).length === 0) return "no_codes"
  if (rationaleNeeded(item, draft) && draft.rationale.trim() === "") return "rationale"
  return null
}

export const PROBLEM_TEXT: Record<Exclude<DraftProblem, null>, string> = {
  get no_codes() { return t("review_model.choisissez_au_moins_un_code") },
  get rationale() { return t("review_model.une_justification_est_obligatoire_qu") },
}

// --------------------------------------------------------------- comparison

export type Agreement = "both" | "human_only" | "ai_only" | "mixed"

export interface CompareRow {
  code: string
  human: boolean
  ai: Record<string, boolean>
  status: Agreement
}

export const AGREEMENT_LABEL: Record<Agreement, string> = {
  both: "accord",
  get human_only() { return t("review_model.vous_seul") },
  get ai_only() { return t("review_model.modele_seul") },
  get mixed() { return t("review_model.selon_le_run") },
}

/** One row per code carried by the frozen human reading or by a run; the codes of the human reading come first. */
export function compareRows(item: ReviewItem, runs: string[]): CompareRow[] {
  const codes = [...new Set([...item.human, ...runs.flatMap((r) => item.ai[r] ?? [])])]
  return codes.map((code) => {
    const human = item.human.includes(code)
    const ai = Object.fromEntries(runs.map((r) => [r, (item.ai[r] ?? []).includes(code)]))
    const flags = Object.values(ai)
    const all = flags.length > 0 && flags.every(Boolean)
    const none = flags.every((f) => !f)
    const status: Agreement = human && all ? "both" : human && none ? "human_only" : !human && all ? "ai_only" : "mixed"
    return { code, human, ai, status }
  })
}

// ----------------------------------------------------------------- keyboard

export type DecisionKeyAction = { type: "choose"; kind: DecisionKind } | { type: "save" } | { type: "next" } | { type: "prev" }

export interface DecisionKeyContext {
  /** Focus is in a text field, a select or a text area. */
  typing: boolean
  /** The choices of the card, in key order. */
  kinds: DecisionKind[]
  /** A dialog or a menu owns the keyboard. */
  modalOpen?: boolean
}

/**
 * Keys of a card: 1 to 4 choose, Enter saves, the arrows move between cards.
 * In a field, only Ctrl or Cmd with Enter acts, so that the rationale can be typed.
 */
export function decisionKey(e: KeyLike, ctx: DecisionKeyContext): DecisionKeyAction | null {
  if (e.isComposing || e.defaultPrevented || ctx.modalOpen) return null
  const mod = !!(e.metaKey || e.ctrlKey)
  if (e.altKey) return null
  if (e.key === "Enter") {
    if (e.shiftKey) return null
    return ctx.typing && !mod ? null : { type: "save" }
  }
  if (mod || ctx.typing || e.shiftKey) return null
  if (e.key === "ArrowDown") return { type: "next" }
  if (e.key === "ArrowUp") return { type: "prev" }
  if (/^[1-9]$/.test(e.key)) {
    const kind = ctx.kinds[Number(e.key) - 1]
    return kind ? { type: "choose", kind } : null
  }
  return null
}

// -------------------------------------------------------------------- queue

export type QueueFilter = "remaining" | "decided" | "all"

const unitOrder = (a: ReviewItem, b: ReviewItem) => a.unit_id.localeCompare(b.unit_id, undefined, { numeric: true })

/** Disagreements first, then the sampled agreements, each in unit order; narrowed to the filter. */
export function filterQueue(items: ReviewItem[], filter: QueueFilter): ReviewItem[] {
  const keep = items.filter((i) => (filter === "all" ? true : filter === "decided" ? !!i.decision : !i.decision))
  return [...keep.filter((i) => i.kind === "disagreement").sort(unitOrder), ...keep.filter((i) => i.kind !== "disagreement").sort(unitOrder)]
}

/** The unit after (1) or before (-1) `current` in the queue; the first card when `current` left the queue. */
export function nextInQueue(queue: ReviewItem[], current: string | null, dir: 1 | -1): string | null {
  if (queue.length === 0) return null
  const at = queue.findIndex((i) => i.unit_id === current)
  if (at < 0) return queue[0].unit_id
  return queue[at + dir]?.unit_id ?? null
}

/** The interviewer turn right before a unit, as context. Interviewer turns are context only and never codable. */
export function precedingQuestion(units: Unit[], unitId: string): Unit | null {
  const at = units.findIndex((u) => u.unit_id === unitId)
  if (at <= 0) return null
  const before = units[at - 1]
  return before.role === "interviewer" ? before : null
}

// --------------------------------------------------------------------- runs

export type RunStateKind = "waiting" | "partial" | "complete"

function partsText(n: number, of: number | null): string {
  return of ? t("review_model.n_parties_sur_of", { n, of }) : `${n} ${n < 2 ? t("review_model.partie_recue") : t("review_model.parties_recues")}`
}

export function runState(run: AiRun): { state: RunStateKind; label: string } {
  if (run.complete) return { state: "complete", label: "complet" }
  const n = run.parts_received.length
  if (n === 0) return { state: "waiting", label: t("review_model.en_attente_de_reponses") }
  return { state: "partial", label: `partiel, ${partsText(n, run.parts_expected)}` }
}

/** The instant now, as the ISO text the server stores. */
export function isoNow(): string {
  return new Date().toISOString()
}

/** "13.10 09:40", local time. */
export function shortStamp(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  const p = (x: number) => String(x).padStart(2, "0")
  return `${p(d.getDate())}.${p(d.getMonth() + 1)} ${p(d.getHours())}:${p(d.getMinutes())}`
}

/** "R1, 13.10 09:40, modèle X, 2 parties sur 2, complet" (guide, action 5). */
export function runLine(run: AiRun): string {
  const model = run.meta.model_name.trim() || t("review_model.non_nomme")
  return t("review_model.run_label_shortstamp_modele_model", { run_label: run.meta.run_label, shortStamp: shortStamp(run.meta.executed_at), model, partsText: partsText(run.parts_received.length, run.parts_expected), label: runState(run).label })
}

/** State of the review for the toolbar: "pas de revue", "revue IA en attente", "revue IA faite, 9 sur 12 décidées". */
export function reviewStatusLabel(runs: AiRun[], decisions: { decided: number; total: number } | null): string {
  if (runs.length === 0) return t("review_model.pas_de_revue")
  if (!runs.some((r) => r.complete) || !decisions) return t("review_model.revue_ia_en_attente")
  if (decisions.total === 0) return t("review_model.revue_ia_faite_aucun_desaccord")
  return t("review_model.revue_ia_faite_decided_sur", { n: decisions.total, decided: decisions.decided })
}

/** The same sentence from the review state the server computes: "pas de revue", "revue IA en attente",
 * "revue IA en cours, 2 sur 3 décidées", "revue IA faite, 3 sur 3 décidées". The counts of concepts to sort
 * and of units to harmonise are in the badges of the toolbar, not in this line. */
export function reviewStateLabel(s: ReviewState): string {
  if (s.state === "not_done") return t("review_model.pas_de_revue")
  if (s.runs.length === 0 && s.n_covered === 0) return t("review_model.revue_ia_en_attente")
  const head = s.state === "complete" ? "revue IA faite" : t("review_model.revue_ia_en_cours")
  if (s.n_disagreements === 0) return t("review_model.head_aucun_desaccord", { head })
  return t("review_model.head_n_decided_sur_n", { n: s.n_disagreements, head, n_decided: s.n_decided })
}

/** Parts of the latest prompt batch (each generation is a new batch). */
export function latestSet(parts: PromptPart[]): PromptPart[] {
  if (!parts.length) return []
  const last = parts[parts.length - 1]
  if (last.batch_id !== undefined) return parts.filter((p) => p.batch_id === last.batch_id)
  return parts.slice(-last.of).filter((p) => p.of === last.of && p.sha256)
}

/** "Partie 1 sur 2". */
export function partTitle(p: Pick<PromptPart, "part" | "of">): string {
  return t("review_model.part_sur_of", { part: p.part, of: p.of })
}

/** The refusal of an answer: a title and one line per problem, all reported together. */
export function issueLines(issues: ValidationIssue[]): { title: string; items: ValidationIssue[] } {
  return { title: t("review_model.reponse_refusee_items_rien_n", { items: t("review_model.n_problemes", { n: issues.length }) }), items: issues }
}

// --------------------------------------------------------------- candidates

/** What the researcher does with a concept the model found outside the codebook. */
export type CandidateAction = { kind: "create"; name: string } | { kind: "map"; code: string } | { kind: "reject" }

export type CandidateProblem = "name" | "code" | "rationale" | null

/** The decision is motivated like an arbitration; the target is checked first. */
export function candidateProblem(action: CandidateAction, rationale: string): CandidateProblem {
  if (action.kind === "create" && action.name.trim() === "") return "name"
  if (action.kind === "map" && action.code.trim() === "") return "code"
  if (rationale.trim() === "") return "rationale"
  return null
}

export const CANDIDATE_PROBLEM_TEXT: Record<Exclude<CandidateProblem, null>, string> = {
  name: "Donnez un nom au nouveau code.",
  get code() { return t("review_model.choisissez_le_code_auquel_rattacher") },
  get rationale() { return t("review_model.une_justification_est_obligatoire_el") },
}
