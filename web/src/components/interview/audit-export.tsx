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
      toast.success(`Paquet écrit : ${plural(r.files.length, "fichier")}`)
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Export échoué"),
  })

  return (
    <div className="flex flex-col gap-5">
      <Section title="Exporter le paquet d'audit" aside="rien n'est jamais écrasé">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <p className="text-muted-foreground max-w-prose text-sm">
            Écrit un nouveau dossier daté à côté de la base : les unités, la préclassification de phase 1, votre codage de phase 2 (les changements faits
            après la réponse du modèle sont signalés), chaque prompt et chaque réponse brute du modèle, la revue et le codage final, les étiquettes à
            clarifier, les cas, le codebook, les événements d'audit de cet entretien et un manifeste d'empreintes SHA-256.
          </p>
          <Button onClick={() => exp.mutate()} disabled={exp.isPending}>
            {exp.isPending ? "Écriture…" : "Exporter le paquet"}
          </Button>
        </div>
        {exp.data && (
          <div className="mt-4 flex flex-col gap-3">
            <Banner tone="ok" title="Paquet écrit">
              <span className="ident break-all">{exp.data.dir}</span>
              {exp.data.manifest_sha256 && (
                <span className="block text-xs">
                  Empreinte SHA-256 du manifeste <span className="ident">{exp.data.manifest_sha256}</span>
                </span>
              )}
            </Banner>
            <div className="max-h-80 overflow-auto rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="pl-4">Fichier</TableHead>
                    <TableHead>SHA-256</TableHead>
                    <TableHead className="pr-4 text-right">Taille</TableHead>
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
        <h2 className="text-base font-semibold">Actions enregistrées pour cet entretien</h2>
        {chain.data && (
          <span className={chain.data.ok ? "text-ok text-sm" : "text-destructive text-sm"}>
            {chain.data.ok
              ? `Chaîne intacte : ${chain.data.n_events} événements dans la base, chaque empreinte recalculée`
              : `Chaîne rompue à l'événement ${chain.data.first_bad_seq}`}
          </span>
        )}
      </div>
      {events.isLoading ? <Skeleton className="h-64 w-full" /> : <AuditTable events={events.data ?? []} />}
    </div>
  )
}
