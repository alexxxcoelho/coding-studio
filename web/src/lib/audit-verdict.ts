// The final audit (guide, "Faire l'audit final de trois affirmations"): for
// each claim drawn, the researcher follows the chain back to the passages and
// gives a verdict. "Étayée" is not available while a link of the chain is
// missing ("corrigez-le avant de conclure"); a claim to nuance or to withdraw
// needs a note saying what to change.

import type { AuditClaim, AuditVerdict } from "./analysis-api"

export const VERDICT_LABEL: Record<AuditVerdict, string> = {
  supported: "étayée",
  to_nuance: "à nuancer",
  to_withdraw: "à retirer",
}

export interface VerdictCheck {
  ok: boolean
  reason?: string
}

export function verdictCheck(claim: Pick<AuditClaim, "gaps">, verdict: AuditVerdict, note: string): VerdictCheck {
  if (verdict === "supported") {
    return claim.gaps.length > 0
      ? { ok: false, reason: "Un maillon manque dans le chemin : corrigez-le avant de conclure que l'affirmation est étayée." }
      : { ok: true }
  }
  return note.trim() ? { ok: true } : { ok: false, reason: "Une note est nécessaire : dites ce qu'il faut nuancer ou retirer." }
}

export interface AuditProgress {
  judged: number
  total: number
  complete: boolean
  supported: number
  toNuance: number
  toWithdraw: number
}

export function auditProgress(claims: Pick<AuditClaim, "verdict">[]): AuditProgress {
  const count = (v: AuditVerdict) => claims.filter((c) => c.verdict?.verdict === v).length
  const judged = claims.filter((c) => c.verdict).length
  return {
    judged,
    total: claims.length,
    complete: claims.length > 0 && judged === claims.length,
    supported: count("supported"),
    toNuance: count("to_nuance"),
    toWithdraw: count("to_withdraw"),
  }
}
