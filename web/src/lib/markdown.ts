// Markdown of the in-app guide (docs/guide/fr). Headings carry an explicit id
// as a trailing {#anchor}; links between files (02_coder.md#coder-detail) are
// rewritten to routes of the app (#/guide/coder-detail). The content is the
// project's own documentation, not user input.

import { Marked, type Tokens } from "marked"

import { GUIDE_SECTIONS } from "@/lib/guide-index"

export interface GuideHeading {
  level: number
  id: string
  text: string
}

const ATTR = /\s*\{#([A-Za-z0-9_-]+)\}\s*$/

/** Route of the app for a link found in a guide file; external links are returned unchanged. */
export function resolveGuideHref(href: string): string {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return href
  if (href.startsWith("#/")) return href
  if (href.startsWith("#")) return `#/guide/${href.slice(1)}`
  const m = /^(?:\.\/|fr\/)*(\d{2}_[A-Za-z0-9_]+\.md)(?:#(.+))?$/.exec(href)
  if (!m) return href
  if (m[2]) return `#/guide/${m[2]}`
  const section = GUIDE_SECTIONS.find((s) => s.file.endsWith(m[1]))
  return section ? `#/guide/${section.anchor}` : href
}

export function renderMarkdown(source: string): { html: string; headings: GuideHeading[] } {
  const headings: GuideHeading[] = []
  const marked = new Marked({
    gfm: true,
    renderer: {
      heading(this, token: Tokens.Heading) {
        const m = ATTR.exec(token.text)
        const inner = this.parser.parseInline(token.tokens).replace(ATTR, "")
        const text = token.text.replace(ATTR, "")
        if (!m) return `<h${token.depth}>${inner}</h${token.depth}>\n`
        headings.push({ level: token.depth, id: m[1], text })
        return `<h${token.depth} id="${m[1]}">${inner}</h${token.depth}>\n`
      },
      link(this, token: Tokens.Link) {
        const href = resolveGuideHref(token.href)
        const inner = this.parser.parseInline(token.tokens)
        const title = token.title ? ` title="${token.title.replace(/"/g, "&quot;")}"` : ""
        const external = /^[a-z][a-z0-9+.-]*:/i.test(href)
        const rel = external ? ' target="_blank" rel="noopener noreferrer"' : ""
        return `<a href="${href.replace(/"/g, "&quot;")}"${title}${rel}>${inner}</a>`
      },
    },
  })
  return { html: marked.parse(source, { async: false }), headings }
}
