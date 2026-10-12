import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useVirtualizer } from "@tanstack/react-virtual"
import { toast } from "sonner"

import { Banner, ChipList, CodeChip, CodeRail, EmptyState, PendingChip } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { CodePicker } from "@/components/code-picker"
import { QualificationPanel } from "@/components/interview/qualifications"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { addCode, codeForKey, EV_LEVELS, hotkeyFor, MODIFIERS, PROVENANCES, sameSet, toggleCode } from "@/lib/codes"
import { roleLabel } from "@/lib/format"
import { isCoded, knownLabels } from "@/lib/clarify"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Code, HumanCoding, InterviewDetail, Phase, ToClarify, UnitRow } from "@/lib/types"

const NONE = "__none"

function emptyDraft(unitId: string): HumanCoding {
  return { unit_id: unitId, codes: [], subcode: null, modifier: null, ev_level: null, provenance: null, memo: null, excerpt: null }
}

function draftChanged(a: HumanCoding, b: HumanCoding | null): boolean {
  const base = b ?? emptyDraft(a.unit_id)
  const norm = (s: string | null) => (s ?? "").trim()
  return (
    !sameSet(a.codes, base.codes) ||
    norm(a.subcode) !== norm(base.subcode) ||
    norm(a.modifier) !== norm(base.modifier) ||
    norm(a.ev_level) !== norm(base.ev_level) ||
    norm(a.provenance) !== norm(base.provenance) ||
    norm(a.memo) !== norm(base.memo) ||
    norm(a.excerpt) !== norm(base.excerpt)
  )
}

function isTyping(el: EventTarget | null): boolean {
  const t = el as HTMLElement | null
  if (!t) return false
  return t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName) || !!t.closest("[cmdk-root]")
}

function OptionSelect({
  label,
  value,
  options,
  onChange,
  disabled,
}: {
  label: string
  value: string | null
  options: readonly string[]
  onChange: (v: string | null) => void
  disabled?: boolean
}) {
  return (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <Select value={value ?? NONE} onValueChange={(v) => onChange(v === NONE ? null : v)} disabled={disabled}>
        <SelectTrigger size="sm" className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>Not set</SelectItem>
          {options.map((o) => (
            <SelectItem key={o} value={o}>
              {o}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  )
}

function Phase1Legend({ codes, open }: { codes: Code[]; open: boolean }) {
  return (
    <details className="bg-card rounded-md border px-4 py-2.5 text-sm" open={open}>
      <summary className="cursor-pointer font-medium">The phase 1 categories</summary>
      <dl className="mt-2 grid gap-x-6 gap-y-2 md:grid-cols-2">
        {codes.map((c) => (
          <div key={c.code} className="flex gap-2.5">
            <dt className="flex shrink-0 items-start gap-1.5">
              <kbd className="text-muted-foreground rounded border px-1.5 font-mono text-[0.7rem]">{hotkeyFor(c.code, 1)}</kbd>
              <CodeChip code={c.code} family={c.family} />
            </dt>
            <dd className="text-muted-foreground text-xs leading-snug">
              <span className="text-foreground font-medium">{c.label}.</span> {c.definition}
            </dd>
          </div>
        ))}
      </dl>
      <p className="text-muted-foreground mt-2 text-xs">
        NA when the unit is not relevant. Keys T, O, E, C and N code the unit, save it and move to the next one. For a unit that touches
        several categories, hold Shift to add each one without saving (Shift T, Shift E), then press the last letter or Ctrl/Cmd Enter.
      </p>
    </details>
  )
}

/** Phase 1 contexts of a phase 2 row. */
function contextsOf(row: UnitRow): string[] {
  return row.phase1 ?? []
}

function openLabels(row: UnitRow): ToClarify[] {
  return (row.to_clarify ?? []).filter((t) => !t.resolved)
}

function ClarifyDialog({
  open,
  onOpenChange,
  unitId,
  interviewId,
  suggestions,
}: {
  open: boolean
  onOpenChange: (v: boolean) => void
  unitId: string
  interviewId: string
  suggestions: string[]
}) {
  const qc = useQueryClient()
  const [label, setLabel] = useState("")
  const [note, setNote] = useState("")
  const add = useMutation({
    mutationFn: () => api.addToClarify(interviewId, unitId, label.trim(), note.trim()),
    onSuccess: () => {
      toast.success(`Label "${label.trim()}" put on ${unitId}`)
      setLabel("")
      setNote("")
      onOpenChange(false)
      qc.invalidateQueries({ queryKey: ["to-clarify"] })
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, "Label not recorded"),
  })
  const listId = `labels-${unitId}`
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>To clarify</DialogTitle>
          <DialogDescription>
            A provisional label for what this unit says that no code captures yet. Later you turn the label into a new code or map it to an
            existing one, for every unit that carries it.
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            if (label.trim()) add.mutate()
          }}
        >
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Provisional label</span>
            <Input autoFocus list={listId} value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Waiting for the partner to tag first" />
            <datalist id={listId}>
              {suggestions.map((l) => (
                <option key={l} value={l} />
              ))}
            </datalist>
          </label>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Short note</span>
            <Textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} placeholder="What makes it hard to code, what it might be" />
          </label>
          <DialogFooter>
            <Button type="submit" disabled={!label.trim() || add.isPending}>
              Put the label on the unit
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function UnitEditor({
  row,
  phase,
  readOnly,
  book,
  onSave,
  saving,
  onPrev,
  onNext,
  position,
  interviewId,
  suggestions,
}: {
  row: UnitRow
  phase: Phase
  readOnly: boolean
  interviewId: string
  suggestions: string[]
  book: Code[]
  onSave: (draft: HumanCoding) => void
  saving: boolean
  onPrev: () => void
  onNext: () => void
  position: string
}) {
  const [draft, setDraft] = useState<HumanCoding>(() => row.coding ?? emptyDraft(row.unit.unit_id))
  const [clarifying, setClarifying] = useState(false)
  const textRef = useRef<HTMLDivElement>(null)
  const labels = openLabels(row)

  const dirty = draftChanged(draft, row.coding)
  const set = <K extends keyof HumanCoding>(k: K, v: HumanCoding[K]) => setDraft((d) => ({ ...d, [k]: v }))

  const captureSelection = () => {
    if (readOnly) return
    const sel = window.getSelection()
    if (!sel || sel.isCollapsed || !textRef.current) return
    if (!textRef.current.contains(sel.anchorNode) || !textRef.current.contains(sel.focusNode)) return
    const text = sel.toString().trim()
    if (text && row.unit.text.includes(text)) set("excerpt", text)
  }

  const save = useCallback(() => {
    if (readOnly) return
    if (!draft.codes.length) {
      // A label to clarify is enough to move on in phase 2.
      if (phase === 2 && labels.length) return onNext()
      toast.error(phase === 2 ? "Choose a code, NA, or put a label to clarify (?)" : "Choose a category, or NA when none applies")
      return
    }
    onSave(draft)
  }, [draft, onSave, readOnly, phase, labels.length, onNext])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault()
        save()
        return
      }
      // Phase 1: a letter adds its category, saves and moves on; with Shift it
      // only toggles, for a unit that touches several contexts.
      if (readOnly || saving || e.metaKey || e.ctrlKey || e.altKey || e.repeat || isTyping(e.target)) return
      if (phase === 2 && e.key === "?") {
        e.preventDefault()
        setClarifying(true)
        return
      }
      const code = codeForKey(
        e.key,
        phase,
        row.allowed.map((c) => c.code),
      )
      if (!code) return
      e.preventDefault()
      if (e.shiftKey) {
        setDraft((d) => ({ ...d, codes: toggleCode(d.codes, code) }))
      } else {
        const next = { ...draft, codes: addCode(draft.codes, code) }
        setDraft(next)
        onSave(next)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [save, readOnly, saving, phase, row.allowed, draft, onSave])

  const contexts = contextsOf(row)

  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-sm">
          <span className="ident font-semibold">{row.unit.unit_id}</span>
          <span className="text-muted-foreground tabular-nums">{row.unit.timestamp}</span>
          <Badge variant={row.unit.role === "respondent" ? "secondary" : "outline"}>{roleLabel(row.unit.role)}</Badge>
          {row.unit.corrected && (
            <Badge variant="outline" className="text-warn" title="The text or speaker comes from a correction; the segmentation is unchanged">
              corrected
            </Badge>
          )}
        </div>
        <div className="flex items-center gap-1">
          <span className="text-muted-foreground mr-1 text-xs tabular-nums">{position}</span>
          <Button size="icon-sm" variant="ghost" onClick={onPrev} aria-label="Previous unit" title="Previous unit (k)">
            ↑
          </Button>
          <Button size="icon-sm" variant="ghost" onClick={onNext} aria-label="Next unit" title="Next unit (j)">
            ↓
          </Button>
        </div>
      </div>

      <div
        ref={textRef}
        onMouseUp={captureSelection}
        className="verbatim bg-muted/30 max-h-60 overflow-auto rounded-md border px-3.5 py-2.5"
        title={readOnly ? undefined : "Select words to set the excerpt"}
      >
        {row.unit.text}
      </div>

      {phase === 2 && (
        <div className="flex flex-col gap-1.5 text-xs">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-muted-foreground">Pre-classified in phase 1</span>
            <ChipList codes={contexts} book={book} empty="not yet" />
            <a className="text-primary ml-auto hover:underline" href={interviewHref(interviewId, "coding", 1, row.unit.unit_id)}>
              revise
            </a>
          </div>
          {row.in_scope === false && (
            <p className="text-warn">Phase 1 now says NA for this unit: its codes are kept but it leaves the next AI prompts.</p>
          )}
          {row.coding?.revised_after_ai && (
            <p className="text-muted-foreground">Revised after the AI answer was imported; the audit trail records it.</p>
          )}
        </div>
      )}

      <CodePicker
        allowed={row.allowed}
        value={draft.codes}
        onChange={(c) => set("codes", c)}
        disabled={readOnly}
        phase={phase}
        phase1Contexts={phase === 2 ? contexts : undefined}
      />
      <div className="flex min-h-6 flex-wrap items-center gap-2 text-xs">
        <span className="text-muted-foreground">Selected</span>
        <ChipList codes={draft.codes} book={book} empty="nothing yet" />
        {labels.map((t) => (
          <PendingChip key={t.id} label={t.label} note={t.note} />
        ))}
        {phase === 2 && !readOnly && (
          <Button size="xs" variant="outline" className="ml-auto" onClick={() => setClarifying(true)} title="Put a provisional label (key ?)">
            To clarify <kbd className="ml-0.5 font-mono opacity-70">?</kbd>
          </Button>
        )}
      </div>
      {phase === 2 && (
        <ClarifyDialog open={clarifying} onOpenChange={setClarifying} unitId={row.unit.unit_id} interviewId={interviewId} suggestions={suggestions} />
      )}

      <div className="grid grid-cols-2 gap-2.5">
        <label className="col-span-2 flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Sub-code facets, comma separated</span>
          <Input value={draft.subcode ?? ""} onChange={(e) => set("subcode", e.target.value || null)} disabled={readOnly} />
        </label>
        <OptionSelect label="Modifier" value={draft.modifier} options={MODIFIERS} onChange={(v) => set("modifier", v)} disabled={readOnly} />
        <OptionSelect label="Evidence level" value={draft.ev_level} options={EV_LEVELS} onChange={(v) => set("ev_level", v)} disabled={readOnly} />
        <div className="col-span-2">
          <OptionSelect label="Provenance" value={draft.provenance} options={PROVENANCES} onChange={(v) => set("provenance", v)} disabled={readOnly} />
        </div>
        <label className="col-span-2 flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Analytical memo</span>
          <Textarea rows={3} value={draft.memo ?? ""} onChange={(e) => set("memo", e.target.value || null)} disabled={readOnly} />
        </label>
        <label className="col-span-2 flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground flex justify-between">
            <span>Excerpt, word for word from the unit (select text above)</span>
            {draft.excerpt && !readOnly && (
              <button type="button" className="text-primary hover:underline" onClick={() => set("excerpt", null)}>
                Clear
              </button>
            )}
          </span>
          <Textarea
            rows={2}
            className="verbatim text-sm"
            value={draft.excerpt ?? ""}
            onChange={(e) => set("excerpt", e.target.value || null)}
            disabled={readOnly}
            aria-invalid={!!draft.excerpt && !row.unit.text.includes(draft.excerpt)}
          />
          {draft.excerpt && !row.unit.text.includes(draft.excerpt) && (
            <span className="text-destructive">The excerpt is not found word for word in the unit.</span>
          )}
        </label>
      </div>

      {phase === 2 && <QualificationPanel interviewId={interviewId} row={row} textRef={textRef} />}

      {!readOnly && (
        <div className="flex items-center justify-between gap-2 border-t pt-3">
          <span className={cn("text-xs", dirty ? "text-warn" : "text-muted-foreground")}>
            {dirty ? "Unsaved changes" : row.coding ? "Saved" : "Not coded yet"}
          </span>
          <Button onClick={save} disabled={saving || !dirty}>
            {saving ? "Saving..." : "Save and next"}
            <kbd className="ml-1 text-[0.7rem] opacity-70">Ctrl/Cmd Enter</kbd>
          </Button>
        </div>
      )}
    </div>
  )
}

export function CodingTab({
  id,
  phase,
  detail,
  initialUnit = null,
}: {
  id: string
  phase: Phase
  detail: InterviewDetail
  initialUnit?: string | null
}) {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["phase", id, phase], queryFn: () => api.phase(id, phase) })
  const units = useQuery({ queryKey: ["units", id, phase], queryFn: () => api.units(id, phase) })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const p1book = useQuery({ queryKey: ["codebook-phase1"], queryFn: api.phase1Codebook, enabled: phase === 1 })
  const labelsQ = useQuery({ queryKey: ["to-clarify", "open"], queryFn: () => api.toClarify({ open: true }), enabled: phase === 2 })
  const [uncodedOnly, setUncodedOnly] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(initialUnit)

  const coded = useCallback((r: UnitRow) => (phase === 2 ? isCoded(r.coding, r.to_clarify) : !!r.coding), [phase])
  const all = useMemo(() => units.data ?? [], [units.data])
  const rows = useMemo(() => (uncodedOnly ? all.filter((r) => !coded(r) || r.unit.unit_id === selectedId) : all), [all, uncodedOnly, coded, selectedId])
  const selIndex = Math.max(0, rows.findIndex((r) => r.unit.unit_id === selectedId))
  const selected = rows[selIndex] ?? null
  const codedCount = all.filter(coded).length
  const codeBook = book.data?.codes ?? []
  const suggestions = useMemo(() => knownLabels(labelsQ.data ?? []), [labelsQ.data])

  // v3: coding is never locked; changes after an AI answer are flagged, not refused.
  const readOnly = false

  const scrollRef = useRef<HTMLDivElement>(null)
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 92,
    overscan: 8,
  })

  const go = useCallback(
    (delta: number) => {
      if (!rows.length) return
      // With "uncoded only", the unit just coded leaves the list: the next
      // uncoded one takes its place.
      const pool = uncodedOnly ? rows.filter((r, i) => i === selIndex || !coded(r)) : rows
      const at = pool.findIndex((r) => r.unit.unit_id === rows[selIndex]?.unit.unit_id)
      const target = pool[Math.min(pool.length - 1, Math.max(0, at + delta))]
      if (!target) return
      setSelectedId(target.unit.unit_id)
      const i = rows.indexOf(target)
      if (i >= 0) virtualizer.scrollToIndex(i, { align: "auto" })
    },
    [rows, selIndex, virtualizer, uncodedOnly, coded],
  )

  // Jump to a unit given in the link (?unit=), once the rows are loaded.
  const jumped = useRef(false)
  useEffect(() => {
    if (jumped.current || !initialUnit || !all.length) return
    jumped.current = true
    const i = rows.findIndex((r) => r.unit.unit_id === initialUnit)
    if (i >= 0) virtualizer.scrollToIndex(i, { align: "center" })
  }, [initialUnit, all.length, rows, virtualizer])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e.target) || e.metaKey || e.ctrlKey || e.altKey) return
      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault()
        go(1)
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault()
        go(-1)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [go])

  const save = useMutation({
    mutationFn: (draft: HumanCoding) => api.setCoding(id, phase, draft),
    onSuccess: (saved) => {
      qc.setQueryData<UnitRow[]>(["units", id, phase], (old) =>
        old?.map((r) => (r.unit.unit_id === saved.unit_id ? { ...r, coding: saved } : r)),
      )
      qc.invalidateQueries({ queryKey: ["phase", id, phase] })
      qc.invalidateQueries({ queryKey: ["interview", id] })
      qc.invalidateQueries({ queryKey: ["interviews"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
      go(1)
    },
    onError: (e) => {
      reportError(e, "Not saved")
      invalidateInterview(qc, id)
    },
  })

  if (!detail.segmentation)
    return (
      <EmptyState title="No units yet" action={<Button asChild><a href={interviewHref(id, "sources")}>Go to Sources</a></Button>}>
        Segment a transcript first; its units appear here for coding.
      </EmptyState>
    )
  if (units.isLoading || state.isLoading) return <Skeleton className="h-96 w-full" />
  if (units.error) return <Banner tone="danger" title="Units could not be loaded">{units.error.message}</Banner>

  if (!all.length)
    return phase === 2 ? (
      <EmptyState
        title="No unit pre-classified as relevant yet"
        action={
          <Button asChild>
            <a href={interviewHref(id, "coding", 1)}>Go to phase 1</a>
          </Button>
        }
      >
        Phase 2 codes the units that phase 1 placed in a TOE context or in CONTEXT. Any category other than NA brings a unit here.
      </EmptyState>
    ) : (
      <EmptyState title="No unit to code">The segmentation holds no unit.</EmptyState>
    )

  return (
    <div className="flex flex-col gap-4">
      {phase === 1 && (
        <Banner tone="info" title="Provisional pre-classification: revisable at any time">
          Phase 1 sorts units by TOE context to organise the detailed coding. It has no AI pass and nothing locks it.
        </Banner>
      )}
      {phase === 1 && p1book.data && <Phase1Legend codes={p1book.data.codes} open={codedCount === 0} />}

      <div className="flex flex-wrap items-center gap-4">
        <div className="flex min-w-[16rem] flex-1 items-center gap-3">
          <Progress value={(codedCount / all.length) * 100} className="h-1.5 flex-1" />
          <span className="text-sm tabular-nums">
            {codedCount} of {all.length} coded
          </span>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={uncodedOnly} onCheckedChange={(v) => setUncodedOnly(v === true)} />
          Uncoded only
        </label>
        <span className="text-muted-foreground text-xs">j and k move between units</span>
      </div>

      <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1fr)_26rem]">
        <div ref={scrollRef} className="bg-card h-[calc(100svh-17rem)] min-h-[24rem] overflow-auto rounded-md border" role="listbox" aria-label="Units">
          {rows.length === 0 ? (
            <p className="text-muted-foreground p-6 text-sm">Every unit of this phase is coded.</p>
          ) : (
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {virtualizer.getVirtualItems().map((v) => {
                const r = rows[v.index]
                const active = r.unit.unit_id === selected?.unit.unit_id
                const pending = openLabels(r)
                const outOfScope = phase === 2 && r.in_scope === false
                return (
                  <div
                    key={r.unit.unit_id}
                    data-index={v.index}
                    ref={virtualizer.measureElement}
                    role="option"
                    aria-selected={active}
                    onClick={() => setSelectedId(r.unit.unit_id)}
                    className={cn(
                      "absolute inset-x-0 flex cursor-pointer gap-3 border-b px-3 py-2.5",
                      active ? "bg-accent/60" : "hover:bg-muted/50",
                      outOfScope && "opacity-50",
                    )}
                    style={{ transform: `translateY(${v.start}px)` }}
                  >
                    <CodeRail codes={r.coding?.codes} book={codeBook} />
                    <div className="min-w-0 flex-1">
                      <div className="text-muted-foreground mb-0.5 flex items-center gap-2 text-xs">
                        <span className="ident">{r.unit.unit_id.split("-U").pop()}</span>
                        <span className="tabular-nums">{r.unit.timestamp}</span>
                        <span className={cn(r.unit.role === "interviewer" && "text-primary")}>{roleLabel(r.unit.role)}</span>
                        {phase === 2 && contextsOf(r).length > 0 && (
                          <span className="ident" title="Phase 1 pre-classification">
                            {contextsOf(r).join(" ")}
                          </span>
                        )}
                        {r.coding?.revised_after_ai && <span title="Revised after the AI answer">revised</span>}
                        {r.unit.corrected && <span className="text-warn">corrected</span>}
                        {phase === 2 && r.flags && r.flags.length > 0 && <span title="Flags of the unit">{r.flags.join(", ")}</span>}
                        {outOfScope && <span className="text-warn">NA in phase 1 now</span>}
                      </div>
                      <p className={cn("verbatim text-[0.98rem]", !active && "line-clamp-3", r.unit.role === "interviewer" && "text-muted-foreground")}>
                        {r.unit.text}
                      </p>
                      {(r.coding?.codes.length || pending.length > 0) && (
                        <div className="mt-1.5 flex flex-wrap gap-1">
                          {r.coding && r.coding.codes.length > 0 && <ChipList codes={r.coding.codes} book={codeBook} />}
                          {pending.map((t) => (
                            <PendingChip key={t.id} label={t.label} note={t.note} />
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </div>

        <aside className="bg-card sticky top-4 max-h-[calc(100svh-2rem)] overflow-y-auto rounded-md border p-4">
          {selected ? (
            <UnitEditor
              key={`${selected.unit.unit_id}:${JSON.stringify(selected.coding)}:${openLabels(selected).length}`}
              row={selected}
              phase={phase}
              readOnly={readOnly}
              book={codeBook}
              onSave={(d) => save.mutate(d)}
              saving={save.isPending}
              onPrev={() => go(-1)}
              onNext={() => go(1)}
              position={`${selIndex + 1} of ${rows.length}`}
              interviewId={id}
              suggestions={suggestions}
            />
          ) : (
            <p className="text-muted-foreground text-sm">Choose a unit to code it.</p>
          )}
        </aside>
      </div>
    </div>
  )
}
