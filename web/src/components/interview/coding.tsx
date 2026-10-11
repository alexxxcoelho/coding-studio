import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useVirtualizer } from "@tanstack/react-virtual"
import { toast } from "sonner"

import { Banner, ChipList, CodeChip, CodeRail, EmptyState } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { CodePicker } from "@/components/code-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { EV_LEVELS, MODIFIERS, PROVENANCES, sameSet } from "@/lib/codes"
import { roleLabel } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Code, HumanCoding, InterviewDetail, Phase, UnitRow } from "@/lib/types"

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
      <summary className="cursor-pointer font-medium">The four phase 1 categories</summary>
      <dl className="mt-2 grid gap-x-6 gap-y-2 md:grid-cols-2">
        {codes.map((c) => (
          <div key={c.code} className="flex gap-2.5">
            <dt className="shrink-0">
              <CodeChip code={c.code} family={c.family} />
            </dt>
            <dd className="text-muted-foreground text-xs leading-snug">
              <span className="text-foreground font-medium">{c.label}.</span> {c.definition}
            </dd>
          </div>
        ))}
      </dl>
      <p className="text-muted-foreground mt-2 text-xs">A unit may touch several contexts. NA when the unit is not relevant.</p>
    </details>
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
}: {
  row: UnitRow
  phase: Phase
  readOnly: boolean
  book: Code[]
  onSave: (draft: HumanCoding) => void
  saving: boolean
  onPrev: () => void
  onNext: () => void
  position: string
}) {
  const [draft, setDraft] = useState<HumanCoding>(() => row.coding ?? emptyDraft(row.unit.unit_id))
  const textRef = useRef<HTMLDivElement>(null)

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
      toast.error("Choose at least one code, or NA when none applies")
      return
    }
    onSave(draft)
  }, [draft, onSave, readOnly])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault()
        save()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [save])

  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-sm">
          <span className="ident font-semibold">{row.unit.unit_id}</span>
          <span className="text-muted-foreground tabular-nums">{row.unit.timestamp}</span>
          <Badge variant={row.unit.role === "respondent" ? "secondary" : "outline"}>{roleLabel(row.unit.role)}</Badge>
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

      {phase === 2 && row.phase1_final && (
        <div className="flex items-center gap-2 text-xs">
          <span className="text-muted-foreground">Reconciled in phase 1</span>
          <ChipList codes={row.phase1_final} book={book} />
        </div>
      )}

      <CodePicker allowed={row.allowed} value={draft.codes} onChange={(c) => set("codes", c)} disabled={readOnly} />
      <div className="flex min-h-6 items-center gap-2 text-xs">
        <span className="text-muted-foreground">Selected</span>
        <ChipList codes={draft.codes} book={book} empty="nothing yet" />
      </div>

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

export function CodingTab({ id, phase, detail }: { id: string; phase: Phase; detail: InterviewDetail }) {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["phase", id, phase], queryFn: () => api.phase(id, phase) })
  const units = useQuery({ queryKey: ["units", id, phase], queryFn: () => api.units(id, phase) })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const p1book = useQuery({ queryKey: ["codebook-phase1"], queryFn: api.phase1Codebook, enabled: phase === 1 })
  const [uncodedOnly, setUncodedOnly] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const all = useMemo(() => units.data ?? [], [units.data])
  const rows = useMemo(() => (uncodedOnly ? all.filter((r) => !r.coding) : all), [all, uncodedOnly])
  const selIndex = Math.max(0, rows.findIndex((r) => r.unit.unit_id === selectedId))
  const selected = rows[selIndex] ?? null
  const codedCount = all.filter((r) => r.coding).length
  const codeBook = book.data?.codes ?? []

  const readOnly = !!state.data && (state.data.locked || state.data.frozen)

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
      const i = Math.min(rows.length - 1, Math.max(0, selIndex + delta))
      setSelectedId(rows[i].unit.unit_id)
      virtualizer.scrollToIndex(i, { align: "auto" })
    },
    [rows, selIndex, virtualizer],
  )

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

  const st = state.data!
  if (phase === 2 && !detail.phase1.reconciled)
    return (
      <Banner
        tone="lock"
        title="Phase 2 opens once phase 1 is reconciled"
        action={
          <Button size="sm" variant="outline" asChild>
            <a href={interviewHref(id, "reconciliation", 1)}>Go to phase 1 reconciliation</a>
          </Button>
        }
      >
        Phase 2 codes only the units found relevant in phase 1, after the AI pass and every divergence is decided.
      </Banner>
    )
  if (!all.length)
    return <EmptyState title="No unit in the scope of this phase">Every unit was reconciled as NA in phase 1.</EmptyState>

  return (
    <div className="flex flex-col gap-4">
      {st.frozen && (
        <Banner tone="lock" title="Phase 1 is frozen">
          Phase 2 has started, so phase 1 decisions can no longer change.
        </Banner>
      )}
      {!st.frozen && st.locked && (
        <Banner tone="lock" title="Your coding is locked">
          An AI answer has been imported for this phase. Changes now go through the Reconciliation tab, with a written rationale.
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
                    )}
                    style={{ transform: `translateY(${v.start}px)` }}
                  >
                    <CodeRail codes={r.coding?.codes} book={codeBook} />
                    <div className="min-w-0 flex-1">
                      <div className="text-muted-foreground mb-0.5 flex items-center gap-2 text-xs">
                        <span className="ident">{r.unit.unit_id.split("-U").pop()}</span>
                        <span className="tabular-nums">{r.unit.timestamp}</span>
                        <span className={cn(r.unit.role === "interviewer" && "text-primary")}>{roleLabel(r.unit.role)}</span>
                      </div>
                      <p className={cn("verbatim text-[0.98rem]", !active && "line-clamp-3", r.unit.role === "interviewer" && "text-muted-foreground")}>
                        {r.unit.text}
                      </p>
                      {r.coding && (
                        <div className="mt-1.5">
                          <ChipList codes={r.coding.codes} book={codeBook} />
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
              key={`${selected.unit.unit_id}:${JSON.stringify(selected.coding)}`}
              row={selected}
              phase={phase}
              readOnly={readOnly}
              book={codeBook}
              onSave={(d) => save.mutate(d)}
              saving={save.isPending}
              onPrev={() => go(-1)}
              onNext={() => go(1)}
              position={`${selIndex + 1} of ${rows.length}`}
            />
          ) : (
            <p className="text-muted-foreground text-sm">Choose a unit to code it.</p>
          )}
        </aside>
      </div>
    </div>
  )
}
