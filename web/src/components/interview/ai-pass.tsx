import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Check, Copy } from "lucide-react"

import { Banner, EmptyState, Section } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { api, ApiError } from "@/lib/api"
import { dateTime, nowLocalInput, plural, shortHash } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import { uncodedUnitIds } from "@/lib/crosscase"
import { Field, NativeSelect } from "@/components/settings/fields"
import { journalLine, nextRunLabel, runBody, runnableProviders } from "@/lib/settings"
import type { AiRun, AiRunMeta, Phase, ProviderEntry, PromptPart, RejectedAnswer, ValidationIssue } from "@/lib/types"

/** Parts of the latest prompt batch (each generation is a new batch). */
function latestSet(parts: PromptPart[]): PromptPart[] {
  if (!parts.length) return []
  const last = parts[parts.length - 1]
  if (last.batch_id !== undefined) return parts.filter((p) => p.batch_id === last.batch_id)
  return parts.slice(-last.of).filter((p) => p.of === last.of && p.sha256)
}

function PromptCard({ part }: { part: PromptPart }) {
  const [copied, setCopied] = useState(false)
  const [open, setOpen] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(part.text)
    } catch {
      const ta = document.createElement("textarea")
      ta.value = part.text
      document.body.appendChild(ta)
      ta.select()
      document.execCommand("copy")
      ta.remove()
    }
    setCopied(true)
    toast.success(`Part ${part.part} copied`)
    setTimeout(() => setCopied(false), 1500)
  }
  const first = part.unit_ids[0]?.split("-U").pop()
  const last = part.unit_ids[part.unit_ids.length - 1]?.split("-U").pop()
  return (
    <div className="rounded-md border">
      <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2">
        <div className="text-sm">
          <span className="font-medium">
            Part {part.part} of {part.of}
          </span>
          <span className="text-muted-foreground ml-2 text-xs">
            {plural(part.unit_ids.length, "unit")}, U{first} to U{last}, SHA-256 <span className="ident">{shortHash(part.sha256, 12)}</span>
          </span>
        </div>
        <div className="flex gap-1.5">
          <Button size="sm" variant="ghost" onClick={() => setOpen((o) => !o)}>
            {open ? "Hide text" : "Show text"}
          </Button>
          <Button size="sm" onClick={copy}>
            {copied ? <Check /> : <Copy />}
            Copy prompt
          </Button>
        </div>
      </div>
      {open && (
        <Textarea readOnly value={part.text} rows={14} className="ident rounded-none border-0 border-t text-xs leading-relaxed" />
      )}
    </div>
  )
}

function Issues({ issues }: { issues: ValidationIssue[] }) {
  return (
    <div className="bg-danger-soft text-destructive rounded-md border border-current/20 p-3 text-sm">
      <p className="font-medium">The answer was rejected: {plural(issues.length, "problem")}. Nothing was imported.</p>
      <ul className="mt-1.5 flex max-h-56 flex-col gap-0.5 overflow-auto text-xs">
        {issues.map((i, k) => (
          <li key={k}>
            <span className="ident">{i.location}</span> <span>{i.message}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

function RejectedAnswerView({ rejected }: { rejected: RejectedAnswer }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="flex flex-col gap-1.5">
      <Button size="sm" variant="ghost" className="self-start" onClick={() => setOpen((o) => !o)}>
        {open ? `Hide the rejected answer of part ${rejected.part}` : `Show the rejected answer of part ${rejected.part}`}
      </Button>
      {open && <pre className="bg-muted ident max-h-64 overflow-auto rounded-md p-2 text-xs whitespace-pre-wrap">{rejected.raw}</pre>}
    </div>
  )
}

type Outcome =
  | { kind: "done"; run: AiRun; journal: string }
  | { kind: "refused"; issues: ValidationIssue[]; rejected: RejectedAnswer[] }
  | { kind: "failed"; message: string }

/** Runs the latest prompt batch through a provider; copy and paste stays the default. */
function RunWithProvider({ id, runs, batch, hasPrompts }: { id: string; runs: AiRun[]; batch: number | undefined; hasPrompts: boolean }) {
  const qc = useQueryClient()
  const registry = useQuery({ queryKey: ["providers"], queryFn: api.providers })
  const entries: ProviderEntry[] = registry.data ?? []
  const choices = runnableProviders(entries)
  const [providerId, setProviderId] = useState("copy_paste")
  const [label, setLabel] = useState<string | null>(null)
  const [form, setForm] = useState({ model: "", temperature: "", seed: "", max_tokens: "" })
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const [journal, setJournal] = useState<string | null>(null)
  const runLabel = label ?? nextRunLabel(runs.map((r) => r.meta.run_label))
  const entry = choices.find((e) => e.id === providerId)

  const run = useMutation({
    mutationFn: () =>
      api.runWithProvider(
        id,
        batch!,
        runBody({ provider_id: providerId, run_label: runLabel, model: form.model, temperature: form.temperature, seed: form.seed, max_tokens: form.max_tokens }),
      ),
    onMutate: () => setOutcome(null),
    onSuccess: (answer) => {
      setOutcome({ kind: "done", run: answer.run, journal: answer.journal_line })
      setJournal(answer.journal_line)
      setLabel(null)
      invalidateInterview(qc, id)
    },
    onError: (e) => {
      if (e instanceof ApiError && (e.issues.length || e.rejected.length)) {
        setOutcome({ kind: "refused", issues: e.issues, rejected: e.rejected })
      } else {
        setOutcome({ kind: "failed", message: e instanceof ApiError ? e.message : String(e) })
      }
    },
  })

  const setField = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => setForm((f) => ({ ...f, [k]: e.target.value }))

  return (
    <Section title="Run with a provider" aside="optional: the answers of the latest batch come from a provider instead of a paste">
      <div className="flex flex-col gap-4">
        <div className="grid gap-3 sm:grid-cols-[14rem_1fr] sm:items-end">
          <Field label="Provider">
            <NativeSelect value={providerId} onChange={(e) => setProviderId(e.target.value)}>
              <option value="copy_paste">Copy and paste</option>
              {choices.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.id}
                </option>
              ))}
            </NativeSelect>
          </Field>
          {entry ? (
            <p className="text-muted-foreground text-xs">
              What leaves the machine: <span className="ident text-foreground">{journal ?? journalLine(entry.description)}</span>
            </p>
          ) : (
            <p className="text-muted-foreground text-xs">Copy each part by hand into the AI tool, then paste its answer in the sections below. Nothing is sent by Coding Studio.</p>
          )}
        </div>

        {!hasPrompts || batch === undefined ? (
          <p className="text-muted-foreground text-xs">Generate the prompts first.</p>
        ) : (
          entry && (
            <form
              className="grid gap-2.5 sm:grid-cols-3"
              onSubmit={(e) => {
                e.preventDefault()
                if (runLabel.trim()) run.mutate()
              }}
            >
              <Field label="Run label">
                <Input value={runLabel} onChange={(e) => setLabel(e.target.value)} required />
              </Field>
              <Field label="Model">
                <Input value={form.model} onChange={setField("model")} placeholder="empty: the provider's own model" />
              </Field>
              <Field label="Temperature">
                <Input type="number" step="0.1" min="0" value={form.temperature} onChange={setField("temperature")} />
              </Field>
              <Field label="Seed">
                <Input type="number" step="1" value={form.seed} onChange={setField("seed")} />
              </Field>
              <Field label="Max tokens">
                <Input type="number" step="1" min="1" value={form.max_tokens} onChange={setField("max_tokens")} />
              </Field>
              <div className="flex items-center gap-3 sm:col-span-3">
                <Button type="submit" disabled={!runLabel.trim() || run.isPending}>
                  {run.isPending ? "Running..." : "Run with provider"}
                </Button>
                <span className="text-muted-foreground text-xs">Every part is sent as prompted, then every answer is checked before anything is kept.</span>
              </div>
            </form>
          )
        )}

        {outcome?.kind === "done" && (
          <p role="status" className="text-sm">
            {outcome.run.complete
              ? `Run ${outcome.run.meta.run_label} complete: ${outcome.run.parts_received.length} of ${outcome.run.parts_expected ?? outcome.run.parts_received.length} parts imported`
              : `Run ${outcome.run.meta.run_label}: ${outcome.run.parts_received.length} of ${outcome.run.parts_expected ?? "?"} parts imported`}
          </p>
        )}
        {outcome?.kind === "refused" && (
          <div className="flex flex-col gap-2">
            <Issues issues={outcome.issues} />
            {outcome.rejected.map((r) => (
              <RejectedAnswerView key={r.part} rejected={r} />
            ))}
          </div>
        )}
        {outcome?.kind === "failed" && (
          <p role="alert" className="text-destructive text-sm">
            {outcome.message}
          </p>
        )}
      </div>
    </Section>
  )
}


function ImportBox({ id, run }: { id: string; run: AiRun }) {
  const qc = useQueryClient()
  const expected = run.parts_expected
  const nextPart = expected ? [...Array(expected).keys()].map((i) => i + 1).find((p) => !run.parts_received.includes(p)) : 1
  const [part, setPart] = useState<number>(nextPart ?? 1)
  const [raw, setRaw] = useState("")
  const [issues, setIssues] = useState<ValidationIssue[]>([])
  const imp = useMutation({
    mutationFn: () => api.importOutput(id, run.run_id, raw, part, expected ?? undefined),
    onSuccess: (r) => {
      setRaw("")
      setIssues([])
      toast.success(`${r.meta.run_label}: part ${part} imported${r.complete ? ", run complete" : ""}`)
      invalidateInterview(qc, id)
      const n = r.parts_expected ? [...Array(r.parts_expected).keys()].map((i) => i + 1).find((p) => !r.parts_received.includes(p)) : undefined
      if (n) setPart(n)
    },
    onError: (e) => {
      if (e instanceof ApiError && e.issues.length) setIssues(e.issues)
      else reportError(e, "Import refused")
    },
  })
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-end gap-2">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Part</span>
          <Input type="number" min={1} max={expected ?? undefined} value={part} onChange={(e) => setPart(Number(e.target.value) || 1)} className="w-20" />
        </label>
        <span className="text-muted-foreground pb-2 text-xs">
          {expected ? `of ${expected}, received ${run.parts_received.join(", ") || "none"}` : "first answer sets the number of parts"}
        </span>
      </div>
      <Textarea
        rows={6}
        className="ident text-xs"
        placeholder="Paste the AI answer here, exactly as returned. Surrounding prose or a code fence is tolerated; the answer itself is checked strictly."
        value={raw}
        onChange={(e) => setRaw(e.target.value)}
      />
      {issues.length > 0 && <Issues issues={issues} />}
      <div>
        <Button disabled={!raw.trim() || imp.isPending} onClick={() => imp.mutate()}>
          {imp.isPending ? "Checking..." : `Import part ${part}`}
        </Button>
      </div>
    </div>
  )
}

function RunForm({ id, runs, batch }: { id: string; runs: AiRun[]; batch: number }) {
  const qc = useQueryClient()
  const [meta, setMeta] = useState({
    run_label: `R${runs.length + 1}`,
    model_name: "",
    model_version: "",
    temperature: "0",
    seed: "",
    context_length: "",
    executed_at: nowLocalInput(),
    notes: "",
  })
  const set = (k: keyof typeof meta) => (e: React.ChangeEvent<HTMLInputElement>) => setMeta((m) => ({ ...m, [k]: e.target.value }))
  const create = useMutation({
    mutationFn: () => {
      const body: AiRunMeta = {
        run_label: meta.run_label.trim(),
        model_name: meta.model_name.trim(),
        model_version: meta.model_version.trim(),
        temperature: meta.temperature === "" ? null : Number(meta.temperature),
        seed: meta.seed.trim() || null,
        context_length: meta.context_length === "" ? null : Number(meta.context_length),
        batch_size: batch || null,
        prompt_id_version: "",
        executed_at: new Date(meta.executed_at).toISOString(),
        notes: meta.notes.trim() || null,
        batch_id: null,
      }
      return api.createRun(id, body)
    },
    onSuccess: (r) => {
      toast.success(`Run ${r.meta.run_label} recorded`)
      setMeta((m) => ({ ...m, run_label: `R${runs.length + 2}` }))
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Run not created"),
  })
  const field = (label: string, k: keyof typeof meta, props: React.ComponentProps<typeof Input> = {}) => (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <Input value={meta[k]} onChange={set(k)} {...props} />
    </label>
  )
  return (
    <form
      className="grid gap-2.5 sm:grid-cols-4"
      onSubmit={(e) => {
        e.preventDefault()
        create.mutate()
      }}
    >
      {field("Run label", "run_label", { required: true })}
      {field("Model name", "model_name", { required: true, placeholder: "e.g. the model shown by the chat tool" })}
      {field("Model version", "model_version")}
      {field("Temperature", "temperature", { type: "number", step: "0.1", min: "0" })}
      {field("Seed", "seed")}
      {field("Context length", "context_length", { type: "number", min: "0" })}
      {field("Executed at", "executed_at", { type: "datetime-local", required: true })}
      {field("Notes", "notes")}
      <div className="sm:col-span-4">
        <Button type="submit" variant="outline" disabled={create.isPending}>
          Record the run
        </Button>
        <span className="text-muted-foreground ml-3 text-xs">
          The eight parameters journalised for every run (P-04). Record the run before pasting its answers.
        </span>
      </div>
    </form>
  )
}

const phase: Phase = 2

export function AiPassTab({ id }: { id: string }) {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["phase", id, phase], queryFn: () => api.phase(id, phase) })
  const prompts = useQuery({ queryKey: ["prompts", id, phase], queryFn: () => api.prompts(id) })
  const runs = useQuery({ queryKey: ["runs", id, phase], queryFn: () => api.runs(id) })
  const [batch, setBatch] = useState(60)

  const gen = useMutation({
    mutationFn: () => api.generatePrompts(id, batch),
    onSuccess: (parts) => {
      qc.setQueryData<PromptPart[]>(["prompts", id, phase], (old) => [...(old ?? []), ...parts])
      toast.success(`${plural(parts.length, "prompt part")} generated and recorded`)
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => {
      if (e instanceof ApiError && e.kind === "validation" && e.message.includes("not yet coded")) {
        const ids = uncodedUnitIds(e.message)
        toast.error(`Prompts not generated: ${plural(ids.length, "unit")} of the batch not yet coded`, {
          description: [
            ids.slice(0, 8).join(", ") + (ids.length > 8 ? `, and ${ids.length - 8} more` : ""),
            "Code every unit of the batch first (the snapshot is taken now).",
          ]
            .filter(Boolean)
            .join("\n"),
        })
        return
      }
      reportError(e, "Prompts not generated")
    },
  })
  const parts = useMemo(() => latestSet(prompts.data ?? []), [prompts.data])

  if (state.isLoading) return <Skeleton className="h-64 w-full" />
  if (state.error) return <Banner tone="danger" title="Phase state unavailable">{state.error.message}</Banner>
  const st = state.data!
  if (st.guard.n_units === 0)
    return (
      <EmptyState
        title="No unit to challenge yet"
        action={
          <Button asChild>
            <a href={interviewHref(id, "coding", 1)}>Go to phase 1</a>
          </Button>
        }
      >
        The AI challenges your concrete codes of phase 2. Pre-classify units in phase 1, then code them in phase 2.
      </EmptyState>
    )

  return (
    <div className="flex flex-col gap-5">
      {st.guard.complete ? (
        <Banner tone="ok" title="Your phase 2 coding covers every unit in scope">
          AI answers can be imported. You can still change your codes afterwards: each change is flagged as revised after the AI answer.
        </Banner>
      ) : (
        <Banner
          tone="lock"
          title={`Blind lock: ${plural(st.guard.missing.length, "unit")} left to code before any AI answer can be imported`}
          action={
            <Button size="sm" variant="outline" asChild>
              <a href={interviewHref(id, "coding", 2)}>Continue coding</a>
            </Button>
          }
        >
          Prompts can be generated now: they contain the units and the codebook, never your codes, phase 1 contexts, labels or memos.
        </Banner>
      )}

      <Section title="1. Generate the prompt" aside="concrete codes, grouped by TOE context">
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Units per part (0 = one part)</span>
            <Input type="number" min={0} value={batch} onChange={(e) => setBatch(Math.max(0, Number(e.target.value) || 0))} className="w-28" />
          </label>
          <Button onClick={() => gen.mutate()} disabled={gen.isPending}>
            {gen.isPending ? "Generating..." : "Generate prompts"}
          </Button>
          <p className="text-muted-foreground max-w-prose text-xs">
            Every generation is recorded with its SHA-256. Paste each part in a new conversation of the AI tool, at temperature 0 when it lets you choose.
          </p>
        </div>
        <p className="text-muted-foreground mt-2 text-xs">Generating the prompt freezes your current coding as the reference of the comparison.</p>
        {parts.length > 0 && (
          <div className="mt-4 flex flex-col gap-2">
            {parts.map((p) => (
              <PromptCard key={`${p.sha256}-${p.part}`} part={p} />
            ))}
          </div>
        )}
      </Section>

      <RunWithProvider id={id} runs={runs.data ?? []} batch={parts.length ? parts[parts.length - 1].batch_id : undefined} hasPrompts={parts.length > 0} />

      <Section title="2. Record each run, then import its answers" aside={runs.data ? plural(runs.data.length, "run") : undefined}>
        {runs.data && runs.data.length > 0 && (
          <div className="-mx-4 -mt-4 mb-4 overflow-x-auto border-b">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Run</TableHead>
                  <TableHead>Model</TableHead>
                  <TableHead>Temperature</TableHead>
                  <TableHead>Executed</TableHead>
                  <TableHead>Parts</TableHead>
                  <TableHead className="pr-4">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {runs.data.map((r) => (
                  <TableRow key={r.run_id}>
                    <TableCell className="pl-4 font-medium">{r.meta.run_label}</TableCell>
                    <TableCell className="text-xs">
                      {r.meta.model_name} <span className="text-muted-foreground">{r.meta.model_version}</span>
                    </TableCell>
                    <TableCell className="text-xs tabular-nums">{r.meta.temperature ?? "not set"}</TableCell>
                    <TableCell className="text-muted-foreground text-xs">{dateTime(r.meta.executed_at)}</TableCell>
                    <TableCell className="text-xs tabular-nums">
                      {r.parts_received.length}
                      {r.parts_expected ? ` of ${r.parts_expected}` : ""}
                    </TableCell>
                    <TableCell className="pr-4">
                      {r.complete ? <Badge className="bg-ok-soft text-ok">complete</Badge> : <Badge variant="outline">waiting for answers</Badge>}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
        {st.guard.complete &&
          (runs.data ?? [])
            .filter((r) => !r.complete)
            .map((r) => (
              <div key={r.run_id} className="mb-5 border-b pb-5">
                <h3 className="mb-2 text-sm font-semibold">Paste the answer of {r.meta.run_label}</h3>
                <ImportBox id={id} run={r} />
              </div>
            ))}
        {(runs.data?.length ?? 0) > 0 && <h3 className="mb-2 text-sm font-semibold">Record another run</h3>}
        <RunForm key={runs.data?.length ?? 0} id={id} runs={runs.data ?? []} batch={batch} />
      </Section>
    </div>
  )
}
