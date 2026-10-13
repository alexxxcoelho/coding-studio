// Edits of the familiarisation memo that hold for one interview only
// (docs/API.md, "Per-interview data"): questions added, removed or reworded.
// The template itself is never touched from the Coder screen.

import type { MemoOverrides, MemoView } from "./types"

export const NO_OVERRIDES: MemoOverrides = { added: [], removed: [], reworded: {} }

export function cloneOverrides(o: MemoOverrides): MemoOverrides {
  return { added: o.added.map((a) => ({ ...a })), removed: [...o.removed], reworded: { ...o.reworded } }
}

/** "+ question pour cet entretien": an added question has no id, the server gives it x1, x2... */
export function addQuestion(o: MemoOverrides, text: string): MemoOverrides {
  const t = text.trim()
  if (!t) return o
  return { ...cloneOverrides(o), added: [...o.added, { text: t }] }
}

/**
 * Removes a question for this interview. An added question disappears; a
 * template question is listed as removed (its answer is kept by the server)
 * and its rewording is dropped.
 */
export function removeQuestion(o: MemoOverrides, id: string, source: "template" | "added"): MemoOverrides {
  const next = cloneOverrides(o)
  if (source === "added") {
    next.added = next.added.filter((a) => a.id !== id)
    return next
  }
  if (!next.removed.includes(id)) next.removed.push(id)
  delete next.reworded[id]
  return next
}

/** Brings a removed template question back. */
export function restoreQuestion(o: MemoOverrides, id: string): MemoOverrides {
  return { ...cloneOverrides(o), removed: o.removed.filter((r) => r !== id) }
}

/**
 * Rewords a question for this interview. For a template question, a text equal
 * to the template's (or empty) clears the rewording; for an added one the text
 * is replaced in place.
 */
export function rewordQuestion(o: MemoOverrides, id: string, text: string, source: "template" | "added", templateText?: string): MemoOverrides {
  const next = cloneOverrides(o)
  const t = text.trim()
  if (source === "added") {
    next.added = next.added.map((a) => (a.id === id && t ? { ...a, text: t } : a))
    return next
  }
  if (!t || t === templateText) delete next.reworded[id]
  else next.reworded[id] = t
  return next
}

/** True when the interview's own edits differ from the template. */
export function hasOverrides(o: MemoOverrides): boolean {
  return o.added.length > 0 || o.removed.length > 0 || Object.keys(o.reworded).length > 0
}

/** Number of edits made for this interview (for the Fiche: "2 questions modifiées"). */
export function overrideCount(o: MemoOverrides): number {
  return o.added.length + o.removed.length + Object.keys(o.reworded).length
}

export function answeredCount(view: Pick<MemoView, "questions">): number {
  return view.questions.filter((q) => (q.answer?.text ?? "").trim() !== "").length
}

/** "0 / 4": answered over shown. */
export function memoProgress(view: Pick<MemoView, "questions">): string {
  return `${answeredCount(view)} / ${view.questions.length}`
}

/** The first visit: no answer yet, so the Mémo tab opens first. */
export function isFirstVisit(view: Pick<MemoView, "questions" | "orphan_answers">): boolean {
  return answeredCount(view) === 0 && Object.keys(view.orphan_answers).length === 0
}

/** Answers that changed against the saved memo, as the body of `PUT .../memo/answers`. Unchanged texts are left out. */
export function changedAnswers(view: Pick<MemoView, "questions">, drafts: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {}
  for (const q of view.questions) {
    if (!(q.id in drafts)) continue
    const saved = q.answer?.text ?? ""
    if (drafts[q.id] !== saved) out[q.id] = drafts[q.id]
  }
  return out
}
