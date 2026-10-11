import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, ChipList, CodeChip, EmptyState, Section } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { CodePicker } from "@/components/code-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { diffCodes, familyOf, sameSet } from "@/lib/codes"
import { dateTime, percent, plural, roleLabel } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { AgreementReport, Arbitration, ArbitrationChoice, Code, DivergenceItem, Phase } from "@/lib/types"

function choiceLabel(c: ArbitrationChoice): string {
  if (c.type === "human") return "Kept your coding"
  if (c.type === "ai") return `Took ${c.run_label}`
  return "Custom decision"
}

function DiffRow({ label, codes, other, book }: { label: string; codes: string[]; other: string[][]; book: Code[] }) {
  // A code is "only" here when no other side carries it.
  const union = [...new Set(other.flat())]
  return (
    <div className="grid grid-cols-[6.5rem_1fr] items-center gap-2 text-sm">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="flex flex-wrap gap-1">
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
      <p className="mt-1">{a.rationale}</p>
      <p className="text-muted-foreground mt-0.5 text-xs">
        Decided by {a.decided_by}, {dateTime(a.decided_at)}
      </p>
    </div>
  )
}

function DivergenceCard({
  id,
  phase,
  item,
  runs,
  book,
  frozen,
}: {
  id: string
  phase: Phase
  item: DivergenceItem
  runs: string[]
  book: Code[]
  frozen: boolean
}) {
  const qc = useQueryClient()
  const d = item.divergence
  const decided = item.arbitration
  const [editing, setEditing] = useState(!decided)
  const [choice, setChoice] = useState<ArbitrationChoice>(decided?.choice ?? { type: "human" })
  const [custom, setCustom] = useState<string[]>(decided?.choice.type === "custom" ? decided.final_codes : d.human)
  const [rationale, setRationale] = useState("")

  const finalCodes = choice.type === "human" ? d.human : choice.type === "ai" ? d.ai[choice.run_label] ?? [] : custom
  const save = useMutation({
    mutationFn: () => api.arbitrate(id, phase, d.unit_id, choice, finalCodes, rationale.trim()),
    onSuccess: () => {
      toast.success(`${d.unit_id} decided`)
      setEditing(false)
      setRationale("")
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Decision refused"),
  })

  const sides: [string, string[]][] = [["Your coding", d.human], ...runs.map((r) => [`AI ${r}`, d.ai[r] ?? []] as [string, string[]])]

  return (
    <article className={cn("bg-card rounded-md border", decided && !editing && "opacity-90")}>
      <header className="flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2">
        <div className="flex items-center gap-2 text-sm">
          <span className="ident font-semibold">{d.unit_id}</span>
          <span className="text-muted-foreground tabular-nums">{item.unit.timestamp}</span>
          <Badge variant="outline">{roleLabel(item.unit.role)}</Badge>
          <span className="text-muted-foreground text-xs">
            {d.kind === "human_vs_ai" ? "your coding differs from the AI" : "the AI runs disagree among themselves"}
          </span>
        </div>
        {decided ? <Badge className="bg-ok-soft text-ok">decided</Badge> : <Badge className="bg-warn-soft text-warn">pending</Badge>}
      </header>
      <div className="grid gap-4 p-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <p className="verbatim">{item.unit.text}</p>
        <div className="flex flex-col gap-2">
          {sides.map(([label, codes], i) => (
            <DiffRow key={label} label={label} codes={codes} other={sides.filter((_, j) => j !== i).map((s) => s[1])} book={book} />
          ))}
          <p className="text-muted-foreground text-xs">Outlined codes appear on one side only.</p>
        </div>
      </div>

      <div className="border-t px-4 py-3">
        {decided && !editing ? (
          <div className="flex items-start justify-between gap-3">
            <Decision a={decided} />
            {!frozen && (
              <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
                Revise
              </Button>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Decision">
              <Button size="sm" variant={choice.type === "human" ? "default" : "outline"} onClick={() => setChoice({ type: "human" })} disabled={frozen}>
                Keep your coding
              </Button>
              {runs.map((r) => (
                <Button
                  key={r}
                  size="sm"
                  variant={choice.type === "ai" && choice.run_label === r ? "default" : "outline"}
                  onClick={() => setChoice({ type: "ai", run_label: r })}
                  disabled={frozen || sameSet(d.ai[r] ?? [], d.human)}
                >
                  Take {r}
                </Button>
              ))}
              <Button size="sm" variant={choice.type === "custom" ? "default" : "outline"} onClick={() => setChoice({ type: "custom" })} disabled={frozen}>
                Custom
              </Button>
            </div>
            {choice.type === "custom" && <CodePicker allowed={item.allowed} value={custom} onChange={setCustom} disabled={frozen} />}
            <div className="flex items-center gap-2 text-xs">
              <span className="text-muted-foreground">Final codes</span>
              <ChipList codes={finalCodes} book={book} empty="none chosen" />
            </div>
            <Textarea
              rows={2}
              placeholder="Rationale, required: why this coding fits what the respondent says."
              value={rationale}
              onChange={(e) => setRationale(e.target.value)}
              disabled={frozen}
            />
            <div className="flex gap-2">
              <Button size="sm" disabled={frozen || !rationale.trim() || !finalCodes.length || save.isPending} onClick={() => save.mutate()}>
                {decided ? "Record the revised decision" : "Record the decision"}
              </Button>
              {decided && (
                <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
                  Cancel
                </Button>
              )}
            </div>
          </div>
        )}
        {item.history.length > 1 && (
          <Collapsible className="mt-3">
            <CollapsibleTrigger className="text-primary text-xs hover:underline">
              {plural(item.history.length, "decision")} in the history
            </CollapsibleTrigger>
            <CollapsibleContent className="mt-2 flex flex-col gap-2 border-l-2 pl-3">
              {item.history.map((a, i) => (
                <Decision key={i} a={a} />
              ))}
            </CollapsibleContent>
          </Collapsible>
        )}
      </div>
    </article>
  )
}

export function ReconciliationTab({ id, phase }: { id: string; phase: Phase }) {
  const state = useQuery({ queryKey: ["phase", id, phase], queryFn: () => api.phase(id, phase) })
  const agreement = useQuery({ queryKey: ["agreement", id, phase], queryFn: () => api.agreement(id, phase) })
  const div = useQuery({ queryKey: ["divergences", id, phase], queryFn: () => api.divergences(id, phase) })
  const fin = useQuery({ queryKey: ["final", id, phase], queryFn: () => api.final(id, phase) })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const [pendingOnly, setPendingOnly] = useState(false)

  if (state.isLoading || div.isLoading) return <Skeleton className="h-96 w-full" />
  if (div.error) return <Banner tone="danger" title="Divergences unavailable">{div.error.message}</Banner>
  const st = state.data
  const reports = agreement.data?.reports ?? []
  const items = div.data?.items ?? []
  const shown = pendingOnly ? items.filter((i) => !i.arbitration) : items

  if (!reports.length)
    return (
      <EmptyState
        title="Nothing to compare yet"
        action={
          <Button asChild>
            <a href={interviewHref(id, "ai", phase)}>Go to the AI pass</a>
          </Button>
        }
      >
        Import an AI answer that covers every unit of this phase; its codes are then compared with yours here.
      </EmptyState>
    )

  return (
    <div className="flex flex-col gap-5">
      {fin.data?.reconciled ? (
        <Banner
          tone="ok"
          title={`Phase ${phase} is reconciled`}
          action={
            phase === 1 ? (
              <Button size="sm" asChild>
                <a href={interviewHref(id, "coding", 2)}>Start phase 2</a>
              </Button>
            ) : (
              <Button size="sm" variant="outline" asChild>
                <a href={interviewHref(id, "audit")}>Export the package</a>
              </Button>
            )
          }
        >
          Every unit has a final coding, agreed or decided with a rationale.
        </Banner>
      ) : (
        fin.data && (
          <Banner tone="warn" title={`${plural(fin.data.pending.length, "unit")} still pending`}>
            Phase {phase} is reconciled once every divergence is decided.
          </Banner>
        )
      )}
      {st?.frozen && <Banner tone="lock" title="Phase 1 is frozen">Phase 2 has started: these decisions can no longer change.</Banner>}

      <Section title="Agreement" aside="computed without any model">
        <div className="flex flex-col gap-3">
          {reports.map((r) => (
            <Collapsible key={`${r.label_a}-${r.label_b}`}>
              <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                <span>
                  <span className="font-medium">
                    {r.label_a} and {r.label_b}
                  </span>
                  : same codes on {r.n_exact} of {r.n_units} units ({percent(r.n_units ? r.n_exact / r.n_units : 0)}),{" "}
                  {plural(r.divergent_unit_ids.length, "divergent unit")}
                </span>
                <CollapsibleTrigger className="text-primary text-xs hover:underline">Kappa per code</CollapsibleTrigger>
              </div>
              <CollapsibleContent className="mt-2 overflow-x-auto rounded-md border">
                <KappaTable report={r} />
              </CollapsibleContent>
            </Collapsible>
          ))}
          <p className="text-muted-foreground text-xs">
            Exact agreement on the full set of codes, then Cohen's kappa per code on presence or absence, as in compute_agreement.py.
          </p>
        </div>
      </Section>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-base font-semibold">
          Divergences{" "}
          <span className="text-muted-foreground text-sm font-normal">
            {div.data!.n_decided} of {div.data!.n_divergences} decided
          </span>
        </h2>
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={pendingOnly} onCheckedChange={(v) => setPendingOnly(v === true)} />
          Pending only
        </label>
      </div>
      {shown.length === 0 ? (
        <p className="text-muted-foreground text-sm">{items.length ? "No pending divergence." : "Your coding and the AI coding agree on every unit."}</p>
      ) : (
        shown.map((item) => (
          <DivergenceCard
            key={item.divergence.unit_id}
            id={id}
            phase={phase}
            item={item}
            runs={div.data!.runs}
            book={book.data?.codes ?? []}
            frozen={!!st?.frozen}
          />
        ))
      )}
    </div>
  )
}
