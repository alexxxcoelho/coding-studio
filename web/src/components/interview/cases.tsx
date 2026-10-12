// Case synthesis: one record per identifiable project in the interview, five
// fields (trigger, alternatives, constraints, choice, result), each backed by
// units of the interview.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Plus, X } from "lucide-react"

import { Banner, EmptyState } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { fieldFilled, FIELD_LABELS } from "@/lib/crosscase"
import { dateTime, roleLabel } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref, unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import { CASE_FIELDS, type Case, type CaseFieldKey, type InterviewDetail, type Unit } from "@/lib/types"

const STAGES = ["STAGE-INIT", "STAGE-PILOT", "STAGE-ROLLOUT", "STAGE-ROUTINE", "STAGE-EXTEND"]
const STATUSES = ["STATUS-STABLE", "STATUS-SCALED", "STATUS-STALLED", "STATUS-DISCONT", "STATUS-FAILED", "STATUS-MIXED", "STATUS-UNCLEAR"]

function short(unitId: string): string {
  return `U${unitId.split("-U").pop()}`
}

/** A unit used as evidence: chip linking to the unit in Coding, removable. */
function EvidenceChip({ unitId, onRemove }: { unitId: string; onRemove?: () => void }) {
  return (
    <span className="bg-muted inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs">
      <a href={unitHref(unitId)} className="ident hover:underline" title={`Open ${unitId} in Coding`}>
        {short(unitId)}
      </a>
      {onRemove && (
        <button type="button" onClick={onRemove} aria-label={`Remove ${unitId}`} className="text-muted-foreground hover:text-foreground">
          <X className="size-3" />
        </button>
      )}
    </span>
  )
}

function UnitPicker({ units, chosen, onPick }: { units: Unit[]; chosen: string[]; onPick: (unitId: string) => void }) {
  const [open, setOpen] = useState(false)
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="xs" variant="outline">
          <Plus /> Evidence
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[30rem] p-0" align="start">
        <Command
          filter={(value, search) => {
            const u = units.find((x) => x.unit_id === value)
            const q = search.toLowerCase()
            return u && (u.unit_id.toLowerCase().includes(q) || u.text.toLowerCase().includes(q)) ? 1 : 0
          }}
        >
          <CommandInput placeholder="Search the units of this interview" />
          <CommandList className="max-h-80">
            <CommandEmpty>No unit matches.</CommandEmpty>
            <CommandGroup>
              {units.map((u) => (
                <CommandItem
                  key={u.unit_id}
                  value={u.unit_id}
                  disabled={chosen.includes(u.unit_id)}
                  onSelect={() => {
                    onPick(u.unit_id)
                    setOpen(false)
                  }}
                  className="items-start"
                >
                  <span className="ident text-muted-foreground w-12 shrink-0">{short(u.unit_id)}</span>
                  <span className="text-muted-foreground w-9 shrink-0 text-xs">{roleLabel(u.role)}</span>
                  <span className="line-clamp-2 text-xs">{u.text}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

function CaseEditor({ initial, units, onSaved }: { initial: Case; units: Unit[]; onSaved: (c: Case) => void }) {
  const qc = useQueryClient()
  const [c, setC] = useState<Case>(initial)
  const dirty = JSON.stringify(c) !== JSON.stringify(initial)
  const setField = (k: CaseFieldKey, patch: Partial<Case[CaseFieldKey]>) => setC((x) => ({ ...x, [k]: { ...x[k], ...patch } }))
  const save = useMutation({
    mutationFn: () => api.editCase(c),
    onSuccess: (saved) => {
      toast.success(`${saved.case_id} saved`)
      qc.invalidateQueries({ queryKey: ["cases"] })
      qc.invalidateQueries({ queryKey: ["cross-case"] })
      invalidateInterview(qc, c.interview_id)
      onSaved(saved)
    },
    onError: (e) => reportError(e, "Case not saved"),
  })
  const textOf = (id: string) => units.find((u) => u.unit_id === id)?.text ?? ""

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-3 md:grid-cols-[1fr_12rem_12rem]">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Case label</span>
          <Input value={c.label} onChange={(e) => setC({ ...c, label: e.target.value })} placeholder="Item-level RFID in a department store" />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Stage</span>
          <select className="border-input bg-background h-9 rounded-md border px-2 text-sm" value={c.stage ?? ""} onChange={(e) => setC({ ...c, stage: e.target.value || null })}>
            <option value="">Not set</option>
            {STAGES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Outcome status</span>
          <select
            className="border-input bg-background h-9 rounded-md border px-2 text-sm"
            value={c.outcome_status ?? ""}
            onChange={(e) => setC({ ...c, outcome_status: e.target.value || null })}
          >
            <option value="">Not set</option>
            {STATUSES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
      </div>

      {CASE_FIELDS.map((k) => {
        const f = c[k]
        return (
          <section key={k} className="grid gap-2 md:grid-cols-[10rem_1fr]">
            <div>
              <h3 className="text-sm font-semibold">{FIELD_LABELS[k].title}</h3>
              <p className="text-muted-foreground text-xs">{FIELD_LABELS[k].hint}</p>
            </div>
            <div className="flex flex-col gap-2">
              <Textarea rows={2} value={f.text} onChange={(e) => setField(k, { text: e.target.value })} placeholder="In your words, grounded in the units below" />
              <div className="flex flex-wrap items-center gap-1.5">
                {f.evidence.map((u) => (
                  <span key={u} title={textOf(u)}>
                    <EvidenceChip unitId={u} onRemove={() => setField(k, { evidence: f.evidence.filter((x) => x !== u) })} />
                  </span>
                ))}
                <UnitPicker units={units} chosen={f.evidence} onPick={(u) => setField(k, { evidence: [...f.evidence, u] })} />
              </div>
            </div>
          </section>
        )
      })}

      <label className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">Notes</span>
        <Textarea rows={2} value={c.notes ?? ""} onChange={(e) => setC({ ...c, notes: e.target.value || null })} />
      </label>

      <div className="flex items-center justify-between border-t pt-3">
        <span className={cn("text-xs", dirty ? "text-warn" : "text-muted-foreground")}>
          {dirty ? "Unsaved changes" : initial.updated_at ? `Saved ${dateTime(initial.updated_at)}` : "Saved"}
        </span>
        <Button onClick={() => save.mutate()} disabled={!dirty || save.isPending}>
          {save.isPending ? "Saving..." : "Save the case"}
        </Button>
      </div>
    </div>
  )
}

const emptyField = () => ({ text: "", evidence: [] as string[] })

export function CasesTab({ id, detail }: { id: string; detail: InterviewDetail }) {
  const qc = useQueryClient()
  const cases = useQuery({ queryKey: ["cases", id], queryFn: () => api.cases(id) })
  const [selected, setSelected] = useState<string | null>(null)
  const units = useMemo(() => detail.segmentation?.units ?? [], [detail.segmentation])

  const add = useMutation({
    mutationFn: () =>
      api.addCase(id, {
        label: `Case ${(cases.data?.length ?? 0) + 1}`,
        trigger: emptyField(),
        alternatives: emptyField(),
        constraints: emptyField(),
        choice: emptyField(),
        result: emptyField(),
        stage: null,
        outcome_status: null,
        notes: null,
      }),
    onSuccess: (c) => {
      qc.invalidateQueries({ queryKey: ["cases"] })
      qc.invalidateQueries({ queryKey: ["cross-case"] })
      invalidateInterview(qc, id)
      setSelected(c.case_id)
      toast.success(`${c.case_id} created`)
    },
    onError: (e) => reportError(e, "Case not created"),
  })

  if (!detail.segmentation)
    return (
      <EmptyState
        title="No units yet"
        action={
          <Button asChild>
            <a href={interviewHref(id, "sources")}>Go to Sources</a>
          </Button>
        }
      >
        A case is backed by units of the interview: segment a transcript first.
      </EmptyState>
    )
  if (cases.isLoading) return <Skeleton className="h-72 w-full" />
  if (cases.error) return <Banner tone="danger" title="Cases unavailable">{cases.error.message}</Banner>

  const list = cases.data ?? []
  const current = list.find((c) => c.case_id === selected) ?? list[0] ?? null

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[17rem_minmax(0,1fr)]">
      <aside className="bg-card flex flex-col rounded-md border">
        <div className="flex items-center justify-between border-b px-3 py-2">
          <h2 className="text-sm font-semibold">Cases</h2>
          <Button size="xs" variant="outline" onClick={() => add.mutate()} disabled={add.isPending}>
            <Plus /> New case
          </Button>
        </div>
        {list.length === 0 ? (
          <p className="text-muted-foreground p-3 text-sm">
            No case yet. A case is one identifiable project the respondent describes; an interview may hold several.
          </p>
        ) : (
          <ul className="flex flex-col">
            {list.map((c) => {
              const filled = CASE_FIELDS.filter((k) => fieldFilled(c, k)).length
              return (
                <li key={c.case_id}>
                  <button
                    type="button"
                    onClick={() => setSelected(c.case_id)}
                    className={cn("flex w-full flex-col items-start gap-0.5 border-b px-3 py-2 text-left", current?.case_id === c.case_id ? "bg-accent/60" : "hover:bg-muted/50")}
                  >
                    <span className="ident text-xs">{c.case_id}</span>
                    <span className="text-sm">{c.label || "Untitled case"}</span>
                    <span className="text-muted-foreground text-xs">{filled} of 5 fields filled</span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </aside>
      <section className="bg-card rounded-md border p-4">
        {current ? (
          <CaseEditor key={`${current.case_id}:${current.updated_at ?? ""}`} initial={current} units={units} onSaved={(c) => setSelected(c.case_id)} />
        ) : (
          <p className="text-muted-foreground text-sm">Create a case to synthesise what the respondent describes: trigger, alternatives, constraints, choice, result.</p>
        )}
      </section>
    </div>
  )
}
