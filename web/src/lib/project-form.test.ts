import { describe, expect, it } from "vitest"
import {
  addDimension,
  addItem,
  addTemplateQuestion,
  applyFrameworkChoice,
  frameworkChoice,
  frameworkProblem,
  listProblem,
  memoTemplatesProblem,
  moveItem,
  newTemplate,
  projectConfigProblem,
  removeItem,
  removeTemplate,
  renameItem,
  setDefaultTemplate,
} from "./project-form"
import type { Framework, MemoTemplate, ProjectConfig } from "./types"

const tpl = (id: string, def = false, qs = ["Que raconte le répondant ?"]): MemoTemplate => ({
  id,
  name: id === "default" ? "Par défaut" : `Modèle ${id}`,
  default: def,
  version: 1,
  questions: qs.map((text, i) => ({ id: `q${i + 1}`, text })),
})

describe("editable lists", () => {
  it("adds an item with a derived id, and renames without changing the id", () => {
    const a = addItem([], "Mémo de fin d'entretien rédigé")
    expect(a).toEqual([{ id: "memo-de-fin-d-entretien-redige", label: "Mémo de fin d'entretien rédigé" }])
    expect(renameItem(a, a[0].id, "Autre")[0]).toEqual({ id: "memo-de-fin-d-entretien-redige", label: "Autre" })
    expect(addItem(a, "  ")).toBe(a)
    expect(addItem(a, "Mémo de fin d'entretien rédigé")[1].id).toBe("memo-de-fin-d-entretien-redige-2")
  })
  it("removes and moves", () => {
    const l = [{ id: "a", label: "A" }, { id: "b", label: "B" }, { id: "c", label: "C" }]
    expect(removeItem(l, "b").map((i) => i.id)).toEqual(["a", "c"])
    expect(moveItem(l, 0, 2).map((i) => i.id)).toEqual(["b", "c", "a"])
    expect(moveItem(l, 0, 9)).toBe(l)
  })
  it("refuses empty, duplicate and badly named items", () => {
    expect(listProblem("Liste", [{ id: "a", label: " " }])).toMatch(/pas de texte/)
    expect(listProblem("Liste", [{ id: "a", label: "X" }, { id: "b", label: "x" }])).toMatch(/texte en double/)
    expect(listProblem("Liste", [{ id: "a", label: "X" }, { id: "a", label: "Y" }])).toMatch(/identifiant en double/)
    expect(listProblem("Liste", [{ id: "A!", label: "X" }])).toMatch(/identifiant invalide/)
    expect(listProblem("Liens", [], 1)).toMatch(/au moins 1/)
    expect(listProblem("Liste", [{ id: "a", label: "X" }])).toBeNull()
  })
})

describe("memo templates", () => {
  it("needs exactly one default", () => {
    expect(memoTemplatesProblem([tpl("default"), tpl("x")])).toMatch(/par défaut/)
    expect(memoTemplatesProblem([tpl("default", true), tpl("x", true)])).toMatch(/par défaut/)
    expect(memoTemplatesProblem([tpl("default", true), tpl("x")])).toBeNull()
  })
  it("refuses an empty template, an empty question and a duplicate name", () => {
    expect(memoTemplatesProblem([tpl("default", true, [])])).toMatch(/aucune question/)
    expect(memoTemplatesProblem([tpl("default", true, ["  "])])).toMatch(/pas de texte/)
    expect(memoTemplatesProblem([{ ...tpl("default", true) }, { ...tpl("x"), name: "par défaut" }])).toMatch(/Nom de modèle en double/)
    expect(memoTemplatesProblem([])).toMatch(/Au moins un/)
  })
  it("creates, switches the default and protects the default from removal", () => {
    let t = [tpl("default", true)]
    t = newTemplate(t, "Entretien expert")
    expect(t[1]).toMatchObject({ id: "entretien-expert", default: false, questions: [{ id: "q1", text: "" }] })
    t = setDefaultTemplate(t, "entretien-expert")
    expect(t.map((x) => x.default)).toEqual([false, true])
    expect(removeTemplate(t, "entretien-expert")).toBe(t)
    expect(removeTemplate(t, "default")).toHaveLength(1)
  })
  it("numbers a new question after the existing ones", () => {
    const t = tpl("default", true, ["a", "b"])
    expect(addTemplateQuestion(t, "c").questions.map((q) => q.id)).toEqual(["q1", "q2", "q3"])
    expect(addTemplateQuestion({ ...t, questions: [{ id: "q3", text: "x" }] }).questions.map((q) => q.id)).toEqual(["q3", "q4"])
  })
})

describe("framework", () => {
  const toe: Framework = { preset: "toe", dimensions: [{ id: "tech", label: "Technologie", definition: "" }] }
  const presets = { toe, tam_utaut: { preset: "tam_utaut" as const, dimensions: [{ id: "pu", label: "Utilité perçue", definition: "" }] } }
  it("reads none, a preset or custom", () => {
    expect(frameworkChoice(null)).toBe("none")
    expect(frameworkChoice(toe)).toBe("toe")
  })
  it("takes the dimensions of a preset from the server", () => {
    expect(applyFrameworkChoice("toe", null, presets)).toEqual(toe)
    expect(applyFrameworkChoice("tam_utaut", toe, presets)?.dimensions[0].id).toBe("pu")
    expect(applyFrameworkChoice("none", toe, presets)).toBeNull()
  })
  it("keeps typed dimensions when staying on custom, starts empty otherwise", () => {
    const custom: Framework = { preset: "custom", dimensions: [{ id: "x", label: "X", definition: "" }] }
    expect(applyFrameworkChoice("custom", custom, presets)).toEqual(custom)
    expect(applyFrameworkChoice("custom", toe, presets)).toEqual({ preset: "custom", dimensions: [] })
  })
  it("a custom framework needs a dimension with a name", () => {
    expect(frameworkProblem({ preset: "custom", dimensions: [] })).toMatch(/au moins une dimension/)
    expect(frameworkProblem({ preset: "custom", dimensions: [{ id: "x", label: " ", definition: "" }] })).toMatch(/pas de nom/)
    expect(frameworkProblem({ preset: "custom", dimensions: addDimension([], "Qui décide") })).toBeNull()
    expect(frameworkProblem(null)).toBeNull()
  })
})

describe("whole configuration", () => {
  const ok: ProjectConfig = {
    attributes: [],
    memo_templates: [tpl("default", true)],
    checklist: [{ id: "a", label: "A" }],
    theme_checks: [],
    evidence_natures: [],
    link_types: [{ id: "soutient", label: "soutient" }],
    matrix_dimensions: [],
    interpretive_framework: null,
  }
  it("passes a good configuration and names the first problem otherwise", () => {
    expect(projectConfigProblem(ok)).toBeNull()
    expect(projectConfigProblem({ ...ok, link_types: [] })).toMatch(/Types de liens/)
    expect(projectConfigProblem({ ...ok, checklist: [{ id: "a", label: "" }] })).toMatch(/Fin d'entretien/)
  })
})
