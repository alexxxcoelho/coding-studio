// Model providers: the registry (copy and paste first, then the configured
// providers), their edition, the isolation probe of command providers and the
// ChatGPT sign-in. Nothing here shows or stores a key: an API key is named by
// its environment variable.

import { useEffect, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { Section } from "@/components/bits"
import { Alerts, Field, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api, ApiError } from "@/lib/api"
import {
  configFromEntries,
  emptyProviderForm,
  entryFromProviderForm,
  formFromProvider,
  journalLine,
  removeProvider,
  upsertProvider,
  validateProviderForm,
  type ProviderForm,
  type ProviderType,
} from "@/lib/settings"
import type { IsolationReport, OAuthStatus, ProviderEntry, ProvidersConfig } from "@/lib/types"

const KIND_LABEL: Record<ProviderEntry["kind"], string> = {
  get copy_paste() { return t("settings.model_providers.manuel") },
  get cli() { return t("settings.model_providers.ligne_de_commande") },
  open_ai_compatible: "API",
  get oauth_chat_gpt() { return t("settings.model_providers.compte_chatgpt") },
}

const types = (): [ProviderType, string][] => [
  ["cli", t("settings.model_providers.ligne_de_commande")],
  ["open_ai_compatible", t("settings.model_providers.api_compatible_openai")],
  ["oauth_chat_gpt", t("settings.model_providers.compte_chatgpt")],
]

/** Defaults filled in by the server when a field is left out. */
const EMPTY_FORM = (type: ProviderType) => emptyProviderForm(type)

export function ProvidersSection({ pollMs = 1500 }: { pollMs?: number }) {
  const registry = useQuery({ queryKey: ["providers"], queryFn: api.providers })
  return (
    <Section title={t("settings.model_providers.fournisseurs_de_modeles")} aside={t("settings.model_providers.le_copier_coller_reste_toujours")}>
      {registry.isLoading ? (
        <Skeleton className="h-32 w-full" />
      ) : registry.error ? (
        <Alerts messages={[registry.error.message]} />
      ) : (
        <ProviderRegistry entries={registry.data ?? []} pollMs={pollMs} />
      )}
    </Section>
  )
}

function ProviderRegistry({ entries, pollMs }: { entries: ProviderEntry[]; pollMs: number }) {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<{ replaceId: string | null; form: ProviderForm } | null>(null)
  const [submitted, setSubmitted] = useState(false)
  const [probes, setProbes] = useState<Record<string, IsolationReport | string>>({})

  const write = useMutation({
    mutationFn: (cfg: ProvidersConfig) => api.setProviders(cfg),
    onSuccess: (list) => {
      qc.setQueryData(["providers"], list)
      qc.invalidateQueries({ queryKey: ["chain"] })
      setEditing(null)
      setSubmitted(false)
    },
  })

  const probe = useMutation({
    mutationFn: (id: string) => api.probeProvider(id),
    onSuccess: (report, id) => setProbes((p) => ({ ...p, [id]: report })),
    onError: (e, id) => setProbes((p) => ({ ...p, [id]: e instanceof ApiError ? e.message : String(e) })),
  })

  const stored = configFromEntries(entries)
  const ids = entries.map((e) => e.id)
  const otherIds = editing?.replaceId ? ids.filter((id) => id !== editing.replaceId) : ids
  const errors = editing && submitted ? validateProviderForm(editing.form, otherIds) : {}

  const save = () => {
    if (!editing) return
    setSubmitted(true)
    if (Object.keys(validateProviderForm(editing.form, otherIds)).length > 0) return
    write.mutate(upsertProvider(stored, entryFromProviderForm(editing.form), editing.replaceId))
  }

  const setField = <K extends keyof ProviderForm>(key: K, value: ProviderForm[K]) =>
    setEditing((e) => (e ? { ...e, form: { ...e.form, [key]: value } } : e))

  return (
    <>
      <div className="flex flex-col gap-4">
        <ul className="flex flex-col divide-y rounded-md border">
          {entries.map((entry) => (
            <li key={entry.id} className="flex flex-col gap-2 px-3 py-2.5">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{entry.id}</span>
                  <Badge variant="outline">{KIND_LABEL[entry.kind]}</Badge>
                  {entry.kind === "copy_paste" ? (
                    <span className="text-muted-foreground text-xs">{t("settings.model_providers.rien_ne_quitte_la_machine")}</span>
                  ) : (
                    <>
                      <span className="text-muted-foreground text-xs">{journalLine(entry.description)}</span>
                      <span className="text-xs">{entry.configured ? t("settings.model_providers.configure") : t("settings.model_providers.non_configure")}</span>
                    </>
                  )}
                </div>
                {entry.kind !== "copy_paste" && (
                  <div className="flex flex-wrap gap-1.5">
                    {entry.kind === "cli" && (
                      <Button size="sm" variant="outline" disabled={probe.isPending} onClick={() => probe.mutate(entry.id)} aria-label={`Tester ${entry.id}`}>
                        {t("settings.model_providers.tester_l_isolation")}
                      </Button>
                    )}
                    {entry.kind === "oauth_chat_gpt" && <ConnectButton id={entry.id} pollMs={pollMs} />}
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`Modifier ${entry.id}`}
                      onClick={() => {
                        setSubmitted(false)
                        setEditing({ replaceId: entry.id, form: entry.config ? formFromProvider(entry.config) : EMPTY_FORM("cli") })
                      }}
                    >
                      {t("common.modifier")}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`Retirer ${entry.id}`}
                      disabled={write.isPending}
                      onClick={() => write.mutate(removeProvider(stored, entry.id))}
                    >
                      {t("common.retirer")}
                    </Button>
                  </div>
                )}
              </div>
              {probes[entry.id] && <ProbeResult id={entry.id} result={probes[entry.id]} />}
            </li>
          ))}
        </ul>

        {write.error && <Alerts messages={[write.error instanceof ApiError ? write.error.message : String(write.error)]} />}

        {editing ? (
          <div className="flex flex-col gap-3 rounded-md border p-4">
            <h3 className="text-sm font-medium">{editing.replaceId ? `Modifier ${editing.replaceId}` : t("settings.model_providers.ajouter_un_fournisseur")}</h3>
            <div className="grid gap-3 sm:grid-cols-2">
              <Field label={t("settings.model_providers.type_de_fournisseur")}>
                <NativeSelect
                  value={editing.form.type}
                  disabled={editing.replaceId !== null}
                  onChange={(e) => setEditing({ replaceId: null, form: { ...EMPTY_FORM(e.target.value as ProviderType), id: editing.form.id } })}
                >
                  {types().map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field label={t("settings.model_providers.identifiant_du_fournisseur")} error={errors.id}>
                <Input value={editing.form.id} onChange={(e) => setField("id", e.target.value)} className="font-mono text-xs" />
              </Field>
            </div>
            <ProviderFields form={editing.form} errors={errors} setField={setField} />
            <div className="flex gap-2">
              <Button onClick={save} disabled={write.isPending}>
                {write.isPending ? t("common.enregistrement") : t("settings.model_providers.enregistrer_le_fournisseur")}
              </Button>
              <Button variant="outline" onClick={() => setEditing(null)}>
                {t("common.annuler")}
              </Button>
            </div>
          </div>
        ) : (
          <div>
            <Button
              variant="outline"
              onClick={() => {
                setSubmitted(false)
                setEditing({ replaceId: null, form: EMPTY_FORM("cli") })
              }}
            >
              {t("settings.model_providers.ajouter_un_fournisseur")}
            </Button>
          </div>
        )}
      </div>
    </>
  )
}

function ProviderFields({
  form,
  errors,
  setField,
}: {
  form: ProviderForm
  errors: Record<string, string>
  setField: <K extends keyof ProviderForm>(key: K, value: ProviderForm[K]) => void
}) {
  if (form.type === "cli") {
    return (
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t("settings.model_providers.commande")} error={errors.command}>
          <Input value={form.command} onChange={(e) => setField("command", e.target.value)} className="font-mono text-xs" />
        </Field>
        <Field label={t("settings.model_providers.envoi_du_prompt_par")}>
          <NativeSelect value={form.prompt_via} onChange={(e) => setField("prompt_via", e.target.value as ProviderForm["prompt_via"])}>
            <option value="stdin">{t("settings.model_providers.entree_standard")}</option>
            <option value="arg">{t("settings.model_providers.dernier_argument")}</option>
          </NativeSelect>
        </Field>
        <Field label={t("settings.model_providers.arguments_un_par_ligne")}>
          <textarea rows={3} value={form.args} onChange={(e) => setField("args", e.target.value)} className={TEXTAREA} />
        </Field>
        <Field label={t("settings.model_providers.options_de_desactivation_une_par")}>
          <textarea rows={3} value={form.disable_flags} onChange={(e) => setField("disable_flags", e.target.value)} className={TEXTAREA} />
        </Field>
        <Field label={t("settings.model_providers.libelle_du_modele")}>
          <Input value={form.model_label} onChange={(e) => setField("model_label", e.target.value)} />
        </Field>
        <Field label={t("settings.model_providers.execution_declaree")}>
          <NativeSelect value={form.execution} onChange={(e) => setField("execution", e.target.value as ProviderForm["execution"])}>
            <option value="local">{t("settings.model_providers.local")}</option>
            <option value="hosted">{t("settings.model_providers.modele_heberge")}</option>
          </NativeSelect>
        </Field>
        {form.execution === "hosted" && (
          <>
            <Field label={t("settings.model_providers.editeur")} error={errors.vendor}>
              <Input value={form.vendor} onChange={(e) => setField("vendor", e.target.value)} />
            </Field>
            <Field label={t("settings.model_providers.endpoint")} error={errors.endpoint}>
              <Input value={form.endpoint} onChange={(e) => setField("endpoint", e.target.value)} />
            </Field>
          </>
        )}
      </div>
    )
  }
  if (form.type === "open_ai_compatible") {
    return (
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t("settings.model_providers.prereglage")}>
          <NativeSelect value={form.preset} onChange={(e) => setField("preset", e.target.value as ProviderForm["preset"])}>
            <option value="openai">{t("settings.model_providers.openai")}</option>
            <option value="zai">Z.ai</option>
            <option value="custom">{t("common.personnalise")}</option>
          </NativeSelect>
        </Field>
        <Field label={t("settings.model_providers.url_de_base")} error={errors.base_url}>
          <Input value={form.base_url} onChange={(e) => setField("base_url", e.target.value)} placeholder={t("settings.model_providers.seulement_pour_un_prereglage_personn")} />
        </Field>
        <Field label={t("settings.model_providers.nom_de_la_variable_de")} error={errors.api_key_env}>
          <Input value={form.api_key_env} onChange={(e) => setField("api_key_env", e.target.value)} placeholder={t("settings.model_providers.par_exemple_openai_api_key")} className="font-mono text-xs" />
        </Field>
        <Field label={t("common.modele")} error={errors.model}>
          <Input value={form.model} onChange={(e) => setField("model", e.target.value)} />
        </Field>
      </div>
    )
  }
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <Field label={t("settings.model_providers.profil")}>
        <Input value={form.profile_id} onChange={(e) => setField("profile_id", e.target.value)} placeholder={t("settings.model_providers.vide_le_premier_profil_connecte")} />
      </Field>
      <Field label={t("common.modele")}>
        <Input value={form.model} onChange={(e) => setField("model", e.target.value)} />
      </Field>
      <Field label={t("settings.model_providers.dossier_des_jetons")}>
        <Input value={form.token_dir} onChange={(e) => setField("token_dir", e.target.value)} className="font-mono text-xs" />
      </Field>
    </div>
  )
}

function ProbeResult({ id, result }: { id: string; result: IsolationReport | string }) {
  if (typeof result === "string") return <Alerts messages={[result]} />
  const rows: [string, boolean][] = [
    [t("settings.model_providers.dossier_de_travail_vide"), result.cwd_empty],
    [t("settings.model_providers.environnement_nettoye"), result.env_scrubbed],
    [t("settings.model_providers.aucun_fichier_d_instructions"), result.instruction_files_absent],
  ]
  return (
    <div className="flex flex-col gap-2 text-sm">
      <p className="font-medium">{result.ok ? t("settings.model_providers.controle_d_isolation_reussi") : t("settings.model_providers.controle_d_isolation_echoue")}</p>
      <ul aria-label={t("settings.model_providers.rapport_d_isolation_de_id", { id })} className="flex flex-col gap-0.5 text-xs">
        {rows.map(([label, value]) => (
          <li key={label} className="flex justify-between gap-4">
            <span>{label}</span>
            <span>{value ? t("settings.model_providers.oui") : t("settings.model_providers.non")}</span>
          </li>
        ))}
      </ul>
      {!result.ok && <p className="text-muted-foreground text-xs">{t("settings.model_providers.reponse_du_modele")}</p>}
      <pre className="bg-muted overflow-x-auto rounded-md p-2 font-mono text-xs whitespace-pre-wrap">{result.probe_answer}</pre>
    </div>
  )
}

/** Starts a ChatGPT sign-in, shows its address, and follows it until it ends. */
function ConnectButton({ id, pollMs }: { id: string; pollMs: number }) {
  const qc = useQueryClient()
  const [started, setStarted] = useState<{ url: string } | null>(null)
  const begin = useMutation({
    mutationFn: () => api.oauthBegin(id),
    onSuccess: (answer) => setStarted({ url: answer.url }),
  })
  const status = useQuery({
    queryKey: ["oauth-status", id],
    queryFn: () => api.oauthStatus(id),
    enabled: started !== null,
    refetchInterval: (q) => (q.state.data && !q.state.data.pending ? false : pollMs),
  })
  const done = status.data !== undefined && !status.data.pending
  const connected = done && status.data!.connected
  // A new connection changes the `configured` flag of the registry.
  useEffect(() => {
    if (connected) void qc.invalidateQueries({ queryKey: ["providers"] })
  }, [connected, qc])
  return (
    <div className="flex flex-col items-end gap-1.5">
      <Button size="sm" variant="outline" aria-label={`Connecter ${id}`} disabled={begin.isPending || (started !== null && !done)} onClick={() => begin.mutate()}>
        {t("settings.model_providers.connecter")}
      </Button>
      {started && (
        <a href={started.url} target="_blank" rel="noreferrer" className="text-primary max-w-xs truncate text-xs underline">
          {started.url}
        </a>
      )}
      {status.data && <SignInState status={status.data} />}
      {begin.error && <Alerts messages={[begin.error instanceof ApiError ? begin.error.message : String(begin.error)]} />}
    </div>
  )
}

function SignInState({ status }: { status: OAuthStatus }) {
  if (status.error) return <Alerts messages={[status.error]} />
  if (status.connected && status.profile) return <p className="text-xs">{t("settings.model_providers.connecte_en_tant_que_email", { email: status.profile.email })}</p>
  if (status.pending) return <p className="text-muted-foreground text-xs">{t("settings.model_providers.en_attente_de_la_connexion")}</p>
  return null
}
