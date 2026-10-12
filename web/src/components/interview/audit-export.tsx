import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { AuditTable } from "@/components/audit-table"
import { Banner, Section } from "@/components/bits"
import { reportError } from "@/lib/notify"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { bytes, plural, shortHash } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"

export function AuditTab({ id }: { id: string }) {
  const qc = useQueryClient()
  const events = useQuery({ queryKey: ["audit", id], queryFn: () => api.audit(id) })
  const chain = useQuery({ queryKey: ["chain"], queryFn: api.verify })
  const exp = useMutation({
    mutationFn: () => api.exportPackage(id),
    onSuccess: (r) => {
      toast.success(`Package written: ${plural(r.files.length, "file")}`)
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Export failed"),
  })

  return (
    <div className="flex flex-col gap-5">
      <Section title="Export the audit package" aside="nothing is ever overwritten">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <p className="text-muted-foreground max-w-prose text-sm">
            Writes a new dated folder next to the database: the units, the phase 1 pre-classification, your phase 2 coding (with changes made after
            the AI answer flagged), every prompt and raw AI answer, the review and the final coding, the labels to clarify, the cases, the codebook,
            this interview's audit events and a manifest of SHA-256 hashes.
          </p>
          <Button onClick={() => exp.mutate()} disabled={exp.isPending}>
            {exp.isPending ? "Writing..." : "Export the package"}
          </Button>
        </div>
        {exp.data && (
          <div className="mt-4 flex flex-col gap-3">
            <Banner tone="ok" title="Package written">
              <span className="ident break-all">{exp.data.dir}</span>
              {exp.data.manifest_sha256 && (
                <span className="block text-xs">
                  Manifest SHA-256 <span className="ident">{exp.data.manifest_sha256}</span>
                </span>
              )}
            </Banner>
            <div className="max-h-80 overflow-auto rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="pl-4">File</TableHead>
                    <TableHead>SHA-256</TableHead>
                    <TableHead className="pr-4 text-right">Size</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {exp.data.files.map((f) => (
                    <TableRow key={f.name}>
                      <TableCell className="ident pl-4">{f.name}</TableCell>
                      <TableCell className="ident text-muted-foreground">{shortHash(f.sha256, 16)}</TableCell>
                      <TableCell className="pr-4 text-right text-xs tabular-nums">{bytes(f.size)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        )}
      </Section>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-base font-semibold">Recorded actions on this interview</h2>
        {chain.data && (
          <span className={chain.data.ok ? "text-ok text-sm" : "text-destructive text-sm"}>
            {chain.data.ok
              ? `Chain intact: ${chain.data.n_events} events in the database, every hash recomputed`
              : `Chain broken at event ${chain.data.first_bad_seq}`}
          </span>
        )}
      </div>
      {events.isLoading ? <Skeleton className="h-64 w-full" /> : <AuditTable events={events.data ?? []} />}
    </div>
  )
}
