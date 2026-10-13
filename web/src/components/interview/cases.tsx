// Case synthesis: one record per identifiable project in the interview, five
// fields (trigger, alternatives, constraints, choice, result), each backed by
// units of the case. The case's units may come from any interview; the
// evidence of a field must be among them.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Plus, X } from "lucide-react"

import { Banner, EmptyState, Section } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command"
import { FIELD } from "@/components/ui/field-styles"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { ACCOUNT_TYPES, DECISIONS, fieldFilled, FIELD_LABELS, GRANULARITIES, parseList, parseUnitIds, unattributedEvidence } from "@/lib/crosscase"
import { dateTime } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref, unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import { CASE_FIELDS, type Case, type CaseField, type CaseFieldKey, type InterviewDetail, type Unit } from "@/lib/types"

const STAGES = ["STAGE-INIT", "STAGE-PILOT", "STAGE-ROLLOUT", "STAGE-ROUTINE", "STAGE-EXTEND"]
const STATUSES = ["STATUS-STABLE", "STATUS-SCALED", "STATUS-STALLED", "STATUS-DISCONT", "STATUS-FAILED", "STATUS-MIXED", "STATUS-UNCLEAR"]
const OTHER = "__other"

function short(unitId: string): string {
  return `U${unitId.split("-U").pop()}`
}

const selectCls = FIELD

/** A unit used as evidence: chip linking to the unit in Coding, removable.
 * Shown as a warning when the unit is not attributed to the case. */
function EvidenceChip({ unitId, attributed, onRemove }: { unitId: string; attributed: boolean; onRemove?: () => void }) {
  return (
    <span
      className={cn("inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs", attributed ? "bg-muted" : "bg-warn-soft text-warn")}
      title={attributed ? undefined : "Not among the attributed units of the case: saving will be refused"}
    >
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

/** Picks evidence for a field among the attributed units of the case only. */
function UnitPicker({ attributed, textOf, chosen, onPick }: { attributed: string[]; textOf: (id: string) => string | undefined; chosen: string[]; onPick: (unitId: string) => void }) {
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
            const q = search.toLowerCase()
            return value.toLowerCase().includes(q) || (textOf(value) ?? "").toLowerCase().includes(q) ? 1 : 0
          }}
        >
          <CommandInput placeholder="Search the attributed units" />
          <CommandList className="max-h-80">
            <CommandEmpty>{attributed.length ? "No attributed unit matches." : "No unit attributed yet. Add units under Attributed units first."}</CommandEmpty>
            <CommandGroup>
              {attributed.map((u) => (
                <CommandItem
                  key={u}
                  value={u}
                  disabled={chosen.includes(u)}
                  onSelect={() => {
                    onPick(u)
                    setOpen(false)
                  }}
                  className="items-start"
                >
                  <span className="ident text-muted-foreground w-24 shrink-0 truncate">{u}</span>
                  <span className="line-clamp-2 text-xs">{textOf(u) ?? "Unit of another interview"}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

/** A value from a short list of documented values, with a free text fallback. */
function OptionField({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string | null
  options: readonly string[]
  onChange: (v: string | null) => void
}) {
  const [other, setOther] = useState(() => value !== null && !options.includes(value))
  return (
    <div className="flex flex-col gap-1 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <select
        className={selectCls}
        value={other ? OTHER : (value ?? "")}
        onChange={(e) => {
          const v = e.target.value
          if (v === OTHER) {
            setOther(true)
            onChange(null)
          } else {
            setOther(false)
            onChange(v || null)
          }
        }}
      >
        <option value="">Not set</option>
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
        <option value={OTHER}>Other, free text</option>
      </select>
      {other && <Input value={value ?? ""} onChange={(e) => onChange(e.target.value || null)} placeholder="Free text" aria-label={`${label}, free text`} />}
    </div>
  )
}

function CaseEditor({ initial, units, onSaved }: { initial: Case; units: Unit[]; onSaved: (c: Case) => void }) {
  const qc = useQueryClient()
  const [c, setC] = useState<Case>(initial)
  // Text as typed, so that separators are not rewritten while the user types.
  const [unitsText, setUnitsText] = useState(initial.units.join("\n"))
  const [carriersText, setCarriersText] = useState(initial.carriers_rejected.join(", "))
  const dirty = JSON.stringify(c) !== JSON.stringify(initial)
  const setField = (k: CaseFieldKey, patch: Partial<CaseField>) => setC((x) => ({ ...x, [k]: { ...x[k], ...patch } }))
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
  const textOf = (id: string) => units.find((u) => u.unit_id === id)?.text

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-3 md:grid-cols-[1fr_12rem_12rem]">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Case label</span>
          <Input value={c.label} onChange={(e) => setC({ ...c, label: e.target.value })} placeholder="Item-level RFID in a department store" />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Stage</span>
          <select className={selectCls} value={c.stage ?? ""} onChange={(e) => setC({ ...c, stage: e.target.value || null })}>
            <option value="">Not set</option>
            {STAGES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Outcome status</span>
          <select className={selectCls} value={c.outcome_status ?? ""} onChange={(e) => setC({ ...c, outcome_status: e.target.value || null })}>
            <option value="">Not set</option>
            {STATUSES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
      </div>

      <div className="grid gap-3 md:grid-cols-3">
        <OptionField label="Decision" value={c.decision} options={DECISIONS} onChange={(v) => setC((x) => ({ ...x, decision: v }))} />
        <OptionField label="Granularity" value={c.granularity} options={GRANULARITIES} onChange={(v) => setC((x) => ({ ...x, granularity: v }))} />
        <OptionField label="Account type" value={c.account_type} options={ACCOUNT_TYPES} onChange={(v) => setC((x) => ({ ...x, account_type: v }))} />
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Carrier chosen</span>
          <Input value={c.carrier_chosen ?? ""} onChange={(e) => setC((x) => ({ ...x, carrier_chosen: e.target.value || null }))} />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Carriers rejected, comma-separated</span>
          <Input
            value={carriersText}
            onChange={(e) => {
              setCarriersText(e.target.value)
              setC((x) => ({ ...x, carriers_rejected: parseList(e.target.value) }))
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Swiss link</span>
          <Input value={c.swiss_link ?? ""} onChange={(e) => setC((x) => ({ ...x, swiss_link: e.target.value || null }))} />
        </label>
      </div>

      <Section title="Attributed units" aside="any interview; the evidence of each field must be among them">
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_16rem]">
          <div className="flex flex-col gap-1.5">
            <Textarea
              rows={4}
              className="ident text-xs"
              aria-label="Attributed unit ids"
              placeholder={"ECO-07-U012\nRET-22-U003"}
              value={unitsText}
              onChange={(e) => {
                setUnitsText(e.target.value)
                setC((x) => ({ ...x, units: parseUnitIds(e.target.value) }))
              }}
            />
            <span className="text-muted-foreground text-xs">One unit id per line, or separated by commas. Unit ids are checked on save.</span>
          </div>
          <div className="flex flex-col gap-2 text-xs">
            <span className="tabular-nums">{c.units.length === 1 ? "1 unit attributed" : `${c.units.length} units attributed`}</span>
            <div>
              <span className="text-muted-foreground">Interviews, read-only, derived on save</span>
              <p className="ident mt-0.5">{initial.interviews.length ? initial.interviews.join(", ") : "none saved yet"}</p>
            </div>
          </div>
        </div>
      </Section>

      {CASE_FIELDS.map((k) => {
        const f = c[k]
        const loose = unattributedEvidence(c, k)
        return (
          <section key={k} className="grid gap-2 md:grid-cols-[10rem_1fr]">
            <div>
              <h3 className="text-sm font-semibold">{FIELD_LABELS[k].title}</h3>
              <p className="text-muted-foreground text-xs">{FIELD_LABELS[k].hint}</p>
            </div>
            <div className="flex flex-col gap-2">
              <Textarea rows={2} value={f.text} onChange={(e) => setField(k, { text: e.target.value })} placeholder="In your words, grounded in the evidence below" />
              <div className="flex flex-wrap items-center gap-1.5">
                {f.evidence.map((u) => (
                  <EvidenceChip
                    key={u}
                    unitId={u}
                    attributed={c.units.includes(u)}
                    onRemove={() => setField(k, { evidence: f.evidence.filter((x) => x !== u) })}
                  />
                ))}
                <UnitPicker
                  attributed={c.units}
                  textOf={textOf}
                  chosen={f.evidence}
                  onPick={(u) => setField(k, { evidence: [...f.evidence, u] })}
                />
              </div>
              {loose.length > 0 && (
                <p className="text-warn text-xs">
                  Not among the attributed units: {loose.join(", ")}. Attribute them above or remove them from the evidence, or the save is refused.
                </p>
              )}
              <div className="grid gap-2 sm:grid-cols-2">
                <label className="flex flex-col gap-1 text-xs">
                  <span className="text-muted-foreground">Contradictions</span>
                  <Textarea rows={2} value={f.contradictions} onChange={(e) => setField(k, { contradictions: e.target.value })} placeholder="Where the data disagree" />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  <span className="text-muted-foreground">Missing</span>
                  <Textarea rows={2} value={f.missing} onChange={(e) => setField(k, { missing: e.target.value })} placeholder="What the data do not tell" />
                </label>
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

const emptyField = (): CaseField => ({ text: "", contradictions: "", missing: "", evidence: [] })

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
        units: [],
        decision: null,
        granularity: null,
        carrier_chosen: null,
        carriers_rejected: [],
        account_type: null,
        swiss_link: null,
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
        A case is backed by units: segment a transcript first.
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
                    <span className="text-muted-foreground text-xs">
                      {filled} of 5 fields filled, {c.units.length} {c.units.length === 1 ? "unit" : "units"}
                    </span>
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
