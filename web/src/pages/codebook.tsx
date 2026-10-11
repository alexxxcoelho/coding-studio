import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeChip, EmptyState, PageTitle, Section } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { familyName, groupByFamily } from "@/lib/codes"
import { dateTime } from "@/lib/format"
import type { Code } from "@/lib/types"

function NewInductiveCode() {
  const qc = useQueryClient()
  const [open, setOpen] = useState(false)
  const blank = { code: "", label: "", definition: "", example: "", reason: "" }
  const [f, setF] = useState(blank)
  const set = (k: keyof typeof f) => (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => setF((x) => ({ ...x, [k]: e.target.value }))
  const add = useMutation({
    mutationFn: () => {
      const code: Code = {
        code: f.code.trim().toUpperCase(),
        family: "INDUCTIVE",
        label: f.label.trim(),
        definition: f.definition.trim(),
        origin: "inductive",
        active: true,
        in_prompt: false,
        example: f.example.trim() || null,
        reason: f.reason.trim(),
        created_at: null,
      }
      return api.addCode(code)
    },
    onSuccess: (book) => {
      qc.setQueryData(["codebook"], book)
      qc.invalidateQueries({ queryKey: ["chain"] })
      toast.success(`Inductive code ${f.code.trim().toUpperCase()} added to the log`)
      setF(blank)
      setOpen(false)
    },
    onError: (e) => reportError(e, "Code not added"),
  })
  const valid = f.code.trim() && f.label.trim() && f.definition.trim() && f.reason.trim()
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button>New inductive code</Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>New inductive code</DialogTitle>
          <DialogDescription>
            For material the TOE framework does not accommodate. The code is offered in phase 2 to you only, never to the AI coder.
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            if (valid) add.mutate()
          }}
        >
          <div className="grid grid-cols-[9rem_1fr] gap-3">
            <label className="flex flex-col gap-1 text-xs">
              <span className="text-muted-foreground">Code</span>
              <Input value={f.code} onChange={set("code")} placeholder="BUSINESS-CASE" className="ident uppercase" required />
            </label>
            <label className="flex flex-col gap-1 text-xs">
              <span className="text-muted-foreground">Label</span>
              <Input value={f.label} onChange={set("label")} placeholder="Business case construction" required />
            </label>
          </div>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Definition, required</span>
            <Textarea rows={3} value={f.definition} onChange={set("definition")} required />
          </label>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Why the code was created, required</span>
            <Textarea rows={2} value={f.reason} onChange={set("reason")} required />
          </label>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Example, a unit ID or a short quote</span>
            <Textarea rows={2} value={f.example} onChange={set("example")} />
          </label>
          <DialogFooter>
            <Button type="submit" disabled={!valid || add.isPending}>
              Add to the inductive log
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function CodebookPage() {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const p1 = useQuery({ queryKey: ["codebook-phase1"], queryFn: api.phase1Codebook })
  const [typed, setPath] = useState<string | null>(null)
  const path = typed ?? state.data?.coding_db ?? ""

  const imp = useMutation({
    mutationFn: () => api.importCodebook(path.trim() || undefined),
    onSuccess: (b) => {
      qc.setQueryData(["codebook"], b)
      qc.invalidateQueries({ queryKey: ["state"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
      toast.success(`${b.codes.length} codes imported`)
    },
    onError: (e) => reportError(e, "Import failed"),
  })
  const toggle = useMutation({
    mutationFn: ({ code, on }: { code: string; on: boolean }) => api.setInPrompt(code, on),
    onSuccess: (b) => {
      qc.setQueryData(["codebook"], b)
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => reportError(e),
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
  const imported = codes.filter((c) => c.origin !== "inductive")
  const inductive = codes.filter((c) => c.origin === "inductive")
  const offered = imported.filter((c) => c.active && c.in_prompt).length

  return (
    <>
      <PageTitle
        title="Codebook"
        sub={book.data ? `${codes.length} codes from ${book.data.source || "no source yet"}, ${offered} offered to the AI coder in phase 2` : " "}
      />

      <div className="flex flex-col gap-5">
        <Section title="Phase 1 categories" aside="the TOE contexts, plus relevant material outside them">
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
            Importing again keeps your inductive codes and your choices of what is offered to the AI. A code removed upstream is deactivated, never
            deleted.
          </p>
        </form>

        <Section title="Inductive log" aside="codes created from the data, phase 2 only" className="overflow-hidden">
          <div className="mb-3 flex items-start justify-between gap-3">
            <p className="text-muted-foreground max-w-prose text-sm">
              Units coded INDUCTIVE-PENDING in phase 1 are relevant but outside the three contexts. In phase 2 you may give them an inductive code,
              defined here with its reason and an example, or an outcome, turning point or transfer code.
            </p>
            <NewInductiveCode />
          </div>
          {inductive.length === 0 ? (
            <p className="text-muted-foreground text-sm">No inductive code yet.</p>
          ) : (
            <div className="-mx-4 -mb-4 overflow-x-auto border-t">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="pl-4">Code</TableHead>
                    <TableHead>Definition</TableHead>
                    <TableHead>Reason</TableHead>
                    <TableHead>Example</TableHead>
                    <TableHead className="pr-4">Created</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {inductive.map((c) => (
                    <TableRow key={c.code} className="align-top">
                      <TableCell className="pl-4">
                        <CodeChip code={c.code} family={c.family} />
                        <div className="mt-1 text-xs">{c.label}</div>
                      </TableCell>
                      <TableCell className="max-w-[22rem] text-xs whitespace-normal">{c.definition}</TableCell>
                      <TableCell className="max-w-[18rem] text-xs whitespace-normal">{c.reason}</TableCell>
                      <TableCell className="max-w-[16rem] text-xs whitespace-normal">{c.example}</TableCell>
                      <TableCell className="text-muted-foreground pr-4 text-xs">{dateTime(c.created_at)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </Section>

        {book.isLoading ? (
          <Skeleton className="h-96 w-full" />
        ) : imported.length === 0 ? (
          <EmptyState title="The study codebook is empty">Import it from the thesis coding.db, which stays untouched.</EmptyState>
        ) : (
          <div className="bg-card overflow-x-auto rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Code</TableHead>
                  <TableHead>Label</TableHead>
                  <TableHead>Definition</TableHead>
                  <TableHead>Origin</TableHead>
                  <TableHead className="pr-4">Offered to AI</TableHead>
                </TableRow>
              </TableHeader>
              {groupByFamily(imported).map(({ family, codes: list }) => (
                <TableBody key={family}>
                  <TableRow className="bg-muted/40 hover:bg-muted/40">
                    <TableCell colSpan={5} className="pl-4 text-xs font-semibold">
                      {familyName(family)}
                    </TableCell>
                  </TableRow>
                  {list.map((c) => (
                    <TableRow key={c.code} className={c.active ? "align-top" : "align-top opacity-55"}>
                      <TableCell className="pl-4">
                        <CodeChip code={c.code} family={c.family} />
                        {!c.active && <div className="text-muted-foreground mt-1 text-xs">inactive</div>}
                      </TableCell>
                      <TableCell className="text-sm">{c.label}</TableCell>
                      <TableCell className="text-muted-foreground max-w-[34rem] text-xs whitespace-normal">{c.definition}</TableCell>
                      <TableCell className="text-muted-foreground text-xs">{c.origin}</TableCell>
                      <TableCell className="pr-4">
                        <Checkbox
                          checked={c.in_prompt}
                          disabled={!c.active || toggle.isPending}
                          aria-label={`Offer ${c.code} to the AI coder`}
                          onCheckedChange={(v) => toggle.mutate({ code: c.code, on: v === true })}
                        />
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              ))}
            </Table>
          </div>
        )}
      </div>
    </>
  )
}
