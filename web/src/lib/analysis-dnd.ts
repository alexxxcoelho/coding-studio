// Drag and drop of the two screens, as pure logic: what travels with a drag, and
// what a drop means. A code dragged from the tree into a theme adds it to the
// theme; a passage dragged from the list into a part of the text creates a typed
// link; a theme dragged onto another one moves in the chapter order.

import type { LinkCreate, LinkTarget, PassageLink } from "./analysis-api"

export const MIME_CODE = "application/x-cs-code"
export const MIME_PASSAGE = "application/x-cs-passage"

export type DragKind = "code" | "passage"
export interface DragPayload {
  kind: DragKind
  id: string
}

export const mimeOf = (kind: DragKind): string => (kind === "code" ? MIME_CODE : MIME_PASSAGE)

export function encodeDrag(p: DragPayload): string {
  return JSON.stringify(p)
}

export function decodeDrag(raw: string | null | undefined, kind: DragKind): DragPayload | null {
  if (!raw) return null
  try {
    const v = JSON.parse(raw) as Partial<DragPayload>
    return v && v.kind === kind && typeof v.id === "string" && v.id ? { kind, id: v.id } : null
  } catch {
    return null
  }
}

type Writable = Pick<DataTransfer, "setData">
type Readable = Pick<DataTransfer, "getData">

/** Writes the payload under its own type and, as a fallback, as plain text. */
export function writeDrag(dt: Writable & { effectAllowed?: string }, p: DragPayload): void {
  const raw = encodeDrag(p)
  dt.setData(mimeOf(p.kind), raw)
  dt.setData("text/plain", raw)
  dt.effectAllowed = "copyMove"
}

export function readDrag(dt: Readable, kind: DragKind): DragPayload | null {
  return decodeDrag(dt.getData(mimeOf(kind)), kind) ?? decodeDrag(dt.getData("text/plain"), kind)
}

/** True when a drag in flight may carry `kind` (the data itself cannot be read before the drop). */
export function carries(dt: { types: readonly string[] } | null | undefined, kind: DragKind): boolean {
  return !!dt && Array.from(dt.types).includes(mimeOf(kind))
}

function sameTarget(a: LinkTarget, b: LinkTarget): boolean {
  if (a.kind === "theme_part" && b.kind === "theme_part") return a.theme_id === b.theme_id && a.part === b.part
  if (a.kind === "rq_part" && b.kind === "rq_part") return a.rq_id === b.rq_id
  return false
}

export type LinkPlan =
  | { action: "create"; body: Required<LinkCreate> }
  | { action: "duplicate"; link: PassageLink }
  | { action: "refused"; reason: string }

/**
 * What dropping a passage on a part does. The type is the first of the study
 * list (the researcher changes it on the chip), the note starts empty. A passage
 * is linked once to a given part; it can be linked to several parts and themes.
 */
export function planLink(opts: { passageId: string; target: LinkTarget; links: PassageLink[]; linkTypes: { id: string; label: string }[] }): LinkPlan {
  const { passageId, target, links, linkTypes } = opts
  const first = linkTypes[0]
  if (!first) return { action: "refused", reason: "L'étude n'a aucun type de lien : ajoutez-en dans les Réglages." }
  const there = links.find((l) => l.passage.passage_id === passageId && sameTarget(l.target, target))
  if (there) return { action: "duplicate", link: there }
  return { action: "create", body: { passage_id: passageId, target, link_type: first.id, note: "" } }
}

export function planCodeAdd(themeCodeIds: string[], codeId: string): "add" | "present" {
  return themeCodeIds.includes(codeId) ? "present" : "add"
}

export function moveItem<T>(list: T[], from: number, to: number): T[] {
  if (from < 0 || from >= list.length || to < 0 || to >= list.length || from === to) return list
  const next = [...list]
  const [item] = next.splice(from, 1)
  next.splice(to, 0, item)
  return next
}

/** The dragged id takes the place of the id it is dropped on. */
export function reorderByDrop(ids: string[], dragId: string, overId: string): string[] {
  const from = ids.indexOf(dragId)
  const to = ids.indexOf(overId)
  if (from < 0 || to < 0) return ids
  return moveItem(ids, from, to)
}
