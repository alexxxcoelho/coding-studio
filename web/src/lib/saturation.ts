// Saturation table: first use and creation only. Enrichment of meaning is not
// computed here; it lives in the memos.

import type { SaturationRow } from "./types"
import { t } from "@/i18n"

/** Rows in interview order. */
export function inOrder(rows: SaturationRow[]): SaturationRow[] {
  return [...rows].sort((a, b) => a.order - b.order)
}

/** Running count of codes first used, interview by interview: the curve to read for saturation. */
export function cumulativeCodes(rows: SaturationRow[]): number[] {
  let total = 0
  return inOrder(rows).map((r) => (total += r.codes_first_used.length))
}

export interface SaturationTotals {
  codes: number
  fieldCodes: number
  labelsOpened: number
  labelsResolved: number
}

export function saturationTotals(rows: SaturationRow[]): SaturationTotals {
  return rows.reduce<SaturationTotals>(
    (t, r) => ({
      codes: t.codes + r.codes_first_used.length,
      fieldCodes: t.fieldCodes + r.field_codes_created.length,
      labelsOpened: t.labelsOpened + r.labels_opened,
      labelsResolved: t.labelsResolved + r.labels_resolved,
    }),
    { codes: 0, fieldCodes: 0, labelsOpened: 0, labelsResolved: 0 },
  )
}

/** The interview id as shown; an empty id means no interview could be attributed. */
export function interviewLabel(id: string): string {
  return id || t("saturation.non_attribue")
}
