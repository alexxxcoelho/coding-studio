// Editors of "Modèles et listes de l'étude": validation that mirrors
// `ProjectConfig::validate` (crates/cs-model/src/project.rs) so that a bad edit
// is refused before the PUT, with the server's rules in the study's words.

import { slugId } from "./attributes"
import type { Framework, FrameworkDimension, ListItem, MemoTemplate, ProjectConfig } from "./types"

const ID_PATTERN = /^[a-z0-9][a-z0-9_-]*$/

/** First problem of an editable list, or null. `min` is the fewest items allowed. */
export function listProblem(name: string, items: ListItem[], min = 0): string | null {
  if (items.length < min) return `${name} : au moins ${min} élément${min > 1 ? "s" : ""}`
  const ids = new Set<string>()
  const labels = new Set<string>()
  for (const it of items) {
    const label = it.label.trim()
    if (!label) return `${name} : un élément n'a pas de texte`
    if (!ID_PATTERN.test(it.id)) return `${name} : identifiant invalide pour « ${label} »`
    if (ids.has(it.id)) return `${name} : identifiant en double (${it.id})`
    if (labels.has(label.toLowerCase())) return `${name} : texte en double (${label})`
    ids.add(it.id)
    labels.add(label.toLowerCase())
  }
  return null
}

/** Appends an item with an id derived from its label. */
export function addItem(items: ListItem[], label: string): ListItem[] {
  const l = label.trim()
  if (!l) return items
  return [...items, { id: slugId(l, items.map((i) => i.id)), label: l }]
}

/** Rewords an item, keeping its id (ids are stable references). */
export function renameItem(items: ListItem[], id: string, label: string): ListItem[] {
  return items.map((i) => (i.id === id ? { ...i, label } : i))
}

export function removeItem(items: ListItem[], id: string): ListItem[] {
  return items.filter((i) => i.id !== id)
}

export function moveItem<T>(items: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= items.length || to >= items.length) return items
  const next = [...items]
  const [x] = next.splice(from, 1)
  next.splice(to, 0, x)
  return next
}

// ------------------------------------------------------------ memo templates

export function memoTemplatesProblem(templates: MemoTemplate[]): string | null {
  if (templates.length === 0) return "Au moins un modèle de mémo est nécessaire"
  if (templates.filter((t) => t.default).length !== 1) return "Un seul modèle doit être le modèle par défaut"
  const ids = new Set<string>()
  const names = new Set<string>()
  for (const t of templates) {
    const name = t.name.trim()
    if (!name) return "Un modèle de mémo n'a pas de nom"
    if (!ID_PATTERN.test(t.id)) return `Identifiant invalide pour le modèle « ${name} »`
    if (ids.has(t.id)) return `Identifiant de modèle en double (${t.id})`
    if (names.has(name.toLowerCase())) return `Nom de modèle en double (${name})`
    ids.add(t.id)
    names.add(name.toLowerCase())
    if (t.questions.length === 0) return `Le modèle « ${name} » n'a aucune question`
    const q = listProblem(`Modèle « ${name} »`, t.questions.map((x) => ({ id: x.id, label: x.text })))
    if (q) return q
  }
  return null
}

export function newTemplate(templates: MemoTemplate[], name: string): MemoTemplate[] {
  const n = name.trim()
  if (!n) return templates
  return [
    ...templates,
    { id: slugId(n, templates.map((t) => t.id)), name: n, default: false, version: 1, questions: [{ id: "q1", text: "" }] },
  ]
}

export function setDefaultTemplate(templates: MemoTemplate[], id: string): MemoTemplate[] {
  return templates.map((t) => ({ ...t, default: t.id === id }))
}

/** Removes a template; the default cannot be removed (choose another default first). */
export function removeTemplate(templates: MemoTemplate[], id: string): MemoTemplate[] {
  const t = templates.find((x) => x.id === id)
  if (!t || t.default) return templates
  return templates.filter((x) => x.id !== id)
}

export function addTemplateQuestion(t: MemoTemplate, text = ""): MemoTemplate {
  const ids = t.questions.map((q) => q.id)
  const nums = ids.map((i) => /^q(\d+)$/.exec(i)?.[1]).filter((x): x is string => x !== undefined).map(Number)
  const n = Math.max(0, ...nums) + 1
  return { ...t, questions: [...t.questions, { id: `q${n}`, text }] }
}

// ----------------------------------------------------------------- framework

export type FrameworkChoice = "none" | "toe" | "tam_utaut" | "custom"

export function frameworkChoice(f: Framework | null): FrameworkChoice {
  return f ? f.preset : "none"
}

/** Framework after choosing in the select: a preset takes its dimensions from the server's presets, custom keeps what was typed. */
export function applyFrameworkChoice(
  choice: FrameworkChoice,
  current: Framework | null,
  presets: { toe: Framework; tam_utaut: Framework } | undefined,
): Framework | null {
  if (choice === "none") return null
  if (choice === "custom") return { preset: "custom", dimensions: current?.preset === "custom" ? current.dimensions : [] }
  const p = presets?.[choice]
  return { preset: choice, dimensions: p ? p.dimensions.map((d) => ({ ...d })) : [] }
}

export function frameworkProblem(f: Framework | null): string | null {
  if (!f) return null
  if (f.preset === "custom" && f.dimensions.length === 0) return "Un cadre personnalisé a besoin d'au moins une dimension"
  const ids = new Set<string>()
  for (const d of f.dimensions) {
    if (!d.label.trim()) return "Une dimension du cadre n'a pas de nom"
    if (!ID_PATTERN.test(d.id) || ids.has(d.id)) return `Identifiant de dimension invalide ou en double (${d.id})`
    ids.add(d.id)
  }
  return null
}

export function addDimension(dims: FrameworkDimension[], label: string): FrameworkDimension[] {
  const l = label.trim()
  if (!l) return dims
  return [...dims, { id: slugId(l, dims.map((d) => d.id)), label: l, definition: "" }]
}

// -------------------------------------------------------------------- whole

/** First problem of the whole configuration, in reading order, or null. */
export function projectConfigProblem(cfg: ProjectConfig): string | null {
  return (
    memoTemplatesProblem(cfg.memo_templates) ??
    listProblem("Fin d'entretien", cfg.checklist) ??
    listProblem("Contrôles d'un thème", cfg.theme_checks) ??
    listProblem("Natures de preuve", cfg.evidence_natures) ??
    listProblem("Types de liens", cfg.link_types, 1) ??
    listProblem("Dimensions de la matrice", cfg.matrix_dimensions) ??
    frameworkProblem(cfg.interpretive_framework)
  )
}
