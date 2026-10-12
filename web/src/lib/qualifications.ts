// Qualifications of units: flags, corrections, verification and claims.
// Pure helpers only; the components render what these return. The rules
// mirror the server (docs/DEVGRAPH_lot2_2026-10-12.md, C3 and C4), so a
// refusal here is a hint, and the server stays the authority.

import { EV_LEVELS, PROVENANCES } from "./codes"
import {
  CLAIM_ROLES,
  CO_CONSTRUCTED_KINDS,
  UNIT_FLAGS,
  type Claim,
  type ClaimInput,
  type CoConstructedKind,
  type UnitFlag,
  type VerificationCoverage,
  type VerificationReason,
  type VerificationState,
  type VerificationStatus,
  type VerificationTarget,
} from "./types"

/** The excerpt must be an exact substring of the unit text: no trimming, no case folding. */
export function isSubstring(excerpt: string, text: string): boolean {
  return excerpt.length > 0 && text.includes(excerpt)
}

// ---------------------------------------------------------------- flags

export const FLAG_HELP: Record<UnitFlag, string> = {
  VERIFY: "Check this unit against the audio before it is used in the write-up.",
  "SECOND-HAND": "The respondent reports what someone else did or said.",
  UNFINISHED: "The passage breaks off or the respondent did not finish the thought.",
  "CO-CONSTRUCTED": "The unit was built with the interviewer: say whether it is assent to a proposal or new content.",
}

export const CO_CONSTRUCTED_HELP: Record<CoConstructedKind, string> = {
  assent: "Assent: the respondent agrees to or confirms the interviewer's proposal.",
  content: "Content: the respondent and the interviewer produce the substance together.",
}

/** Message for a flag selection the server would refuse, or null when it is valid. */
export function flagsProblem(flags: string[], kind: string | null): string | null {
  const unknown = flags.find((f) => !(UNIT_FLAGS as readonly string[]).includes(f))
  if (unknown) return `${unknown} is not a flag.`
  const coConstructed = flags.includes("CO-CONSTRUCTED")
  if (coConstructed && !kind) return "CO-CONSTRUCTED needs a kind: assent or content."
  if (!coConstructed && kind) return "The kind is set only with CO-CONSTRUCTED."
  if (kind && !(CO_CONSTRUCTED_KINDS as readonly string[]).includes(kind)) return `${kind} is not a kind of co-construction.`
  return null
}

/** Flags in the closed-list order, without duplicates; the kind only with CO-CONSTRUCTED. */
export function normalizeFlags(flags: string[], kind: string | null): { flags: string[]; co_constructed_kind: CoConstructedKind | null } {
  const set = new Set(flags)
  const ordered = UNIT_FLAGS.filter((f) => set.has(f))
  const co = set.has("CO-CONSTRUCTED")
  return { flags: ordered, co_constructed_kind: co && kind ? (kind as CoConstructedKind) : null }
}

// --------------------------------------------------------- verification

const STATE_LABEL: Record<VerificationState, string> = {
  unverified: "Unverified",
  verified: "Verified",
  unusable: "Unusable",
}

/** "Verified", "Unusable", "Unverified", with ", stale" when the check no longer matches the unit. */
export function verificationLabel(v: { state: VerificationState; stale: boolean }): string {
  return v.stale ? `${STATE_LABEL[v.state]}, stale` : STATE_LABEL[v.state]
}

/** Colour family for the badge: ok, danger or muted. Stale checks are never shown as ok. */
export function verificationTone(v: { state: VerificationState; stale: boolean }): "ok" | "danger" | "muted" {
  if (v.stale) return "muted"
  if (v.state === "verified") return "ok"
  if (v.state === "unusable") return "danger"
  return "muted"
}

export const REASON_HELP: Record<VerificationReason, string> = {
  quoted: "An excerpt of the human coding or a claim quotes this unit.",
  flagged: "The unit carries the VERIFY flag.",
  disagreement: "Human and AI codings disagree on the unit in the current review.",
  case: "The unit is attributed to a case.",
  sample: "Drawn by a seeded sample of the other units: at least 5, or 10 percent.",
}

export interface TargetRow {
  unit_id: string
  reasons: VerificationReason[]
  state: VerificationState
  stale: boolean
  at: string | null
  by: string | null
  note: string | null
}

/** Targets joined with their current state; a target without a status is unverified. */
export function targetRows(targets: VerificationTarget[], status: VerificationStatus[]): TargetRow[] {
  const byUnit = new Map(status.map((s) => [s.unit_id, s]))
  return targets
    .map((t) => {
      const s = byUnit.get(t.unit_id)
      return {
        unit_id: t.unit_id,
        reasons: t.reasons,
        state: s?.state ?? "unverified",
        stale: s?.stale ?? false,
        at: s?.at ?? null,
        by: s?.by ?? null,
        note: s?.note ?? null,
      }
    })
    .sort((a, b) => a.unit_id.localeCompare(b.unit_id, undefined, { numeric: true }))
}

/** Share of targets verified, 0 to 1; null when there is no target. */
export function coverageShare(cov: VerificationCoverage): number | null {
  return cov.n_targets === 0 ? null : cov.n_targets_verified / cov.n_targets
}

// ---------------------------------------------------------------- claims

/** Reasons a claim cannot stand on a unit: empty or moved excerpt, codes no longer on the unit. Empty when valid. */
export function claimIssues(excerpt: string, codes: string[], unitText: string, unitCodes: string[]): string[] {
  const issues: string[] = []
  if (!excerpt.trim()) issues.push("The excerpt is empty.")
  else if (!isSubstring(excerpt, unitText)) issues.push("The excerpt is not a word-for-word substring of the unit text.")
  const own = new Set(unitCodes)
  for (const c of codes) if (!own.has(c)) issues.push(`${c} is not a code of this unit.`)
  return issues
}

export interface ClaimDraft {
  excerpt: string
  codes: string[]
  ev_level: string | null
  provenance: string | null
  role: string | null
  memo: string
}

export function emptyClaimDraft(): ClaimDraft {
  return { excerpt: "", codes: [], ev_level: null, provenance: null, role: null, memo: "" }
}

export function draftOf(c: Claim): ClaimDraft {
  return {
    excerpt: c.excerpt,
    codes: [...c.codes],
    ev_level: c.ev_level,
    provenance: c.provenance,
    role: c.role,
    memo: c.memo ?? "",
  }
}

/** Body sent to the server: codes sorted and unique, empty memo as null, excerpt untouched. */
export function claimPayload(d: ClaimDraft, unitId: string): ClaimInput {
  return {
    unit_id: unitId,
    excerpt: d.excerpt,
    codes: [...new Set(d.codes)].sort(),
    ev_level: d.ev_level,
    provenance: d.provenance,
    role: d.role,
    memo: d.memo.trim() || null,
  }
}

/** Closed lists offered by the claim form, from the server's lists. */
export const CLAIM_CHOICES = {
  ev: EV_LEVELS as readonly string[],
  provenance: PROVENANCES as readonly string[],
  role: CLAIM_ROLES as readonly string[],
}
