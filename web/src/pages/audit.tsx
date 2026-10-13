import { useQuery, useQueryClient } from "@tanstack/react-query"

import { AuditTable } from "@/components/audit-table"
import { Banner, PageTitle } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"

export function AuditPage() {
  const qc = useQueryClient()
  const events = useQuery({ queryKey: ["audit-all"], queryFn: () => api.audit() })
  const chain = useQuery({ queryKey: ["chain"], queryFn: api.verify })
  return (
    <>
      <PageTitle
        title="Journal d'audit"
        sub="Chaque action est ajoutée, jamais modifiée. Chaque événement porte l'empreinte SHA-256 du précédent : toute modification ultérieure rompt la chaîne."
        actions={
          <Button variant="outline" onClick={() => qc.invalidateQueries({ queryKey: ["chain"] })} disabled={chain.isFetching}>
            Vérifier la chaîne
          </Button>
        }
      />
      {chain.data && (
        <Banner tone={chain.data.ok ? "ok" : "danger"} className="mb-4" title={chain.data.ok ? `Chaîne intacte : ${chain.data.n_events} événements, chaque empreinte recalculée et conforme` : `Chaîne rompue à l'événement ${chain.data.first_bad_seq}`}>
          {chain.data.ok ? undefined : "Un événement a été modifié après son écriture. Les événements à partir de ce point ne sont plus fiables."}
        </Banner>
      )}
      {events.isLoading ? <Skeleton className="h-96 w-full" /> : <AuditTable events={events.data ?? []} showInterview />}
    </>
  )
}
