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
import { t } from "@/i18n"

/** The excerpt must be an exact substring of the unit text: no trimming, no case folding. */
export function isSubstring(excerpt: string, text: string): boolean {
  return excerpt.length > 0 && text.includes(excerpt)
}

// ---------------------------------------------------------------- flags

export const FLAG_HELP: Record<UnitFlag, string> = {
  get VERIFY() { return t("qualifications.controler_cette_unite_a_l") },
  get "SECOND-HAND"() { return t("qualifications.le_repondant_rapporte_ce_qu") },
  get UNFINISHED() { return t("qualifications.le_passage_s_interrompt_ou") },
  get "CO-CONSTRUCTED"() { return t("qualifications.l_unite_a_ete_construite") },
}

export const CO_CONSTRUCTED_HELP: Record<CoConstructedKind, string> = {
  get assent() { return t("qualifications.assentiment_le_repondant_accepte_ou") },
  get content() { return t("qualifications.contenu_le_repondant_et_l") },
}

/** Message for a flag selection the server would refuse, or null when it is valid. */
export function flagsProblem(flags: string[], kind: string | null): string | null {
  const unknown = flags.find((f) => !(UNIT_FLAGS as readonly string[]).includes(f))
  if (unknown) return t("qualifications.unknown_n_est_pas_un", { unknown })
  const coConstructed = flags.includes("CO-CONSTRUCTED")
  if (coConstructed && !kind) return t("qualifications.co_constructed_demande_un_type")
  if (!coConstructed && kind) return t("qualifications.le_type_ne_se_renseigne")
  if (kind && !(CO_CONSTRUCTED_KINDS as readonly string[]).includes(kind)) return t("qualifications.kind_n_est_pas_un", { kind })
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
  get unverified() { return t("qualifications.non_verifiee") },
  get verified() { return t("qualifications.verifiee") },
  get unusable() { return t("common.inutilisable") },
}

/** "Verified", "Unusable", "Unverified", with ", stale" when the check no longer matches the unit. */
export function verificationLabel(v: { state: VerificationState; stale: boolean }): string {
  return v.stale ? t("qualifications.v_perimee", { v: STATE_LABEL[v.state] }) : STATE_LABEL[v.state]
}

/** Colour family for the badge: ok, danger or muted. Stale checks are never shown as ok. */
export function verificationTone(v: { state: VerificationState; stale: boolean }): "ok" | "danger" | "muted" {
  if (v.stale) return "muted"
  if (v.state === "verified") return "ok"
  if (v.state === "unusable") return "danger"
  return "muted"
}

export const REASON_LABEL: Record<VerificationReason, string> = {
  get quoted() { return t("qualifications.citee") },
  get flagged() { return t("qualifications.signalee") },
  get disagreement() { return t("common.desaccord") },
  case: "cas",
  get sample() { return t("qualifications.echantillon") },
}

export const REASON_HELP: Record<VerificationReason, string> = {
  get quoted() { return t("qualifications.un_extrait_du_codage_humain") },
  get flagged() { return t("qualifications.l_unite_porte_le_drapeau") },
  get disagreement() { return t("qualifications.le_codage_humain_et_celui") },
  get case() { return t("qualifications.l_unite_est_attribuee_a") },
  get sample() { return t("qualifications.tiree_dans_un_echantillon_a") },
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
  if (!excerpt.trim()) issues.push(t("qualifications.l_extrait_est_vide"))
  else if (!isSubstring(excerpt, unitText)) issues.push(t("qualifications.l_extrait_n_est_pas"))
  const own = new Set(unitCodes)
  for (const c of codes) if (!own.has(c)) issues.push(t("qualifications.c_n_est_pas_un", { c }))
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
