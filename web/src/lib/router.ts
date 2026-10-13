// Tiny hash router: #/interviews, #/interviews/ECO-07/coding?phase=2,
// #/codebook, #/to-clarify, #/analyser, #/audit, #/guide/coder-detail. Hash routing keeps the
// embedded server stateless. `?unit=ECO-07-U003` selects a unit in Coding.

import { useSyncExternalStore } from "react"
import type { Phase } from "./types"

export type Tab = "sources" | "coding" | "highlights" | "ai" | "review" | "cases" | "verification" | "audit"
export const TABS: Tab[] = ["sources", "coding", "highlights", "ai", "review", "cases", "verification", "audit"]

/** Tabs that exist in phase 2 only (the AI challenges the detailed codes). */
export const PHASE2_ONLY: Tab[] = ["ai", "review"]

export type Route =
  | { page: "library" }
  | { page: "interview"; id: string; tab: Tab; phase: Phase; unit: string | null }
  | { page: "codebook" }
  | { page: "to-clarify" }
  | { page: "saturation" }
  | { page: "settings" }
  | { page: "audit" }
  | { page: "analyser" }
  | { page: "lexique" }
  | { page: "rediger" }
  /** The guide; `anchor` is a section or heading anchor of docs/guide/guide.json. */
  | { page: "guide"; anchor: string | null }

export function parseHash(hash: string): Route {
  const raw = hash.replace(/^#\/?/, "")
  const [path, query = ""] = raw.split("?")
  const parts = path.split("/").filter(Boolean).map(decodeURIComponent)
  const params = new URLSearchParams(query)
  if (parts[0] === "interviews" && parts[1]) {
    // v2 links to the reconciliation tab land on the review.
    const raw = parts[2] === "reconciliation" ? "review" : parts[2]
    const tab = TABS.includes(raw as Tab) ? (raw as Tab) : "sources"
    const phase: Phase = PHASE2_ONLY.includes(tab) || params.get("phase") === "2" ? 2 : 1
    return { page: "interview", id: parts[1], tab, phase, unit: params.get("unit") }
  }
  if (parts[0] === "codebook") return { page: "codebook" }
  if (parts[0] === "to-clarify") return { page: "to-clarify" }
  // The English pages are gone: their old links land on the screens that replaced them.
  if (parts[0] === "cross-case") return { page: "analyser" }
  if (parts[0] === "answers") return { page: "rediger" }
  if (parts[0] === "saturation") return { page: "saturation" }
  if (parts[0] === "settings") return { page: "settings" }
  if (parts[0] === "audit") return { page: "audit" }
  if (parts[0] === "analyser") return { page: "analyser" }
  if (parts[0] === "lexique") return { page: "lexique" }
  if (parts[0] === "rediger") return { page: "rediger" }
  if (parts[0] === "guide") return { page: "guide", anchor: parts[1] ?? null }
  return { page: "library" }
}

export function interviewHref(id: string, tab: Tab = "sources", phase: Phase = 1, unit?: string): string {
  const q = new URLSearchParams()
  if (phase === 2 && !PHASE2_ONLY.includes(tab)) q.set("phase", "2")
  if (unit) q.set("unit", unit)
  const qs = q.toString()
  return `#/interviews/${encodeURIComponent(id)}/${tab}${qs ? `?${qs}` : ""}`
}

/** Link to a unit in the Coding tab of its interview. */
export function unitHref(unitId: string, phase: Phase = 2): string {
  const id = unitId.replace(/-U\d+$/, "")
  return interviewHref(id, "coding", phase, unitId)
}

function subscribe(cb: () => void) {
  window.addEventListener("hashchange", cb)
  return () => window.removeEventListener("hashchange", cb)
}

export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, () => window.location.hash)
  return parseHash(hash)
}

export function navigate(href: string): void {
  window.location.hash = href.replace(/^#/, "")
}
