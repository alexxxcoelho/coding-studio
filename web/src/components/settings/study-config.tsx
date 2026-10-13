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
    <Section title="Étude">
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
          <Field label="Nom de l'étude">
            <Input value={form.study_name} onChange={(e) => set("study_name", e.target.value)} />
          </Field>
          <Field label="Longueur maximale d'une unité, en mots" error={errors.max_words}>
            <Input type="number" min={20} step={1} value={form.max_words} onChange={(e) => set("max_words", e.target.value)} />
          </Field>
        </div>

        <Field label="Description de l'étude" error={errors.description}>
          <textarea rows={3} value={form.description} onChange={(e) => set("description", e.target.value)} className={TEXTAREA} />
        </Field>
        <Field label="Règles de codage" error={errors.coding_rules}>
          <textarea rows={3} value={form.coding_rules} onChange={(e) => set("coding_rules", e.target.value)} className={TEXTAREA} />
        </Field>

        <div className="grid gap-5 md:grid-cols-2">
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">Préfixes d'identifiants des dossiers</h3>
            <div className="flex flex-wrap gap-1.5">
              {form.id_prefixes.map((p) => (
                <Chip key={p} value={p} removeLabel={`Retirer le préfixe ${p}`} onRemove={() => set("id_prefixes", removeChip(form.id_prefixes, p))} />
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                aria-label="Nouveau préfixe"
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
                Ajouter un préfixe
              </Button>
            </div>
            <Alerts messages={errors.id_prefixes ? [errors.id_prefixes] : []} />
          </div>

          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">Intervieweurs</h3>
            <div className="flex flex-wrap gap-1.5">
              {form.interviewers.map((n) => (
                <Chip key={n} value={n} removeLabel={`Retirer l'intervieweur ${n}`} onRemove={() => set("interviewers", removeChip(form.interviewers, n))} />
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                aria-label="Nouvel intervieweur"
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
                Ajouter un intervieweur
              </Button>
            </div>
          </div>
        </div>

        <div className="flex flex-col gap-3">
          <h3 className="text-sm font-medium">Instructions envoyées au modèle</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Version des instructions" error={errors.prompt_version}>
              <Input value={form.template.version} onChange={(e) => setTemplate("version", e.target.value)} />
            </Field>
            <Field label="Introduction du codebook">
              <textarea rows={2} value={form.template.codebook_intro} onChange={(e) => setTemplate("codebook_intro", e.target.value)} className={TEXTAREA} />
            </Field>
            <Field label="Introduction des unités">
              <textarea rows={2} value={form.template.units_intro} onChange={(e) => setTemplate("units_intro", e.target.value)} className={TEXTAREA} />
            </Field>
          </div>
        </div>

        {serverError && <Alerts messages={[serverError]} />}
        <div className="flex items-center gap-3">
          <Button onClick={submit} disabled={save.isPending}>
            {save.isPending ? "Enregistrement..." : "Enregistrer l'étude"}
          </Button>
          {saved && (
            <p role="status" className="text-sm">
              Étude enregistrée
            </p>
          )}
        </div>
      </div>

      <Dialog open={guard !== null} onOpenChange={(open) => !open && setGuard(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Enregistrer quand même ?</DialogTitle>
            <DialogDescription>Le serveur a refusé ce changement pour protéger le codage déjà fait.</DialogDescription>
          </DialogHeader>
          <p className="text-sm">{guard?.message}</p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setGuard(null)}>
              Annuler
            </Button>
            <Button
              variant="destructive"
              disabled={save.isPending}
              onClick={() => guard && save.mutate({ cfg: guard.cfg, force: true })}
            >
              Enregistrer quand même
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
