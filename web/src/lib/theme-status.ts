// Rules of the status of a theme (guide, "Régler le statut et passer les
// contrôles"): candidate, revised, retained. The checks are an aid to the
// decision, not a lock: the tool never sets "retained" on its own, and the
// researcher can retain a theme with checks left if they write why.

import type { ThemeStatus } from "./analysis-api"
import { t } from "@/i18n"

export const STATUS_LABEL: Record<ThemeStatus, string> = {
  get candidate() { return t("theme_status.candidat") },
  get revised() { return t("theme_status.revise") },
  get retained() { return t("theme_status.retenu") },
}

export interface CheckLike {
  check_id: string
  done: boolean
}

export interface StatusPlan {
  /** True when the change can be sent as it stands. */
  allowed: boolean
  /** True when the target is "retained" and a check is not done: a reason is then required. */
  needsOverride: boolean
  /** Ids of the checks not done (only filled for the move to retained). */
  pending: string[]
  reason?: "same" | "override_required"
}

export function checksProgress(checks: CheckLike[]): { done: number; total: number; complete: boolean } {
  const done = checks.filter((c) => c.done).length
  return { done, total: checks.length, complete: done === checks.length }
}

export function planStatusChange(opts: { from: ThemeStatus; to: ThemeStatus; checks: CheckLike[]; overrideReason?: string }): StatusPlan {
  const { from, to, checks, overrideReason } = opts
  if (from === to) return { allowed: false, needsOverride: false, pending: [], reason: "same" }
  if (to !== "retained") return { allowed: true, needsOverride: false, pending: [] }
  const pending = checks.filter((c) => !c.done).map((c) => c.check_id)
  if (pending.length === 0) return { allowed: true, needsOverride: false, pending }
  const written = (overrideReason ?? "").trim().length > 0
  return written ? { allowed: true, needsOverride: true, pending } : { allowed: false, needsOverride: true, pending, reason: "override_required" }
}
