import { describe, expect, it } from "vitest"

import { checksProgress, planStatusChange, STATUS_LABEL } from "./theme-status"

const all = (done: boolean[]) => done.map((d, i) => ({ check_id: `c${i + 1}`, done: d }))

describe("planStatusChange", () => {
  it("refuses a change to the current status", () => {
    const p = planStatusChange({ from: "revised", to: "revised", checks: all([true]) })
    expect(p.allowed).toBe(false)
    expect(p.reason).toBe("same")
  })

  it("lets a theme go from candidate to revised whatever the checks say", () => {
    const p = planStatusChange({ from: "candidate", to: "revised", checks: all([false, false]) })
    expect(p).toMatchObject({ allowed: true, needsOverride: false })
  })

  it("lets a retained theme go back without any reason", () => {
    expect(planStatusChange({ from: "retained", to: "revised", checks: all([false]) }).allowed).toBe(true)
    expect(planStatusChange({ from: "retained", to: "candidate", checks: all([false]) }).allowed).toBe(true)
  })

  it("allows retained when every check is done", () => {
    const p = planStatusChange({ from: "revised", to: "retained", checks: all([true, true, true]) })
    expect(p).toMatchObject({ allowed: true, needsOverride: false, pending: [] })
  })

  it("allows retained for a study that has no check", () => {
    expect(planStatusChange({ from: "candidate", to: "retained", checks: [] }).allowed).toBe(true)
  })

  it("asks for a reason when a check is not done, and lists the checks left", () => {
    const p = planStatusChange({ from: "revised", to: "retained", checks: all([true, false, false]) })
    expect(p).toMatchObject({ allowed: false, needsOverride: true, reason: "override_required" })
    expect(p.pending).toEqual(["c2", "c3"])
  })

  it("accepts the override once the reason is written, and ignores blanks", () => {
    const checks = all([true, false])
    expect(planStatusChange({ from: "revised", to: "retained", checks, overrideReason: "   " }).allowed).toBe(false)
    const p = planStatusChange({ from: "revised", to: "retained", checks, overrideReason: "Cas unique justifié" })
    expect(p).toMatchObject({ allowed: true, needsOverride: true })
  })
})

describe("checksProgress", () => {
  it("counts the checks done", () => {
    expect(checksProgress(all([true, false, true]))).toEqual({ done: 2, total: 3, complete: false })
    expect(checksProgress([])).toEqual({ done: 0, total: 0, complete: true })
  })
})

describe("labels", () => {
  it("are the French words of the guide", () => {
    expect(STATUS_LABEL).toEqual({ candidate: "Candidat", revised: "Révisé", retained: "Retenu" })
  })
})
