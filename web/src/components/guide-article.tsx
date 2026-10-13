// A guide section as rendered from its markdown. `scrollTo` is a heading id to
// bring into view once shown; `onNavigate` is called when an internal link is
// followed (the help drawer closes itself then).

import { useEffect, useMemo, useRef } from "react"

import { renderMarkdown } from "@/lib/markdown"

export function GuideArticle({
  source,
  scrollTo,
  onNavigate,
  className,
}: {
  source: string
  scrollTo?: string | null
  onNavigate?: () => void
  className?: string
}) {
  const { html } = useMemo(() => renderMarkdown(source), [source])
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const root = ref.current
    if (!root) return
    const target = scrollTo ? root.querySelector<HTMLElement>(`[id="${CSS.escape(scrollTo)}"]`) : null
    if (target && typeof target.scrollIntoView === "function") target.scrollIntoView({ block: "start" })
  }, [html, scrollTo])

  return (
    <div
      ref={ref}
      className={className ? `paper ${className}` : "paper"}
      onClick={(e) => {
        const a = (e.target as HTMLElement).closest("a")
        if (a && a.getAttribute("href")?.startsWith("#/")) onNavigate?.()
      }}
      // The guide is the project's own documentation, bundled at build time.
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}
