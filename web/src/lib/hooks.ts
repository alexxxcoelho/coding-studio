// TanStack Query hooks of the V1 screens, and the coding actions with their
// optimistic updates and their undo.

import { useCallback, useRef } from "react"
import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query"

import { api } from "./api"
import { LATER_ID } from "./code-tree"
import type { CoderUnit } from "./coder-model"
import { hasCode, findSpan, withCoding, withoutCoding, withoutPassage, withServerPassage, type Target } from "./passage-cache"
import { reportError } from "./notify"
import type { UndoStack } from "./undo"
import type { CodeNode, Passage, UnitRow } from "./types"

export const keys = {
  codes: ["codes"] as const,
  projectConfig: ["project-config"] as const,
  presets: ["project-presets"] as const,
  units: (id: string) => ["units", id] as const,
  passages: (id: string) => ["passages", id] as const,
  highlights: (id: string) => ["highlights", id] as const,
  memo: (id: string) => ["memo", id] as const,
  checklist: (id: string) => ["checklist", id] as const,
  attributes: (id: string) => ["attributes", id] as const,
  libraryProgress: ["library-progress"] as const,
  marks: (id: string) => ["marks", id] as const,
  /** Review state of an interview, and of the corpus (the library pipeline). */
  reviewState: (id: string) => ["review-state", id] as const,
  corpusReviewState: ["corpus-review-state"] as const,
  /** Harmonisation worklist of an interview, and of the project. */
  harmonisation: (id: string) => ["harmonisation", id] as const,
  /** The pseudonymisation list of the project (the switch, the entries, the digest). */
  projectPseudonymisation: ["project-pseudonymisation"] as const,
}

export const useCodes = () => useQuery({ queryKey: keys.codes, queryFn: api.codes })
export const useProjectConfig = () => useQuery({ queryKey: keys.projectConfig, queryFn: api.projectConfig })
export const useUnits = (id: string) => useQuery({ queryKey: keys.units(id), queryFn: () => api.units(id, 1) })
export const usePassages = (id: string) => useQuery({ queryKey: keys.passages(id), queryFn: () => api.passages(id) })
export const useMemoView = (id: string, enabled = true) => useQuery({ queryKey: keys.memo(id), queryFn: () => api.memo(id), enabled })
export const useChecklist = (id: string, enabled = true) => useQuery({ queryKey: keys.checklist(id), queryFn: () => api.checklist(id), enabled })
export const useAttributes = (id: string, enabled = true) => useQuery({ queryKey: keys.attributes(id), queryFn: () => api.attributes(id), enabled })

/** Everything that depends on the passages of an interview, and on the counts of the codes. */
export function invalidatePassages(qc: QueryClient, id: string): Promise<unknown> {
  return Promise.all([
    qc.invalidateQueries({ queryKey: keys.passages(id) }),
    qc.invalidateQueries({ queryKey: keys.highlights(id) }),
    qc.invalidateQueries({ queryKey: keys.codes }),
    qc.invalidateQueries({ queryKey: ["interviews"] }),
    qc.invalidateQueries({ queryKey: ["chain"] }),
    qc.invalidateQueries({ queryKey: ["later"] }),
    qc.invalidateQueries({ queryKey: keys.libraryProgress }),
    qc.invalidateQueries({ queryKey: keys.marks(id) }),
    qc.invalidateQueries({ queryKey: keys.reviewState(id) }),
    qc.invalidateQueries({ queryKey: keys.corpusReviewState }),
  ])
}

/** After a change of the codebook (merge, redefinition, new sub-code): the worklist of units to reread and the review state follow. */
export function invalidateHarmonisation(qc: QueryClient): Promise<unknown> {
  return Promise.all([
    qc.invalidateQueries({ queryKey: ["harmonisation"] }),
    qc.invalidateQueries({ queryKey: ["review-state"] }),
    qc.invalidateQueries({ queryKey: keys.corpusReviewState }),
  ])
}

export interface CodingActions {
  /** Toggles `code` on the target span: adds it, or removes it when it is already there. */
  toggleCode: (unit: CoderUnit, range: { start: number; end: number } | null, code: CodeNode) => Promise<void>
  /** Marks the selected span as a highlight (no code). */
  highlight: (unit: CoderUnit, range: { start: number; end: number }) => Promise<Passage | undefined>
  /** Toggles "non pertinent" on a respondent turn. */
  toggleNotRelevant: (unit: CoderUnit) => Promise<void>
  /** Toggles "à revoir" (the code Plus tard on the whole turn), or parks the selected span with a note. */
  toggleLater: (unit: CoderUnit, range: { start: number; end: number } | null, note?: string) => Promise<void>
  /** Removes a code from a passage (a chip of the Détail tab). */
  removeCode: (passage: Passage, codeId: string, codeName: string) => Promise<void>
  /** Deletes a passage (logical delete on the server). */
  deletePassage: (passage: Passage) => Promise<void>
}

/**
 * The coding actions of one interview. Each one updates the cached passages
 * first, calls the server, replaces the guess with the server's passage, rolls
 * back on a refusal, and leaves its reverse on the undo stack. The list is
 * refetched when the last call in flight ends, so that a quick series of keys
 * is never overwritten by an early refetch.
 */
export function useCodingActions(interviewId: string, undo: UndoStack): CodingActions {
  const qc = useQueryClient()
  const inflight = useRef(0)
  const pKey = keys.passages(interviewId)
  const uKey = keys.units(interviewId)

  const passages = useCallback(() => qc.getQueryData<Passage[]>(pKey) ?? [], [qc, pKey])

  const run = useCallback(
    async <T,>(opts: {
      patch?: (l: Passage[]) => Passage[]
      patchUnits?: (rows: UnitRow[]) => UnitRow[]
      call: () => Promise<T>
      done?: (r: T) => void
    }): Promise<T | undefined> => {
      inflight.current++
      await Promise.all([qc.cancelQueries({ queryKey: pKey }), qc.cancelQueries({ queryKey: uKey })])
      const prevP = qc.getQueryData<Passage[]>(pKey)
      const prevU = qc.getQueryData<UnitRow[]>(uKey)
      if (opts.patch) qc.setQueryData<Passage[]>(pKey, opts.patch(prevP ?? []))
      if (opts.patchUnits && prevU) qc.setQueryData<UnitRow[]>(uKey, opts.patchUnits(prevU))
      try {
        const r = await opts.call()
        opts.done?.(r)
        return r
      } catch (e) {
        if (prevP) qc.setQueryData(pKey, prevP)
        if (prevU) qc.setQueryData(uKey, prevU)
        reportError(e)
        return undefined
      } finally {
        inflight.current--
        if (inflight.current === 0) {
          await invalidatePassages(qc, interviewId)
          await qc.invalidateQueries({ queryKey: uKey })
        }
      }
    },
    [qc, pKey, uKey, interviewId],
  )

  const toggleCode = useCallback<CodingActions["toggleCode"]>(
    async (unit, range, code) => {
      const target: Target = { unitId: unit.unit_id, ...(range ?? {}) }
      const existing = findSpan(passages(), target, unit.text.length)
      if (existing && hasCode(existing, code.code_id)) {
        if (existing.passage_id.startsWith("tmp-")) return
        const pid = existing.passage_id
        const r = await run({
          patch: (l) => withoutCoding(l, pid, code.code_id),
          call: () => api.removeCoding(pid, code.code_id),
        })
        if (r) undo.push({ label: `${code.name} retiré`, run: () => api.addCoding(pid, code.code_id) })
        return
      }
      const tmpPassage = (l: Passage[]) => withCoding(l, target, unit.text, interviewId, code)
      let tmpId: string | undefined
      const r = await run({
        patch: (l) => {
          const next = tmpPassage(l)
          tmpId = next.find((p) => p.passage_id.startsWith("tmp-") && !l.includes(p))?.passage_id
          return next
        },
        call: () =>
          api.createPassage(interviewId, {
            unit_id: unit.unit_id,
            ...(range ? { start: range.start, end: range.end } : {}),
            code: code.code_id,
          }),
        done: (res) => qc.setQueryData<Passage[]>(pKey, (l) => withServerPassage(l ?? [], res.passage, tmpId)),
      })
      if (r) {
        const pid = r.passage.passage_id
        undo.push(
          r.created
            ? { label: `${code.name} posé`, run: () => api.deletePassage(pid, "annulation") }
            : { label: `${code.name} posé`, run: () => api.removeCoding(pid, code.code_id, "annulation") },
        )
      }
    },
    [passages, run, undo, interviewId, qc, pKey],
  )

  const highlight = useCallback<CodingActions["highlight"]>(
    async (unit, range) => {
      const target: Target = { unitId: unit.unit_id, start: range.start, end: range.end }
      const there = findSpan(passages(), target, unit.text.length)
      if (there) return there
      let tmpId: string | undefined
      const r = await run({
        patch: (l) => {
          const next = withCoding(l, target, unit.text, interviewId, null)
          tmpId = next.find((p) => p.passage_id.startsWith("tmp-") && !l.includes(p))?.passage_id
          return next
        },
        call: () => api.createPassage(interviewId, { unit_id: unit.unit_id, start: range.start, end: range.end }),
        done: (res) => qc.setQueryData<Passage[]>(pKey, (l) => withServerPassage(l ?? [], res.passage, tmpId)),
      })
      if (r?.created) {
        const pid = r.passage.passage_id
        undo.push({ label: "Surlignage posé", run: () => api.deletePassage(pid, "annulation") })
      }
      return r?.passage
    },
    [passages, run, undo, interviewId, qc, pKey],
  )

  const toggleNotRelevant = useCallback<CodingActions["toggleNotRelevant"]>(
    async (unit) => {
      if (unit.role === "interviewer") return
      const relevant = unit.notRelevant
      const setNA = (rows: UnitRow[], na: boolean) =>
        rows.map((row) =>
          row.unit.unit_id === unit.unit_id
            ? { ...row, coding: { ...(row.coding ?? { unit_id: unit.unit_id, subcode: null, modifier: null, ev_level: null, provenance: null, memo: null, excerpt: null }), codes: na ? ["NA"] : ["CONTEXT"] } }
            : row,
        )
      const r = await run({
        patchUnits: (rows) => setNA(rows, !relevant),
        call: () => api.setRelevant(interviewId, unit.unit_id, relevant),
      })
      if (r) {
        undo.push({
          label: relevant ? "Tour remis en jeu" : "Tour marqué non pertinent",
          run: () => api.setRelevant(interviewId, unit.unit_id, !relevant),
        })
      }
    },
    [run, undo, interviewId],
  )

  const toggleLater = useCallback<CodingActions["toggleLater"]>(
    async (unit, range, note) => {
      if (unit.role === "interviewer") return
      const later = { code_id: LATER_ID, name: "Plus tard", system: true }
      const target: Target = { unitId: unit.unit_id, ...(range ?? {}) }
      const existing = findSpan(passages(), target, unit.text.length)
      if (existing && hasCode(existing, LATER_ID) && !note) {
        if (existing.passage_id.startsWith("tmp-")) return
        const pid = existing.passage_id
        const only = existing.codings.length === 1
        const r = await run({
          patch: (l) => (only ? withoutPassage(l, pid) : withoutCoding(l, pid, LATER_ID)),
          call: () => (only ? api.deletePassage(pid, "annulation") : api.removeCoding(pid, LATER_ID)).then(() => true),
        })
        if (r) undo.push({ label: "À revoir retiré", run: () => api.addCoding(pid, LATER_ID) })
        return
      }
      let tmpId: string | undefined
      const r = await run({
        patch: (l) => {
          const next = withCoding(l, target, unit.text, interviewId, later, note ?? "")
          tmpId = next.find((p) => p.passage_id.startsWith("tmp-") && !l.includes(p))?.passage_id
          return next
        },
        call: () =>
          api.createPassage(interviewId, {
            unit_id: unit.unit_id,
            ...(range ? { start: range.start, end: range.end } : {}),
            code: LATER_ID,
            ...(note ? { note } : {}),
          }),
        done: (res) => qc.setQueryData<Passage[]>(pKey, (l) => withServerPassage(l ?? [], res.passage, tmpId)),
      })
      if (r) {
        const pid = r.passage.passage_id
        undo.push(
          r.created
            ? { label: "Plus tard posé", run: () => api.deletePassage(pid, "annulation") }
            : { label: "Plus tard posé", run: () => api.removeCoding(pid, LATER_ID, "annulation") },
        )
      }
    },
    [passages, run, undo, interviewId, qc, pKey],
  )

  const removeCode = useCallback<CodingActions["removeCode"]>(
    async (passage, codeId, codeName) => {
      if (passage.passage_id.startsWith("tmp-")) return
      const pid = passage.passage_id
      const r = await run({ patch: (l) => withoutCoding(l, pid, codeId), call: () => api.removeCoding(pid, codeId) })
      if (r) undo.push({ label: `${codeName} retiré`, run: () => api.addCoding(pid, codeId) })
    },
    [run, undo],
  )

  const deletePassage = useCallback<CodingActions["deletePassage"]>(
    async (passage) => {
      if (passage.passage_id.startsWith("tmp-")) return
      const pid = passage.passage_id
      await run({ patch: (l) => withoutPassage(l, pid), call: () => api.deletePassage(pid) })
    },
    [run],
  )

  return { toggleCode, highlight, toggleNotRelevant, toggleLater, removeCode, deletePassage }
}
