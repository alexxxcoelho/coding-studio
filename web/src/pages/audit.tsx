import { useQuery, useQueryClient } from "@tanstack/react-query"

import { AuditTable } from "@/components/audit-table"
import { Banner, PageTitle } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"

export function AuditPage() {
  const qc = useQueryClient()
  const events = useQuery({ queryKey: ["audit-all"], queryFn: () => api.audit() })
  const chain = useQuery({ queryKey: ["chain"], queryFn: api.verify })
  return (
    <>
      <PageTitle
        title={t("common.journal_d_audit")}
        sub={t("audit.chaque_action_est_ajoutee_jamais")}
        actions={
          <Button variant="outline" onClick={() => qc.invalidateQueries({ queryKey: ["chain"] })} disabled={chain.isFetching}>
            {t("audit.verifier_la_chaine")}
          </Button>
        }
      />
      {chain.data && (
        <Banner tone={chain.data.ok ? "ok" : "danger"} className="mb-4" title={chain.data.ok ? t("audit.chaine_intacte_n_events_evenements", { n_events: chain.data.n_events }) : t("common.chaine_rompue_a_l_evenement", { first_bad_seq: chain.data.first_bad_seq })}>
          {chain.data.ok ? undefined : t("audit.un_evenement_a_ete_modifie")}
        </Banner>
      )}
      {events.isLoading ? <Skeleton className="h-96 w-full" /> : <AuditTable events={events.data ?? []} showInterview />}
    </>
  )
}
