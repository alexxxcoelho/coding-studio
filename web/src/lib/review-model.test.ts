import { describe, expect, it } from "vitest"

import {
  availableChoices,
  candidateProblem,
  decisionKey,
  draftFromDecision,
  draftProblem,
  filterQueue,
  finalCodesOf,
  issueLines,
  newDraft,
  nextInQueue,
  precedingQuestion,
  rationaleNeeded,
  reviewStatusLabel,
  runLine,
  runState,
  toChoice,
  type DecisionDraft,
} from "./review-model"
import type { AiRun, ReviewItem, Unit } from "./types"

const unit = (seq: number, role: Unit["role"], text = "t"): Unit => ({
  unit_id: `X-U${String(seq).padStart(3, "0")}`,
  seq,
  speaker_raw: role === "interviewer" ? "INT" : "RESP",
  role,
  timestamp: "00:01",
  text,
})

const item = (over: Partial<ReviewItem> = {}): ReviewItem => ({
  unit_id: "X-U002",
  kind: "disagreement",
  human: ["A", "B"],
  human_current: ["A", "B"],
  revised_since_snapshot: false,
  ai: { R1: ["B", "C"], R2: ["C"] },
  decision: null,
  unit: unit(2, "respondent"),
  ...over,
})

const draft = (over: Partial<DecisionDraft> = {}): DecisionDraft => ({ kind: "human", runLabel: "R1", custom: [], rationale: "", ...over })

describe("final codes of a draft", () => {
  it("keeps the frozen human codes", () => {
    expect(finalCodesOf(item(), draft())).toEqual(["A", "B"])
  })
  it("takes the codes of the chosen run", () => {
    expect(finalCodesOf(item(), draft({ kind: "ai", runLabel: "R2" }))).toEqual(["C"])
  })
  it("merges the frozen human codes with those of the run, without doubles", () => {
    expect(finalCodesOf(item(), draft({ kind: "merge" })).sort()).toEqual(["A", "B", "C"])
  })
  it("takes the picked codes for another choice", () => {
    expect(finalCodesOf(item(), draft({ kind: "other", custom: ["Z"] }))).toEqual(["Z"])
  })
})

describe("choice sent to the server", () => {
  it("maps keep, take, merge and other onto the three server choices", () => {
    expect(toChoice(draft())).toEqual({ type: "human" })
    expect(toChoice(draft({ kind: "ai", runLabel: "R2" }))).toEqual({ type: "ai", run_label: "R2" })
    expect(toChoice(draft({ kind: "merge" }))).toEqual({ type: "custom" })
    expect(toChoice(draft({ kind: "other" }))).toEqual({ type: "custom" })
  })
})

describe("available choices", () => {
  it("offers four choices on a disagreement, in key order", () => {
    expect(availableChoices(item())).toEqual(["human", "ai", "merge", "other"])
  })
  it("offers confirm or correct on a sampled agreement", () => {
    expect(availableChoices(item({ kind: "agreement_sample" }))).toEqual(["human", "other"])
  })
})

describe("rationale requirement", () => {
  it("is required on every disagreement, even to keep the human reading", () => {
    expect(rationaleNeeded(item(), draft())).toBe(true)
  })
  it("is optional to confirm a sampled agreement", () => {
    expect(rationaleNeeded(item({ kind: "agreement_sample" }), draft())).toBe(false)
  })
  it("is required to correct a sampled agreement, because it diverges from the human", () => {
    expect(rationaleNeeded(item({ kind: "agreement_sample" }), draft({ kind: "other", custom: ["Z"] }))).toBe(true)
  })
  it("is not required when the picked codes equal the human ones on a sample", () => {
    expect(rationaleNeeded(item({ kind: "agreement_sample" }), draft({ kind: "other", custom: ["B", "A"] }))).toBe(false)
  })
})

describe("draft problems", () => {
  it("refuses a blank rationale on a disagreement", () => {
    expect(draftProblem(item(), draft({ rationale: "   " }))).toBe("rationale")
  })
  it("accepts a written rationale", () => {
    expect(draftProblem(item(), draft({ rationale: "le répondant décrit un choix" }))).toBeNull()
  })
  it("refuses an empty set of final codes before the rationale", () => {
    expect(draftProblem(item(), draft({ kind: "other", custom: [] }))).toBe("no_codes")
  })
  it("needs no rationale to confirm a sample", () => {
    expect(draftProblem(item({ kind: "agreement_sample" }), draft())).toBeNull()
  })
})

describe("drafts", () => {
  it("starts on keeping the human reading, with the first run", () => {
    expect(newDraft(item(), ["R1", "R2"])).toEqual({ kind: "human", runLabel: "R1", custom: ["A", "B"], rationale: "" })
  })
  it("restores a stored decision for editing", () => {
    const decided = item({
      decision: { unit_id: "X-U002", phase: 2, choice: { type: "ai", run_label: "R2" }, final_codes: ["C"], rationale: "r", decided_by: "me", decided_at: "" },
    })
    expect(draftFromDecision(decided, ["R1", "R2"])).toMatchObject({ kind: "ai", runLabel: "R2", rationale: "r" })
  })
  it("restores a custom decision that equals the merge as a merge", () => {
    const decided = item({
      decision: { unit_id: "X-U002", phase: 2, choice: { type: "custom" }, final_codes: ["C", "B", "A"], rationale: "r", decided_by: "me", decided_at: "" },
    })
    expect(draftFromDecision(decided, ["R1"])?.kind).toBe("merge")
  })
  it("restores another custom decision with its codes", () => {
    const decided = item({
      decision: { unit_id: "X-U002", phase: 2, choice: { type: "custom" }, final_codes: ["Z"], rationale: "r", decided_by: "me", decided_at: "" },
    })
    expect(draftFromDecision(decided, ["R1"])).toMatchObject({ kind: "other", custom: ["Z"] })
  })
  it("has nothing to restore on an undecided item", () => {
    expect(draftFromDecision(item(), ["R1"])).toBeNull()
  })
})

describe("decision keyboard", () => {
  const ctx = { typing: false, kinds: availableChoices(item()) }
  it("chooses with 1, 2, 3 and 4", () => {
    expect(decisionKey({ key: "1" }, ctx)).toEqual({ type: "choose", kind: "human" })
    expect(decisionKey({ key: "2" }, ctx)).toEqual({ type: "choose", kind: "ai" })
    expect(decisionKey({ key: "3" }, ctx)).toEqual({ type: "choose", kind: "merge" })
    expect(decisionKey({ key: "4" }, ctx)).toEqual({ type: "choose", kind: "other" })
  })
  it("ignores a number beyond the choices offered", () => {
    expect(decisionKey({ key: "3" }, { typing: false, kinds: ["human", "other"] })).toBeNull()
    expect(decisionKey({ key: "2" }, { typing: false, kinds: ["human", "other"] })).toEqual({ type: "choose", kind: "other" })
  })
  it("saves on Enter", () => {
    expect(decisionKey({ key: "Enter" }, ctx)).toEqual({ type: "save" })
  })
  it("leaves the keys alone while typing, except Ctrl or Cmd with Enter", () => {
    const typing = { ...ctx, typing: true }
    expect(decisionKey({ key: "1" }, typing)).toBeNull()
    expect(decisionKey({ key: "Enter" }, typing)).toBeNull()
    expect(decisionKey({ key: "Enter", ctrlKey: true }, typing)).toEqual({ type: "save" })
    expect(decisionKey({ key: "Enter", metaKey: true }, typing)).toEqual({ type: "save" })
  })
  it("leaves browser shortcuts, composition and handled events alone", () => {
    expect(decisionKey({ key: "1", metaKey: true }, ctx)).toBeNull()
    expect(decisionKey({ key: "1", altKey: true }, ctx)).toBeNull()
    expect(decisionKey({ key: "1", isComposing: true }, ctx)).toBeNull()
    expect(decisionKey({ key: "1", defaultPrevented: true }, ctx)).toBeNull()
    expect(decisionKey({ key: "x" }, ctx)).toBeNull()
  })
  it("moves between cards with the arrows", () => {
    expect(decisionKey({ key: "ArrowDown" }, ctx)).toEqual({ type: "next" })
    expect(decisionKey({ key: "ArrowUp" }, ctx)).toEqual({ type: "prev" })
  })
  it("does nothing while a dialog owns the keyboard", () => {
    expect(decisionKey({ key: "1" }, { ...ctx, modalOpen: true })).toBeNull()
  })
})

describe("queue", () => {
  const items = [
    item({ unit_id: "X-U010" }),
    item({
      unit_id: "X-U002",
      decision: { unit_id: "X-U002", phase: 2, choice: { type: "human" }, final_codes: ["A"], rationale: "r", decided_by: "me", decided_at: "" },
    }),
    item({ unit_id: "X-U005", kind: "agreement_sample" }),
  ]
  it("filters remaining, decided and all", () => {
    expect(filterQueue(items, "remaining").map((i) => i.unit_id)).toEqual(["X-U010", "X-U005"])
    expect(filterQueue(items, "decided").map((i) => i.unit_id)).toEqual(["X-U002"])
    expect(filterQueue(items, "all").map((i) => i.unit_id)).toEqual(["X-U002", "X-U010", "X-U005"])
  })
  it("goes to the next card of the queue, and stops at the end", () => {
    const q = filterQueue(items, "all")
    expect(nextInQueue(q, "X-U002", 1)).toBe("X-U010")
    expect(nextInQueue(q, "X-U005", 1)).toBeNull()
    expect(nextInQueue(q, "X-U002", -1)).toBeNull()
    expect(nextInQueue(q, "X-U010", -1)).toBe("X-U002")
  })
  it("starts from the first card when the current one left the queue", () => {
    expect(nextInQueue(filterQueue(items, "remaining"), "X-U002", 1)).toBe("X-U010")
  })
})

describe("question before a unit", () => {
  const units = [unit(1, "interviewer", "Combien de temps ?"), unit(2, "respondent", "Six mois."), unit(3, "respondent", "Puis plus.")]
  it("is the interviewer turn right before it", () => {
    expect(precedingQuestion(units, "X-U002")?.text).toBe("Combien de temps ?")
  })
  it("is none when the previous turn is a respondent turn", () => {
    expect(precedingQuestion(units, "X-U003")).toBeNull()
  })
  it("is none for an unknown unit or the first turn", () => {
    expect(precedingQuestion(units, "X-U099")).toBeNull()
    expect(precedingQuestion(units, "X-U001")).toBeNull()
  })
})

const run = (over: Partial<AiRun> = {}): AiRun => ({
  run_id: 1,
  interview_id: "X",
  phase: 2,
  codings: {},
  parts_received: [1, 2],
  parts_expected: 2,
  complete: true,
  meta: {
    run_label: "R1",
    model_name: "modèle X",
    model_version: "",
    temperature: null,
    seed: null,
    context_length: null,
    batch_size: null,
    prompt_id_version: "",
    executed_at: new Date(2026, 9, 13, 9, 40).toISOString(),
    notes: null,
  },
  ...over,
})

describe("run list states", () => {
  it("is complete when every part is in", () => {
    expect(runState(run())).toEqual({ state: "complete", label: "complet" })
  })
  it("is waiting before the first answer", () => {
    expect(runState(run({ complete: false, parts_received: [] }))).toEqual({ state: "waiting", label: "en attente de réponses" })
  })
  it("is partial between the first and the last answer", () => {
    expect(runState(run({ complete: false, parts_received: [1] }))).toEqual({ state: "partial", label: "partiel, 1 partie sur 2" })
  })
  it("is partial without a known total", () => {
    expect(runState(run({ complete: false, parts_received: [1], parts_expected: null })).label).toBe("partiel, 1 partie reçue")
  })
  it("writes a run as the guide does", () => {
    expect(runLine(run())).toBe("R1, 13.10 09:40, modèle modèle X, 2 parties sur 2, complet")
  })
  it("writes a waiting run with no part", () => {
    expect(runLine(run({ complete: false, parts_received: [] }))).toBe("R1, 13.10 09:40, modèle modèle X, 0 partie sur 2, en attente de réponses")
  })
})

describe("review status of the toolbar", () => {
  it("says no review without a run", () => {
    expect(reviewStatusLabel([], null)).toBe("pas de revue")
  })
  it("says the review is waiting while no run is complete", () => {
    expect(reviewStatusLabel([run({ complete: false, parts_received: [1] })], null)).toBe("revue IA en attente")
  })
  it("counts the decided disagreements once a run is complete", () => {
    expect(reviewStatusLabel([run()], { decided: 9, total: 12 })).toBe("revue IA faite, 9 sur 12 décidées")
  })
  it("says done with one disagreement singular", () => {
    expect(reviewStatusLabel([run()], { decided: 0, total: 1 })).toBe("revue IA faite, 0 sur 1 décidée")
  })
  it("says done without any disagreement", () => {
    expect(reviewStatusLabel([run()], { decided: 0, total: 0 })).toBe("revue IA faite, aucun désaccord")
  })
})

describe("answer issues", () => {
  it("counts the problems and keeps their location", () => {
    const lines = issueLines([
      { location: "$.codings", message: "missing" },
      { location: "$.codings.U003", message: "unknown code" },
    ])
    expect(lines.title).toBe("Réponse refusée : 2 problèmes. Rien n'a été importé.")
    expect(lines.items).toEqual([
      { location: "$.codings", message: "missing" },
      { location: "$.codings.U003", message: "unknown code" },
    ])
  })
  it("is singular for one problem", () => {
    expect(issueLines([{ location: "$", message: "x" }]).title).toBe("Réponse refusée : 1 problème. Rien n'a été importé.")
  })
})

describe("candidate concept actions", () => {
  it("needs a written reason for every action", () => {
    expect(candidateProblem({ kind: "reject" }, "")).toBe("rationale")
    expect(candidateProblem({ kind: "reject" }, "hors sujet")).toBeNull()
  })
  it("needs a name to create a code", () => {
    expect(candidateProblem({ kind: "create", name: "  " }, "motif")).toBe("name")
    expect(candidateProblem({ kind: "create", name: "Formation interne" }, "motif")).toBeNull()
  })
  it("needs a target code to map", () => {
    expect(candidateProblem({ kind: "map", code: "" }, "motif")).toBe("code")
    expect(candidateProblem({ kind: "map", code: "SKILLS" }, "motif")).toBeNull()
  })
  it("reports the missing target before the missing reason", () => {
    expect(candidateProblem({ kind: "map", code: "" }, "")).toBe("code")
  })
})
