import { describe, expect, it } from "vitest"

import type { AuditClaim } from "./analysis-api"
import { auditProgress, VERDICT_LABEL, verdictCheck } from "./audit-verdict"

const claim = (over: Partial<AuditClaim> = {}): AuditClaim => ({
  claim_key: "theme_part:T-0001:mechanism_evidence",
  part_title: "Mécanisme et preuves",
  sentence: "Le lot suffit.",
  theme: { theme_id: "T-0001", title: "Le lot suffit" },
  rq_id: null,
  codes: [],
  links: [],
  gaps: [],
  verdict: null,
  ...over,
})
const gap = { kind: "unverified_passage" as const, message: "Passage non vérifié à l'audio" }

describe("verdictCheck", () => {
  it("accepts 'étayée' for a complete chain, with or without a note", () => {
    expect(verdictCheck(claim(), "supported", "")).toEqual({ ok: true })
    expect(verdictCheck(claim(), "supported", "Net")).toEqual({ ok: true })
  })
  it("refuses 'étayée' when a link of the chain is missing", () => {
    const r = verdictCheck(claim({ gaps: [{ kind: "no_link", message: "Aucun passage lié" }] }), "supported", "")
    expect(r.ok).toBe(false)
    expect(r.reason).toMatch(/maillon/)
  })
  it("refuses 'étayée' for a passage not verified either", () => {
    expect(verdictCheck(claim({ gaps: [gap] }), "supported", "ok").ok).toBe(false)
  })
  it("requires a note for 'à nuancer' and 'à retirer'", () => {
    expect(verdictCheck(claim(), "to_nuance", "  ").ok).toBe(false)
    expect(verdictCheck(claim(), "to_withdraw", "").ok).toBe(false)
    expect(verdictCheck(claim(), "to_nuance", "Dire « plusieurs »").ok).toBe(true)
  })
  it("lets a claim with a broken chain be nuanced or withdrawn, with its note", () => {
    expect(verdictCheck(claim({ gaps: [gap] }), "to_withdraw", "Pas de preuve").ok).toBe(true)
  })
})

describe("auditProgress", () => {
  it("counts the verdicts and says when the audit is complete", () => {
    const v = (verdict: "supported" | "to_nuance") => ({ verdict, note: "", at: "t" })
    const claims = [claim({ verdict: v("supported") }), claim({ claim_key: "b", verdict: v("to_nuance") }), claim({ claim_key: "c" })]
    expect(auditProgress(claims)).toMatchObject({ judged: 2, total: 3, complete: false })
    claims[2].verdict = { verdict: "to_withdraw", note: "x", at: "t" }
    expect(auditProgress(claims)).toMatchObject({ judged: 3, total: 3, complete: true, supported: 1, toNuance: 1, toWithdraw: 1 })
  })
  it("is not complete without a claim", () => {
    expect(auditProgress([]).complete).toBe(false)
  })
})

describe("labels", () => {
  it("are the words of the guide", () => {
    expect(VERDICT_LABEL).toEqual({ supported: "étayée", to_nuance: "à nuancer", to_withdraw: "à retirer" })
  })
})
