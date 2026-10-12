# Dev graph, lot 2 (12.10 evening): verification, claims, memos, saturation, challenger guard

Source: `03_Method/generated/2026-10-12_METHOD_reference_v4.md` (decisions D1 to D8, rules R3 to R6). Same pipeline as lot 1 (`DEVGRAPH_2026-10-12.md`): Sonnet writes tests red, Haiku implements, Haiku runs. Branch `v4-cases-snapshots` continues.

## Contracts

### C3. Audio verification, flags and corrections (D1)

- Table `unit_flags (interview_id, unit_id, flags TEXT JSON array, co_constructed_kind TEXT NULL, updated_at, PRIMARY KEY (interview_id, unit_id))`. Flags closed list: `VERIFY`, `SECOND-HAND`, `UNFINISHED`, `CO-CONSTRUCTED`; `co_constructed_kind` (`assent` | `content`) required when `CO-CONSTRUCTED` is set, refused otherwise. Editable at any time, audited (`unit.flags`, before/after).
- Table `unit_corrections (correction_id AUTOINCREMENT, interview_id, unit_id, text, role, reason NOT NULL, created_at, created_by)`, append-only (triggers). The segmentation is never rewritten: the current text and role of a unit are those of its latest correction, else the segmented ones. `Store::current_unit` returns the corrected unit; `Store::segmentation` keeps the original. `Unit` gains `corrected: bool` (`#[serde(default)]`). Audited `unit.correct`.
- Table `unit_verifications (verification_id AUTOINCREMENT, interview_id, unit_id, state TEXT, text_sha256, role, note, created_at, created_by)`, append-only; `state` is `verified` or `unusable`. The current status is computed: `unverified` when no row; else the latest row's state, downgraded to `unverified` with `stale: true` when its `text_sha256` or `role` differ from the current unit. Model `VerificationStatus { unit_id, state, stale, at: Option<String>, by: Option<String>, note: Option<String> }`. Audited `unit.verify`.
- `Store::verification_targets(interview_id) -> Vec<VerificationTarget { unit_id, reasons: Vec<String> }>`: units that must be verified: `quoted` (an `excerpt` in the human coding or a claim on the unit), `flagged` (`VERIFY` flag), `disagreement` (a disagreement in the current review), `case` (unit in `case_units`), plus `sample`: `max(5, ceil(10%))` of the remaining units drawn with the same seeded shuffle as the review sample; the seed is stored in `settings` under `verification_sample_seed:<interview_id>` on first draw and audited (`verification.sample`). Deterministic afterwards.
- `Store::verification_coverage(interview_id) -> VerificationCoverage { n_units, n_verified, n_unusable, n_stale, n_targets, n_targets_verified }`.
- Export (T18): `units_verification.csv` (`unit_id,state,stale,reasons,flags,co_constructed_kind,corrected`), coverage in README and status. Counts describe the eligible coded corpus; verification coverage is reported separately.

### C4. Claims: qualifications per statement (D2, D3)

- Table `claims (claim_id AUTOINCREMENT, interview_id, unit_id, excerpt TEXT NOT NULL, codes TEXT JSON array, ev_level TEXT NULL, provenance TEXT NULL, role TEXT NULL, memo TEXT NULL, created_at, updated_at)`. Rules: `excerpt` exact substring of the current unit text (after correction); `codes` subset of the unit's current phase 2 codes (may be empty); closed lists: `ev_level` EV0..EV4, `provenance` SPONTANEOUS | PROBED | DOCUMENT-DERIVED | RESEARCHER-INFERRED, `role` trigger | retrospective-justification | enabling-condition | constraint | undetermined. Validation errors name the field. Create, update, delete allowed; all audited (`claim.create` / `claim.update` / `claim.delete`, before/after); deletion leaves the audit row.
- Model `Claim { claim_id, interview_id, unit_id, excerpt, codes: CodeSet, ev_level, provenance, role, memo, created_at, updated_at }`.
- `HumanCoding.modifier` accepted values extended with the four roles and `undetermined` (per-unit fields stay, documented as legacy).
- Store: `claims(interview_id)`, `unit_claims(interview_id, unit_id)`, `create_claim(interview_id, &Claim, actor)`, `update_claim(claim_id, &Claim, actor)`, `delete_claim(claim_id, actor)`, `claims_to_review(interview_id) -> Vec<(claim_id, reason)>`: claims whose `codes` are no longer in the unit's current codes, or whose excerpt is no longer a substring of the current text.
- Export (T18): `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv` counting distinct claims; README states the counting unit.

### C5. Research question memos (R6, point 5)

- Table `rq_memos (rq_id TEXT PRIMARY KEY REFERENCES research_questions(id), documented TEXT, interpretation TEXT, rivals TEXT, limits TEXT, units_for TEXT JSON, units_against TEXT JSON, updated_at)`, audited `rq_memo.update`. Unit ids must exist in their interview's current segmentation. Model `RqMemo`.
- Store: `rq_memo(rq_id) -> Option<RqMemo>`, `set_rq_memo(&RqMemo, actor)`, `rq_memos()`.
- API (T17): `GET/PUT /api/research-questions/{id}/memo`. Export (T18): global export `POST /api/export/cross-case` -> `<db folder>/exports/<date>_cross-case/` with `answers.md` (per question: the four parts, units for and against), `cross_case.csv`, `saturation.csv`, `saturation.md`, `manifest.json`.

### C6. Saturation table (D4)

- `Store::saturation() -> Vec<SaturationRow { interview_id, order: u32, first_coded_at: String, codes_first_used: Vec<String>, field_codes_created: Vec<String>, labels_opened: u32, labels_resolved: u32 }>`: interviews ordered by the timestamp of their first phase 2 human coding event in the audit log (`coding.set` or whatever the action is named; check) ; a code is first used in the first interview, in that order, whose current phase 2 human coding carries it; a field code is created in the interview of the unit whose label resolution created it (`to_clarify_resolutions` kind `new_code`), else in the interview being coded at its `created_at`, else listed under `interview_id = ""`; labels opened/resolved counted from `to_clarify`. Header of the export says the table is provisional and that enrichment of meaning is not computed.
- API (T17): `GET /api/saturation`.

### C7. Challenger package guard and export manifest (point 8, R3)

- cs-ai: `PromptPart` builder gains redaction: any unit id of the prompted interview (`<ID>-U<nnn>`) found in a code's `example` or `reason` is replaced by `[unit]`; the prompt never includes memos, excerpts, claims, flags, labels or phase 1 contexts. Test: build a prompt from a store (or in-memory inputs) where every such field holds a sentinel string; assert the prompt text does not contain it and does not contain any unit id of the interview in the codebook section.
- cs-app export: `state.json` with sha256 of the interview's human codings, claims, cases, case_units, decisions, flags, verifications and of the codebook, plus `generator: {version: env!("CARGO_PKG_VERSION"), git: <git describe or "unknown">}`; `db_snapshot.sqlite` produced with `VACUUM INTO` (documented in README as the immutable state behind the package). Manifest covers both.

## Tasks

| id | task | scope | owner | depends | status |
|---|---|---|---|---|---|
| T12 | Model types C3 to C6 and schema tables + triggers | cs-model lib.rs, cs-store schema.sql | Haiku | none | todo |
| T13 | Tests C3 (red) | cs-store tests/verification_v4.rs | Sonnet | T12 | todo |
| T14 | Tests C4, C5, C6 (red) | cs-store tests/claims_v4.rs, tests/memos_saturation_v4.rs | Sonnet | T12 | todo |
| T15 | Implement C3 | cs-store src/verification.rs (new), lib.rs (`current_unit`), coding.rs (excerpt check on current text) | Haiku | T13 | todo |
| T16 | Implement C4, C5, C6 | cs-store src/claims.rs, src/memos.rs, src/saturation.rs (new), lib.rs mod decls | Haiku | T14 | todo |
| T17 | API routes + api.rs tests | cs-app lib.rs, tests/api.rs | Sonnet then Haiku | T15, T16 | todo |
| T18 | Exports C3 to C7, state.json, db snapshot; cs-ai redaction + sentinel test | cs-app export.rs, cs-ai prompt.rs + tests.rs | Haiku | T17 | todo |
| T19 | Web: verification panel, claims editor, RQ memo page, saturation page | web/src | Haiku | T17 | todo |
| T20 | Docs and full check, bundle | docs, all | orchestrator + Haiku run | T18, T19 | todo |

## Log

- 12.10 21:45 lot 2 opened after decisions D1 to D8.
