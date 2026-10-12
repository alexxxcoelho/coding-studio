import type { QueryClient } from "@tanstack/react-query"

/** After any write on an interview: refresh everything about it (units, flags,
 * claims, corrections, verification, and the saturation table, which follows
 * phase 2 coding), the library table and the audit chain badge. */
export function invalidateInterview(qc: QueryClient, id: string): Promise<void> {
  return qc.invalidateQueries({
    predicate: (q) => {
      const k = q.queryKey
      return k[1] === id || k[0] === "interviews" || k[0] === "chain" || k[0] === "audit-all" || k[0] === "saturation"
    },
  })
}
