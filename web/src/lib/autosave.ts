// Autosave of a written part: the text is sent to the server after a pause in
// typing, at once on blur or on leaving, never twice for the same text, and a
// failed save keeps the text and says so. `createAutosaver` is the logic with no
// React in it; `useAutosavedText` binds it to a textarea.

import { useEffect, useState } from "react"
import { t } from "@/i18n"

export type SaveStatus = "idle" | "pending" | "saving" | "saved" | "error"

export interface Autosaver<T> {
  /** A new value from the editor: restarts the wait. */
  change: (value: T) => void
  /** Saves what is pending now. Resolves when nothing is pending or in flight. */
  flush: () => Promise<void>
  /** Drops the pending value. */
  cancel: () => void
  /** The value waiting to be saved, if any. */
  pending: () => T | undefined
  /** Takes `value` as the saved one (the server's copy changed under us). */
  reset: (value: T) => void
  /** Replaces the function that sends a value (the caller's closure changed). */
  setSave: (save: (value: T) => Promise<unknown>) => void
}

export function createAutosaver<T>(opts: {
  delay: number
  save: (value: T) => Promise<unknown>
  initial: T
  onStatus?: (status: SaveStatus) => void
}): Autosaver<T> {
  let saved = opts.initial
  let pending: T | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  let inflight: Promise<void> | null = null
  let status: SaveStatus = "idle"
  let send = opts.save

  const setStatus = (s: SaveStatus) => {
    if (s === status) return
    status = s
    opts.onStatus?.(s)
  }
  const clearTimer = () => {
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
  }
  const schedule = () => {
    clearTimer()
    timer = setTimeout(() => void run(), opts.delay)
  }

  async function run(): Promise<void> {
    clearTimer()
    if (inflight || pending === undefined) return
    const value = pending
    setStatus("saving")
    inflight = (async () => {
      try {
        await send(value)
        saved = value
        if (pending === value) pending = undefined
        setStatus(pending === undefined ? "saved" : "pending")
      } catch {
        setStatus("error")
      } finally {
        inflight = null
      }
    })()
    await inflight
    // Something was typed while the save was in flight: save it after a pause.
    if (pending !== undefined && status === "pending") schedule()
  }

  return {
    change(value) {
      if (value === saved && !inflight) {
        pending = undefined
        clearTimer()
        return
      }
      pending = value
      setStatus("pending")
      schedule()
    },
    async flush() {
      clearTimer()
      if (inflight) await inflight
      if (pending !== undefined) await run()
    },
    cancel() {
      pending = undefined
      clearTimer()
    },
    pending: () => pending,
    reset(value) {
      saved = value
    },
    setSave(save) {
      send = save
    },
  }
}

/**
 * State of a textarea that saves itself. `serverValue` is the copy held by the
 * server; it replaces the local text only when nothing is waiting to be saved.
 */
export function useAutosavedText(serverValue: string, save: (value: string) => Promise<unknown>, delay = 800) {
  const [value, setValue] = useState(serverValue)
  const [status, setStatus] = useState<SaveStatus>("idle")
  const [seen, setSeen] = useState(serverValue)
  const [saver] = useState(() => createAutosaver<string>({ delay, initial: serverValue, save, onStatus: setStatus }))
  useEffect(() => {
    saver.setSave(save)
  })

  if (serverValue !== seen) {
    setSeen(serverValue)
    if (saver.pending() === undefined) {
      saver.reset(serverValue)
      setValue(serverValue)
    }
  }

  // Leaving the screen saves what is waiting.
  useEffect(() => () => void saver.flush(), [saver])

  return {
    value,
    status,
    onChange: (v: string) => {
      setValue(v)
      saver.change(v)
    },
    flush: () => saver.flush(),
  }
}

export const SAVE_STATUS_LABEL: Record<SaveStatus, string> = {
  idle: "",
  get pending() { return t("common.modifie") },
  get saving() { return t("autosave.enregistrement") },
  get saved() { return t("common.enregistre") },
  get error() { return t("autosave.echec_de_l_enregistrement") },
}
