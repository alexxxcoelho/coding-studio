// Review of the AI challenge (phase 2): every disagreement, then a seeded
// sample of agreements. The candidate keeps the final decision; agreement
// figures are descriptive and sit in a secondary panel.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, ChipList, CodeChip, EmptyState, Section } from "@/components/bits"
import { CodePicker } from "@/components/code-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Progress } from "@/components/ui/progress"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { diffCodes, familyOf, sameSet } from "@/lib/codes"
import { dateTime, percent, plural, roleLabel } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { orderReview, rationaleRequired, reviewProgress } from "@/lib/review"
import { interviewHref, unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { AgreementReport, Arbitration, ArbitrationChoice, Code, ReviewItem } from "@/lib/types"

function choiceLabel(c: ArbitrationChoice): string {
  if (c.type === "human") return "Kept your coding"
  if (c.type === "ai") return `Took ${c.run_label}`
  return "Custom decision"
}

function DiffRow({ label, codes, other, book }: { label: string; codes: string[]; other: string[][]; book: Code[] }) {
  const union = [...new Set(other.flat())]
  return (
    <div className="grid grid-cols-[6.5rem_1fr] items-center gap-2 text-sm">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="flex flex-wrap gap-1">
        {codes.length === 0 && <span className="text-muted-foreground text-xs">no code</span>}
        {diffCodes(codes, union).map(({ code, mark }) => (
          <CodeChip key={code} code={code} family={familyOf(code, book)} mark={mark} title={mark === "only" ? "Only on this side" : "Shared"} />
        ))}
      </span>
    </div>
  )
}

function KappaTable({ report }: { report: AgreementReport }) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Code</TableHead>
          <TableHead className="text-right">{report.label_a}</TableHead>
          <TableHead className="text-right">{report.label_b}</TableHead>
          <TableHead className="text-right">Raw agreement</TableHead>
          <TableHead className="text-right">Cohen's kappa</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {report.per_code.map((c) => (
          <TableRow key={c.code}>
            <TableCell>
              <CodeChip code={c.code} />
            </TableCell>
            <TableCell className="text-right tabular-nums">{c.n_a}</TableCell>
            <TableCell className="text-right tabular-nums">{c.n_b}</TableCell>
            <TableCell className="text-right tabular-nums">{percent(c.raw, 1)}</TableCell>
            <TableCell className="text-right tabular-nums">{c.kappa === null ? "undefined" : c.kappa.toFixed(3)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}

function Decision({ a }: { a: Arbitration }) {
  return (
    <div className="text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{choiceLabel(a.choice)}</span>
        <ChipList codes={a.final_codes} />
      </div>
      {a.rationale && <p className="mt-1">{a.rationale}</p>}
      <p className="text-muted-foreground mt-0.5 text-xs">
        Decided by {a.decided_by}, {dateTime(a.decided_at)}
      </p>
    </div>
  )
}

function ReviewCard({ id, item, runs, book }: { id: string; item: ReviewItem; runs: string[]; book: Code[] }) {
  const qc = useQueryClient()
  const decided = item.decision
  const required = rationaleRequired(item)
  const [editing, setEditing] = useState(!decided)
  const [choice, setChoice] = useState<ArbitrationChoice>(decided?.choice ?? { type: "human" })
  const [custom, setCustom] = useState<string[]>(decided?.choice.type === "custom" ? decided.final_codes : item.human)
  const [rationale, setRationale] = useState("")

  const finalCodes = choice.type === "human" ? item.human : choice.type === "ai" ? item.ai[choice.run_label] ?? [] : custom
  const save = useMutation({
    mutationFn: () => api.decide(id, item.unit_id, choice, finalCodes, rationale.trim()),
    onSuccess: () => {
      toast.success(`${item.unit_id} reviewed`)
      setEditing(false)
      setRationale("")
      qc.invalidateQueries({ queryKey: ["review", id] })
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Decision refused"),
  })

  const sides: [string, string[]][] = [["Your coding", item.human], ...runs.map((r) => [`AI ${r}`, item.ai[r] ?? []] as [string, string[]])]
  const sample = item.kind !== "disagreement"
  const canSave = finalCodes.length > 0 && (!required || rationale.trim().length > 0) && !save.isPending

  return (
    <article className={cn("bg-card rounded-md border", decided && !editing && "opacity-90", sample && "border-dashed")}>
      <header className="flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2">
        <div className="flex items-center gap-2 text-sm">
          <a href={unitHref(item.unit_id)} className="ident font-semibold hover:underline">
            {item.unit_id}
          </a>
          <span className="text-muted-foreground tabular-nums">{item.unit.timestamp}</span>
          <Badge variant="outline">{roleLabel(item.unit.role)}</Badge>
          <span className="text-muted-foreground text-xs">
            {sample ? "sampled agreement: check that the shared reading holds" : "your coding differs from the AI"}
          </span>
        </div>
        {decided ? (
          <Badge className="bg-ok-soft text-ok">{sample ? "reviewed" : "decided"}</Badge>
        ) : (
          <Badge className={sample ? "bg-muted text-muted-foreground" : "bg-warn-soft text-warn"}>{sample ? "to look at" : "open"}</Badge>
        )}
      </header>
      <div className="grid gap-4 p-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <p className="verbatim">{item.unit.text}</p>
        <div className="flex flex-col gap-2">
          {sides.map(([label, codes], i) => (
            <DiffRow key={label} label={label} codes={codes} other={sides.filter((_, j) => j !== i).map((s) => s[1])} book={book} />
          ))}
          {!sample && <p className="text-muted-foreground text-xs">Outlined codes appear on one side only.</p>}
        </div>
      </div>

      <div className="border-t px-4 py-3">
        {decided && !editing ? (
          <div className="flex items-start justify-between gap-3">
            <Decision a={decided} />
            <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
              Revise
            </Button>
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Decision">
              <Button size="sm" variant={choice.type === "human" ? "default" : "outline"} onClick={() => setChoice({ type: "human" })}>
                Keep your coding
              </Button>
              {runs.map((r) => (
                <Button
                  key={r}
                  size="sm"
                  variant={choice.type === "ai" && choice.run_label === r ? "default" : "outline"}
                  onClick={() => setChoice({ type: "ai", run_label: r })}
                  disabled={sameSet(item.ai[r] ?? [], item.human)}
                >
                  Take {r}
                </Button>
              ))}
              <Button size="sm" variant={choice.type === "custom" ? "default" : "outline"} onClick={() => setChoice({ type: "custom" })}>
                Custom
              </Button>
            </div>
            {choice.type === "custom" && (
              <CodePicker allowed={book.filter((c) => c.active && !c.merged_into && c.origin !== "legacy")} value={custom} onChange={setCustom} phase={2} phase1Contexts={[]} />
            )}
            <div className="flex items-center gap-2 text-xs">
              <span className="text-muted-foreground">Final codes</span>
              <ChipList codes={finalCodes} book={book} empty="none chosen" />
            </div>
            <Textarea
              rows={2}
              placeholder={
                required
                  ? "Rationale, required: what the respondent says that settles the reading."
                  : "Note, optional: what this agreement shows about the code."
              }
              value={rationale}
              onChange={(e) => setRationale(e.target.value)}
            />
            <div className="flex gap-2">
              <Button size="sm" disabled={!canSave} onClick={() => save.mutate()}>
                {decided ? "Record the revised decision" : sample ? "Mark as reviewed" : "Record the decision"}
              </Button>
              {decided && (
                <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
                  Cancel
                </Button>
              )}
            </div>
          </div>
        )}
        {(item.history?.length ?? 0) > 1 && (
          <Collapsible className="mt-3">
            <CollapsibleTrigger className="text-primary text-xs hover:underline">{plural(item.history!.length, "decision")} in the history</CollapsibleTrigger>
            <CollapsibleContent className="mt-2 flex flex-col gap-2 border-l-2 pl-3">
              {item.history!.map((a, i) => (
                <Decision key={i} a={a} />
              ))}
            </CollapsibleContent>
          </Collapsible>
        )}
      </div>
    </article>
  )
}

export function ReviewTab({ id }: { id: string }) {
  const review = useQuery({ queryKey: ["review", id], queryFn: () => api.review(id) })
  const agreement = useQuery({ queryKey: ["agreement", id], queryFn: () => api.agreement(id) })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const [openOnly, setOpenOnly] = useState(false)

  if (review.isLoading) return <Skeleton className="h-96 w-full" />
  if (review.error) return <Banner tone="danger" title="Review unavailable">{review.error.message}</Banner>
  const r = review.data!
  const items = r.items ?? []
  const prog = reviewProgress(items)
  const shown = orderReview(items, openOnly)
  const reports = agreement.data?.reports ?? []

  if (!r.runs.length)
    return (
      <EmptyState
        title="Nothing to review yet"
        action={
          <Button asChild>
            <a href={interviewHref(id, "ai")}>Go to the AI challenge</a>
          </Button>
        }
      >
        Import an AI answer that covers the units of a prompt; its codes are then set against yours here.
      </EmptyState>
    )

  return (
    <div className="flex flex-col gap-5">
      <div className="bg-card flex flex-wrap items-center gap-x-8 gap-y-3 rounded-md border px-4 py-3">
        <div className="flex min-w-[14rem] flex-1 items-center gap-3">
          <Progress value={prog.disagreements ? (prog.disagreementsDecided / prog.disagreements) * 100 : 100} className="h-1.5 flex-1" />
          <span className="text-sm tabular-nums">
            {prog.disagreementsDecided} of {plural(prog.disagreements, "disagreement")} decided
          </span>
        </div>
        <span className="text-sm tabular-nums">
          {prog.sampledReviewed} of {plural(prog.sampled, "sampled agreement")} looked at
        </span>
        <span className="text-muted-foreground text-xs">
          {r.seeds?.length ? (
            <>
              Sample drawn with seed{" "}
              {r.seeds.map((s, i) => (
                <span key={s.run_label}>
                  {i > 0 && ", "}
                  <span className="ident">{String(s.seed)}</span> ({s.run_label})
                </span>
              ))}
            </>
          ) : (
            "No sample drawn yet"
          )}
        </span>
        <label className="ml-auto flex items-center gap-2 text-sm">
          <Checkbox checked={openOnly} onCheckedChange={(v) => setOpenOnly(v === true)} />
          Open only
        </label>
      </div>

      {prog.complete && prog.disagreements > 0 && (
        <Banner tone="ok" title="Every disagreement is decided">
          The final coding takes your decisions where you made one and your current codes elsewhere.
        </Banner>
      )}

      {shown.length === 0 ? (
        <p className="text-muted-foreground text-sm">{items.length ? "Nothing left open." : "Your coding and the AI coding agree on every unit."}</p>
      ) : (
        shown.map((item) => <ReviewCard key={`${item.kind}-${item.unit_id}`} id={id} item={item} runs={r.runs} book={book.data?.codes ?? []} />)
      )}

      {reports.length > 0 && (
        <Collapsible>
          <Section
            title={
              <CollapsibleTrigger className="hover:underline">Agreement figures, descriptive</CollapsibleTrigger>
            }
            aside="computed without any model; reported, not a target"
          >
            <div className="text-muted-foreground flex flex-col gap-1 text-sm">
              {reports.map((rep) => (
                <span key={`${rep.label_a}-${rep.label_b}`}>
                  {rep.label_a} and {rep.label_b}: same codes on {rep.n_exact} of {rep.n_units} units (
                  {percent(rep.n_units ? rep.n_exact / rep.n_units : 0)})
                </span>
              ))}
            </div>
            <CollapsibleContent className="mt-3 flex flex-col gap-3">
              {reports.map((rep) => (
                <div key={`${rep.label_a}-${rep.label_b}-t`} className="overflow-x-auto rounded-md border">
                  <KappaTable report={rep} />
                </div>
              ))}
              <p className="text-muted-foreground text-xs">
                Exact agreement on the full set of codes, then Cohen's kappa per code on presence or absence, as in compute_agreement.py.
              </p>
            </CollapsibleContent>
          </Section>
        </Collapsible>
      )}
    </div>
  )
}
