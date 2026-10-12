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
import type { AiRun, AiRunMeta, Phase, PromptPart, ValidationIssue } from "@/lib/types"

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
            <span className="ident">{i.location}</span> {i.message}
          </li>
        ))}
      </ul>
    </div>
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
