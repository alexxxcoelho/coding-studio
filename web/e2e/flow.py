#!/usr/bin/env python3
"""Full flow through the web UI in headless Chromium, on a seeded database.

Codes a passage, creates a code (and undoes it), marks a unit to review,
qualifies a passage, retains a theme with its checks, sets a matrix cell,
writes a part of the write-up, links a passage by the keyboard alternative,
draws the final audit, judges a claim and exports the Markdown.

  python3 e2e/seed.py http://127.0.0.1:7979     # once, on an empty database
  PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers python3 e2e/flow.py --base http://127.0.0.1:7979 --out /tmp/flow

Exit code 1 when a step fails. Screenshots of each step go to --out.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path

from playwright.sync_api import Page, expect, sync_playwright

IV = "INT-01"


def api(base: str, method: str, path: str, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, method=method, headers={"content-type": "application/json"})
    with urllib.request.urlopen(req) as r:
        raw = r.read()
        return json.loads(raw) if raw else None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://127.0.0.1:7979")
    ap.add_argument("--out", default="/tmp/cs-flow")
    args = ap.parse_args()
    base = args.base.rstrip("/")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    steps: list[tuple[str, str]] = []
    errors: list[str] = []

    def shot(page: Page, name: str):
        page.screenshot(path=str(out / f"{name}.png"))

    with sync_playwright() as pw:
        browser = pw.chromium.launch()
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
        page.on("response", lambda r: errors.append(f"{r.status} {r.url}") if r.status >= 500 else None)

        def step(name: str, fn):
            try:
                fn()
                steps.append((name, "ok"))
            except Exception as e:  # noqa: BLE001
                steps.append((name, f"ECHEC {str(e).splitlines()[0][:160]}"))
                shot(page, f"echec_{len(steps)}")

        # 1. Coder: code a passage of a remaining unit with the key 1.
        def code_a_passage():
            page.goto(f"{base}/#/interviews/{IV}/coding", wait_until="load")
            page.wait_for_selector("[data-unit]")
            unit = page.locator('[data-unit="INT-01-U004"]')
            unit.click()
            before = len(api(base, "GET", f"/api/interviews/{IV}/passages"))
            page.keyboard.press("1")
            page.wait_for_function("() => true")
            expect(unit.locator(".applied .tag").first).to_be_visible()
            after = len(api(base, "GET", f"/api/interviews/{IV}/passages"))
            assert after == before + 1, f"passages {before} -> {after}"
            shot(page, "01_coder_code")

        step("coder: poser un code avec la touche 1", code_a_passage)

        # 2. R marks the unit to review; the filter counts it; the server holds the mark.
        def mark_review():
            unit = page.locator('[data-unit="INT-01-U002"]')
            unit.click()
            page.keyboard.press("r")
            expect(unit.locator(".badge", has_text="à revoir")).to_be_visible()
            marks = api(base, "GET", f"/api/interviews/{IV}/marks")
            assert any(m["unit_id"] == "INT-01-U002" for m in marks), marks
            shot(page, "02_coder_revoir")

        step("coder: R marque l'unité à revoir", mark_review)

        # 3. New code from the unit, then undo (POST /api/codes/{id}/undo-create), then again.
        def create_and_undo():
            page.locator('[data-unit="INT-01-U016"]').click()
            n0 = len(api(base, "GET", "/api/codes")["codes"])
            page.keyboard.press("Control+n")
            dialog = page.get_by_role("dialog")
            dialog.get_by_label("Nom").fill("Garde des clés")
            dialog.get_by_label("Définition").fill("Qui détient les clés du dépôt et ce que cela change.")
            dialog.get_by_role("button", name="Créer et poser").click()
            expect(dialog).to_have_count(0)
            n1 = len(api(base, "GET", "/api/codes")["codes"])
            assert n1 == n0 + 1, f"codes {n0} -> {n1}"
            page.keyboard.press("Control+z")
            page.wait_for_timeout(600)
            n2 = len(api(base, "GET", "/api/codes")["codes"])
            assert n2 == n0, f"après annulation {n2}, attendu {n0}"
            page.keyboard.press("Control+n")
            dialog = page.get_by_role("dialog")
            dialog.get_by_label("Nom").fill("Garde des clés")
            dialog.get_by_label("Définition").fill("Qui détient les clés du dépôt et ce que cela change.")
            dialog.get_by_role("button", name="Créer et poser").click()
            expect(dialog).to_have_count(0)
            assert len(api(base, "GET", "/api/codes")["codes"]) == n0 + 1
            shot(page, "03_coder_nouveau_code")

        step("coder: créer un code puis annuler (undo-create)", create_and_undo)

        # 4. Détail tab: origin and role of a coded passage.
        def qualify():
            page.locator('[data-unit="INT-01-U008"]').click()
            page.locator('[data-unit="INT-01-U008"] [data-passage]').first.click()
            page.get_by_label("Origine du propos").select_option("project")
            page.get_by_label("Rôle dans la décision").select_option("trigger")
            page.wait_for_timeout(500)
            ps = api(base, "GET", f"/api/interviews/{IV}/passages")
            q = [p for p in ps if p["unit_id"] == "INT-01-U008" and p.get("origin") == "project" and p.get("role") == "trigger"]
            assert q, "qualification non enregistrée"
            shot(page, "04_coder_detail")

        step("coder: qualifier un passage (origine, rôle)", qualify)

        # 5. Analyser: retain the second theme with its checks.
        def retain_theme():
            page.goto(f"{base}/#/analyser", wait_until="load")
            page.get_by_role("button", name="La pression des clients", exact=False).click()
            region = page.get_by_role("region", name="Fiche du thème")
            expect(region).to_be_visible()
            region.get_by_role("button", name="Contrôles avant « retenu »").click()
            boxes = page.get_by_role("list", name="Contrôles avant « retenu »").get_by_role("checkbox")
            for i in range(boxes.count()):
                if boxes.nth(i).get_attribute("aria-checked") != "true":
                    boxes.nth(i).click()
                    page.wait_for_timeout(250)
            page.locator("#theme-status").select_option("retained")
            page.wait_for_timeout(600)
            themes = api(base, "GET", "/api/themes")
            assert [t["status"] for t in themes] == ["retained", "retained"], themes
            shot(page, "05_analyser_themes")

        step("analyser: retenir un thème après les contrôles", retain_theme)

        # 6. Matrix: open a cell, set its value and its summary.
        def matrix_cell():
            page.get_by_role("tab", name="Matrice").click()
            table = page.get_by_role("table", name="Matrice")
            expect(table).to_be_visible()
            table.locator("button").first.click()
            panel = page.get_by_role("complementary")
            label = panel.get_attribute("aria-label") or ""
            row = label.removeprefix("Cellule ").split(",")[0].strip()
            panel.get_by_label("Valeur de cellule").select_option("not_mentioned")
            page.wait_for_timeout(600)
            shot(page, "06_analyser_matrice")
            cells = api(base, "POST", "/api/matrix/compute", {"rows": [{"kind": "interviews"}], "cols": [{"kind": "themes"}], "filters": []})
            got = [c["value"] for r in cells["cells"] for c in r if c["row_key"] == f"interview:{row}"]
            assert "not_mentioned" in got, (row, got)

        step("analyser: régler une cellule de la matrice", matrix_cell)

        # 7. Rédiger: write a part, link a passage by the keyboard alternative.
        def write_and_link():
            page.goto(f"{base}/#/rediger", wait_until="load")
            page.get_by_role("button", name=re.compile(r"^\d+\s+La pression des clients")).click()
            ta = page.get_by_role("textbox", name="1. Énoncé analytique")
            expect(ta).to_be_visible()
            ta.fill("Une exigence extérieure précède le besoin interne et déclenche le projet.")
            ta.blur()
            page.wait_for_timeout(1500)
            w = api(base, "GET", "/api/writing/themes/T-0002")
            assert w["parts"]["analytical_statement"]["text"].startswith("Une exigence"), w["parts"]["analytical_statement"]["text"]
            selects = page.locator('select[aria-label^="Lier "]')
            expect(selects.first).to_be_visible()
            selects.first.select_option(label="Énoncé analytique")
            page.wait_for_timeout(800)
            w = api(base, "GET", "/api/writing/themes/T-0002")
            assert len(w["parts"]["analytical_statement"]["links"]) == 1, w["parts"]["analytical_statement"]["links"]
            shot(page, "07_rediger_partie")

        step("rédiger: écrire une partie et lier un passage au clavier", write_and_link)

        # 8. Final audit.
        def final_audit():
            page.get_by_role("tab", name="Audit final").click()
            page.get_by_role("button", name="Relancer un tirage").click()
            page.wait_for_timeout(800)
            arts = page.get_by_role("article")
            assert arts.count() >= 1, "aucune affirmation tirée"
            arts.first.get_by_label("À nuancer").check()
            arts.first.get_by_role("textbox", name="Note du verdict").fill("La seconde source nuance le propos.")
            arts.first.get_by_role("button", name="Enregistrer le verdict").click()
            page.wait_for_timeout(600)
            latest = api(base, "GET", "/api/final-audit/latest")
            assert any(c["verdict"] for c in latest["claims"]), "verdict non enregistré"
            shot(page, "08_rediger_audit")

        step("rédiger: tirer l'audit final et juger une affirmation", final_audit)

        # 9. Markdown export.
        def export():
            page.get_by_role("button", name="Exporter en Markdown").click()
            page.wait_for_timeout(1500)
            shot(page, "09_export")
            r = api(base, "POST", "/api/writing/export", {"format": "markdown"})
            names = [f["name"] for f in r["files"]]
            assert "writeup.md" in names and "manifest.json" in names, names
            assert "Une exigence extérieure" in r["markdown"], "partie absente de l'export"

        step("rédiger: exporter en Markdown", export)

        browser.close()

    width = max(len(n) for n, _ in steps)
    for n, s in steps:
        print(f"{n.ljust(width)}  {s}")
    if errors:
        print("Erreurs de console ou 5xx:")
        for e in dict.fromkeys(errors):
            print("  ", e[:200])
    return 0 if all(s == "ok" for _, s in steps) and not errors else 1


if __name__ == "__main__":
    sys.exit(main())
