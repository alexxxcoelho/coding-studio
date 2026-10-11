import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, Section } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { bytes, dateTime, shortHash } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import type { InterviewDetail, SourceKind } from "@/lib/types"

const KIND_LABEL: Record<SourceKind, string> = {
  vtt: "Platform transcript (VTT)",
  timestamped_text: "Raw ASR text",
  cleaned_markdown: "Cleaned transcript",
  budgie_markdown: "Budgie Echo export",
  written_response: "Written answer",
  summary: "Summary",
  record_sheet: "Record sheet",
  toe_coding: "Earlier coding",
  other: "Other",
}

const TRANSCRIPT: SourceKind[] = ["vtt", "timestamped_text", "cleaned_markdown", "budgie_markdown", "written_response"]

function FileViewer({ id, path, onClose }: { id: string; path: string | null; onClose: () => void }) {
  const file = useQuery({ queryKey: ["file", id, path], queryFn: () => api.file(id, path!), enabled: !!path })
  return (
    <Sheet open={!!path} onOpenChange={(o) => !o && onClose()}>
      <SheetContent side="right" className="w-[min(52rem,92vw)] sm:max-w-none">
        <SheetHeader>
          <SheetTitle className="ident break-all">{path}</SheetTitle>
          <SheetDescription>Read only. The file on disk is never modified.</SheetDescription>
        </SheetHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-4 pb-4">
          {file.data?.changed_since_scan && (
            <Banner tone="warn" title="This file changed since the last scan">
              Rescan the library so the inventory and the audit trail record the new SHA-256.
            </Banner>
          )}
          {file.isLoading && <Skeleton className="h-64 w-full" />}
          {file.error && <Banner tone="danger" title="The file could not be read">{file.error.message}</Banner>}
          {file.data && (
            <pre className="bg-muted/40 min-h-0 flex-1 overflow-auto rounded-md border p-4 font-serif text-[0.95rem] leading-relaxed whitespace-pre-wrap">
              {file.data.text}
            </pre>
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}

export function SourcesTab({ detail }: { detail: InterviewDetail }) {
  const qc = useQueryClient()
  const id = detail.entry.interview_id
  const [viewing, setViewing] = useState<string | null>(null)
  const seg = detail.segmentation
  const coded = detail.phase1.n_human_coded > 0 || detail.phase2_started

  const primary = useMutation({
    mutationFn: (rel: string) => api.setPrimary(id, rel),
    onSuccess: () => invalidateInterview(qc, id),
    onError: (e) => reportError(e),
  })
  const segment = useMutation({
    mutationFn: () => api.segment(id, detail.entry.primary_source ?? undefined),
    onSuccess: (s) => {
      toast.success(`Segmented into ${s.units.length} units`)
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Segmentation refused"),
  })

  return (
    <div className="flex flex-col gap-5">
      <Section
        title="Segmentation"
        aside={seg ? `rule ${seg.rule.split(":")[0]}` : undefined}
      >
        {seg ? (
          <div className="flex flex-wrap items-start justify-between gap-4 text-sm">
            <div className="flex flex-col gap-1">
              <div>
                <span className="font-semibold tabular-nums">{seg.units.length} units</span> from{" "}
                <span className="ident">{seg.source_rel_path}</span>
              </div>
              <div className="text-muted-foreground text-xs">
                Source SHA-256 <span className="ident">{shortHash(seg.source_sha256, 16)}</span>, unit list hash{" "}
                <span className="ident">{shortHash(seg.hash, 16)}</span>
              </div>
              <div className="text-muted-foreground max-w-prose text-xs">{seg.rule}</div>
              {seg.warnings.length > 0 && (
                <details className="text-warn mt-1 text-xs">
                  <summary className="cursor-pointer">{seg.warnings.length} lines not turned into units</summary>
                  <ul className="mt-1 list-inside list-disc">
                    {seg.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
            <Button asChild>
              <a href={interviewHref(id, "coding", 1)}>Start coding phase 1</a>
            </Button>
          </div>
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
            <p className="text-muted-foreground max-w-prose">
              {detail.entry.primary_source
                ? "Segment the primary transcript: one unit per speaker turn, long turns split at sentence ends. The same file always gives the same units."
                : "No transcript was found in this folder. Add one and rescan the library."}
            </p>
            <Button disabled={!detail.entry.primary_source || segment.isPending} onClick={() => segment.mutate()}>
              {segment.isPending ? "Segmenting..." : "Segment the primary transcript"}
            </Button>
          </div>
        )}
        {seg && coded && (
          <p className="text-muted-foreground mt-3 text-xs">
            Coding has started, so the segmentation is frozen. Any change would break the link between codes and units.
          </p>
        )}
      </Section>

      <Section title="Files in the interview folder" aside={`${detail.entry.sources.length} inventoried`}>
        <div className="-m-4 overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="pl-4">File</TableHead>
                <TableHead>Kind</TableHead>
                <TableHead>SHA-256</TableHead>
                <TableHead className="text-right">Size</TableHead>
                <TableHead>Modified</TableHead>
                <TableHead className="pr-4 text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {detail.entry.sources.map((f) => {
                const isPrimary = f.rel_path === detail.entry.primary_source
                const transcript = TRANSCRIPT.includes(f.kind)
                return (
                  <TableRow key={f.rel_path}>
                    <TableCell className="pl-4">
                      <span className="ident break-all">{f.rel_path}</span>
                      {isPrimary && (
                        <Badge variant="secondary" className="ml-2">
                          primary
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">{KIND_LABEL[f.kind]}</TableCell>
                    <TableCell className="ident text-muted-foreground">{shortHash(f.sha256, 12)}</TableCell>
                    <TableCell className="text-right text-xs tabular-nums">{bytes(f.size)}</TableCell>
                    <TableCell className="text-muted-foreground text-xs">{dateTime(f.modified)}</TableCell>
                    <TableCell className="pr-4 text-right whitespace-nowrap">
                      <Button size="sm" variant="ghost" onClick={() => setViewing(f.rel_path)}>
                        View
                      </Button>
                      {transcript && !isPrimary && !seg && (
                        <Button size="sm" variant="outline" disabled={primary.isPending} onClick={() => primary.mutate(f.rel_path)}>
                          Use as primary
                        </Button>
                      )}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </div>
      </Section>
      <FileViewer id={id} path={viewing} onClose={() => setViewing(null)} />
    </div>
  )
}
