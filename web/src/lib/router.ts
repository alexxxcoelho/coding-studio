// Tiny hash router: #/interviews, #/interviews/ECO-07/coding?phase=2,
// #/codebook, #/audit. Hash routing keeps the embedded server stateless.

import { useSyncExternalStore } from "react"
import type { Phase } from "./types"

export type Tab = "sources" | "coding" | "ai" | "reconciliation" | "audit"
export const TABS: Tab[] = ["sources", "coding", "ai", "reconciliation", "audit"]

export type Route =
  | { page: "library" }
  | { page: "interview"; id: string; tab: Tab; phase: Phase }
  | { page: "codebook" }
  | { page: "audit" }

export function parseHash(hash: string): Route {
  const raw = hash.replace(/^#\/?/, "")
  const [path, query = ""] = raw.split("?")
  const parts = path.split("/").filter(Boolean).map(decodeURIComponent)
  const params = new URLSearchParams(query)
  if (parts[0] === "interviews" && parts[1]) {
    const tab = TABS.includes(parts[2] as Tab) ? (parts[2] as Tab) : "sources"
    const phase: Phase = params.get("phase") === "2" ? 2 : 1
    return { page: "interview", id: parts[1], tab, phase }
  }
  if (parts[0] === "codebook") return { page: "codebook" }
  if (parts[0] === "audit") return { page: "audit" }
  return { page: "library" }
}

export function interviewHref(id: string, tab: Tab = "sources", phase: Phase = 1): string {
  return `#/interviews/${encodeURIComponent(id)}/${tab}${phase === 2 ? "?phase=2" : ""}`
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
