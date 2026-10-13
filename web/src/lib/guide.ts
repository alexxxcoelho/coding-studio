// The in-app guide: sections of docs/guide/guide.json, their markdown (bundled
// with Vite ?raw) and the map from a screen to the part of the guide to show
// in the help drawer.

import { guideScreens, guideSections, type GuideScreen, type GuideSection } from "@/lib/guide-index"
import type { Route } from "@/lib/router"

export { guideScreens, guideSections }
export type { GuideScreen, GuideSection }

// Both languages are bundled: the markdown files of fr/ and en/, found by their path.
const RAW = import.meta.glob("@guide/{fr,en}/*.md", { query: "?raw", import: "default", eager: true }) as Record<string, string>

function basename(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1)
}

const SOURCES = new Map(Object.entries(RAW).map(([path, text]) => [basename(path), text]))

export function sectionSource(sectionId: string): string {
  const section = guideSections().find((s) => s.id === sectionId)
  return (section && SOURCES.get(basename(section.file))) || ""
}

/** The section that owns a section or heading anchor, null when none does. */
export function sectionForAnchor(anchor: string): GuideSection | null {
  return guideSections().find((s) => s.anchor === anchor || s.headings.some((h) => h.anchor === anchor)) ?? null
}

/** Section and anchor to show for a screen id. Unknown screens get the summary. */
export function helpTarget(screen: string): GuideScreen {
  const mapped = guideScreens()[screen]
  if (mapped) return mapped
  // A screen the map does not list but whose topic is a whole section (export).
  const section = guideSections().find((s) => s.id === screen)
  if (section) return { section: section.id, file: section.file, anchor: section.anchor }
  const first = guideSections()[0]
  return { section: first.id, file: first.file, anchor: first.anchor }
}

/** The screen id (guide.json "screens") that describes the current route. */
export function screenForRoute(route: Route): string {
  switch (route.page) {
    case "library":
      return "bibliotheque"
    case "interview":
      if (route.tab === "ai") return "revoir-ia"
      if (route.tab === "review") return "revoir-decisions"
      if (route.tab === "harmonise") return "revoir-decisions"
      if (route.tab === "cases") return "analyser"
      if (route.tab === "audit") return "exporter"
      return "coder"
    case "codebook":
      return "codebook"
    case "lexique":
    case "search":
      return "coder"
    case "analyser":
    case "saturation":
      return "analyser"
    case "rediger":
      return "rediger"
    case "to-clarify":
      return "revoir-decisions"
    case "settings":
      return "reglages"
    case "audit":
      return "exporter"
    case "guide":
      return "guide"
  }
}
