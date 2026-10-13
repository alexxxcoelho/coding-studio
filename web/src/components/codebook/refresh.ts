// What a split, a decision of its worklist or an undo changes on screen.

import { useQueryClient } from "@tanstack/react-query"

import { invalidateHarmonisation, keys } from "@/lib/hooks"

export function useRefreshAfterCodeOp() {
  const qc = useQueryClient()
  return () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: keys.codes }),
      qc.invalidateQueries({ queryKey: ["codebook"] }),
      qc.invalidateQueries({ queryKey: ["code-ops"] }),
      qc.invalidateQueries({ queryKey: ["split-worklist"] }),
      qc.invalidateQueries({ queryKey: ["code-stats"] }),
      qc.invalidateQueries({ queryKey: ["code-search"] }),
      qc.invalidateQueries({ queryKey: ["passages"] }),
      qc.invalidateQueries({ queryKey: ["interviews"] }),
      qc.invalidateQueries({ queryKey: ["chain"] }),
      qc.invalidateQueries({ queryKey: ["state"] }),
      invalidateHarmonisation(qc),
    ])
}
