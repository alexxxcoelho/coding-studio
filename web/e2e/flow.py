#!/usr/bin/env python3
"""Full flow through the web UI in headless Chromium, on a seeded database.

Codes a passage, creates a code (and undoes it), marks a unit to review,
qualifies a passage, retains a theme with its checks, sets a matrix cell,
writes a part of the write-up, links a passage by the keyboard alternative,
draws the final audit, judges a claim and exports the Markdown. Then the
lexical workbench (create a code from keywords, near passages, keyword in
context) and the review of INT-02 by copy and paste (generate the prompt, import
a handcrafted answer with a candidate concept, decide a disagreement with the
keys, decide the candidate, harmonise, library state, pseudonymisation list), then Personnes
(group, person, link to an interview, anonymise and the derived entry of the list, CSV import, move and delete
a group, the persons of an interview fiche, the derived entries in Paramètres de l'étude) and the verification package.

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
        page.on("console", lambda m: errors.append(f"{m.text} {m.location.get('url', '')}") if m.type == "error" else None)
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

        # ---- Lot 3: the lexical workbench.
        def lexicon_code_from_terms():
            kw = api(base, "GET", "/api/lexicon/keywords")
            multi = [r for r in kw["rows"] if r["n_passages"] >= 2]
            assert multi, "aucun mot dans deux surlignages"
            row = multi[0]
            before = {c["code_id"] for c in api(base, "GET", "/api/codes")["codes"]}
            n_codes = len(before)
            page.goto(f"{base}/#/lexique", wait_until="load")
            table = page.get_by_test_id("keyword-table")
            expect(table).to_be_visible()
            tr = table.locator(f'tr[data-stem="{row["stem"]}"]')
            expect(tr).to_be_visible()
            tr.get_by_role("checkbox").check()
            shot(page, "10_lexique_mots_cles")
            page.get_by_role("button", name=re.compile(r"^Créer un code")).click()
            dialog = page.get_by_role("dialog")
            expect(dialog).to_be_visible()
            dialog.get_by_label("Définition").fill("Le pilote qui précède l'achat, ce qu'il montre et ce qu'il ne montre pas.")
            dialog.get_by_role("button", name="Créer et poser").click()
            expect(dialog).to_have_count(0)
            codes = api(base, "GET", "/api/codes")["codes"]
            assert len(codes) == n_codes + 1, f"codes {n_codes} -> {len(codes)}"
            new = [c for c in codes if c["code_id"] not in before][0]
            coded = api(base, "GET", "/api/codes/" + new["code_id"] + "/passages")
            assert len(coded) >= 2, f"{len(coded)} passage(s) codé(s), attendu au moins 2"

        step("lexique: créer un code à partir des mots-clés des surlignages", lexicon_code_from_terms)

        def lexicon_similar():
            page.goto(f"{base}/#/lexique", wait_until="load")
            page.get_by_role("tab", name="Passages proches").click()
            page.get_by_label("Entretien", exact=True).select_option("INT-02")
            highlights = page.get_by_role("list", name="Surlignages").get_by_role("button")
            expect(highlights.first).to_be_visible()
            highlights.first.click()
            expect(page.get_by_test_id("passage-lexicon")).to_contain_text("Expressions-clés")
            cards = page.get_by_test_id("similar-passages").get_by_role("article")
            expect(cards.first).to_be_visible()
            assert cards.count() >= 1
            before = len(api(base, "GET", "/api/interviews/INT-02/passages"))
            cards.first.locator("select").select_option(index=1)
            page.wait_for_timeout(800)
            after = len(api(base, "GET", "/api/interviews/INT-02/passages")) + len(api(base, "GET", "/api/interviews/INT-01/passages"))
            assert after > before, "aucun passage créé par « Coder avec… »"
            shot(page, "11_lexique_proches")

        step("lexique: passages proches d'un surlignage, coder un passage proche", lexicon_similar)

        def lexicon_kwic():
            page.goto(f"{base}/#/lexique", wait_until="load")
            page.get_by_role("tab", name="Recherche en contexte").click()
            page.get_by_label("Mot ou expression à chercher").fill("intégration")
            page.get_by_role("button", name="Chercher").click()
            hits = page.get_by_role("list", name="Occurrences").get_by_role("listitem")
            expect(hits.first).to_be_visible()
            answer = api(base, "GET", "/api/lexicon/kwic?q=int%C3%A9gration")
            total = answer["total"]
            # Case and accents are ignored: the English "integration" of INT-01 is found too.
            assert total >= 2 and answer["n_interviews"] == 2 and hits.count() == total, (total, hits.count())
            expect(page.get_by_text(f"{total} occurrences dans 2 entretiens")).to_be_visible()
            shot(page, "12_lexique_kwic")

        step("lexique: recherche en contexte", lexicon_kwic)

        # ---- Lot 4: the review of INT-02, by copy and paste.
        IV2 = "INT-02"
        i2 = f"/api/interviews/{IV2}"

        def review_generate():
            page.goto(f"{base}/#/interviews/{IV2}/ai", wait_until="load")
            page.get_by_role("button", name="Générer le prompt").click()
            expect(page.get_by_role("button", name="Copier la partie 1 sur 1")).to_be_visible()
            parts = api(base, "GET", f"{i2}/phases/2/prompts")
            text = "\n".join(p["text"] for p in parts)
            assert "Claire Vuilleumier" not in text and "Lectis" not in text, "un nom de la liste est parti dans le prompt"
            assert "Interviewer questions" in text, "les questions de l'intervieweur manquent"
            expect(page.get_by_text(re.compile(r"Pseudonymisation active : .* nom"))).to_be_visible()
            shot(page, "13_revoir_prompt")

        step("revoir: générer le prompt (noms remplacés, codage figé)", review_generate)

        def review_import():
            part = api(base, "GET", f"{i2}/phases/2/prompts")[-1]
            seg = api(base, "GET", i2)["segmentation"]
            texts = {u["unit_id"]: u["text"] for u in seg["units"]}
            ai = {2: ["NA"], 4: ["CLIENT_PRESSURE"], 6: ["STOCK_ACCURACY"], 8: ["READER_COST"], 10: ["READER_COST"], 12: ["SKILLS_GAP"],
                  14: ["SKILLS_GAP"], 16: ["SKILLS_GAP"], 18: ["CLIENT_PRESSURE"], 20: ["PRIVACY_CONCERN"], 22: ["CHANGE_RESISTANCE"],
                  24: ["READER_COST"], 26: ["STOCK_ACCURACY"]}
            quote = " ".join(texts[f"{IV2}-U010"].split()[:7])
            answer = {
                "schema": "coding-studio/ai-coding/v1", "interview_id": IV2, "phase": 2, "segmentation_hash": part["segmentation_hash"],
                "part": part["part"], "of": part["of"],
                "codings": [{"unit_id": u, "codes": ai.get(int(u[-3:]), ["NA"])} for u in part["unit_ids"]],
                "candidate_concepts": [{
                    "label": "Matériel pris en charge par la commune", "description": "La commune finance une partie du matériel sur son budget d'investissement.",
                    "unit_ids": [f"{IV2}-U010"], "evidence": [{"unit_id": f"{IV2}-U010", "quote": quote}]}],
            }
            page.goto(f"{base}/#/interviews/{IV2}/ai", wait_until="load")
            page.get_by_label("Modèle utilisé").fill("modele-main")
            page.get_by_role("group", name="Réponse à la partie 1 sur 1").get_by_role("textbox").fill(json.dumps(answer, ensure_ascii=False))
            page.get_by_role("button", name="Importer la réponse").click()
            expect(page.get_by_text(re.compile(r"Run R1 complet"))).to_be_visible()
            runs = api(base, "GET", f"{i2}/phases/2/runs")
            assert runs and runs[-1]["complete"], runs
            cands = api(base, "GET", f"{i2}/concept-candidates")
            assert len(cands) == 1 and cands[0]["evidence"][0]["quote"] == quote, cands
            state = api(base, "GET", f"{i2}/review-state")
            assert state["state"] == "in_progress" and state["n_disagreements"] >= 2 and state["n_candidates"] == 1, state
            shot(page, "14_revoir_import")

        step("revoir: importer une réponse faite à la main, avec un concept candidat", review_import)

        def review_decide():
            page.goto(f"{base}/#/interviews/{IV2}/review", wait_until="load")
            card = page.get_by_role("article", name=re.compile(r"^Unité U0"))
            expect(card).to_be_visible()
            page.keyboard.press("2")  # the model's reading
            card.get_by_role("textbox").fill("Le tour parle du coût de l'intégration, pas seulement de la précision.")
            page.keyboard.press("Control+Enter")
            page.wait_for_timeout(800)
            items = api(base, "GET", f"{i2}/review")["items"]
            decided = [i for i in items if i["decision"]]
            assert len(decided) == 1 and decided[0]["decision"]["choice"]["type"] == "ai", decided
            state = api(base, "GET", f"{i2}/review-state")
            assert state["n_decided"] == 1, state
            expect(page.get_by_text(f"revue IA en cours, 1 sur {state['n_disagreements']} décidées")).to_be_visible()
            shot(page, "15_revoir_decision")

        step("revoir: décider un désaccord avec les touches", review_decide)

        def review_candidate():
            page.get_by_role("button", name=re.compile("Matériel pris en charge")).click()
            card = page.get_by_role("article", name=re.compile("^Concept Matériel"))
            expect(card).to_be_visible()
            expect(card.locator("mark")).to_be_visible()
            card.get_by_role("button", name="Créer un code").click()
            card.get_by_label("Nom du nouveau code").fill("Financement par la commune")
            card.get_by_label("Justification, obligatoire").fill("Revient dans la comparaison avec les communes voisines.")
            card.get_by_role("button", name="Enregistrer la décision").click()
            page.wait_for_timeout(800)
            c = api(base, "GET", f"{i2}/concept-candidates")[0]
            assert c["decision"] and c["decision"]["action"] == "create" and c["decision"]["code_id"], c
            names = [x["name"] for x in api(base, "GET", "/api/codes")["codes"]]
            assert "Financement par la commune" in names, names
            shot(page, "16_revoir_concept")

        step("revoir: décider un concept candidat (créer un code)", review_candidate)

        def review_harmonise():
            page.goto(f"{base}/#/interviews/{IV2}/harmonise", wait_until="load")
            open0 = api(base, "GET", f"{i2}/harmonisation")["n_open"]
            assert open0 >= 2, f"{open0} unité(s) à relire, attendu au moins 2"
            expect(page.get_by_role("link", name=re.compile(rf"^Harmoniser\s*{open0}$"))).to_be_visible()
            first = page.get_by_role("article").first
            first.get_by_label("Note, facultative").fill("Le sens du code ne change pas pour ce tour.")
            first.get_by_role("button", name="Confirmer le codage").click()
            page.wait_for_timeout(700)
            page.get_by_role("article").first.get_by_role("button", name="J'ai corrigé le codage").click()
            page.wait_for_timeout(700)
            w = api(base, "GET", f"{i2}/harmonisation")
            assert w["n_done"] == 2 and w["n_open"] == open0 - 2, w["n_done"]
            res = sorted(i["resolution"] for i in w["items"] if i["status"] == "done")
            assert res == ["confirmed", "corrected"], res
            page.get_by_role("button", name="Traitées").click()
            expect(page.get_by_text("Codage confirmé").first).to_be_visible()
            shot(page, "17_revoir_harmoniser")

        step("revoir: harmoniser sans IA (confirmer, corriger)", review_harmonise)

        def library_state():
            page.goto(f"{base}/#/interviews", wait_until="load")
            row = page.get_by_role("row").filter(has_text="INT-02")
            expect(row).to_contain_text("IA · 100 % relus")
            shot(page, "18_bibliotheque_revue")

        step("bibliothèque: l'état de la revue dans la progression", library_state)

        def settings_pseudonymisation():
            page.goto(f"{base}/#/settings", wait_until="load")
            region = page.get_by_role("region", name="Pseudonymisation avant envoi")
            region.scroll_into_view_if_needed()
            before = api(base, "GET", "/api/pseudonymisation")
            assert before["n_person_entries"] >= 5, before["n_person_entries"]  # the seeded register is in force
            region.get_by_role("button", name="Ajouter un nom").click()
            region.get_by_label("Nom réel 3").fill("Mme Rochat")
            region.get_by_label("Pseudonyme 3").fill("Personne B")
            region.get_by_role("button", name="Enregistrer la liste").click()
            page.wait_for_timeout(700)
            p = api(base, "GET", "/api/pseudonymisation")
            assert p["n_list_entries"] == before["n_list_entries"] + 1 and p["entries"][-1]["original"] == "Mme Rochat", p
            expect(region.get_by_text(re.compile(rf"Liste de {p['n_list_entries']} noms"))).to_be_visible()
            side = page.get_by_role("navigation", name="Barre latérale")
            expect(side.get_by_text("Pseudonymisation active")).to_be_visible()
            # The manual switch alone: the anonymised records keep the list in force whatever it says.
            region.get_by_role("switch").click()
            expect(region.get_by_text(re.compile("la liste reste active"))).to_be_visible()
            p = api(base, "GET", "/api/pseudonymisation")
            assert p["enabled_manual"] is False and p["enabled"] is True, p
            expect(side.get_by_text("Pseudonymisation active")).to_be_visible()
            region.get_by_role("switch").click()
            page.wait_for_timeout(500)
            p = api(base, "GET", "/api/pseudonymisation")
            assert p["enabled_manual"] is True and p["enabled"] is True, p
            shot(page, "19_parametres_pseudonymisation")

        step("paramètres: éditer la liste de pseudonymisation et l'interrupteur", settings_pseudonymisation)

        # ---- Personnes: the register of the people interviewed.
        def personnes_create_group_person_link():
            page.goto(f"{base}/#/personnes", wait_until="load")
            tree = page.get_by_role("table", name="Arbre des groupes")
            people = page.get_by_role("table", name="Liste des personnes")
            expect(tree.get_by_text("Suisse romande")).to_be_visible()
            expect(people.get_by_text("Sophie Aubry")).to_be_visible()
            expect(page.get_by_role("navigation", name="Barre latérale").get_by_role("link", name=re.compile("^Personnes"))).to_be_visible()
            shot(page, "30_personnes")
            # A group, then a person created in it.
            page.get_by_role("button", name="+ Groupe").click()
            dialog = page.get_by_role("dialog")
            dialog.get_by_label("Nom du groupe").fill("Bibliothèques scolaires")
            dialog.get_by_role("button", name="Créer le groupe").click()
            expect(dialog).to_have_count(0)
            group = next(g for g in api(base, "GET", "/api/person-groups")["groups"] if g["name"] == "Bibliothèques scolaires")
            expect(tree.get_by_text("Bibliothèques scolaires")).to_be_visible()
            page.get_by_role("button", name="+ Personne").click()
            dialog = page.get_by_role("dialog")
            dialog.get_by_label("Nom", exact=True).fill("Isabelle Garnier")
            dialog.get_by_role("button", name="Créer la fiche").click()
            expect(dialog).to_have_count(0)
            side = page.get_by_role("complementary", name="Fiche de la personne")
            expect(side.get_by_label("Nom", exact=True)).to_have_value("Isabelle Garnier")
            made = next(p for p in api(base, "GET", "/api/persons")["persons"] if p["name"] == "Isabelle Garnier")
            assert made["group_ids"] == [group["group_id"]] and made["anonymise"] is True and made["pseudonym"], made
            # Link it to an interview with a role, from the fiche.
            side.get_by_label("Entretien à lier").select_option("INT-02")
            side.get_by_label("Rôle à donner").select_option("co_respondent")
            side.get_by_role("button", name="Lier", exact=True).click()
            page.wait_for_function("() => document.body.innerText.includes('Retirer')")
            detail = api(base, "GET", f"/api/persons/{made['person_id']}")
            assert [(i["interview_id"], i["role"]) for i in detail["interviews"]] == [("INT-02", "co_respondent")], detail["interviews"]
            # A name that another record already carries is refused, in plain words.
            side.get_by_placeholder("Nouvel alias").fill("Sophie")
            side.get_by_role("button", name="Ajouter", exact=True).click()
            expect(page.get_by_role("alert").first).to_contain_text("déjà pris")
            assert api(base, "GET", f"/api/persons/{made['person_id']}")["aliases"] == []
            shot(page, "31_personnes_nouvelle_fiche")

        step("personnes: créer un groupe, une personne, la lier à un entretien, nom en conflit", personnes_create_group_person_link)

        def personnes_toggle_anonymise():
            page.goto(f"{base}/#/personnes", wait_until="load")
            people = page.get_by_role("table", name="Liste des personnes")
            page.get_by_role("table", name="Arbre des groupes").get_by_text("Toutes les personnes").click()
            people.get_by_text("Médiathèque des Rives").first.click()
            side = page.get_by_role("complementary", name="Fiche de la personne")
            switch = side.get_by_role("switch", name="Anonymiser")
            expect(switch).to_have_attribute("aria-checked", "true")

            def derived(path):
                return [e for e in api(base, "GET", path)["entries"] if e["original"] == "Médiathèque des Rives"]

            assert derived("/api/interviews/INT-02/pseudonymisation"), "le nom de la fiche anonymisée manque à la liste de INT-02"
            assert any(e.get("source") == "person" and e["original"] == "Médiathèque des Rives" for e in api(base, "GET", "/api/pseudonymisation")["entries"])
            switch.click()
            expect(switch).to_have_attribute("aria-checked", "false")
            assert not derived("/api/interviews/INT-02/pseudonymisation"), "le nom reste dans la liste alors que la fiche n'est plus anonymisée"
            switch.click()
            expect(switch).to_have_attribute("aria-checked", "true")
            assert derived("/api/interviews/INT-02/pseudonymisation")
            # A pseudonym typed in the fiche is kept, and it is the one the list uses.
            side.locator("#p-pseudonym").fill("Médiathèque A")
            side.locator("#p-pseudonym").press("Enter")
            page.wait_for_timeout(600)
            assert next(e for e in derived("/api/pseudonymisation")) ["pseudonym"] == "Médiathèque A"
            shot(page, "32_personnes_anonymiser")

        step("personnes: anonymiser et désanonymiser, l'entrée dérivée suit la liste", personnes_toggle_anonymise)

        def personnes_import_csv():
            page.goto(f"{base}/#/personnes", wait_until="load")
            page.get_by_role("button", name="Importer un CSV").click()
            dialog = page.get_by_role("dialog")
            with page.expect_download() as dl:
                dialog.get_by_role("button", name="Télécharger le modèle").click()
            header = Path(dl.value.path()).read_text().splitlines()[0]
            assert header.startswith("person_id,name,kind,") and "attr:anciennete" in header and "attr:taille" in header, header
            n0 = api(base, "GET", "/api/persons")["n"]
            apply_btn = dialog.get_by_role("button", name="Appliquer", exact=True)
            dialog.get_by_label("Contenu du fichier CSV").fill("name,kind,groups\nPaul Dumas,animal,Détaillants/Vaud\n")
            dialog.get_by_role("button", name="Vérifier le fichier").click()
            expect(dialog.get_by_role("table", name="Problèmes du fichier")).to_contain_text("Le type doit être")
            expect(apply_btn).to_be_disabled()
            shot(page, "33_personnes_import_problemes")
            dialog.get_by_label("Contenu du fichier CSV").fill("name,kind,groups,interviews,anonymise\nPaul Dumas,person,Détaillants/Vaud,INT-01:mentioned,non\n")
            expect(apply_btn).to_be_disabled()  # changed since the check
            dialog.get_by_role("button", name="Vérifier le fichier").click()
            expect(dialog.get_by_text("Vérification réussie")).to_be_visible()
            expect(dialog).to_contain_text("Groupes qui seront créés : Détaillants/Vaud")
            assert api(base, "GET", "/api/persons")["n"] == n0, "la simulation a écrit"
            shot(page, "34_personnes_import_simulation")
            apply_btn.click()
            expect(dialog.get_by_text("Import appliqué.")).to_be_visible()
            assert api(base, "GET", "/api/persons")["n"] == n0 + 1
            paul = next(p for p in api(base, "GET", "/api/persons")["persons"] if p["name"] == "Paul Dumas")
            assert paul["anonymise"] is False and paul["n_interviews"] == 1, paul
            dialog.get_by_role("button", name="Vérifier le fichier").click()
            expect(dialog.get_by_text(re.compile("1 inchangée"))).to_be_visible()  # the same file changes nothing
            dialog.get_by_role("button", name="Fermer").last.click()
            expect(dialog).to_have_count(0)

        step("personnes: importer un CSV (modèle, simulation, problèmes, application)", personnes_import_csv)

        def personnes_move_group():
            page.goto(f"{base}/#/personnes", wait_until="load")
            tree = page.get_by_role("table", name="Arbre des groupes")
            tree.get_by_text("Bibliothèques scolaires").click()
            page.get_by_role("button", name="Modifier", exact=True).click()
            dialog = page.get_by_role("dialog")
            dialog.get_by_label("Groupe parent").select_option(label="Médiathèques")
            dialog.get_by_role("button", name="Enregistrer").click()
            expect(dialog).to_have_count(0)
            tr = api(base, "GET", "/api/person-groups")["groups"]
            med = next(g for g in tr if g["name"] == "Médiathèques")
            assert [c["name"] for c in med["children"]] == ["Bibliothèques scolaires"], med["children"]
            assert med["n_persons_total"] == 3, med["n_persons_total"]  # Marc, the organisation and the new person, rolled up
            # Delete with reassignment: the members and sub-groups go to the chosen group.
            tree.get_by_text("Médiathèques", exact=True).first.click()
            page.get_by_role("button", name="Supprimer", exact=True).first.click()
            dialog = page.get_by_role("dialog")
            confirm = dialog.get_by_role("button", name="Supprimer le groupe")
            expect(confirm).to_be_disabled()
            dialog.get_by_label("Rattacher les sous-groupes et les membres à").select_option(label="Détaillants")
            confirm.click()
            expect(dialog).to_have_count(0)
            tr = api(base, "GET", "/api/person-groups")["groups"]
            det = next(g for g in tr if g["name"] == "Détaillants")
            assert "Bibliothèques scolaires" in [c["name"] for c in det["children"]], det["children"]
            assert not any(g["name"] == "Médiathèques" for g in tr)
            assert all(p["group_ids"] for p in api(base, "GET", "/api/persons?q=Garnier")["persons"])
            shot(page, "35_personnes_groupes")

        step("personnes: déplacer un groupe, supprimer avec réaffectation", personnes_move_group)

        def bibliotheque_personnes():
            page.goto(f"{base}/#/interviews", wait_until="load")
            page.get_by_role("row").filter(has_text="INT-01").first.click()
            box = page.get_by_role("complementary", name="Fiche INT-01").get_by_role("group", name="Personnes de l'entretien INT-01")
            expect(box).to_contain_text("Sophie Aubry")
            expect(box).to_contain_text("pour le modèle : « Répondant 01 »")
            expect(box).to_contain_text("Léa Morand")
            box.get_by_label("Personne à lier").select_option(label="Marc Bovet")
            box.get_by_label("Rôle à donner").select_option("mentioned")
            box.get_by_role("button", name="Lier", exact=True).click()
            box.get_by_role("button", name="Retirer Marc Bovet de cet entretien").wait_for()
            links = {p["person_id"]: p["role"] for p in api(base, "GET", "/api/interviews/INT-01/persons")["persons"]}
            assert links.get("P-0003") == "mentioned", links
            shot(page, "36_bibliotheque_personnes")
            box.get_by_role("button", name="Retirer Marc Bovet de cet entretien").click()
            page.wait_for_function("() => !document.querySelector('[aria-label=\"Retirer Marc Bovet de cet entretien\"]')")
            assert "P-0003" not in {p["person_id"] for p in api(base, "GET", "/api/interviews/INT-01/persons")["persons"]}

        step("bibliothèque: personnes de la fiche d'un entretien, lier et retirer", bibliotheque_personnes)

        def settings_derived_entries():
            page.goto(f"{base}/#/settings", wait_until="load")
            region = page.get_by_role("region", name="Pseudonymisation avant envoi")
            region.scroll_into_view_if_needed()
            derived = region.get_by_role("table", name="Noms issus des fiches de Personnes")
            expect(derived).to_contain_text("Sophie Aubry")
            expect(derived).to_contain_text("Médiathèque A")
            assert derived.locator("input, select").count() == 0, "une entrée dérivée est modifiable"
            expect(region.get_by_label("Nom réel 1")).to_have_value("Claire Vuilleumier")  # the manual part comes after
            shot(page, "37_parametres_entrees_derivees")
            derived.get_by_role("link", name=re.compile("Ouvrir la fiche P-0001")).first.click()
            expect(page.get_by_role("complementary", name="Fiche de la personne").get_by_label("Nom", exact=True)).to_have_value("Sophie Aubry")
            assert page.url.endswith("#/personnes/P-0001"), page.url

        step("paramètres: entrées dérivées des fiches à part, lien vers la fiche", settings_derived_entries)

        def verification_package():
            page.goto(f"{base}/#/settings", wait_until="load")
            page.get_by_role("button", name="Paquet de vérification").click()
            dialog = page.get_by_role("dialog")
            expect(dialog.get_by_role("button", name="Générer le paquet")).to_be_disabled()
            dialog.get_by_role("checkbox", name="INT-02").check()
            dialog.get_by_role("button", name="Générer le paquet").click()
            expect(dialog.get_by_test_id("verification-result")).to_be_visible()
            folder = Path(dialog.get_by_test_id("verification-dir").inner_text())
            expect(dialog.get_by_test_id("verification-result")).to_contain_text("personnes")
            shot(page, "38_paquet_verification")
            assert (folder / "manifest.json").is_file() and (folder / "persons.csv").is_file(), list(folder.iterdir())
            coded = (folder / "interviews" / "INT-02" / "transcript_coded.md").read_text()
            assert "Lectis" not in coded and "Claire Vuilleumier" not in coded and "Rives" not in coded, "un nom de la liste reste dans la transcription"
            public = (folder / "persons.csv").read_text()
            assert "Médiathèque A" in public and "Répondant 04" in public, public
            assert "Médiathèque des Rives" not in public and "Garnier" not in public and "@" not in public, public
            # The same dialog from the toolbar of the library, with the selected interview ticked.
            page.goto(f"{base}/#/interviews", wait_until="load")
            page.get_by_role("button", name="Exporter").click()
            page.get_by_role("menuitem", name="Paquet de vérification").click()
            dialog = page.get_by_role("dialog")
            expect(dialog.get_by_role("checkbox", name="INT-01")).to_be_checked()
            dialog.get_by_role("button", name="Fermer").last.click()

        step("exports: paquet de vérification (paramètres et bibliothèque), noms masqués", verification_package)

        # ---- V2: split a code, decide on the keyboard, undo from the journal.
        def v2_split_and_undo():
            page.goto(f"{base}/#/codebook", wait_until="load")
            page.get_by_role("row").filter(has_text="Exactitude des stocks").first.click()
            page.get_by_role("button", name="Scinder…").click()
            dialog = page.get_by_role("dialog")
            dialog.get_by_label("Nom de la partie 1").fill("Écart de stock chiffré")
            dialog.get_by_label("Définition de la partie 1").fill("Le chiffre d'écart entre stock théorique et compté.")
            dialog.get_by_label("Nom de la partie 2").fill("Fiabilité ressentie du stock")
            dialog.get_by_label("Définition de la partie 2").fill("Le sentiment de pouvoir se fier au stock.")
            dialog.get_by_label("Motif, obligatoire").fill("Deux idées dans un seul code")
            dialog.get_by_role("button", name="Scinder et réaffecter les passages").click()
            expect(page.get_by_role("heading", name=re.compile("Réaffecter les passages"))).to_be_visible()
            ops = api(base, "GET", "/api/code-ops")["operations"]
            op = ops[-1]
            assert op["kind"] == "split", op
            shot(page, "20_codebook_scission_liste")
            page.keyboard.press("1")
            page.keyboard.press("ArrowDown")
            page.keyboard.press("2")
            page.keyboard.press("ArrowDown")
            page.keyboard.press("l")
            page.get_by_role("button", name=re.compile("Enregistrer \\d+ décision")).click()
            page.wait_for_timeout(800)
            wl = api(base, "GET", f"/api/splits/{op['op_id']}/worklist")
            assert wl["assigned"] + wl["left"] >= 2, {k: wl[k] for k in ("total", "pending", "assigned", "left")}
            page.keyboard.press("Escape")
            page.get_by_role("button", name="Journal").first.click()
            journal = page.get_by_role("dialog")
            expect(journal.get_by_text(re.compile("scindé en")).first).to_be_visible()
            shot(page, "21_codebook_journal")
            journal.get_by_role("button", name=re.compile("Annuler cette scission")).first.click()
            undo = page.get_by_role("dialog").last
            undo.get_by_label("Motif de l'annulation, obligatoire").fill("Essai du parcours")
            undo.get_by_role("button", name="Annuler l'opération").click()
            page.wait_for_timeout(800)
            ops = api(base, "GET", "/api/code-ops")["operations"]
            assert ops[-1]["undone"] is not None, ops[-1]

        step("codebook: scinder un code, décider au clavier, annuler depuis le journal", v2_split_and_undo)

        # ---- V2: full-text search, filter on the coded state, code the selection.
        def v2_search():
            page.goto(f"{base}/#/recherche", wait_until="load")
            page.get_by_label("Texte à chercher").fill("étiquettes")
            page.get_by_role("button", name="Chercher").click()
            expect(page.get_by_role("status")).to_contain_text("occurrence")
            expect(page.locator("article[data-unit]").first).to_be_visible()
            expect(page.locator("article mark").first).to_be_visible()
            shot(page, "22_recherche")
            page.get_by_role("radio", name="Non codés").click()
            page.wait_for_timeout(600)
            articles = page.locator("article[data-unit]")
            assert articles.count() > 0, "aucun résultat non codé"
            first = articles.first
            uid = first.get_attribute("data-unit")
            iv = uid.rsplit("-U", 1)[0]
            before = len(api(base, "GET", f"/api/interviews/{iv}/passages"))
            first.get_by_role("button", name=re.compile("Coder la sélection")).click()
            page.get_by_label("Code à poser sur la sélection").fill("Coût des lecteurs")
            page.get_by_label("Code à poser sur la sélection").press("Enter")
            page.wait_for_timeout(800)
            after = len(api(base, "GET", f"/api/interviews/{iv}/passages"))
            assert after == before + 1, f"passages {before} -> {after}"
            shot(page, "23_recherche_code")

        step("recherche: chercher, filtrer sur non codé, coder la sélection", v2_search)

        # ---- V2: co-occurrence table and the passages of a cell.
        def v2_cooccurrence():
            page.goto(f"{base}/#/analyser", wait_until="load")
            page.get_by_role("tab", name="Co-occurrence").click()
            table = page.get_by_role("table", name=re.compile("Co-occurrences"))
            table.wait_for()
            page.get_by_label("Relation").select_option("same_unit")
            page.wait_for_timeout(600)
            cell = table.locator("button.heat-cell").first
            expect(cell).to_be_visible()
            assert cell.inner_text().strip().isdigit()
            shot(page, "24_cooccurrence")
            cell.click()
            region = page.get_by_role("region", name="Passages de la cellule")
            expect(region.locator("a").first).to_be_visible()
            shot(page, "25_cooccurrence_passages")

        step("analyser: co-occurrence, passages d'une cellule", v2_cooccurrence)

        # ---- V2: correct a turn, the coded sub-span lands in the review, accept it.
        def v2_transcript_edit_review():
            page.goto(f"{base}/#/interviews/INT-02/coding", wait_until="load")
            page.wait_for_selector("[data-unit]")
            page.locator('[data-unit="INT-02-U016"]').click()
            u = next(x["unit"] for x in api(base, "GET", "/api/interviews/INT-02/phases/2/units") if x["unit"]["unit_id"] == "INT-02-U016")
            ps = [p for p in api(base, "GET", "/api/interviews/INT-02/passages") if p["unit_id"] == "INT-02-U016" and not p["whole_unit"]]
            assert ps, "pas de passage partiel sur INT-02-U016"
            page.get_by_role("tab", name="Codes").click()
            page.get_by_role("button", name=re.compile("Corriger le texte de cette unité")).click()
            dialog = page.get_by_role("dialog")
            words = u["text"].split(" ")
            words[1] = words[1] + " vraiment"
            dialog.get_by_label("Texte corrigé").fill(" ".join(words))
            dialog.get_by_label("Motif de la correction").fill("Mot omis à l'écoute")
            dialog.get_by_role("button", name="Enregistrer la correction").click()
            page.wait_for_timeout(900)
            review = api(base, "GET", "/api/transcript/review")
            assert review["open"] >= 1, review["open"]
            page.get_by_role("button", name=re.compile("Versions")).click()
            dialog = page.get_by_role("dialog")
            expect(dialog.get_by_text("Correction d'un tour").first).to_be_visible()
            shot(page, "26_versions_revue")
            before = api(base, "GET", "/api/transcript/review")["open"]
            dialog.get_by_role("button", name="Accepter").first.click()
            page.wait_for_timeout(900)
            after = api(base, "GET", "/api/transcript/review")["open"]
            assert after == before - 1, f"à revoir {before} -> {after}"
            page.keyboard.press("Escape")

        step("coder: corriger un tour, revoir les passages déplacés, accepter", v2_transcript_edit_review)

        # ---- V2: audio of INT-02, seek from the time of a turn.
        def v2_audio():
            page.goto(f"{base}/#/interviews/INT-02/coding", wait_until="load")
            page.wait_for_selector("[data-unit]")
            player = page.get_by_role("group", name="Lecteur audio")
            expect(player).to_be_visible()
            page.locator('[data-unit="INT-02-U002"] button.ts').click()
            page.wait_for_function("() => document.querySelector('.u.hear') !== null", timeout=8000)
            expect(player.get_by_role("button", name=re.compile("Vitesse"))).to_be_visible()
            player.get_by_role("button", name=re.compile("Vitesse")).click()
            expect(player.get_by_role("button", name="Vitesse 1.25x, changer")).to_be_visible()
            shot(page, "27_audio")
            page.goto(f"{base}/#/interviews/INT-01/coding", wait_until="load")
            page.wait_for_selector("[data-unit]")
            expect(page.get_by_role("group", name="Lecteur audio")).to_have_count(0)

        step("coder: lecteur audio, aller au tour par son horodatage", v2_audio)

        # ---- V2: origins and roles from the study lists.
        def v2_origins_roles():
            page.goto(f"{base}/#/settings", wait_until="load")
            page.get_by_text("Origines du propos").first.scroll_into_view_if_needed()
            cfg = api(base, "GET", "/api/project-config")
            assert cfg["passage_origins"] and cfg["passage_roles"], "listes absentes de la config"
            page.goto(f"{base}/#/interviews/INT-01/coding", wait_until="load")
            page.wait_for_selector("[data-unit]")
            page.locator('[data-unit="INT-01-U008"]').click()
            page.locator('[data-unit="INT-01-U008"] [data-passage]').first.click()
            labels = page.get_by_label("Origine du propos").locator("option").all_inner_texts()
            assert [x["label"] for x in cfg["passage_origins"]][0] in labels, labels

        step("coder et paramètres: origines et rôles issus des listes de l'étude", v2_origins_roles)

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
