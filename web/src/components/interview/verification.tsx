// Verification tab: coverage of the audio check, the units that must be
// checked and why, and their current state. Counts describe the eligible coded
// corpus; the share of targets checked is reported apart from them.

import { useQuery } from "@tanstack/react-query"

import { Banner, EmptyState, Section, Stat } from "@/components/bits"
import { VerificationBadge } from "@/components/interview/qualifications"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { coverageShare, REASON_HELP, REASON_LABEL, targetRows } from "@/lib/qualifications"
import { percent, dateTime } from "@/lib/format"
import { unitHref } from "@/lib/router"
import type { VerificationReason } from "@/lib/types"

const REASONS = Object.keys(REASON_HELP) as VerificationReason[]

export function VerificationTab({ id }: { id: string }) {
  const q = useQuery({ queryKey: ["verification", id], queryFn: () => api.verification(id) })

  if (q.isLoading) return <Skeleton className="h-64 w-full" />
  if (q.error) return <Banner tone="danger" title={t("interview.verification.la_verification_n_a_pas")}>{q.error.message}</Banner>
  const d = q.data!
  const rows = targetRows(d.targets, d.status)
  const cov = d.coverage
  const share = coverageShare(cov)

  if (cov.n_units === 0 && rows.length === 0)
    return <EmptyState title={t("interview.verification.rien_a_verifier_pour_l")}>{t("interview.verification.codez_d_abord_les_unites")}</EmptyState>

  return (
    <div className="flex flex-col gap-4">
      <Section title={t("interview.verification.couverture")} aside={t("interview.verification.les_decomptes_portent_sur_le")}>
        <div className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3 lg:grid-cols-6">
          <Stat label={t("interview.verification.unites_du_corpus")} value={cov.n_units} />
          <Stat label={t("interview.verification.verifiees")} value={cov.n_verified} tone={cov.n_verified ? "ok" : undefined} />
          <Stat label={t("interview.verification.inutilisables")} value={cov.n_unusable} />
          <Stat label={t("interview.verification.perimees")} value={cov.n_stale} tone={cov.n_stale ? "warn" : undefined} />
          <Stat label={t("interview.verification.cibles")} value={cov.n_targets} />
          <Stat label={t("interview.verification.cibles_verifiees")} value={share === null ? t("common.aucune") : t("interview.verification.n_targets_verified_sur_n", { n_targets_verified: cov.n_targets_verified, n_targets: cov.n_targets, percent: percent(share) })} />
        </div>
      </Section>

      <Section title={t("interview.verification.unites_a_verifier_a_l")} aside={`${rows.length} ${rows.length < 2 ? t("common.unite") : t("interview.verification.unites")}`}>
        {rows.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("interview.verification.aucune_unite_n_est_encore")}</p>
        ) : (
          <div className="-m-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">{t("interview.verification.unite")}</TableHead>
                  <TableHead>{t("interview.verification.pourquoi_c_est_une_cible")}</TableHead>
                  <TableHead>{t("interview.verification.etat")}</TableHead>
                  <TableHead>{t("interview.verification.controlee")}</TableHead>
                  <TableHead className="pr-4 text-right">
                    <span className="sr-only">{t("common.action")}</span>
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
                            {REASON_LABEL[reason]}
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
                        t("common.pas_encore")
                      )}
                    </TableCell>
                    <TableCell className="pr-4 text-right">
                      <Button size="xs" variant="ghost" asChild>
                        <a href={unitHref(r.unit_id)}>{t("interview.verification.ouvrir_dans_le_codage")}</a>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </Section>

      <Section title={t("interview.verification.legende")} aside={t("interview.verification.une_unite_peut_etre_une")}>
        <dl className="grid gap-2.5 md:grid-cols-2">
          {REASONS.map((r) => (
            <div key={r} className="flex gap-2.5 text-sm">
              <dt className="shrink-0">
                <span className="bg-muted rounded px-1.5 py-0.5 text-xs">{REASON_LABEL[r]}</span>
              </dt>
              <dd className="text-muted-foreground text-xs leading-snug">{REASON_HELP[r]}</dd>
            </div>
          ))}
        </dl>
        <p className="text-muted-foreground mt-3 text-xs">
          {t("interview.verification.une_unite_verifiee_garde_son")}
        </p>
      </Section>
    </div>
  )
}
