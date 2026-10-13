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

- An export package is a folder `exports/<date>_<interview>/` or `exports/<date>_cross-case/` with a `manifest.json` listing every file with its sha256, a `state.json` with the sha256 of the human codings, claims, cases, case units, decisions, flags, verifications, codebook, passages, code tree, attribute values, memos, checklists and project configuration, plus `generator: {version, git}`, and a `db_snapshot.sqlite` taken with `VACUUM INTO` at export time.
- A package is never modified. A later export is a new folder. The manuscript cites one package per table and names it in the appendix on the coding database.
- File list per interview (names as written by `crates/cs-app/src/export.rs`): `units.csv`, `phase1_contexts.csv`, `phase2_coding_human.csv`, `phase2_human_full.csv`, `phase2_coding_final.csv`, `phase2_agreement.md`, `phase2_review.md`, `to_clarify.csv`, `codebook.csv`, `prompts/`, `answers/`, `human_snapshot_<batch>.json`, `cases.csv`, `case_units.csv`, `cases.json`, `cases.md`, `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv`, `units_verification.csv`, `unit_corrections.csv`, `passages.csv`, `highlights.csv`, `attributes.csv`, `memos.json`, `study_config.json`, `audit_log.jsonl`, `audit_chain.json`, `state.json`, `db_snapshot.sqlite`, `manifest.json`, `README.md`. Cross-case: `answers.md`, `cross_case.csv`, `saturation.csv`, `saturation.md`, `cases.csv`, `codebook.csv`, `passages.csv`, `highlights.csv`, `attributes.csv`, `memos.json`, `study_config.json`, `state.json`, `manifest.json`, `README.md`. The manuscript's table generator (`06_Writing/thesis/tables/generated/build_tables_from_exports.py`) reads `codebook.csv`, `cases.csv`, `claims.csv`, `ev_distribution.csv`, `units_verification.csv`, `cross_case.csv`, `saturation.csv`, `manifest.json` and `state.json`, and refuses a package whose manifest digests do not match.
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

## 10. Passages, code tree and study configuration (V1 lots 1 and 2)

- A code is identified by `code_id` (`C-0001`, stable, never reused). Its `name` is free text and can change; each former name, each merged name and the legacy token stay as aliases and resolve to the same code. `token` is the legacy handle of the unit-level coding and the AI pass and never changes. Tables cite codes by `code_id` and `name`.
- A passage is `(interview, unit, start, end)` with the exact text and the digest of the unit text. Positions count Unicode characters from 0, end excluded. A passage whose unit text changed is flagged `stale`; it is never moved. A highlight is a passage with no active code. The reserved code "Plus tard" is not a code of the codebook and is counted in no table: the exports give it as the `later` column.
- Counts are given as passages and as distinct interviews, never mixed, and a rolled-up count includes the descendants of the code (a passage counted once).
- `passages.csv`: `passage_id,interview_id,unit_id,start,end,text,kind,code_ids,code_names,later,later_note,stale,anchor_digest,created_at,created_by`. `code_ids` and `code_names` are joined with `;`, in the same order. `kind` is `coded` or `highlight`.
- `highlights.csv`: `passage_id,interview_id,unit_id,start,end,text,stale,created_at,created_by`, the passages with no active code.
- `codebook.csv`: the thirteen columns of the earlier layout, then `code_id,name,parent_id,aliases,includes,excludes`. A reader that reads columns by name is unaffected.
- `attributes.csv`: `interview_id,attribute_id,label,kind,value`, one row per attribute value (long format, so that a change of the attribute list does not change the columns).
- `memos.json`: `{interviews: [{interview_id, memo, checklist}]}`. `memo` carries the template id and version the answers were written with, the interview's own added, removed and reworded questions, and each answer with its time. `checklist` carries the items of the study configuration with their checked time, and the note.
- `study_config.json`: the project template (`docs/API.md`, "Project template"): study settings, research questions, attribute definitions, memo templates, lists and framework. No interview data and no secret. Importing it into another project reproduces the configuration.
- The interpretive framework belongs to Analyse and Write. It is not in `codebook.csv`, not in the prompt, and not in any coding table.

## 11. REFI-QDA exchange (F-103, crate `cs-qdpx`)

Exchange with NVivo, MAXQDA and ATLAS.ti through the REFI-QDA standard: a codebook file (`.qdc`, XML, namespace `urn:QDA-XML:codebook:1.0`) and a project file (`.qdpx`, a ZIP with `project.qde`, namespace `urn:QDA-XML:project:1.0`, and `Sources/` text files). The XSD files could not be fetched when this was written; the structure follows the standard from knowledge (`crates/cs-qdpx/schemas/README.md`) and is checked in tests against element names and required attributes, not against the XSD. State: implemented and tested against the exporter's own files and hand-written files; not yet opened in the three programs.

**Identifiers.** Every GUID is a UUID version 5 in the namespace `6f2b7c1e-5a9d-4d34-8f0e-2c6a1b7d9e41`, from the name `code:<code_id>`, `source:<interview_id>`, `passage:<passage_id>`, `coding:<passage_id>:<code_id>`, `user:<name>`, or `highlights`. The same data therefore gives the same GUIDs, and the same bytes. The namespace and the names never change (a test freezes one value).

**Export.**
- Codes: active codes of the tree, not merged, without "Plus tard". `name`, `Description` = definition, children nested in codebook order, `isCodable="true"`. A child of a code that is not exported moves up to its nearest exported ancestor. No `color`. Aliases, includes, excludes, TOE attachment and research questions are not carried (REFI-QDA has no field for them).
- Sources: one `TextSource` per interview with units, `name` = interview id, `plainTextPath="internal://<guid>.txt"`. The plain text is every unit, interviewer and respondent, as stored with the unit corrections applied, joined by one blank line (`\n\n`). Nothing is added to the text. Interviewer turns are text only and are never coded.
- Offset map: `coding-studio-offsets.json` at the root of the archive: format, GUID namespace, position unit, separator, and per source its GUID, interview id, path, SHA-256, length and `units: [{unit_id, role, start, end}]`. Other tools ignore it; the importer uses it to find the interview of a renamed source.
- Positions: `startPosition` and `endPosition` of a `PlainTextSelection` count Unicode scalar values from 0 in the plain text, end excluded (the convention of passages). A passage `(unit, start, end)` is the selection `(unit.start + start, unit.start + end)`.
- Passages: one `PlainTextSelection` per passage with one `Coding/CodeRef` per exported code, `creatingUser` and `creationDateTime` from the passage. Left out and counted in the summary: stale passages, passages whose only codes are "Plus tard" or not exported, highlights. With the highlights option, highlights are exported with a code "Surlignages" (GUID from `highlights`). "Plus tard" is never exported.
- ZIP: entries `project.qde`, `Sources/<guid>.txt` per interview in interview id order, the offsets file; fixed entry time and permissions, so that the archive is a function of the data.
- Audit: `refi.export.qdc` and `refi.export.qdpx` with counts, size and SHA-256.

**Import.**
- A code of the file is matched by GUID (a GUID written by this crate for a code of the project), then by name, alias or legacy token without regard to case, following merges. Otherwise it is created (origin `literature` for a `.qdc`, `field` for a `.qdpx`; definition = `Description`, or a placeholder; reason "Import REFI-QDA"), under the code the file puts it under. A matched code is never edited or moved; a different position in the file is a warning. A name that is reserved ("NA", "Plus tard") or too long is skipped and its children move up. Two codes of the file with the same name are one code.
- A text source is matched with an interview by the GUID of the offsets file, else by name (interview id or folder name), and its text must be byte-identical to the interview's plain text (SHA-256). Without a match by name, an interview with the same text is accepted (`matched_by: digest`). Otherwise the source is reported and skipped; the codes are still imported.
- A selection becomes one passage per respondent unit it overlaps, clipped to the unit. Interviewer text and the blank line are dropped (warning). A selection with no usable code, past the end of the text, reversed, or entirely on interviewer text is skipped and reported. Sources that are not text (PDF, picture, audio, video) are listed and ignored. Variables, cases, notes, sets and graphs are ignored.
- Idempotent: the same file imported twice adds nothing. Dry run first (default), then apply.
- A file that is not a usable REFI-QDA file (malformed XML, wrong root element, not a ZIP, no `project.qde`, duplicated GUIDs, a code without name) is rejected with every problem listed; nothing is written, nothing is audited.
- The writes go through the store, one transaction each, so they are audited as usual (code create, move, passage create, coding), plus one `refi.import.qdc` or `refi.import.qdpx` event with counts and the SHA-256 of the file. The import as a whole is not one transaction.

## 12. Accepted transcript formats

- Accepted transcript formats: md, txt, vtt, srt, docx, pdf with text layer. A PDF without a text layer is inventoried but yields no turns.

## 13. Themes, matrix, links, write-up, final audit (V1 lot 5)

- A theme has a sentence as title, a status (candidate, revised, retained), linked codes (by `code_id`) and research questions, and a sheet. Retained needs every project check done or an override with a recorded reason. Changes are in `theme_history`, which is append-only.
- Counts on themes and matrix cells are computed from the codings at read and export time (passages and distinct interviews, never mixed). Only the researcher's inputs are stored: cell value (present, not_mentioned, contradicted, uncertain), summary, passages attached by hand, texts.
- A link joins a passage to a part of the write-up with a type from the project list. The exact quotation and the verification state travel with it.
- Writing: five parts per retained theme, a synthesis per research question, a framework reading table only if the project has a framework. Every save is a row in `writing_versions` (append-only).
- Final audit: three claims drawn with a recorded seed (SHA-256 ordering of the claim keys), replayable; verdicts (supported, to_nuance, to_withdraw) are appended to a log, never overwritten.
- Files added to the cross-case and write-up packages, all in `manifest.json`: `themes.csv` (`theme_id,order,title,status,proposition,code_ids,code_names,research_questions,passages,interviews,checks_done,checks_total,created_at,updated_at`), `theme_sheets.json`, `matrix.csv` (`view_id,view_name,row_key,row_label,col_key,col_label,passages,interviews,value,value_label,summary,passages_linked_by_hand`), `links.csv` (`link_id,passage_id,interview_id,unit_id,target_kind,target_id,part,link_type,note,quote,verification,created_at,created_by`), `final_audit.json`, `decisions.csv` (`decision_id,interview_id,name,outcome,outcome_label,decision_as_written,characteristics,units,passages`), `decisions.json`; `writeup.md` in the write-up package (footnotes: interview, unit, exact quotation).
- `state.json` parts gain `themes`, `writing_versions`, `links`, `matrix`, `final_audit`, `unit_marks`, and `memo` in the `code_tree` nodes; the state digest moves with them.

## 14. Review: prompt, pseudonymisation, candidates, harmonisation (V1 lot 4)

- The prompt is a pure function of the configuration, the codebook, the offered units and the pseudonymisation list. It never contains a human code, memo or highlight, and never an interviewer unit as a unit to code; the interviewer turn appears only as context in `## Interviewer questions`. Its SHA-256, the configuration digest and the pseudonymisation digest are in the audit.
- The human reference is frozen (table and event) before the prompt is generated, in the same transaction. A later edit of the human coding never alters a snapshot.
- Pseudonymisation is one project list, kind person or organisation. It is applied to everything sent to a model and reversed only for display. The audit and the exports keep the list digest, the categories and the counts, never the names. Free text is outside its reach and every view says so.
- A candidate concept is a proposal. Its quotes are checked word for word against the pseudonymised unit before it is stored. Create, map and reject each need a reason, are appended to `concept_decisions` and change no coding; `create` makes a code in the same transaction.
- A unit is coded with X when any passage in it has X. The rows written by the sync carry `source = passages`; a hand coding of the unit is never removed by a sync, only overwritten when passages exist.
- A merge, a redefinition (definition, includes, excludes) and a new sub-code open harmonisation items, one open item per interview, unit and code. Resolutions are `confirmed` or `corrected`.
- Review state per interview: `not_done`, `in_progress`, `complete`, with a declaration sentence for the documents of the study.
- Schema version 7. `review_schema::migrate` runs on every open and is idempotent for a database at any version below 7.
- Files: `review.json` in the interview and cross-case packages, and the `review` part of `state.json`.

