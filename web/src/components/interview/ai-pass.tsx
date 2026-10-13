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
    toast.success(`Partie ${part.part} copiée`)
    setTimeout(() => setCopied(false), 1500)
  }
  const first = part.unit_ids[0]?.split("-U").pop()
  const last = part.unit_ids[part.unit_ids.length - 1]?.split("-U").pop()
  return (
    <div className="rounded-md border">
      <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2">
        <div className="text-sm">
          <span className="font-medium">
            Partie {part.part} sur {part.of}
          </span>
          <span className="text-muted-foreground ml-2 text-xs">
            {plural(part.unit_ids.length, "unité")}, de U{first} à U{last}, SHA-256 <span className="ident">{shortHash(part.sha256, 12)}</span>
          </span>
        </div>
        <div className="flex gap-1.5">
          <Button size="sm" variant="ghost" onClick={() => setOpen((o) => !o)}>
            {open ? "Masquer le texte" : "Afficher le texte"}
          </Button>
          <Button size="sm" onClick={copy}>
            {copied ? <Check /> : <Copy />}
            Copier le prompt
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
      <p className="font-medium">Réponse refusée : {plural(issues.length, "problème")}. Rien n'a été importé.</p>
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
        {open ? `Masquer la réponse refusée de la partie ${rejected.part}` : `Afficher la réponse refusée de la partie ${rejected.part}`}
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
    <Section title="Lancer avec un fournisseur" aside="facultatif : les réponses du dernier lot viennent d'un fournisseur au lieu d'un collage">
      <div className="flex flex-col gap-4">
        <div className="grid gap-3 sm:grid-cols-[14rem_1fr] sm:items-end">
          <Field label="Fournisseur">
            <NativeSelect value={providerId} onChange={(e) => setProviderId(e.target.value)}>
              <option value="copy_paste">Copier-coller</option>
              {choices.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.id}
                </option>
              ))}
            </NativeSelect>
          </Field>
          {entry ? (
            <p className="text-muted-foreground text-xs">
              Ce qui quitte la machine : <span className="ident text-foreground">{journal ?? journalLine(entry.description)}</span>
            </p>
          ) : (
            <p className="text-muted-foreground text-xs">Copiez chaque partie à la main dans l'outil de votre choix, puis collez sa réponse dans les sections ci-dessous. Coding Studio n'envoie rien.</p>
          )}
        </div>

        {!hasPrompts || batch === undefined ? (
          <p className="text-muted-foreground text-xs">Générez d'abord les prompts.</p>
        ) : (
          entry && (
            <form
              className="grid gap-2.5 sm:grid-cols-3"
              onSubmit={(e) => {
                e.preventDefault()
                if (runLabel.trim()) run.mutate()
              }}
            >
              <Field label="Nom de la passe">
                <Input value={runLabel} onChange={(e) => setLabel(e.target.value)} required />
              </Field>
              <Field label="Modèle">
                <Input value={form.model} onChange={setField("model")} placeholder="vide : le modèle propre du fournisseur" />
              </Field>
              <Field label="Température">
                <Input type="number" step="0.1" min="0" value={form.temperature} onChange={setField("temperature")} />
              </Field>
              <Field label="Graine">
                <Input type="number" step="1" value={form.seed} onChange={setField("seed")} />
              </Field>
              <Field label="Jetons maximum">
                <Input type="number" step="1" min="1" value={form.max_tokens} onChange={setField("max_tokens")} />
              </Field>
              <div className="flex items-center gap-3 sm:col-span-3">
                <Button type="submit" disabled={!runLabel.trim() || run.isPending}>
                  {run.isPending ? "Exécution…" : "Lancer avec le fournisseur"}
                </Button>
                <span className="text-muted-foreground text-xs">Chaque partie est envoyée telle quelle, puis chaque réponse est contrôlée avant que rien ne soit gardé.</span>
              </div>
            </form>
          )
        )}

        {outcome?.kind === "done" && (
          <p role="status" className="text-sm">
            {outcome.run.complete
              ? `Passe ${outcome.run.meta.run_label} complète : ${outcome.run.parts_received.length} parties sur ${outcome.run.parts_expected ?? outcome.run.parts_received.length} importées`
              : `Passe ${outcome.run.meta.run_label} : ${outcome.run.parts_received.length} parties sur ${outcome.run.parts_expected ?? "?"} importées`}
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
      toast.success(`${r.meta.run_label} : partie ${part} importée${r.complete ? ", passe complète" : ""}`)
      invalidateInterview(qc, id)
      const n = r.parts_expected ? [...Array(r.parts_expected).keys()].map((i) => i + 1).find((p) => !r.parts_received.includes(p)) : undefined
      if (n) setPart(n)
    },
    onError: (e) => {
      if (e instanceof ApiError && e.issues.length) setIssues(e.issues)
      else reportError(e, "Import refusé")
    },
  })
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-end gap-2">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Partie</span>
          <Input type="number" min={1} max={expected ?? undefined} value={part} onChange={(e) => setPart(Number(e.target.value) || 1)} className="w-20" />
        </label>
        <span className="text-muted-foreground pb-2 text-xs">
          {expected ? `sur ${expected}, reçues : ${run.parts_received.join(", ") || "aucune"}` : "la première réponse fixe le nombre de parties"}
        </span>
      </div>
      <Textarea
        rows={6}
        className="ident text-xs"
        placeholder="Collez ici la réponse du modèle, telle quelle. Du texte autour ou une clôture de code est toléré ; la réponse elle-même est contrôlée strictement."
        value={raw}
        onChange={(e) => setRaw(e.target.value)}
      />
      {issues.length > 0 && <Issues issues={issues} />}
      <div>
        <Button disabled={!raw.trim() || imp.isPending} onClick={() => imp.mutate()}>
          {imp.isPending ? "Contrôle…" : `Importer la partie ${part}`}
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
      toast.success(`Passe ${r.meta.run_label} enregistrée`)
      setMeta((m) => ({ ...m, run_label: `R${runs.length + 2}` }))
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Passe non créée"),
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
      {field("Nom de la passe", "run_label", { required: true })}
      {field("Nom du modèle", "model_name", { required: true, placeholder: "par exemple le modèle affiché par l'outil de discussion" })}
      {field("Version du modèle", "model_version")}
      {field("Température", "temperature", { type: "number", step: "0.1", min: "0" })}
      {field("Graine", "seed")}
      {field("Longueur de contexte", "context_length", { type: "number", min: "0" })}
      {field("Exécutée le", "executed_at", { type: "datetime-local", required: true })}
      {field("Notes", "notes")}
      <div className="sm:col-span-4">
        <Button type="submit" variant="outline" disabled={create.isPending}>
          Enregistrer la passe
        </Button>
        <span className="text-muted-foreground ml-3 text-xs">
          Les huit paramètres journalisés pour chaque passe (P-04). Enregistrez la passe avant de coller ses réponses.
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
      toast.success(`${plural(parts.length, "partie de prompt", "parties de prompt")} générées et enregistrées`)
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => {
      if (e instanceof ApiError && e.kind === "validation" && e.message.includes("not yet coded")) {
        const ids = uncodedUnitIds(e.message)
        toast.error(`Prompts non générés : ${plural(ids.length, "unité")} du lot pas encore codées`, {
          description: [
            ids.slice(0, 8).join(", ") + (ids.length > 8 ? `, et ${ids.length - 8} autres` : ""),
            "Codez d'abord chaque unité du lot (la capture est prise maintenant).",
          ]
            .filter(Boolean)
            .join("\n"),
        })
        return
      }
      reportError(e, "Prompts non générés")
    },
  })
  const parts = useMemo(() => latestSet(prompts.data ?? []), [prompts.data])

  if (state.isLoading) return <Skeleton className="h-64 w-full" />
  if (state.error) return <Banner tone="danger" title="État de la phase indisponible">{state.error.message}</Banner>
  const st = state.data!
  if (st.guard.n_units === 0)
    return (
      <EmptyState
        title="Aucune unité à challenger pour l'instant"
        action={
          <Button asChild>
            <a href={interviewHref(id, "coding", 1)}>Aller à la phase 1</a>
          </Button>
        }
      >
        Le modèle challenge vos codes concrets de phase 2. Préclassez les unités en phase 1, puis codez-les en phase 2.
      </EmptyState>
    )

  return (
    <div className="flex flex-col gap-5">
      {st.guard.complete ? (
        <Banner tone="ok" title="Votre codage de phase 2 couvre toutes les unités du périmètre">
          Les réponses du modèle peuvent être importées. Vous pouvez encore changer vos codes ensuite : chaque changement est signalé comme révisé après la réponse du modèle.
        </Banner>
      ) : (
        <Banner
          tone="lock"
          title={`Verrou à l'aveugle : ${plural(st.guard.missing.length, "unité")} à coder avant de pouvoir importer une réponse du modèle`}
          action={
            <Button size="sm" variant="outline" asChild>
              <a href={interviewHref(id, "coding", 2)}>Continuer le codage</a>
            </Button>
          }
        >
          Les prompts peuvent être générés dès maintenant : ils contiennent les unités et le codebook, jamais vos codes, vos contextes de phase 1, vos étiquettes ni vos mémos.
        </Banner>
      )}

      <Section title="1. Générer le prompt" aside="codes concrets, groupés par contexte TOE">
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Unités par partie (0 = une seule partie)</span>
            <Input type="number" min={0} value={batch} onChange={(e) => setBatch(Math.max(0, Number(e.target.value) || 0))} className="w-28" />
          </label>
          <Button onClick={() => gen.mutate()} disabled={gen.isPending}>
            {gen.isPending ? "Génération…" : "Générer les prompts"}
          </Button>
          <p className="text-muted-foreground max-w-prose text-xs">
            Chaque génération est enregistrée avec son empreinte SHA-256. Collez chaque partie dans une nouvelle conversation de l'outil, à température 0 quand il le permet.
          </p>
        </div>
        <p className="text-muted-foreground mt-2 text-xs">Générer le prompt fige votre codage actuel comme référence de la comparaison.</p>
        {parts.length > 0 && (
          <div className="mt-4 flex flex-col gap-2">
            {parts.map((p) => (
              <PromptCard key={`${p.sha256}-${p.part}`} part={p} />
            ))}
          </div>
        )}
      </Section>

      <RunWithProvider id={id} runs={runs.data ?? []} batch={parts.length ? parts[parts.length - 1].batch_id : undefined} hasPrompts={parts.length > 0} />

      <Section title="2. Enregistrer chaque passe, puis importer ses réponses" aside={runs.data ? plural(runs.data.length, "passe") : undefined}>
        {runs.data && runs.data.length > 0 && (
          <div className="-mx-4 -mt-4 mb-4 overflow-x-auto border-b">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Passe</TableHead>
                  <TableHead>Modèle</TableHead>
                  <TableHead>Température</TableHead>
                  <TableHead>Exécutée</TableHead>
                  <TableHead>Parties</TableHead>
                  <TableHead className="pr-4">Statut</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {runs.data.map((r) => (
                  <TableRow key={r.run_id}>
                    <TableCell className="pl-4 font-medium">{r.meta.run_label}</TableCell>
                    <TableCell className="text-xs">
                      {r.meta.model_name} <span className="text-muted-foreground">{r.meta.model_version}</span>
                    </TableCell>
                    <TableCell className="text-xs tabular-nums">{r.meta.temperature ?? "non renseignée"}</TableCell>
                    <TableCell className="text-muted-foreground text-xs">{dateTime(r.meta.executed_at)}</TableCell>
                    <TableCell className="text-xs tabular-nums">
                      {r.parts_received.length}
                      {r.parts_expected ? ` sur ${r.parts_expected}` : ""}
                    </TableCell>
                    <TableCell className="pr-4">
                      {r.complete ? <Badge className="bg-ok-soft text-ok">complète</Badge> : <Badge variant="outline">en attente de réponses</Badge>}
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
                <h3 className="mb-2 text-sm font-semibold">Coller la réponse de {r.meta.run_label}</h3>
                <ImportBox id={id} run={r} />
              </div>
            ))}
        {(runs.data?.length ?? 0) > 0 && <h3 className="mb-2 text-sm font-semibold">Enregistrer une autre passe</h3>}
        <RunForm key={runs.data?.length ?? 0} id={id} runs={runs.data ?? []} batch={batch} />
      </Section>
    </div>
  )
}
