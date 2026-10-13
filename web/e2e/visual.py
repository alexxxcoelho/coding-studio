#!/usr/bin/env python3
"""Visual check of the Coding Studio web UI.

Captures every screen in 6 appearance combinations (system light/dark x app
choice system/light/dark) at 1440x900 and 1024x700, with the desktop shell
emulation (window.__CS_VIBRANCY__ and a stand-in material on <html>), then
runs three checks on each capture:

  1. axe-core (color-contrast and every "missing name or label" rule);
  2. text and background both dark or both light (luminance of each text node
     over its nearest opaque background);
  3. layout: horizontal page scroll, toolbar that wraps, element past the
     right edge of the window.

It writes, in --out: one contact sheet per route and viewport
(<route>_<viewport>.png), the captures (shots/), report.md and report.json.
Exit code 1 when a check fails.

  python3 e2e/visual.py --base http://127.0.0.1:7979 --out /tmp/visual
  python3 e2e/visual.py --only coding,codebook --viewports 1024x700 --combos light/light,dark/dark
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont
from playwright.sync_api import Page, sync_playwright

HERE = Path(__file__).resolve().parent
AXE = HERE.parent / "node_modules" / "axe-core" / "axe.min.js"
ENGLISH = json.loads((HERE / "english-words.json").read_text())

INTERVIEW = "INT-01"
VIEWPORTS = {"1440x900": (1440, 900), "1024x700": (1024, 700)}
SYSTEMS = ["light", "dark"]
CHOICES = ["system", "light", "dark"]
MATERIAL = {"light": "#e6e6e9", "dark": "#2c2c2e"}

AXE_RULES = [
    "color-contrast", "label", "button-name", "link-name", "select-name", "input-button-name", "input-image-alt",
    "aria-dialog-name", "aria-command-name", "aria-input-field-name", "aria-toggle-field-name", "aria-tooltip-name",
    "aria-meter-name", "aria-progressbar-name", "aria-treeitem-name", "image-alt", "frame-title", "document-title",
    "label-title-only", "form-field-multiple-labels",
]


# ---------------------------------------------------------------- routes

def _goto(hash_: str):
    def go(page: Page, base: str):
        page.goto(f"{base}/#{hash_}", wait_until="load")
        page.wait_for_selector("main", timeout=15000)
    return go


def _settle(page: Page):
    """Waits until the skeletons are gone (data loaded), then lets the layout rest."""
    try:
        page.wait_for_function("() => !document.querySelector('.animate-pulse, [data-slot=\"skeleton\"]')", timeout=5000)
    except Exception:
        pass
    page.wait_for_timeout(300)


def _open_settings_tab(label: str):
    def act(page: Page):
        page.get_by_role("button", name="Réglages").first.click()
        page.get_by_role("dialog").wait_for()
        if label != "Général":
            page.get_by_role("tab", name=label).click()
        page.wait_for_timeout(300)
    return act


def _open_help(page: Page):
    page.get_by_role("button", name="Aide pour cet écran").click()
    page.get_by_role("dialog").wait_for()
    page.wait_for_timeout(300)


def _selection_menu(page: Page):
    # Select the first words of a respondent turn that starts with plain text, as a drag would.
    ok = page.evaluate(
        """() => {
          for (const el of document.querySelectorAll('.u.resp [data-unit-text]')) {
            const t = el.firstChild;
            if (t && t.nodeType === 3 && t.nodeValue.length > 30) {
              const sel = getSelection();
              sel.setBaseAndExtent(t, 0, t, Math.min(40, t.nodeValue.length));
              el.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
              return true;
            }
          }
          return false;
        }"""
    )
    if not ok:
        raise RuntimeError("no respondent turn with plain text to select")
    page.get_by_role("menu", name="Coder la sélection").wait_for(timeout=4000)
    page.wait_for_timeout(250)


def _first_row(page: Page):
    rows = page.locator("table tbody tr")
    if rows.count():
        rows.first.click()
        page.wait_for_timeout(300)


ROUTES: list[dict] = [
    {"name": "library", "hash": "/interviews"},
    {"name": "library-fiche", "hash": "/interviews", "act": _first_row},
    {"name": "interview-sources", "hash": f"/interviews/{INTERVIEW}/sources"},
    {"name": "interview-coding", "hash": f"/interviews/{INTERVIEW}/coding?phase=2"},
    {"name": "interview-coding-phase1", "hash": f"/interviews/{INTERVIEW}/coding"},
    {"name": "interview-highlights", "hash": f"/interviews/{INTERVIEW}/highlights"},
    {"name": "interview-ai", "hash": f"/interviews/{INTERVIEW}/ai"},
    {"name": "interview-review", "hash": f"/interviews/{INTERVIEW}/review"},
    {"name": "interview-cases", "hash": f"/interviews/{INTERVIEW}/cases"},
    {"name": "interview-verification", "hash": f"/interviews/{INTERVIEW}/verification"},
    {"name": "interview-audit", "hash": f"/interviews/{INTERVIEW}/audit"},
    {"name": "coder-selection-menu", "hash": f"/interviews/{INTERVIEW}/coding?phase=2", "act": _selection_menu},
    {"name": "codebook", "hash": "/codebook"},
    {"name": "settings-study", "hash": "/settings"},
    {"name": "guide", "hash": "/guide"},
    {"name": "audit", "hash": "/audit"},
    {"name": "saturation", "hash": "/saturation"},
    {"name": "to-clarify", "hash": "/to-clarify"},
    {"name": "cross-case", "hash": "/cross-case"},
    {"name": "answers", "hash": "/answers"},
    {"name": "analyser", "hash": "/analyser"},
    {"name": "rediger", "hash": "/rediger"},
    {"name": "dialog-reglages-general", "hash": "/interviews", "act": _open_settings_tab("Général")},
    {"name": "dialog-reglages-providers", "hash": "/interviews", "act": _open_settings_tab("Fournisseurs IA")},
    {"name": "dialog-reglages-advanced", "hash": "/interviews", "act": _open_settings_tab("Avancé")},
    {"name": "help-drawer", "hash": f"/interviews/{INTERVIEW}/coding?phase=2", "act": _open_help},
]


# ---------------------------------------------------------------- in-page checks

LUMINANCE_JS = r"""
() => {
  const cv = document.createElement('canvas'); cv.width = cv.height = 1;
  const cx = cv.getContext('2d', { willReadFrequently: true });
  const cache = new Map();
  function rgba(css) {
    if (cache.has(css)) return cache.get(css);
    cx.clearRect(0, 0, 1, 1); cx.fillStyle = '#000'; cx.fillStyle = css; cx.fillRect(0, 0, 1, 1);
    const d = cx.getImageData(0, 0, 1, 1).data;
    // getImageData is premultiplied-free for fillRect on a cleared canvas: alpha stays in d[3].
    const a = d[3] / 255;
    const out = a === 0 ? [0, 0, 0, 0] : [d[0], d[1], d[2], a];
    cache.set(css, out); return out;
  }
  const over = (top, bot) => {
    const a = top[3] + bot[3] * (1 - top[3]);
    if (a === 0) return [0, 0, 0, 0];
    return [0, 1, 2].map(i => (top[i] * top[3] + bot[i] * bot[3] * (1 - top[3])) / a).concat([a]);
  };
  const lum = c => {
    const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
  };
  function backdrop(el) {
    const layers = [];
    for (let e = el; e; e = e.parentElement) {
      const cs = getComputedStyle(e);
      const c = rgba(cs.backgroundColor);
      if (c[3] > 0) layers.push(c);
      if (c[3] >= 0.999) break;
    }
    let base = layers.length && layers[layers.length - 1][3] >= 0.999 ? layers.pop() : null;
    if (!base) base = rgba(getComputedStyle(document.documentElement).backgroundColor);
    if (base[3] < 0.999) base = [255, 255, 255, 1];
    let acc = base;
    while (layers.length) acc = over(layers.pop(), acc);
    return acc;
  }
  const DARK = 0.18, LIGHT = 0.5;
  const found = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const seen = new Set();
  let n = 0;
  while (walker.nextNode()) {
    const t = walker.currentNode;
    if (!t.nodeValue || !t.nodeValue.trim()) continue;
    const el = t.parentElement;
    if (!el || seen.has(el)) continue;
    seen.add(el);
    if (el.closest('script,style,noscript,[hidden],[aria-hidden="true"]')) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === 'hidden' || cs.display === 'none' || parseFloat(cs.opacity) === 0) continue;
    // Text inside a field: the input's own value counts, the placeholder is skipped.
    let fg = rgba(cs.color);
    const bg = backdrop(el);
    fg = over(fg, bg);
    const lf = lum(fg), lb = lum(bg);
    n++;
    const bothDark = lf < DARK && lb < DARK, bothLight = lf > LIGHT && lb > LIGHT;
    if (bothDark || bothLight) {
      found.push({
        kind: bothDark ? 'both-dark' : 'both-light',
        text: t.nodeValue.trim().slice(0, 60),
        sel: el.tagName.toLowerCase() + (el.className && typeof el.className === 'string' ? '.' + el.className.trim().split(/\s+/).slice(0, 3).join('.') : ''),
        fg: 'rgb(' + fg.slice(0, 3).map(Math.round).join(',') + ')',
        bg: 'rgb(' + bg.slice(0, 3).map(Math.round).join(',') + ')',
      });
    }
  }
  return { checked: n, found };
}
"""

LAYOUT_JS = r"""
() => {
  const out = [];
  const de = document.documentElement;
  if (de.scrollWidth > window.innerWidth + 1) out.push({ kind: 'page-scroll-x', detail: `scrollWidth ${de.scrollWidth} > ${window.innerWidth}` });
  const tb = document.querySelector('.tb');
  if (tb) {
    const r = tb.getBoundingClientRect();
    if (r.height > 60) out.push({ kind: 'toolbar-wraps', detail: `toolbar height ${Math.round(r.height)}px` });
    for (const el of tb.querySelectorAll('button, a, input, label')) {
      const b = el.getBoundingClientRect();
      if (b.width && b.right > window.innerWidth + 1) out.push({ kind: 'toolbar-overflow', detail: (el.getAttribute('aria-label') || el.textContent || el.tagName).trim().slice(0, 40) + ` right ${Math.round(b.right)}` });
    }
  }
  const seen = new Set();
  for (const el of document.querySelectorAll('main *')) {
    const b = el.getBoundingClientRect();
    if (!b.width || !b.height || b.right <= window.innerWidth + 1) continue;
    // Ignore what a scroll container or a clipped ancestor already contains.
    let clipped = false;
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const ox = getComputedStyle(p).overflowX;
      if (ox !== 'visible') { const pb = p.getBoundingClientRect(); if (pb.right <= window.innerWidth + 1) { clipped = true; break; } }
    }
    if (clipped) continue;
    const key = el.tagName + (el.className || '');
    if (seen.has(key)) continue; seen.add(key);
    out.push({ kind: 'past-right-edge', detail: el.tagName.toLowerCase() + '.' + String(el.className).split(/\s+/).slice(0, 2).join('.') + ` right ${Math.round(b.right)}` });
    if (out.length > 12) break;
  }
  return out;
}
"""


ENGLISH_JS = r"""
({ words, skip }) => {
  const set = new Set(words);
  const found = new Map();
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {
    const t = walker.currentNode, el = t.parentElement;
    if (!el || el.closest(skip)) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    for (const tok of t.nodeValue.split(/\s+/)) {
      if (/[_0-9\-\/\.@]/.test(tok.replace(/[.,;:!?)]+$/, ''))) continue;
      const raw = tok.replace(/^[("«]+|[.,;:!?)"»]+$/g, ''); if (raw.length > 1 && raw === raw.toUpperCase()) continue; const w = raw.toLowerCase();
      if (set.has(w)) found.set(w, (found.get(w) || t.nodeValue.trim().slice(0, 70)));
    }
  }
  // Attributes read by the user: placeholders, labels and titles.
  for (const el of document.querySelectorAll('[placeholder],[aria-label],[title]')) {
    if (el.closest(skip.replace('input, ', '').replace('textarea, ', ''))) continue;
    for (const a of ['placeholder', 'aria-label', 'title']) {
      const v = el.getAttribute(a);
      if (!v) continue;
      for (const tok of v.split(/\s+/)) {
        if (/[_0-9\-\/\.@]/.test(tok.replace(/[.,;:!?)]+$/, ''))) continue;
        const raw = tok.replace(/^[("«]+|[.,;:!?)"»]+$/g, ''); if (raw.length > 1 && raw === raw.toUpperCase()) continue; const w = raw.toLowerCase();
        if (set.has(w)) found.set(w, a + ': ' + v.slice(0, 70));
      }
    }
  }
  return [...found.entries()];
}
"""


def run_axe(page: Page) -> list[dict]:
    page.evaluate(AXE.read_text())
    res = page.evaluate(
        "async (rules) => { const r = await axe.run(document, { runOnly: { type: 'rule', values: rules }, resultTypes: ['violations'] }); "
        "return r.violations.map(v => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.map(n => ({ target: n.target.join(' '), summary: (n.any[0] || n.all[0] || n.none[0] || {}).message || '' })) })); }",
        AXE_RULES,
    )
    return res


# ---------------------------------------------------------------- run

def effective(system: str, choice: str) -> str:
    return system if choice == "system" else choice


def combo_label(system: str, choice: str) -> str:
    return f"{system}/{choice}"


def capture(browser, base: str, out: Path, viewport: str, system: str, choice: str, routes: list[dict], results: list[dict]):
    w, h = VIEWPORTS[viewport]
    ctx = browser.new_context(viewport={"width": w, "height": h}, color_scheme=system, locale="fr-CH", reduced_motion="reduce")
    init = "window.__CS_VIBRANCY__ = true;"
    if choice != "system":
        init += f"try {{ localStorage.setItem('cs-theme', '{choice}') }} catch (e) {{}};"
    ctx.add_init_script(init)
    page = ctx.new_page()
    errors: list[str] = []
    page.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
    page.on("console", lambda m: errors.append(f"console.{m.type}: {m.text}") if m.type == "error" else None)
    page.on("response", lambda r: errors.append(f"HTTP {r.status} {r.url}") if r.status >= 500 else None)
    eff = effective(system, choice)
    for route in routes:
        errors.clear()
        entry = {"route": route["name"], "viewport": viewport, "system": system, "choice": choice, "problems": []}
        try:
            for attempt in (1, 2):  # the dev server sometimes answers a cold page late: one retry
                try:
                    page.goto("about:blank")  # a hash-only change would keep the previous screen and its dialogs
                    _goto(route["hash"])(page, base)
                    _settle(page)
                    break
                except Exception:
                    if attempt == 2:
                        raise
            page.evaluate(f"document.documentElement.style.background = '{MATERIAL[eff]}'")
            if "act" in route:
                route["act"](page)
            _settle(page)
            shot = out / "shots" / f"{route['name']}_{viewport}_{system}-{choice}.png"
            page.screenshot(path=str(shot))
            entry["shot"] = str(shot.relative_to(out))
            dark = page.evaluate("document.documentElement.classList.contains('dark')")
            if dark != (eff == "dark"):
                entry["problems"].append({"check": "theme", "detail": f"html.dark is {dark}, expected {eff == 'dark'}"})
            for v in run_axe(page):
                for n in v["nodes"]:
                    entry["problems"].append({"check": f"axe:{v['id']}", "detail": f"{n['target']} {n['summary']}"[:260]})
            lum = page.evaluate(LUMINANCE_JS)
            entry["text_nodes"] = lum["checked"]
            for f in lum["found"]:
                entry["problems"].append({"check": f"contrast:{f['kind']}", "detail": f"{f['sel']} \"{f['text']}\" text {f['fg']} on {f['bg']}"})
            for f in page.evaluate(LAYOUT_JS):
                entry["problems"].append({"check": f"layout:{f['kind']}", "detail": f["detail"]})
            for word, where in page.evaluate(ENGLISH_JS, {"words": ENGLISH["words"], "skip": ENGLISH["skip"]}):
                entry["problems"].append({"check": "i18n:english", "detail": f"\"{word}\" in: {where}"})
            for e in errors:
                if "404" in e and "Failed to load resource" in e:
                    continue  # a question without memo answers 404 by design, read as null
                entry["problems"].append({"check": "console", "detail": e[:240]})
        except Exception as e:  # a screen that cannot be reached is a failure too
            entry["problems"].append({"check": "capture", "detail": f"{type(e).__name__}: {str(e)[:200]}"})
        results.append(entry)
        n = len(entry["problems"])
        print(f"  {route['name']:28s} {viewport} {combo_label(system, choice):13s} {'ok' if n == 0 else str(n) + ' problem(s)'}", flush=True)
    ctx.close()


def font(size: int):
    for p in ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/System/Library/Fonts/Helvetica.ttc"):
        try:
            return ImageFont.truetype(p, size)
        except Exception:
            pass
    return ImageFont.load_default()


def contact_sheets(out: Path, results: list[dict], viewports: list[str], combos: list[tuple[str, str]]):
    by = {}
    for r in results:
        if "shot" in r:
            by[(r["route"], r["viewport"], r["system"], r["choice"])] = r
    names = sorted({r["route"] for r in results}, key=lambda n: [x["name"] for x in ROUTES].index(n))
    cols = CHOICES
    for name in names:
        for vp in viewports:
            tw = 720 if vp == "1440x900" else 600
            th = int(tw * VIEWPORTS[vp][1] / VIEWPORTS[vp][0])
            systems = [s for s in SYSTEMS if any((s, c) in combos for c in cols)]
            head = 26
            sheet = Image.new("RGB", (tw * len(cols) + 8 * (len(cols) + 1), (th + head) * len(systems) + 8 * (len(systems) + 1)), (128, 128, 128))
            d = ImageDraw.Draw(sheet)
            for ri, s in enumerate(systems):
                for ci, c in enumerate(cols):
                    r = by.get((name, vp, s, c))
                    x = 8 + ci * (tw + 8)
                    y = 8 + ri * (th + head + 8)
                    bad = len(r["problems"]) if r else 0
                    d.text((x, y + 4), f"systeme {s} / app {c}" + (f"   {bad} probleme(s)" if bad else ""), fill=(255, 220, 220) if bad else (255, 255, 255), font=font(14))
                    if r:
                        im = Image.open(out / r["shot"]).convert("RGB").resize((tw, th), Image.LANCZOS)
                        sheet.paste(im, (x, y + head))
            sheet.save(out / f"{name}_{vp}.png")


def write_report(out: Path, results: list[dict], base: str, seconds: float):
    total = sum(len(r["problems"]) for r in results)
    by_check: dict[str, int] = {}
    for r in results:
        for p in r["problems"]:
            by_check[p["check"]] = by_check.get(p["check"], 0) + 1
    lines = [f"# Contrôle visuel", "", f"Serveur : {base}. {len(results)} captures en {seconds:.0f} s. Problèmes : {total}.", ""]
    lines += ["## Par contrôle", ""] + [f"- {k} : {v}" for k, v in sorted(by_check.items())] + [""]
    lines += ["## Par écran", ""]
    per: dict[str, list[dict]] = {}
    for r in results:
        per.setdefault(r["route"], []).append(r)
    for name, rs in per.items():
        n = sum(len(r["problems"]) for r in rs)
        lines.append(f"### {name} ({n})")
        merged: dict[tuple[str, str], list[str]] = {}
        for r in rs:
            for p in r["problems"]:
                merged.setdefault((p["check"], re.sub(r"\s+", " ", p["detail"])), []).append(f"{r['viewport']} {r['system']}/{r['choice']}")
        for (check, detail), where in merged.items():
            lines.append(f"- {check}: {detail} [{len(where)} combinaison(s): {', '.join(where[:3])}{'…' if len(where) > 3 else ''}]")
        lines.append("")
    (out / "report.md").write_text("\n".join(lines))
    (out / "report.json").write_text(json.dumps({"base": base, "total": total, "by_check": by_check, "results": results}, indent=1, ensure_ascii=False))
    return total, by_check


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", default="http://127.0.0.1:7979")
    ap.add_argument("--out", default="/tmp/cs-visual")
    ap.add_argument("--only", default="", help="comma-separated route names (default: all)")
    ap.add_argument("--viewports", default=",".join(VIEWPORTS))
    ap.add_argument("--combos", default="", help="system/choice pairs, e.g. light/light,dark/system (default: the 6)")
    ap.add_argument("--list", action="store_true", help="list the routes and exit")
    a = ap.parse_args()
    if a.list:
        print("\n".join(r["name"] for r in ROUTES))
        return 0
    if not AXE.exists():
        print("axe-core missing: run `pnpm install` in web/", file=sys.stderr)
        return 2
    out = Path(a.out)
    (out / "shots").mkdir(parents=True, exist_ok=True)
    routes = [r for r in ROUTES if not a.only or r["name"] in a.only.split(",")]
    viewports = a.viewports.split(",")
    combos = [(s, c) for s in SYSTEMS for c in CHOICES]
    if a.combos:
        combos = [tuple(x.split("/")) for x in a.combos.split(",")]  # type: ignore[misc]
    t0 = time.time()
    results: list[dict] = []
    with sync_playwright() as p:
        browser = p.chromium.launch()
        for vp in viewports:
            for system, choice in combos:
                print(f"{vp} system {system}, app {choice}", flush=True)
                capture(browser, a.base.rstrip("/"), out, vp, system, choice, routes, results)
        browser.close()
    contact_sheets(out, results, viewports, combos)
    total, by_check = write_report(out, results, a.base, time.time() - t0)
    print(f"\n{len(results)} captures, {total} problems. Report: {out / 'report.md'}")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
