import { describe, expect, it } from "vitest"
import {
  addChip,
  blankCategory,
  categoriesEditable,
  configFromForm,
  configFromEntries,
  emptyProviderForm,
  entryFromProviderForm,
  formFromConfig,
  formFromProvider,
  journalLine,
  linesToList,
  listToLines,
  nextRunLabel,
  removeChip,
  removeProvider,
  runBody,
  runnableProviders,
  upsertProvider,
  validateForm,
  validateProviderForm,
} from "./settings"
import type { ProviderConfig, ProviderEntry, StudyConfig } from "./types"

const TOE: StudyConfig = {
  framework: "toe",
  phase1_categories: [
    { code: "TECH", label: "Technological context", definition: "Technology.", attachment: "T" },
    { code: "CONTEXT", label: "Context", definition: "Background.", attachment: null },
  ],
  id_prefixes: ["INT", "ECO"],
  interviewers: ["Alex", "Alexandre Dos Santos"],
  max_words: 150,
  prompt_template: { version: "CS-P04-EN v3.1", header: "# H", codebook_intro: "{count} codes", units_intro: "Units", footer: "1. Rule" },
  study_name: "Pilot",
}

function customForm() {
  return { ...formFromConfig(TOE), framework: "custom" as const }
}

describe("study form", () => {
  it("round-trips a stored configuration into form state", () => {
    const f = formFromConfig(TOE)
    expect(f.framework).toBe("toe")
    expect(f.max_words).toBe("150")
    expect(f.categories[0]).toEqual({ code: "TECH", label: "Technological context", definition: "Technology.", attachment: "T" })
    expect(f.categories[1].attachment).toBe("")
    expect(f.id_prefixes).toEqual(["INT", "ECO"])
    expect(f.template.version).toBe("CS-P04-EN v3.1")
    expect(f.study_name).toBe("Pilot")
  })

  it("edits the category table for the custom framework only", () => {
    expect(categoriesEditable("custom")).toBe(true)
    for (const fw of ["toe", "tam_utaut", "none"] as const) expect(categoriesEditable(fw)).toBe(false)
    expect(blankCategory()).toEqual({ code: "", label: "", definition: "", attachment: "" })
  })

  it("builds the PUT body: categories only for custom, attachment '' as null", () => {
    expect(configFromForm(formFromConfig(TOE)).phase1_categories).toEqual([])
    const f = customForm()
    const cfg = configFromForm({ ...f, categories: [{ code: " ALPHA ", label: "A", definition: "d", attachment: "" }, { code: "BETA", label: "B", definition: "e", attachment: "O" }] })
    expect(cfg.framework).toBe("custom")
    expect(cfg.phase1_categories).toEqual([
      { code: "ALPHA", label: "A", definition: "d", attachment: null },
      { code: "BETA", label: "B", definition: "e", attachment: "O" },
    ])
  })

  it("types max_words, cleans interviewers and keeps the template as typed", () => {
    const f = formFromConfig(TOE)
    const cfg = configFromForm({
      ...f,
      max_words: "200",
      interviewers: [" Alex ", "", "Alex", "Jo"],
      template: { ...f.template, header: "  # kept as typed\n" },
    })
    expect(cfg.max_words).toBe(200)
    expect(cfg.interviewers).toEqual(["Alex", "Jo"])
    expect(cfg.prompt_template.header).toBe("  # kept as typed\n")
    expect(cfg.id_prefixes).toEqual(["INT", "ECO"])
  })

  it("accepts the stored configuration", () => {
    expect(validateForm(formFromConfig(TOE))).toEqual({})
  })

  it("refuses max_words below 20 with the server's words", () => {
    expect(validateForm({ ...formFromConfig(TOE), max_words: "10" }).max_words).toBe("le nombre maximal de mots doit valoir au moins 20, pas 10")
    expect(validateForm({ ...formFromConfig(TOE), max_words: "20" }).max_words).toBeUndefined()
    expect(validateForm({ ...formFromConfig(TOE), max_words: "" }).max_words).toMatch(/être un entier/)
    expect(validateForm({ ...formFromConfig(TOE), max_words: "30.5" }).max_words).toMatch(/être un entier/)
  })

  it("refuses empty, reserved and duplicate category names for custom only", () => {
    const row = (code: string) => ({ code, label: "l", definition: "d", attachment: "" })
    const v = (codes: string[]) => validateForm({ ...customForm(), categories: codes.map(row) }).categories
    expect(v([])).toBe("la liste des catégories de phase 1 est vide")
    expect(v(["ALPHA", "  "])).toBe("le nom d'une catégorie de phase 1 est vide")
    expect(v(["NA"])).toBe("NA est réservé et ne peut pas être une catégorie de phase 1")
    expect(v(["ALPHA", "BETA", "ALPHA"])).toBe("catégorie de phase 1 en double : ALPHA")
    expect(v(["ALPHA", "BETA"])).toBeUndefined()
    // A preset ignores the (hidden) table.
    const preset = { ...formFromConfig(TOE), categories: [row("")] }
    expect(validateForm(preset).categories).toBeUndefined()
  })

  it("checks id prefixes", () => {
    const v = (p: string[]) => validateForm({ ...formFromConfig(TOE), id_prefixes: p }).id_prefixes
    expect(v([])).toBe("indiquez au moins un préfixe d'identifiant")
    expect(v(["A-B"])).toBe("préfixes d'identifiant : « A-B » ne doit contenir que des lettres et des chiffres")
    expect(v(["INT", "INT"])).toBe("préfixes d'identifiant : INT en double")
    expect(v(["INT", "RET2"])).toBeUndefined()
  })

  it("requires a prompt version", () => {
    const f = formFromConfig(TOE)
    expect(validateForm({ ...f, template: { ...f.template, version: "  " } }).prompt_version).toBe("la version du modèle de prompt est vide")
  })

  it("adds and removes chips", () => {
    expect(addChip(["INT"], " ECO ")).toEqual(["INT", "ECO"])
    expect(addChip(["INT"], "   ")).toEqual(["INT"])
    expect(addChip(["INT"], "INT")).toEqual(["INT"])
    expect(removeChip(["INT", "ECO"], "INT")).toEqual(["ECO"])
    expect(removeChip(["INT"], "XXX")).toEqual(["INT"])
  })
})

// ------------------------------------------------------------- providers

const CLI: ProviderConfig = {
  type: "cli",
  id: "claude_cli",
  command: "claude",
  args: ["-p", "--output-format", "text"],
  prompt_via: "stdin",
  disable_flags: ["--no-tools"],
  declared_execution: { kind: "hosted", vendor: "anthropic", endpoint: "https://api.anthropic.com" },
  model_label: "claude-x",
}
const GPT: ProviderConfig = { type: "open_ai_compatible", id: "gpt", preset: "openai", base_url: null, api_key_env: "OPENAI_API_KEY", model: "gpt-4o" }
const OAUTH: ProviderConfig = { type: "oauth_chat_gpt", id: "chatgpt", profile_id: null, model: "gpt-5", token_dir: "/tmp/tokens" }

describe("provider form", () => {
  it("round-trips each kind", () => {
    for (const c of [CLI, GPT, OAUTH]) expect(entryFromProviderForm(formFromProvider(c))).toEqual(c)
  })

  it("starts each kind from sensible defaults", () => {
    const cli = emptyProviderForm("cli")
    expect(cli).toMatchObject({ type: "cli", prompt_via: "stdin", execution: "local", id: "", command: "" })
    expect(emptyProviderForm("open_ai_compatible")).toMatchObject({ type: "open_ai_compatible", preset: "openai", api_key_env: "" })
    expect(emptyProviderForm("oauth_chat_gpt")).toMatchObject({ type: "oauth_chat_gpt", token_dir: "" })
  })

  it("splits lines into arguments and flags, local execution without vendor", () => {
    const f = { ...emptyProviderForm("cli"), id: "x", command: "tool", args: " -p \n\n--json\n", disable_flags: "--a\n--b", model_label: "" }
    expect(entryFromProviderForm(f)).toEqual({
      type: "cli",
      id: "x",
      command: "tool",
      args: ["-p", "--json"],
      prompt_via: "stdin",
      disable_flags: ["--a", "--b"],
      declared_execution: { kind: "local" },
      model_label: null,
    })
    expect(linesToList("a\n\n b \n")).toEqual(["a", "b"])
    expect(listToLines(["a", "b"])).toBe("a\nb")
  })

  it("turns empty optional strings of the API kinds into null", () => {
    const f = { ...emptyProviderForm("open_ai_compatible"), id: "z", preset: "zai" as const, api_key_env: "ZAI_KEY", model: "glm", base_url: "  " }
    expect(entryFromProviderForm(f)).toEqual({ type: "open_ai_compatible", id: "z", preset: "zai", base_url: null, api_key_env: "ZAI_KEY", model: "glm" })
    const o = { ...emptyProviderForm("oauth_chat_gpt"), id: "c", model: "", token_dir: "/t", profile_id: "" }
    expect(entryFromProviderForm(o)).toEqual({ type: "oauth_chat_gpt", id: "c", profile_id: null, model: null, token_dir: "/t" })
  })

  it("validates ids, commands and hosted declarations", () => {
    const base = { ...emptyProviderForm("cli"), id: "ok", command: "tool" }
    expect(validateProviderForm(base, [])).toEqual({})
    expect(validateProviderForm({ ...base, id: " " }, []).id).toBe("l'identifiant du fournisseur est vide")
    expect(validateProviderForm({ ...base, id: "copy_paste" }, []).id).toBe("copy_paste est réservé au parcours manuel")
    expect(validateProviderForm(base, ["ok"]).id).toBe("identifiant de fournisseur en double : ok")
    expect(validateProviderForm({ ...base, command: "" }, []).command).toBe("la commande est vide")
    const hosted = { ...base, execution: "hosted" as const }
    expect(validateProviderForm(hosted, [])).toMatchObject({
      vendor: "l'éditeur d'un modèle hébergé est obligatoire",
      endpoint: "le point d'accès d'un modèle hébergé est obligatoire",
    })
    expect(validateProviderForm({ ...hosted, vendor: "v", endpoint: "e" }, [])).toEqual({})
  })

  it("validates the API kinds, and refuses a key pasted as its variable name", () => {
    const base = { ...emptyProviderForm("open_ai_compatible"), id: "gpt", api_key_env: "OPENAI_API_KEY", model: "gpt-4o" }
    expect(validateProviderForm(base, [])).toEqual({})
    expect(validateProviderForm({ ...base, preset: "custom" }, []).base_url).toBe("un préréglage personnalisé demande une URL de base")
    expect(validateProviderForm({ ...base, preset: "custom", base_url: "http://127.0.0.1:8080/v1" }, [])).toEqual({})
    expect(validateProviderForm({ ...base, api_key_env: "" }, []).api_key_env).toBe("le nom de la variable d'environnement qui contient la clé est obligatoire")
    expect(validateProviderForm({ ...base, api_key_env: "sk-abc123-def456" }, []).api_key_env).toBe(
      "api_key_env est le nom d'une variable d'environnement, pas la clé elle-même",
    )
    expect(validateProviderForm({ ...base, model: "" }, []).model).toBe("le modèle est obligatoire")
    const oauth = { ...emptyProviderForm("oauth_chat_gpt"), id: "chatgpt" }
    expect(validateProviderForm(oauth, [])).toEqual({})
  })

  it("adds, replaces and removes entries in order", () => {
    const cfg = { providers: [CLI, GPT] }
    expect(upsertProvider(cfg, OAUTH).providers.map((p) => p.id)).toEqual(["claude_cli", "gpt", "chatgpt"])
    const renamed = { ...GPT, id: "gpt2", model: "gpt-5" }
    expect(upsertProvider(cfg, renamed, "gpt").providers).toEqual([CLI, renamed])
    expect(removeProvider(cfg, "claude_cli").providers).toEqual([GPT])
    expect(cfg.providers).toHaveLength(2)
  })
})

const entry = (id: string, kind: ProviderEntry["kind"], config: ProviderConfig | null): ProviderEntry => ({
  id,
  kind,
  description: { id, kind, sends: [], execution: { kind: "local" } },
  configured: true,
  config,
})

describe("registry helpers", () => {
  const entries = [entry("copy_paste", "copy_paste", null), entry("claude_cli", "cli", CLI), entry("gpt", "open_ai_compatible", GPT)]
  it("lists the providers that can run a batch, without copy_paste", () => {
    expect(runnableProviders(entries).map((e) => e.id)).toEqual(["claude_cli", "gpt"])
  })
  it("recovers the stored configuration from the listing", () => {
    expect(configFromEntries(entries)).toEqual({ providers: [CLI, GPT] })
  })
  it("words the journal line like the server", () => {
    const d = (kind: ProviderEntry["kind"], execution: ProviderEntry["description"]["execution"]) => ({ id: "x", kind, sends: [], execution })
    const hosted = { kind: "hosted" as const, vendor: "openai", endpoint: "https://api.openai.com/v1" }
    expect(journalLine(d("cli", { kind: "local" }))).toBe("command local, model local")
    expect(journalLine(d("cli", hosted))).toBe("command local, model hosted")
    expect(journalLine(d("open_ai_compatible", hosted))).toBe("hosted (openai, https://api.openai.com/v1)")
    expect(journalLine(d("open_ai_compatible", { kind: "local" }))).toBe("local")
  })
})

describe("run request", () => {
  it("leaves out empty optional fields and types the numbers", () => {
    expect(runBody({ provider_id: "gpt", run_label: " R2 ", model: "", temperature: "", seed: "", max_tokens: "" })).toEqual({
      provider_id: "gpt",
      run_label: "R2",
    })
    expect(runBody({ provider_id: "gpt", run_label: "R2", model: "gpt-4o", temperature: "0.2", seed: "7", max_tokens: "512" })).toEqual({
      provider_id: "gpt",
      run_label: "R2",
      model: "gpt-4o",
      temperature: 0.2,
      seed: 7,
      max_tokens: 512,
    })
  })
  it("proposes the next free label", () => {
    expect(nextRunLabel([])).toBe("R1")
    expect(nextRunLabel(["R1", "R2"])).toBe("R3")
    expect(nextRunLabel(["R1", "R4", "pilot"])).toBe("R5")
  })
})
