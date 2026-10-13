// Pure helpers of the Settings page: study configuration form, provider
// entries and the run request. Mirrors of the server checks live here so the
// form can refuse early; the server stays the authority (crates/cs-model
// config.rs and crates/cs-app providers).

import type {
  FrameworkPreset,
  PromptVia,
  Preset,
  ProviderConfig,
  ProviderDescription,
  ProviderEntry,
  ProviderRunBody,
  ProvidersConfig,
  StudyConfig,
} from "./types"

/** Smallest accepted `max_words` (cs-model MIN_MAX_WORDS). */
export const MIN_MAX_WORDS = 20

/** Id reserved by the manual workflow. */
export const COPY_PASTE_ID = "copy_paste"

/** First message per field; the keys are named in each helper. */
export type FormErrors = Record<string, string>

// ------------------------------------------------------ study configuration

export interface CategoryRow {
  code: string
  label: string
  definition: string
  /** "" (none), "T", "O" or "E". */
  attachment: string
}

export interface StudyForm {
  study_name: string
  framework: FrameworkPreset
  categories: CategoryRow[]
  id_prefixes: string[]
  interviewers: string[]
  /** Text of the number input, so that an empty or partial value can be shown. */
  max_words: string
  template: { version: string; header: string; codebook_intro: string; units_intro: string; footer: string }
}

/** The category table is editable for the custom framework only. */
export function categoriesEditable(framework: FrameworkPreset): boolean {
  return framework === "custom"
}

export function blankCategory(): CategoryRow {
  return { code: "", label: "", definition: "", attachment: "" }
}

/** Form state of a stored configuration (attachment null becomes ""). */
export function formFromConfig(cfg: StudyConfig): StudyForm {
  return {
    study_name: cfg.study_name,
    framework: cfg.framework,
    categories: cfg.phase1_categories.map((c) => ({
      code: c.code,
      label: c.label,
      definition: c.definition,
      attachment: c.attachment ?? "",
    })),
    id_prefixes: [...cfg.id_prefixes],
    interviewers: [...cfg.interviewers],
    max_words: String(cfg.max_words),
    template: { ...cfg.prompt_template },
  }
}

/**
 * Body of `PUT /api/config`. Categories are sent for `custom` only (the
 * server derives them from the preset otherwise) with "" as null; names are
 * trimmed; interviewers are trimmed, emptied and de-duplicated; `max_words`
 * is a number; the template is sent as typed.
 */
export function configFromForm(form: StudyForm): StudyConfig {
  const custom = form.framework === "custom"
  return {
    framework: form.framework,
    study_name: form.study_name.trim(),
    phase1_categories: custom
      ? form.categories.map((c) => ({
          code: c.code.trim(),
          label: c.label.trim(),
          definition: c.definition.trim(),
          attachment: c.attachment === "" ? null : c.attachment,
        }))
      : [],
    id_prefixes: form.id_prefixes.map((p) => p.trim()).filter((p) => p !== ""),
    interviewers: [...new Set(form.interviewers.map((n) => n.trim()).filter((n) => n !== ""))],
    max_words: Number(form.max_words),
    prompt_template: { ...form.template },
  }
}

const isPrefix = (p: string) => /^[A-Za-z0-9]+$/.test(p)

/**
 * Mirror of `StudyConfig::validate`, with the server's messages. Keys:
 * `categories` (custom only: empty list, empty name, NA, duplicate),
 * `max_words`, `id_prefixes` (empty list, empty or non-alphanumeric or
 * duplicate prefix), `prompt_version`.
 */
export function validateForm(form: StudyForm): FormErrors {
  const errors: FormErrors = {}
  const words = form.max_words.trim()
  const n = Number(words)
  if (words === "" || !Number.isInteger(n)) {
    errors.max_words = "max_words must be a whole number"
  } else if (n < MIN_MAX_WORDS) {
    errors.max_words = `max_words must be at least ${MIN_MAX_WORDS}, not ${n}`
  }
  if (form.framework === "custom") {
    const problem = categoryProblem(form.categories)
    if (problem) errors.categories = problem
  }
  if (form.id_prefixes.length === 0) {
    errors.id_prefixes = "id_prefixes must list at least one prefix"
  } else {
    const seen = new Set<string>()
    for (const raw of form.id_prefixes) {
      const p = raw.trim()
      if (!isPrefix(p)) {
        errors.id_prefixes = `id_prefixes: "${p}" must contain only letters and digits`
        break
      }
      if (seen.has(p)) {
        errors.id_prefixes = `id_prefixes: duplicate prefix ${p}`
        break
      }
      seen.add(p)
    }
  }
  if (form.template.version.trim() === "") errors.prompt_version = "the prompt template version is empty"
  return errors
}

function categoryProblem(rows: CategoryRow[]): string | undefined {
  if (rows.length === 0) return "the phase 1 category list is empty"
  const seen = new Set<string>()
  for (const row of rows) {
    const code = row.code.trim()
    if (code === "") return "a phase 1 category name is empty"
    if (code === "NA") return "NA is reserved and cannot be a phase 1 category"
    if (seen.has(code)) return `duplicate phase 1 category ${code}`
    seen.add(code)
  }
  return undefined
}

/** Chip input: trimmed value appended; an empty or duplicate value leaves the list as it is. */
export function addChip(list: string[], raw: string): string[] {
  const value = raw.trim()
  if (value === "" || list.includes(value)) return list
  return [...list, value]
}

export function removeChip(list: string[], value: string): string[] {
  return list.filter((v) => v !== value)
}

// ---------------------------------------------------------------- providers

export type ProviderType = ProviderConfig["type"]

export interface ProviderForm {
  type: ProviderType
  id: string
  // cli
  command: string
  /** One argument per line. */
  args: string
  prompt_via: PromptVia
  /** One flag per line. */
  disable_flags: string
  execution: "local" | "hosted"
  vendor: string
  endpoint: string
  model_label: string
  // open_ai_compatible
  preset: Preset
  base_url: string
  api_key_env: string
  // open_ai_compatible and oauth_chat_gpt
  model: string
  // oauth_chat_gpt
  profile_id: string
  token_dir: string
}

/** Default values: cli = stdin, local; openai = preset openai; empty strings elsewhere. */
export function emptyProviderForm(type: ProviderType): ProviderForm {
  return {
    type,
    id: "",
    command: "",
    args: "",
    prompt_via: "stdin",
    disable_flags: "",
    execution: "local",
    vendor: "",
    endpoint: "",
    model_label: "",
    preset: "openai",
    base_url: "",
    api_key_env: "",
    model: "",
    profile_id: "",
    token_dir: "",
  }
}

export function formFromProvider(c: ProviderConfig): ProviderForm {
  const form = emptyProviderForm(c.type)
  form.id = c.id
  if (c.type === "cli") {
    form.command = c.command
    form.args = listToLines(c.args)
    form.prompt_via = c.prompt_via
    form.disable_flags = listToLines(c.disable_flags)
    form.model_label = c.model_label ?? ""
    if (c.declared_execution.kind === "hosted") {
      form.execution = "hosted"
      form.vendor = c.declared_execution.vendor
      form.endpoint = c.declared_execution.endpoint
    }
  } else if (c.type === "open_ai_compatible") {
    form.preset = c.preset
    form.base_url = c.base_url ?? ""
    form.api_key_env = c.api_key_env
    form.model = c.model
  } else {
    form.profile_id = c.profile_id ?? ""
    form.model = c.model ?? ""
    form.token_dir = c.token_dir ?? ""
  }
  return form
}

const orNull = (s: string) => (s.trim() === "" ? null : s.trim())

/**
 * Config entry of a form, for the fields of its type only. Lists are split on
 * lines (trimmed, empty lines dropped); empty optional strings become null;
 * `declared_execution` is `{kind:"local"}` or `{kind:"hosted", vendor, endpoint}`.
 */
export function entryFromProviderForm(f: ProviderForm): ProviderConfig {
  const id = f.id.trim()
  if (f.type === "cli") {
    return {
      type: "cli",
      id,
      command: f.command.trim(),
      args: linesToList(f.args),
      prompt_via: f.prompt_via,
      disable_flags: linesToList(f.disable_flags),
      declared_execution:
        f.execution === "hosted"
          ? { kind: "hosted", vendor: f.vendor.trim(), endpoint: f.endpoint.trim() }
          : { kind: "local" },
      model_label: orNull(f.model_label),
    }
  }
  if (f.type === "open_ai_compatible") {
    return {
      type: "open_ai_compatible",
      id,
      preset: f.preset,
      base_url: orNull(f.base_url),
      api_key_env: f.api_key_env.trim(),
      model: f.model.trim(),
    }
  }
  return {
    type: "oauth_chat_gpt",
    id,
    profile_id: orNull(f.profile_id),
    model: orNull(f.model),
    token_dir: orNull(f.token_dir),
  }
}

const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/

/**
 * Mirror of the server's build checks. Keys: `id` (empty, `copy_paste`,
 * duplicate among `otherIds`), `command`, `vendor` and `endpoint` (hosted),
 * `base_url` (custom preset), `api_key_env` (empty, or not shaped like the
 * name of an environment variable, so that a pasted key is refused), `model`
 * (open_ai_compatible).
 */
export function validateProviderForm(f: ProviderForm, otherIds: string[]): FormErrors {
  const errors: FormErrors = {}
  const id = f.id.trim()
  if (id === "") errors.id = "the provider id is empty"
  else if (id === COPY_PASTE_ID) errors.id = `${COPY_PASTE_ID} is reserved for the manual workflow`
  else if (otherIds.includes(id)) errors.id = `duplicate provider id ${id}`
  if (f.type === "cli") {
    if (f.command.trim() === "") errors.command = "the command is empty"
    if (f.execution === "hosted") {
      if (f.vendor.trim() === "") errors.vendor = "the vendor of a hosted model is required"
      if (f.endpoint.trim() === "") errors.endpoint = "the endpoint of a hosted model is required"
    }
  }
  if (f.type === "open_ai_compatible") {
    if (f.preset === "custom" && f.base_url.trim() === "") errors.base_url = "a custom preset needs a base_url"
    const key = f.api_key_env.trim()
    if (key === "") errors.api_key_env = "the name of the environment variable that holds the key is required"
    else if (!ENV_NAME.test(key)) errors.api_key_env = "api_key_env is the name of an environment variable, not the key itself"
    if (f.model.trim() === "") errors.model = "the model is required"
  }
  return errors
}

export function linesToList(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l !== "")
}

export function listToLines(list: string[]): string {
  return list.join("\n")
}

/** Adds the entry, or replaces the one named `replaceId` in place. */
export function upsertProvider(cfg: ProvidersConfig, entry: ProviderConfig, replaceId?: string | null): ProvidersConfig {
  const target = replaceId ?? entry.id
  const index = cfg.providers.findIndex((p) => p.id === target)
  if (index === -1) return { providers: [...cfg.providers, entry] }
  return { providers: cfg.providers.map((p, i) => (i === index ? entry : p)) }
}

export function removeProvider(cfg: ProvidersConfig, id: string): ProvidersConfig {
  return { providers: cfg.providers.filter((p) => p.id !== id) }
}

/** The stored configuration behind a registry listing (copy_paste has none). */
export function configFromEntries(entries: ProviderEntry[]): ProvidersConfig {
  return { providers: entries.flatMap((e) => (e.config ? [e.config] : [])) }
}

/** Providers that can run a batch: every entry but `copy_paste`, in order. */
export function runnableProviders(entries: ProviderEntry[]): ProviderEntry[] {
  return entries.filter((e) => e.kind !== "copy_paste" && e.id !== COPY_PASTE_ID)
}

/** Same sentence as `ProviderDescription::journal_line` in cs-ai. */
export function journalLine(d: ProviderDescription): string {
  const local = d.execution.kind === "local"
  if (d.kind === "cli") return local ? "command local, model local" : "command local, model hosted"
  if (local) return "local"
  if (d.execution.kind === "hosted") return `hosted (${d.execution.vendor}, ${d.execution.endpoint})`
  return "local"
}

/** Text fields of the "Run with provider" form. */
export interface RunForm {
  provider_id: string
  run_label: string
  model: string
  temperature: string
  seed: string
  max_tokens: string
}

/** Body of the run route: empty optional fields are left out; numbers are numbers. */
export function runBody(f: RunForm): ProviderRunBody {
  const body: ProviderRunBody = { provider_id: f.provider_id, run_label: f.run_label.trim() }
  if (f.model.trim() !== "") body.model = f.model.trim()
  if (f.temperature.trim() !== "") body.temperature = Number(f.temperature)
  if (f.seed.trim() !== "") body.seed = Number(f.seed)
  if (f.max_tokens.trim() !== "") body.max_tokens = Number(f.max_tokens)
  return body
}

/** Next free label R<n> for a list of existing labels. */
export function nextRunLabel(labels: string[]): string {
  const used = labels.map((l) => /^R(\d+)$/.exec(l)?.[1]).filter((n): n is string => n !== undefined).map(Number)
  return `R${used.length === 0 ? 1 : Math.max(...used) + 1}`
}
