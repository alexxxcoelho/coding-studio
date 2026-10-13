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
import { t } from "@/i18n"
import { api, ApiError } from "@/lib/api"
import { shortHash } from "@/lib/format"
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
          {t("revoir.revue_ia.la_revue_ia_est_facultative")} <a href={GUIDE}>{t("common.lire_la_section_revoir_du")}</a>.
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
      toast.success(t("revoir.revue_ia.prompt_genere_en_items_votre", { items: t("revoir.revue_ia.n_parties", { n: made.length }) }))
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, t("revoir.revue_ia.prompt_non_genere")),
  })

  const batch = parts[0]?.batch_id
  const snap = snapshots.data?.find((s) => s.batch_id === batch)
  const nUnits = parts.reduce((n, p) => n + p.unit_ids.length, 0)
  const blocked = !guard.complete
  const copy = async (p: PromptPart) => {
    await copyText(p.text)
    setCopied((c) => (c.includes(p.sha256) ? c : [...c, p.sha256]))
    toast.success(t("revoir.revue_ia.partie_parttitle_copiee", { partTitle: partTitle(p) }))
  }

  return (
    <section aria-label={t("revoir.revue_ia.generer_le_prompt")}>
      <h2 className="gh">{t("revoir.revue_ia.generer_le_prompt")}</h2>
      <div className="grp">
        <div className="row">
          <div className="note-warn" style={{ flex: 1 }}>
            <span>
              {t("revoir.revue_ia.generer_le_prompt_fige_votre")}
            </span>
          </div>
        </div>
        <div className="row">
          <span className="k">
            {t("revoir.revue_ia.unites_par_partie")}
            <small>{t("revoir.revue_ia.0_pour_une_seule_partie")}</small>
          </span>
          <Input aria-label={t("revoir.revue_ia.unites_par_partie")} type="number" min={0} value={size} onChange={(e) => setSize(Math.max(0, Number(e.target.value) || 0))} style={{ width: 72, textAlign: "right" }} />
          <Button variant="default" disabled={blocked || gen.isPending} onClick={() => gen.mutate()}>
            {gen.isPending ? t("revoir.revue_ia.generation") : t("revoir.revue_ia.generer_le_prompt")}
          </Button>
        </div>
        {blocked && (
          <div className="row">
            <span className="k small l2">
              {guard.n_units === 0
                ? t("revoir.revue_ia.aucun_tour_pertinent_a_relire")
                : t("revoir.revue_ia.items_pas_encore_codes_le", { n: guard.missing.length, items: t("revoir.revue_ia.n_tour_pertinents", { n: guard.missing.length }) })}
            </span>
            <Button variant="outline" asChild>
              <a href={interviewHref(id, "coding")}>{t("revoir.revue_ia.continuer_le_codage")}</a>
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
              t("revoir.revue_ia.n_unites", { n: nUnits }),
              t("revoir.revue_ia.n_parties", { n: parts.length }),
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
          <div className="grp" style={{ marginTop: 6 }}>
            {parts.map((p) => (
              <div key={p.sha256} className="row" style={{ flexWrap: "wrap" }}>
                <span className="k">
                  {t("revoir.revue_ia.partie_parttitle", { partTitle: partTitle(p) })}
                  <small>
                    {t("revoir.revue_ia.items_sha_256", { items: t("common.n_unites", { n: p.unit_ids.length }) })} <span className="mono">{shortHash(p.sha256, 12)}</span>
                  </small>
                </span>
                <Button variant="ghost" onClick={() => setShown((s) => (s === p.sha256 ? null : p.sha256))}>
                  {shown === p.sha256 ? t("revoir.revue_ia.masquer_le_texte") : t("revoir.revue_ia.afficher_le_texte")}
                </Button>
                <Button variant="outline" onClick={() => copy(p)}>
                  {t("revoir.revue_ia.copier_la_partie_parttitle", { partTitle: partTitle(p) })}
                </Button>
                {shown === p.sha256 && <Textarea readOnly value={p.text} rows={12} className="mono" style={{ flexBasis: "100%" }} aria-label={t("revoir.revue_ia.texte_de_la_partie_parttitle", { partTitle: partTitle(p) })} />}
              </div>
            ))}
          </div>
          <p className="gf" role="status">
            {copied.length === 0 ? t("revoir.revue_ia.aucune_partie_copiee") : t("revoir.revue_ia.length_sur_n_parties_copiees", { n: parts.length, length: copied.length })} {t("revoir.revue_ia.collez_chaque_partie_dans_une")}
          </p>
          <PseudonymList id={id} />
        </>
      )}
    </section>
  )
}

/** The names replaced before sending, with the state of the switch and the digest recorded with the prompt. */
function PseudonymList({ id }: { id: string }) {
  const list = useQuery({ queryKey: ["pseudonymisation", id], queryFn: () => reviewApi.pseudonymisation(id) })
  if (!list.data) return null
  const d = list.data
  const n = d.entries.length
  return (
    <details className="grp" style={{ padding: "8px 12px" }}>
      <summary style={{ cursor: "pointer" }}>
        {d.enabled
          ? `Pseudonymisation active : ${n === 0 ? t("revoir.revue_ia.aucun_nom_de_la_liste") : t("revoir.revue_ia.n_noms_remplaces", { n })} avant envoi`
          : t("revoir.revue_ia.pseudonymisation_inactive_les_noms_p")}
      </summary>
      {d.enabled && n > 0 && (
        <ul className="small" style={{ margin: "8px 0 0", paddingLeft: 18 }}>
          {d.entries.map((e) => (
            <li key={`${e.kind}-${e.original}`}>
              {e.original} <span className="l2">{t("revoir.revue_ia.devient")}</span> {e.pseudonym} <span className="l2">({e.kind === "person" ? t("revoir.revue_ia.personne") : t("revoir.revue_ia.organisation")}, {t("revoir.revue_ia.n_remplacements", { n: e.count ?? 0 })})</span>
            </li>
          ))}
        </ul>
      )}
      <p className="gf">
        {d.enabled ? t("revoir.revue_ia.liste_de_items_empreinte_shorthash", { items: t("revoir.revue_ia.n_noms", { n: d.n_list_entries }), shortHash: shortHash(d.digest, 12) }) : ""}
        {d.note} <a href="#/settings">{t("revoir.revue_ia.modifier_la_liste_dans_les")}</a>.
      </p>
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
      else reportError(e, t("revoir.revue_ia.reponse_non_importee"))
    },
    onSettled: () => invalidateInterview(qc, id),
  })

  return (
    <section aria-label={t("revoir.revue_ia.importer_la_reponse_du_modele")}>
      <h2 className="gh">{t("revoir.revue_ia.importer_la_reponse_du_modele")}</h2>
      {parts.length === 0 ? (
        <p className="gf">{t("revoir.revue_ia.generez_d_abord_le_prompt")}</p>
      ) : done && !target && !again ? (
        <div className="grp">
          <div className="row">
            <span className="k">
              {t("revoir.revue_ia.run_run_label_complet", { run_label: done.meta.run_label })}
              <small>{t("revoir.revue_ia.un_run_complet_suffit_un")}</small>
            </span>
            <Button variant="outline" onClick={() => setAgain(true)}>
              {t("revoir.revue_ia.importer_un_autre_run")}
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div className="grp">
            <div className="row">
              <label className="k" htmlFor="rv-model">
                {t("revoir.revue_ia.modele_utilise")}
                <small>{t("revoir.revue_ia.nommez_le_modele_tel_que")}</small>
              </label>
              <Input id="rv-model" value={target ? target.meta.model_name : model} disabled={!!target} onChange={(e) => setModel(e.target.value)} style={{ width: 240 }} />
            </div>
          </div>
          {!guard.complete && <p className="gf">{t("revoir.revue_ia.tous_les_tours_pertinents_doivent")}</p>}
          <div className="grp" style={{ marginTop: 8 }}>
            {parts.map((p) => {
              const received = !!target?.parts_received.includes(p.part)
              const label = t("revoir.revue_ia.reponse_a_la_partie_parttitle", { partTitle: partTitle(p) })
              const raw = raws[p.part] ?? ""
              return (
                <div key={p.sha256} className="row" role="group" aria-label={label} style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
                  <label className="k" htmlFor={`rv-raw-${p.part}`}>
                    {label}
                    {received && <small>{t("revoir.revue_ia.importee")}</small>}
                  </label>
                  <Textarea
                    id={`rv-raw-${p.part}`}
                    rows={5}
                    className="mono"
                    disabled={received}
                    placeholder={t("revoir.revue_ia.collez_ici_la_reponse_du")}
                    value={raw}
                    onChange={(e) => setRaws((r) => ({ ...r, [p.part]: e.target.value }))}
                  />
                  {(issues[p.part]?.length ?? 0) > 0 && <AnswerIssues issues={issues[p.part]} />}
                  {!received && (
                    <div>
                      <Button variant="default" disabled={!raw.trim() || !modelName || !guard.complete || imp.isPending} onClick={() => imp.mutate(p)}>
                        {imp.isPending && imp.variables?.part === p.part ? t("revoir.revue_ia.controle") : t("revoir.revue_ia.importer_la_reponse")}
                      </Button>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
          {!modelName && <p className="gf">{t("revoir.revue_ia.nommez_le_modele_utilise_avant")}</p>}
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
    <section aria-label={t("revoir.revue_ia.lancer_avec_un_fournisseur")}>
      <h2 className="gh">{t("revoir.revue_ia.lancer_avec_un_fournisseur")}</h2>
      <div className="grp">
        <div className="row">
          <label className="k" htmlFor="rv-provider">
            {t("revoir.revue_ia.fournisseur")}
            <small>{t("revoir.revue_ia.facultatif_il_fait_l_aller")}</small>
          </label>
          <NativeSelect id="rv-provider" value={providerId} onChange={(e) => setProviderId(e.target.value)} style={{ width: 200 }}>
            <option value="copy_paste">{t("revoir.revue_ia.copier_coller")}</option>
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
                {t("revoir.revue_ia.ce_qui_quitte_la_machine")}
                <small className="mono">{journalLine(entry.description)}</small>
              </span>
            </div>
            {!hasPrompts || batch === undefined ? (
              <div className="row">
                <span className="k small l2">{t("revoir.revue_ia.generez_d_abord_le_prompt")}</span>
              </div>
            ) : (
              <>
                <div className="row">
                  <label className="k" htmlFor="rv-pmodel">
                    {t("common.modele")}
                    <small>{t("revoir.revue_ia.vide_le_modele_propre_du")}</small>
                  </label>
                  <Input id="rv-pmodel" value={model} onChange={(e) => setModel(e.target.value)} style={{ width: 240 }} />
                </div>
                <details className="row" style={{ display: "block" }}>
                  <summary style={{ cursor: "pointer" }}>{t("revoir.revue_ia.parametres")}</summary>
                  <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
                    <Input aria-label={t("revoir.revue_ia.temperature")} placeholder={t("revoir.revue_ia.temperature")} type="number" step="0.1" min="0" value={more.temperature} onChange={(e) => setMore((m) => ({ ...m, temperature: e.target.value }))} style={{ width: 120 }} />
                    <Input aria-label={t("revoir.revue_ia.graine")} placeholder={t("revoir.revue_ia.graine")} type="number" step="1" value={more.seed} onChange={(e) => setMore((m) => ({ ...m, seed: e.target.value }))} style={{ width: 120 }} />
                    <Input aria-label={t("revoir.revue_ia.jetons_maximum")} placeholder={t("revoir.revue_ia.jetons_maximum")} type="number" step="1" min="1" value={more.max_tokens} onChange={(e) => setMore((m) => ({ ...m, max_tokens: e.target.value }))} style={{ width: 140 }} />
                  </div>
                </details>
                <div className="row">
                  <span className="k small l2">{t("revoir.revue_ia.l_envoi_est_un_geste")}</span>
                  <Button variant="default" disabled={run.isPending} onClick={() => run.mutate()}>
                    {run.isPending ? t("revoir.revue_ia.execution") : t("revoir.revue_ia.lancer")}
                  </Button>
                </div>
              </>
            )}
          </>
        ) : (
          <div className="row">
            <span className="k small l2">{t("revoir.revue_ia.avec_le_copier_coller_coding")}</span>
          </div>
        )}
      </div>
      {outcome?.kind === "done" && (
        <p role="status" className="gf">
          {t("revoir.revue_ia.run_run_label", { run_label: outcome.run.meta.run_label })} {outcome.run.complete ? t("common.complet") : t("revoir.revue_ia.recu")} {t("revoir.revue_ia.items_sur", { items: t("revoir.revue_ia.n_parties", { n: outcome.run.parts_received.length }) })} {outcome.run.parts_expected ?? "?"} {t("revoir.revue_ia.importees_journal_journal", { journal: outcome.journal })}
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
    <section aria-label={t("revoir.revue_ia.runs_enregistres")}>
      <h2 className="gh">{t("revoir.revue_ia.runs_enregistres")}</h2>
      {runs.length === 0 ? (
        <p className="gf">{t("revoir.revue_ia.aucun_run_pour_l_instant")}</p>
      ) : (
        <div className="grp">
          {runs.map((r) => {
            return (
              <div key={r.run_id} className="row" data-state={runState(r).state}>
                <span className="k">{runLine(r)}</span>
                {r.complete && (
                  <a className="small" href={interviewHref(id, "review")}>
                    {t("revoir.revue_ia.arbitrer")}
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
