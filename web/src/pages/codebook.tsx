// Codebook: the phase 1 categories, the concrete codes of phase 2 (edited
// here, versioned and audited), the research questions they answer, and the
// export to the thesis coding.db format.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { ArrowDown, ArrowUp, Trash2 } from "lucide-react"

import { CodeChip, EmptyState, PageTitle, Section } from "@/components/bits"
import { CodePicker } from "@/components/code-picker"
import { QuestionPicker, ToePicker } from "@/components/code-fields"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { groupByToe, phase2Codes } from "@/lib/codes"
import { bytes, plural, roleLabel, shortHash } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Code, CodeEdit, ResearchQuestion, Toe, WrittenFiles } from "@/lib/types"

function useBookUpdate() {
  const qc = useQueryClient()
  return (book: { codes: Code[]; source: string }) => {
    qc.setQueryData(["codebook"], book)
    qc.invalidateQueries({ queryKey: ["chain"] })
    qc.invalidateQueries({ queryKey: ["cross-case"] })
  }
}

// ------------------------------------------------------------- code form

interface FormState {
  code: string
  label: string
  definition: string
  toe: Toe | null
  research_questions: string[]
  example: string
  reason: string
  origin: "literature" | "field"
}

function formOf(c?: Code): FormState {
  return {
    code: c?.code ?? "",
    label: c?.label ?? "",
    definition: c?.definition ?? "",
    toe: c?.toe ?? null,
    research_questions: c?.research_questions ?? [],
    example: c?.example ?? "",
    reason: c?.reason ?? "",
    origin: c?.origin === "literature" ? "literature" : "field",
  }
}

function CodeForm({
  initial,
  questions,
  onSubmit,
  pending,
  submitLabel,
}: {
  initial?: Code
  questions: ResearchQuestion[]
  onSubmit: (f: FormState) => void
  pending: boolean
  submitLabel: string
}) {
  const [f, setF] = useState<FormState>(formOf(initial))
  const isNew = !initial
  const valid = f.code.trim() && f.label.trim() && f.definition.trim()
  return (
    <form
      className="grid gap-3 md:grid-cols-2"
      onSubmit={(e) => {
        e.preventDefault()
        if (valid) onSubmit(f)
      }}
    >
      {isNew && (
        <>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Code key</span>
            <Input value={f.code} onChange={(e) => setF({ ...f, code: e.target.value })} className="ident uppercase" placeholder="LEGAL-OBLIGATION" />
          </label>
          <div className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Origin</span>
            <div className="flex gap-1">
              {(["literature", "field"] as const).map((o) => (
                <Button key={o} type="button" size="xs" variant={f.origin === o ? "default" : "outline"} onClick={() => setF({ ...f, origin: o })}>
                  {o}
                </Button>
              ))}
            </div>
          </div>
        </>
      )}
      <label className="flex flex-col gap-1 text-xs md:col-span-2">
        <span className="text-muted-foreground">Label, concrete and plain</span>
        <Input value={f.label} onChange={(e) => setF({ ...f, label: e.target.value })} placeholder="Legal obligation" />
      </label>
      <label className="flex flex-col gap-1 text-xs md:col-span-2">
        <span className="text-muted-foreground">Definition, required: what a unit must say to carry the code</span>
        <Textarea rows={3} value={f.definition} onChange={(e) => setF({ ...f, definition: e.target.value })} />
      </label>
      <div className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">TOE attachment</span>
        <ToePicker value={f.toe} onChange={(toe) => setF({ ...f, toe })} />
      </div>
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">Example, a unit ID or a short quote</span>
        <Input value={f.example} onChange={(e) => setF({ ...f, example: e.target.value })} />
      </label>
      <div className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">Research questions it bears on</span>
        <QuestionPicker questions={questions} value={f.research_questions} onChange={(research_questions) => setF({ ...f, research_questions })} />
      </div>
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">Reason, why the code exists or changed</span>
        <Textarea rows={2} value={f.reason} onChange={(e) => setF({ ...f, reason: e.target.value })} />
      </label>
      <div className="flex justify-end md:col-span-2">
        <Button type="submit" size="sm" disabled={!valid || pending}>
          {pending ? "Saving..." : submitLabel}
        </Button>
      </div>
    </form>
  )
}

function NewCode({ questions }: { questions: ResearchQuestion[] }) {
  const update = useBookUpdate()
  const [open, setOpen] = useState(false)
  const add = useMutation({
    mutationFn: (f: FormState) =>
      api.addCode({
        code: f.code.trim().toUpperCase(),
        family: f.toe === "T" ? "TECH" : f.toe === "O" ? "ORG" : f.toe === "E" ? "ENV" : "OTHER",
        label: f.label.trim(),
        definition: f.definition.trim(),
        origin: f.origin,
        active: true,
        in_prompt: true,
        toe: f.toe,
        research_questions: f.research_questions,
        merged_into: null,
        version: 1,
        example: f.example.trim() || null,
        reason: f.reason.trim() || null,
        created_at: null,
      }),
    onSuccess: (book) => {
      update(book)
      toast.success("Code added")
      setOpen(false)
    },
    onError: (e) => reportError(e, "Code not added"),
  })
  return (
    <>
      <Button size="sm" onClick={() => setOpen(true)}>
        New code
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>New concrete code</DialogTitle>
            <DialogDescription>
              Name a mechanism in plain words (legal obligation, customer requirement, lack of skills) and attach it to a TOE context when it fits one.
            </DialogDescription>
          </DialogHeader>
          <CodeForm questions={questions} onSubmit={(f) => add.mutate(f)} pending={add.isPending} submitLabel="Add the code" />
        </DialogContent>
      </Dialog>
    </>
  )
}

// ------------------------------------------------------ units with a code

function UnitsDrawer({ code, onClose }: { code: Code | null; onClose: () => void }) {
  const units = useQuery({ queryKey: ["code-units", code?.code], queryFn: () => api.codeUnits(code!.code), enabled: !!code })
  return (
    <Sheet open={!!code} onOpenChange={(o) => !o && onClose()}>
      <SheetContent className="w-[min(46rem,100vw)] sm:max-w-none">
        <SheetHeader>
          <SheetTitle className="flex items-center gap-2">
            Units with {code && <CodeChip code={code.code} family={code.family} />}
          </SheetTitle>
          <SheetDescription>{code?.label}. Every unit whose phase 2 coding carries the code, across interviews, to revisit the passages.</SheetDescription>
        </SheetHeader>
        <div className="flex flex-col gap-2 overflow-y-auto px-4 pb-6">
          {units.isLoading ? (
            <Skeleton className="h-40 w-full" />
          ) : (units.data ?? []).length === 0 ? (
            <p className="text-muted-foreground text-sm">No unit carries this code yet.</p>
          ) : (
            units.data!.map((u) => (
              <a key={u.unit.unit_id} href={unitHref(u.unit.unit_id)} onClick={onClose} className="hover:bg-muted/50 rounded-md border px-3 py-2">
                <div className="text-muted-foreground flex items-center gap-2 text-xs">
                  <span className="ident">{u.unit.unit_id}</span>
                  <span className="tabular-nums">{u.unit.timestamp}</span>
                  <span>{roleLabel(u.unit.role)}</span>
                  {u.coding && u.coding.codes.length > 1 && <span>with {u.coding.codes.filter((c) => c !== code?.code).join(", ")}</span>}
                </div>
                <p className="verbatim line-clamp-4 text-sm">{u.unit.text}</p>
              </a>
            ))
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}

function MergeDialog({ code, codes, onClose }: { code: Code | null; codes: Code[]; onClose: () => void }) {
  const update = useBookUpdate()
  const qc = useQueryClient()
  const [into, setInto] = useState<string[]>([])
  const [rationale, setRationale] = useState("")
  const units = useQuery({ queryKey: ["code-units", code?.code], queryFn: () => api.codeUnits(code!.code), enabled: !!code })
  const merge = useMutation({
    mutationFn: () => api.mergeCode(code!.code, into[0], rationale.trim()),
    onSuccess: (res) => {
      update(res.codebook)
      qc.invalidateQueries({ predicate: (q) => ["units", "review", "final", "code-units"].includes(String(q.queryKey[0])) })
      toast.success(`${code!.code} merged into ${into[0]}: ${plural(res.rewritten_codings, "coding")} and ${plural(res.rewritten_decisions, "decision")} rewritten`)
      setInto([])
      setRationale("")
      onClose()
    },
    onError: (e) => reportError(e, "Merge refused"),
  })
  return (
    <Dialog open={!!code} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Merge {code?.code} into another code</DialogTitle>
          <DialogDescription>
            Your phase 2 codings and open decisions carrying {code?.code} are rewritten to the target code, each change in the audit trail. AI answers
            stay as received. {units.data ? `${plural(units.data.length, "unit")} carry the code now.` : ""}
          </DialogDescription>
        </DialogHeader>
        <CodePicker
          allowed={codes.filter((c) => c.code !== code?.code)}
          value={into}
          onChange={(v) => setInto(v.filter((c) => c !== "NA").slice(-1))}
          phase={2}
          phase1Contexts={[]}
        />
        <Textarea rows={2} placeholder="Rationale, required" value={rationale} onChange={(e) => setRationale(e.target.value)} />
        <DialogFooter>
          <Button disabled={into.length !== 1 || !rationale.trim() || merge.isPending} onClick={() => merge.mutate()}>
            Merge into {into[0] ?? "..."}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ------------------------------------------------------------- code rows

function CodeRow({
  c,
  questions,
  onUnits,
  onMerge,
}: {
  c: Code
  questions: ResearchQuestion[]
  onUnits: (c: Code) => void
  onMerge: (c: Code) => void
}) {
  const update = useBookUpdate()
  const [editing, setEditing] = useState(false)
  const edit = useMutation({
    mutationFn: (f: FormState) => {
      const body: CodeEdit = {
        label: f.label.trim(),
        definition: f.definition.trim(),
        toe: f.toe,
        research_questions: f.research_questions,
        example: f.example.trim() || null,
        reason: f.reason.trim() || null,
      }
      return api.editCode(c.code, body)
    },
    onSuccess: (b) => {
      update(b)
      toast.success(`${c.code} saved`)
      setEditing(false)
    },
    onError: (e) => reportError(e, "Not saved"),
  })
  const active = useMutation({
    mutationFn: () => api.setActive(c.code, !c.active),
    onSuccess: (b) => update(b),
    onError: (e) => reportError(e),
  })
  const prompt = useMutation({
    mutationFn: (on: boolean) => api.setInPrompt(c.code, on),
    onSuccess: (b) => update(b),
    onError: (e) => reportError(e),
  })
  return (
    <li className={cn("border-b px-4 py-3 last:border-b-0", !c.active && "opacity-55")}>
      <div className="flex flex-wrap items-start gap-x-4 gap-y-1.5">
        <div className="flex min-w-[14rem] flex-1 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <CodeChip code={c.code} family={c.family} />
            <span className="text-sm font-medium">{c.label}</span>
            <Badge variant="outline" className={cn("text-[0.68rem]", c.origin === "field" && "text-inductive border-current/40")}>
              {c.origin}
            </Badge>
            <span className="text-muted-foreground text-[0.7rem]">v{c.version ?? 1}</span>
            {c.merged_into && <span className="text-muted-foreground text-xs">merged into {c.merged_into}</span>}
            {!c.active && !c.merged_into && <span className="text-muted-foreground text-xs">inactive</span>}
          </div>
          <p className="text-muted-foreground max-w-prose text-xs">{c.definition}</p>
          {(c.research_questions?.length ?? 0) > 0 && (
            <div className="flex flex-wrap gap-1">
              {c.research_questions!.map((q) => (
                <span key={q} className="ident bg-muted rounded px-1.5 text-[0.7rem]" title={questions.find((x) => x.id === q)?.text}>
                  {q}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <label className="text-muted-foreground mr-1 flex items-center gap-1.5 text-xs" title="Offered to the AI coder in phase 2 prompts">
            <Checkbox checked={c.in_prompt} disabled={!c.active || prompt.isPending} onCheckedChange={(v) => prompt.mutate(v === true)} />
            AI
          </label>
          <Button size="xs" variant="ghost" onClick={() => onUnits(c)}>
            Units
          </Button>
          {!c.merged_into && (
            <>
              <Button size="xs" variant="ghost" onClick={() => setEditing((e) => !e)}>
                {editing ? "Close" : "Edit"}
              </Button>
              <Button size="xs" variant="ghost" onClick={() => onMerge(c)} disabled={!c.active}>
                Merge
              </Button>
              <Button size="xs" variant="ghost" onClick={() => active.mutate()} disabled={active.isPending}>
                {c.active ? "Deactivate" : "Activate"}
              </Button>
            </>
          )}
        </div>
      </div>
      {editing && (
        <div className="bg-muted/30 mt-3 rounded-md border p-3">
          <CodeForm initial={c} questions={questions} onSubmit={(f) => edit.mutate(f)} pending={edit.isPending} submitLabel={`Save as version ${(c.version ?? 1) + 1}`} />
        </div>
      )}
    </li>
  )
}

// --------------------------------------------------- research questions

function QuestionsEditor({ questions }: { questions: ResearchQuestion[] }) {
  const qc = useQueryClient()
  const [id, setId] = useState("")
  const [text, setText] = useState("")
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const done = (list: ResearchQuestion[]) => {
    qc.setQueryData(["questions"], list)
    qc.invalidateQueries({ queryKey: ["cross-case"] })
    qc.invalidateQueries({ queryKey: ["chain"] })
  }
  const add = useMutation({
    mutationFn: () => api.addQuestion(id.trim().toUpperCase(), text.trim()),
    onSuccess: (l) => {
      done(l)
      setId("")
      setText("")
    },
    onError: (e) => reportError(e, "Question not added"),
  })
  const edit = useMutation({
    mutationFn: ({ q, text, ord }: { q: ResearchQuestion; text: string; ord?: number }) => api.editQuestion(q.id, text, ord),
    onSuccess: (l) => done(l),
    onError: (e) => reportError(e, "Question not saved"),
  })
  const del = useMutation({
    mutationFn: (q: string) => api.deleteQuestion(q),
    onSuccess: (l) => done(l),
    onError: (e) => reportError(e, "Question not deleted"),
  })
  const sorted = [...questions].sort((a, b) => a.ord - b.ord)
  const swap = (i: number, j: number) => {
    const a = sorted[i]
    const b = sorted[j]
    edit.mutate({ q: a, text: a.text, ord: b.ord })
    edit.mutate({ q: b, text: b.text, ord: a.ord })
  }
  return (
    <div className="flex flex-col gap-2">
      {sorted.map((q, i) => {
        const draft = drafts[q.id] ?? q.text
        return (
          <div key={q.id} className="flex items-start gap-2">
            <span className="ident w-12 pt-2 font-semibold">{q.id}</span>
            <Textarea rows={1} className="min-h-9 flex-1" value={draft} onChange={(e) => setDrafts({ ...drafts, [q.id]: e.target.value })} />
            <div className="flex gap-0.5 pt-1">
              {draft !== q.text && (
                <Button size="xs" onClick={() => edit.mutate({ q, text: draft.trim() })} disabled={!draft.trim()}>
                  Save
                </Button>
              )}
              <Button size="icon-xs" variant="ghost" aria-label="Move up" disabled={i === 0} onClick={() => swap(i, i - 1)}>
                <ArrowUp />
              </Button>
              <Button size="icon-xs" variant="ghost" aria-label="Move down" disabled={i === sorted.length - 1} onClick={() => swap(i, i + 1)}>
                <ArrowDown />
              </Button>
              <Button size="icon-xs" variant="ghost" aria-label={`Delete ${q.id}`} onClick={() => del.mutate(q.id)}>
                <Trash2 />
              </Button>
            </div>
          </div>
        )
      })}
      <form
        className="flex items-start gap-2 border-t pt-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (id.trim() && text.trim()) add.mutate()
        }}
      >
        <Input value={id} onChange={(e) => setId(e.target.value)} placeholder="RQ1" className="ident w-20 uppercase" />
        <Textarea rows={1} className="min-h-9 flex-1" value={text} onChange={(e) => setText(e.target.value)} placeholder="The question, as in the thesis" />
        <Button type="submit" size="sm" variant="outline" disabled={!id.trim() || !text.trim() || add.isPending}>
          Add
        </Button>
      </form>
    </div>
  )
}

// ------------------------------------------------------------------ page

export function CodebookPage() {
  const qc = useQueryClient()
  const update = useBookUpdate()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const p1 = useQuery({ queryKey: ["codebook-phase1"], queryFn: api.phase1Codebook })
  const questions = useQuery({ queryKey: ["questions"], queryFn: api.questions })
  const [typed, setPath] = useState<string | null>(null)
  const [unitsOf, setUnitsOf] = useState<Code | null>(null)
  const [mergeOf, setMergeOf] = useState<Code | null>(null)
  const [written, setWritten] = useState<WrittenFiles | null>(null)
  const path = typed ?? state.data?.coding_db ?? ""

  const imp = useMutation({
    mutationFn: () => api.importCodebook(path.trim() || undefined),
    onSuccess: (b) => {
      update(b)
      qc.invalidateQueries({ queryKey: ["state"] })
      toast.success(`${b.codes.length} codes imported`)
    },
    onError: (e) => reportError(e, "Import failed"),
  })
  const exp = useMutation({
    mutationFn: api.exportCodebook,
    onSuccess: (w) => {
      setWritten(w)
      qc.invalidateQueries({ queryKey: ["chain"] })
      toast.success(`${plural(w.files.length, "file")} written`)
    },
    onError: (e) => reportError(e, "Export failed"),
  })
  const browse = async () => {
    try {
      const r = await api.pick("file", "Choose the thesis coding.db")
      if (r.path) setPath(r.path)
    } catch (e) {
      reportError(e)
    }
  }

  const codes = book.data?.codes ?? []
  const concrete = codes.filter((c) => c.origin !== "legacy" && c.origin !== "phase1")
  const legacy = codes.filter((c) => c.origin === "legacy")
  const live = phase2Codes(codes)
  const rq = questions.data ?? []
  const offered = live.filter((c) => c.in_prompt).length
  const field = concrete.filter((c) => c.origin === "field").length

  return (
    <>
      <PageTitle
        title="Codebook"
        sub={book.data ? `${live.length} concrete codes in use (${field} from the field), ${offered} offered to the AI coder` : " "}
        actions={
          <>
            <Button variant="outline" size="sm" onClick={() => exp.mutate()} disabled={exp.isPending}>
              {exp.isPending ? "Exporting..." : "Export for coding.db"}
            </Button>
            <NewCode questions={rq} />
          </>
        }
      />

      <div className="flex flex-col gap-5">
        {written && (
          <Section title="Export for coding.db" aside="CSV and SQL in the coding.db schema; coding.db itself is never written">
            <ul className="flex flex-col gap-1 text-xs">
              {written.files.map((f) => (
                <li key={f.path} className="flex flex-wrap gap-x-3">
                  <span className="ident">{f.path}</span>
                  <span className="text-muted-foreground">{bytes(f.size)}</span>
                  <span className="text-muted-foreground ident">SHA-256 {shortHash(f.sha256, 12)}</span>
                </li>
              ))}
            </ul>
          </Section>
        )}

        <Section title="Research questions" aside="codes map to them; the cross-case matrix is organised by them">
          {questions.isLoading ? <Skeleton className="h-16 w-full" /> : <QuestionsEditor questions={rq} />}
        </Section>

        <section className="bg-card rounded-md border">
          <header className="flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2.5">
            <h2 className="text-sm font-semibold">Concrete codes, phase 2</h2>
            <span className="text-muted-foreground text-xs">grouped by TOE attachment; edits are versioned and audited</span>
          </header>
          {book.isLoading ? (
            <Skeleton className="m-4 h-64" />
          ) : concrete.length === 0 ? (
            <div className="p-4">
              <EmptyState title="No concrete code yet">Import the codebook from the thesis coding.db below, or create codes with New code.</EmptyState>
            </div>
          ) : (
            groupByToe(concrete).map((g) => (
              <div key={g.key}>
                <h3 className="bg-muted/40 border-b px-4 py-1.5 text-xs font-semibold">{g.title}</h3>
                <ul>
                  {g.codes.map((c) => (
                    <CodeRow key={c.code} c={c} questions={rq} onUnits={setUnitsOf} onMerge={setMergeOf} />
                  ))}
                </ul>
              </div>
            ))
          )}
        </section>

        <Section title="Phase 1 categories" aside="provisional pre-classification, revisable at any time">
          {p1.data ? (
            <dl className="grid gap-3 md:grid-cols-2">
              {p1.data.codes.map((c) => (
                <div key={c.code} className="flex gap-3">
                  <dt className="shrink-0">
                    <CodeChip code={c.code} family={c.family} />
                  </dt>
                  <dd className="text-sm">
                    <span className="font-medium">{c.label}.</span> <span className="text-muted-foreground">{c.definition}</span>
                  </dd>
                </div>
              ))}
            </dl>
          ) : (
            <Skeleton className="h-24 w-full" />
          )}
          <p className="text-muted-foreground mt-3 text-xs">{p1.data?.source}</p>
        </Section>

        <form
          className="bg-card flex flex-wrap items-end gap-2 rounded-md border p-4"
          onSubmit={(e) => {
            e.preventDefault()
            imp.mutate()
          }}
        >
          <label className="flex min-w-[20rem] flex-1 flex-col gap-1.5">
            <span className="text-muted-foreground text-xs">Thesis coding.db, opened read only</span>
            <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/path/to/05_Analysis/coding/coding.db" />
          </label>
          {state.data?.native_dialogs && (
            <Button type="button" variant="outline" onClick={browse}>
              Browse...
            </Button>
          )}
          <Button type="submit" variant="outline" disabled={imp.isPending}>
            {imp.isPending ? "Importing..." : "Import from coding.db"}
          </Button>
          <p className="text-muted-foreground basis-full text-xs">
            Importing again keeps your edits, your field codes and your choices of what is offered to the AI. A code removed upstream is deactivated,
            never deleted.
          </p>
        </form>

        {legacy.length > 0 && (
          <Collapsible>
            <CollapsibleTrigger className="text-primary text-xs hover:underline">{plural(legacy.length, "legacy family code")}, read only</CollapsibleTrigger>
            <CollapsibleContent className="mt-2 flex flex-wrap gap-2">
              {legacy.map((c) => (
                <span key={c.code} title={c.definition}>
                  <CodeChip code={c.code} family={c.family} />
                </span>
              ))}
            </CollapsibleContent>
          </Collapsible>
        )}
        {book.data && <p className="text-muted-foreground text-xs">Imported from {book.data.source || "nowhere yet"}.</p>}
      </div>

      <UnitsDrawer code={unitsOf} onClose={() => setUnitsOf(null)} />
      <MergeDialog code={mergeOf} codes={live} onClose={() => setMergeOf(null)} />
    </>
  )
}
