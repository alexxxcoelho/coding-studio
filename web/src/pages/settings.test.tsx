// Contract of the Settings page. Accessible names, texts and request bodies
// below are the interface the implementation must honour.
//
// - Regions named by their heading: "Study configuration", "Import du codebook"
//   (page Paramètres de l'étude) and "Fournisseurs de modèles" (tab of Réglages).
// - Selects are native <select> elements, so tests drive them with change events.
// - Server refusals of a save are shown inside the region (role="alert"),
//   never only as a toast.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { ProvidersSection } from "@/components/settings/model-providers"
import { SettingsPage } from "@/pages/settings"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { IsolationReport, ProjectConfig, ProviderConfig, ProviderEntry, StudyConfig } from "@/lib/types"

const TOE: StudyConfig = {
  framework: "toe",
  phase1_categories: [
    { code: "TECH", label: "Technological context", definition: "Technology.", attachment: "T" },
    { code: "ORG", label: "Organisational context", definition: "Organisation.", attachment: "O" },
    { code: "CONTEXT", label: "Context", definition: "Background.", attachment: null },
  ],
  id_prefixes: ["INT", "ECO"],
  interviewers: ["Alex"],
  max_words: 150,
  prompt_template: {
    version: "CS-P04-EN v3.1",
    header: "# Coding Studio",
    codebook_intro: "These {count} codes.",
    units_intro: "One unit per line.",
    footer: "1. Use only the codes.",
  },
  study_name: "Pilot",
  description: "Étude pilote sur la RFID.",
  coding_rules: "Coder ce que le répondant affirme.",
}

const PROJECT: ProjectConfig = {
  attributes: [{ id: "sector", label: "Secteur", kind: "text", choices: [], shared_with_model: false }],
  memo_templates: [{ id: "default", name: "Par défaut", default: true, version: 1, questions: [{ id: "q1", text: "Première impression ?" }] }],
  checklist: [{ id: "memo", label: "Mémo écrit" }],
  theme_checks: [],
  evidence_natures: [],
  link_types: [{ id: "causes", label: "cause" }],
  matrix_dimensions: [],
  passage_origins: [{ id: "opinion", label: "Opinion" }],
  passage_roles: [{ id: "constraint", label: "Contrainte" }],
  interpretive_framework: null,
}

const CLI: ProviderConfig = {
  type: "cli",
  id: "claude_cli",
  command: "claude",
  args: ["-p"],
  prompt_via: "stdin",
  disable_flags: ["--no-tools"],
  declared_execution: { kind: "hosted", vendor: "anthropic", endpoint: "https://api.anthropic.com" },
  model_label: "claude-x",
}
const GPT: ProviderConfig = { type: "open_ai_compatible", id: "gpt", preset: "openai", base_url: null, api_key_env: "OPENAI_API_KEY", model: "gpt-4o" }
const OAUTH: ProviderConfig = { type: "oauth_chat_gpt", id: "chatgpt", profile_id: null, model: "gpt-5", token_dir: "/tmp/tokens" }

function listing(configs: ProviderConfig[], configured: Record<string, boolean> = {}): ProviderEntry[] {
  const copy: ProviderEntry = {
    id: "copy_paste",
    kind: "copy_paste",
    description: { id: "copy_paste", kind: "copy_paste", sends: [], execution: { kind: "local" } },
    configured: true,
    config: null,
  }
  const rows = configs.map((c): ProviderEntry => {
    const execution = c.type === "cli" ? c.declared_execution : { kind: "hosted" as const, vendor: "openai", endpoint: "https://api.openai.com/v1" }
    return {
      id: c.id,
      kind: c.type,
      description: { id: c.id, kind: c.type, sends: ["prompt"], execution },
      configured: configured[c.id] ?? true,
      config: c,
    }
  })
  return [copy, ...rows]
}

let mock: MockApi
let stored: ProviderConfig[]

beforeEach(() => {
  stored = [CLI, GPT]
  mock = mockApi()
  mock.on("GET", "/api/config", { body: TOE })
  mock.on("GET", "/api/research-questions", { body: [] })
  mock.on("GET", "/api/project-config", { body: PROJECT })
  mock.on("GET", "/api/project-config/presets", { body: { frameworks: { toe: { preset: "toe", dimensions: [] }, tam_utaut: { preset: "tam_utaut", dimensions: [] } }, defaults: {} } })
  mock.on("GET", "/api/state", { body: { native_dialogs: false, codebook_size: 0 } })
  mock.on("GET", "/api/providers", () => ({ body: listing(stored, { gpt: false }) }))
  mock.on("PUT", "/api/providers", (call) => {
    stored = (call.body as { providers: ProviderConfig[] }).providers
    return { body: listing(stored) }
  })
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

async function study() {
  renderWithClient(<SettingsPage />)
  const region = await screen.findByRole("region", { name: "Étude" })
  await within(region).findByDisplayValue("Pilot")
  return region
}

const change = (el: HTMLElement, value: string) => fireEvent.change(el, { target: { value } })
const putConfig = () => mock.to("PUT", /^\/api\/config/)

// ------------------------------------------------------ study configuration

describe("study configuration form", () => {
  it("loads the stored configuration", async () => {
    const r = await study()
    expect(within(r).getByLabelText("Nom de l'étude")).toHaveValue("Pilot")
    expect(within(r).queryByLabelText("Jeu de catégories de départ")).toBeNull()
    expect(within(r).getByLabelText("Longueur maximale d'une unité, en mots")).toHaveValue(150)
    expect(within(r).getByLabelText("Version des instructions")).toHaveValue("CS-P04-EN v3.1")
    expect(within(r).getByLabelText("Description de l'étude")).toHaveValue("Étude pilote sur la RFID.")
    expect(within(r).getByLabelText("Règles de codage")).toHaveValue("Coder ce que le répondant affirme.")
    expect(within(r).getByLabelText("Introduction du codebook")).toHaveValue("These {count} codes.")
    expect(within(r).getByLabelText("Introduction des unités")).toHaveValue("One unit per line.")
    for (const chip of ["INT", "ECO", "Alex"]) expect(within(r).getByText(chip)).toBeInTheDocument()
  })

  it("no longer shows the phase 1 category set", async () => {
    const r = await study()
    expect(within(r).queryByLabelText("Code de la catégorie 1")).toBeNull()
    expect(within(r).queryByRole("button", { name: "Ajouter une catégorie" })).toBeNull()
  })

  it("saves with PUT /api/config and says so", async () => {
    const r = await study()
    mock.on("PUT", "/api/config", { body: TOE })
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    expect(await within(r).findByText("Étude enregistrée")).toBeInTheDocument()
    expect(putConfig()).toHaveLength(1)
    expect(putConfig()[0].path).toBe("/api/config")
    expect(putConfig()[0].body).toEqual({ ...TOE, phase1_categories: [] })
  })

  it("sends the description, the coding rules and the other fields", async () => {
    const r = await study()
    mock.on("PUT", "/api/config", { body: TOE })
    change(within(r).getByLabelText("Description de l'étude"), "Une étude sur les bibliothèques.")
    change(within(r).getByLabelText("Règles de codage"), "Plusieurs codes possibles.")
    change(within(r).getByLabelText("Nom de l'étude"), "Main study")
    change(within(r).getByLabelText("Longueur maximale d'une unité, en mots"), "200")
    change(within(r).getByLabelText("Version des instructions"), "CS-P04-EN v4")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    await waitFor(() => expect(putConfig()).toHaveLength(1))
    expect(putConfig()[0].body).toMatchObject({
      study_name: "Main study",
      max_words: 200,
      description: "Une étude sur les bibliothèques.",
      coding_rules: "Plusieurs codes possibles.",
      prompt_template: { version: "CS-P04-EN v4", header: "# Coding Studio", footer: "1. Use only the codes." },
    })
  })

  it("adds and removes id prefixes and interviewers", async () => {
    const r = await study()
    mock.on("PUT", "/api/config", { body: TOE })
    change(within(r).getByLabelText("Nouveau préfixe"), "RET")
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un préfixe" }))
    expect(within(r).getByText("RET")).toBeInTheDocument()
    expect(within(r).getByLabelText("Nouveau préfixe")).toHaveValue("")
    fireEvent.click(within(r).getByRole("button", { name: "Retirer le préfixe INT" }))
    expect(within(r).queryByText("INT")).toBeNull()

    change(within(r).getByLabelText("Nouvel intervieweur"), "Jo Smith")
    fireEvent.keyDown(within(r).getByLabelText("Nouvel intervieweur"), { key: "Enter" })
    expect(within(r).getByText("Jo Smith")).toBeInTheDocument()

    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    await waitFor(() => expect(putConfig()).toHaveLength(1))
    expect(putConfig()[0].body).toMatchObject({ id_prefixes: ["ECO", "RET"], interviewers: ["Alex", "Jo Smith"] })
  })

  it("refuses max words below 20 before calling the server", async () => {
    const r = await study()
    change(within(r).getByLabelText("Longueur maximale d'une unité, en mots"), "10")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    expect(await within(r).findByText("le nombre maximal de mots doit valoir au moins 20, pas 10")).toBeInTheDocument()
    expect(putConfig()).toHaveLength(0)
  })

  it("shows the server's validation error next to the form", async () => {
    const r = await study()
    mock.on("PUT", "/api/config", { status: 400, body: { error: "catégorie de phase 1 en double : ALPHA", kind: "validation" } })
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    const alert = await within(r).findByRole("alert")
    expect(alert).toHaveTextContent("catégorie de phase 1 en double : ALPHA")
    expect(within(r).queryByText("Étude enregistrée")).toBeNull()
  })

  it("asks to force a save refused by the guard, and re-sends with force=true", async () => {
    const r = await study()
    const guard = "protocol guard: phase 1 coding exists; renaming a category would orphan it"
    mock.on("PUT", "/api/config", { status: 409, body: { error: guard, kind: "guard" } })
    mock.on("PUT", "/api/config?force=true", { body: TOE })
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent(guard)
    expect(putConfig()).toHaveLength(1)
    fireEvent.click(within(dialog).getByRole("button", { name: "Enregistrer quand même" }))
    await waitFor(() => expect(putConfig()).toHaveLength(2))
    expect(putConfig()[1].path).toBe("/api/config?force=true")
    expect(putConfig()[1].body).toEqual(putConfig()[0].body)
    expect(await within(r).findByText("Étude enregistrée")).toBeInTheDocument()
  })

  it("does not force when the guard confirmation is cancelled", async () => {
    const r = await study()
    mock.on("PUT", "/api/config", { status: 409, body: { error: "protocol guard: x", kind: "guard" } })
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer l'étude" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.click(within(dialog).getByRole("button", { name: "Annuler" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(putConfig()).toHaveLength(1)
  })
})

// ---------------------------------------------------------- codebook import

describe("codebook import", () => {
  async function region() {
    renderWithClient(<SettingsPage />)
    return screen.findByRole("region", { name: "Import du codebook" })
  }

  it("offers csv, json and coding_db", async () => {
    const r = await region()
    const kind = within(r).getByLabelText("Type d'import")
    expect(within(kind).getAllByRole("option").map((o) => (o as HTMLOptionElement).value)).toEqual(["csv", "json", "coding_db"])
    expect(kind).toHaveValue("csv")
  })

  it("imports with the kind and the path, and shows the counts", async () => {
    const r = await region()
    mock.on("POST", "/api/codebook/import", { body: { source: "/data/codebook.json", codes: [], report: { added: 2, updated: 1, absent: 3 } } })
    change(within(r).getByLabelText("Type d'import"), "json")
    change(within(r).getByLabelText("Chemin du fichier"), "/data/codebook.json")
    fireEvent.click(within(r).getByRole("button", { name: "Importer le codebook" }))
    expect(await within(r).findByText("2 ajoutés, 1 mis à jour, 3 absents")).toBeInTheDocument()
    expect(mock.to("POST", "/api/codebook/import")[0].body).toEqual({ kind: "json", path: "/data/codebook.json" })
  })

  it("cannot import without a path", async () => {
    const r = await region()
    expect(within(r).getByRole("button", { name: "Importer le codebook" })).toBeDisabled()
  })

  it("shows the server's refusal", async () => {
    const r = await region()
    mock.on("POST", "/api/codebook/import", { status: 400, body: { error: "duplicate code A-ONE", kind: "validation" } })
    change(within(r).getByLabelText("Chemin du fichier"), "/data/bad.csv")
    fireEvent.click(within(r).getByRole("button", { name: "Importer le codebook" }))
    expect(await within(r).findByRole("alert")).toHaveTextContent("duplicate code A-ONE")
  })

  it("uses the native picker when the desktop app provides one", async () => {
    mock.on("GET", "/api/state", { body: { native_dialogs: true, codebook_size: 0 } })
    mock.on("POST", "/api/pick", { body: { path: "/Users/me/codebook.csv" } })
    const r = await region()
    fireEvent.click(await within(r).findByRole("button", { name: "Choisir un fichier" }))
    await waitFor(() => expect(within(r).getByLabelText("Chemin du fichier")).toHaveValue("/Users/me/codebook.csv"))
    expect(mock.to("POST", "/api/pick")[0].body).toMatchObject({ kind: "file" })
  })

  it("keeps the typed path when the picker is cancelled", async () => {
    mock.on("GET", "/api/state", { body: { native_dialogs: true, codebook_size: 0 } })
    mock.on("POST", "/api/pick", { body: { path: null } })
    const r = await region()
    change(within(r).getByLabelText("Chemin du fichier"), "/typed/path.csv")
    fireEvent.click(await within(r).findByRole("button", { name: "Choisir un fichier" }))
    await waitFor(() => expect(mock.to("POST", "/api/pick")).toHaveLength(1))
    expect(within(r).getByLabelText("Chemin du fichier")).toHaveValue("/typed/path.csv")
  })

  it("has no picker button in browser mode", async () => {
    const r = await region()
    await waitFor(() => expect(mock.to("GET", "/api/state").length).toBeGreaterThan(0))
    expect(within(r).queryByRole("button", { name: "Choisir un fichier" })).toBeNull()
  })
})

// ------------------------------------------------------------- providers

describe("model providers", () => {
  async function region() {
    renderWithClient(<ProvidersSection pollMs={5} />)
    const r = await screen.findByRole("region", { name: "Fournisseurs de modèles" })
    await within(r).findByText("claude_cli")
    return r
  }
  const putProviders = () => mock.to("PUT", "/api/providers")

  it("lists the registry with copy_paste first and no edit controls on it", async () => {
    const r = await region()
    const rows = within(r).getAllByRole("listitem")
    expect(rows.map((row) => within(row).getAllByText(/^(copy_paste|claude_cli|gpt)$/)[0].textContent)).toEqual(["copy_paste", "claude_cli", "gpt"])
    expect(within(rows[0]).queryByRole("button", { name: /^(Modifier|Retirer|Tester)/ })).toBeNull()
    expect(within(rows[1]).getByText("command local, model hosted")).toBeInTheDocument()
    expect(within(rows[1]).getByText("configuré")).toBeInTheDocument()
    expect(within(rows[2]).getByText("non configuré")).toBeInTheDocument()
    expect(within(rows[2]).getByText("hosted (openai, https://api.openai.com/v1)")).toBeInTheDocument()
  })

  it("never offers a field for the key itself", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un fournisseur" }))
    change(within(r).getByLabelText("Type de fournisseur"), "open_ai_compatible")
    expect(within(r).getByLabelText("Nom de la variable de la clé d'API")).toBeInTheDocument()
    expect(within(r).queryByLabelText(/^API key$/i)).toBeNull()
    expect(r.querySelector("input[type=password]")).toBeNull()
  })

  it("adds a command line provider and saves the whole configuration", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un fournisseur" }))
    expect(within(r).getByLabelText("Type de fournisseur")).toHaveValue("cli")
    const kinds = within(within(r).getByLabelText("Type de fournisseur")).getAllByRole("option")
    expect(kinds.map((o) => (o as HTMLOptionElement).value)).toEqual(["cli", "open_ai_compatible", "oauth_chat_gpt"])
    change(within(r).getByLabelText("Identifiant du fournisseur"), "local_llm")
    change(within(r).getByLabelText("Commande"), "/usr/local/bin/llm")
    change(within(r).getByLabelText("Arguments (un par ligne)"), "--quiet\n--json")
    change(within(r).getByLabelText("Envoi du prompt par"), "arg")
    change(within(r).getByLabelText("Options de désactivation (une par ligne)"), "--no-memory")
    change(within(r).getByLabelText("Libellé du modèle"), "llama-3")
    expect(within(r).queryByLabelText("Éditeur")).toBeNull()
    change(within(r).getByLabelText("Exécution déclarée"), "hosted")
    change(within(r).getByLabelText("Éditeur"), "meta")
    change(within(r).getByLabelText("Endpoint"), "https://llm.example/v1")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    await waitFor(() => expect(putProviders()).toHaveLength(1))
    expect(putProviders()[0].body).toEqual({
      providers: [
        CLI,
        GPT,
        {
          type: "cli",
          id: "local_llm",
          command: "/usr/local/bin/llm",
          args: ["--quiet", "--json"],
          prompt_via: "arg",
          disable_flags: ["--no-memory"],
          declared_execution: { kind: "hosted", vendor: "meta", endpoint: "https://llm.example/v1" },
          model_label: "llama-3",
        },
      ],
    })
    expect(await within(r).findByText("local_llm")).toBeInTheDocument()
  })

  it("adds an OpenAI-compatible provider; a custom preset needs a base url", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un fournisseur" }))
    change(within(r).getByLabelText("Type de fournisseur"), "open_ai_compatible")
    const presets = within(within(r).getByLabelText("Préréglage")).getAllByRole("option")
    expect(presets.map((o) => (o as HTMLOptionElement).value)).toEqual(["openai", "zai", "custom"])
    change(within(r).getByLabelText("Identifiant du fournisseur"), "lan")
    change(within(r).getByLabelText("Préréglage"), "custom")
    change(within(r).getByLabelText("Nom de la variable de la clé d'API"), "LAN_KEY")
    change(within(r).getByLabelText("Modèle"), "qwen")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    expect(await within(r).findByText("un préréglage personnalisé demande une URL de base")).toBeInTheDocument()
    expect(putProviders()).toHaveLength(0)
    change(within(r).getByLabelText("URL de base"), "http://127.0.0.1:8080/v1")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    await waitFor(() => expect(putProviders()).toHaveLength(1))
    expect(putProviders()[0].body).toMatchObject({
      providers: [CLI, GPT, { type: "open_ai_compatible", id: "lan", preset: "custom", base_url: "http://127.0.0.1:8080/v1", api_key_env: "LAN_KEY", model: "qwen" }],
    })
  })

  it("refuses a pasted key in the variable name field", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un fournisseur" }))
    change(within(r).getByLabelText("Type de fournisseur"), "open_ai_compatible")
    change(within(r).getByLabelText("Identifiant du fournisseur"), "oops")
    change(within(r).getByLabelText("Nom de la variable de la clé d'API"), "sk-abc123-def456")
    change(within(r).getByLabelText("Modèle"), "m")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    expect(await within(r).findByText("api_key_env est le nom d'une variable d'environnement, pas la clé elle-même")).toBeInTheDocument()
    expect(putProviders()).toHaveLength(0)
  })

  it("adds a ChatGPT account provider", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un fournisseur" }))
    change(within(r).getByLabelText("Type de fournisseur"), "oauth_chat_gpt")
    change(within(r).getByLabelText("Identifiant du fournisseur"), "chatgpt")
    change(within(r).getByLabelText("Profil"), "work")
    change(within(r).getByLabelText("Modèle"), "gpt-5")
    change(within(r).getByLabelText("Dossier des jetons"), "/tmp/tokens")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    await waitFor(() => expect(putProviders()).toHaveLength(1))
    expect(putProviders()[0].body).toMatchObject({
      providers: [CLI, GPT, { type: "oauth_chat_gpt", id: "chatgpt", profile_id: "work", model: "gpt-5", token_dir: "/tmp/tokens" }],
    })
  })

  it("edits an entry in place, keeping the order", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Modifier claude_cli" }))
    expect(within(r).getByLabelText("Identifiant du fournisseur")).toHaveValue("claude_cli")
    expect(within(r).getByLabelText("Commande")).toHaveValue("claude")
    expect(within(r).getByLabelText("Arguments (un par ligne)")).toHaveValue("-p")
    expect(within(r).getByLabelText("Exécution déclarée")).toHaveValue("hosted")
    expect(within(r).getByLabelText("Éditeur")).toHaveValue("anthropic")
    change(within(r).getByLabelText("Libellé du modèle"), "claude-y")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    await waitFor(() => expect(putProviders()).toHaveLength(1))
    expect(putProviders()[0].body).toEqual({ providers: [{ ...CLI, model_label: "claude-y" }, GPT] })
  })

  it("does not accept a second provider with the same id", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Ajouter un fournisseur" }))
    change(within(r).getByLabelText("Identifiant du fournisseur"), "gpt")
    change(within(r).getByLabelText("Commande"), "x")
    fireEvent.click(within(r).getByRole("button", { name: "Enregistrer le fournisseur" }))
    expect(await within(r).findByText("identifiant de fournisseur en double : gpt")).toBeInTheDocument()
    expect(putProviders()).toHaveLength(0)
  })

  it("shows a server refusal of the providers configuration", async () => {
    const r = await region()
    mock.on("PUT", "/api/providers", { status: 400, body: { error: "custom preset needs a base_url", kind: "validation" } })
    fireEvent.click(within(r).getByRole("button", { name: "Retirer gpt" }))
    expect(await within(r).findByRole("alert")).toHaveTextContent("custom preset needs a base_url")
  })

  it("removes an entry", async () => {
    const r = await region()
    fireEvent.click(within(r).getByRole("button", { name: "Retirer claude_cli" }))
    await waitFor(() => expect(putProviders()).toHaveLength(1))
    expect(putProviders()[0].body).toEqual({ providers: [GPT] })
    await waitFor(() => expect(within(r).queryByText("claude_cli")).toBeNull())
  })

  it("probes a command line provider and shows the isolation report", async () => {
    const r = await region()
    const report: IsolationReport = { ok: true, cwd_empty: true, env_scrubbed: true, instruction_files_absent: true, probe_answer: "NONE" }
    mock.on("POST", "/api/providers/claude_cli/probe", { body: report })
    fireEvent.click(within(r).getByRole("button", { name: "Tester claude_cli" }))
    expect(await within(r).findByText("Contrôle d'isolation réussi")).toBeInTheDocument()
    const rows = within(r).getByRole("list", { name: "Rapport d'isolation de claude_cli" })
    expect(within(rows).getByText("Dossier de travail vide").parentElement).toHaveTextContent("oui")
    expect(within(rows).getByText("Environnement nettoyé").parentElement).toHaveTextContent("oui")
    expect(within(rows).getByText("Aucun fichier d'instructions").parentElement).toHaveTextContent("oui")
    expect(within(r).getByText("NONE")).toBeInTheDocument()
  })

  it("reports a failed probe with what the model said", async () => {
    const r = await region()
    mock.on("POST", "/api/providers/claude_cli/probe", {
      body: { ok: false, cwd_empty: true, env_scrubbed: true, instruction_files_absent: false, probe_answer: "I can see CLAUDE.md" },
    })
    fireEvent.click(within(r).getByRole("button", { name: "Tester claude_cli" }))
    expect(await within(r).findByText("Contrôle d'isolation échoué")).toBeInTheDocument()
    expect(within(r).getByText("I can see CLAUDE.md")).toBeInTheDocument()
    expect(within(within(r).getByRole("list", { name: "Rapport d'isolation de claude_cli" })).getByText("Aucun fichier d'instructions").parentElement).toHaveTextContent("non")
  })

  it("probes command line providers only", async () => {
    const r = await region()
    expect(within(r).queryByRole("button", { name: "Tester gpt" })).toBeNull()
    expect(within(r).queryByRole("button", { name: "Tester copy_paste" })).toBeNull()
  })

  it("connects a ChatGPT account: shows the address to open, then follows the sign-in", async () => {
    stored = [OAUTH]
    mock.on("GET", "/api/providers", () => ({ body: listing(stored, { chatgpt: false }) }))
    mock.on("POST", "/api/providers/chatgpt/oauth/begin", { body: { url: "https://auth.example/authorize?state=abc", state: "abc" } })
    mock.on("GET", "/api/providers/chatgpt/oauth/status", (_c, nth) =>
      nth < 2
        ? { body: { connected: false, pending: true, profile: null, error: null } }
        : { body: { connected: true, pending: false, profile: { id: "p1", email: "me@example.org" }, error: null } },
    )
    renderWithClient(<ProvidersSection pollMs={5} />)
    const r = await screen.findByRole("region", { name: "Fournisseurs de modèles" })
    fireEvent.click(await within(r).findByRole("button", { name: "Connecter chatgpt" }))
    const link = await within(r).findByRole("link", { name: "https://auth.example/authorize?state=abc" })
    expect(link).toHaveAttribute("href", "https://auth.example/authorize?state=abc")
    expect(link).toHaveAttribute("target", "_blank")
    expect(mock.to("POST", "/api/providers/chatgpt/oauth/begin")).toHaveLength(1)
    expect(await within(r).findByText("Connecté en tant que me@example.org")).toBeInTheDocument()
    expect(mock.to("GET", "/api/providers/chatgpt/oauth/status").length).toBeGreaterThanOrEqual(3)
    // Polling stops once connected.
    const n = mock.to("GET", "/api/providers/chatgpt/oauth/status").length
    await new Promise((res) => setTimeout(res, 60))
    expect(mock.to("GET", "/api/providers/chatgpt/oauth/status").length).toBe(n)
  })

  it("reports a failed sign-in and stops polling", async () => {
    stored = [OAUTH]
    mock.on("GET", "/api/providers", () => ({ body: listing(stored, { chatgpt: false }) }))
    mock.on("POST", "/api/providers/chatgpt/oauth/begin", { body: { url: "https://auth.example/a", state: "s" } })
    mock.on("GET", "/api/providers/chatgpt/oauth/status", { body: { connected: false, pending: false, profile: null, error: "the sign-in timed out" } })
    renderWithClient(<ProvidersSection pollMs={5} />)
    const r = await screen.findByRole("region", { name: "Fournisseurs de modèles" })
    fireEvent.click(await within(r).findByRole("button", { name: "Connecter chatgpt" }))
    expect(await within(r).findByRole("alert")).toHaveTextContent("the sign-in timed out")
    const n = mock.to("GET", "/api/providers/chatgpt/oauth/status").length
    await new Promise((res) => setTimeout(res, 60))
    expect(mock.to("GET", "/api/providers/chatgpt/oauth/status").length).toBe(n)
  })

  it("offers Connect for ChatGPT accounts only", async () => {
    const r = await region()
    expect(within(r).queryByRole("button", { name: /^Connecter/ })).toBeNull()
  })
})
