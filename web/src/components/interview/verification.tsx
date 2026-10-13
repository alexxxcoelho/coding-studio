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
  if (q.error) return <Banner tone="danger" title="La vérification n'a pas pu être chargée">{q.error.message}</Banner>
  const d = q.data!
  const rows = targetRows(d.targets, d.status)
  const cov = d.coverage
  const share = coverageShare(cov)

  if (cov.n_units === 0 && rows.length === 0)
    return <EmptyState title="Rien à vérifier pour l'instant">Codez d'abord les unités de cet entretien en phase 2 : les cibles sont tirées du corpus codé.</EmptyState>

  return (
    <div className="flex flex-col gap-4">
      <Section title="Couverture" aside="les décomptes portent sur le corpus codé éligible ; la part des cibles vérifiées est donnée à part">
        <div className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3 lg:grid-cols-6">
          <Stat label="Unités du corpus" value={cov.n_units} />
          <Stat label="Vérifiées" value={cov.n_verified} tone={cov.n_verified ? "ok" : undefined} />
          <Stat label="Inutilisables" value={cov.n_unusable} />
          <Stat label="Périmées" value={cov.n_stale} tone={cov.n_stale ? "warn" : undefined} />
          <Stat label="Cibles" value={cov.n_targets} />
          <Stat label="Cibles vérifiées" value={share === null ? "aucune" : `${cov.n_targets_verified} sur ${cov.n_targets}, ${percent(share)}`} />
        </div>
      </Section>

      <Section title="Unités à vérifier à l'audio" aside={`${rows.length} ${rows.length < 2 ? "unité" : "unités"}`}>
        {rows.length === 0 ? (
          <p className="text-muted-foreground text-sm">Aucune unité n'est encore une cible.</p>
        ) : (
          <div className="-m-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Unité</TableHead>
                  <TableHead>Pourquoi c'est une cible</TableHead>
                  <TableHead>État</TableHead>
                  <TableHead>Contrôlée</TableHead>
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
                        "pas encore"
                      )}
                    </TableCell>
                    <TableCell className="pr-4 text-right">
                      <Button size="xs" variant="ghost" asChild>
                        <a href={unitHref(r.unit_id)}>Ouvrir dans le codage</a>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </Section>

      <Section title="Légende" aside="une unité peut être une cible pour plusieurs raisons">
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
          Une unité vérifiée garde son état tant que son texte ou son locuteur ne change pas ; elle apparaît alors comme périmée et doit être contrôlée de nouveau.
          Non vérifiée signifie qu'aucun contrôle n'a eu lieu, pas qu'un défaut a été constaté.
        </p>
      </Section>
    </div>
  )
}
