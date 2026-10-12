import { describe, expect, it } from "vitest"

import {
  claimIssues,
  claimPayload,
  coverageShare,
  draftOf,
  flagsProblem,
  isSubstring,
  normalizeFlags,
  targetRows,
  verificationLabel,
  verificationTone,
} from "./qualifications"
import type { Claim, VerificationStatus, VerificationTarget } from "./types"

describe("isSubstring", () => {
  const text = "On a dit: on verra demain."
  it("accepts exact substrings only", () => {
    expect(isSubstring("on verra", text)).toBe(true)
    expect(isSubstring("On verra", text)).toBe(false)
    expect(isSubstring("", text)).toBe(false)
    expect(isSubstring("verra  demain", text)).toBe(false)
    expect(isSubstring(" on verra", text)).toBe(true)
  })
})

describe("flags", () => {
  it("requires a kind with CO-CONSTRUCTED and refuses it otherwise", () => {
    expect(flagsProblem(["CO-CONSTRUCTED"], null)).toMatch(/needs a kind/)
    expect(flagsProblem(["CO-CONSTRUCTED"], "assent")).toBeNull()
    expect(flagsProblem(["VERIFY"], "content")).toMatch(/only with CO-CONSTRUCTED/)
    expect(flagsProblem(["VERIFY"], null)).toBeNull()
    expect(flagsProblem(["NOPE"], null)).toMatch(/not a flag/)
    expect(flagsProblem(["CO-CONSTRUCTED"], "other")).toMatch(/not a kind/)
  })
  it("normalises flags to the closed-list order and drops the kind without the flag", () => {
    expect(normalizeFlags(["UNFINISHED", "VERIFY", "VERIFY"], "content")).toEqual({
      flags: ["VERIFY", "UNFINISHED"],
      co_constructed_kind: null,
    })
    expect(normalizeFlags(["CO-CONSTRUCTED", "VERIFY"], "assent")).toEqual({
      flags: ["VERIFY", "CO-CONSTRUCTED"],
      co_constructed_kind: "assent",
    })
  })
})

describe("verification", () => {
  it("labels the state and marks a stale check", () => {
    expect(verificationLabel({ state: "verified", stale: false })).toBe("Verified")
    expect(verificationLabel({ state: "unverified", stale: true })).toBe("Unverified, stale")
    expect(verificationLabel({ state: "unusable", stale: false })).toBe("Unusable")
  })
  it("never shows a stale check as ok", () => {
    expect(verificationTone({ state: "verified", stale: false })).toBe("ok")
    expect(verificationTone({ state: "verified", stale: true })).toBe("muted")
    expect(verificationTone({ state: "unusable", stale: false })).toBe("danger")
  })
  it("joins targets with their state, unverified when no status, sorted naturally", () => {
    const targets: VerificationTarget[] = [
      { unit_id: "ECO-07-U010", reasons: ["sample"] },
      { unit_id: "ECO-07-U002", reasons: ["quoted", "flagged"] },
    ]
    const status: VerificationStatus[] = [{ unit_id: "ECO-07-U002", state: "verified", stale: false, at: "t", by: "a", note: "ok" }]
    const rows = targetRows(targets, status)
    expect(rows.map((r) => [r.unit_id, r.state, r.note])).toEqual([
      ["ECO-07-U002", "verified", "ok"],
      ["ECO-07-U010", "unverified", null],
    ])
  })
  it("reports the share of targets verified, null without targets", () => {
    const cov = { n_units: 40, n_verified: 3, n_unusable: 0, n_stale: 0, n_targets: 8, n_targets_verified: 2 }
    expect(coverageShare(cov)).toBe(0.25)
    expect(coverageShare({ ...cov, n_targets: 0, n_targets_verified: 0 })).toBeNull()
  })
})

describe("claims", () => {
  const unitText = "Le dossier est parti chez l'avocat en mars."
  it("accepts an excerpt that is in the unit and codes that are on it", () => {
    expect(claimIssues("chez l'avocat", ["LEGAL"], unitText, ["LEGAL", "TIME"])).toEqual([])
  })
  it("names each problem: empty excerpt, moved text, foreign code", () => {
    expect(claimIssues("  ", [], unitText, [])).toEqual(["The excerpt is empty."])
    expect(claimIssues("chez le notaire", [], unitText, [])[0]).toMatch(/not a word-for-word/)
    expect(claimIssues("chez l'avocat", ["COST"], unitText, ["LEGAL"])).toEqual(["COST is not a code of this unit."])
  })
  it("builds a payload with sorted unique codes and a null memo when blank", () => {
    const payload = claimPayload({ ...draftOf(sample()), codes: ["TIME", "LEGAL", "LEGAL"], memo: "   " }, "ECO-07-U002")
    expect(payload).toEqual({
      unit_id: "ECO-07-U002",
      excerpt: "chez l'avocat",
      codes: ["LEGAL", "TIME"],
      ev_level: "EV2",
      provenance: "SPONTANEOUS",
      role: "trigger",
      memo: null,
    })
  })
})

function sample(): Claim {
  return {
    claim_id: 1,
    interview_id: "ECO-07",
    unit_id: "ECO-07-U002",
    excerpt: "chez l'avocat",
    codes: ["LEGAL"],
    ev_level: "EV2",
    provenance: "SPONTANEOUS",
    role: "trigger",
    memo: null,
    created_at: "",
    updated_at: "",
  }
}
