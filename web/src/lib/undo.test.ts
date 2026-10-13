import { describe, expect, it } from "vitest"
import { UndoStack } from "./undo"

describe("UndoStack", () => {
  it("undoes the latest action first", async () => {
    const order: string[] = []
    const s = new UndoStack()
    s.push({ label: "a", run: async () => void order.push("a") })
    s.push({ label: "b", run: async () => void order.push("b") })
    expect((await s.undo())?.label).toBe("b")
    expect((await s.undo())?.label).toBe("a")
    expect(order).toEqual(["b", "a"])
    expect(await s.undo()).toBeNull()
  })
  it("keeps the entry when the reversal fails", async () => {
    const s = new UndoStack()
    let fail = true
    s.push({
      label: "a",
      run: async () => {
        if (fail) throw new Error("boom")
      },
    })
    await expect(s.undo()).rejects.toThrow("boom")
    expect(s.size).toBe(1)
    fail = false
    expect((await s.undo())?.label).toBe("a")
    expect(s.size).toBe(0)
  })
  it("forgets the oldest entries beyond the limit", () => {
    const s = new UndoStack(2)
    for (const l of ["a", "b", "c"]) s.push({ label: l, run: async () => {} })
    expect(s.size).toBe(2)
    expect(s.peek()?.label).toBe("c")
    s.clear()
    expect(s.size).toBe(0)
  })
})
