// The Mémo tab: the familiarisation memo of the interview. Questions come from
// the template chosen for it; the interview can add, reword or remove
// questions for itself only; answers are saved with their date.
// docs/guide/fr/02_coder.md, action 1.

import { useEffect, useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Pencil, Trash2 } from "lucide-react"

import { NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { api } from "@/lib/api"
import { dateTime } from "@/lib/format"
import { keys, useMemoView, useProjectConfig } from "@/lib/hooks"
import { addQuestion, changedAnswers, memoProgress, removeQuestion, restoreQuestion, rewordQuestion } from "@/lib/memo-overrides"
import { reportError } from "@/lib/notify"
import type { MemoOverrides, MemoQuestionView } from "@/lib/types"

function Question({
  q,
  draft,
  onDraft,
  onBlur,
  onReword,
  onRemove,
}: {
  q: MemoQuestionView
  draft: string
  onDraft: (text: string) => void
  onBlur: () => void
  onReword: (text: string) => void
  onRemove: () => void
}) {
  const [editing, setEditing] = useState(false)
  const [text, setText] = useState(q.text)
  const [seenText, setSeenText] = useState(q.text)
  if (seenText !== q.text) {
    setSeenText(q.text)
    setText(q.text)
  }
  const id = `memo-${q.id}`
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <div style={{ display: "flex", gap: 6, alignItems: "flex-start" }}>
        {editing ? (
          <form
            style={{ display: "flex", gap: 6, flex: 1 }}
            onSubmit={(e) => {
              e.preventDefault()
              onReword(text)
              setEditing(false)
            }}
          >
            <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} aria-label="Reformuler la question" />
            <Button type="submit" size="sm">
              OK
            </Button>
          </form>
        ) : (
          <label htmlFor={id} style={{ flex: 1, fontWeight: 500, fontSize: 13 }}>
            {q.text}
            {(q.reworded || q.source === "added") && <span className="badge" style={{ marginLeft: 6 }}>{q.source === "added" ? "ajoutée" : "reformulée"}</span>}
          </label>
        )}
        {!editing && (
          <>
            <button type="button" className="text-label2" style={{ border: 0, background: "transparent", cursor: "pointer", padding: 2 }} aria-label={`Reformuler : ${q.text}`} title="Reformuler pour cet entretien" onClick={() => setEditing(true)}>
              <Pencil className="size-3.5" aria-hidden />
            </button>
            <button type="button" className="text-label2" style={{ border: 0, background: "transparent", cursor: "pointer", padding: 2 }} aria-label={`Retirer : ${q.text}`} title="Retirer pour cet entretien" onClick={onRemove}>
              <Trash2 className="size-3.5" aria-hidden />
            </button>
          </>
        )}
      </div>
      <textarea id={id} className={TEXTAREA} rows={3} value={draft} onChange={(e) => onDraft(e.target.value)} onBlur={onBlur} />
      {q.answer && <span className="small l3">Enregistré le {dateTime(q.answer.at)}</span>}
    </div>
  )
}

export function MemoPanel({ interviewId }: { interviewId: string }) {
  const qc = useQueryClient()
  const view = useMemoView(interviewId)
  const config = useProjectConfig()
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const [adding, setAdding] = useState(false)
  const [newText, setNewText] = useState("")
  const [switching, setSwitching] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const apply = (v: Awaited<ReturnType<typeof api.memo>>) => {
    qc.setQueryData(keys.memo(interviewId), v)
    qc.invalidateQueries({ queryKey: ["chain"] })
  }
  const saveAnswers = useMutation({
    mutationFn: (answers: Record<string, string>) => api.setMemoAnswers(interviewId, answers),
    onSuccess: (v) => {
      apply(v)
      setDrafts({})
    },
    onError: (e) => reportError(e, "Réponse non enregistrée"),
  })
  const saveOverrides = useMutation({
    mutationFn: (o: MemoOverrides) => api.setMemoOverrides(interviewId, o),
    onSuccess: apply,
    onError: (e) => reportError(e, "Question non modifiée"),
  })
  const chooseTemplate = useMutation({
    mutationFn: (id: string) => api.setMemoTemplate(interviewId, id),
    onSuccess: (v) => {
      apply(v)
      setSwitching(false)
    },
    onError: (e) => reportError(e, "Modèle non changé"),
  })

  const data = view.data
  const flush = (current: Record<string, string> = drafts) => {
    if (!data) return
    const changed = changedAnswers(data, current)
    if (Object.keys(changed).length > 0) saveAnswers.mutate(changed)
  }
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current)
    },
    [],
  )

  if (view.isLoading || !data) return <div className="insp-in small l2">Chargement du mémo…</div>
  const templates = config.data?.memo_templates ?? []
  const value = (q: MemoQuestionView) => drafts[q.id] ?? q.answer?.text ?? ""
  const onDraft = (id: string, text: string) => {
    const next = { ...drafts, [id]: text }
    setDrafts(next)
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => flush(next), 1200)
  }
  const edit = (fn: (o: MemoOverrides) => MemoOverrides) => saveOverrides.mutate(fn(data.overrides))

  return (
    <div className="insp-in">
      <div>
        <div className="hs">Mémo de familiarisation</div>
        <div className="small l2">
          Modèle : {data.template_name}, version {data.template_version} · {memoProgress(data)} réponses
        </div>
      </div>
      {data.outdated && (
        <p className="note-warn" role="note" style={{ flexWrap: "wrap" }}>
          <span style={{ flex: 1 }}>Le modèle a changé depuis le début de ce mémo (version {data.current_version}).</span>
          <Button size="sm" variant="outline" onClick={() => chooseTemplate.mutate(data.template_id)}>
            Passer à la version {data.current_version}
          </Button>
        </p>
      )}
      {data.questions.map((q) => (
        <Question
          key={q.id}
          q={q}
          draft={value(q)}
          onDraft={(t) => onDraft(q.id, t)}
          onBlur={() => flush()}
          onReword={(t) => {
            const tpl = templates.find((x) => x.id === data.template_id)?.questions.find((x) => x.id === q.id)
            edit((o) => rewordQuestion(o, q.id, t, q.source, tpl?.text))
          }}
          onRemove={() => edit((o) => removeQuestion(o, q.id, q.source))}
        />
      ))}

      {adding ? (
        <form
          style={{ display: "flex", gap: 6 }}
          onSubmit={(e) => {
            e.preventDefault()
            if (newText.trim()) edit((o) => addQuestion(o, newText))
            setNewText("")
            setAdding(false)
          }}
        >
          <Input autoFocus value={newText} onChange={(e) => setNewText(e.target.value)} placeholder="Qui décide de quoi ?" aria-label="Nouvelle question pour cet entretien" />
          <Button type="submit" size="sm">
            Ajouter
          </Button>
        </form>
      ) : (
        <div>
          <Button variant="outline" onClick={() => setAdding(true)}>
            + question pour cet entretien
          </Button>
        </div>
      )}

      {data.removed.length > 0 && (
        <div className="grp">
          {data.removed.map((q) => (
            <div className="row" key={q.id}>
              <span className="k l2">
                {q.text}
                <small>Retirée pour cet entretien</small>
              </span>
              <Button size="sm" variant="outline" onClick={() => edit((o) => restoreQuestion(o, q.id))}>
                Rétablir
              </Button>
            </div>
          ))}
        </div>
      )}

      {switching ? (
        <div className="grp">
          <div className="row" style={{ flexWrap: "wrap" }}>
            <label className="k" htmlFor="memo-switch">
              Changer de modèle
              <small>Les modifications propres à cet entretien sont remises à zéro. Les réponses sont gardées.</small>
            </label>
            <NativeSelect id="memo-switch" style={{ width: 140 }} value={data.template_id} onChange={(e) => chooseTemplate.mutate(e.target.value)}>
              {templates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </NativeSelect>
          </div>
        </div>
      ) : (
        <div>
          <Button variant="ghost" className="text-accent-text!" onClick={() => setSwitching(true)}>
            Changer de modèle
          </Button>
        </div>
      )}
      <p className="small l2" style={{ margin: 0 }}>
        Le mémo ne bloque rien : vous pouvez coder sans l'avoir rempli. Le modèle lui-même se modifie dans Paramètres de l'étude.
      </p>
    </div>
  )
}
