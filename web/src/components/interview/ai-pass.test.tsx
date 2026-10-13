// "Run with provider" in the AI pass tab. Contract:
// - a native <select> labelled "Provider" whose first option, selected by
//   default, is "Copy and paste" (value copy_paste); the other options are
//   the registry entries other than copy_paste, in order, valued by id;
// - with copy_paste selected the manual flow is unchanged and no run button
//   exists; with a provider selected a "Run with provider" form appears;
// - the run goes to POST /api/interviews/{id}/batches/{batch_id}/run with the
//   batch_id of the latest prompt batch;
// - the outcome is shown inline: the journal line and the run label when it
//   succeeded; the issues, "Nothing was imported" and each rejected raw
//   answer when the validation refused it; any other error as role="alert".

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { AiPassTab } from "@/components/interview/ai-pass"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { AiRun, ProviderEntry, PromptPart } from "@/lib/types"

const ID = "INT-01"

const part = (n: number, of: number, batch: number): PromptPart => ({
  interview_id: ID,
  phase: 2,
  batch_id: batch,
  codes: ["TECH-ADV"],
  part: n,
  of,
  unit_ids: [`${ID}-U00${n}`],
  segmentation_hash: "h",
  codebook_hash: "c",
  prompt_version: "CS-P04-EN v3.1",
  text: `prompt ${n}`,
  sha256: `sha${batch}${n}`.padEnd(64, "0"),
})

const entry = (id: string, kind: ProviderEntry["kind"], execution: ProviderEntry["description"]["execution"]): ProviderEntry => ({
  id,
  kind,
  description: { id, kind, sends: kind === "copy_paste" ? [] : ["prompt"], execution },
  configured: true,
  config: null,
})

const REGISTRY: ProviderEntry[] = [
  entry("copy_paste", "copy_paste", { kind: "local" }),
  entry("claude_cli", "cli", { kind: "hosted", vendor: "anthropic", endpoint: "https://api.anthropic.com" }),
  entry("gpt", "open_ai_compatible", { kind: "hosted", vendor: "openai", endpoint: "https://api.openai.com/v1" }),
]

const RUN: AiRun = {
  run_id: 1,
  interview_id: ID,
  phase: 2,
  batch_id: 2,
  unit_ids: [`${ID}-U001`],
  codings: { [`${ID}-U001`]: ["TECH-ADV"] },
  parts_received: [1, 2],
  parts_expected: 2,
  complete: true,
  meta: {
    run_label: "R1",
    model_name: "claude-x",
    model_version: "",
    temperature: null,
    seed: null,
    context_length: null,
    batch_size: 1,
    prompt_id_version: "CS-P04-EN v3.1",
    executed_at: "2026-10-13T08:00:00Z",
    notes: null,
    batch_id: 2,
  },
}

let mock: MockApi
let runs: AiRun[]
const RUN_URI = `/api/interviews/${ID}/batches/2/run`

beforeEach(() => {
  runs = []
  mock = mockApi()
  mock.on("GET", `/api/interviews/${ID}/phases/2`, {
    body: {
      phase: 2,
      summary: { phase: 2, n_units: 2, n_human_coded: 2, human_complete: true, n_ai_runs: 0, n_ai_runs_complete: 0, n_divergences: 0, n_arbitrated: 0, reconciled: false },
      guard: { phase: 2, complete: true, n_units: 2, missing: [] },
    },
  })
  // An older batch (1) and the latest batch (2) of two parts.
  mock.on("GET", `/api/interviews/${ID}/phases/2/prompts`, { body: [part(1, 1, 1), part(1, 2, 2), part(2, 2, 2)] })
  mock.on("GET", `/api/interviews/${ID}/phases/2/runs`, () => ({ body: runs }))
  mock.on("GET", "/api/providers", { body: REGISTRY })
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

async function tab() {
  renderWithClient(<AiPassTab id={ID} />)
  await screen.findByText("Part 1 of 2")
  return screen.findByRole("region", { name: "Run with a provider" })
}
const change = (el: HTMLElement, value: string) => fireEvent.change(el, { target: { value } })

describe("Run with provider", () => {
  it("lists copy and paste first and selected, then the other providers", async () => {
    const r = await tab()
    const select = within(r).getByLabelText("Provider") as HTMLSelectElement
    const options = within(select).getAllByRole("option") as HTMLOptionElement[]
    expect(options.map((o) => o.value)).toEqual(["copy_paste", "claude_cli", "gpt"])
    expect(options[0].textContent).toBe("Copy and paste")
    expect(select.value).toBe("copy_paste")
  })

  it("keeps the manual flow and offers no run while copy and paste is selected", async () => {
    const r = await tab()
    expect(within(r).queryByRole("button", { name: "Run with provider" })).toBeNull()
    expect(screen.getByRole("button", { name: "Generate prompts" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Record the run" })).toBeInTheDocument()
    expect(within(r).getByText(/copied by hand|copy each part/i)).toBeInTheDocument()
  })

  it("shows what leaves the machine for the selected provider", async () => {
    const r = await tab()
    change(within(r).getByLabelText("Provider"), "claude_cli")
    expect(within(r).getByText("command local, model hosted")).toBeInTheDocument()
    change(within(r).getByLabelText("Provider"), "gpt")
    expect(within(r).getByText("hosted (openai, https://api.openai.com/v1)")).toBeInTheDocument()
  })

  it("runs the latest batch and shows the journal line and the result", async () => {
    const r = await tab()
    mock.on("POST", RUN_URI, () => {
      runs = [RUN]
      return { body: { run: RUN, journal_line: "command local, model hosted" } }
    })
    change(within(r).getByLabelText("Provider"), "claude_cli")
    expect(within(r).getByLabelText("Run label")).toHaveValue("R1")
    fireEvent.click(within(r).getByRole("button", { name: "Run with provider" }))
    expect(await within(r).findByText("Run R1 complete: 2 of 2 parts imported")).toBeInTheDocument()
    expect(within(r).getByText("command local, model hosted")).toBeInTheDocument()
    expect(mock.to("POST", RUN_URI)).toHaveLength(1)
    expect(mock.to("POST", RUN_URI)[0].body).toEqual({ provider_id: "claude_cli", run_label: "R1" })
    // Never the older batch.
    expect(mock.to("POST", /\/batches\/1\/run$/)).toHaveLength(0)
    // The runs table is refreshed.
    expect(await screen.findByText("claude-x")).toBeInTheDocument()
  })

  it("sends the optional parameters it was given", async () => {
    const r = await tab()
    mock.on("POST", RUN_URI, { body: { run: RUN, journal_line: "hosted (openai, https://api.openai.com/v1)" } })
    change(within(r).getByLabelText("Provider"), "gpt")
    change(within(r).getByLabelText("Run label"), "R2")
    change(within(r).getByLabelText("Model"), "gpt-4o")
    change(within(r).getByLabelText("Temperature"), "0.2")
    change(within(r).getByLabelText("Seed"), "7")
    change(within(r).getByLabelText("Max tokens"), "512")
    fireEvent.click(within(r).getByRole("button", { name: "Run with provider" }))
    await waitFor(() => expect(mock.to("POST", RUN_URI)).toHaveLength(1))
    expect(mock.to("POST", RUN_URI)[0].body).toEqual({
      provider_id: "gpt",
      run_label: "R2",
      model: "gpt-4o",
      temperature: 0.2,
      seed: 7,
      max_tokens: 512,
    })
  })

  it("needs a run label", async () => {
    const r = await tab()
    change(within(r).getByLabelText("Provider"), "gpt")
    change(within(r).getByLabelText("Run label"), "  ")
    expect(within(r).getByRole("button", { name: "Run with provider" })).toBeDisabled()
  })

  it("proposes the next label after existing runs", async () => {
    runs = [RUN]
    const r = await tab()
    change(within(r).getByLabelText("Provider"), "gpt")
    expect(within(r).getByLabelText("Run label")).toHaveValue("R2")
  })

  it("shows the issues and the rejected answer when the validation refuses it", async () => {
    const r = await tab()
    mock.on("POST", RUN_URI, {
      status: 400,
      body: {
        error: "the AI answer was rejected: 2 problem(s) found",
        kind: "validation",
        issues: [
          { location: "$", message: "no JSON object found" },
          { location: "$.codings", message: "missing" },
        ],
        rejected: [{ part: 1, raw: "I am sorry, I cannot code these units." }],
      },
    })
    change(within(r).getByLabelText("Provider"), "claude_cli")
    fireEvent.click(within(r).getByRole("button", { name: "Run with provider" }))
    expect(await within(r).findByText(/The answer was rejected: 2 problems\. Nothing was imported\./)).toBeInTheDocument()
    expect(within(r).getByText("no JSON object found")).toBeInTheDocument()
    expect(within(r).getByText("missing")).toBeInTheDocument()
    fireEvent.click(within(r).getByRole("button", { name: "Show the rejected answer of part 1" }))
    expect(within(r).getByText("I am sorry, I cannot code these units.")).toBeInTheDocument()
    expect(screen.queryByText(/Run R1 complete/)).toBeNull()
  })

  it("shows any other failure inline", async () => {
    const r = await tab()
    mock.on("POST", RUN_URI, { status: 500, body: { error: "`claude` exited with exit status: 3: boom", kind: "internal" } })
    change(within(r).getByLabelText("Provider"), "claude_cli")
    fireEvent.click(within(r).getByRole("button", { name: "Run with provider" }))
    expect(await within(r).findByRole("alert")).toHaveTextContent("exited with exit status: 3: boom")
  })

  it("shows the guard when the batch has no prompt part", async () => {
    const r = await tab()
    mock.on("POST", RUN_URI, { status: 409, body: { error: "protocol guard: batch 2 has no prompt part", kind: "guard" } })
    change(within(r).getByLabelText("Provider"), "gpt")
    fireEvent.click(within(r).getByRole("button", { name: "Run with provider" }))
    expect(await within(r).findByRole("alert")).toHaveTextContent("batch 2 has no prompt part")
  })

  it("keeps the manual flow when the registry cannot be read", async () => {
    mock.on("GET", "/api/providers", { status: 500, body: { error: "boom", kind: "internal" } })
    const r = await tab()
    const select = within(r).getByLabelText("Provider") as HTMLSelectElement
    expect(select.value).toBe("copy_paste")
    expect(within(select).getAllByRole("option")).toHaveLength(1)
    expect(screen.getByRole("button", { name: "Record the run" })).toBeInTheDocument()
  })

  it("is not offered before any prompt has been generated", async () => {
    mock.on("GET", `/api/interviews/${ID}/phases/2/prompts`, { body: [] })
    renderWithClient(<AiPassTab id={ID} />)
    await screen.findByRole("button", { name: "Generate prompts" })
    await waitFor(() => expect(mock.to("GET", "/api/providers").length).toBeGreaterThanOrEqual(0))
    expect(screen.queryByRole("button", { name: "Run with provider" })).toBeNull()
    const r = screen.getByRole("region", { name: "Run with a provider" })
    expect(within(r).getByText("Generate the prompts first.")).toBeInTheDocument()
  })
})
