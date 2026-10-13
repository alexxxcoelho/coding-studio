// Study configuration: the framework, the phase 1 categories (editable for
// the custom framework only), the id prefixes, the interviewers, the words
// per unit and the prompt template. A save refused by the guard (409) asks
// before it is forced.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Trash2 } from "lucide-react"

import { Section } from "@/components/bits"
import { Alerts, Chip, Field, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { api, ApiError } from "@/lib/api"
import {
  addChip,
  blankCategory,
  categoriesEditable,
  configFromForm,
  formFromConfig,
  removeChip,
  validateForm,
  type CategoryRow,
  type FormErrors,
  type StudyForm,
} from "@/lib/settings"
import type { StudyConfig } from "@/lib/types"

const FRAMEWORKS = [
  ["toe", "TOE"],
  ["tam_utaut", "TAM/UTAUT"],
  ["none", "None"],
  ["custom", "Custom"],
] as const

export function StudyConfigSection() {
  const config = useQuery({ queryKey: ["config"], queryFn: api.config })
  // One region for every state: the section must not be replaced while loading.
  return (
    <Section title="Study configuration">
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
  const editable = categoriesEditable(form.framework)
  const set = <K extends keyof StudyForm>(key: K, value: StudyForm[K]) => {
    setSaved(false)
    setForm((f) => ({ ...f, [key]: value }))
  }
  const setRow = (i: number, patch: Partial<CategoryRow>) =>
    set(
      "categories",
      form.categories.map((row, k) => (k === i ? { ...row, ...patch } : row)),
    )
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
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label="Study name">
            <Input value={form.study_name} onChange={(e) => set("study_name", e.target.value)} />
          </Field>
          <Field label="Framework">
            <NativeSelect value={form.framework} onChange={(e) => set("framework", e.target.value as StudyForm["framework"])}>
              {FRAMEWORKS.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field label="Maximum words per unit" error={errors.max_words}>
            <Input type="number" min={20} step={1} value={form.max_words} onChange={(e) => set("max_words", e.target.value)} />
          </Field>
        </div>

        <div className="flex flex-col gap-2">
          <h3 className="text-sm font-medium">Phase 1 categories</h3>
          <p className="text-muted-foreground text-xs">
            {editable
              ? "Custom categories: name, label, definition and the TOE letter their phase 2 codes carry."
              : "Read only: the categories of this framework come from its preset."}
          </p>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[36rem] text-sm">
              <thead className="text-muted-foreground text-left text-xs">
                <tr>
                  <th className="pb-1 font-medium">Code</th>
                  <th className="pb-1 font-medium">Label</th>
                  <th className="pb-1 font-medium">Definition</th>
                  <th className="pb-1 font-medium">Attachment</th>
                  <th className="pb-1" />
                </tr>
              </thead>
              <tbody>
                {form.categories.map((row, i) => (
                  <tr key={i} className="align-top">
                    <td className="pr-2 pb-2">
                      <Input aria-label={`Category code ${i + 1}`} value={row.code} disabled={!editable} onChange={(e) => setRow(i, { code: e.target.value })} className="font-mono text-xs" />
                    </td>
                    <td className="pr-2 pb-2">
                      <Input aria-label={`Category label ${i + 1}`} value={row.label} disabled={!editable} onChange={(e) => setRow(i, { label: e.target.value })} />
                    </td>
                    <td className="pr-2 pb-2">
                      <textarea aria-label={`Category definition ${i + 1}`} rows={2} value={row.definition} disabled={!editable} onChange={(e) => setRow(i, { definition: e.target.value })} className={TEXTAREA} />
                    </td>
                    <td className="pr-2 pb-2">
                      <NativeSelect aria-label={`Category attachment ${i + 1}`} value={row.attachment} disabled={!editable} onChange={(e) => setRow(i, { attachment: e.target.value })}>
                        <option value="">none</option>
                        <option value="T">T</option>
                        <option value="O">O</option>
                        <option value="E">E</option>
                      </NativeSelect>
                    </td>
                    <td className="pb-2 text-right">
                      {editable && (
                        <Button variant="ghost" size="icon-sm" aria-label={`Remove category ${i + 1}`} onClick={() => set("categories", form.categories.filter((_, k) => k !== i))}>
                          <Trash2 />
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {editable && (
            <div>
              <Button variant="outline" size="sm" onClick={() => set("categories", [...form.categories, blankCategory()])}>
                Add category
              </Button>
            </div>
          )}
          <Alerts messages={errors.categories ? [errors.categories] : []} />
        </div>

        <div className="grid gap-5 md:grid-cols-2">
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">Id prefixes</h3>
            <div className="flex flex-wrap gap-1.5">
              {form.id_prefixes.map((p) => (
                <Chip key={p} value={p} removeLabel={`Remove prefix ${p}`} onRemove={() => set("id_prefixes", removeChip(form.id_prefixes, p))} />
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                aria-label="New id prefix"
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
                Add id prefix
              </Button>
            </div>
            <Alerts messages={errors.id_prefixes ? [errors.id_prefixes] : []} />
          </div>

          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">Interviewers</h3>
            <div className="flex flex-wrap gap-1.5">
              {form.interviewers.map((n) => (
                <Chip key={n} value={n} removeLabel={`Remove interviewer ${n}`} onRemove={() => set("interviewers", removeChip(form.interviewers, n))} />
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                aria-label="New interviewer"
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
                Add interviewer
              </Button>
            </div>
          </div>
        </div>

        <div className="flex flex-col gap-3">
          <h3 className="text-sm font-medium">Prompt template</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Prompt version" error={errors.prompt_version}>
              <Input value={form.template.version} onChange={(e) => setTemplate("version", e.target.value)} />
            </Field>
            <Field label="Prompt header">
              <textarea rows={3} value={form.template.header} onChange={(e) => setTemplate("header", e.target.value)} className={TEXTAREA} />
            </Field>
            <Field label="Codebook intro">
              <textarea rows={2} value={form.template.codebook_intro} onChange={(e) => setTemplate("codebook_intro", e.target.value)} className={TEXTAREA} />
            </Field>
            <Field label="Units intro">
              <textarea rows={2} value={form.template.units_intro} onChange={(e) => setTemplate("units_intro", e.target.value)} className={TEXTAREA} />
            </Field>
          </div>
          <Field label="Prompt footer">
            <textarea rows={2} value={form.template.footer} onChange={(e) => setTemplate("footer", e.target.value)} className={TEXTAREA} />
          </Field>
        </div>

        {serverError && <Alerts messages={[serverError]} />}
        <div className="flex items-center gap-3">
          <Button onClick={submit} disabled={save.isPending}>
            {save.isPending ? "Saving..." : "Save configuration"}
          </Button>
          {saved && (
            <p role="status" className="text-sm">
              Configuration saved
            </p>
          )}
        </div>
      </div>

      <Dialog open={guard !== null} onOpenChange={(open) => !open && setGuard(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Save anyway?</DialogTitle>
            <DialogDescription>The server refused this change to protect the coding already made.</DialogDescription>
          </DialogHeader>
          <p className="text-sm">{guard?.message}</p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setGuard(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={save.isPending}
              onClick={() => guard && save.mutate({ cfg: guard.cfg, force: true })}
            >
              Save anyway
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
