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
        title="Audit trail"
        sub="Every action, append only. Each event carries the SHA-256 of the previous one, so any later edit breaks the chain."
        actions={
          <Button variant="outline" onClick={() => qc.invalidateQueries({ queryKey: ["chain"] })} disabled={chain.isFetching}>
            Verify the chain again
          </Button>
        }
      />
      {chain.data && (
        <Banner tone={chain.data.ok ? "ok" : "danger"} className="mb-4" title={chain.data.ok ? `Chain intact: ${chain.data.n_events} events, every hash recomputed and matching` : `Chain broken at event ${chain.data.first_bad_seq}`}>
          {chain.data.ok ? undefined : "An event was changed after it was written. The events from that point on can no longer be trusted."}
        </Banner>
      )}
      {events.isLoading ? <Skeleton className="h-96 w-full" /> : <AuditTable events={events.data ?? []} showInterview />}
    </>
  )
}
