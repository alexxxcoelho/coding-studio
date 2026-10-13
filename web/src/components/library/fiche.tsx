// The Fiche of an interview, in the inspector of the Bibliothèque: attributes
// (edited in place, validated), familiarisation memo (template choice and
// count) and source. Wording of docs/guide/fr/01_preparer.md, action 10.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Check } from "lucide-react"

import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { api } from "@/lib/api"
import { checkAttribute } from "@/lib/attributes"
import { keys, useAttributes, useMemoView, useProjectConfig } from "@/lib/hooks"
import { memoProgress, overrideCount } from "@/lib/memo-overrides"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"
import type { AttributeDef, InterviewSummary, SourceKind } from "@/lib/types"
import type { CodingProgress } from "@/lib/pipeline"

const KIND_LABEL: Record<SourceKind, string> = {
  vtt: "Transcription de plateforme (VTT)",
  timestamped_text: "Texte brut horodaté",
  cleaned_markdown: "Transcription épurée",
  budgie_markdown: "Export Budgie Echo",
  written_response: "Réponse écrite",
  summary: "Synthèse",
  record_sheet: "Fiche d'enregistrement",
  toe_coding: "Codage antérieur",
  other: "Autre fichier",
}

/** One attribute, edited in place. The value is checked before the call; a wrong one shows its reason and is not sent. */
export function AttributeRow({ def, saved, onSave }: { def: AttributeDef; saved: string; onSave: (value: string | null) => void }) {
  const [draft, setDraft] = useState(saved)
  const [error, setError] = useState<string | null>(null)
  // A new saved value (after a save or a refresh) replaces the draft: adjusted during render, not in an effect.
  const [seen, setSeen] = useState(saved)
  if (seen !== saved) {
    setSeen(saved)
    setDraft(saved)
    setError(null)
  }

  const commit = (raw: string) => {
    const c = checkAttribute(def, raw)
    if (!c.ok) {
      setError(c.error ?? "Valeur refusée")
      return
    }
    setError(null)
    if ((c.value ?? "") !== saved) onSave(c.value)
  }
  const id = `attr-${def.id}`
  return (
    <div className="row" style={{ flexWrap: "wrap" }}>
      <label className="k" htmlFor={id}>
        {def.label}
      </label>
      {def.kind === "choice" ? (
        <NativeSelect
          id={id}
          style={{ width: 128 }}
          value={draft}
          aria-invalid={!!error}
          onChange={(e) => {
            setDraft(e.target.value)
            commit(e.target.value)
          }}
        >
          <option value="">Choisir…</option>
          {def.choices.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </NativeSelect>
      ) : (
        <Input
          id={id}
          style={{ width: 128 }}
          type="text"
          inputMode={def.kind === "number" ? "decimal" : undefined}
          placeholder={def.kind === "text" ? "Texte libre" : def.kind === "date" ? "AAAA-MM-JJ" : "Nombre"}
          value={draft}
          aria-invalid={!!error}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => commit(draft)}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit(draft)
          }}
        />
      )}
      {error && (
        <p role="alert" className="small basis-full text-right" style={{ color: "var(--red-text)", margin: 0 }}>
          {error}
        </p>
      )}
    </div>
  )
}

export function Fiche({ summary, progress }: { summary: InterviewSummary; progress: CodingProgress | null }) {
  const id = summary.interview_id
  const qc = useQueryClient()
  const config = useProjectConfig()
  const attrs = useAttributes(id)
  const memo = useMemoView(id)
  const detail = useQuery({ queryKey: ["interview", id], queryFn: () => api.interview(id) })
  const checklist = useQuery({ queryKey: keys.checklist(id), queryFn: () => api.checklist(id) })

  const save = useMutation({
    mutationFn: (vars: { attr: string; value: string | null }) => api.setAttributes(id, { [vars.attr]: vars.value }),
    onSuccess: (view) => {
      qc.setQueryData(keys.attributes(id), view)
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => reportError(e, "Attribut non enregistré"),
  })
  const chooseTemplate = useMutation({
    mutationFn: (template_id: string) => api.setMemoTemplate(id, template_id),
    onSuccess: (view) => {
      qc.setQueryData(keys.memo(id), view)
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => reportError(e, "Modèle non changé"),
  })

  const defs = attrs.data?.definitions ?? config.data?.attributes ?? []
  const values = attrs.data?.values ?? {}
  const templates = config.data?.memo_templates ?? []
  const current = templates.find((t) => t.id === memo.data?.template_id)
  const edits = memo.data ? overrideCount(memo.data.overrides) : 0
  const sources = detail.data?.entry.sources ?? []
  const primary = detail.data?.entry.primary_source ?? summary.primary_source
  const interviewerTurns = progress ? summary.n_units - progress.respondent : 0

  return (
    <div className="insp-in">
      <div className="r8" style={{ justifyContent: "space-between", display: "flex", alignItems: "center", gap: 8 }}>
        <span className="b mono" style={{ fontSize: 15, fontWeight: 600 }}>
          {id}
        </span>
        <Button asChild variant="ghost" className="text-accent-text!">
          <a href={interviewHref(id, "coding")}>Coder</a>
        </Button>
      </div>

      <div className="hs">Attributs</div>
      <div className="grp">
        {defs.length === 0 ? (
          <div className="row">
            <span className="k l2">
              Aucun attribut défini
              <small>
                Ajoutez-en dans <a href="#/settings">Paramètres de l'étude</a>.
              </small>
            </span>
          </div>
        ) : (
          defs.map((d) => (
            <AttributeRow key={d.id} def={d} saved={values[d.id] ?? ""} onSave={(value) => save.mutate({ attr: d.id, value })} />
          ))
        )}
      </div>

      <div className="hs">Mémo de familiarisation</div>
      <div className="grp">
        <div className="row">
          <span className="k">
            <label htmlFor="memo-template">Modèle</label>
            <small>
              {current ? `${current.name}, ${current.questions.length} question${current.questions.length > 1 ? "s" : ""}` : "…"}
              {edits > 0 && `, ${edits} modification${edits > 1 ? "s" : ""} pour cet entretien`}
            </small>
          </span>
          <NativeSelect
            id="memo-template"
            style={{ width: 128 }}
            value={memo.data?.template_id ?? ""}
            disabled={!memo.data || chooseTemplate.isPending}
            onChange={(e) => chooseTemplate.mutate(e.target.value)}
          >
            {templates.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </NativeSelect>
        </div>
        <div className="row">
          <span className="k">Réponses</span>
          <span className="v">{memo.data ? memoProgress(memo.data) : ""}</span>
        </div>
        <div className="row">
          <span className="k">Mémo de fin d'entretien</span>
          <span className="v">{checklist.data ? (checklist.data.note.trim() ? "rédigé" : "vide") : ""}</span>
        </div>
      </div>

      <div className="hs">Source</div>
      <div className="grp">
        {sources.length === 0 && (
          <div className="row">
            <span className="k l2">Aucun fichier</span>
          </div>
        )}
        {sources.map((s) => (
          <div className="row" key={s.rel_path}>
            <span className="k" title={s.rel_path}>
              {KIND_LABEL[s.kind]}
              <small className="mono" style={{ wordBreak: "break-all" }}>
                {s.rel_path}
              </small>
            </span>
            {s.rel_path === primary && <Check className="size-3.5" style={{ color: "var(--green-text)" }} role="img" aria-label="Transcription retenue" />}
          </div>
        ))}
        {summary.n_units > 0 && progress && (
          <div className="row">
            <span className="k">
              {summary.n_units} tours dont {progress.respondent} du répondant
              <small>
                {interviewerTurns} de l'intervieweur, grisés et jamais codés
              </small>
            </span>
          </div>
        )}
      </div>
      <p className="small l2" style={{ margin: 0 }}>
        Les attributs servent de filtres et de contrastes dans la matrice d'analyse.
      </p>
    </div>
  )
}
