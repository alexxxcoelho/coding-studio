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
import { ApiError, api } from "@/lib/api"
import { attributeDefsProblem, cleanDef, slugId } from "@/lib/attributes"
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
  { key: "checklist", label: "Fin d'entretien", min: 0, note: "Les points à vérifier avant de passer à la suite" },
  { key: "theme_checks", label: "Contrôles d'un thème", min: 0, note: "Ce qu'on vérifie avant de garder un thème" },
  { key: "evidence_natures", label: "Natures de preuve", min: 0, note: "Comment un passage appuie une affirmation" },
  { key: "link_types", label: "Types de liens", min: 1, note: "Les relations qu'on peut poser entre deux thèmes" },
  { key: "passage_origins", label: "Origines du propos", min: 1, note: "D'où vient ce qui est dit : expérience vécue, opinion, document…" },
  { key: "passage_roles", label: "Rôles dans la décision", min: 1, note: "Ce que le passage fait dans la décision : déclencheur, contrainte…" },
]

const KINDS: [AttributeKind, string][] = [
  ["text", "texte"],
  ["choice", "liste"],
  ["number", "nombre"],
  ["date", "date"],
]

const kindLabel = (k: AttributeKind) => KINDS.find(([v]) => v === k)?.[1] ?? k
const msg = (e: unknown) => {
  if (e instanceof ApiError && e.status === 409 && /origin|role|passage/i.test(e.message)) {
    return "Une origine ou un rôle retiré de la liste est encore utilisé par des passages. Remettez-le dans la liste, ou changez la qualification de ces passages avant de l'enlever."
  }
  return e instanceof ApiError ? e.message : String(e)
}

type Editor = "memo" | "attributes" | "dimensions" | ListKey | null

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
      toast.success("Modèles et listes enregistrés")
    },
    onError: (e) => setError(msg(e)),
  })

  if (cfgQ.isLoading || !cfg) return cfgQ.error ? <Alerts messages={[cfgQ.error.message]} /> : <Skeleton className="h-40 w-full" />

  const choice = frameworkChoice(cfg.interpretive_framework)
  const defaultTemplate = cfg.memo_templates.find((tt) => tt.default)
  const memoNote = cfg.memo_templates.map((tt) => `${tt.name}, ${tt.questions.length} question${tt.questions.length > 1 ? "s" : ""}`).join(" · ")
  const attrNote = cfg.attributes.map((a) => `${a.label} (${kindLabel(a.kind)}${a.kind === "choice" && a.choices.length ? ` ${a.choices.join(", ")}` : ""})`).join(" · ")

  return (
    <>
      <div className="grp" aria-label="Modèles et listes de l'étude">
        <Row title="Modèles de mémo de familiarisation" note={memoNote || "Aucun modèle"} count={cfg.memo_templates.length} onEdit={() => setEditor("memo")} />
        <Row title="Attributs d'entretien" note={attrNote || "Aucun attribut"} count={cfg.attributes.length} onEdit={() => setEditor("attributes")} />
        <label className="row">
          <span className="k">
            Cadre d'interprétation
            <small>N'apparaît qu'aux étapes Analyser et Rédiger, jamais au codage</small>
          </span>
          <NativeSelect
            aria-label="Cadre d'interprétation"
            style={{ width: 160 }}
            value={choice}
            onChange={(e) => edit({ interpretive_framework: applyFrameworkChoice(e.target.value as FrameworkChoice, cfg.interpretive_framework, presetsQ.data?.frameworks) })}
          >
            <option value="none">Aucun</option>
            <option value="toe">TOE</option>
            <option value="tam_utaut">TAM/UTAUT</option>
            <option value="custom">Personnalisé</option>
          </NativeSelect>
        </label>
        <Row
          title="Dimensions personnalisées de la matrice"
          note="Par exemple : qui décide, qui paie, qui utilise"
          count={cfg.matrix_dimensions.length}
          countLabel={cfg.matrix_dimensions.length === 0 ? "aucune" : undefined}
          onEdit={() => setEditor("dimensions")}
        />
        {LISTS.map((l) => (
          <Row key={l.key} title={l.label} note={l.note} count={cfg[l.key].length} onEdit={() => setEditor(l.key)} />
        ))}
      </div>
      <p className="small l2" style={{ margin: "4px 2px" }}>
        Les valeurs de cellule de la matrice (présent, non évoqué, contredit, incertain) restent fixes : elles font partie de la méthode.
      </p>
      {choice === "custom" && <FrameworkDimensions cfg={cfg} onChange={(f) => edit({ interpretive_framework: f })} />}
      {(error || (dirty && problem)) && <Alerts messages={[error ?? problem!]} />}
      <div className="r8" style={{ marginTop: 8 }}>
        <Button disabled={!dirty || !!problem || save.isPending} onClick={() => save.mutate(cfg)}>
          Enregistrer les modèles et listes
        </Button>
        {dirty && (
          <Button variant="outline" onClick={() => setDraft(null)}>
            Annuler les changements
          </Button>
        )}
        {defaultTemplate && !dirty && <span className="small l2">Modèle de mémo par défaut : {defaultTemplate.name}</span>}
      </div>

      <MemoDialog open={editor === "memo"} onClose={() => setEditor(null)} templates={cfg.memo_templates} onChange={(tt) => edit({ memo_templates: tt })} />
      <AttributesDialog open={editor === "attributes"} onClose={() => setEditor(null)} attributes={cfg.attributes} onChange={(a) => edit({ attributes: a })} />
      <ListDialog
        open={editor === "dimensions"}
        onClose={() => setEditor(null)}
        title="Dimensions personnalisées de la matrice"
        description="Chaque dimension devient une colonne de la matrice transversale."
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
        Modifier…
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
      <span className="b">Dimensions du cadre personnalisé</span>
      {f.dimensions.map((d, i) => (
        <div key={d.id} className="r8" style={{ marginTop: 6, alignItems: "flex-start" }}>
          <Input
            aria-label={`Nom de la dimension ${i + 1}`}
            style={{ width: 160 }}
            value={d.label}
            onChange={(e) => onChange({ ...f, dimensions: f.dimensions.map((x) => (x.id === d.id ? { ...x, label: e.target.value } : x)) })}
          />
          <textarea
            aria-label={`Définition de la dimension ${i + 1}`}
            className={TEXTAREA}
            rows={2}
            style={{ flex: 1, minWidth: 180 }}
            value={d.definition}
            onChange={(e) => onChange({ ...f, dimensions: f.dimensions.map((x) => (x.id === d.id ? { ...x, definition: e.target.value } : x)) })}
          />
          <Button variant="outline" aria-label={`Retirer la dimension ${d.label}`} onClick={() => onChange({ ...f, dimensions: f.dimensions.filter((x) => x.id !== d.id) })}>
            Retirer
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
        <Input aria-label="Nouvelle dimension" placeholder="Nouvelle dimension" style={{ width: 200 }} value={label} onChange={(e) => setLabel(e.target.value)} />
        <Button type="submit" variant="outline" disabled={!label.trim()}>
          Ajouter une dimension
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
          {items.length === 0 && <p className="small l2">Liste vide.</p>}
          {items.map((it, i) => (
            <div key={it.id} className="r8">
              <Input aria-label={`${title} ${i + 1}`} value={it.label} onChange={(e) => onChange(renameItem(items, it.id, e.target.value))} style={{ flex: 1 }} />
              <Button variant="outline" aria-label={`Monter ${it.label}`} disabled={i === 0} onClick={() => onChange(moveItem(items, i, i - 1))}>
                Monter
              </Button>
              <Button variant="outline" aria-label={`Descendre ${it.label}`} disabled={i === items.length - 1} onClick={() => onChange(moveItem(items, i, i + 1))}>
                Descendre
              </Button>
              <Button variant="outline" aria-label={`Retirer ${it.label}`} onClick={() => onChange(removeItem(items, it.id))}>
                Retirer
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
          <Input aria-label="Nouvel élément" placeholder="Nouvel élément" value={text} onChange={(e) => setText(e.target.value)} style={{ flex: 1 }} />
          <Button type="submit" variant="outline" disabled={!text.trim()}>
            Ajouter
          </Button>
        </form>
        <DialogFooter>
          <Button onClick={onClose}>Terminé</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ------------------------------------------------------------ attributes

function AttributesDialog({
  open,
  onClose,
  attributes,
  onChange,
}: {
  open: boolean
  onClose: () => void
  attributes: AttributeDef[]
  onChange: (a: AttributeDef[]) => void
}) {
  const [label, setLabel] = useState("")
  const set = (id: string, patch: Partial<AttributeDef>) => onChange(attributes.map((a) => (a.id === id ? { ...a, ...patch } : a)))
  const problem = attributeDefsProblem(attributes)
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Attributs d'entretien</DialogTitle>
          <DialogDescription>Ce qu'on note sur chaque entretien : secteur, rôle, version du guide. Un attribut marqué « envoyé au modèle » est transmis avec les passages.</DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 360, overflow: "auto", display: "flex", flexDirection: "column", gap: 8 }}>
          {attributes.length === 0 && <p className="small l2">Aucun attribut.</p>}
          {attributes.map((a, i) => (
            <div key={a.id} className="r8" style={{ alignItems: "flex-start" }}>
              <Input aria-label={`Nom de l'attribut ${i + 1}`} style={{ width: 150 }} value={a.label} onChange={(e) => set(a.id, { label: e.target.value })} />
              <NativeSelect aria-label={`Type de l'attribut ${i + 1}`} style={{ width: 90 }} value={a.kind} onChange={(e) => set(a.id, { kind: e.target.value as AttributeKind })}>
                {KINDS.map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </NativeSelect>
              {a.kind === "choice" && (
                <Input
                  aria-label={`Valeurs de l'attribut ${i + 1}`}
                  placeholder="Valeurs, séparées par des virgules"
                  style={{ width: 200 }}
                  value={a.choices.join(", ")}
                  onChange={(e) => set(a.id, { choices: e.target.value.split(",").map((s) => s.trimStart()) })}
                />
              )}
              <label className="chk" style={{ display: "inline-flex", gap: 4, alignItems: "center", fontSize: 12 }}>
                <input type="checkbox" checked={a.shared_with_model} onChange={(e) => set(a.id, { shared_with_model: e.target.checked })} />
                Envoyé au modèle
              </label>
              <Button variant="outline" aria-label={`Retirer l'attribut ${a.label}`} onClick={() => onChange(attributes.filter((x) => x.id !== a.id))}>
                Retirer
              </Button>
            </div>
          ))}
        </div>
        <form
          className="r8"
          onSubmit={(e) => {
            e.preventDefault()
            if (!label.trim()) return
            onChange([...attributes, { id: slugId(label, attributes.map((a) => a.id)), label: label.trim(), kind: "text", choices: [], shared_with_model: false }])
            setLabel("")
          }}
        >
          <Input aria-label="Nouvel attribut" placeholder="Nouvel attribut" value={label} onChange={(e) => setLabel(e.target.value)} style={{ flex: 1 }} />
          <Button type="submit" variant="outline" disabled={!label.trim()}>
            Ajouter
          </Button>
        </form>
        {problem && <Alerts messages={[problem]} />}
        <DialogFooter>
          <Button onClick={onClose}>Terminé</Button>
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
          <DialogTitle>Modèles de mémo de familiarisation</DialogTitle>
          <DialogDescription>
            Les questions auxquelles on répond après lecture d'un entretien. Changer un modèle n'efface pas les réponses déjà données, et chaque entretien peut ajuster ses questions.
          </DialogDescription>
        </DialogHeader>
        <div className="r8">
          <NativeSelect aria-label="Modèle à modifier" style={{ width: 220 }} value={current?.id ?? ""} onChange={(e) => setPicked(e.target.value)}>
            {templates.map((tt) => (
              <option key={tt.id} value={tt.id}>
                {tt.name}
                {tt.default ? " (par défaut)" : ""}
              </option>
            ))}
          </NativeSelect>
          {current && !current.default && (
            <>
              <Button variant="outline" onClick={() => onChange(setDefaultTemplate(templates, current.id))}>
                Définir par défaut
              </Button>
              <Button
                variant="outline"
                onClick={() => {
                  onChange(removeTemplate(templates, current.id))
                  setPicked(null)
                }}
              >
                Supprimer le modèle
              </Button>
            </>
          )}
        </div>
        {current && (
          <div style={{ maxHeight: 300, overflow: "auto", display: "flex", flexDirection: "column", gap: 8 }}>
            <Field label="Nom du modèle">
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
                  aria-label={`Monter la question ${i + 1}`}
                  disabled={i === 0}
                  onClick={() => patch({ ...current, questions: moveItem(current.questions, i, i - 1) })}
                >
                  Monter
                </Button>
                <Button
                  variant="outline"
                  aria-label={`Retirer la question ${i + 1}`}
                  disabled={current.questions.length <= 1}
                  onClick={() => patch({ ...current, questions: current.questions.filter((x) => x.id !== q.id) })}
                >
                  Retirer
                </Button>
              </div>
            ))}
            <div>
              <Button variant="outline" onClick={() => patch(addTemplateQuestion(current))}>
                Ajouter une question
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
          <Input aria-label="Nom du nouveau modèle" placeholder="Nouveau modèle" value={name} onChange={(e) => setName(e.target.value)} style={{ flex: 1 }} />
          <Button type="submit" variant="outline" disabled={!name.trim()}>
            Créer un modèle
          </Button>
        </form>
        <DialogFooter>
          <Button onClick={onClose}>Terminé</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
