// Saturation: interview by interview, the codes whose first use falls there,
// the field codes created, and the labels to clarify opened and resolved. The
// reading is provisional: first use and creation only.

import { useQuery } from "@tanstack/react-query"

import { Banner, ChipList, EmptyState, PageTitle, Section } from "@/components/bits"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableFooter, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { cumulativeCodes, inOrder, interviewLabel, saturationTotals } from "@/lib/saturation"
import { dateTime } from "@/lib/format"
import { interviewHref } from "@/lib/router"

export function SaturationPage() {
  const q = useQuery({ queryKey: ["saturation"], queryFn: api.saturation })

  if (q.isLoading) return <Skeleton className="h-96 w-full" />
  if (q.error) return <Banner tone="danger" title={t("saturation.le_tableau_de_saturation_n")}>{q.error.message}</Banner>

  const rows = inOrder(q.data ?? [])
  const cumulative = cumulativeCodes(rows)
  const totals = saturationTotals(rows)

  return (
    <>
      <PageTitle title={t("saturation.saturation")} sub={t("saturation.lecture_provisoire_premiere_utilisat")} />
      <Banner tone="info" title={t("saturation.lecture_provisoire_premiere_utilisat_2")} className="mb-4">
        {t("saturation.un_code_est_compte_dans")}
      </Banner>

      {rows.length === 0 ? (
        <EmptyState title={t("saturation.aucun_codage_de_phase_2")}>{t("saturation.le_tableau_se_remplit_a")}</EmptyState>
      ) : (
        <Section title={t("saturation.par_entretien_dans_l_ordre")} aside={t("saturation.n_entretiens", { n: rows.length })}>
          <div className="-m-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">{t("saturation.ordre")}</TableHead>
                  <TableHead>{t("common.entretien")}</TableHead>
                  <TableHead>{t("saturation.premier_codage_de_phase_2")}</TableHead>
                  <TableHead>{t("saturation.codes_utilises_pour_la_premiere")}</TableHead>
                  <TableHead className="text-right">{t("saturation.cumul")}</TableHead>
                  <TableHead>{t("saturation.codes_de_terrain_crees")}</TableHead>
                  <TableHead className="text-right">{t("saturation.etiquettes_ouvertes")}</TableHead>
                  <TableHead className="pr-4 text-right">{t("saturation.etiquettes_resolues")}</TableHead>
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
                    <TableCell className="text-muted-foreground text-xs tabular-nums">{r.first_coded_at ? dateTime(r.first_coded_at) : t("common.pas_encore")}</TableCell>
                    <TableCell>
                      <ChipList codes={r.codes_first_used} empty={t("saturation.aucun_nouveau")} />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{cumulative[i]}</TableCell>
                    <TableCell>
                      <ChipList codes={r.field_codes_created} empty={t("common.aucun")} />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{r.labels_opened}</TableCell>
                    <TableCell className="pr-4 text-right tabular-nums">{r.labels_resolved}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
              <TableFooter>
                <TableRow>
                  <TableCell className="pl-4 font-medium" colSpan={3}>
                    {t("saturation.totaux")}
                  </TableCell>
                  <TableCell className="font-medium tabular-nums">{t("saturation.codes_codes", { codes: totals.codes })}</TableCell>
                  <TableCell className="text-right tabular-nums">{cumulative.at(-1) ?? 0}</TableCell>
                  <TableCell className="font-medium tabular-nums">{t("saturation.fieldcodes_codes_de_terrain", { fieldCodes: totals.fieldCodes })}</TableCell>
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
