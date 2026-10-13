# Common contracts (13.10.2026)

Binding for the three work streams that depend on Coding Studio: the tool itself (B), the thesis manuscript (C) and the VapeShop application dossier (D). The authority on the method is the short reference `03_Method/generated/2026-10-12_METHOD_reference_v4.md` in the thesis folder (rules R1 to R6, decisions D1 to D8) and the thesis Decision Log (D-011, D-012, D-016). `docs/METHOD.md` describes the procedure as implemented; `docs/API.md` describes the routes. When the three disagree, the short reference wins and the other two are corrected.

Four states are distinguished everywhere, in the tool's exports as in the manuscript: designed, implemented, tested, established by the data. Nothing moves to the next state without the corresponding act.

## 1. Research questions and codes

- Research questions live in `research_questions` and are entered by the candidate, independently of the codebook. A code maps to zero, one or several questions (`Code.research_questions`). The table "codes by question" is generated from this mapping; the questions are never generated from the codes.
- A code carries its origin (`literature`, `field`, `legacy`, `phase1`), its TOE attachment (`T`, `O`, `E` or none), its version, and for field codes the unit of origin in `reason` or `example`. "From the field" and "outside the TOE frame" are two independent properties.
- Codebook v0.2 section 12 (coding procedure) is superseded by `docs/METHOD.md`; the manuscript says so.

## 2. Single source of reference

- Coding Studio's database is the source of the codebook, the segmentation, the human coding, the AI answers, the review, the claims, the flags, the corrections, the verifications, the cases, the memos and the audit log. The earlier rule of the manuscript ("coding sheets are the source, the database is regenerable") is withdrawn (D-012).
- `coding.db` (the legacy Python base) is an import adapter only. Nothing is written back to it.
- Every figure in the manuscript comes from one export package identified by its digests. No number is typed by hand; `\XNUM{}` stands until the export exists.

## 3. Versions and export packages

- An export package is a folder `exports/<date>_<interview>/` or `exports/<date>_cross-case/` with a `manifest.json` listing every file with its sha256, a `state.json` with the sha256 of the human codings, claims, cases, case units, decisions, flags, verifications and codebook, plus `generator: {version, git}`, and a `db_snapshot.sqlite` taken with `VACUUM INTO` at export time.
- A package is never modified. A later export is a new folder. The manuscript cites one package per table and names it in the appendix on the coding database.
- File list per interview (names as written by `crates/cs-app/src/export.rs`): `units.csv`, `phase1_contexts.csv`, `phase2_coding_human.csv`, `phase2_human_full.csv`, `phase2_coding_final.csv`, `phase2_agreement.md`, `phase2_review.md`, `to_clarify.csv`, `codebook.csv`, `prompts/`, `answers/`, `human_snapshot_<batch>.json`, `cases.csv`, `case_units.csv`, `cases.json`, `cases.md`, `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv`, `units_verification.csv`, `unit_corrections.csv`, `audit_log.jsonl`, `audit_chain.json`, `state.json`, `db_snapshot.sqlite`, `manifest.json`, `README.md`. Cross-case: `answers.md`, `cross_case.csv`, `saturation.csv`, `saturation.md`, `cases.csv`, `state.json`, `manifest.json`, `README.md`. The manuscript's table generator (`06_Writing/thesis/tables/generated/build_tables_from_exports.py`) reads `codebook.csv`, `cases.csv`, `claims.csv`, `ev_distribution.csv`, `units_verification.csv`, `cross_case.csv`, `saturation.csv`, `manifest.json` and `state.json`, and refuses a package whose manifest digests do not match.
- Counting unit of every distribution is stated in the file header and in the README of the package.

## 4. Claims and provenance

- A claim is an exact excerpt of the current unit text, with an evidence level (EV0 to EV4), a provenance (SPONTANEOUS, PROBED, DOCUMENT-DERIVED, RESEARCHER-INFERRED), a role in the decision (trigger, retrospective-justification, enabling-condition, constraint, undetermined) and the codes of the unit it supports. Distributions count distinct claims. A claim is flagged for review when its codes leave the unit's coding or its excerpt leaves the unit's text.
- Per-unit `ev_level`, `provenance` and `modifier` fields of `HumanCoding` are legacy and are not used in the manuscript's tables.

## 5. Cases across interviews

- A case is one identifiable decision about item-level identification. It is documented only by the units explicitly attributed to it (`case_units`), from any interview. A case without attributed units is undocumented: its cells in the cross-case matrix are empty and marked as such. The interview of a case is never taken as a whole.
- Attributed units (the base of the matrix) are distinct from the excerpts chosen to illustrate a field of the synthesis (`evidence` per field).
- Case fields: decision (adopted, deferred, rejected), granularity (item, batch, model), carrier chosen and carriers rejected, kind of account (first hand, reported, ecosystem, anticipation), Swiss link; for trigger, alternatives, constraints, choice and result: what the data support, contradictions, what is missing.

## 6. Human reference and the challenger

- A prompt batch can only be generated when every unit of the batch carries the candidate's phase 2 coding (codes, or an open label which counts as an empty coding). Generating the batch freezes `human_snapshots` (coding, units digest, codebook digest), append-only.
- The initial comparison, the revisions after the model was read, and the final decision are three objects: the comparison and the agreement sample are computed on the snapshot; later human edits are stored as current coding with `revised_since_snapshot`; the arbitration records the final codes with the choice made (human, model, custom) and the rationale.
- The prompt contains the offered codes with definitions, grouped by TOE attachment, and the units; never the candidate's codes, phase 1 contexts, labels, memos, claims, flags or corrections. Unit identifiers found in code texts are redacted (`[unit]`).
- Any provider (copy-and-paste, command line, API) records the model name and version, the parameters it could set, the parameters it could not, the place of execution (local command is not local model), the prompt sha256 and the raw answer byte for byte.

## 7. Verification against the recording

- States: unverified, verified, unusable. A correction of text or speaker is an append-only overlay with a reason; the segmentation is never rewritten; the unit returns to unverified and is marked stale.
- Targets: every quoted unit (excerpt or claim), every flagged unit (VERIFY), every disagreement, every unit attributed to a case, plus a sample of max(5, 10 %) of the rest drawn with a recorded seed.
- Coverage (`n_units`, `n_verified`, `n_unusable`, `n_stale`, `n_targets`, `n_targets_verified`) is exported and reported in the manuscript next to any count; counts describe the eligible coded corpus.

## 8. Coverage by question and consent

- For each research question the memo holds what the data document, the interpretation, the competing explanations and the limits, each pointing to units for and against. `answers.md` exports the memos; it is never derived from frequencies.
- The state of coverage by question (identifiable decisions, adoption or not, internal need or external requirement, imposed or discussed choice, elements relevant to an SME) is kept by the candidate; every gap leads to a targeted follow-up or to an explicit limit in the answer.
- Before the first AI run on real data, the candidate records which provider is used, whether texts leave the machine, and whether the consent form covers it; the appendix on AI use reproduces this record.

## 9. Saturation

- The saturation record lists, in the order in which interviews were first coded, the codes used for the first time and the field codes created, with labels opened and resolved. It is provisional. The enrichment of a code's meaning is documented in memos and is not computed. A stop decided by recruitment or calendar is named as such.
