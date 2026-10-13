// Questions de recherche: each keeps its identifier (SQ1, SQ2...), codes and
// the writing attach to it. A text is saved when the field is left; adding and
// removing act at once.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { Alerts } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { ApiError, api } from "@/lib/api"
import type { ResearchQuestion } from "@/lib/types"

const msg = (e: unknown) => (e instanceof ApiError ? e.message : String(e))

/** Next free identifier: SQ1, SQ2... after the highest number in use. */
export function nextQuestionId(questions: ResearchQuestion[]): string {
  const nums = questions.map((q) => /(\d+)$/.exec(q.id)?.[1]).filter((x): x is string => x !== undefined).map(Number)
  return `SQ${Math.max(0, ...nums) + 1}`
}

export function ResearchQuestionsEditor() {
  const qc = useQueryClient()
  const q = useQuery({ queryKey: ["questions"], queryFn: api.questions })
  const [text, setText] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [edits, setEdits] = useState<Record<string, string>>({})
  const done = (list: ResearchQuestion[]) => {
    qc.setQueryData(["questions"], list)
    qc.invalidateQueries({ queryKey: ["codebook"] })
    qc.invalidateQueries({ queryKey: ["codes"] })
    setError(null)
  }
  const add = useMutation({ mutationFn: (t: string) => api.addQuestion(nextQuestionId(q.data ?? []), t), onSuccess: (l) => (done(l), setText("")), onError: (e) => setError(msg(e)) })
  const edit = useMutation({ mutationFn: (v: { id: string; text: string }) => api.editQuestion(v.id, v.text), onSuccess: done, onError: (e) => setError(msg(e)) })
  const remove = useMutation({ mutationFn: (id: string) => api.deleteQuestion(id), onSuccess: done, onError: (e) => setError(msg(e)) })
  const list = q.data ?? []
  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-sm font-medium">{t("common.questions_de_recherche")}</h3>
      {list.map((r) => (
        <div key={r.id} className="r8">
          <span className="badge">{r.id}</span>
          <Input
            aria-label={`Question ${r.id}`}
            style={{ flex: 1 }}
            value={edits[r.id] ?? r.text}
            onChange={(e) => setEdits({ ...edits, [r.id]: e.target.value })}
            onBlur={() => {
              const t = (edits[r.id] ?? r.text).trim()
              setEdits(({ [r.id]: _drop, ...rest }) => rest)
              if (t && t !== r.text) edit.mutate({ id: r.id, text: t })
            }}
          />
          <Button variant="outline" aria-label={t("settings.research_questions.retirer_la_question_id", { id: r.id })} onClick={() => remove.mutate(r.id)}>
            {t("common.retirer")}
          </Button>
        </div>
      ))}
      <form
        className="r8"
        onSubmit={(e) => {
          e.preventDefault()
          if (text.trim()) add.mutate(text.trim())
        }}
      >
        <Input aria-label={t("settings.research_questions.nouvelle_question_de_recherche")} placeholder={t("settings.research_questions.nouvelle_question_de_recherche")} style={{ flex: 1 }} value={text} onChange={(e) => setText(e.target.value)} />
        <Button type="submit" variant="outline" disabled={!text.trim() || add.isPending}>
          {t("common.ajouter_une_question")}
        </Button>
      </form>
      <p className="small l2">{t("settings.research_questions.chaque_question_garde_son_identifian")}</p>
      {error && <Alerts messages={[error]} />}
    </div>
  )
}
