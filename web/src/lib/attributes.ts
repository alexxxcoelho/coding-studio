// Interview attributes: a client mirror of `AttributeDef::check_value`
// (crates/cs-model/src/project.rs) so that a wrong value is refused before the
// call. The server stays the authority; one wrong value refuses the whole PUT.

import type { AttributeDef } from "./types"
import { t } from "@/i18n"

export interface Checked {
  ok: boolean
  /** Normalised value to send; null clears the attribute. */
  value: string | null
  error?: string
}

/** `YYYY-MM-DD` with a real calendar day. */
export function validDate(s: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s)
  if (!m) return false
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])]
  const leap = (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0
  const max = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][mo - 1]
  return y >= 1 && mo >= 1 && mo <= 12 && d >= 1 && d <= max
}

export function checkAttribute(def: AttributeDef, raw: string): Checked {
  const v = raw.trim()
  if (v === "") return { ok: true, value: null }
  switch (def.kind) {
    case "text":
      return v.length > 2000 ? { ok: false, value: null, error: t("attributes.2000_caracteres_au_plus") } : { ok: true, value: v }
    case "choice":
      return def.choices.includes(v) ? { ok: true, value: v } : { ok: false, value: null, error: t("attributes.valeur_absente_de_la_liste") }
    case "number": {
      const n = Number(v.replace(",", "."))
      return /^[+-]?(\d+([.,]\d*)?|[.,]\d+)$/.test(v) && Number.isFinite(n)
        ? { ok: true, value: v.replace(",", ".") }
        : { ok: false, value: null, error: t("attributes.nombre_attendu") }
    }
    case "date":
      return validDate(v) ? { ok: true, value: v } : { ok: false, value: null, error: t("attributes.date_attendue_au_format_aaaa") }
  }
}

/** Errors by attribute id for a draft; empty when every value passes. */
export function attributeErrors(defs: AttributeDef[], draft: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {}
  for (const d of defs) {
    if (!(d.id in draft)) continue
    const c = checkAttribute(d, draft[d.id])
    if (!c.ok && c.error) out[d.id] = c.error
  }
  return out
}

/** Body of the PUT for the attributes whose value differs from the saved one. Null when nothing changed or a value is wrong. */
export function attributePatch(
  defs: AttributeDef[],
  saved: Record<string, string>,
  draft: Record<string, string>,
): Record<string, string | null> | null {
  const patch: Record<string, string | null> = {}
  for (const d of defs) {
    if (!(d.id in draft)) continue
    const c = checkAttribute(d, draft[d.id])
    if (!c.ok) return null
    const before = saved[d.id] ?? null
    if (c.value !== before) patch[d.id] = c.value
  }
  return Object.keys(patch).length ? patch : null
}

export const KIND_LABELS: Record<AttributeKind2, string> = { get text() { return t("common.texte") }, get choice() { return t("common.liste") }, get number() { return t("common.nombre") }, date: "date" }
type AttributeKind2 = AttributeDef["kind"]

/** "Secteur (liste)" or "Version du guide (liste A, B)" as in the settings summary. */
export function attributeSummary(d: AttributeDef): string {
  return d.kind === "choice" && d.choices.length ? `${d.label} (liste ${d.choices.join(", ")})` : `${d.label} (${KIND_LABELS[d.kind]})`
}

// ------------------------------------------------------------ definition editor

const ID_PATTERN = /^[a-z0-9][a-z0-9_-]*$/

/** Id proposed from a label: lower case, no accents, dashes. Unique among `taken`. */
export function slugId(label: string, taken: string[]): string {
  const base =
    label
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40) || "element"
  let id = ID_PATTERN.test(base) ? base : `a-${base}`
  let n = 2
  while (taken.includes(id)) id = `${base}-${n++}`
  return id
}

/** First problem of an attribute definition list, with the server's rules (choice needs values, only a choice has values, unique labels and ids). */
export function attributeDefsProblem(defs: AttributeDef[]): string | null {
  const ids = new Set<string>()
  const labels = new Set<string>()
  for (const d of defs) {
    const label = d.label.trim()
    if (!label) return t("attributes.un_attribut_n_a_pas")
    if (!ID_PATTERN.test(d.id)) return t("attributes.identifiant_invalide_pour_label", { label })
    if (ids.has(d.id)) return t("attributes.identifiant_en_double_id", { id: d.id })
    if (labels.has(label.toLowerCase())) return t("attributes.nom_en_double_label", { label })
    ids.add(d.id)
    labels.add(label.toLowerCase())
    if (d.kind === "choice") {
      if (d.choices.filter((c) => c.trim()).length === 0) return t("attributes.label_est_une_liste_indiquez", { label })
      const seen = new Set<string>()
      for (const c of d.choices) {
        const tt = c.trim()
        if (seen.has(tt)) return t("attributes.label_valeur_en_double_tt", { label, tt })
        seen.add(tt)
      }
    } else if (d.choices.length > 0) {
      return t("attributes.label_seule_une_liste_porte", { label })
    }
  }
  return null
}

/** Definition with its choices cleaned: trimmed, no empty value, none for a kind other than a list. */
export function cleanDef(d: AttributeDef): AttributeDef {
  return {
    ...d,
    label: d.label.trim(),
    choices: d.kind === "choice" ? d.choices.map((c) => c.trim()).filter(Boolean) : [],
  }
}
