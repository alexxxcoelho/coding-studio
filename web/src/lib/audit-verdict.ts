// The final audit (guide, "Faire l'audit final de trois affirmations"): for
// each claim drawn, the researcher follows the chain back to the passages and
// gives a verdict. "Étayée" is not available while a link of the chain is
// missing ("corrigez-le avant de conclure"); a claim to nuance or to withdraw
// needs a note saying what to change.

import type { AuditClaim, AuditVerdict } from "./analysis-api"
import { t } from "@/i18n"

export const VERDICT_LABEL: Record<AuditVerdict, string> = {
  get supported() { return t("audit_verdict.etayee") },
  get to_nuance() { return t("audit_verdict.a_nuancer") },
  get to_withdraw() { return t("audit_verdict.a_retirer") },
}

export interface VerdictCheck {
  ok: boolean
  reason?: string
}

export function verdictCheck(claim: Pick<AuditClaim, "gaps">, verdict: AuditVerdict, note: string): VerdictCheck {
  if (verdict === "supported") {
    return claim.gaps.length > 0
      ? { ok: false, reason: t("audit_verdict.un_maillon_manque_dans_le") }
      : { ok: true }
  }
  return note.trim() ? { ok: true } : { ok: false, reason: t("audit_verdict.une_note_est_necessaire_dites") }
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
