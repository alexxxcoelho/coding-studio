import { describe, expect, it } from "vitest"
import { codingProgress, examined, matchesInterview, nextAction, pipelineSteps, stageOf, type LibraryRow } from "./pipeline"
import type { ReviewState } from "./review-api"
import type { InterviewSummary } from "./types"

const phase = (extra: Record<string, unknown> = {}) => ({
  phase: 2,
  n_units: 0,
  n_human_coded: 0,
  human_complete: false,
  n_ai_runs: 0,
  n_ai_runs_complete: 0,
  n_divergences: 0,
  n_sampled: 0,
  n_arbitrated: 0,
  n_to_clarify: 0,
  reconciled: false,
  ...extra,
})
const summary = (id: string, n_units: number, p2: Record<string, unknown> = {}): InterviewSummary =>
  ({ interview_id: id, folder_name: `${id}_Nom`, n_sources: 1, primary_source: null, n_units, n_warnings: 0, phase1: phase({ phase: 1 }), phase2: phase(p2) }) as InterviewSummary

const row = (over: Partial<LibraryRow> = {}): LibraryRow => ({
  interview_id: "ECO-05", units: 22, respondent_units: 10, relevant: 8, not_relevant: 2, coded: 0, units_coded: 2, units_later: 1, to_review: 3,
  memo_answered: 0, memo_total: 4, review: { state: "none", disagreements: 0, arbitrated: 0, sample: 0, sample_reviewed: 0 }, arbitrated: true, done: false, ...over,
})

describe("codingProgress", () => {
  it("reads the row of the library progress: the units the Coder shows as coded or parked, the others not relevant", () => {
    expect(codingProgress(row())).toEqual({ respondent: 10, nonPertinent: 2, coded: 2, later: 1, toReview: 3 })
  })
  it("accepts progress payloads from before passage counters without displaying NaN", () => {
    expect(codingProgress(row({ units_coded: undefined, units_later: undefined, coded: 8 } as unknown as Partial<LibraryRow>))).toEqual({ respondent: 10, nonPertinent: 2, coded: 8, later: 0, toReview: 3 })
  })
  it("examined adds coded, parked and not relevant", () => {
    expect(examined({ respondent: 10, nonPertinent: 2, coded: 2, later: 1, toReview: 0 })).toBe(5)
  })
})

describe("pipelineSteps", () => {
  it("shows an unsegmented interview with empty bars and the optional AI step dashed", () => {
    const steps = pipelineSteps(summary("ECO-05", 0), null)
    expect(steps.map((s) => s.label)).toEqual(["Segmentation", "Codage", "IA facultative", "Arbitrage"])
    expect(steps.map((s) => s.value)).toEqual([0, 0, 0, 0])
    expect(steps[2].optional).toBe(true)
  })
  it("does not invent a 0/0 count while progress is loading", () => {
    expect(pipelineSteps(summary("A", 10), null)[1]).toMatchObject({ label: "Codage", value: 0 })
  })
  it("counts sampled agreements alongside disagreements in a legacy summary", () => {
    const steps = pipelineSteps(summary("A", 10, { n_divergences: 10, n_sampled: 6, n_arbitrated: 16 }), null)
    expect(steps[3]).toMatchObject({ label: "Arbitré 16/16", value: 1 })
  })
  it("shows coding in progress", () => {
    const steps = pipelineSteps(summary("RET-22", 132), { respondent: 97, nonPertinent: 0, coded: 40, later: 0, toReview: 0 })
    expect(steps[0]).toMatchObject({ label: "Segmenté", value: 1 })
    expect(steps[1].label).toBe("Codage 40/97")
    expect(steps[1].value).toBeCloseTo(40 / 97)
  })
  it("shows a finished coding, the AI passes and the arbitration", () => {
    const steps = pipelineSteps(summary("ECO-16", 84, { n_ai_runs_complete: 1, n_divergences: 12, n_arbitrated: 9 }), {
      respondent: 61,
      nonPertinent: 5,
      coded: 56,
      later: 0,
      toReview: 0,
    })
    expect(steps.map((s) => s.label)).toEqual(["Segmenté", "Codé 61/61", "IA · 1 passe", "Arbitré 9/12"])
    expect(steps[3].value).toBe(0.75)
    expect(steps[2].optional).toBe(false)
  })
})

describe("stage and next action", () => {
  const full = { respondent: 61, nonPertinent: 5, coded: 56, later: 0, toReview: 0 }
  it("classifies", () => {
    expect(stageOf(summary("A", 0), null)).toBe("new")
    expect(stageOf(summary("A", 10), { respondent: 5, nonPertinent: 0, coded: 2, later: 0, toReview: 0 })).toBe("in_progress")
    expect(stageOf(summary("A", 10), full)).toBe("ready")
    expect(stageOf(summary("A", 10, { n_divergences: 3, n_arbitrated: 1 }), full)).toBe("in_progress")
    expect(stageOf(summary("A", 10), null)).toBe("in_progress")
    expect(stageOf(summary("A", 10), { ...full, coded: 55, later: 1 })).toBe("in_progress")
    expect(stageOf(summary("A", 10), { ...full, toReview: 1 })).toBe("in_progress")
    expect(nextAction(summary("A", 10), { ...full, toReview: 1 })).toMatchObject({ label: "Reprendre le codage" })
  })
  it("proposes the next step", () => {
    expect(nextAction(summary("A", 0), null)).toEqual({ kind: "segment", label: "Segmenter…" })
    expect(nextAction(summary("A", 9), { respondent: 5, nonPertinent: 0, coded: 0, later: 0, toReview: 0 })).toMatchObject({ label: "Commencer le codage", href: "#/interviews/A/coding" })
    expect(nextAction(summary("A", 9), { respondent: 5, nonPertinent: 0, coded: 2, later: 0, toReview: 0 })).toMatchObject({ label: "Reprendre le codage" })
    expect(nextAction(summary("A", 9, { n_divergences: 3, n_arbitrated: 0 }), { respondent: 5, nonPertinent: 0, coded: 5, later: 0, toReview: 0 })).toMatchObject({
      label: "Arbitrer les 3 restants",
      href: "#/interviews/A/review",
    })
    expect(nextAction(summary("A", 9, { n_divergences: 3, n_arbitrated: 2 }), { respondent: 5, nonPertinent: 0, coded: 5, later: 0, toReview: 0 })).toMatchObject({ label: "Arbitrer le dernier" })
    expect(nextAction(summary("A", 9), { respondent: 5, nonPertinent: 0, coded: 5, later: 0, toReview: 0 })).toMatchObject({ label: "Lancer la revue IA" })
  })
  it("searches by id and folder", () => {
    expect(matchesInterview(summary("ECO-16", 1), "eco")).toBe(true)
    expect(matchesInterview(summary("ECO-16", 1), "nom")).toBe(true)
    expect(matchesInterview(summary("ECO-16", 1), "ret")).toBe(false)
    expect(matchesInterview(summary("ECO-16", 1), " ")).toBe(true)
  })
})

describe("review state in the pipeline", () => {
  const rs = (over: Partial<ReviewState> = {}): ReviewState => ({
    interview_id: "ECO-05", state: "not_done", n_relevant: 10, n_covered: 0, coverage: 0, runs: [], n_batches: 0, n_disagreements: 0, n_decided: 0,
    n_sample: 0, n_sample_reviewed: 0, n_candidates: 0, n_candidates_decided: 0, harmonisation_open: 0, harmonisation_done: 0, declaration: "", ...over,
  })
  const s = summary("ECO-05", 22)
  const p = codingProgress(row({ units_coded: 8, not_relevant: 2, units_later: 0, to_review: 0 }))
  const ai = (r: ReviewState | undefined) => pipelineSteps(s, p, r).find((x) => x.key === "ai")!

  it("keeps the optional dashed step while nothing is done", () => {
    expect(ai(rs())).toMatchObject({ label: "IA facultative", value: 0, optional: true })
  })
  it("waits for the answer, then shows the coverage, then the passes", () => {
    expect(ai(rs({ state: "in_progress", n_batches: 1 }))).toMatchObject({ label: "IA en attente", value: 0 })
    expect(ai(rs({ state: "in_progress", runs: ["a"], n_covered: 5, coverage: 0.5 }))).toMatchObject({ label: "IA · 50 % relus", value: 0.5 })
    expect(ai(rs({ state: "complete", runs: ["a", "b"], n_covered: 10, coverage: 1 }))).toMatchObject({ label: "IA · 2 passes", value: 1 })
  })
  it("does not let decided samples hide an undecided disagreement", () => {
    const legacy = summary("A", 22, { n_divergences: 2, n_sampled: 2, n_arbitrated: 2 })
    const review = rs({ n_disagreements: 2, n_decided: 0, n_sample: 2, n_sample_reviewed: 2 })
    expect(pipelineSteps(legacy, p, review)[3]).toMatchObject({ label: "Arbitré 2/4", value: 0.5 })
    expect(stageOf(legacy, p, review)).toBe("in_progress")
    expect(nextAction(legacy, p, review)).toMatchObject({ href: "#/interviews/A/review" })
  })
  it("keeps undecided candidate concepts and sampled agreements open", () => {
    expect(stageOf(s, p, rs({ n_candidates: 1 }))).toBe("in_progress")
    expect(stageOf(s, p, rs({ n_sample: 1 }))).toBe("in_progress")
    expect(nextAction(s, p, rs({ n_candidates: 1 }))).toMatchObject({ href: "#/interviews/ECO-05/review" })
    expect(stageOf(s, p, rs({ state: "in_progress", n_covered: 5, coverage: 0.5 }))).toBe("ready")
  })
  it("sends to the worklist before the arbitration, and keeps the interview in progress", () => {
    expect(nextAction(s, p, rs({ harmonisation_open: 3 }))).toEqual({ kind: "link", label: "Harmoniser 3 unités", href: "#/interviews/ECO-05/harmonise" })
    expect(stageOf(s, p, rs({ harmonisation_open: 3 }))).toBe("in_progress")
    expect(stageOf(s, p, rs())).toBe("ready")
  })
})
