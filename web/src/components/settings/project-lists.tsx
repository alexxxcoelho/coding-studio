// "Modèles et listes de l'étude" (design/macos/Etude.dc.html): the memo
// templates, the interview attributes, the interpretive framework with its
// matrix dimensions, and the editable lists. The whole project configuration
// is edited as one draft and saved with one PUT; a bad edit is refused before
// the call, in the study's words (lib/project-form.ts).

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Alerts, Field, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { ApiError, api } from "@/lib/api"
import { attributeDefsProblem, cleanDef, interviewAttributes, personAttributes, slugId } from "@/lib/attributes"
import { keys } from "@/lib/hooks"
import {
  addDimension,
  addItem,
  addTemplateQuestion,
  applyFrameworkChoice,
  frameworkChoice,
  moveItem,
  newTemplate,
  projectConfigProblem,
  removeItem,
  removeTemplate,
  renameItem,
  setDefaultTemplate,
  type FrameworkChoice,
} from "@/lib/project-form"
import type { AttributeDef, AttributeKind, ListItem, MemoTemplate, ProjectConfig } from "@/lib/types"

type ListKey = "checklist" | "theme_checks" | "evidence_natures" | "link_types" | "passage_origins" | "passage_roles"

const LISTS: { key: ListKey; label: string; min: number; note: string }[] = [
  { key: "checklist", get label() { return t("common.fin_d_entretien") }, min: 0, get note() { return t("settings.project_lists.les_points_a_verifier_avant") } },
  { key: "theme_checks", get label() { return t("common.controles_d_un_theme") }, min: 0, get note() { return t("settings.project_lists.ce_qu_on_verifie_avant") } },
  { key: "evidence_natures", get label() { return t("common.natures_de_preuve") }, min: 0, get note() { return t("settings.project_lists.comment_un_passage_appuie_une") } },
  { key: "link_types", get label() { return t("common.types_de_liens") }, min: 1, get note() { return t("settings.project_lists.les_relations_qu_on_peut") } },
  { key: "passage_origins", get label() { return t("common.origines_du_propos") }, min: 1, get note() { return t("settings.project_lists.d_ou_vient_ce_qui") } },
  { key: "passage_roles", get label() { return t("common.roles_dans_la_decision") }, min: 1, get note() { return t("settings.project_lists.ce_que_le_passage_fait") } },
]

const kinds = (): [AttributeKind, string][] => [
  ["text", t("settings.project_lists.kind_text")],
  ["choice", t("settings.project_lists.kind_choice")],
  ["number", t("settings.project_lists.kind_number")],
  ["date", "date"],
]

const kindLabel = (k: AttributeKind) => kinds().find(([v]) => v === k)?.[1] ?? k
const msg = (e: unknown) => {
  if (e instanceof ApiError && e.status === 409 && /origin|role|passage/i.test(e.message)) {
    return t("settings.project_lists.une_origine_ou_un_role")
  }
  return e instanceof ApiError ? e.message : String(e)
}

type Editor = "memo" | "attributes" | "person_attributes" | "dimensions" | ListKey | null

export function ProjectListsSection() {
  const qc = useQueryClient()
  const cfgQ = useQuery({ queryKey: keys.projectConfig, queryFn: api.projectConfig })
  const presetsQ = useQuery({ queryKey: keys.presets, queryFn: api.projectPresets })
  const [draft, setDraft] = useState<ProjectConfig | null>(null)
  const [editor, setEditor] = useState<Editor>(null)
  const [error, setError] = useState<string | null>(null)

  const cfg = draft ?? cfgQ.data ?? null
  const dirty = draft !== null
  const edit = (patch: Partial<ProjectConfig>) => {
    if (!cfg) return
    setError(null)
    setDraft({ ...cfg, ...patch })
  }

  const problem = cfg ? (attributeDefsProblem(cfg.attributes) ?? projectConfigProblem(cfg)) : null

  const save = useMutation({
    mutationFn: (c: ProjectConfig) => api.setProjectConfig({ ...c, attributes: c.attributes.map(cleanDef) }),
    onSuccess: async (saved) => {
      qc.setQueryData(keys.projectConfig, saved)
      setDraft(null)
      setError(null)
      await Promise.all([qc.invalidateQueries({ queryKey: ["attributes"] }), qc.invalidateQueries({ queryKey: ["memo"] })])
      toast.success(t("settings.project_lists.modeles_et_listes_enregistres"))
    },
    onError: (e) => setError(msg(e)),
  })

  if (cfgQ.isLoading || !cfg) return cfgQ.error ? <Alerts messages={[cfgQ.error.message]} /> : <Skeleton className="h-40 w-full" />

  const choice = frameworkChoice(cfg.interpretive_framework)
  const defaultTemplate = cfg.memo_templates.find((tt) => tt.default)
  const memoNote = cfg.memo_templates.map((tt) => t("common.name_n_questions", { n: tt.questions.length, name: tt.name })).join(" · ")
  const interviewAttrs = interviewAttributes(cfg.attributes)
  const personAttrs = personAttributes(cfg.attributes)
  const attrNote = interviewAttrs.map((a) => `${a.label} (${kindLabel(a.kind)}${a.kind === "choice" && a.choices.length ? ` ${a.choices.join(", ")}` : ""})`).join(" · ")

  return (
    <>
      <div className="grp" aria-label={t("common.modeles_et_listes_de_l")}>
        <Row title={t("settings.project_lists.modeles_de_memo_de_familiarisation")} note={memoNote || t("settings.project_lists.aucun_modele")} count={cfg.memo_templates.length} onEdit={() => setEditor("memo")} />
        <Row title={t("settings.project_lists.attributs_d_entretien")} note={attrNote || t("settings.project_lists.aucun_attribut")} count={interviewAttrs.length} onEdit={() => setEditor("attributes")} />
        <Row
          title={t("persons.person_attributes")}
          note={personAttrs.map((a) => `${a.label} (${kindLabel(a.kind)})`).join(" · ") || t("persons.person_attributes_none")}
          count={personAttrs.length}
          onEdit={() => setEditor("person_attributes")}
        />
        <label className="row">
          <span className="k">
            {t("settings.project_lists.cadre_d_interpretation")}
            <small>{t("settings.project_lists.n_apparait_qu_aux_etapes")}</small>
          </span>
          <NativeSelect
            aria-label={t("settings.project_lists.cadre_d_interpretation")}
            style={{ width: 160 }}
            value={choice}
            onChange={(e) => edit({ interpretive_framework: applyFrameworkChoice(e.target.value as FrameworkChoice, cfg.interpretive_framework, presetsQ.data?.frameworks) })}
          >
            <option value="none">{t("common.aucun_2")}</option>
            <option value="toe">TOE</option>
            <option value="tam_utaut">{t("settings.project_lists.tam_utaut")}</option>
            <option value="custom">{t("common.personnalise")}</option>
          </NativeSelect>
        </label>
        <Row
          title={t("settings.project_lists.dimensions_personnalisees_de_la_matr")}
          note={t("settings.project_lists.par_exemple_qui_decide_qui")}
          count={cfg.matrix_dimensions.length}
          countLabel={cfg.matrix_dimensions.length === 0 ? t("common.aucune") : undefined}
          onEdit={() => setEditor("dimensions")}
        />
        {LISTS.map((l) => (
          <Row key={l.key} title={l.label} note={l.note} count={(cfg[l.key] ?? []).length} onEdit={() => setEditor(l.key)} />
        ))}
      </div>
      <p className="small l2" style={{ margin: "4px 2px" }}>
        {t("settings.project_lists.les_valeurs_de_cellule_de")}
      </p>
      {choice === "custom" && <FrameworkDimensions cfg={cfg} onChange={(f) => edit({ interpretive_framework: f })} />}
      {(error || (dirty && problem)) && <Alerts messages={[error ?? problem!]} />}
      <div className="r8" style={{ marginTop: 8 }}>
        <Button disabled={!dirty || !!problem || save.isPending} onClick={() => save.mutate(cfg)}>
          {t("settings.project_lists.enregistrer_les_modeles_et_listes")}
        </Button>
        {dirty && (
          <Button variant="outline" onClick={() => setDraft(null)}>
            {t("common.annuler_les_changements")}
          </Button>
        )}
        {defaultTemplate && !dirty && <span className="small l2">{t("settings.project_lists.modele_de_memo_par_defaut", { name: defaultTemplate.name })}</span>}
      </div>

      <MemoDialog open={editor === "memo"} onClose={() => setEditor(null)} templates={cfg.memo_templates} onChange={(tt) => edit({ memo_templates: tt })} />
      <AttributesDialog open={editor === "attributes"} onClose={() => setEditor(null)} scope="interview" attributes={cfg.attributes} onChange={(a) => edit({ attributes: a })} />
      <AttributesDialog open={editor === "person_attributes"} onClose={() => setEditor(null)} scope="person" attributes={cfg.attributes} onChange={(a) => edit({ attributes: a })} />
      <ListDialog
        open={editor === "dimensions"}
        onClose={() => setEditor(null)}
        title={t("settings.project_lists.dimensions_personnalisees_de_la_matr")}
        description={t("settings.project_lists.chaque_dimension_devient_une_colonne")}
        items={cfg.matrix_dimensions}
        onChange={(items) => edit({ matrix_dimensions: items })}
      />
      {LISTS.map((l) => (
        <ListDialog
          key={l.key}
          open={editor === l.key}
          onClose={() => setEditor(null)}
          title={l.label}
          description={l.note}
          items={cfg[l.key]}
          onChange={(items) => edit({ [l.key]: items } as Partial<ProjectConfig>)}
        />
      ))}
    </>
  )
}

function Row({ title, note, count, countLabel, onEdit }: { title: string; note: string; count: number; countLabel?: string; onEdit: () => void }) {
  return (
    <div className="row">
      <span className="k">
        {title}
        <small>{note}</small>
      </span>
      <span className="v">{countLabel ?? count}</span>
      <Button variant="outline" aria-label={`Modifier : ${title}`} onClick={onEdit}>
        {t("settings.project_lists.modifier")}
      </Button>
    </div>
  )
}

// ----------------------------------------------------------- framework

function FrameworkDimensions({ cfg, onChange }: { cfg: ProjectConfig; onChange: (f: ProjectConfig["interpretive_framework"]) => void }) {
  const f = cfg.interpretive_framework!
  const [label, setLabel] = useState("")
  return (
    <div className="grp" style={{ padding: 12, marginTop: 8 }}>
      <span className="b">{t("settings.project_lists.dimensions_du_cadre_personnalise")}</span>
      {f.dimensions.map((d, i) => (
        <div key={d.id} className="r8" style={{ marginTop: 6, alignItems: "flex-start" }}>
          <Input
            aria-label={t("settings.project_lists.nom_de_la_dimension_v", { v: i + 1 })}
            style={{ width: 160 }}
            value={d.label}
            onChange={(e) => onChange({ ...f, dimensions: f.dimensions.map((x) => (x.id === d.id ? { ...x, label: e.target.value } : x)) })}
          />
          <textarea
            aria-label={t("settings.project_lists.definition_de_la_dimension_v", { v: i + 1 })}
            className={TEXTAREA}
            rows={2}
            style={{ flex: 1, minWidth: 180 }}
            value={d.definition}
            onChange={(e) => onChange({ ...f, dimensions: f.dimensions.map((x) => (x.id === d.id ? { ...x, definition: e.target.value } : x)) })}
          />
          <Button variant="outline" aria-label={t("settings.project_lists.retirer_la_dimension_label", { label: d.label })} onClick={() => onChange({ ...f, dimensions: f.dimensions.filter((x) => x.id !== d.id) })}>
            {t("common.retirer")}
          </Button>
        </div>
      ))}
      <form
        className="r8"
        style={{ marginTop: 8 }}
        onSubmit={(e) => {
          e.preventDefault()
          if (!label.trim()) return
          onChange({ ...f, dimensions: addDimension(f.dimensions, label) })
          setLabel("")
        }}
      >
        <Input aria-label={t("settings.project_lists.nouvelle_dimension")} placeholder={t("settings.project_lists.nouvelle_dimension")} style={{ width: 200 }} value={label} onChange={(e) => setLabel(e.target.value)} />
        <Button type="submit" variant="outline" disabled={!label.trim()}>
          {t("settings.project_lists.ajouter_une_dimension")}
        </Button>
      </form>
    </div>
  )
}

// ---------------------------------------------------------------- lists

function ListDialog({
  open,
  onClose,
  title,
  description,
  items,
  onChange,
}: {
  open: boolean
  onClose: () => void
  title: string
  description: string
  items: ListItem[]
  onChange: (items: ListItem[]) => void
}) {
  const [text, setText] = useState("")
  const add = () => {
    if (!text.trim()) return
    onChange(addItem(items, text))
    setText("")
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 340, overflow: "auto", display: "flex", flexDirection: "column", gap: 6 }}>
          {items.length === 0 && <p className="small l2">{t("settings.project_lists.liste_vide")}</p>}
          {items.map((it, i) => (
            <div key={it.id} className="r8">
              <Input aria-label={`${title} ${i + 1}`} value={it.label} onChange={(e) => onChange(renameItem(items, it.id, e.target.value))} style={{ flex: 1 }} />
              <Button variant="outline" aria-label={`Monter ${it.label}`} disabled={i === 0} onClick={() => onChange(moveItem(items, i, i - 1))}>
                {t("settings.project_lists.monter")}
              </Button>
              <Button variant="outline" aria-label={`Descendre ${it.label}`} disabled={i === items.length - 1} onClick={() => onChange(moveItem(items, i, i + 1))}>
                {t("settings.project_lists.descendre")}
              </Button>
              <Button variant="outline" aria-label={`Retirer ${it.label}`} onClick={() => onChange(removeItem(items, it.id))}>
                {t("common.retirer")}
              </Button>
            </div>
          ))}
        </div>
        <form
          className="r8"
          onSubmit={(e) => {
            e.preventDefault()
            add()
          }}
        >
          <Input aria-label={t("settings.project_lists.nouvel_element")} placeholder={t("settings.project_lists.nouvel_element")} value={text} onChange={(e) => setText(e.target.value)} style={{ flex: 1 }} />
          <Button type="submit" variant="outline" disabled={!text.trim()}>
            {t("common.ajouter")}
          </Button>
        </form>
        <DialogFooter>
          <Button onClick={onClose}>{t("settings.project_lists.termine")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ------------------------------------------------------------ attributes

function AttributesDialog({
  open,
  onClose,
  scope,
  attributes: all,
  onChange,
}: {
  open: boolean
  onClose: () => void
  /** Which attributes the dialog lists: those of the interviews or those of the register. */
  scope: "interview" | "person"
  attributes: AttributeDef[]
  onChange: (a: AttributeDef[]) => void
}) {
  const [label, setLabel] = useState("")
  const attributes = all.filter((a) => (a.scope === "person") === (scope === "person"))
  const set = (id: string, patch: Partial<AttributeDef>) => onChange(all.map((a) => (a.id === id ? { ...a, ...patch } : a)))
  const problem = attributeDefsProblem(all)
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{scope === "person" ? t("persons.person_attributes") : t("settings.project_lists.attributs_d_entretien")}</DialogTitle>
          <DialogDescription>{scope === "person" ? t("persons.person_attributes_help") : t("settings.project_lists.ce_qu_on_note_sur")}</DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 360, overflow: "auto", display: "flex", flexDirection: "column", gap: 8 }}>
          {attributes.length === 0 && <p className="small l2">{t("settings.project_lists.aucun_attribut_2")}</p>}
          {attributes.map((a, i) => (
            <div key={a.id} className="r8" style={{ alignItems: "flex-start" }}>
              <Input aria-label={t("settings.project_lists.nom_de_l_attribut_v", { v: i + 1 })} style={{ width: 150 }} value={a.label} onChange={(e) => set(a.id, { label: e.target.value })} />
              <NativeSelect aria-label={t("settings.project_lists.type_de_l_attribut_v", { v: i + 1 })} style={{ width: 90 }} value={a.kind} onChange={(e) => set(a.id, { kind: e.target.value as AttributeKind })}>
                {kinds().map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </NativeSelect>
              {a.kind === "choice" && (
                <Input
                  aria-label={t("settings.project_lists.valeurs_de_l_attribut_v", { v: i + 1 })}
                  placeholder={t("settings.project_lists.valeurs_separees_par_des_virgules")}
                  style={{ width: 200 }}
                  value={a.choices.join(", ")}
                  onChange={(e) => set(a.id, { choices: e.target.value.split(",").map((s) => s.trimStart()) })}
                />
              )}
              <label className="chk" style={{ display: "inline-flex", gap: 4, alignItems: "center", fontSize: 12 }}>
                <input type="checkbox" checked={a.shared_with_model} onChange={(e) => set(a.id, { shared_with_model: e.target.checked })} />
                {t("settings.project_lists.envoye_au_modele")}
              </label>
              <Button variant="outline" aria-label={t("settings.project_lists.retirer_l_attribut_label", { label: a.label })} onClick={() => onChange(all.filter((x) => x.id !== a.id))}>
                {t("common.retirer")}
              </Button>
            </div>
          ))}
        </div>
        <form
          className="r8"
          onSubmit={(e) => {
            e.preventDefault()
            if (!label.trim()) return
            onChange([...all, { id: slugId(label, all.map((a) => a.id)), label: label.trim(), kind: "text", choices: [], shared_with_model: false, ...(scope === "person" ? { scope: "person" as const } : {}) }])
            setLabel("")
          }}
        >
          <Input aria-label={t("settings.project_lists.nouvel_attribut")} placeholder={t("settings.project_lists.nouvel_attribut")} value={label} onChange={(e) => setLabel(e.target.value)} style={{ flex: 1 }} />
          <Button type="submit" variant="outline" disabled={!label.trim()}>
            {t("common.ajouter")}
          </Button>
        </form>
        {problem && <Alerts messages={[problem]} />}
        <DialogFooter>
          <Button onClick={onClose}>{t("settings.project_lists.termine")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ------------------------------------------------------------- memo

function MemoDialog({
  open,
  onClose,
  templates,
  onChange,
}: {
  open: boolean
  onClose: () => void
  templates: MemoTemplate[]
  onChange: (tt: MemoTemplate[]) => void
}) {
  const [name, setName] = useState("")
  const [pickedId, setPicked] = useState<string | null>(null)
  const current = useMemo(() => templates.find((tt) => tt.id === pickedId) ?? templates[0] ?? null, [templates, pickedId])
  const patch = (tt: MemoTemplate) => onChange(templates.map((x) => (x.id === tt.id ? { ...tt, version: x.version } : x)))
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("settings.project_lists.modeles_de_memo_de_familiarisation")}</DialogTitle>
          <DialogDescription>
            {t("settings.project_lists.les_questions_auxquelles_on_repond")}
          </DialogDescription>
        </DialogHeader>
        <div className="r8">
          <NativeSelect aria-label={t("settings.project_lists.modele_a_modifier")} style={{ width: 220 }} value={current?.id ?? ""} onChange={(e) => setPicked(e.target.value)}>
            {templates.map((tt) => (
              <option key={tt.id} value={tt.id}>
                {tt.name}
                {tt.default ? t("settings.project_lists.par_defaut") : ""}
              </option>
            ))}
          </NativeSelect>
          {current && !current.default && (
            <>
              <Button variant="outline" onClick={() => onChange(setDefaultTemplate(templates, current.id))}>
                {t("settings.project_lists.definir_par_defaut")}
              </Button>
              <Button
                variant="outline"
                onClick={() => {
                  onChange(removeTemplate(templates, current.id))
                  setPicked(null)
                }}
              >
                {t("settings.project_lists.supprimer_le_modele")}
              </Button>
            </>
          )}
        </div>
        {current && (
          <div style={{ maxHeight: 300, overflow: "auto", display: "flex", flexDirection: "column", gap: 8 }}>
            <Field label={t("settings.project_lists.nom_du_modele")}>
              <Input value={current.name} onChange={(e) => patch({ ...current, name: e.target.value })} />
            </Field>
            {current.questions.map((q, i) => (
              <div key={q.id} className="r8" style={{ alignItems: "flex-start" }}>
                <textarea
                  aria-label={`Question ${i + 1}`}
                  className={TEXTAREA}
                  rows={2}
                  style={{ flex: 1 }}
                  value={q.text}
                  onChange={(e) => patch({ ...current, questions: current.questions.map((x) => (x.id === q.id ? { ...x, text: e.target.value } : x)) })}
                />
                <Button
                  variant="outline"
                  aria-label={t("settings.project_lists.monter_la_question_v", { v: i + 1 })}
                  disabled={i === 0}
                  onClick={() => patch({ ...current, questions: moveItem(current.questions, i, i - 1) })}
                >
                  {t("settings.project_lists.monter")}
                </Button>
                <Button
                  variant="outline"
                  aria-label={t("settings.project_lists.retirer_la_question_v", { v: i + 1 })}
                  disabled={current.questions.length <= 1}
                  onClick={() => patch({ ...current, questions: current.questions.filter((x) => x.id !== q.id) })}
                >
                  {t("common.retirer")}
                </Button>
              </div>
            ))}
            <div>
              <Button variant="outline" onClick={() => patch(addTemplateQuestion(current))}>
                {t("common.ajouter_une_question")}
              </Button>
            </div>
          </div>
        )}
        <form
          className="r8"
          onSubmit={(e) => {
            e.preventDefault()
            if (!name.trim()) return
            const next = newTemplate(templates, name)
            onChange(next)
            setPicked(next[next.length - 1].id)
            setName("")
          }}
        >
          <Input aria-label={t("settings.project_lists.nom_du_nouveau_modele")} placeholder={t("settings.project_lists.nouveau_modele")} value={name} onChange={(e) => setName(e.target.value)} style={{ flex: 1 }} />
          <Button type="submit" variant="outline" disabled={!name.trim()}>
            {t("settings.project_lists.creer_un_modele")}
          </Button>
        </form>
        <DialogFooter>
          <Button onClick={onClose}>{t("settings.project_lists.termine")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
