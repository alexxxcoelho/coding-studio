import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, EmptyState, PageTitle } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { nextStep } from "@/lib/pipeline"
import { interviewHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { PhaseSummary } from "@/lib/types"

function PhaseCell({ s, open }: { s: PhaseSummary; open: boolean }) {
  if (!open) return <span className="text-muted-foreground text-xs">not open</span>
  if (s.n_units === 0) return <span className="text-muted-foreground text-xs">no unit</span>
  const pct = s.n_units ? s.n_human_coded / s.n_units : 0
  return (
    <div className="flex min-w-[11rem] flex-col gap-1">
      <div className="flex items-center gap-2">
        <div className="bg-muted h-1.5 w-16 overflow-hidden rounded-full" aria-hidden>
          <div className={cn("h-full", s.human_complete ? "bg-ok" : "bg-primary")} style={{ width: `${pct * 100}%` }} />
        </div>
        <span className="text-xs tabular-nums">
          {s.n_human_coded}/{s.n_units} coded
        </span>
      </div>
      <div className="text-muted-foreground flex gap-3 text-xs tabular-nums">
        <span>{s.n_ai_runs_complete} AI run{s.n_ai_runs_complete === 1 ? "" : "s"}</span>
        {s.n_divergences > 0 && (
          <span>
            {s.n_arbitrated}/{s.n_divergences} decided
          </span>
        )}
        {s.reconciled && <span className="text-ok font-medium">reconciled</span>}
      </div>
    </div>
  )
}

export function LibraryPage() {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const list = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [typed, setRoot] = useState<string | null>(null)
  const root = typed ?? state.data?.library_root ?? ""

  const scan = useMutation({
    mutationFn: () => api.scan(root.trim() || undefined),
    onSuccess: (res) => {
      qc.setQueryData(["interviews"], res.interviews)
      qc.invalidateQueries({ queryKey: ["state"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
      toast.success(`Scanned ${res.interviews.length} interviews`)
    },
    onError: (e) => reportError(e, "Scan failed"),
  })

  const browse = async () => {
    try {
      const res = await api.pick("folder", "Choose the interview folder")
      if (res.path) setRoot(res.path)
    } catch (e) {
      reportError(e)
    }
  }

  const rows = list.data ?? []
  return (
    <>
      <PageTitle
        title="Interviews"
        sub={state.data ? `${rows.length} in the library, codebook of ${state.data.codebook_size} codes` : " "}
      />

      <form
        className="bg-card mb-6 flex flex-wrap items-end gap-2 rounded-md border p-4"
        onSubmit={(e) => {
          e.preventDefault()
          scan.mutate()
        }}
      >
        <label className="flex min-w-[20rem] flex-1 flex-col gap-1.5 text-sm">
          <span className="text-muted-foreground text-xs">Interview folder, one sub-folder per interview</span>
          <Input value={root} onChange={(e) => setRoot(e.target.value)} placeholder="/path/to/04_Fieldwork/Interviews" />
        </label>
        {state.data?.native_dialogs && (
          <Button type="button" variant="outline" onClick={browse}>
            Browse...
          </Button>
        )}
        <Button type="submit" disabled={scan.isPending}>
          {scan.isPending ? "Scanning..." : "Scan folder"}
        </Button>
        <p className="text-muted-foreground basis-full text-xs">
          A rescan records every added, removed or changed file with its SHA-256. Transcripts are read, never modified.
        </p>
      </form>

      {state.data && state.data.codebook_size === 0 && (
        <Banner
          tone="warn"
          className="mb-4"
          title="The codebook is empty"
          action={
            <Button size="sm" variant="outline" asChild>
              <a href="#/codebook">Import the codebook</a>
            </Button>
          }
        >
          Phase 2 needs the study-specific codes from the thesis coding.db.
        </Banner>
      )}

      {list.isLoading ? (
        <div className="flex flex-col gap-2">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </div>
      ) : rows.length === 0 ? (
        <EmptyState title="No interview yet">
          Choose the folder that holds one sub-folder per interview, named with its ID (for example ECO-05_Vieweger), then
          scan it.
        </EmptyState>
      ) : (
        <div className="bg-card overflow-x-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Interview</TableHead>
                <TableHead className="text-right">Units</TableHead>
                <TableHead>Phase 1, TOE contexts</TableHead>
                <TableHead>Phase 2, study codes</TableHead>
                <TableHead>Next step</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((s) => {
                const next = nextStep(s)
                return (
                  <TableRow key={s.interview_id}>
                    <TableCell>
                      <a href={interviewHref(s.interview_id)} className="ident font-semibold hover:underline">
                        {s.interview_id}
                      </a>
                      <div className="text-muted-foreground text-xs">
                        {s.folder_name}
                        {s.n_warnings > 0 && <span className="text-warn">, {s.n_warnings} warning{s.n_warnings > 1 ? "s" : ""}</span>}
                      </div>
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{s.n_units || "-"}</TableCell>
                    <TableCell>
                      <PhaseCell s={s.phase1} open={s.n_units > 0} />
                    </TableCell>
                    <TableCell>
                      <PhaseCell s={s.phase2} open={s.phase1.reconciled} />
                    </TableCell>
                    <TableCell>
                      <a href={next.href} className={cn("text-sm hover:underline", next.done ? "text-ok" : "text-primary")}>
                        {next.label}
                      </a>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </div>
      )}
    </>
  )
}
