#!/usr/bin/env node
// Consistency check of the interface catalogues (pure node, no dependency).
//
//   node scripts/check-i18n.mjs
//
// Fails (exit 1) when:
//   - fr.json and en.json do not have exactly the same keys;
//   - a value is empty, or its {placeholders} differ between fr and en;
//   - a catalogue contains an em dash, an en dash or an emoji;
//   - an English value contains French stop words or accented French letters;
//   - a key is never referenced in the sources (unused key).
// Unknown keys are caught by the compiler (the `Key` type), not here.

import { readFileSync, readdirSync, statSync } from "node:fs"
import { dirname, join, relative } from "node:path"
import { fileURLToPath } from "node:url"

const root = join(dirname(fileURLToPath(import.meta.url)), "..")
const src = join(root, "src")
const fr = JSON.parse(readFileSync(join(src, "i18n/fr.json"), "utf8"))
const en = JSON.parse(readFileSync(join(src, "i18n/en.json"), "utf8"))
const stop = JSON.parse(readFileSync(join(root, "scripts/french-stopwords.json"), "utf8"))
const ignore = JSON.parse(readFileSync(join(root, "scripts/i18n-ignore.json"), "utf8"))
const errors = []

const frKeys = Object.keys(fr)
const enKeys = new Set(Object.keys(en))
for (const k of frKeys) if (!enKeys.has(k)) errors.push(`missing in en.json: ${k}`)
for (const k of enKeys) if (!(k in fr)) errors.push(`missing in fr.json: ${k}`)

const placeholders = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",")
const emoji = /\p{Extended_Pictographic}/u
const dash = /[–—]/
const frWords = new Set(stop.words)
const accents = new RegExp(`[${stop.accents}]`)
// Values that are the same in both languages by design (proper names, code names).
const sameOk = new Set(ignore.enAllowFrench ?? [])

for (const [name, cat] of [["fr", fr], ["en", en]]) {
  for (const [k, v] of Object.entries(cat)) {
    if (typeof v !== "string" || v === "") errors.push(`${name}.json: empty value for ${k}`)
    if (dash.test(v)) errors.push(`${name}.json: dash character in ${k}`)
    if (emoji.test(v)) errors.push(`${name}.json: emoji in ${k}`)
  }
}
for (const k of frKeys) {
  if (!(k in en)) continue
  if (placeholders(fr[k]) !== placeholders(en[k])) {
    errors.push(`placeholders differ for ${k}: fr {${placeholders(fr[k])}} en {${placeholders(en[k])}}`)
  }
  if (sameOk.has(k)) continue
  const words = en[k].toLowerCase().split(/[^\p{L}]+/u).filter(Boolean)
  const hit = words.filter((w) => frWords.has(w))
  if (hit.length) errors.push(`French words in en.json ${k}: ${hit.join(", ")}`)
  else if (accents.test(en[k])) errors.push(`French accented letters in en.json ${k}: ${en[k]}`)
}

// Unused keys: a base key (without _one/_other) must appear as a string literal in a source file.
function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) {
      if (name === "__snapshots__") continue
      walk(p, out)
    } else if (/\.(ts|tsx)$/.test(name) && !/\.test\.(ts|tsx)$/.test(name)) out.push(p)
  }
  return out
}
const literals = new Set()
for (const file of walk(src)) {
  const text = readFileSync(file, "utf8")
  for (const m of text.matchAll(/["'`]([a-z0-9_]+(?:\.[a-z0-9_]+)+)["'`]/g)) literals.add(m[1])
}
const bases = new Set(frKeys.map((k) => k.replace(/_(one|other)$/, "")))
for (const b of bases) {
  if (!literals.has(b)) errors.push(`unused key: ${b} (referenced nowhere in ${relative(root, src)})`)
}

if (errors.length) {
  console.error(errors.join("\n"))
  console.error(`\n${errors.length} i18n problem(s)`)
  process.exit(1)
}
console.log(`i18n ok: ${frKeys.length} keys, fr and en in step`)
