import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { createAutosaver, type SaveStatus } from "./autosave"

beforeEach(() => {
  vi.useFakeTimers()
})
afterEach(() => {
  vi.useRealTimers()
})

function setup(save: (v: string) => Promise<void> = async () => {}, initial = "") {
  const statuses: SaveStatus[] = []
  const spy = vi.fn(save)
  const saver = createAutosaver<string>({ delay: 800, save: spy, initial, onStatus: (s) => statuses.push(s) })
  return { saver, spy, statuses }
}

describe("createAutosaver", () => {
  it("waits for the pause before saving, and saves only the last value", async () => {
    const { saver, spy } = setup()
    saver.change("a")
    saver.change("ab")
    saver.change("abc")
    await vi.advanceTimersByTimeAsync(799)
    expect(spy).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy).toHaveBeenCalledWith("abc")
  })

  it("restarts the wait at each keystroke", async () => {
    const { saver, spy } = setup()
    saver.change("a")
    await vi.advanceTimersByTimeAsync(600)
    saver.change("ab")
    await vi.advanceTimersByTimeAsync(600)
    expect(spy).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(200)
    expect(spy).toHaveBeenCalledWith("ab")
  })

  it("does not save a value equal to the saved one", async () => {
    const { saver, spy } = setup(undefined, "texte")
    saver.change("texte")
    await vi.advanceTimersByTimeAsync(2000)
    expect(spy).not.toHaveBeenCalled()
  })

  it("flush saves at once and clears the wait", async () => {
    const { saver, spy } = setup()
    saver.change("x")
    await saver.flush()
    expect(spy).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(2000)
    expect(spy).toHaveBeenCalledTimes(1)
  })

  it("flush with nothing pending does nothing", async () => {
    const { saver, spy } = setup()
    await saver.flush()
    expect(spy).not.toHaveBeenCalled()
  })

  it("goes through pending, saving, saved", async () => {
    const { saver, statuses } = setup()
    saver.change("x")
    await vi.advanceTimersByTimeAsync(800)
    expect(statuses).toEqual(["pending", "saving", "saved"])
  })

  it("keeps the text and reports an error when the save fails, then retries on the next change", async () => {
    let fail = true
    const { saver, spy, statuses } = setup(async () => {
      if (fail) throw new Error("hors ligne")
    })
    saver.change("x")
    await vi.advanceTimersByTimeAsync(800)
    expect(statuses.at(-1)).toBe("error")
    expect(saver.pending()).toBe("x")
    fail = false
    saver.change("xy")
    await vi.advanceTimersByTimeAsync(800)
    expect(spy).toHaveBeenLastCalledWith("xy")
    expect(statuses.at(-1)).toBe("saved")
    expect(saver.pending()).toBeUndefined()
  })

  it("queues what is typed during a save and saves it after", async () => {
    let release: () => void = () => {}
    const first = new Promise<void>((r) => (release = r))
    const calls: string[] = []
    const saver = createAutosaver<string>({
      delay: 100,
      initial: "",
      save: async (v) => {
        calls.push(v)
        if (calls.length === 1) await first
      },
    })
    saver.change("a")
    await vi.advanceTimersByTimeAsync(100)
    saver.change("ab")
    await vi.advanceTimersByTimeAsync(100)
    expect(calls).toEqual(["a"])
    release()
    await vi.advanceTimersByTimeAsync(0)
    await vi.advanceTimersByTimeAsync(100)
    expect(calls).toEqual(["a", "ab"])
  })

  it("cancel drops the pending value", async () => {
    const { saver, spy } = setup()
    saver.change("x")
    saver.cancel()
    await vi.advanceTimersByTimeAsync(2000)
    expect(spy).not.toHaveBeenCalled()
  })
})
