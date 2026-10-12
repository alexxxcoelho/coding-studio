// Saturation: interview by interview, the codes whose first use falls there,
// the field codes created, and the labels to clarify opened and resolved. The
// reading is provisional: first use and creation only.

import { useQuery } from "@tanstack/react-query"

import { Banner, ChipList, EmptyState, PageTitle, Section } from "@/components/bits"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableFooter, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { cumulativeCodes, inOrder, interviewLabel, saturationTotals } from "@/lib/saturation"
import { dateTime } from "@/lib/format"
import { interviewHref } from "@/lib/router"

export function SaturationPage() {
  const q = useQuery({ queryKey: ["saturation"], queryFn: api.saturation })

  if (q.isLoading) return <Skeleton className="h-96 w-full" />
  if (q.error) return <Banner tone="danger" title="The saturation table could not be loaded">{q.error.message}</Banner>

  const rows = inOrder(q.data ?? [])
  const cumulative = cumulativeCodes(rows)
  const totals = saturationTotals(rows)

  return (
    <>
      <PageTitle title="Saturation" sub="Provisional; first use and creation only. Enrichment of meaning is documented in memos, not counted here." />
      <Banner tone="info" title="Provisional; first use and creation only" className="mb-4">
        A code is counted in the first interview, in coding order, whose phase 2 coding carries it. A field code is counted where it was created. The
        cumulative column reads as a curve: a flat end means the later interviews added no new code. Meaning gained by a code is in the memos on the
        Answers page.
      </Banner>

      {rows.length === 0 ? (
        <EmptyState title="No phase 2 coding yet">The table fills as interviews are coded in phase 2.</EmptyState>
      ) : (
        <Section title="By interview, in order of first phase 2 coding" aside={`${rows.length} ${rows.length === 1 ? "interview" : "interviews"}`}>
          <div className="-m-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Order</TableHead>
                  <TableHead>Interview</TableHead>
                  <TableHead>First phase 2 coding</TableHead>
                  <TableHead>Codes first used</TableHead>
                  <TableHead className="text-right">Cumulative</TableHead>
                  <TableHead>Field codes created</TableHead>
                  <TableHead className="text-right">Labels opened</TableHead>
                  <TableHead className="pr-4 text-right">Labels resolved</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r, i) => (
                  <TableRow key={`${r.order}-${r.interview_id}`}>
                    <TableCell className="pl-4 tabular-nums">{r.order}</TableCell>
                    <TableCell>
                      {r.interview_id ? (
                        <a href={interviewHref(r.interview_id, "coding", 2)} className="ident hover:underline">
                          {r.interview_id}
                        </a>
                      ) : (
                        <span className="text-muted-foreground text-xs">{interviewLabel(r.interview_id)}</span>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs tabular-nums">{r.first_coded_at ? dateTime(r.first_coded_at) : "not yet"}</TableCell>
                    <TableCell>
                      <ChipList codes={r.codes_first_used} empty="none new" />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{cumulative[i]}</TableCell>
                    <TableCell>
                      <ChipList codes={r.field_codes_created} empty="none" />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{r.labels_opened}</TableCell>
                    <TableCell className="pr-4 text-right tabular-nums">{r.labels_resolved}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
              <TableFooter>
                <TableRow>
                  <TableCell className="pl-4 font-medium" colSpan={3}>
                    Totals
                  </TableCell>
                  <TableCell className="font-medium tabular-nums">{totals.codes} codes</TableCell>
                  <TableCell className="text-right tabular-nums">{cumulative.at(-1) ?? 0}</TableCell>
                  <TableCell className="font-medium tabular-nums">{totals.fieldCodes} field codes</TableCell>
                  <TableCell className="text-right tabular-nums">{totals.labelsOpened}</TableCell>
                  <TableCell className="pr-4 text-right tabular-nums">{totals.labelsResolved}</TableCell>
                </TableRow>
              </TableFooter>
            </Table>
          </div>
        </Section>
      )}
    </>
  )
}
