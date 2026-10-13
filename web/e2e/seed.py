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

# The project reads French; the first example interview is in English.
call("PUT", "/api/lexicon/config", {"language": "fr", "interview_languages": {IV: "en"}})

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

# The codebook of the V1 routes names a code by its handle; the researcher's own name is the label.
for _c in call("GET", "/api/codes")["codes"]:
    if _c["label"] and _c["name"] == _c["token"]:
        call("PUT", f"/api/codes/{_c['code_id']}", {"name": _c["label"]})

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
passage(28)  # highlights for the lexical workbench
passage(16, start=0, end=units[unit_id(16)]["text"].index(".") + 1)

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
    # Two concepts outside the codebook, each with a quotation word for word.
    "candidate_concepts": [
        {
            "label": "Départ de la personne qui connaît le système",
            "description": "Le projet perd des mois quand la seule personne qui comprend l'entrepôt et la base de données s'en va.",
            "unit_ids": [unit_id(14)],
            "evidence": [{"unit_id": unit_id(14), "quote": "We had one person who understood both the warehouse and the database"}],
        },
        {
            "label": "Partage du coût des étiquettes par le fournisseur",
            "description": "Le fournisseur prend à sa charge une partie du coût, ce qui rend l'amortissement possible.",
            "unit_ids": [unit_id(10)],
            "evidence": [{"unit_id": unit_id(10), "quote": "the supplier shares part of the tag cost"}],
        },
    ],
}
for c in answer["candidate_concepts"]:
    for ev in c["evidence"]:
        assert ev["quote"] in units[ev["unit_id"]]["text"], ev
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

# ---- Analyse and Rédiger (lot 5): themes, matrix, write-up, links, final audit, decisions.
# A second interview (INT-02, in French) gives the matrix a second row and the lexical workbench a second language.
IV2 = "INT-02"
seg2 = call("POST", f"/api/interviews/{IV2}/segment", {})
units2 = {u["unit_id"]: u for u in seg2["units"]}
for n, code in [(6, "STOCK_ACCURACY"), (8, "STOCK_ACCURACY"), (10, "READER_COST"), (12, "CHANGE_RESISTANCE"), (14, "SKILLS_GAP"), (18, "CLIENT_PRESSURE"), (20, "PRIVACY_CONCERN")]:
    call("POST", f"/api/interviews/{IV2}/passages", {"unit_id": f"{IV2}-U{n:03d}", "code": code})
for n, span in [(4, None), (16, (0, 60)), (22, None), (26, None)]:  # highlights
    body = {"unit_id": f"{IV2}-U{n:03d}"}
    if span:
        body.update({"start": span[0], "end": span[1]})
    call("POST", f"/api/interviews/{IV2}/passages", body)
HUMAN2 = {2: ["NA"], 4: ["CLIENT_PRESSURE"], 6: ["STOCK_ACCURACY"], 8: ["STOCK_ACCURACY"], 10: ["READER_COST"], 12: ["CHANGE_RESISTANCE"],
          14: ["SKILLS_GAP"], 16: ["SKILLS_GAP"], 18: ["CLIENT_PRESSURE"], 20: ["PRIVACY_CONCERN"], 22: ["CHANGE_RESISTANCE"],
          24: ["READER_COST"], 26: ["STOCK_ACCURACY"]}
for n, codes in HUMAN2.items():
    uid = f"{IV2}-U{n:03d}"
    call("PUT", f"/api/interviews/{IV2}/phases/2/coding/{uid}", {"unit_id": uid, "codes": codes, "subcode": "", "modifier": "", "ev_level": "", "provenance": "", "memo": "", "excerpt": "", "revised_after_ai": False})

tree = call("GET", "/api/codes")["codes"]
cid = {c["token"]: c["code_id"] for c in tree}

# A memo on a code, a unit to review, the origin and role of a passage.
call("PUT", f"/api/codes/{cid['READER_COST']}/memo", {"text": "Le coût des lecteurs et de l'intégration, à distinguer du prix des étiquettes."})
call("PUT", f"/api/interviews/{IV}/units/{unit_id(16)}/mark", {"note": "Deux codes sur ce tour, à relire."})
first = call("GET", f"/api/interviews/{IV}/passages")
pq = [p for p in first if any(c["code_id"] == cid["READER_COST"] for c in p["codings"])]
if pq:
    call("PUT", f"/api/passages/{pq[0]['passage_id']}/qualification", {"origin": "lived_experience", "role": "constraint"})

def theme(title, codes, rqs, fields):
    t = call("POST", "/api/themes", {"title": title})
    for c in codes:
        call("POST", f"/api/themes/{t['theme_id']}/codes", {"code_id": cid[c]})
    call("PUT", f"/api/themes/{t['theme_id']}", {"research_questions": rqs, **fields})
    return t["theme_id"]

T1 = theme(
    "Le coût d'intégration pèse plus lourd que celui des étiquettes",
    ["READER_COST", "SKILLS_GAP"],
    ["RQ1"],
    {
        "proposition": "Les organisations découvrent que l'intégration au système existant coûte plus que le matériel.",
        "conditions": "Un système de gestion ancien et peu de compétences internes.",
        "alternative_interpretation": "Le coût perçu tiendrait surtout à un chiffrage initial incomplet.",
        "limits": "Deux entretiens, un seul secteur.",
        "decision_memo": "Retenu après relecture des passages des deux entretiens.",
        "supporting_cases": [f"{IV}-C1"],
    },
)
T2 = theme(
    "La pression des clients déclenche le projet avant tout besoin interne",
    ["CLIENT_PRESSURE", "STOCK_ACCURACY"],
    ["RQ1", "RQ2"],
    {"proposition": "Le projet part d'une exigence extérieure; le gain de stock vient ensuite.", "conditions": "Un client qui impose le marquage à la source."},
)
checks = [c["id"] for c in call("GET", "/api/project-config")["theme_checks"]]
for ck in checks:
    call("PUT", f"/api/themes/{T1}/checks/{ck}", {"done": True, "note": "Vérifié sur les passages."})
call("POST", f"/api/themes/{T1}/status", {"status": "retained", "reason": "Les sept contrôles sont faits."})
for ck in checks[:3]:
    call("PUT", f"/api/themes/{T2}/checks/{ck}", {"done": True, "note": ""})
call("POST", f"/api/themes/{T2}/status", {"status": "revised", "reason": "Titre reformulé en proposition."})

# Matrix: the researcher's reading of each cell.
cells = [
    (IV, T1, "present", "Le coût de l'intégration dépasse celui des étiquettes."),
    (IV, T2, "present", "Un fournisseur impose l'étiquetage à la source."),
    (IV2, T1, "contradicted", "Ici le matériel pèse davantage que l'intégration."),
    (IV2, T2, "uncertain", "La pression n'est évoquée qu'à demi-mot."),
]
for iv, t, value, summary in cells:
    call("PUT", "/api/matrix/cell", {"row_key": f"interview:{iv}", "col_key": f"theme:{t}", "value": value, "summary": summary})

# Write-up of the retained theme, with typed links.
parts = {
    "analytical_statement": ("Le coût d'intégration dépasse celui des étiquettes. Les lecteurs et le raccordement au système existant absorbent l'essentiel du budget.", "first_hand"),
    "mechanism_evidence": ("Le répondant chiffre d'abord les étiquettes, bon marché, puis découvre le prix des lecteurs. L'interface avec le système de gestion ancien a exigé un développement spécifique. Le départ de la seule personne qui comprenait l'entrepôt et la base de données a coûté des mois.", "first_hand"),
    "variations_divergent": ("Dans le second entretien, le matériel pèse davantage que l'intégration, ce qui nuance la proposition.", "reported"),
    "competing_limits": ("Un chiffrage initial incomplet pourrait expliquer à lui seul la surprise. Deux entretiens ne permettent pas de trancher.", "opinion"),
    "mini_synthesis": ("Pour les organisations de petite taille, l'intégration est le poste de coût que l'on sous-estime.", None),
}
for part, (text, nature) in parts.items():
    call("PUT", f"/api/writing/themes/{T1}/parts/{part}", {"text": text, **({"nature": nature} if nature else {})})
tp = call("GET", f"/api/themes/{T1}/passages")
kinds = {"analytical_statement": "supports", "mechanism_evidence": "mechanism", "variations_divergent": "exception", "competing_limits": "nuances"}
for i, (part, ltype) in enumerate(kinds.items()):
    p = tp[i % len(tp)]
    call("POST", "/api/links", {"passage_id": p["passage_id"], "target": {"kind": "theme_part", "theme_id": T1, "part": part}, "link_type": ltype, "note": "Ce passage appuie la partie."})
call("PUT", "/api/writing/questions/RQ1", {"answer": "L'adoption est freinée par le coût d'intégration, et déclenchée par une exigence extérieure.", "limits": "Un seul secteur, deux entretiens."})

# A decision with its outcome in the words of the guide.
d = call("GET", f"/api/decisions/{IV}-C1")
d["outcome"] = "adopt"
call("PUT", f"/api/decisions/{IV}-C1", d)

# Final audit: a seeded draw, with one verdict.
audit = call("POST", "/api/final-audit", {"n": 3, "seed": "controle-visuel"})
if audit["claims"]:
    call("POST", f"/api/final-audit/{audit['audit_id']}/verdicts", {"claim_key": audit["claims"][0]["claim_key"], "verdict": "to_nuance", "note": "À nuancer par le second entretien."})
print(f"analysis seeded: themes {T1}, {T2}")

# ---- Lot 4: units to harmonise after a change of the codebook, and the pseudonymisation list.
call("PUT", f"/api/codes/{cid['CHANGE_RESISTANCE']}", {"definition": "Réaction des équipes face au nouveau geste de travail, y compris le sentiment de perdre le coeur de son métier."})
call("POST", "/api/codes", {"name": "Écart de comptage", "definition": "Différence mesurée entre le stock compté et le stock du système.", "parent_id": cid["STOCK_ACCURACY"]})
w = call("GET", "/api/harmonisation")
print(f"harmonisation worklist: {w['n_open']} to reread")
call("PUT", "/api/pseudonymisation", {
    "enabled": True,
    "entries": [
        {"kind": "person", "original": "Claire Vuilleumier", "pseudonym": "Personne A"},
        {"kind": "organisation", "original": "Lectis", "pseudonym": "Fournisseur A"},
    ],
})
print("seed done")
