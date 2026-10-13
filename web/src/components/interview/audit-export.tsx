import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { AuditTable } from "@/components/audit-table"
import { Banner, Section } from "@/components/bits"
import { t } from "@/i18n"
import { reportError } from "@/lib/notify"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { bytes, shortHash } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"

export function AuditTab({ id }: { id: string }) {
  const qc = useQueryClient()
  const events = useQuery({ queryKey: ["audit", id], queryFn: () => api.audit(id) })
  const chain = useQuery({ queryKey: ["chain"], queryFn: api.verify })
  const exp = useMutation({
    mutationFn: () => api.exportPackage(id),
    onSuccess: (r) => {
      toast.success(t("interview.audit_export.paquet_ecrit_items", { items: t("interview.audit_export.n_fichiers", { n: r.files.length }) }))
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, t("interview.audit_export.export_echoue")),
  })

  return (
    <div className="flex flex-col gap-5">
      <Section title={t("interview.audit_export.exporter_le_paquet_d_audit")} aside={t("interview.audit_export.rien_n_est_jamais_ecrase")}>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <p className="text-muted-foreground max-w-prose text-sm">
            {t("interview.audit_export.ecrit_un_nouveau_dossier_date")}
          </p>
          <Button onClick={() => exp.mutate()} disabled={exp.isPending}>
            {exp.isPending ? t("interview.audit_export.ecriture") : t("interview.audit_export.exporter_le_paquet")}
          </Button>
        </div>
        {exp.data && (
          <div className="mt-4 flex flex-col gap-3">
            <Banner tone="ok" title={t("interview.audit_export.paquet_ecrit")}>
              <span className="ident break-all">{exp.data.dir}</span>
              {exp.data.manifest_sha256 && (
                <span className="block text-xs">
                  {t("interview.audit_export.empreinte_sha_256_du_manifeste")} <span className="ident">{exp.data.manifest_sha256}</span>
                </span>
              )}
            </Banner>
            <div className="max-h-80 overflow-auto rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="pl-4">{t("common.fichier")}</TableHead>
                    <TableHead>SHA-256</TableHead>
                    <TableHead className="pr-4 text-right">{t("common.taille")}</TableHead>
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
        <h2 className="text-base font-semibold">{t("interview.audit_export.actions_enregistrees_pour_cet_entret")}</h2>
        {chain.data && (
          <span className={chain.data.ok ? "text-ok text-sm" : "text-destructive text-sm"}>
            {chain.data.ok
              ? t("interview.audit_export.chaine_intacte_n_events_evenements", { n_events: chain.data.n_events })
              : t("common.chaine_rompue_a_l_evenement", { first_bad_seq: chain.data.first_bad_seq })}
          </span>
        )}
      </div>
      {events.isLoading ? <Skeleton className="h-64 w-full" /> : <AuditTable events={events.data ?? []} />}
    </div>
  )
}
