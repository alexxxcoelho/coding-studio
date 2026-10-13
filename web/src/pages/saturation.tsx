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
  if (q.error) return <Banner tone="danger" title="Le tableau de saturation n'a pas pu être chargé">{q.error.message}</Banner>

  const rows = inOrder(q.data ?? [])
  const cumulative = cumulativeCodes(rows)
  const totals = saturationTotals(rows)

  return (
    <>
      <PageTitle title="Saturation" sub="Lecture provisoire : première utilisation et création seulement. L'enrichissement du sens se documente dans les mémos, il n'est pas compté ici." />
      <Banner tone="info" title="Lecture provisoire : première utilisation et création seulement" className="mb-4">
        Un code est compté dans le premier entretien, dans l'ordre du codage, dont le codage de phase 2 le porte. Un code de terrain est compté là où
        il a été créé. La colonne cumulée se lit comme une courbe : une fin plate signifie que les entretiens suivants n'ont ajouté aucun code. Le sens
        gagné par un code se trouve dans les mémos de la page Réponses.
      </Banner>

      {rows.length === 0 ? (
        <EmptyState title="Aucun codage de phase 2 pour l'instant">Le tableau se remplit à mesure que les entretiens sont codés en phase 2.</EmptyState>
      ) : (
        <Section title="Par entretien, dans l'ordre du premier codage de phase 2" aside={`${rows.length} ${rows.length < 2 ? "entretien" : "entretiens"}`}>
          <div className="-m-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Ordre</TableHead>
                  <TableHead>Entretien</TableHead>
                  <TableHead>Premier codage de phase 2</TableHead>
                  <TableHead>Codes utilisés pour la première fois</TableHead>
                  <TableHead className="text-right">Cumul</TableHead>
                  <TableHead>Codes de terrain créés</TableHead>
                  <TableHead className="text-right">Étiquettes ouvertes</TableHead>
                  <TableHead className="pr-4 text-right">Étiquettes résolues</TableHead>
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
                    <TableCell className="text-muted-foreground text-xs tabular-nums">{r.first_coded_at ? dateTime(r.first_coded_at) : "pas encore"}</TableCell>
                    <TableCell>
                      <ChipList codes={r.codes_first_used} empty="aucun nouveau" />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{cumulative[i]}</TableCell>
                    <TableCell>
                      <ChipList codes={r.field_codes_created} empty="aucun" />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{r.labels_opened}</TableCell>
                    <TableCell className="pr-4 text-right tabular-nums">{r.labels_resolved}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
              <TableFooter>
                <TableRow>
                  <TableCell className="pl-4 font-medium" colSpan={3}>
                    Totaux
                  </TableCell>
                  <TableCell className="font-medium tabular-nums">{totals.codes} codes</TableCell>
                  <TableCell className="text-right tabular-nums">{cumulative.at(-1) ?? 0}</TableCell>
                  <TableCell className="font-medium tabular-nums">{totals.fieldCodes} codes de terrain</TableCell>
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
