# Coding Studio HTTP API (v3)

Local JSON API served by `cs-app` on 127.0.0.1. The web UI (`web/`) is its only client. Design: `docs/DESIGN_V3.md`. Types are the serde forms of `crates/cs-model/src/lib.rs` (snake_case field names, snake_case enum values). Phase 1 is the candidate's provisional TOE pre-classification; phase 2 carries the concrete codes and is the only phase the AI challenges.

Errors: status 400 (validation), 404 (not found), 409 (protocol guard), 500 (internal), 501 (native dialog unavailable); body `{"error": string, "kind": "validation"|"guard"|"not_found"|"internal", "issues"?: ValidationIssue[], "rejected"?: {part, raw}[]}` (`rejected` only when a provider run refuses answers).

## Types (JSON)

- `Code`: `{code, family, label, definition, origin: "literature"|"field"|"legacy"|"phase1", toe: "T"|"O"|"E"|null, active, in_prompt, research_questions: string[], merged_into: string|null, version: number, example: string|null, reason: string|null, created_at: string|null}`. `family` is TECH, ORG or ENV from `toe`, otherwise OTHER (phase 1 categories carry their own name).
- `Codebook`: `{source, codes: Code[]}`.
- `Unit`: `{unit_id, seq, speaker_raw, role: "interviewer"|"respondent"|"unknown", timestamp, text}`.
- `HumanCoding`: `{unit_id, codes: string[], subcode, modifier, ev_level, provenance, memo, excerpt, revised_after_ai: boolean}` (`revised_after_ai` is set by the server and ignored on input).
- `ToClarify`: `{id, interview_id, unit_id, label, note, created_at, created_by, resolved: Resolution|null}`; `Resolution`: `{kind: "new_code"|"mapped", code, at, by, rationale: string|null}`.
- `ResearchQuestion`: `{id, text, ord}`.
- `CaseField`: `{text, evidence: string[] /* unit_ids */}`; `Case`: `{case_id, interview_id, label, trigger, alternatives, constraints, choice, result: CaseField, stage: string|null, outcome_status: string|null, notes: string|null, updated_at}`.
- `ArbitrationChoice`: `{type: "human"}` | `{type: "ai", run_label}` | `{type: "custom"}`; `Arbitration`: `{unit_id, phase, choice, final_codes: string[], rationale, decided_by, decided_at}`.
- `ReviewItem`: `{unit_id, kind: "disagreement"|"agreement_sample", human: string[], ai: {[run_label]: string[]}, decision: Arbitration|null}`.
- `FinalCoding`: `{unit_id, codes, basis: "candidate"|"reviewed"}`.
- `AiRunMeta`: `{run_label, model_name, model_version, temperature, seed, context_length, batch_size, prompt_id_version, executed_at, notes, batch_id: number|null}`.
- `PromptPart`: `{interview_id, phase: 2, batch_id, codes: string[], part, of, unit_ids, segmentation_hash, codebook_hash, prompt_version, text, sha256}`.
- `AiRun`: `{run_id, interview_id, phase: 2, meta, batch_id, unit_ids, codings: {[unit_id]: string[]}, parts_received, parts_expected, complete}`.
- `PhaseSummary`: `{phase, n_units, n_human_coded, human_complete, n_ai_runs, n_ai_runs_complete, n_divergences, n_sampled, n_arbitrated, n_to_clarify, reconciled}` (phase 1 fills only the unit and coding counts; `reconciled` = at least one complete run and every disagreement decided).
- `InterviewSummary`: `{interview_id, folder_name, n_sources, primary_source, n_units, n_warnings, phase1: PhaseSummary, phase2: PhaseSummary}`.
- `GuardStatus`: `{phase, complete, n_units, missing: string[]}`.

## State and library

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/state` | | `{version, db_path, library_root, coding_db, actor, codebook_size, chain: ChainCheck, native_dialogs}` |
| POST | `/api/pick` | `{kind: "folder"\|"file", title?}` | `{path: string\|null}` (501 in browser mode) |
| POST | `/api/library/scan` | `{root?}` | `{root, interviews: InterviewSummary[]}` |
| GET | `/api/interviews` | | `InterviewSummary[]` |
| GET | `/api/interviews/{id}` | | `{entry: InterviewEntry, segmentation: Segmentation\|null, phase1: PhaseSummary, phase2: PhaseSummary}` |
| GET | `/api/interviews/{id}/file?path=rel` | | `{path, kind, sha256, changed_since_scan, text}` |
| POST | `/api/interviews/{id}/primary` | `{rel_path}` | `InterviewEntry` |
| POST | `/api/interviews/{id}/segment` | `{rel_path?}` | `Segmentation`. Saving a segmentation fills the phase 1 coding of every unit that has none (automatic, audited once as `coding.auto_phase1`, see below). Re-segmenting keeps every candidate coding by unit id and is refused only when an AI run has answered a unit whose text or role would change |

## Coding

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/codebook/phase1` | | `Codebook` (TECH, ORG, ENV, CONTEXT) |
| GET | `/api/interviews/{id}/phases/{p}` | | `{phase, summary: PhaseSummary, guard: GuardStatus}` |
| GET | `/api/interviews/{id}/phases/1/units` | | `[{unit, coding: HumanCoding\|null, allowed: Code[]}]`, every unit; `allowed` = the four phase 1 categories (NA always valid besides) |
| GET | `/api/interviews/{id}/phases/2/units` | | `[{unit, coding: HumanCoding\|null, allowed: Code[], phase1: string[]\|null, in_scope: boolean, to_clarify: ToClarify[]}]`: the units in scope (phase 1 category not NA, never an interviewer turn) and the out-of-scope units that still carry a phase 2 coding, in segmentation order. `allowed` = every phase 2 code, those attached to the unit's phase 1 contexts first |
| GET | `/api/interviews/{id}/simple` | | `SimpleProgress`: `{n_respondent_units, n_relevant, n_coded, n_batches, n_runs_complete, n_disagreements, n_decided, n_sample, n_sample_reviewed, done}`. `n_respondent_units` counts the units whose role is not interviewer; `n_relevant` the units in phase 2 scope; `n_coded` the relevant units coded in phase 2; `done` when every relevant unit is coded and covered by a complete run, and every disagreement and sample item is decided. 404 for an unknown interview |
| PUT | `/api/interviews/{id}/phases/{p}/coding/{unit_id}` | `HumanCoding`, or the shorthand `{relevant: bool}` (phase 1 only) | `HumanCoding` as stored. Phase 1: TECH/ORG/ENV/CONTEXT or NA, always editable. Shorthand `{relevant: false}` stores NA; `{relevant: true}` stores CONTEXT unless the unit already carries a category other than NA, which is kept. Phase 2: phase 2 codes or NA; `codes` may be empty only if the unit has an open label to clarify. An interviewer unit cannot be relevant, and `relevant` in phase 2 is refused (400) |

## Labels to clarify

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/to-clarify?interview_id=&open=true` | | `[ToClarify & {unit_text}]` (both filters optional) |
| POST | `/api/interviews/{id}/units/{unit_id}/to-clarify` | `{label, note}` | `ToClarify` (unit must be in phase 2 scope) |
| PUT | `/api/to-clarify/{tid}` | `{label, note}` | `ToClarify` (open items only) |
| POST | `/api/to-clarify/{tid}/resolve` | `{kind: "new_code"\|"mapped", code, rationale?, all_with_label: boolean, new_code?: Code}` | `{resolved: ToClarify[], codebook: Codebook}`. `new_code` creates `new_code` (origin field, definition required, TOE attachment optional; `code` must equal `new_code.code`); `mapped` uses an existing phase 2 code. The code is added to the phase 2 coding of every resolved unit; `all_with_label` resolves every open item with the same label (trimmed, case-insensitive) across the corpus |

## AI challenge (phase 2 only; phase 1 answers 400)

| Method | Path | Body | Answer |
|---|---|---|---|
| POST | `/api/interviews/{id}/phases/2/prompts` | `{batch_size}` | `PromptPart[]` of a new batch (units in scope, codes offered = phase 2 codes with `in_prompt`) |
| GET | `/api/interviews/{id}/phases/2/prompts` | | `PromptPart[]`, every batch, oldest first |
| GET | `/api/interviews/{id}/phases/2/runs` | | `AiRun[]` |
| POST | `/api/interviews/{id}/phases/2/runs` | `AiRunMeta` | `AiRun` (`batch_id` null = latest batch; `prompt_id_version` empty = current prompt version) |
| POST | `/api/interviews/{id}/phases/2/runs/{run_id}/outputs` | `{raw, part?, of?}` | `AiRun`, or 400 with `issues`. The answer is checked against the part of the run's batch (its units and its code snapshot). Blind guard (409): every unit of the part must be coded by the candidate first |

## Review

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/review` | | `{runs: string[], items: [ReviewItem & {unit: Unit, history: Arbitration[], allowed: Code[]}], n_disagreements, n_sampled, n_decided, seeds: [{run_label, seed}]}`; disagreements first, then sampled agreements, each in segmentation order; `allowed` lists the phase 2 codes for a custom decision. A unit coded only with an open label to clarify is not compared until the label is resolved |
| PUT | `/api/interviews/{id}/review/{unit_id}` | `{choice: ArbitrationChoice, final_codes: string[], rationale}` | `Arbitration`. Rationale required for a disagreement, optional for a sampled agreement |
| GET | `/api/interviews/{id}/final` | | `{finals: FinalCoding[], uncoded: string[], n_decided, n_open_disagreements}` over the phase 2 scope |
| GET | `/api/interviews/{id}/agreement` | | `{reports: AgreementReport[], markdown}` (candidate against each complete run, then runs pairwise) |

## Codebook and research questions

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/codebook` | | `Codebook` |
| POST | `/api/codebook/import` | `{kind: "csv"\|"json"\|"coding_db", path}`; legacy `{coding_db?}` still accepted | `Codebook` plus `report: {added, updated, absent}` (counts: codes new, rewritten from the file, and codebook codes the file lacks) |
| POST | `/api/codebook/codes` | `Code` (`origin` literature or field, default field; `definition` required; `toe` T, O, E or null; `research_questions` must exist) | `Codebook` |
| PUT | `/api/codebook/codes/{code}` | `{label?, definition?, toe?, research_questions?, example?, reason?, in_prompt?}` (omitted = unchanged; `toe: null` clears) | `Codebook` (version incremented) |
| PUT | `/api/codebook/codes/{code}/in_prompt` | `{in_prompt}` | `Codebook` |
| POST | `/api/codebook/codes/{code}/active` | `{active}` | `Codebook` |
| POST | `/api/codebook/codes/{code}/merge` | `{into, rationale}` | `{codebook, rewritten_codings, rewritten_decisions}` |
| GET | `/api/codebook/codes/{code}/units` | | `[{interview_id, unit: Unit, coding: HumanCoding}]`, every unit whose phase 2 coding carries the code |
| POST | `/api/codebook/export` | | `{files: [{path, sha256, size}]}` (CSV and SQL for `05_Analysis/coding/coding.db`) |
| GET | `/api/research-questions` | | `ResearchQuestion[]` |
| POST | `/api/research-questions` | `{id, text}` | `ResearchQuestion[]` |
| PUT | `/api/research-questions/{rq}` | `{text, ord?}` | `ResearchQuestion[]` |
| DELETE | `/api/research-questions/{rq}` | | `ResearchQuestion[]` (409 while a code maps to it) |

## Cases

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/cases` | | `Case[]` |
| GET | `/api/interviews/{id}/cases` | | `Case[]` |
| POST | `/api/interviews/{id}/cases` | `Case` (`case_id` empty = generated, e.g. ECO-05-C1) | `Case` |
| PUT | `/api/cases/{case_id}` | `Case` | `Case` |
| GET | `/api/cross-case` | | `{questions: ResearchQuestion[], cases: Case[], cells: [{rq, case_id, codes: [{code, units: string[]}]}]}` (codes mapped to the question found in the final phase 2 coding of the case's interview, or of its evidence units when the case lists any) |

## Audit and export

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/audit?interview_id=` | | `AuditEvent[]` |
| GET | `/api/audit/verify` | | `ChainCheck` |
| POST | `/api/interviews/{id}/export` | | `{dir, files: [{name, sha256, size}], manifest_sha256, status}` (`status`: unit, coding, run, review, label and case counts) |

The phase 1 export `phase1_contexts.csv` has the columns `unit_id,codes,derived_contexts`. `derived_contexts` lists the TOE contexts of the unit's current phase 2 codes, joined with `;` in the order TECH, ORG, ENV (`CONTEXT` when no code is attached to a context, empty for an interviewer turn or an NA unit). The interview export package holds `units.csv`, `phase1_contexts.csv`, `phase2_coding_human.csv` (with `revised_after_ai`), `phase2_human_full.csv`, `phase2_batch<b>_prompt_part<n>_of<m>_<hash>.txt`, per run `phase2_run<id>_<label>.json`, `phase2_coding_ai_run<id>_<label>.csv` and `phase2_ai_run<id>_<label>_part<n>_raw.txt`, `phase2_agreement.md` and `phase2_review.md` (once a run is complete), `phase2_coding_final.csv`, `to_clarify.csv`, `cases.json`, `cases.md`, `codebook.csv`, `audit_log.jsonl`, `audit_chain.json`, `manifest.json` and `README.md`. The codebook export writes `<db dir>/exports/codebook/<YYYY-MM-DD_HHMMSS>_codebook.csv` and `.sql`, never overwriting.

## v5 additions (simple view)

- Automatic phase 1: saving a segmentation codes every unit that has no phase 1 coding (interviewer turns NA, the others CONTEXT, or the first unattached category when CONTEXT is not configured), in one audit event `coding.auto_phase1` with detail `{n_filled, n_na, n_context, n_kept}`. Those codings carry `modifier = "auto"`; the config guard (phase 1 category names) and the re-segmentation lock ignore them. A candidate's write replaces the marker.
- Interviewer units are out of phase 2 scope: `phase_units(2)` excludes them, `POST /phases/2/prompts` refuses a batch containing one (400, the unit named), and they are never verification targets, review items or agreement samples.
- `GET /api/interviews/{id}/simple`: see the Coding table.

## v4 additions (12.10.2026, cases across interviews, frozen reference)

- `POST /api/interviews/{id}/phases/2/prompts` is refused (400, `validation`) while any unit of the batch lacks a phase 2 coding; the message lists the unit ids. In the same transaction the server writes the batch's `human_snapshots` row.
- `GET /api/interviews/{id}/snapshots` -> `HumanSnapshot[]` `{snapshot_id, interview_id, batch_id, phase, segmentation_hash, codebook_hash, codings: {unit_id: codes[]}, sha256, created_at}`.
- `GET /api/interviews/{id}/review`: items gain `human_current` and `revised_since_snapshot`; `human` is the frozen value; top-level `snapshots: [{run_label, snapshot_id, sha256}]`.
- `GET /api/interviews/{id}/agreement`: `Candidate vs AI <run>` computed on the frozen reference of the run's batch.
- Cases (`POST /api/interviews/{id}/cases`, `PUT /api/cases/{case_id}`, `GET /api/cases`, `GET /api/interviews/{id}/cases`): `Case` gains `units` (attributed unit ids, any interview), `interviews` (read-only), `decision`, `granularity`, `carrier_chosen`, `carriers_rejected`, `account_type`, `swiss_link`; `CaseField` gains `contradictions` and `missing`; evidence must be among `units` (400 otherwise). `GET /api/interviews/{id}/cases` also lists cases holding a unit of the interview.
- `GET /api/cross-case`: cells gain `documented` (false, with empty `codes`, when the case has no attributed unit).
- Export adds `cases.csv`, `case_units.csv`, `human_snapshot_batch<id>.csv` and `.json`, and two sections in `phase2_review.md` (revisions since the snapshot, snapshots).

## Lot 2 additions (12.10.2026 evening, decisions D1 to D8)

- Flags: `GET /api/interviews/{id}/flags`, `PUT /api/interviews/{id}/units/{unit_id}/flags` `{flags, co_constructed_kind?}` (closed list VERIFY, SECOND-HAND, UNFINISHED, CO-CONSTRUCTED; the kind, assent or content, is required with CO-CONSTRUCTED).
- Corrections: `GET`/`POST /api/interviews/{id}/units/{unit_id}/corrections` `{text, role, reason}`; append-only overlay, the segmentation is never rewritten; `unit.corrected` and the current text appear in the phase unit rows.
- Verification: `GET /api/interviews/{id}/verification` -> `{status: VerificationStatus[], targets: VerificationTarget[], coverage}`; `PUT /api/interviews/{id}/units/{unit_id}/verification` `{state: verified|unusable, note?}`; a correction makes the state unverified and stale.
- Claims: `GET /api/interviews/{id}/claims`, `GET /api/interviews/{id}/units/{unit_id}/claims`, `POST /api/interviews/{id}/claims` (`Claim`, excerpt exact substring, codes among the unit's codes, closed lists EV0..EV4, provenances, roles), `PUT`/`DELETE /api/claims/{claim_id}`, `GET /api/interviews/{id}/claims/review` -> `[{claim_id, reason}]`.
- Memos: `GET`/`PUT /api/research-questions/{id}/memo` (`RqMemo`, four parts, units for and against).
- Saturation: `GET /api/saturation` -> `SaturationRow[]` (provisional).
- Exports: per interview adds `units_verification.csv`, `unit_corrections.csv`, `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv`, `state.json` (digest of the whole state, generator version and git description) and `db_snapshot.sqlite`; `POST /api/export/cross-case` writes `answers.md`, `cross_case.csv`, `saturation.csv`, `saturation.md`, `cases.csv`, `state.json`, `manifest.json`, `README.md`.
- Prompt version `CS-P04-EN v3.1`: unit identifiers found in code texts are redacted as `[unit]`.

## Providers and run (v5)

Model providers are optional. The copy-and-paste workflow is always listed first and is the reference; a provider only automates the copy of the prompt and the paste of the answer. Settings page of the interface: "Study configuration", "Codebook import" and "Model providers" (the list is edited there; the "Run with a provider" control sits in the AI pass of an interview).

- `GET /api/providers` -> `[{id, kind, description, configured, config}]`. `kind`: `copy_paste`, `cli`, `open_ai_compatible` or `oauth_chat_gpt`; `description`: `{id, kind, sends: string[], execution}` with `execution` `{kind: "local"}` or `{kind: "hosted", vendor, endpoint}`; `configured`: the command is found, the API key variable is set, or a ChatGPT profile is connected (`copy_paste` is always configured); `config`: the stored entry, or null.
- `PUT /api/providers` `{providers: Entry[]}` -> the same list. Entries are those of `examples/providers.example.json`. The whole list is checked first: an unknown type or field, an empty id, the reserved id `copy_paste`, a duplicate id, or a `custom` preset without `base_url` gives 400 `validation` naming the entry, and nothing is stored or audited. Stored as JSON under the settings key `providers_config`, exactly as given; an API key is named by its environment variable and never stored. Audit `providers.update` with `{before, after}`.
- `POST /api/providers/{id}/probe` -> `{ok, cwd_empty, env_scrubbed, instruction_files_absent, probe_answer}`. The isolation probe of a `cli` provider: the model is asked what it can see from an empty working directory, with a clean environment. `ok` only when the answer is `NONE`. For any other kind: 400 "does not apply"; unknown id: 404.
- `POST /api/interviews/{id}/batches/{batch_id}/run` `{provider_id, run_label, model?, temperature?, seed?, max_tokens?}` -> `{run: AiRun, journal_line}`. Every part of the batch is sent with its prompt text byte for byte; every answer is then validated against the batch as a pasted answer is (`parse_ai_output`), and the run is created only when all of them are valid. Refusals:
  - 409 `guard` when the batch has no prompt, or a unit of it is not yet coded by the candidate (blind coding guard).
  - 400 `validation` for `copy_paste` (message: the manual workflow runs no model, paste the answer instead), for a blank or already used run label.
  - 404 for an unknown provider or interview.
  - 400 `validation` with `issues` and `rejected: [{part, raw}]` when any answer is refused; `raw` is the answer exactly as returned. Nothing is kept.
  - 500 `internal` when the command fails (its message says it exited).
  Success audits `ai_run.provider` with `{run_id, provider_id, journal_line, batch_id, parts}`; `journal_line` is `command local, model hosted` or `command local, model local` for a command, `hosted (<vendor>, <endpoint>)` or `local` otherwise.
- `AiRunMeta.provider` (absent for a pasted run): `{id, execution, parameters_exposed: string[], parameters_not_exposed: string[], parts: [{part, request_sha256, started_at, finished_at}]}`. `parameters_*` name `temperature`, `seed`, `max_tokens` and `model`; `meta.temperature` and `meta.seed` are set only when the provider exposed them. `request_sha256` is the SHA-256 of the prompt part as sent. The export keeps each raw answer as `phase2_ai_run<id>_<label>_part<n>_raw.txt`, byte for byte, and the run in `phase2_run<id>_<label>.json` carries the record.
- `POST /api/providers/{id}/oauth/begin` -> `{url, state}`. Only for `oauth_chat_gpt` with a `token_dir` (otherwise 400). A loopback listener on a free port receives the callback; the sign-in completes in the background (5 minutes at most).
- `GET /api/providers/{id}/oauth/status` -> `{connected, pending, profile, error}`; `profile` is the connected ChatGPT profile (`{id, email, subject, client_id, connected}`) or null, `error` the message of the last failed sign-in or null. Same refusals as `begin`.


## V1 lots 1 and 2

Server side of the V1 specification (`docs/SPEC.md`, sections 3 and 4): passages, a stable code tree, the project configuration and the project template. Everything is additive: the earlier routes (`/api/codebook`, `/api/phases/...`) answer as before and keep using the upper-case token of a code. Every write below is one transaction with its audit event. Errors follow the usual mapping: 400 `validation`, 404 not found, 409 `guard`.

### Passages

A passage is a selected part of a unit (a turn). It stores the interview, the unit, `start` and `end`, the exact `text`, the segmentation hash, `anchor_digest` (SHA-256 of the unit text the positions refer to), `created_at` and `created_by`. Positions count Unicode characters from 0, end excluded (the JSON also gives `start_utf16` and `end_utf16`, in the units of a browser selection). A client that counts in UTF-16 sends `"offset_unit": "utf16"`; a position inside a surrogate pair is refused with 400. A passage is `stale` (with `stale_reason`) when the current text of the unit no longer has the digest, or when the unit left the segmentation: it is flagged, never moved. A passage with no active code is a highlight (`kind: "highlight"`, otherwise `"coded"`). Interviewer units can carry no passage: 400, the message says the unit is the interviewer's.

| Method | Path | Body | Answer |
|---|---|---|---|
| POST | `/api/interviews/{id}/passages` | `{unit_id, start?, end?, offset_unit?, text?, code?, note?}` | `{passage, created}`. Without `start` and `end` the whole unit is selected. `text`, when given, must be what the positions select. Without `code` the passage is a highlight. `code` is an id (`C-0001`), a name, an alias or a token, case-insensitive; `"Plus tard"` or `LATER` parks the passage (with `note`). The same span is the same passage: a second code goes to it (`created: false`) |
| GET | `/api/interviews/{id}/passages?unit_id=&code=&kind=` | | `Passage[]`; `kind`: `highlight`, `coded` or `later` |
| GET | `/api/passages/{pid}` | | `Passage` with `codings: [{coding_id, code_id, code_name, system, note, created_at, created_by}]`, `before` and `after` (up to 60 characters of context) |
| DELETE | `/api/passages/{pid}?reason=` | | `{deleted}`. Logical, audited |
| POST | `/api/passages/{pid}/codings` | `{code, note?}` | `Passage`. Several codes per passage; a code already on it is a no-op; an inactive or merged-away code is refused |
| DELETE | `/api/passages/{pid}/codings/{code}?reason=` | | `Passage`. Removes the coding logically; a passage left without code becomes a highlight |
| GET | `/api/interviews/{id}/highlights` | | `Passage[]` without active code, to be converted by coding them |
| GET | `/api/interviews/{id}/later` | | The passages parked with "Plus tard" in the interview, with their note |
| GET | `/api/later` | | `{summary: [{interview_id, n}], items}` for the whole corpus |

Audit actions: `passage.create`, `passage.code`, `passage.code_note`, `passage.uncode`, `passage.delete`. Coding a whole turn through this route does not write the unit-level `human_codings` of phases 1 and 2 (see `docs/DEVGRAPH_v1_lots1-2.md`).

### Codes, names, aliases, tree

Each code has a stable `code_id` (`C-0001`, never reused), an editable `name`, a list of `aliases`, an optional `parent_id`, `includes` and `excludes` texts, and its legacy `token`. Existing codes get an id in codebook order, their token as name, and the token as alias (`reason: legacy_token`). The reserved system code "Plus tard" (`code_id` and `token` `LATER`) is always listed last, is not stored in the codebook, is never counted and is never sent to the AI.

`CodeNode`: `{code_id, token, name, label, definition, includes, excludes, parent_id, origin, toe, research_questions, active, system, in_prompt, merged_into (a code_id), version, example, reason, origin_passage_id, example_passage_id, created_at, aliases: [{alias, reason, created_at}], counts: {passages, interviews, passages_rollup, interviews_rollup}}`. `counts` never mixes the two: passages and distinct interviews, own and rolled up with all descendants (a passage counted once). The codebook version is the number of revisions, and goes up by one at each change of the codebook.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/codes?attribute=&value=` | | `{codebook_version, filter, codes: CodeNode[]}`. With `attribute` and `value` together the counts cover only the interviews whose attribute has that value ("codes by attribute"); one without the other is 400; an unknown attribute is 400 |
| GET | `/api/codes/{id}` | | `CodeNode` (`id` is an id, name, alias or token) |
| PUT | `/api/codes/{id}` | `{name?, label?, definition?, includes?, excludes?, ...}` | `CodeNode`. A new name is validated (not empty, one line, 200 characters at most, not `NA` nor "Plus tard", unique among names, aliases and tokens) and the former name becomes an alias. The id and the codings do not change. A merged code cannot be edited |
| POST | `/api/codes/{id}/move` | `{parent_id: id \| null, rationale}` | `CodeNode`. Refused (400) when it makes a cycle, when the parent is unknown, merged or the reserved code, or without rationale. The passages do not change |
| POST | `/api/codes/{id}/aliases` | `{alias}` | `CodeNode`. A synonym |
| DELETE | `/api/codes/{id}/aliases/{alias}` | | `CodeNode`. The alias that equals the token or the name cannot be removed |
| POST | `/api/codes/{id}/merge` | `{into, rationale}` | `{outcome: {moved_passage_codings, deduplicated_passage_codings, ...}, merged, code}`. Passages coded with both codes keep one coding; the merged name stays an alias of the target; children are re-attached to the target |
| POST | `/api/codes/{id}/active` | `{active}` | `CodeNode`. An inactive code cannot be applied |
| POST | `/api/codes/from-passage` | `{name, definition, includes?, excludes?, parent_id?, passage_id \| passage: {interview_id, unit_id, start?, end?, offset_unit?}}` | `{code, passage, codebook_version}`. Creates the code (origin `field`) with its origin passage, codes the passage with it, increments the codebook version, all in one transaction. A highlight given by `passage_id` becomes coded. The definition is required |
| GET | `/api/codes/{id}/passages?descendants=true` | | `PassageContext[]`: `{passage, speaker, previous, next, other_codes}` |
| GET | `/api/codebook/revisions` | | `{codebook_version, revisions: [{rev, ts, actor, reason, target}]}`; `reason`: `import`, `create`, `edit`, `active`, `merge`, `rename`, `move`, `alias`, `create_from_passage` |

Audit actions: `codebook.code.rename`, `.edit`, `.move`, `.alias_add`, `.alias_remove`, `.create_from_passage`, and the earlier `codebook.merge`, `codebook.import` (the CSV and JSON imports now also assign ids and add a revision).

### Project configuration

Stored under the settings key `project_config`, next to `study_config`; validated as a whole (400 names the element), audited as `project_config.update` with `{before, after}`; no event when nothing changes.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/project-config` | | `ProjectConfig` |
| PUT | `/api/project-config` | `ProjectConfig` | `ProjectConfig` as stored (memo template versions assigned). 409 when it would orphan data (see below) |
| GET | `/api/project-config/presets` | | `{frameworks: {toe, tam_utaut}, defaults}` to fill the Settings form |

`ProjectConfig`:

- `attributes: [{id, label, kind: text | choice | number | date, choices, shared_with_model}]`. A choice needs values; only a choice has values. Dates are `YYYY-MM-DD`, numbers accept a comma and are stored with a point.
- `memo_templates: [{id, name, default, version, questions: [{id, text}]}]`. Exactly one default. `version` is set by the server: it goes up by one each time the name or the questions of a template change, and the earlier version is kept (`memo_history`) so that an interview keeps the questions it was answered with. Default: `default` ("Par défaut"), four questions: what the respondent tells, what surprised me, which contradictions, what to ask the next interviews.
- `checklist`: four items by default (`turns_seen`, `later_handled`, `code_memos`, `next_questions`).
- `theme_checks`: seven by default. `evidence_natures`: five (first-hand account, reported statement, figure estimate, quoted document, opinion). `link_types`: six (soutient, nuance, conditionne, mécanisme, exception, contredit; at least one). `matrix_dimensions`: none by default (for example "qui impose", "qui paie"). Items are `{id, label}`; ids match `^[a-z0-9][a-z0-9_-]*$`, are unique in their list, labels are unique in their list.
- `interpretive_framework`: `null` by default, or `{preset: toe | tam_utaut | custom, dimensions: [{id, label, definition}]}`. Used only in Analyse and Write: it is never in the codes, the codebook, the coding screen or the AI prompt (the prompt is built from `study_config` and the codebook only).

Data protection (409 `guard`): an attribute that has values cannot be removed or change type, a choice in use cannot be dropped, a memo template an interview uses cannot be removed.

### Per-interview data

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/attributes` | | `{interview_id, definitions, values: {attribute_id: value}}` |
| PUT | `/api/interviews/{id}/attributes` | `{values: {attribute_id: value \| null}}` | same. `null` or `""` clears. Validated by type; one wrong value refuses the whole call. Audit `interview.attributes` |
| GET | `/api/interviews/{id}/memo` | | `MemoView`: `{template_id, template_name, template_version, current_version, outdated, questions: [{id, text, source: template \| added, reworded, answer: {text, at} \| null}], removed, orphan_answers, overrides, saved}`. The default template until another is chosen |
| PUT | `/api/interviews/{id}/memo/template` | `{template_id}` | `MemoView`. Chooses the template and moves to its current version; the interview's own edits are reset, answers are kept by question id. Audit `memo.template` |
| PUT | `/api/interviews/{id}/memo/overrides` | `{added: [{id?, text}], removed: [question_id], reworded: {question_id: text}}` | `MemoView`. Edits for this interview only; an added question without id gets `x1`, `x2`... Audit `memo.overrides` |
| PUT | `/api/interviews/{id}/memo/answers` | `{answers: {question_id: text}}` | `MemoView`. Each answer carries the time of its last change; `""` clears; only questions shown for the interview can be answered. Audit `memo.answer` (the question ids, not the text) |
| GET | `/api/interviews/{id}/checklist` | | `{interview_id, items: [{id, label, checked, checked_at}], note, complete}`. The items are those of the configuration; `complete` informs and never locks |
| PUT | `/api/interviews/{id}/checklist` | `{checked: {item_id: bool}, note?}` | same. Audit `interview.checklist` |

### Project template

One JSON file: `{format: "coding-studio-project-template", version: 1, study_config, research_questions: [{id, text}], project_config}`. It holds no interview text, no coding, no attribute value, no memo answer, no provider and no secret. The export is deterministic: the same configuration gives the same bytes.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/project/template` | | the template |
| POST | `/api/project/template/import` | `{template, dry_run?}` | `{dry_run, applied, changes: [{part, action, detail}], problems: string[]}`. 200 whether or not it could be applied; 400 only when the body is not a template at all |

Import is all or nothing and reports every problem at once; with a problem, or with `dry_run`, nothing is written. It works on a new project and on one with data. The lists, memo templates, framework, study settings and research questions are replaced or added; an attribute with values, or a memo template an interview uses, that the template does not list is kept (`action: kept`); an attribute that has values and would change type, or a phase 1 category change after phase 1 coding, is a problem. Research questions of the project that the template does not list are kept. `action` is `added`, `updated`, `removed` or `kept`. Audit `project.template.import` (the changes) and `project_config.update`, `config.update`. `examples/project_template.example.json` is a fictional library study.

### Exports (additions)

The interview package and the cross-case package gain `passages.csv`, `highlights.csv`, `attributes.csv`, `memos.json` and `study_config.json`; the cross-case package also gains `codebook.csv`. `codebook.csv` gains six trailing columns `code_id,name,parent_id,aliases,includes,excludes` (aliases joined with `;`, the name excluded); the first thirteen columns and the codebook import are unchanged. `state.json` parts gain `passages`, `attribute_values`, `memos_and_checklists`, `project_config` and `code_tree`, so the state digest moves with them. All new files are in `manifest.json` with their SHA-256. Formats are in `docs/CONTRACTS.md`, section 10.
