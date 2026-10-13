// Study configuration: the name, the description and the coding rules (their
// own fields, read by the model in an AI review), the words per unit, the id
// prefixes, the interviewers and the version of the instructions. The phase 1
// category set is a legacy setting kept as stored and no longer shown. A save
// refused by the guard (409) asks before it is forced.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { Section } from "@/components/bits"
import { Alerts, Chip, Field, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api, ApiError } from "@/lib/api"
import {
  addChip,
  configFromForm,
  formFromConfig,
  removeChip,
  validateForm,
  type FormErrors,
  type StudyForm,
} from "@/lib/settings"
import type { StudyConfig } from "@/lib/types"

export function StudyConfigSection() {
  const config = useQuery({ queryKey: ["config"], queryFn: api.config })
  // One region for every state: the section must not be replaced while loading.
  return (
    <Section title={t("common.etude")}>
      {config.isLoading ? (
        <Skeleton className="h-48 w-full" />
      ) : config.error ? (
        <Alerts messages={[config.error.message]} />
      ) : (
        <StudyForms initial={config.data!} />
      )}
    </Section>
  )
}

/** The form starts from the configuration read once; saves replace the query data without resetting it. */
function StudyForms({ initial }: { initial: StudyConfig }) {
  const qc = useQueryClient()
  const [form, setForm] = useState<StudyForm>(() => formFromConfig(initial))
  const [submitted, setSubmitted] = useState(false)
  const [saved, setSaved] = useState(false)
  const [serverError, setServerError] = useState<string | null>(null)
  const [guard, setGuard] = useState<{ message: string; cfg: StudyConfig } | null>(null)

  const save = useMutation({
    mutationFn: (vars: { cfg: StudyConfig; force: boolean }) => api.setConfig(vars.cfg, vars.force),
    onSuccess: (cfg) => {
      qc.setQueryData(["config"], cfg)
      qc.invalidateQueries({ queryKey: ["state"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
      setGuard(null)
      setServerError(null)
      setSaved(true)
    },
    onError: (e, vars) => {
      setSaved(false)
      if (e instanceof ApiError && e.isGuard && !vars.force) {
        setGuard({ message: e.message, cfg: vars.cfg })
        return
      }
      setServerError(e instanceof ApiError ? e.message : String(e))
    },
  })

  const errors: FormErrors = submitted ? validateForm(form) : {}
  const set = <K extends keyof StudyForm>(key: K, value: StudyForm[K]) => {
    setSaved(false)
    setForm((f) => ({ ...f, [key]: value }))
  }
  const setTemplate = (key: keyof StudyForm["template"], value: string) => set("template", { ...form.template, [key]: value })

  const submit = () => {
    setSubmitted(true)
    if (Object.keys(validateForm(form)).length > 0) return
    save.mutate({ cfg: configFromForm(form), force: false })
  }

  const [newPrefix, setNewPrefix] = useState("")
  const [newInterviewer, setNewInterviewer] = useState("")

  return (
    <>
      <div className="flex flex-col gap-5">
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t("settings.study_config.nom_de_l_etude")}>
            <Input value={form.study_name} onChange={(e) => set("study_name", e.target.value)} />
          </Field>
          <Field label={t("settings.study_config.longueur_maximale_d_une_unite")} error={errors.max_words}>
            <Input type="number" min={20} step={1} value={form.max_words} onChange={(e) => set("max_words", e.target.value)} />
          </Field>
        </div>

        <Field label={t("settings.study_config.description_de_l_etude")} error={errors.description}>
          <textarea rows={3} value={form.description} onChange={(e) => set("description", e.target.value)} className={TEXTAREA} />
        </Field>
        <Field label={t("settings.study_config.regles_de_codage")} error={errors.coding_rules}>
          <textarea rows={3} value={form.coding_rules} onChange={(e) => set("coding_rules", e.target.value)} className={TEXTAREA} />
        </Field>

        <div className="grid gap-5 md:grid-cols-2">
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">{t("settings.study_config.prefixes_d_identifiants_des_dossiers")}</h3>
            <div className="flex flex-wrap gap-1.5">
              {form.id_prefixes.map((p) => (
                <Chip key={p} value={p} removeLabel={t("settings.study_config.retirer_le_prefixe_p", { p })} onRemove={() => set("id_prefixes", removeChip(form.id_prefixes, p))} />
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                aria-label={t("settings.study_config.nouveau_prefixe")}
                value={newPrefix}
                onChange={(e) => setNewPrefix(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault()
                    set("id_prefixes", addChip(form.id_prefixes, newPrefix))
                    setNewPrefix("")
                  }
                }}
              />
              <Button
                variant="outline"
                onClick={() => {
                  set("id_prefixes", addChip(form.id_prefixes, newPrefix))
                  setNewPrefix("")
                }}
              >
                {t("settings.study_config.ajouter_un_prefixe")}
              </Button>
            </div>
            <Alerts messages={errors.id_prefixes ? [errors.id_prefixes] : []} />
          </div>

          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">{t("settings.study_config.intervieweurs")}</h3>
            <div className="flex flex-wrap gap-1.5">
              {form.interviewers.map((n) => (
                <Chip key={n} value={n} removeLabel={t("settings.study_config.retirer_l_intervieweur_n", { n })} onRemove={() => set("interviewers", removeChip(form.interviewers, n))} />
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                aria-label={t("settings.study_config.nouvel_intervieweur")}
                value={newInterviewer}
                onChange={(e) => setNewInterviewer(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault()
                    set("interviewers", addChip(form.interviewers, newInterviewer))
                    setNewInterviewer("")
                  }
                }}
              />
              <Button
                variant="outline"
                onClick={() => {
                  set("interviewers", addChip(form.interviewers, newInterviewer))
                  setNewInterviewer("")
                }}
              >
                {t("settings.study_config.ajouter_un_intervieweur")}
              </Button>
            </div>
          </div>
        </div>

        <div className="flex flex-col gap-3">
          <h3 className="text-sm font-medium">{t("settings.study_config.instructions_envoyees_au_modele")}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t("settings.study_config.version_des_instructions")} error={errors.prompt_version}>
              <Input value={form.template.version} onChange={(e) => setTemplate("version", e.target.value)} />
            </Field>
            <Field label={t("settings.study_config.introduction_du_codebook")}>
              <textarea rows={2} value={form.template.codebook_intro} onChange={(e) => setTemplate("codebook_intro", e.target.value)} className={TEXTAREA} />
            </Field>
            <Field label={t("settings.study_config.introduction_des_unites")}>
              <textarea rows={2} value={form.template.units_intro} onChange={(e) => setTemplate("units_intro", e.target.value)} className={TEXTAREA} />
            </Field>
          </div>
        </div>

        {serverError && <Alerts messages={[serverError]} />}
        <div className="flex items-center gap-3">
          <Button onClick={submit} disabled={save.isPending}>
            {save.isPending ? "Enregistrement..." : t("settings.study_config.enregistrer_l_etude")}
          </Button>
          {saved && (
            <p role="status" className="text-sm">
              {t("settings.study_config.etude_enregistree")}
            </p>
          )}
        </div>
      </div>

      <Dialog open={guard !== null} onOpenChange={(open) => !open && setGuard(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("settings.study_config.enregistrer_quand_meme")}</DialogTitle>
            <DialogDescription>{t("settings.study_config.le_serveur_a_refuse_ce")}</DialogDescription>
          </DialogHeader>
          <p className="text-sm">{guard?.message}</p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setGuard(null)}>
              {t("common.annuler")}
            </Button>
            <Button
              variant="destructive"
              disabled={save.isPending}
              onClick={() => guard && save.mutate({ cfg: guard.cfg, force: true })}
            >
              {t("settings.study_config.enregistrer_quand_meme_2")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
