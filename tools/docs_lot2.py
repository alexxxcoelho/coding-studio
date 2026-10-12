"""One-off: lot 2 documentation updates (METHOD.md, API.md, README.md).
Run from the repository root: python3 tools/docs_lot2.py"""
import pathlib

root = pathlib.Path('.')

# METHOD.md: new steps in part one and part two.
p = root / 'docs/METHOD.md'
s = p.read_text()

old = "### 7. Synthesis by case"
new = """### 6a. Claims: the unit of evidence qualification

A unit may carry several statements that do not share the same standing. The qualifications of evidence are therefore attached to the statement, not to the unit: the candidate marks an exact passage of the unit, the claim, and gives it an evidence level (EV0 to EV4), a provenance (spontaneous, probed, derived from a document, inferred by the researcher) and a role in the decision described (trigger, retrospective justification, enabling condition, constraint, or undetermined), together with the codes of the unit it supports. A claim may support several codes without counting as several pieces of evidence; distributions of evidence levels, provenances and roles count distinct claims, and the counting unit is stated wherever a figure is given. A story told after the event is not a retrospective justification by default: the role is assigned only when the justification is identifiable in the passage. Roles distinguish positions in the decision; they do not measure a causal weight. Claims are revisited when the codes of their unit change or when the text of the unit is corrected.

### 6b. Verification against the recording

The transcript is checked against the recording for every unit that carries a quotation or a claim, every unit flagged as uncertain, every disagreement with the model, every unit attributed to a case, and a sample of the remaining units of at least five or a tenth of them, drawn by a deterministic procedure whose seed is recorded. Each unit is in one of three states: unverified, verified, or unusable. A correction of the text or of the speaker never rewrites the segmentation: it is recorded as a dated overlay with its reason, the original is kept, and the unit returns to the unverified state until it is checked again. Four flags qualify a unit at any time: uncertain attribution or wording, second-hand account, unfinished sentence, and co-constructed content, the last one distinguishing a mere assent to the interviewer's wording from an answer with content of its own. Counts describe the eligible coded corpus; the coverage of the verification is reported next to them, separately. A unit whose text or speaker remains uncertain supports no conclusion until the uncertainty is resolved.

### 7. Synthesis by case"""
assert old in s
s = s.replace(old, new, 1)

old = "### 8. Audit trail"
new = """### 7a. Answers by question and the saturation record

For each research question the candidate keeps a memo in four separate parts: what the data document, the interpretation, the competing explanations, and the limits, each part pointing to the units that support or contradict it. The answers of the thesis are written from these memos, never derived from frequencies. A saturation record lists, in the order in which the interviews were coded, the codes used for the first time and the codes created from the field; the enrichment of the meaning of a code is documented in memos and is not computed. The record is provisional, and a stop decided by recruitment or by the calendar is named as such.

### 8. Audit trail"""
assert old in s
s = s.replace(old, new, 1)

s = s.replace("each case with its attributed units, and each export.",
              "each case with its attributed units, each claim, each flag, each correction, each verification, each memo, and each export.")
s = s.replace("the case syntheses with their attributed units, the codebook, the relevant part of the record and a manifest of the digests of all these files.",
              "the case syntheses with their attributed units, the claims and their distributions, the verification states and corrections, the codebook, the relevant part of the record, a digest of the whole state the package was produced from with the version of the program, a copy of the database, and a manifest of the digests of all these files. A package is never modified; the empirical tables of the final manuscript come from a single package.")

old = "### Integrity of the record"
new = """### Claims, verification and memos

Claims are entered on the unit, the excerpt being selected in the unit text and checked as an exact substring; the closed lists of evidence levels, provenances and roles are enforced, and the codes of a claim must belong to the unit's current coding. Flags and corrections are entered on the unit; the program computes the verification state from the latest verification and the current text, so that a correction makes a verified unit unverified again. The list of units to verify and the coverage figures are computed on request, the sample of the remaining units being drawn once with a recorded seed. The memos by research question are edited in the program and exported as the answers file; the saturation record is computed from the audit log and the current coding. The prompt sent to the model redacts any unit identifier found in the texts of the codebook, so that the origin of a field code cannot reveal the candidate's coding of the units being challenged.

### Integrity of the record"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)

# API.md
p = root / 'docs/API.md'
s = p.read_text()
s += """
## Lot 2 additions (12.10.2026 evening, decisions D1 to D8)

- Flags: `GET /api/interviews/{id}/flags`, `PUT /api/interviews/{id}/units/{unit_id}/flags` `{flags, co_constructed_kind?}` (closed list VERIFY, SECOND-HAND, UNFINISHED, CO-CONSTRUCTED; the kind, assent or content, is required with CO-CONSTRUCTED).
- Corrections: `GET`/`POST /api/interviews/{id}/units/{unit_id}/corrections` `{text, role, reason}`; append-only overlay, the segmentation is never rewritten; `unit.corrected` and the current text appear in the phase unit rows.
- Verification: `GET /api/interviews/{id}/verification` -> `{status: VerificationStatus[], targets: VerificationTarget[], coverage}`; `PUT /api/interviews/{id}/units/{unit_id}/verification` `{state: verified|unusable, note?}`; a correction makes the state unverified and stale.
- Claims: `GET /api/interviews/{id}/claims`, `GET /api/interviews/{id}/units/{unit_id}/claims`, `POST /api/interviews/{id}/claims` (`Claim`, excerpt exact substring, codes among the unit's codes, closed lists EV0..EV4, provenances, roles), `PUT`/`DELETE /api/claims/{claim_id}`, `GET /api/interviews/{id}/claims/review` -> `[{claim_id, reason}]`.
- Memos: `GET`/`PUT /api/research-questions/{id}/memo` (`RqMemo`, four parts, units for and against).
- Saturation: `GET /api/saturation` -> `SaturationRow[]` (provisional).
- Exports: per interview adds `units_verification.csv`, `unit_corrections.csv`, `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv`, `state.json` (digest of the whole state, generator version and git description) and `db_snapshot.sqlite`; `POST /api/export/cross-case` writes `answers.md`, `cross_case.csv`, `saturation.csv`, `saturation.md`, `cases.csv`, `state.json`, `manifest.json`, `README.md`.
- Prompt version `CS-P04-EN v3.1`: unit identifiers found in code texts are redacted as `[unit]`.
"""
p.write_text(s)

# README.md
p = root / 'README.md'
s = p.read_text()
old = "## Protocol guarantees"
new = """7. Claims and verification. On each unit, mark the exact passages that carry a statement and qualify them (evidence level, provenance, role in the decision); flag uncertain, second-hand, unfinished or co-constructed units; record corrections against the recording with a reason; mark units verified or unusable. The Verification tab lists what must be checked and the coverage reached.
8. Answers. Keep a memo per research question (documented, interpretation, competing explanations, limits) and export the cross-case package (answers, matrix, saturation record).

## Protocol guarantees"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("- Raw answers kept verbatim.",
              "- Corrections never rewrite the segmentation: they are append-only overlays, and a corrected unit returns to the unverified state.\n- The prompt redacts unit identifiers found in code texts, so the codebook cannot reveal the candidate's coding of the challenged units.\n- Raw answers kept verbatim.")
s = s.replace("| `cases.csv`, `case_units.csv`, `cases.json`, `cases.md` | Cases, their attributed units and their syntheses |",
              "| `cases.csv`, `case_units.csv`, `cases.json`, `cases.md` | Cases, their attributed units and their syntheses |\n| `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv` | Claims and their distributions (counting unit: the claim) |\n| `units_verification.csv`, `unit_corrections.csv` | Verification states, flags, corrections |\n| `state.json`, `db_snapshot.sqlite` | Digest of the whole state behind the package, generator version, database copy |")
p.write_text(s)
print('docs updated')
