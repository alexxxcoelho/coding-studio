// Labels to clarify, across interviews: each label is resolved by creating a
// field code or by mapping it to an existing code, for every unit carrying it.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeChip, EmptyState, PageTitle, PendingChip, Section } from "@/components/bits"
import { CodePicker } from "@/components/code-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { groupByLabel, type LabelGroup } from "@/lib/clarify"
import { codeKeyFrom, phase2Codes } from "@/lib/codes"
import { QuestionPicker, ToePicker } from "@/components/code-fields"
import { dateTime, plural } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Code, ResearchQuestion, Toe } from "@/lib/types"

function ResolveForm({ group, codes, questions }: { group: LabelGroup; codes: Code[]; questions: ResearchQuestion[] }) {
  const qc = useQueryClient()
  const [mode, setMode] = useState<"new" | "map">("new")
  const [key, setKey] = useState(codeKeyFrom(group.label))
  const [label, setLabel] = useState(group.label)
  const [definition, setDefinition] = useState("")
  const [toe, setToe] = useState<Toe | null>(null)
  const [rqs, setRqs] = useState<string[]>([])
  const [example, setExample] = useState(group.items[0]?.unit_id ?? "")
  const [mapped, setMapped] = useState<string[]>([])
  const [rationale, setRationale] = useState("")
  const [all, setAll] = useState(true)

  const target = mode === "new" ? key.trim().toUpperCase() : mapped[0] ?? ""
  const resolve = useMutation({
    mutationFn: () => {
      const newCode: Code | undefined =
        mode === "new"
          ? {
              code: target,
              family: toe === "T" ? "TECH" : toe === "O" ? "ORG" : toe === "E" ? "ENV" : "OTHER",
              label: label.trim(),
              definition: definition.trim(),
              origin: "field",
              active: true,
              in_prompt: true,
              toe,
              research_questions: rqs,
              merged_into: null,
              version: 1,
              example: example.trim() || null,
              reason: `From the label to clarify "${group.label}"`,
              created_at: null,
            }
          : undefined
      return api.resolveToClarify(group.items[0].id, {
        kind: mode === "new" ? "new_code" : "mapped",
        code: target,
        rationale: rationale.trim(),
        all_with_label: all,
        new_code: newCode,
      })
    },
    onSuccess: (res) => {
      qc.setQueryData(["codebook"], res.codebook)
      qc.invalidateQueries({ queryKey: ["to-clarify"] })
      qc.invalidateQueries({ predicate: (q) => ["units", "interview", "interviews", "chain", "review", "final"].includes(String(q.queryKey[0])) })
      toast.success(`"${group.label}" resolved into ${target} on ${plural(res.resolved.length, "unit")}`)
    },
    onError: (e) => reportError(e, "Not resolved"),
  })
  const valid = mode === "new" ? !!target && !!label.trim() && !!definition.trim() : mapped.length === 1

  return (
    <div className="flex flex-col gap-3 border-t pt-3">
      <div className="flex gap-1.5" role="radiogroup" aria-label="Resolution">
        <Button size="sm" variant={mode === "new" ? "default" : "outline"} onClick={() => setMode("new")}>
          Create a field code
        </Button>
        <Button size="sm" variant={mode === "map" ? "default" : "outline"} onClick={() => setMode("map")}>
          Map to an existing code
        </Button>
      </div>
      {mode === "new" ? (
        <div className="grid gap-3 md:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Code key</span>
            <Input value={key} onChange={(e) => setKey(e.target.value)} className="ident uppercase" />
          </label>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Label</span>
            <Input value={label} onChange={(e) => setLabel(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1 text-xs md:col-span-2">
            <span className="text-muted-foreground">Definition, required: what a unit must say to carry the code</span>
            <Textarea rows={2} value={definition} onChange={(e) => setDefinition(e.target.value)} />
          </label>
          <div className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">TOE attachment</span>
            <ToePicker value={toe} onChange={setToe} />
          </div>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Example, a unit ID or a short quote</span>
            <Input value={example} onChange={(e) => setExample(e.target.value)} />
          </label>
          <div className="flex flex-col gap-1 text-xs md:col-span-2">
            <span className="text-muted-foreground">Research questions it bears on</span>
            <QuestionPicker questions={questions} value={rqs} onChange={setRqs} />
          </div>
        </div>
      ) : (
        <CodePicker allowed={codes} value={mapped} onChange={(v) => setMapped(v.filter((c) => c !== "NA").slice(-1))} phase={2} phase1Contexts={[]} />
      )}
      <Textarea rows={2} placeholder="Rationale: why this label is this code" value={rationale} onChange={(e) => setRationale(e.target.value)} />
      <div className="flex flex-wrap items-center gap-4">
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={all} onCheckedChange={(v) => setAll(v === true)} />
          Apply to all {plural(group.items.length, "unit")} with this label
        </label>
        <Button size="sm" disabled={!valid || resolve.isPending} onClick={() => resolve.mutate()}>
          Resolve into {target || "a code"}
        </Button>
      </div>
    </div>
  )
}

function GroupCard({ group, codes, questions }: { group: LabelGroup; codes: Code[]; questions: ResearchQuestion[] }) {
  const [open, setOpen] = useState(false)
  return (
    <article className="bg-card rounded-md border">
      <header className="flex flex-wrap items-center justify-between gap-2 px-4 py-2.5">
        <div className="flex flex-wrap items-center gap-2">
          <PendingChip label={group.label} />
          <span className="text-muted-foreground text-xs">
            {plural(group.items.length, "unit")} in {group.interviews.join(", ")}
          </span>
        </div>
        <Button size="sm" variant={open ? "ghost" : "outline"} onClick={() => setOpen((o) => !o)}>
          {open ? "Close" : "Resolve"}
        </Button>
      </header>
      <ul className="flex flex-col border-t">
        {group.items.map((t) => (
          <li key={t.id} className="grid gap-x-3 gap-y-0.5 border-b px-4 py-2 last:border-b-0 md:grid-cols-[8rem_1fr]">
            <a href={unitHref(t.unit_id)} className="ident text-xs hover:underline">
              {t.unit_id}
            </a>
            <div className="min-w-0">
              {t.unit_text && <p className="verbatim line-clamp-2 text-sm">{t.unit_text}</p>}
              {t.note && <p className="text-muted-foreground text-xs">Note: {t.note}</p>}
            </div>
          </li>
        ))}
      </ul>
      {open && (
        <div className="px-4 pb-4">
          <ResolveForm group={group} codes={codes} questions={questions} />
        </div>
      )}
    </article>
  )
}

export function ToClarifyPage() {
  const items = useQuery({ queryKey: ["to-clarify", "all"], queryFn: () => api.toClarify() })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const questions = useQuery({ queryKey: ["questions"], queryFn: api.questions })

  const open = useMemo(() => groupByLabel((items.data ?? []).filter((t) => !t.resolved)), [items.data])
  const resolved = useMemo(() => (items.data ?? []).filter((t) => t.resolved).sort((a, b) => (b.resolved!.at ?? "").localeCompare(a.resolved!.at ?? "")), [items.data])
  const codes = phase2Codes(book.data?.codes ?? [])

  return (
    <>
      <PageTitle
        title="To clarify"
        sub="Provisional labels put on units during phase 2. Each becomes a field code or joins an existing code; field is a property of the code, not a category next to T, O and E."
      />
      {items.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : open.length === 0 ? (
        <EmptyState title="Nothing to clarify">
          In phase 2, press ? on a unit that no code captures yet to give it a provisional label and a short note.
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-3">
          {open.map((g) => (
            <GroupCard key={g.key} group={g} codes={codes} questions={questions.data ?? []} />
          ))}
        </div>
      )}

      {resolved.length > 0 && (
        <Section title="Resolved" aside={plural(resolved.length, "label")} className="mt-6">
          <ul className="flex flex-col gap-2">
            {resolved.map((t) => (
              <li key={t.id} className="flex flex-wrap items-center gap-2 text-sm">
                <PendingChip label={t.label} className="opacity-60" />
                <span className="text-muted-foreground">to</span>
                <CodeChip code={t.resolved!.code} />
                <Badge variant="outline" className={cn("text-xs")}>
                  {t.resolved!.kind === "new_code" ? "new field code" : "mapped"}
                </Badge>
                <a href={unitHref(t.unit_id)} className="ident text-xs hover:underline">
                  {t.unit_id}
                </a>
                <span className="text-muted-foreground text-xs">
                  {t.resolved!.by}, {dateTime(t.resolved!.at)}
                  {t.resolved!.rationale ? `: ${t.resolved!.rationale}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      )}
    </>
  )
}
