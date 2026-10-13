// State of the framework reading of one theme, with a patch that saves the whole.

import { useQueryClient } from "@tanstack/react-query"

import { t } from "@/i18n"
import { analysisApi, type FrameworkReading } from "@/lib/analysis-api"
import { akeys, useFrameworkReading } from "@/lib/analysis-hooks"
import { reportError } from "@/lib/notify"

export type FrameworkBody = Omit<FrameworkReading, "theme_id">

const EMPTY: FrameworkBody = { cells: {}, out_of_frame: false, out_of_frame_note: "", complementary_dimension: "", evidence_limit: "" }

/** Reads the reading of a theme and returns a function that patches it and saves the whole. */
export function useFrameworkEditor(themeId: string) {
  const qc = useQueryClient()
  const q = useFrameworkReading(themeId)
  const reading: FrameworkBody = q.data ?? EMPTY
  const patch = async (change: (r: FrameworkBody) => FrameworkBody): Promise<void> => {
    // Built on the latest copy held by the cache, so that two quick edits do not overwrite each other.
    const current = qc.getQueryData<FrameworkReading>(akeys.framework(themeId)) ?? { theme_id: themeId, ...EMPTY }
    const next = change({ ...current })
    try {
      const saved = await analysisApi.setFrameworkReading(themeId, {
        cells: next.cells,
        out_of_frame: next.out_of_frame,
        out_of_frame_note: next.out_of_frame_note,
        complementary_dimension: next.complementary_dimension,
        evidence_limit: next.evidence_limit,
      })
      qc.setQueryData(akeys.framework(themeId), saved)
      void qc.invalidateQueries({ queryKey: akeys.frameworkTable })
    } catch (e) {
      reportError(e, t("analysis.use_framework_editor.lecture_par_le_cadre_non"))
      throw e
    }
  }
  return { reading, patch, loading: q.isLoading }
}
