// The interviews "currently open", listed under Bibliothèque in the sidebar.
// An interview opens when its route is visited and stays until closed. Kept
// for the session (sessionStorage), most recent first.

import { useSyncExternalStore } from "react"

const projectKey = (window as Window & {__CS_PROJECT_KEY__?: string}).__CS_PROJECT_KEY__
const KEY = projectKey ? `cs-open-interviews:${projectKey}` : "cs-open-interviews"
export const MAX_OPEN = 6

function read(): string[] {
  try {
    const raw = window.sessionStorage.getItem(KEY)
    const v = raw ? JSON.parse(raw) : []
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, MAX_OPEN) : []
  } catch {
    return []
  }
}

let list: string[] = read()
const listeners = new Set<() => void>()

function commit(next: string[]) {
  list = next
  try {
    window.sessionStorage.setItem(KEY, JSON.stringify(next))
  } catch {
    // Storage unavailable: the list lasts for this page load only.
  }
  listeners.forEach((l) => l())
}

/** Pure: the list after opening `id` (kept in place if already open, else added first). */
export function withOpened(current: string[], id: string): string[] {
  if (current.includes(id)) return current
  return [id, ...current].slice(0, MAX_OPEN)
}

export const openInterviews = {
  get: () => list,
  open: (id: string) => {
    const next = withOpened(list, id)
    if (next !== list) commit(next)
  },
  close: (id: string) => commit(list.filter((x) => x !== id)),
  reset: () => commit([]),
}

function subscribe(cb: () => void) {
  listeners.add(cb)
  return () => {
    listeners.delete(cb)
  }
}

export function useOpenInterviews(): string[] {
  return useSyncExternalStore(subscribe, openInterviews.get)
}
