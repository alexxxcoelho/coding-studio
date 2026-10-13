#!/usr/bin/env python3
"""Seed a fresh Coding Studio database with enough data to exercise every screen.

Usage: python3 e2e/seed.py [base_url]   (default http://127.0.0.1:7979)
Run it once against a server started on an empty --db file.
"""
import json
import sys
import urllib.error
import urllib.request

BASE = (sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:7979").rstrip("/")
IV = "INT-01"


def call(method, path, body=None, ok=True):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data, method=method, headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(req) as r:
            raw = r.read()
            return json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        msg = e.read().decode()[:300]
        if ok:
            raise SystemExit(f"{method} {path} -> {e.code} {msg}")
        print(f"  (ignored) {method} {path} -> {e.code} {msg}")
        return None


def unit_id(n):
    return f"{IV}-U{n:03d}"


call("POST", "/api/library/scan", {})
seg = call("POST", f"/api/interviews/{IV}/segment", {})
units = {u["unit_id"]: u for u in seg["units"]}

for rid, text in [("RQ1", "Quels facteurs expliquent l'adoption de la RFID ?"), ("RQ2", "Quels freins organisationnels rencontre-t-on ?")]:
    call("POST", "/api/research-questions", {"id": rid, "text": text})

CODES = [
    ("STOCK_ACCURACY", "Exactitude des stocks", "Écart entre le stock théorique et le stock compté.", "T", ["RQ1"]),
    ("READER_COST", "Coût des lecteurs", "Coût des lecteurs et de l'intégration, au-delà des étiquettes.", "T", ["RQ1"]),
    ("CHANGE_RESISTANCE", "Résistance au changement", "Réaction des équipes face au nouveau geste de travail.", "O", ["RQ2"]),
    ("SKILLS_GAP", "Compétences manquantes", "Compétences absentes en interne pour l'intégration.", "O", ["RQ2"]),
    ("CLIENT_PRESSURE", "Pression des clients", "Exigence d'un client ou d'un fournisseur.", "E", ["RQ1"]),
    ("PRIVACY_CONCERN", "Crainte de lecture à distance", "Inquiétude sur la lecture des étiquettes hors du site.", "E", []),
]
for code, label, definition, toe, rqs in CODES:
    call("POST", "/api/codebook/codes", {"code": code, "label": label, "definition": definition, "toe": toe, "research_questions": rqs})

call("POST", "/api/codes/SKILLS_GAP/aliases", {"alias": "compétences"}, ok=False)
call("POST", "/api/codes/SKILLS_GAP/move", {"parent_id": "CHANGE_RESISTANCE", "rationale": "Lot de test visuel"}, ok=False)

# Passages: coded (whole unit and sub-span), a highlight, a "Plus tard".
def passage(n, code=None, start=None, end=None, note=None):
    body = {"unit_id": unit_id(n)}
    if start is not None:
        body.update({"start": start, "end": end})
    if code:
        body["code"] = code
    if note:
        body["note"] = note
    return call("POST", f"/api/interviews/{IV}/passages", body)


passage(6, "STOCK_ACCURACY")
passage(8, "STOCK_ACCURACY", 0, 60)
passage(10, "READER_COST")
passage(12, "CHANGE_RESISTANCE")
p = passage(14, "SKILLS_GAP")
call("POST", f"/api/passages/{p['passage']['passage_id']}/codings", {"code": "READER_COST", "note": "Intégration ERP"}, ok=False)
passage(18, "CLIENT_PRESSURE")
passage(20, "PRIVACY_CONCERN")
passage(22)  # highlight
passage(24, start=0, end=40)  # highlight, sub-span
passage(26, "LATER", note="À revoir avec le terrain")

# Project configuration: attributes and a second memo template.
cfg = call("GET", "/api/project-config")
cfg["attributes"] = [
    {"id": "sector", "label": "Secteur", "kind": "choice", "choices": ["Distribution", "Industrie", "Santé"], "shared_with_model": False},
    {"id": "headcount", "label": "Effectif", "kind": "number", "choices": [], "shared_with_model": False},
    {"id": "interview_date", "label": "Date de l'entretien", "kind": "date", "choices": [], "shared_with_model": False},
    {"id": "role_note", "label": "Fonction", "kind": "text", "choices": [], "shared_with_model": True},
]
cfg["memo_templates"].append(
    {"id": "short", "name": "Court", "default": False, "version": 1, "questions": [{"id": "q1", "text": "Quel est l'essentiel ?"}, {"id": "q2", "text": "Quelle suite donner ?"}]}
)
call("PUT", "/api/project-config", cfg)
call("PUT", f"/api/interviews/{IV}/attributes", {"values": {"sector": "Distribution", "headcount": "120", "interview_date": "2026-09-14", "role_note": "Responsable des opérations"}})
call("PUT", f"/api/interviews/{IV}/memo/answers", {"answers": {"q1": "Gain d'exactitude des stocks, coût des lecteurs sous-estimé.", "q2": "Une surprise sur le rôle des clients."}}, ok=False)
call("PUT", f"/api/interviews/{IV}/checklist", {"checked": {"turns_seen": True}, "note": "Reste à relire les derniers tours."}, ok=False)

# Phase 2 human coding of every respondent unit.
HUMAN = {
    2: ["NA"], 4: ["CLIENT_PRESSURE"], 6: ["STOCK_ACCURACY"], 8: ["STOCK_ACCURACY"], 10: ["READER_COST"],
    12: ["CHANGE_RESISTANCE"], 14: ["SKILLS_GAP"], 16: ["SKILLS_GAP", "READER_COST"], 18: ["CLIENT_PRESSURE"],
    20: ["PRIVACY_CONCERN"], 22: ["CHANGE_RESISTANCE"], 24: ["READER_COST"], 26: ["CLIENT_PRESSURE"], 28: ["STOCK_ACCURACY"],
}
# Unit 28 first gets an open label to clarify, which allows an empty code list.
call("POST", f"/api/interviews/{IV}/units/{unit_id(28)}/to-clarify", {"label": "Mesure sur douze mois", "note": "Quel code pour la période de mesure ?"})
HUMAN[28] = []
for n, codes in HUMAN.items():
    uid = unit_id(n)
    call("PUT", f"/api/interviews/{IV}/phases/2/coding/{uid}", {"unit_id": uid, "codes": codes, "subcode": "", "modifier": "", "ev_level": "", "provenance": "", "memo": "", "excerpt": "", "revised_after_ai": False})

# Blind AI challenge: pasted run that disagrees on three units.
parts = call("POST", f"/api/interviews/{IV}/phases/2/prompts", {"batch_size": 30})
part = parts[0]
ai = {2: ["NA"], 4: ["CLIENT_PRESSURE"], 6: ["STOCK_ACCURACY"], 8: ["READER_COST"], 10: ["READER_COST"], 12: ["SKILLS_GAP"],
      14: ["SKILLS_GAP"], 16: ["SKILLS_GAP"], 18: ["CLIENT_PRESSURE"], 20: ["PRIVACY_CONCERN"], 22: ["CHANGE_RESISTANCE"],
      24: ["READER_COST"], 26: ["CLIENT_PRESSURE"], 28: ["STOCK_ACCURACY"]}
answer = {
    "schema": "coding-studio/ai-coding/v1", "interview_id": IV, "phase": 2, "segmentation_hash": part["segmentation_hash"],
    "part": part["part"], "of": part["of"],
    "codings": [{"unit_id": u, "codes": ai.get(int(u[-3:]), ["NA"])} for u in part["unit_ids"]],
}
run = call("POST", f"/api/interviews/{IV}/phases/2/runs", {
    "run_label": "run-a", "model_name": "modele-test", "model_version": "1", "temperature": 0, "seed": "1", "context_length": 8000,
    "batch_size": 30, "prompt_id_version": "", "executed_at": "2026-10-13T10:00:00Z", "notes": "", "batch_id": None})
call("POST", f"/api/interviews/{IV}/phases/2/runs/{run['run_id']}/outputs", {"raw": json.dumps(answer), "part": part["part"], "of": part["of"]}, ok=False)

review = call("GET", f"/api/interviews/{IV}/review", None, ok=False)
if review and review["items"]:
    it = review["items"][0]
    print("review item:", json.dumps({k: it[k] for k in it if k not in ("unit", "allowed", "history")})[:300])

# A case with its evidence.
def field(text, evidence=()):
    return {"text": text, "evidence": list(evidence)}


call("POST", f"/api/interviews/{IV}/cases", {
    "case_id": "", "interview_id": IV, "label": "Pilote d'étiquetage des cartons",
    "trigger": field("Un fournisseur étiquette à la source", [unit_id(4)]), "alternatives": field("Rester au code-barres"),
    "constraints": field("ERP ancien", [unit_id(14)]), "choice": field("Lancer un pilote"),
    "result": field("Exactitude de 70 % à 98 %", [unit_id(8)]), "stage": None, "outcome_status": None, "notes": None,
    "units": [unit_id(4), unit_id(8), unit_id(14)]})
print("seed done")
