// Revoir, part "Revue IA" (docs/guide/fr/03_revoir.md, actions 4 and 5): generate
// the prompt (which freezes the human coding), copy its parts, name the model,
// paste or run the answers, and keep the list of runs.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { AnswerIssues, RejectedRaw } from "@/components/revoir/answer-issues"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { api, ApiError } from "@/lib/api"
import { plural, shortHash } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import { isoNow, latestSet, partTitle, runLine, runState, shortStamp } from "@/lib/review-model"
import { reviewApi } from "@/lib/review-api"
import { journalLine, nextRunLabel, runBody, runnableProviders } from "@/lib/settings"
import type { AiRun, GuardStatus, PromptPart, ProviderEntry, RejectedAnswer, ValidationIssue } from "@/lib/types"

import "./revoir.css"

const GUIDE = "#/guide/revoir"

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = document.createElement("textarea")
    ta.value = text
    document.body.appendChild(ta)
    ta.select()
    document.execCommand("copy")
    ta.remove()
  }
}

/** The batch a run answers. */
const batchOf = (r: AiRun) => r.batch_id ?? r.meta.batch_id ?? undefined

export function RevueIa({ id, guard, prompts, runs }: { id: string; guard: GuardStatus; prompts: PromptPart[]; runs: AiRun[] }) {
  const parts = useMemo(() => latestSet(prompts), [prompts])
  const batch = parts[0]?.batch_id
  return (
    <div className="page">
      <div className="rv-col">
        <p className="small l2" style={{ margin: "0 2px" }}>
          La revue IA est facultative. Le modèle passe après vous et ne reçoit jamais votre codage : il lit les tours et le codebook, puis ses codes sont comparés aux vôtres. <a href={GUIDE}>Lire la section Revoir du guide</a>.
        </p>
        <PromptPanel id={id} guard={guard} parts={parts} />
        <ImportPanel id={id} guard={guard} parts={parts} runs={runs} />
        <ProviderPanel id={id} runs={runs} batch={batch} hasPrompts={parts.length > 0} />
        <RunsPanel id={id} runs={runs} />
      </div>
    </div>
  )
}

// ----------------------------------------------------------------- prompt

function PromptPanel({ id, guard, parts }: { id: string; guard: GuardStatus; parts: PromptPart[] }) {
  const qc = useQueryClient()
  const [size, setSize] = useState(60)
  const [copied, setCopied] = useState<string[]>([])
  const [shown, setShown] = useState<string | null>(null)
  const snapshots = useQuery({ queryKey: ["snapshots", id], queryFn: () => api.snapshots(id) })

  const gen = useMutation({
    mutationFn: () => api.generatePrompts(id, size),
    onSuccess: (made) => {
      qc.setQueryData<PromptPart[]>(["prompts", id, 2], (old) => [...(old ?? []), ...made])
      setCopied([])
      toast.success(`Prompt généré en ${plural(made.length, "partie")}. Votre codage est figé.`)
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Prompt non généré"),
  })

  const batch = parts[0]?.batch_id
  const snap = snapshots.data?.find((s) => s.batch_id === batch)
  const nUnits = parts.reduce((n, p) => n + p.unit_ids.length, 0)
  const blocked = !guard.complete
  const copy = async (p: PromptPart) => {
    await copyText(p.text)
    setCopied((c) => (c.includes(p.sha256) ? c : [...c, p.sha256]))
    toast.success(`Partie ${partTitle(p)} copiée`)
  }

  return (
    <section aria-label="Générer le prompt">
      <h2 className="gh">Générer le prompt</h2>
      <div className="grp">
        <div className="row">
          <div className="note-warn" style={{ flex: 1 }}>
            <span>
              Générer le prompt fige votre codage de cet entretien, tel qu'il est à cet instant. C'est la référence de la comparaison, même si vous changez d'avis ensuite. Vos codes ne figurent jamais dans le prompt.
            </span>
          </div>
        </div>
        <div className="row">
          <span className="k">
            Unités par partie
            <small>0 pour une seule partie</small>
          </span>
          <Input aria-label="Unités par partie" type="number" min={0} value={size} onChange={(e) => setSize(Math.max(0, Number(e.target.value) || 0))} style={{ width: 72, textAlign: "right" }} />
          <Button variant="default" disabled={blocked || gen.isPending} onClick={() => gen.mutate()}>
            {gen.isPending ? "Génération…" : "Générer le prompt"}
          </Button>
        </div>
        {blocked && (
          <div className="row">
            <span className="k small l2">
              {guard.n_units === 0
                ? "Aucun tour pertinent à relire pour l'instant. Codez d'abord l'entretien."
                : `${plural(guard.missing.length, "tour pertinent")} pas encore codé${guard.missing.length < 2 ? "" : "s"}. Le prompt se génère quand tous les tours pertinents sont codés.`}
            </span>
            <Button variant="outline" asChild>
              <a href={interviewHref(id, "coding")}>Continuer le codage</a>
            </Button>
          </div>
        )}
      </div>

      {parts.length > 0 && (
        <>
          <p className="gf" style={{ marginTop: 10 }}>
            {[
              snap ? shortStamp(snap.created_at).replace(" ", " à ") : null,
              `codebook ${shortHash(parts[0].codebook_hash, 8)}`,
              plural(nUnits, "unité"),
              plural(parts.length, "partie"),
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
          <div className="grp" style={{ marginTop: 6 }}>
            {parts.map((p) => (
              <div key={p.sha256} className="row" style={{ flexWrap: "wrap" }}>
                <span className="k">
                  Partie {partTitle(p)}
                  <small>
                    {plural(p.unit_ids.length, "unité")} · SHA-256 <span className="mono">{shortHash(p.sha256, 12)}</span>
                  </small>
                </span>
                <Button variant="ghost" onClick={() => setShown((s) => (s === p.sha256 ? null : p.sha256))}>
                  {shown === p.sha256 ? "Masquer le texte" : "Afficher le texte"}
                </Button>
                <Button variant="outline" onClick={() => copy(p)}>
                  Copier la partie {partTitle(p)}
                </Button>
                {shown === p.sha256 && <Textarea readOnly value={p.text} rows={12} className="mono" style={{ flexBasis: "100%" }} aria-label={`Texte de la partie ${partTitle(p)}`} />}
              </div>
            ))}
          </div>
          <p className="gf" role="status">
            {copied.length === 0 ? "Aucune partie copiée." : `${copied.length} sur ${parts.length} ${parts.length < 2 ? "partie copiée" : "parties copiées"}.`} Collez chaque partie dans une nouvelle conversation, à température 0 quand l'outil le permet.
          </p>
          <PseudonymList id={id} />
        </>
      )}
    </section>
  )
}

/** The names replaced before sending; absent on a server without the route. */
function PseudonymList({ id }: { id: string }) {
  const list = useQuery({ queryKey: ["pseudonymisation", id], queryFn: () => reviewApi.pseudonymisation(id) })
  if (!list.data) return null
  const n = list.data.entries.length
  return (
    <details className="grp" style={{ padding: "8px 12px" }}>
      <summary style={{ cursor: "pointer" }}>
        Pseudonymisation : {n === 0 ? "aucun nom à remplacer" : plural(n, "nom remplacé", "noms remplacés")} avant envoi
      </summary>
      {n > 0 && (
        <ul className="small" style={{ margin: "8px 0 0", paddingLeft: 18 }}>
          {list.data.entries.map((e) => (
            <li key={`${e.kind}-${e.original}`}>
              {e.original} <span className="l2">devient</span> {e.pseudonym} <span className="l2">({e.kind})</span>
            </li>
          ))}
        </ul>
      )}
      <p className="gf">Relisez cette liste avant d'envoyer le prompt à un fournisseur hébergé.</p>
    </details>
  )
}

// ----------------------------------------------------------------- import

function ImportPanel({ id, guard, parts, runs }: { id: string; guard: GuardStatus; parts: PromptPart[]; runs: AiRun[] }) {
  const qc = useQueryClient()
  const batch = parts[0]?.batch_id
  const batchRuns = runs.filter((r) => batchOf(r) === batch)
  const target = batchRuns.find((r) => !r.complete)
  const done = batchRuns.find((r) => r.complete)
  const [again, setAgain] = useState(false)
  const [model, setModel] = useState("")
  const [createdId, setCreatedId] = useState<number | null>(null)
  const [raws, setRaws] = useState<Record<number, string>>({})
  const [issues, setIssues] = useState<Record<number, ValidationIssue[]>>({})
  const modelName = (target?.meta.model_name ?? model).trim()

  const imp = useMutation({
    mutationFn: async (p: PromptPart) => {
      let runId = target?.run_id ?? createdId
      if (runId === null || runId === undefined) {
        const created = await api.createRun(id, {
          run_label: nextRunLabel(runs.map((r) => r.meta.run_label)),
          model_name: modelName,
          model_version: "",
          temperature: null,
          seed: null,
          context_length: null,
          batch_size: null,
          prompt_id_version: p.prompt_version,
          executed_at: isoNow(),
          notes: null,
          batch_id: batch ?? null,
        })
        runId = created.run_id
        setCreatedId(runId)
      }
      return api.importOutput(id, runId, raws[p.part] ?? "", p.part, p.of)
    },
    onSuccess: (run, p) => {
      setRaws((r) => ({ ...r, [p.part]: "" }))
      setIssues((i) => ({ ...i, [p.part]: [] }))
      toast.success(`${run.meta.run_label} : partie ${partTitle(p)} importée${run.complete ? ", run complet" : ""}`)
      if (run.complete) {
        setCreatedId(null)
        setAgain(false)
        setModel("")
      }
    },
    onError: (e, p) => {
      if (e instanceof ApiError && e.issues.length) setIssues((i) => ({ ...i, [p.part]: e.issues }))
      else reportError(e, "Réponse non importée")
    },
    onSettled: () => invalidateInterview(qc, id),
  })

  return (
    <section aria-label="Importer la réponse du modèle">
      <h2 className="gh">Importer la réponse du modèle</h2>
      {parts.length === 0 ? (
        <p className="gf">Générez d'abord le prompt.</p>
      ) : done && !target && !again ? (
        <div className="grp">
          <div className="row">
            <span className="k">
              Run {done.meta.run_label} complet
              <small>Un run complet suffit. Un second run n'est utile que pour comparer deux modèles.</small>
            </span>
            <Button variant="outline" onClick={() => setAgain(true)}>
              Importer un autre run
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div className="grp">
            <div className="row">
              <label className="k" htmlFor="rv-model">
                Modèle utilisé
                <small>Nommez le modèle tel que l'outil le désigne. Il est journalisé avec le run.</small>
              </label>
              <Input id="rv-model" value={target ? target.meta.model_name : model} disabled={!!target} onChange={(e) => setModel(e.target.value)} style={{ width: 240 }} />
            </div>
          </div>
          {!guard.complete && <p className="gf">Tous les tours pertinents doivent être codés avant d'importer une réponse : le modèle passe après vous.</p>}
          <div className="grp" style={{ marginTop: 8 }}>
            {parts.map((p) => {
              const received = !!target?.parts_received.includes(p.part)
              const label = `Réponse à la partie ${partTitle(p)}`
              const raw = raws[p.part] ?? ""
              return (
                <div key={p.sha256} className="row" role="group" aria-label={label} style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
                  <label className="k" htmlFor={`rv-raw-${p.part}`}>
                    {label}
                    {received && <small>Importée.</small>}
                  </label>
                  <Textarea
                    id={`rv-raw-${p.part}`}
                    rows={5}
                    className="mono"
                    disabled={received}
                    placeholder="Collez ici la réponse du modèle, telle quelle. Une réponse mal formée est refusée en bloc, avec la liste des problèmes."
                    value={raw}
                    onChange={(e) => setRaws((r) => ({ ...r, [p.part]: e.target.value }))}
                  />
                  {(issues[p.part]?.length ?? 0) > 0 && <AnswerIssues issues={issues[p.part]} />}
                  {!received && (
                    <div>
                      <Button variant="default" disabled={!raw.trim() || !modelName || !guard.complete || imp.isPending} onClick={() => imp.mutate(p)}>
                        {imp.isPending && imp.variables?.part === p.part ? "Contrôle…" : "Importer la réponse"}
                      </Button>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
          {!modelName && <p className="gf">Nommez le modèle utilisé avant d'importer une réponse.</p>}
        </>
      )}
    </section>
  )
}

// --------------------------------------------------------------- provider

type Outcome =
  | { kind: "done"; run: AiRun; journal: string }
  | { kind: "refused"; issues: ValidationIssue[]; rejected: RejectedAnswer[] }
  | { kind: "failed"; message: string }

/** Runs the latest batch through a provider; copy and paste stays the reference path. */
function ProviderPanel({ id, runs, batch, hasPrompts }: { id: string; runs: AiRun[]; batch: number | undefined; hasPrompts: boolean }) {
  const qc = useQueryClient()
  const registry = useQuery({ queryKey: ["providers"], queryFn: api.providers })
  const choices = runnableProviders((registry.data ?? []) as ProviderEntry[])
  const [providerId, setProviderId] = useState("copy_paste")
  const [model, setModel] = useState("")
  const [more, setMore] = useState({ temperature: "", seed: "", max_tokens: "" })
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const entry = choices.find((e) => e.id === providerId)
  const label = nextRunLabel(runs.map((r) => r.meta.run_label))

  const run = useMutation({
    mutationFn: () => api.runWithProvider(id, batch!, runBody({ provider_id: providerId, run_label: label, model, ...more })),
    onMutate: () => setOutcome(null),
    onSuccess: (a) => setOutcome({ kind: "done", run: a.run, journal: a.journal_line }),
    onError: (e) => {
      if (e instanceof ApiError && (e.issues.length || e.rejected.length)) setOutcome({ kind: "refused", issues: e.issues, rejected: e.rejected })
      else setOutcome({ kind: "failed", message: e instanceof ApiError ? e.message : String(e) })
    },
    onSettled: () => invalidateInterview(qc, id),
  })

  return (
    <section aria-label="Lancer avec un fournisseur">
      <h2 className="gh">Lancer avec un fournisseur</h2>
      <div className="grp">
        <div className="row">
          <label className="k" htmlFor="rv-provider">
            Fournisseur
            <small>Facultatif : il fait l'aller-retour à votre place. Le copier-coller reste la référence.</small>
          </label>
          <NativeSelect id="rv-provider" value={providerId} onChange={(e) => setProviderId(e.target.value)} style={{ width: 200 }}>
            <option value="copy_paste">Copier-coller</option>
            {choices.map((c) => (
              <option key={c.id} value={c.id}>
                {c.id}
              </option>
            ))}
          </NativeSelect>
        </div>
        {entry ? (
          <>
            <div className="row">
              <span className="k">
                Ce qui quitte la machine
                <small className="mono">{journalLine(entry.description)}</small>
              </span>
            </div>
            {!hasPrompts || batch === undefined ? (
              <div className="row">
                <span className="k small l2">Générez d'abord le prompt.</span>
              </div>
            ) : (
              <>
                <div className="row">
                  <label className="k" htmlFor="rv-pmodel">
                    Modèle
                    <small>Vide : le modèle propre du fournisseur.</small>
                  </label>
                  <Input id="rv-pmodel" value={model} onChange={(e) => setModel(e.target.value)} style={{ width: 240 }} />
                </div>
                <details className="row" style={{ display: "block" }}>
                  <summary style={{ cursor: "pointer" }}>Paramètres</summary>
                  <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
                    <Input aria-label="Température" placeholder="Température" type="number" step="0.1" min="0" value={more.temperature} onChange={(e) => setMore((m) => ({ ...m, temperature: e.target.value }))} style={{ width: 120 }} />
                    <Input aria-label="Graine" placeholder="Graine" type="number" step="1" value={more.seed} onChange={(e) => setMore((m) => ({ ...m, seed: e.target.value }))} style={{ width: 120 }} />
                    <Input aria-label="Jetons maximum" placeholder="Jetons maximum" type="number" step="1" min="1" value={more.max_tokens} onChange={(e) => setMore((m) => ({ ...m, max_tokens: e.target.value }))} style={{ width: 140 }} />
                  </div>
                </details>
                <div className="row">
                  <span className="k small l2">L'envoi est un geste explicite et journalisé. Chaque réponse est contrôlée avant que rien ne soit gardé.</span>
                  <Button variant="default" disabled={run.isPending} onClick={() => run.mutate()}>
                    {run.isPending ? "Exécution…" : "Lancer"}
                  </Button>
                </div>
              </>
            )}
          </>
        ) : (
          <div className="row">
            <span className="k small l2">Avec le copier-coller, Coding Studio n'envoie rien : copiez chaque partie à la main, puis collez la réponse ci-dessus.</span>
          </div>
        )}
      </div>
      {outcome?.kind === "done" && (
        <p role="status" className="gf">
          Run {outcome.run.meta.run_label} {outcome.run.complete ? "complet" : "reçu"} : {plural(outcome.run.parts_received.length, "partie")} sur {outcome.run.parts_expected ?? "?"} importées. Journal : {outcome.journal}.
        </p>
      )}
      {outcome?.kind === "refused" && (
        <div className="flex flex-col gap-2" style={{ marginTop: 8 }}>
          <AnswerIssues issues={outcome.issues} />
          {outcome.rejected.map((r) => (
            <RejectedRaw key={r.part} rejected={r} />
          ))}
        </div>
      )}
      {outcome?.kind === "failed" && (
        <p role="alert" className="rv-err" style={{ marginTop: 8 }}>
          {outcome.message}
        </p>
      )}
    </section>
  )
}

// ------------------------------------------------------------------- runs

function RunsPanel({ id, runs }: { id: string; runs: AiRun[] }) {
  return (
    <section aria-label="Runs enregistrés">
      <h2 className="gh">Runs enregistrés</h2>
      {runs.length === 0 ? (
        <p className="gf">Aucun run pour l'instant.</p>
      ) : (
        <div className="grp">
          {runs.map((r) => {
            return (
              <div key={r.run_id} className="row" data-state={runState(r).state}>
                <span className="k">{runLine(r)}</span>
                {r.complete && (
                  <a className="small" href={interviewHref(id, "review")}>
                    Arbitrer
                  </a>
                )}
              </div>
            )
          })}
        </div>
      )}
    </section>
  )
}
