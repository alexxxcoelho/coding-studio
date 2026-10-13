// "Modèles et listes de l'étude", research questions and the project template
// of the Paramètres de l'étude page.

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { nextQuestionId } from "@/components/settings/research-questions"
import { SettingsPage } from "@/pages/settings"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { ProjectConfig, StudyConfig } from "@/lib/types"

const STUDY: StudyConfig = {
  framework: "none",
  phase1_categories: [],
  id_prefixes: ["INT"],
  interviewers: [],
  max_words: 150,
  prompt_template: { version: "v1", header: "", codebook_intro: "", units_intro: "", footer: "" },
  study_name: "Pilote",
  description: "",
  coding_rules: "",
}
const PROJECT: ProjectConfig = {
  attributes: [{ id: "sector", label: "Secteur", kind: "text", choices: [], shared_with_model: false }],
  memo_templates: [{ id: "default", name: "Par défaut", default: true, version: 1, questions: [{ id: "q1", text: "Première impression ?" }] }],
  checklist: [{ id: "memo", label: "Mémo écrit" }],
  theme_checks: [],
  evidence_natures: [],
  link_types: [{ id: "causes", label: "cause" }],
  matrix_dimensions: [],
  interpretive_framework: null,
}
const TOE_DIMS = [{ id: "tech", label: "Technologie", definition: "" }]

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/config", { body: STUDY })
  mock.on("GET", "/api/state", { body: { native_dialogs: false, codebook_size: 0 } })
  mock.on("GET", "/api/research-questions", { body: [{ id: "SQ1", text: "Pourquoi ?", ord: 1 }] })
  mock.on("GET", "/api/project-config", { body: PROJECT })
  mock.on("GET", "/api/project-config/presets", {
    body: { frameworks: { toe: { preset: "toe", dimensions: TOE_DIMS }, tam_utaut: { preset: "tam_utaut", dimensions: [] } }, defaults: {} },
  })
})
afterEach(() => cleanup())

async function lists() {
  renderWithClient(<SettingsPage />)
  const region = await screen.findByRole("region", { name: "Modèles et listes de l'étude" })
  await within(region).findByText("Par défaut, 1 question")
  return region
}
const putProject = () => mock.to("PUT", "/api/project-config")

describe("nextQuestionId", () => {
  it("follows the highest number", () => {
    expect(nextQuestionId([])).toBe("SQ1")
    expect(nextQuestionId([{ id: "SQ1", text: "", ord: 1 }, { id: "SQ4", text: "", ord: 2 }])).toBe("SQ5")
  })
})

describe("modèles et listes", () => {
  it("summarises the templates and the attributes", async () => {
    const r = await lists()
    expect(within(r).getByText("Secteur (texte)")).toBeInTheDocument()
    expect(within(r).getByLabelText("Cadre d'interprétation")).toHaveValue("none")
    expect(within(r).getByRole("button", { name: "Enregistrer les modèles et listes" })).toBeDisabled()
  })

  it("edits a list and saves the whole configuration with one PUT", async () => {
    const r = await lists()
    mock.on("PUT", "/api/project-config", (call) => ({ body: call.body }))
    fireEvent.click(within(r).getByRole("button", { name: "Modifier : Fin d'entretien" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.change(within(dialog).getByLabelText("Nouvel élément"), { target: { value: "Passages à revoir traités" } })
    fireEvent.click(within(dialog).getByRole("button", { name: "Ajouter" }))
    fireEvent.click(within(dialog).getByRole("button", { name: "Terminé" }))
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer les modèles et listes" }))
    await waitFor(() => expect(putProject()).toHaveLength(1))
    expect((putProject()[0].body as ProjectConfig).checklist.map((i) => i.label)).toEqual(["Mémo écrit", "Passages à revoir traités"])
  })

  it("refuses a list with no link type before calling the server", async () => {
    const r = await lists()
    fireEvent.click(within(r).getByRole("button", { name: "Modifier : Types de liens" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.click(within(dialog).getByRole("button", { name: "Retirer cause" }))
    fireEvent.click(within(dialog).getByRole("button", { name: "Terminé" }))
    expect(await within(r).findByRole("alert")).toHaveTextContent("Types de liens")
    expect(within(r).getByRole("button", { name: "Enregistrer les modèles et listes" })).toBeDisabled()
  })

  it("takes the dimensions of a preset framework from the server", async () => {
    const r = await lists()
    mock.on("PUT", "/api/project-config", (call) => ({ body: call.body }))
    fireEvent.change(within(r).getByLabelText("Cadre d'interprétation"), { target: { value: "toe" } })
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer les modèles et listes" }))
    await waitFor(() => expect(putProject()).toHaveLength(1))
    expect((putProject()[0].body as ProjectConfig).interpretive_framework).toEqual({ preset: "toe", dimensions: TOE_DIMS })
  })

  it("asks for at least one dimension of a custom framework", async () => {
    const r = await lists()
    fireEvent.change(within(r).getByLabelText("Cadre d'interprétation"), { target: { value: "custom" } })
    expect(await within(r).findByText("Un cadre personnalisé a besoin d'au moins une dimension")).toBeInTheDocument()
    fireEvent.change(within(r).getByLabelText("Nouvelle dimension"), { target: { value: "Qui décide" } })
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter une dimension" }))
    expect(within(r).getByRole("button", { name: "Enregistrer les modèles et listes" })).toBeEnabled()
  })

  it("adds a question to a memo template", async () => {
    const r = await lists()
    mock.on("PUT", "/api/project-config", (call) => ({ body: call.body }))
    fireEvent.click(within(r).getByRole("button", { name: "Modifier : Modèles de mémo de familiarisation" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.click(within(dialog).getByRole("button", { name: "Ajouter une question" }))
    fireEvent.change(within(dialog).getByLabelText("Question 2"), { target: { value: "Ce qui surprend ?" } })
    fireEvent.click(within(dialog).getByRole("button", { name: "Terminé" }))
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer les modèles et listes" }))
    await waitFor(() => expect(putProject()).toHaveLength(1))
    expect((putProject()[0].body as ProjectConfig).memo_templates[0].questions.map((q) => q.text)).toEqual(["Première impression ?", "Ce qui surprend ?"])
  })
})

describe("questions de recherche", () => {
  it("adds a question with the next identifier", async () => {
    renderWithClient(<SettingsPage />)
    mock.on("POST", "/api/research-questions", { body: [{ id: "SQ1", text: "Pourquoi ?", ord: 1 }, { id: "SQ2", text: "Comment ?", ord: 2 }] })
    const input = await screen.findByLabelText("Nouvelle question de recherche")
    fireEvent.change(input, { target: { value: "Comment ?" } })
    fireEvent.click(screen.getByRole("button", { name: "Ajouter une question" }))
    await waitFor(() => expect(mock.to("POST", "/api/research-questions")).toHaveLength(1))
    expect(mock.to("POST", "/api/research-questions")[0].body).toEqual({ id: "SQ2", text: "Comment ?" })
  })
})

describe("modèle de projet", () => {
  it("shows a dry run before importing and applies only on confirmation", async () => {
    renderWithClient(<SettingsPage />)
    mock.on("POST", "/api/project/template/import", (call) => ({
      body: { dry_run: (call.body as { dry_run: boolean }).dry_run, applied: false, changes: [{ part: "attributs", action: "ajout", detail: "Rôle" }], problems: [] },
    }))
    const input = (await screen.findByLabelText("Fichier de modèle de projet")) as HTMLInputElement
    const file = new File([JSON.stringify({ attributes: [] })], "m.json", { type: "application/json" })
    Object.defineProperty(file, "text", { value: () => Promise.resolve(JSON.stringify({ attributes: [] })) })
    fireEvent.change(input, { target: { files: [file] } })
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("attributs : ajout, Rôle")
    expect(mock.to("POST", "/api/project/template/import").map((c) => (c.body as { dry_run: boolean }).dry_run)).toEqual([true])
    fireEvent.click(within(dialog).getByRole("button", { name: "Importer" }))
    await waitFor(() => expect(mock.to("POST", "/api/project/template/import")).toHaveLength(2))
    expect((mock.to("POST", "/api/project/template/import")[1].body as { dry_run: boolean }).dry_run).toBe(false)
  })
})
