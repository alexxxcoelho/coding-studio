#!/usr/bin/env node
// Finds the user-visible French strings of web/src and moves them to the
// catalogue src/i18n/fr.json, replacing each by a t("key") call.
//
//   node scripts/extract-strings.mjs            list what is left to translate (exit 1 if any)
//   node scripts/extract-strings.mjs --apply    rewrite the sources and add the keys to fr.json
//   node scripts/extract-strings.mjs --json     the same list as JSON
//
// What counts as a visible string: JSX text (text and {expressions} between two
// elements make one message with {name} variables), string attributes such as
// aria-label, title, placeholder, label, sub, hint; strings of toasts, errors and
// object properties that read as French (accents, French stop words, or one
// capitalised word). Skipped: code tokens, class names, comparisons, imports,
// anything under <code>, <pre>, <kbd>, data-no-i18n or data-verbatim, and the
// exact strings listed in scripts/i18n-ignore.json.
//
// Strings evaluated at module load (tables of labels in lib/) become getters so
// that they follow the language. Strings it cannot place safely are reported as
// "manual" and left in French.

import fs from "node:fs"
import path from "node:path"
import ts from "typescript"

const ROOT = path.resolve(import.meta.dirname, "..")
const SRC = path.join(ROOT, "src")
const FR_PATH = path.join(SRC, "i18n/fr.json")
const args = new Set(process.argv.slice(2))
const APPLY = args.has("--apply")
const AS_JSON = args.has("--json")

const IGNORE = new Set(JSON.parse(fs.readFileSync(path.join(import.meta.dirname, "i18n-ignore.json"), "utf8")).exact)
const SKIP_FILES = /(^|\/)(i18n\/|.*\.test\.tsx?$|test-setup\.ts|test-utils\.tsx|main\.tsx|lib\/analysis-fixtures\.ts|lib\/memo\.ts|vite-env\.d\.ts)/

// ------------------------------------------------------------------ French?

const STOP = new Set(
  ("le la les un une des du de d l et ou à au aux en dans pour par sur avec sans ce cet cette ces se sa son ses est sont ne pas plus que qui quoi dont où y il elle ils elles nous vous " +
    "votre vos notre nos leur leurs aucun aucune tous toutes tout toute rien oui non encore déjà ici mais donc car si comme entre vers chez sous ni ont être avoir fait peut doit " +
    "n qu s j m c ça cela celui celle ceux sera été faire voir seul seule seuls vide aucune ").split(/\s+/),
)

/** Tokens of a string, split on anything that is not a letter (apostrophes split: l'étude -> l, étude). */
function tokens(s) {
  return s.toLowerCase().match(/[a-zà-ÿœ]+/g) ?? []
}

/** Strict test: reads as French on its own (accent, or a stop word beside another word). */
function readsFrench(s) {
  if (/[àâäçéèêëîïôöùûüÿœÀÂÇÉÈÊËÎÏÔÙÛÜŒ]/.test(s)) return true
  const tk = tokens(s)
  if (tk.length < 2) return tk.length === 1 && STOP.has(tk[0]) && tk[0].length > 2
  return tk.some((w) => STOP.has(w) && w.length > 1) || (/^[A-ZÉ]/.test(s) && /\s/.test(s) && !/[{}=;<>/\\]/.test(s) && !/-/.test(s.split(/\s+/)[0]))
}

const CAPITALISED_WORD = /^[A-ZÉÀ][a-zà-ÿ]{2,}$/

const CSS_PROPS = new Set(
  ("margin padding gap width height top left right bottom border borderLeft borderRight borderTop borderBottom background color gridTemplateColumns gridTemplateRows flex display position " +
    "fontSize fontFamily fontWeight textAlign whiteSpace lineHeight maxWidth minWidth maxHeight minHeight boxShadow transition transform opacity overflow cursor outline zIndex " +
    "cls className class variant size side align default destructive secondary outline ghost link xs sm lg line key id type role href src name value kind tone mode d stroke fill").split(/\s+/),
)

const TEXT_ATTRS = new Set(
  ("aria-label aria-description aria-roledescription title placeholder alt label sub aside hint note empty heading description footer summary tip caption text message confirm cancel " +
    "tooltip legend subtitle emptyText loadingText ok").split(/\s+/),
)

const SKIP_ATTRS = new Set(
  ("className class style id key href src type role htmlFor name value defaultValue variant size side align d viewBox fill stroke xmlns data-testid data-slot to target rel " +
    "autoComplete inputMode pattern accept method encType onClick min max step rows cols wrap dir lang crossOrigin as asChild").split(/\s+/),
)

// ------------------------------------------------------------------ program

const configPath = path.join(ROOT, "tsconfig.app.json")
const cfg = ts.readConfigFile(configPath, ts.sys.readFile)
const parsed = ts.parseJsonConfigFileContent(cfg.config, ts.sys, ROOT)
const program = ts.createProgram(parsed.fileNames, { ...parsed.options, noEmit: true })
const checker = program.getTypeChecker()

const rel = (f) => path.relative(SRC, f).split(path.sep).join("/")
const files = program
  .getSourceFiles()
  .filter((sf) => sf.fileName.startsWith(SRC + path.sep) && !SKIP_FILES.test(rel(sf.fileName)))
  .sort((a, b) => a.fileName.localeCompare(b.fileName))

// ------------------------------------------------------------------ helpers

const ENTITIES = { nbsp: " ", apos: "'", quot: '"', amp: "&", lt: "<", gt: ">", hellip: "…", laquo: "«", raquo: "»", middot: "·", rsquo: "’", lsquo: "‘", eacute: "é", egrave: "è", agrave: "à" }
function decodeEntities(s) {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) => {
    if (e[0] === "#") return String.fromCodePoint(e[1] === "x" || e[1] === "X" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10))
    return ENTITIES[e] ?? m
  })
}

/** React's whitespace rules for JSX text. */
function cleanJsxText(raw) {
  const lines = raw.split(/\r\n|\n|\r/)
  let lastNonEmpty = 0
  lines.forEach((l, i) => {
    if (/[^ \t]/.test(l)) lastNonEmpty = i
  })
  let str = ""
  lines.forEach((line, i) => {
    let trimmed = line.replace(/\t/g, " ")
    if (i !== 0) trimmed = trimmed.replace(/^ +/, "")
    if (i !== lines.length - 1) trimmed = trimmed.replace(/ +$/, "")
    if (trimmed) {
      if (i !== lastNonEmpty) trimmed += " "
      str += trimmed
    }
  })
  return decodeEntities(str)
}

function nsOf(file) {
  const parts = rel(file).replace(/\.tsx?$/, "").split("/").filter((p) => !["components", "pages", "lib"].includes(p))
  return parts.map((p) => p.replace(/-/g, "_")).join(".")
}

function slug(text) {
  const base = text
    .replace(/\{(\w+)\}/g, " $1 ")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 5)
    .join("_")
    .slice(0, 36)
    .replace(/_$/, "")
  return base || "text"
}

function hasJsx(node) {
  let found = false
  const visit = (n) => {
    if (found) return
    if (ts.isJsxElement(n) || ts.isJsxSelfClosingElement(n) || ts.isJsxFragment(n)) {
      found = true
      return
    }
    ts.forEachChild(n, visit)
  }
  visit(node)
  return found
}

const unwrap = (e) => {
  while (ts.isParenthesizedExpression(e) || ts.isNonNullExpression(e) || ts.isAsExpression(e)) e = e.expression
  return e
}

/** True when the expression's type is made of strings and numbers only. */
function isTextual(expr) {
  if (hasJsx(expr)) return false
  const type = checker.getTypeAtLocation(expr)
  const parts = type.isUnion() ? type.types : [type]
  const ok = parts.filter((p) => !(p.flags & (ts.TypeFlags.Undefined | ts.TypeFlags.Null)))
  return ok.length > 0 && ok.every((p) => p.flags & (ts.TypeFlags.StringLike | ts.TypeFlags.NumberLike))
}

const PLURAL_TRUE_PLURAL = [/^(.+?)\s*>\s*1$/, /^(.+?)\s*>=\s*2$/, /^(.+?)\s*!==?\s*1$/]
const PLURAL_TRUE_SINGULAR = [/^(.+?)\s*===?\s*1$/, /^(.+?)\s*<\s*2$/, /^(.+?)\s*<=\s*1$/]

/** {counter: node, whenTrue: "one"|"other"} for `n > 1 ? "s" : ""` style conditions. */
function pluralCondition(cond) {
  const text = unwrap(cond).getText()
  for (const re of PLURAL_TRUE_PLURAL) {
    const m = text.match(re)
    if (m) return { counter: m[1], whenTrue: "other" }
  }
  for (const re of PLURAL_TRUE_SINGULAR) {
    const m = text.match(re)
    if (m) return { counter: m[1], whenTrue: "one" }
  }
  return null
}

const stringOf = (e) => {
  e = unwrap(e)
  return ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e) ? e.text : null
}

/** Declarations named `t` and the ranges where they hide the import. */
function shadowRanges(sf) {
  const ranges = []
  const visit = (n) => {
    if ((ts.isParameter(n) || ts.isVariableDeclaration(n) || ts.isBindingElement(n) || ts.isFunctionDeclaration(n) || ts.isImportSpecifier(n) || ts.isImportClause(n)) && n.name && ts.isIdentifier(n.name) && n.name.text === "t") {
      let scope = n.parent
      while (scope && !(ts.isFunctionLike(scope) || ts.isBlock(scope) || ts.isSourceFile(scope) || ts.isCatchClause(scope) || ts.isForStatement(scope) || ts.isForOfStatement(scope) || ts.isForInStatement(scope))) scope = scope.parent
      ranges.push([scope.getStart(), scope.getEnd()])
    }
    ts.forEachChild(n, visit)
  }
  visit(sf)
  return ranges
}

/** Does evaluating `node` happen at module load (outside any function)? */
function atModuleLevel(node) {
  for (let p = node.parent; p; p = p.parent) {
    if (ts.isFunctionLike(p) || ts.isClassLike(p)) return false
    if (ts.isSourceFile(p)) return true
  }
  return false
}

// -------------------------------------------------------------- collection

/** A French message found in a file. forms: {one, other} or {text}. */
function collect(sf) {
  const found = []
  const shadows = shadowRanges(sf)
  const consumed = []
  const isConsumed = (n) => consumed.some(([a, b]) => n.getStart() >= a && n.getEnd() <= b)
  const inShadow = (n) => shadows.some(([a, b]) => n.getStart() >= a && n.getEnd() <= b)

  /** Pattern pieces -> forms. pieces: [{lit}|{expr, name}|{plural: cond info, a, b}] */
  function build(pieces) {
    const vars = new Map() // name -> source text
    const nameFor = (expr, hint) => {
      const text = expr.getText()
      for (const [k, v] of vars) if (v === text) return k
      let base = hint
      if (!base) {
        const e = unwrap(expr)
        if (ts.isIdentifier(e)) base = e.text
        else if (ts.isPropertyAccessExpression(e)) base = e.name.text
        else if (ts.isCallExpression(e)) {
          const c = unwrap(e.expression)
          base = ts.isIdentifier(c) ? c.text : ts.isPropertyAccessExpression(c) ? c.name.text : "v"
        } else base = "v"
      }
      base = base.replace(/[^A-Za-z0-9_]/g, "") || "v"
      if (/^\d/.test(base)) base = "v" + base
      let name = base
      for (let i = 2; vars.has(name); i++) name = base + i
      vars.set(name, text)
      return name
    }
    const hasPlural = pieces.some((p) => p.plural)
    let counterText = null
    for (const p of pieces) if (p.plural) counterText ??= p.plural.counter
    if (pieces.some((p) => p.plural && p.plural.counter !== counterText)) return null
    if (hasPlural) vars.set("n", counterText)
    const render = (form) =>
      pieces
        .map((p) => {
          if (p.lit !== undefined) return p.lit
          if (p.plural) {
            if (p.plural.counter !== counterText) return null
            const branch = p.plural.whenTrue === form ? p.a : p.b
            return branch
          }
          if (hasPlural && p.expr.getText() === counterText) return "{n}"
          return `{${nameFor(p.expr)}}`
        })
        .join("")
    if (!hasPlural) {
      const text = render("other")
      return { forms: { text }, vars }
    }
    const one = render("one")
    const other = render("other")
    if (one.includes("\u0000")) return null
    // `n` must appear as a variable: it does when counter is also printed; else it is only passed.
    return { forms: { one, other }, vars }
  }

  function addCandidate(c) {
    if (c.forms) {
      const all = Object.values(c.forms)
      if (all.every((t) => IGNORE.has(t.trim()))) return
      if (all.some((t) => t === null)) return
    }
    found.push(c)
  }

  function replacementCall(node, vars, kind) {
    return { node, vars, kind }
  }

  // ---- JSX text runs
  function skipJsxAncestors(el) {
    for (let p = el; p; p = p.parent) {
      if (ts.isJsxElement(p) || ts.isJsxSelfClosingElement(p)) {
        const open = ts.isJsxElement(p) ? p.openingElement : p
        const tag = open.tagName.getText()
        if (["code", "pre", "kbd", "script", "style"].includes(tag)) return true
        for (const a of open.attributes.properties) {
          if (ts.isJsxAttribute(a) && ["data-no-i18n", "data-verbatim"].includes(a.name.getText())) return true
        }
      }
    }
    return false
  }

  function handleChildren(parent, children) {
    if (skipJsxAncestors(parent)) return
    let run = []
    const flush = () => {
      const items = run
      run = []
      // Trim empty edges.
      while (items.length && items[0].empty) items.shift()
      while (items.length && items[items.length - 1].empty) items.pop()
      if (!items.length) return
      const litText = items.filter((i) => i.lit !== undefined).map((i) => i.lit).join("")
      if (!/[A-Za-zÀ-ÿ]{2}/.test(litText)) return
      // Leading / trailing blanks stay outside the message.
      let lead = ""
      let trail = ""
      const first = items[0]
      const last = items[items.length - 1]
      const pieces = items.filter((i) => !i.empty).map((i) => ({ ...i }))
      if (pieces[0].lit !== undefined) {
        const m = pieces[0].lit.match(/^\s+/)
        if (m) {
          lead = m[0]
          pieces[0].lit = pieces[0].lit.slice(m[0].length)
        }
      }
      const lp = pieces[pieces.length - 1]
      if (lp.lit !== undefined) {
        const m = lp.lit.match(/\s+$/)
        if (m) {
          trail = m[0]
          lp.lit = lp.lit.slice(0, -m[0].length)
        }
      }
      const pure = pieces.filter((p) => p.lit === undefined || p.lit !== "")
      const built = build(pure)
      if (!built) return
      const nodes = [first.node, last.node]
      addCandidate({ kind: "jsx", sf, start: nodes[0].getStart(), end: nodes[1].getEnd(), anchor: nodes[0], forms: built.forms, vars: built.vars, lead, trail })
    }
    for (const child of children) {
      if (ts.isJsxText(child)) {
        const text = cleanJsxText(child.text)
        run.push({ node: child, lit: text, empty: text === "" })
      } else if (ts.isJsxExpression(child)) {
        const e = child.expression
        if (!e) {
          flush()
          continue
        }
        const u = unwrap(e)
        const s = stringOf(u)
        if (s !== null) {
          run.push({ node: child, lit: s, empty: false })
        } else if (ts.isConditionalExpression(u) && pluralCondition(u.condition) && stringOf(u.whenTrue) !== null && stringOf(u.whenFalse) !== null) {
          const pc = pluralCondition(u.condition)
          const a = stringOf(u.whenTrue)
          const b = stringOf(u.whenFalse)
          run.push({ node: child, plural: pc, a, b, empty: false })
        } else if (!ts.isConditionalExpression(u) && !ts.isBinaryExpression(u) && !ts.isTemplateExpression(u) && isTextual(u)) {
          run.push({ node: child, expr: u, empty: false })
        } else if (ts.isBinaryExpression(u) && [ts.SyntaxKind.PlusToken, ts.SyntaxKind.MinusToken, ts.SyntaxKind.AsteriskToken, ts.SyntaxKind.SlashToken].includes(u.operatorToken.kind) && isTextual(u)) {
          run.push({ node: child, expr: u, empty: false })
        } else flush()
      } else flush()
    }
    flush()
  }

  // ---- string literals / templates
  function positionOf(lit) {
    // Returns {ctx, strict} or null when the position is not user-visible.
    const p = lit.parent
    if (ts.isImportDeclaration(p) || ts.isExportDeclaration(p) || ts.isLiteralTypeNode(p) || ts.isCaseClause(p) || ts.isExternalModuleReference(p)) return null
    if (ts.isJsxAttribute(p)) {
      const name = p.name.getText()
      if (SKIP_ATTRS.has(name) || name.startsWith("data-")) return null
      return { ctx: "attr:" + name, strict: !TEXT_ATTRS.has(name) }
    }
    if (ts.isJsxExpression(p) && ts.isJsxAttribute(p.parent)) {
      const name = p.parent.name.getText()
      if (SKIP_ATTRS.has(name) || name.startsWith("data-")) return null
      return { ctx: "attr:" + name, strict: !TEXT_ATTRS.has(name), wrapped: true }
    }
    if (ts.isJsxExpression(p) && (ts.isJsxElement(p.parent) || ts.isJsxFragment(p.parent))) return { ctx: "child", strict: true, wrapped: true }
    if (ts.isPropertyAssignment(p) && p.initializer === lit) {
      const name = p.name.getText().replace(/^["']|["']$/g, "")
      if (CSS_PROPS.has(name) || (ts.isIdentifier(p.name) && p.name.text.startsWith("grid"))) return null
      return { ctx: "prop:" + name, strict: false, prop: p }
    }
    if (ts.isPropertyAssignment(p)) return null
    if (ts.isCallExpression(p) || ts.isNewExpression(p)) {
      const callee = p.expression.getText()
      if (/^(cn|cva|clsx|twMerge|tw|require|import)$/.test(callee) || /(matchMedia|startsWith|endsWith|includes|split|join|replace|replaceAll|querySelector|querySelectorAll|getItem|setItem|removeItem|getAttribute|setAttribute|addEventListener|indexOf|has|get|set|delete|test|match|padStart|padEnd|localeCompare|normalize|toLocale\w*|Intl\.\w+|URLSearchParams|fetch|json|stringify|parse|key|on|off|emit|useQuery|queryKey|invalidateQueries)$/.test(callee.split(".").pop())) return null
      if (/^(toast(\.\w+)?|reportError|notify\w*|new Error|Error|setError|setMessage|setMsg|listProblem|issues\.push|api\.pick|confirm|alert|done|run|flash|say|announce)$/.test(callee) || /^toast/.test(callee)) return { ctx: "arg:" + callee, strict: false }
      if (/^api\./.test(callee)) return null // data sent to the server stays as written
      return { ctx: "arg:" + callee, strict: true }
    }
    if (ts.isBinaryExpression(p)) {
      const op = p.operatorToken.kind
      if ([ts.SyntaxKind.EqualsEqualsToken, ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(op)) return null
      return { ctx: "bin", strict: true }
    }
    if (ts.isElementAccessExpression(p) || ts.isComputedPropertyName(p) || ts.isEnumMember(p) || ts.isTypeAliasDeclaration(p) || ts.isPropertySignature(p) || ts.isTemplateSpan(p)) return null
    if (ts.isReturnStatement(p) || ts.isConditionalExpression(p) || ts.isParenthesizedExpression(p) || ts.isArrowFunction(p) || ts.isArrayLiteralExpression(p) || ts.isVariableDeclaration(p) || ts.isAsExpression(p) || ts.isSatisfiesExpression(p) || ts.isSpreadElement(p) || ts.isBindingElement(p) || ts.isParameter(p) || ts.isShorthandPropertyAssignment(p) || ts.isThrowStatement(p) || ts.isNonNullExpression(p) || ts.isExpressionStatement(p)) {
      if (ts.isExpressionStatement(p)) return null // "use client"
      if (ts.isConditionalExpression(p) && p.condition === lit) return null
      return { ctx: "expr:" + ts.SyntaxKind[p.kind], strict: true }
    }
    return null
  }

  function stringCandidate(lit, text) {
    if (isConsumed(lit)) return
    if (IGNORE.has(text) || !/[A-Za-zÀ-ÿ]/.test(text)) return
    const pos = positionOf(lit)
    if (!pos) return
    let ok = readsFrench(text)
    if (!ok && !pos.strict) ok = CAPITALISED_WORD.test(text) || (tokens(text).length === 1 && STOP.has(tokens(text)[0]))
    if (!ok && (pos.ctx.startsWith("prop:") || pos.ctx.startsWith("arg:")) && CAPITALISED_WORD.test(text) && !CSS_PROPS.has(text)) ok = false
    if (!ok) return
    if (/^[\w./:#@-]+$/.test(text) && !CAPITALISED_WORD.test(text)) return
    // Strings that are nothing but a code token or a template of one.
    if (/^(use client)$/.test(text) || /gradient|^[MmLlHhVvCcSsQqTtAaZz0-9 .,-]+$/.test(text)) return
    found.push({ kind: "str", sf, node: lit, ctx: pos.ctx, wrapped: !!pos.wrapped, prop: pos.prop, forms: { text }, vars: new Map() })
  }

  function templateCandidate(tpl) {
    if (isConsumed(tpl)) return
    const p = tpl.parent
    // Never inside a className or similar.
    if (ts.isJsxExpression(p) && ts.isJsxAttribute(p.parent) && (SKIP_ATTRS.has(p.parent.name.getText()) || p.parent.name.getText().startsWith("data-"))) return
    if (ts.isCallExpression(p) && /^(cn|cva|clsx|useQuery|fetch)$/.test(p.expression.getText())) return
    if (ts.isElementAccessExpression(p) || ts.isPropertyAssignment(p) && CSS_PROPS.has(p.name.getText())) return
    const pieces = []
    const joined = [tpl.head.text, ...tpl.templateSpans.map((s) => s.literal.text)].join("\u0001")
    const parts = joined.split("\u0001")
    const litText = parts.join(" ")
    if (!/[A-Za-zÀ-ÿ]{2}/.test(litText)) return
    if (/var\(|calc\(|rgb|srgb|gradient|\bpx\b|#[0-9a-f]{3}|^\s*[#/]|\/[a-z]|\?[a-z]+=|^[MmLlHhVvCcSsQqTtAaZz0-9 .,-]+$|^Request failed/i.test(litText)) return
    if (tpl.templateSpans.some((sp) => /^(class|cls|style)/i.test(unwrap(sp.expression).getText().split(".").pop()))) return
    const hasPluralSpan = tpl.templateSpans.some((sp) => ts.isConditionalExpression(unwrap(sp.expression)) && pluralCondition(unwrap(sp.expression).condition))
    const frenchish = readsFrench(litText.replace(/\s+/g, " ").trim()) || hasPluralSpan || parts.some((x) => /^\s[a-zà-ÿ]{4,}$/.test(x))
    if (!frenchish) return
    pieces.push({ lit: tpl.head.text })
    for (const span of tpl.templateSpans) {
      const e = unwrap(span.expression)
      const s = stringOf(e)
      if (s !== null) pieces.push({ lit: s })
      else if (ts.isConditionalExpression(e) && pluralCondition(e.condition) && stringOf(e.whenTrue) !== null && stringOf(e.whenFalse) !== null) {
        pieces.push({ plural: pluralCondition(e.condition), a: stringOf(e.whenTrue), b: stringOf(e.whenFalse) })
      } else if (isTextual(e) && !ts.isConditionalExpression(e)) pieces.push({ expr: e })
      else return // a conditional or something we cannot name: leave it to the inner literals
      pieces.push({ lit: span.literal.text })
    }
    const merged = []
    for (const pc of pieces) {
      const last = merged[merged.length - 1]
      if (pc.lit !== undefined && last && last.lit !== undefined) last.lit += pc.lit
      else merged.push({ ...pc })
    }
    const built = build(merged)
    if (!built) return
    if (!positionOk(tpl)) return
    consumed.push([tpl.getStart(), tpl.getEnd()])
    addCandidate({ kind: "tpl", sf, node: tpl, forms: built.forms, vars: built.vars, wrapped: ts.isJsxExpression(p) })
  }

  const positionOk = (tpl) => {
    const p = tpl.parent
    if (ts.isTaggedTemplateExpression(p)) return false
    if (ts.isPropertyAssignment(p) && (CSS_PROPS.has(p.name.getText()) || p.name.getText().startsWith("grid"))) return false
    if (ts.isBinaryExpression(p) && [ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(p.operatorToken.kind)) return false
    if (ts.isCallExpression(p) && /^(api\.|fetch|cn$|cva$)/.test(p.expression.getText())) return false
    return true
  }

  // ---- plural helper: plural(n, "passage", "passages")
  function pluralCall(call) {
    if (!ts.isIdentifier(call.expression) || call.expression.text !== "plural") return false
    const [n, one, many] = call.arguments
    if (!n || !one || stringOf(one) === null) return false
    if (call.arguments.length > 3) return false
    const o = stringOf(one)
    const m = many ? stringOf(many) : o + "s"
    if (m === null) return false
    if (!readsFrench(o) && !CAPITALISED_WORD.test(o) && !/^[a-zà-ÿ'’ -]{3,}$/.test(o)) return false
    consumed.push([call.getStart(), call.getEnd()])
    found.push({ kind: "call", sf, node: call, forms: { one: `{n} ${o}`, other: `{n} ${m}` }, vars: new Map([["n", n.getText()]]) })
    return true
  }

  // JSX runs consume their children (so the literals inside are not collected twice).
  const before = () => found.length
  const jsxVisit = (n) => {
    const start = before()
    if (ts.isJsxElement(n) || ts.isJsxFragment(n)) {
      handleChildren(n, n.children)
      for (const c of found.slice(start)) if (c.kind === "jsx") consumed.push([c.start, c.end])
    }
    ts.forEachChild(n, jsxVisit)
  }
  jsxVisit(sf)
  // handleChildren ran once already; collect the rest without re-running it.
  const visit2 = (n) => {
    if (isConsumed(n) && !ts.isSourceFile(n)) return
    if (ts.isCallExpression(n) && pluralCall(n)) return
    if (ts.isTemplateExpression(n)) templateCandidate(n)
    else if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) stringCandidate(n, n.text)
    ts.forEachChild(n, visit2)
  }
  visit2(sf)
  return { found, shadows, inShadow }
}

// ----------------------------------------------------------------- catalogue

const fr = fs.existsSync(FR_PATH) ? JSON.parse(fs.readFileSync(FR_PATH, "utf8")) : {}
const frByText = new Map() // "ns\0form-signature" -> key
for (const [k, v] of Object.entries(fr)) frByText.set(v, [...(frByText.get(v) ?? []), k])

const results = files.map((sf) => ({ sf, ...collect(sf) }))

// Texts that occur in several files go to common.*
const occurrences = new Map()
for (const r of results)
  for (const c of r.found) {
    const sig = JSON.stringify(c.forms)
    if (!occurrences.has(sig)) occurrences.set(sig, new Set())
    occurrences.get(sig).add(rel(r.sf.fileName))
  }

const newKeys = {} // key -> value
const keyOfSig = new Map() // ns|sig -> key
function keyFor(ns, c) {
  const sig = JSON.stringify(c.forms)
  const shared = occurrences.get(sig).size >= 2 && Object.values(c.forms).join("").length <= 90
  const space = shared ? "common" : ns
  const id = space + "|" + sig
  if (keyOfSig.has(id)) return keyOfSig.get(id)
  const text = c.forms.text ?? c.forms.other
  const base = `${space}.${slug(text)}`
  let key = base
  const same = (k) => {
    const forms = c.forms
    if (forms.text !== undefined) return fr[k] === forms.text || newKeys[k] === forms.text
    return (fr[k + "_one"] ?? newKeys[k + "_one"]) === forms.one && (fr[k + "_other"] ?? newKeys[k + "_other"]) === forms.other
  }
  const taken = (k) => fr[k] !== undefined || newKeys[k] !== undefined || fr[k + "_one"] !== undefined || newKeys[k + "_one"] !== undefined
  for (let i = 2; taken(key) && !same(key); i++) key = `${base}_${i}`
  if (c.forms.text !== undefined) newKeys[key] = c.forms.text
  else {
    newKeys[key + "_one"] = c.forms.one
    newKeys[key + "_other"] = c.forms.other
  }
  keyOfSig.set(id, key)
  return key
}

// -------------------------------------------------------------------- output

// Local variables named `t` would hide the import: rename them (in the files that need it).
if (args.has("--rename-t")) {
  let renamedFiles = 0
  for (const r of results) {
    if (!r.found.some((c) => r.inShadow(c.node ?? c.anchor))) continue
    const sf = r.sf
    const name = /\btt\b/.test(sf.getFullText()) ? "tt2" : "tt"
    const edits = []
    const visit = (n) => {
      if (ts.isIdentifier(n) && n.text === "t") {
        const p = n.parent
        let sym = null
        let shorthand = false
        if (ts.isShorthandPropertyAssignment(p) && p.name === n) {
          sym = checker.getShorthandAssignmentValueSymbol(p)
          shorthand = true
        } else if (ts.isPropertyAccessExpression(p) && p.name === n) sym = null
        else if ((ts.isPropertyAssignment(p) || ts.isPropertySignature(p) || ts.isPropertyDeclaration(p) || ts.isMethodDeclaration(p) || ts.isEnumMember(p) || ts.isJsxAttribute(p)) && p.name === n) sym = null
        else if (ts.isBindingElement(p) && p.propertyName === n) sym = null
        else {
          sym = checker.getSymbolAtLocation(n)
          if (ts.isBindingElement(p) && p.name === n && !p.propertyName && ts.isObjectBindingPattern(p.parent)) shorthand = true
        }
        if (sym?.declarations?.some((d) => d.getSourceFile() === sf && (ts.isParameter(d) || ts.isVariableDeclaration(d) || ts.isBindingElement(d) || ts.isFunctionDeclaration(d)))) {
          edits.push([n.getStart(), n.getEnd(), shorthand ? `t: ${name}` : name])
        }
      }
      ts.forEachChild(n, visit)
    }
    visit(sf)
    let out = sf.getFullText()
    for (const [a, b, rep] of edits.sort((x, y) => y[0] - x[0])) out = out.slice(0, a) + rep + out.slice(b)
    fs.writeFileSync(sf.fileName, out)
    renamedFiles++
  }
  console.log(`${renamedFiles} files: local variables named t renamed`)
  process.exit(0)
}

const manual = []
let applied = 0
const listing = []

for (const r of results) {
  const { sf, found } = r
  if (!found.length) continue
  const ns = nsOf(sf.fileName)
  const text = sf.getFullText()
  const edits = []
  for (const c of found.sort((a, b) => (a.node ?? a.anchor).getStart() - (b.node ?? b.anchor).getStart())) {
    const anchor = c.node ?? c.anchor
    const { line } = sf.getLineAndCharacterOfPosition(anchor.getStart())
    const shown = c.forms.text ?? `${c.forms.one} | ${c.forms.other}`
    listing.push({ file: rel(sf.fileName), line: line + 1, kind: c.kind + (c.ctx ? ":" + c.ctx : ""), text: shown })
    if (r.inShadow(anchor)) {
      manual.push({ file: rel(sf.fileName), line: line + 1, why: "a local variable named t hides the import", text: shown })
      continue
    }
    const varsObj = c.vars.size
      ? ", { " + [...c.vars].map(([name, src]) => (name === src ? name : `${name}: ${src}`)).join(", ") + " }"
      : ""
    if (c.forms.text === undefined && !c.vars.has("n")) {
      manual.push({ file: rel(sf.fileName), line: line + 1, why: "plural without counter", text: shown })
      continue
    }
    const moduleLevel = atModuleLevel(anchor)
    let range
    let replacement
    const key = APPLY ? keyFor(ns, c) : "…"
    const call = `t(${JSON.stringify(key)}${varsObj})`
    if (c.kind === "jsx") {
      range = [c.start, c.end]
      const lead = c.lead ? '{" "}' : ""
      const trail = c.trail ? '{" "}' : ""
      replacement = `${lead}{${call}}${trail}`
      if (moduleLevel) {
        manual.push({ file: rel(sf.fileName), line: line + 1, why: "module level JSX", text: shown })
        continue
      }
    } else {
      const lit = c.node
      const p = lit.parent
      range = [lit.getStart(), lit.getEnd()]
      if (ts.isJsxAttribute(p)) replacement = `{${call}}`
      else if (ts.isJsxExpression(p) || !c.wrapped) replacement = call
      else replacement = call
      if (moduleLevel) {
        if (c.prop && ts.isPropertyAssignment(p) && p.initializer === lit) {
          range = [p.getStart(), p.getEnd()]
          replacement = `get ${p.name.getText()}() { return ${call} }`
        } else {
          manual.push({ file: rel(sf.fileName), line: line + 1, why: "evaluated at module load", text: shown })
          continue
        }
      }
    }
    edits.push({ range, replacement })
    applied++
  }
  if (APPLY && edits.length) {
    let out = text
    for (const e of edits.sort((a, b) => b.range[0] - a.range[0])) out = out.slice(0, e.range[0]) + e.replacement + out.slice(e.range[1])
    if (!/from "@\/i18n"/.test(out)) {
      const imports = [...sf.statements].filter(ts.isImportDeclaration)
      const last = imports[imports.length - 1]
      const stmt = 'import { t } from "@/i18n"\n'
      if (last) {
        const pos = last.getEnd()
        out = out.slice(0, pos) + "\n" + stmt.trimEnd() + out.slice(pos)
      } else {
        // After the header comment block, if any.
        const m = out.match(/^(\/\/.*\n)+\n?/)
        const at = m ? m[0].length : 0
        out = out.slice(0, at) + stmt + (m ? "" : "\n") + out.slice(at)
      }
    }
    fs.writeFileSync(sf.fileName, out)
  }
}

if (APPLY) {
  const merged = { ...fr, ...newKeys }
  const sorted = Object.fromEntries(Object.keys(merged).sort().map((k) => [k, merged[k]]))
  fs.writeFileSync(FR_PATH, JSON.stringify(sorted, null, 2) + "\n")
}

if (AS_JSON) {
  console.log(JSON.stringify({ found: listing, manual }, null, 2))
} else if (APPLY) {
  console.log(`${applied} strings replaced, ${Object.keys(newKeys).length} keys added, ${manual.length} left for hand work`)
  for (const m of manual) console.log(`manual ${m.file}:${m.line} (${m.why}) ${m.text.slice(0, 90)}`)
} else {
  for (const l of listing) console.log(`${l.file}:${l.line} [${l.kind}] ${l.text.replace(/\n/g, "\\n").slice(0, 120)}`)
  console.log(`${listing.length} French strings not yet in the catalogue`)
  for (const m of manual) console.log(`manual ${m.file}:${m.line} (${m.why}) ${m.text.slice(0, 90)}`)
  if (listing.length) process.exitCode = 1
}
