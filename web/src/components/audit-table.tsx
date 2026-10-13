import { Fragment, useState } from "react"

import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { dateTime, shortHash } from "@/lib/format"
import type { AuditEvent } from "@/lib/types"

export function AuditTable({ events, showInterview }: { events: AuditEvent[]; showInterview?: boolean }) {
  const [open, setOpen] = useState<number | null>(null)
  const rows = [...events].sort((a, b) => b.seq - a.seq)
  return (
    <div className="bg-card overflow-x-auto rounded-md border">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="w-14 pl-4 text-right">N°</TableHead>
            <TableHead>Date</TableHead>
            <TableHead>Auteur</TableHead>
            <TableHead>Action</TableHead>
            {showInterview && <TableHead>Entretien</TableHead>}
            <TableHead>Cible</TableHead>
            <TableHead className="pr-4">Empreinte</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((e) => (
            <Fragment key={e.seq}>
              <TableRow className="cursor-pointer" onClick={() => setOpen(open === e.seq ? null : e.seq)} aria-expanded={open === e.seq}>
                <TableCell className="pl-4 text-right tabular-nums">{e.seq}</TableCell>
                <TableCell className="text-muted-foreground text-xs whitespace-nowrap">{dateTime(e.ts)}</TableCell>
                <TableCell className="text-xs">{e.actor}</TableCell>
                <TableCell className="ident">{e.action}</TableCell>
                {showInterview && <TableCell className="ident">{e.interview_id ?? ""}</TableCell>}
                <TableCell className="ident max-w-[24rem] truncate" title={e.target ?? undefined}>
                  {e.target ?? ""}
                </TableCell>
                <TableCell className="ident text-muted-foreground pr-4">{shortHash(e.hash, 12)}</TableCell>
              </TableRow>
              {open === e.seq && (
                <TableRow className="hover:bg-transparent">
                  <TableCell colSpan={showInterview ? 7 : 6} className="bg-muted/30 px-4">
                    <pre className="ident max-h-80 overflow-auto text-xs whitespace-pre-wrap">{JSON.stringify(e.detail, null, 2)}</pre>
                    <p className="text-muted-foreground mt-2 text-xs">
                      Empreinte précédente <span className="ident">{e.prev_hash}</span>
                      <br />
                      Cette empreinte <span className="ident">{e.hash}</span>
                    </p>
                  </TableCell>
                </TableRow>
              )}
            </Fragment>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}
