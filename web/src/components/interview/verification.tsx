// Verification tab: coverage of the audio check, the units that must be
// checked and why, and their current state. Counts describe the eligible coded
// corpus; the share of targets checked is reported apart from them.

import { useQuery } from "@tanstack/react-query"

import { Banner, EmptyState, Section, Stat } from "@/components/bits"
import { VerificationBadge } from "@/components/interview/qualifications"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { coverageShare, REASON_HELP, targetRows } from "@/lib/qualifications"
import { percent, dateTime } from "@/lib/format"
import { unitHref } from "@/lib/router"
import type { VerificationReason } from "@/lib/types"

const REASONS = Object.keys(REASON_HELP) as VerificationReason[]

export function VerificationTab({ id }: { id: string }) {
  const q = useQuery({ queryKey: ["verification", id], queryFn: () => api.verification(id) })

  if (q.isLoading) return <Skeleton className="h-64 w-full" />
  if (q.error) return <Banner tone="danger" title="Verification could not be loaded">{q.error.message}</Banner>
  const d = q.data!
  const rows = targetRows(d.targets, d.status)
  const cov = d.coverage
  const share = coverageShare(cov)

  if (cov.n_units === 0 && rows.length === 0)
    return <EmptyState title="Nothing to verify yet">Code the unit of this interview in phase 2 first; targets are drawn from the coded corpus.</EmptyState>

  return (
    <div className="flex flex-col gap-4">
      <Section title="Coverage" aside="counts describe the eligible coded corpus; the share of targets checked is reported apart">
        <div className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3 lg:grid-cols-6">
          <Stat label="Units in the corpus" value={cov.n_units} />
          <Stat label="Verified" value={cov.n_verified} tone={cov.n_verified ? "ok" : undefined} />
          <Stat label="Unusable" value={cov.n_unusable} />
          <Stat label="Stale" value={cov.n_stale} tone={cov.n_stale ? "warn" : undefined} />
          <Stat label="Targets" value={cov.n_targets} />
          <Stat label="Targets verified" value={share === null ? "none" : `${cov.n_targets_verified} of ${cov.n_targets}, ${percent(share)}`} />
        </div>
      </Section>

      <Section title="Units to verify against the audio" aside={`${rows.length} ${rows.length === 1 ? "unit" : "units"}`}>
        {rows.length === 0 ? (
          <p className="text-muted-foreground text-sm">No unit is a target yet.</p>
        ) : (
          <div className="-m-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Unit</TableHead>
                  <TableHead>Why it is a target</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead>Checked</TableHead>
                  <TableHead className="pr-4 text-right">
                    <span className="sr-only">Action</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r) => (
                  <TableRow key={r.unit_id}>
                    <TableCell className="pl-4">
                      <span className="ident">{r.unit_id}</span>
                    </TableCell>
                    <TableCell>
                      <span className="flex flex-wrap gap-1">
                        {r.reasons.map((reason) => (
                          <span key={reason} className="bg-muted rounded px-1.5 py-0.5 text-xs" title={REASON_HELP[reason]}>
                            {reason}
                          </span>
                        ))}
                      </span>
                    </TableCell>
                    <TableCell>
                      <VerificationBadge v={r} />
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">
                      {r.at ? (
                        <>
                          {dateTime(r.at)}
                          {r.by ? `, ${r.by}` : ""}
                          {r.note && <span className="block italic">{r.note}</span>}
                        </>
                      ) : (
                        "not yet"
                      )}
                    </TableCell>
                    <TableCell className="pr-4 text-right">
                      <Button size="xs" variant="ghost" asChild>
                        <a href={unitHref(r.unit_id)}>Open in coding</a>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </Section>

      <Section title="Legend" aside="a unit can be a target for several reasons">
        <dl className="grid gap-2.5 md:grid-cols-2">
          {REASONS.map((r) => (
            <div key={r} className="flex gap-2.5 text-sm">
              <dt className="shrink-0">
                <span className="bg-muted rounded px-1.5 py-0.5 text-xs">{r}</span>
              </dt>
              <dd className="text-muted-foreground text-xs leading-snug">{REASON_HELP[r]}</dd>
            </div>
          ))}
        </dl>
        <p className="text-muted-foreground mt-3 text-xs">
          A verified unit keeps its state until its text or speaker changes; then it shows as stale and must be checked again. Unverified is the
          absence of a check, not a finding about the unit.
        </p>
      </Section>
    </div>
  )
}
