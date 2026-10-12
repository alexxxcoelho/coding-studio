// Answers: per research question, the memo that carries the reading across
// interviews. Four written parts, and the units that support the answer and
// the units that cut against it. Saved per question and audited.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, EmptyState, PageTitle, Section } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { dateTime } from "@/lib/format"
import { memoDirty, memoDraft, memoPayload, MEMO_PARTS, type MemoDraft } from "@/lib/memo"
import { reportError } from "@/lib/notify"
import { parseUnitIds } from "@/lib/crosscase"
import { unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { ResearchQuestion, RqMemo } from "@/lib/types"

function UnitLinks({ ids }: { ids: string[] }) {
  if (!ids.length) return <span className="text-muted-foreground text-xs">none yet</span>
  return (
    <span className="flex flex-wrap gap-x-2 gap-y-1">
      {ids.map((u) => (
        <a key={u} href={unitHref(u)} className="ident text-primary text-xs hover:underline">
          {u}
        </a>
      ))}
    </span>
  )
}

function MemoForm({ rq, memo }: { rq: ResearchQuestion; memo: RqMemo | null }) {
  const qc = useQueryClient()
  const [d, setD] = useState<MemoDraft>(() => memoDraft(memo))
  const set = <K extends keyof MemoDraft>(k: K, v: MemoDraft[K]) => setD((x) => ({ ...x, [k]: v }))
  const dirty = memoDirty(d, memo)
  const save = useMutation({
    mutationFn: () => api.setRqMemo(rq.id, memoPayload(d)),
    onSuccess: () => {
      toast.success(`Memo of ${rq.id} saved`)
      qc.invalidateQueries({ queryKey: ["rq-memo", rq.id] })
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => reportError(e, "Memo not saved"),
  })
  const forIds = parseUnitIds(d.unitsFor)
  const againstIds = parseUnitIds(d.unitsAgainst)

  return (
    <div className="flex flex-col gap-4">
      <Section
        title={
          <span className="flex items-baseline gap-2">
            <span className="ident">{rq.id}</span>
            <span className="text-muted-foreground font-normal">{rq.text}</span>
          </span>
        }
        aside={memo ? `saved ${dateTime(memo.updated_at)}` : "no memo yet"}
      >
        <div className="flex flex-col gap-4">
          {MEMO_PARTS.map((p) => (
            <label key={p.key} className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">{p.title}</span>
              <span className="text-muted-foreground text-xs">{p.hint}</span>
              <Textarea rows={4} value={d[p.key]} onChange={(e) => set(p.key, e.target.value)} />
            </label>
          ))}
        </div>
      </Section>

      <Section title="Units that bear on the answer" aside="unit ids, one per line or separated by commas; checked against the segmentations">
        <div className="grid gap-5 md:grid-cols-2">
          <div className="flex flex-col gap-2">
            <label className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">For the answer</span>
              <Textarea rows={4} className="ident text-xs" value={d.unitsFor} onChange={(e) => set("unitsFor", e.target.value)} placeholder="ECO-07-U003" />
            </label>
            <UnitLinks ids={forIds} />
          </div>
          <div className="flex flex-col gap-2">
            <label className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">Against the answer</span>
              <Textarea rows={4} className="ident text-xs" value={d.unitsAgainst} onChange={(e) => set("unitsAgainst", e.target.value)} placeholder="RET-22-U010" />
            </label>
            <UnitLinks ids={againstIds} />
          </div>
        </div>
      </Section>

      <div className="flex items-center justify-between gap-2">
        <span className={cn("text-xs", dirty ? "text-warn" : "text-muted-foreground")}>{dirty ? "Unsaved changes" : memo ? "Saved" : "Not saved yet"}</span>
        <Button onClick={() => save.mutate()} disabled={!dirty || save.isPending}>
          {save.isPending ? "Saving..." : "Save the memo"}
        </Button>
      </div>
    </div>
  )
}

function MemoEditor({ rq }: { rq: ResearchQuestion }) {
  const memo = useQuery({ queryKey: ["rq-memo", rq.id], queryFn: () => api.rqMemo(rq.id) })
  if (memo.isLoading) return <Skeleton className="h-96 w-full" />
  if (memo.error) return <Banner tone="danger" title="The memo could not be loaded">{memo.error.message}</Banner>
  // Keyed on the saved version: a save reloads the draft from what the server kept.
  return <MemoForm key={memo.data?.updated_at ?? "none"} rq={rq} memo={memo.data ?? null} />
}

export function AnswersPage() {
  const questions = useQuery({ queryKey: ["questions"], queryFn: api.questions })
  const [picked, setPicked] = useState<string | null>(null)
  const sorted = useMemo(() => [...(questions.data ?? [])].sort((a, b) => a.ord - b.ord), [questions.data])
  const current = sorted.find((q) => q.id === picked) ?? sorted[0] ?? null

  return (
    <>
      <PageTitle
        title="Answers"
        sub="For each research question: what the data document, how it is read, the rival readings and the limits, with the units for and against. Carried across interviews."
      />
      {questions.isLoading ? (
        <Skeleton className="h-96 w-full" />
      ) : sorted.length === 0 ? (
        <EmptyState title="No research question yet">Add the research questions in the codebook first; each one gets its memo here.</EmptyState>
      ) : (
        <div className="grid items-start gap-4 lg:grid-cols-[14rem_minmax(0,1fr)]">
          <nav aria-label="Research questions" className="bg-card flex flex-col rounded-md border">
            {sorted.map((q) => (
              <button
                key={q.id}
                type="button"
                aria-current={current?.id === q.id ? "true" : undefined}
                onClick={() => setPicked(q.id)}
                className={cn(
                  "border-b px-3 py-2 text-left text-sm last:border-b-0",
                  current?.id === q.id ? "bg-accent/60 font-medium" : "hover:bg-muted/50",
                )}
              >
                <span className="ident block text-xs">{q.id}</span>
                <span className="text-muted-foreground line-clamp-2 text-xs">{q.text}</span>
              </button>
            ))}
          </nav>
          {current && <MemoEditor rq={current} />}
        </div>
      )}
    </>
  )
}
