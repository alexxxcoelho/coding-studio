// Synthèse par question: one page per research question, with the themes that
// answer it and their mini-syntheses, then the answer in a few paragraphs and
// the limits. The text is saved with a button; it is dated and the earlier
// versions are kept (guide, "Écrire la synthèse par question de recherche").

import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { EmptyState } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { analysisApi, type QuestionSynthesis } from "@/lib/analysis-api"
import { akeys, useQuestions } from "@/lib/analysis-hooks"
import { plural } from "@/lib/analysis-text"
import { reportError } from "@/lib/notify"
import { cn } from "@/lib/utils"

function formatDate(iso: string | null): string {
  if (!iso) return ""
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("fr-CH", { dateStyle: "long", timeStyle: "short" })
}

function SynthesisEditor({ synthesis }: { synthesis: QuestionSynthesis }) {
  const qc = useQueryClient()
  const [answer, setAnswer] = useState(synthesis.answer)
  const [limits, setLimits] = useState(synthesis.limits)
  const [saving, setSaving] = useState(false)
  const dirty = answer !== synthesis.answer || limits !== synthesis.limits

  const save = async () => {
    setSaving(true)
    try {
      const saved = await analysisApi.setQuestionSynthesis(synthesis.rq_id, { answer, limits })
      qc.setQueryData(akeys.synthesis(synthesis.rq_id), saved)
      toast.success("Synthèse enregistrée.")
    } catch (e) {
      reportError(e, "Synthèse non enregistrée")
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <section aria-label="Thèmes qui répondent à la question" className="flex flex-col gap-2">
        <h2 className="hs">Thèmes qui répondent à cette question</h2>
        {synthesis.themes.length === 0 ? (
          <p className="text-muted-foreground text-sm">Aucun thème retenu ne répond encore à cette question : rattachez-la à un thème dans la fiche, dans Analyser.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {synthesis.themes.map((tt) => (
              <li key={tt.theme_id} className="grp p-2.5">
                <div className="text-sm font-semibold">{tt.title}</div>
                <p className={cn("mt-1 text-sm leading-snug", !tt.mini_synthesis && "text-muted-foreground")}>{tt.mini_synthesis || "Mini-synthèse pas encore écrite."}</p>
              </li>
            ))}
          </ul>
        )}
      </section>
      <label className="flex flex-col gap-1">
        <span className="hs">Réponse à la question</span>
        <Textarea rows={9} value={answer} onChange={(e) => setAnswer(e.target.value)} className="font-serif text-base leading-relaxed" placeholder="La réponse vient des thèmes, pas des fréquences." />
      </label>
      <label className="flex flex-col gap-1">
        <span className="hs">Limites</span>
        <Textarea rows={4} value={limits} onChange={(e) => setLimits(e.target.value)} className="font-serif text-base leading-relaxed" />
      </label>
      <div className="flex items-center gap-3">
        <Button onClick={() => void save()} disabled={!dirty || saving}>
          Enregistrer
        </Button>
        <span className="text-muted-foreground text-[11.5px]">
          {synthesis.updated_at ? `Dernière version du ${formatDate(synthesis.updated_at)}. ` : "Pas encore enregistrée. "}
          {synthesis.n_versions > 1 ? `${plural(synthesis.n_versions, "version conservée", "versions conservées")}.` : ""}
        </span>
      </div>
    </div>
  )
}

export function QuestionSynthesisTab() {
  const questionsQ = useQuestions()
  const questions = [...(questionsQ.data ?? [])].sort((a, b) => a.ord - b.ord)
  const [picked, setPicked] = useState<string | null>(null)
  const rqId = questions.some((q) => q.id === picked) ? picked : (questions[0]?.id ?? null)
  const q = useQuery({ queryKey: akeys.synthesis(rqId ?? ""), queryFn: () => analysisApi.questionSynthesis(rqId!), enabled: !!rqId })

  if (questionsQ.isLoading) return <Skeleton className="h-64 w-full" />
  if (questions.length === 0) {
    return (
      <EmptyState title="Aucune question de recherche">
        Les questions viennent des Réglages. Définissez-les, puis rattachez-les à vos thèmes.
        <a href="#/settings" className="text-accent-text ml-1 hover:underline">
          Ouvrir les Réglages
        </a>
      </EmptyState>
    )
  }
  const current = questions.find((x) => x.id === rqId)!
  return (
    <div className="grid min-w-0 gap-6 lg:grid-cols-[220px_minmax(0,1fr)]">
      <nav aria-label="Questions de recherche" className="flex flex-col gap-0.5">
        <h2 className="hs px-1.5 pb-1">Questions de recherche</h2>
        {questions.map((rq) => (
          <button
            key={rq.id}
            type="button"
            onClick={() => setPicked(rq.id)}
            aria-current={rq.id === rqId ? "true" : undefined}
            className={cn("flex flex-col rounded-lg px-2.5 py-1.5 text-left text-sm", rq.id === rqId ? "bg-fill2" : "hover:bg-fill")}
          >
            <span className="font-semibold">{rq.id}</span>
            <span className="text-muted-foreground line-clamp-2 text-[11.5px]">{rq.text}</span>
          </button>
        ))}
      </nav>
      <div className="mx-auto flex w-full max-w-[720px] min-w-0 flex-col gap-3">
        <h1 className="m-0 font-serif text-[22px] leading-tight font-semibold">{current.text}</h1>
        {q.isLoading || !q.data ? (
          q.error ? <p className="text-destructive text-sm">{q.error.message}</p> : <Skeleton className="h-64 w-full" />
        ) : (
          <SynthesisEditor key={`${q.data.rq_id}:${q.data.updated_at}`} synthesis={q.data} />
        )}
      </div>
    </div>
  )
}
