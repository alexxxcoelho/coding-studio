import { describe, expect, it } from "vitest"
import {
  addQuestion,
  answeredCount,
  changedAnswers,
  hasOverrides,
  isFirstVisit,
  memoProgress,
  NO_OVERRIDES,
  overrideCount,
  removeQuestion,
  restoreQuestion,
  rewordQuestion,
} from "./memo-overrides"
import type { MemoOverrides, MemoView } from "./types"

const view = (answers: (string | null)[], orphans: MemoView["orphan_answers"] = {}): Pick<MemoView, "questions" | "orphan_answers"> => ({
  questions: answers.map((a, i) => ({
    id: `q${i + 1}`,
    text: `Question ${i + 1}`,
    source: "template" as const,
    reworded: false,
    answer: a === null ? null : { text: a, at: "2026-10-13T09:00:00Z" },
  })),
  orphan_answers: orphans,
})

describe("memo overrides", () => {
  it("adds a question without id, trimmed, and ignores an empty one", () => {
    expect(addQuestion(NO_OVERRIDES, "  Qui décide de quoi ?  ").added).toEqual([{ text: "Qui décide de quoi ?" }])
    expect(addQuestion(NO_OVERRIDES, "   ")).toBe(NO_OVERRIDES)
  })
  it("does not mutate its input", () => {
    const o: MemoOverrides = { added: [], removed: [], reworded: {} }
    addQuestion(o, "x")
    removeQuestion(o, "q1", "template")
    expect(o).toEqual({ added: [], removed: [], reworded: {} })
  })
  it("removes a template question and drops its rewording", () => {
    const o = rewordQuestion(NO_OVERRIDES, "q2", "Autre texte", "template", "Question 2")
    const r = removeQuestion(o, "q2", "template")
    expect(r.removed).toEqual(["q2"])
    expect(r.reworded).toEqual({})
    expect(removeQuestion(r, "q2", "template").removed).toEqual(["q2"])
  })
  it("removes an added question outright", () => {
    const o: MemoOverrides = { added: [{ id: "x1", text: "A" }, { id: "x2", text: "B" }], removed: [], reworded: {} }
    expect(removeQuestion(o, "x1", "added").added).toEqual([{ id: "x2", text: "B" }])
    expect(removeQuestion(o, "x1", "added").removed).toEqual([])
  })
  it("restores a removed question", () => {
    expect(restoreQuestion({ added: [], removed: ["q1", "q2"], reworded: {} }, "q1").removed).toEqual(["q2"])
  })
  it("rewords a template question and clears the rewording when the text returns to the template's", () => {
    const o = rewordQuestion(NO_OVERRIDES, "q1", "  Que raconte-t-il ?  ", "template", "Que raconte le répondant ?")
    expect(o.reworded).toEqual({ q1: "Que raconte-t-il ?" })
    expect(rewordQuestion(o, "q1", "Que raconte le répondant ?", "template", "Que raconte le répondant ?").reworded).toEqual({})
    expect(rewordQuestion(o, "q1", "", "template", "x").reworded).toEqual({})
  })
  it("rewords an added question in place", () => {
    const o: MemoOverrides = { added: [{ id: "x1", text: "A" }], removed: [], reworded: {} }
    expect(rewordQuestion(o, "x1", "A2", "added").added).toEqual([{ id: "x1", text: "A2" }])
  })
  it("counts the interview's own edits", () => {
    expect(hasOverrides(NO_OVERRIDES)).toBe(false)
    const o: MemoOverrides = { added: [{ text: "a" }], removed: ["q1"], reworded: { q2: "t" } }
    expect(hasOverrides(o)).toBe(true)
    expect(overrideCount(o)).toBe(3)
  })
})

describe("memo answers", () => {
  it("counts non blank answers", () => {
    const v = view(["Un texte", "  ", null, "Autre"])
    expect(answeredCount(v)).toBe(2)
    expect(memoProgress(v)).toBe("2 / 4")
  })
  it("is a first visit when nothing is answered, even orphans count as answered", () => {
    expect(isFirstVisit(view([null, null, null, null]))).toBe(true)
    expect(isFirstVisit(view([null, "x"]))).toBe(false)
    expect(isFirstVisit(view([null], { q9: { text: "ancienne", at: "" } }))).toBe(false)
  })
  it("sends only the answers that changed, clearing with an empty text", () => {
    const v = view(["Ancien", null, "Gardé"])
    expect(changedAnswers(v, { q1: "Nouveau", q2: "", q3: "Gardé" })).toEqual({ q1: "Nouveau" })
    expect(changedAnswers(v, { q1: "", q2: "Réponse" })).toEqual({ q1: "", q2: "Réponse" })
    expect(changedAnswers(v, {})).toEqual({})
  })
})
