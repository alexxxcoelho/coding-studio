// Pure helpers of the V2 screens: wording of the server's refusals in French,
// matched fields of the code search, pieces of a text around its matches,
// clock formats and the colour steps of the co-occurrence table.

import { ApiError } from "./api"
import type { AudioUnit, CodeMatchField, CoCell, CoGroup } from "./v2-api"

// ------------------------------------------------------------ code search

const FIELD_LABEL: Record<CodeMatchField, string> = {
  name: "Nom",
  alias: "Alias",
  token: "Identifiant d'origine",
  label: "Libellé",
  definition: "Définition",
}

export function matchedFieldLabel(field: CodeMatchField): string {
  return FIELD_LABEL[field] ?? field
}

/** The line shown under a code found by the search: the field that matched and its text. A hit on the name needs no line. */
export function matchedLine(m: { field: CodeMatchField; text: string } | undefined | null, max = 70): string | null {
  if (!m || m.field === "name") return null
  const t = m.text.length > max ? `${m.text.slice(0, max - 1).trimEnd()}…` : m.text
  return `${matchedFieldLabel(m.field)} : ${t}`
}

// ------------------------------------------------------------ refusals

/** What a refused undo, split or decision means, in the researcher's words. */
export function explainCodeOpError(e: unknown): string {
  if (!(e instanceof ApiError)) return e instanceof Error ? e.message : String(e)
  const m = e.message
  if (e.status === 409) {
    if (/already undone/.test(m)) return "Cette opération a déjà été annulée. Rien n'est à refaire."
    const later = /operation (\d+) came after/.exec(m)
    if (later) return `L'opération n° ${later[1]}, plus récente, touche les mêmes codes. Annulez-la d'abord, puis revenez à celle-ci.`
    if (/no longer merged/.test(m)) return "Le code n'est plus fusionné dans cette cible : il a changé depuis. Rien n'a été annulé."
    if (/split \d+ was undone/.test(m)) return "Cette scission a été annulée : sa liste ne reçoit plus de décision."
    return `Refusé par la règle de protection : ${m}`
  }
  if (e.status === 404) {
    if (/code operation/.test(m)) return "Cette opération est introuvable. Une fusion faite avant l'existence du journal ne peut pas être annulée."
    return "Introuvable. La page est peut-être en retard sur le serveur : rechargez-la."
  }
  if (/needs a rationale/.test(m)) return "Le motif est obligatoire."
  if (/two or more new codes/.test(m)) return "Une scission donne au moins deux nouveaux codes."
  const twice = /the name (.+) is given twice/.exec(m)
  if (twice) return `Le nom « ${twice[1]} » est donné deux fois.`
  if (/reserved code/.test(m)) return "Le code réservé « Plus tard » ne peut pas être scindé."
  if (/inactive or merged/.test(m)) return "Un code inactif ou fusionné ne peut pas être scindé."
  return m
}

/** Wording of a refusal of the transcript review. */
export function explainReviewError(e: unknown): string {
  if (!(e instanceof ApiError)) return e instanceof Error ? e.message : String(e)
  const m = e.message
  if (e.status === 409) return "Ce passage a déjà été décidé. La liste va se mettre à jour."
  if (/proposed span/.test(m)) return "Rien ne propose de nouvel emplacement : choisissez « Ajuster » ou « Détacher »."
  if (/another passage|already covers|overlap/.test(m)) return "Un autre passage couvre déjà cet extrait."
  if (/interviewer/.test(m)) return "Le tour est devenu un tour de l'intervieweur : un passage ne s'y code pas."
  if (/empty|reversed|outside/.test(m)) return "Sélectionnez un extrait non vide dans le texte actuel du tour."
  return m
}

// ----------------------------------------------------------------- text

export interface TextPiece {
  text: string
  /** Index of the match when the piece is one, else -1. */
  hit: number
}

/** Cuts `text` into pieces around the ranges [start, end) given in UTF-16 units (overlaps and bad ranges are skipped). */
export function piecesAround(text: string, ranges: { start: number; end: number }[]): TextPiece[] {
  const sorted = ranges
    .map((r, i) => ({ ...r, i }))
    .filter((r) => r.start >= 0 && r.end > r.start && r.end <= text.length)
    .sort((a, b) => a.start - b.start)
  const out: TextPiece[] = []
  let at = 0
  for (const r of sorted) {
    if (r.start < at) continue
    if (r.start > at) out.push({ text: text.slice(at, r.start), hit: -1 })
    out.push({ text: text.slice(r.start, r.end), hit: r.i })
    at = r.end
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: -1 })
  return out
}

/** UTF-16 position of the position `n` counted in Unicode scalar values. */
export function scalarToUtf16(text: string, n: number): number {
  let units = 0
  let count = 0
  for (const ch of text) {
    if (count >= n) break
    units += ch.length
    count++
  }
  return units
}

/** Highlights of an excerpt (scalar positions relative to the excerpt) as UTF-16 ranges. */
export function excerptRanges(text: string, highlights: { start: number; end: number }[]): { start: number; end: number }[] {
  return highlights.map((h) => ({ start: scalarToUtf16(text, h.start), end: scalarToUtf16(text, h.end) }))
}

// ---------------------------------------------------------------- clock

/** 75.4 gives "1:15", 3725 gives "1:02:05". */
export function clock(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) return "--:--"
  const s = Math.floor(seconds)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const r = s % 60
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m)
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(r).padStart(2, "0")}`
}

/** The unit being heard at `t` seconds: the last one that starts at or before it (and ends after it when it has an end). */
export function unitAt(units: AudioUnit[], t: number): string | null {
  let found: string | null = null
  for (const u of units) {
    if (u.audio_seconds === null) continue
    if (u.audio_seconds <= t + 1e-6 && (u.end_seconds === null || t < u.end_seconds)) found = u.unit_id
    else if (u.audio_seconds > t) break
  }
  return found
}

/** A decimal offset typed by the researcher ("2,5", "-3", "1:30") in seconds, or null when it is not a number. */
export function parseOffset(text: string): number | null {
  const t = text.trim().replace(",", ".")
  if (!t) return null
  const clockForm = /^(-)?(\d+):(\d{1,2})(?:\.(\d+))?$/.exec(t)
  if (clockForm) {
    const v = Number(clockForm[2]) * 60 + Number(clockForm[3]) + (clockForm[4] ? Number(`0.${clockForm[4]}`) : 0)
    return clockForm[1] ? -v : v
  }
  if (!/^-?\d+(\.\d+)?$/.test(t)) return null
  const v = Number(t)
  return Number.isFinite(v) && Math.abs(v) <= 86400 ? v : null
}

// -------------------------------------------------------- co-occurrence

/** Alpha of the accent for a value, in five steps from 0.10 to 0.62 so that the printed number keeps its contrast. */
export function heatAlpha(value: number, max: number): number {
  if (value <= 0 || max <= 0) return 0
  const step = Math.min(5, Math.max(1, Math.ceil((value / max) * 5)))
  return [0, 0.1, 0.22, 0.34, 0.48, 0.62][step]
}

/** The cell of a group for two codes, whatever the order they were asked in. */
export function cellOf(group: CoGroup, a: string, b: string): CoCell | null {
  return group.cells.find((c) => (c.a === a && c.b === b) || (c.a === b && c.b === a)) ?? null
}

/** The largest count of units among the cells of the groups, for the colour steps. */
export function maxUnits(groups: CoGroup[]): number {
  let m = 0
  for (const g of groups) for (const c of g.cells) m = Math.max(m, c.units)
  return m
}

// ------------------------------------------------------------- worklist

export interface Draft {
  action: "assign" | "leave"
  targets: string[]
}

/** The draft after pressing the key of the code `codeId` on a passage: toggles it among the targets (assign), clearing "leave". */
export function toggleTarget(d: Draft | undefined, codeId: string): Draft | undefined {
  const targets = d?.action === "assign" ? d.targets : []
  const next = targets.includes(codeId) ? targets.filter((t) => t !== codeId) : [...targets, codeId]
  return next.length ? { action: "assign", targets: next } : undefined
}
