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


## V1 lots 3 and 4

Server side of the lexical workbench (lot 3) and of the review (lot 4). Everything is additive. Errors follow the usual mapping: 400 `validation`, 404 not found, 409 `guard`. The lexical routes are read-only apart from the language setting; none calls a model.

### Lexical workbench (lot 3)

Deterministic and local: French, English and German tokenisation, stop words, Snowball stems, YAKE keyphrases, TF-IDF over the respondent units only (interviewer turns are never indexed for similarity), cosine similarity. The index is built from the current text of the units (corrections applied), kept in memory, and rebuilt when a segmentation, a correction or the language setting changes (a fingerprint is compared before each query). A corpus of 30 interviews of 15,000 words indexes in about 3 s in a debug build; each query answers in well under 300 ms.

Language: the project language (default `fr`) and an optional language per interview.

| Method | Path | Body or query | Answer |
|---|---|---|---|
| GET | `/api/lexicon/config` | | `{language, interview_languages: {interview_id: lang}}` |
| PUT | `/api/lexicon/config` | `{language: fr\|en\|de, interview_languages?}` | the stored configuration. 400 for an unknown language or interview. An interview already in the project language is not kept. Audit `lexicon.config.update` |
| GET | `/api/lexicon/status` | | `{config, languages, index: {n_interviews, n_units, n_respondent_units, n_words, n_stems}, fingerprint, method, note}` |
| GET | `/api/passages/{pid}/lexical` | | The lexical panel of a passage: `{language, keyphrases: [{phrase, score, count}], stems: [{stem, forms, count}], label, provisional: true, note}`. YAKE scores: lower is better. `label` is a provisional name the researcher may use or ignore |
| GET | `/api/passages/{pid}/similar` | `k` (10, max 100), `min_score`, `interview_id`, `attribute`+`value`, `include_own_unit`, `without_code` | `{passage_id, items, note}`. Each item: `{interview_id, unit_id, seq, score, shared_terms: [{stem, form, weight}], text, coded_with: [{code_id, name}]}`, ranked by score then by position. `coded_with` lists the codes of the passages of that unit. `without_code` (id, name, alias or token) leaves out the units that already carry the code: what is left is what coding may have missed. The note says that the method sees neither synonyms nor other languages |
| GET | `/api/interviews/{id}/units/{unit_id}/similar` | same | the same for a unit (an unknown unit gives 404; the results are always respondent units) |
| GET | `/api/lexicon/keywords` | `scope` (`highlights` default, or `passages`), `min_passages` (1), `limit` (100), `interview_id`, `attribute`+`value` | `{scope, n_passages, n_terms_in_passages, n_rows, rows, note}`. A row: `{stem, forms, n_passages, n_interviews, interviews, passage_ids, n_occurrences, corpus_occurrences, keyness}`. `keyness` is the log-likelihood (G2) of the stem in these passages against the rest of the respondent text, negative when rarer. Rows are sorted by keyness, then passages, then stem. A table with the passages behind each count, never a word cloud. The highlights are the passages with no active code |
| GET | `/api/lexicon/kwic` | `q` (word or phrase, required), `stem` (false), `width` (60 characters each side), `interview_id`, `attribute`+`value`, `speakers` (`respondent` default, `interviewer`, `all`), `limit` (200, max 1000), `offset` | `{query, stem, query_tokens, total, n_interviews, by_interview: [{interview_id, n}], hits, offset}`. A hit: `{interview_id, unit_id, seq, role, start, end, left, match, right}` with `start` and `end` in Unicode scalar positions of the unit text, end excluded. Case and accents are ignored; a phrase must be adjacent words; with `stem` every word of the query is compared by its stem. `total` counts every occurrence, `limit` and `offset` page the hits |
| GET | `/api/lexicon/term-frequency` | `attribute` (required), `terms` (comma-separated), `stem`, `speakers`, `top` (10) | With `terms`: `{attribute, mode: "terms", stem, terms: [{term, by_value: [{value, n_interviews, n_interviews_with_term, n_units_with_term, n_occurrences, n_words, per_1000_words}]}], note}`; `value` is null for the interviews where the attribute is empty. Without `terms`: `{attribute, mode: "top_terms", values: [{value, terms: [{stem, form, n_occurrences, n_interviews}]}]}` |

The attribute and value filters are the ones of `/api/codes`: both or neither (400 otherwise), an unknown attribute is 400; with `interview_id` too, the two filters intersect.
### Review (lot 4)

The AI challenge keeps its routes (`/phases/2/prompts`, `/runs`, `/runs/{id}/outputs`, `/review`, `/snapshots`). What lot 4 changes and adds:

- **Configured prompt.** `POST .../phases/2/prompts` builds the prompt from the configuration: study description, coding rules, research questions, the code tree (definitions, includes, excludes, only the codes offered to the model), the interview attributes flagged `shared_with_model`, and the interviewer turn that precedes each respondent unit, in a separate block `## Interviewer questions` (lines `{"before_unit", "speaker": "INT", "question"}`). Interviewer units are never in the `## Units` block and never sent for coding. A flat codebook (no code has a parent) keeps the Technology, Organisation, Environment grouping. The text is deterministic; the meta lines `configuration_digest:` and `pseudonymisation_digest:` and the prompt SHA-256 are recorded in the audit event `prompt.generate`, next to `human_snapshot_sha256`. The prompt version string stays `CS-P04-EN v3.1`.
- **Human reference frozen first.** The snapshot row and its event `human_snapshot.take` are written before the prompts, in the same transaction. No human code, memo or highlight reaches the prompt: changing the human coding gives a byte-identical prompt, and the snapshot taken earlier is not touched.
- **Pseudonymisation.** One project list applied to everything sent to a model (prompt, provider send) and reversed for display only. Matching ignores case, respects word boundaries and takes the longest name first. A provider send that still contains a listed name is refused (400). The run record (`ai_runs.pseudonymisation`) holds the list digest, the categories and the counts, never the names. Free text is not covered; the answer carries `note` to say so.
- **Candidate concepts.** The answer schema has an optional `candidate_concepts: [{label, description?, unit_ids, evidence: [{unit_id, quote}]}]`. Each quote must be word for word in its unit (whitespace squashed, on the pseudonymised text), otherwise the answer is refused with 400 and nothing is stored. Candidates are stored append-only and never change a coding.
- **Passage to unit sync.** A unit is coded with X when any passage in it has X; a passage that covers the whole unit writes the unit coding. Coding rows written by the sync have `source = passages` and are removed when no passage carries the code any more. A hand coding of the unit is overwritten when passages exist; a hand edit sets `source` to NULL.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/concept-candidates` | | `[{candidate_id, label, description?, run_label?, unit_ids, evidence: [{unit_id, quote}], decision, history}]`. `decision` is null until decided, then `{action, code_id, rationale, decided_at, decided_by}`. Labels, descriptions and quotes are shown with the original names |
| PUT | `/api/interviews/{id}/concept-candidates/{cid}` | `{action: create\|map\|reject, code_id?, new_code_name?, rationale}` | the candidate with its decision. `rationale` is required (400). `map` needs `code_id` (400), `create` needs `new_code_name`; the new code takes the description as definition and is created in the same transaction. A decision never changes a coding. Deciding again is allowed, the history stays; a second `create` is a 409 guard error. Audit `concept.candidate.decide` |
| GET | `/api/interviews/{id}/pseudonymisation` | | `{enabled, digest, categories, n_list_entries, entries: [{kind, original, pseudonym, count}], free_text_covered: false, note}`. `entries` are the names of the list found in this interview, with the number of replacements |
| GET | `/api/pseudonymisation` | | the same for the project, `entries` is the whole list (without `count`) |
| PUT | `/api/pseudonymisation` and `/api/interviews/{id}/pseudonymisation` | `{enabled?, entries?: [{kind: person\|organisation, original, pseudonym}]}` | the stored view. `entries` replaces the whole list. 400 for a duplicate name (case ignored), a pseudonym that is itself a name, or an unknown kind. Audit `pseudonymisation.update` keeps the digest and the counts, never the names |
| GET | `/api/interviews/{id}/harmonisation` and `/api/harmonisation` | | `{n_open, n_done, items: [{item_id, interview_id, unit_id, text, codes, code_id, code_name, cause: merge\|redefine\|new_subcode, detail, status: open\|done, resolution?, note?, ...}]}` |
| PUT | `/api/interviews/{id}/harmonisation/{item}` | `{resolution: confirmed\|corrected, note?}` | the item. 400 for another resolution |
| GET | `/api/interviews/{id}/review-state` and `/api/review-state` | | `{interview_id, state: not_done\|in_progress\|complete, n_relevant, n_covered, coverage, runs, n_batches, n_disagreements, n_decided, n_sample, n_sample_reviewed, n_candidates, n_candidates_decided, harmonisation_open, harmonisation_done, declaration}`; the corpus route gives `{interviews, n_interviews, n_complete, n_in_progress, n_not_done, declaration}` |

The worklist is filled by a merge (units of the passages that carried the merged code, for the target), by a change of definition, includes or excludes of a code (a rename does not count), and by the creation of a sub-code (for the parent). One open item exists per interview, unit and code.

Exports: `review.json` in the interview and cross-case packages, and a `review` part in `state.json`: candidates with decisions, the pseudonymisation digest, categories and number of entries (never the names), the worklist and the review state. Candidate texts are exported as the model saw them.

## REFI-QDA exchange (F-103)

Files for NVivo, MAXQDA and ATLAS.ti. Rules in `docs/CONTRACTS.md`, section 11. Implemented in `crates/cs-qdpx`, routes in `crates/cs-app/src/qdpx.rs`.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/export/qdc` | | `application/xml`, attachment `codebook.qdc`. Headers `X-Export-Sha256` and `X-Export-Summary` (JSON: `format, codes, sources, selections, codings, skipped_stale, skipped_highlights, skipped_without_exported_code, bytes, sha256`) |
| GET | `/api/export/qdpx?highlights=&name=` | | `application/zip`, attachment `project.qdpx`, same headers. `highlights=true` exports highlights under the code "Surlignages"; `name` is the REFI project name (default "Coding Studio") |
| POST | `/api/import/qdc?apply=` | the `.qdc` file as the raw body | `ImportReport` |
| POST | `/api/import/qdpx?apply=` | the `.qdpx` file as the raw body | `ImportReport` |

`apply` is false by default: the call is a dry run that reads the file, compares it with the project and returns the plan, writing nothing and logging nothing. `apply=true` does the same and writes. Bodies up to 256 MB.

`ImportReport`: `{kind: "qdc"|"qdpx", applied, codes_created, codes_matched, codes_skipped, sources_imported, sources_skipped, passages_created, codings_added, codings_existing, selections_skipped, codes: [{path, name, outcome: "create"|"match"|"skip", code_id, matched_by: "guid"|"name"|"file"|null, parent, reason}], sources: [{name, interview_id, matched_by: "guid"|"name"|"digest"|null, imported, reason, selections, selections_skipped, passages_new, codings_new, codings_existing}], other_sources_ignored: string[], issues: [{severity: "warning", location, message}]}`. `code_id` is filled for matched codes, and for created codes once applied. For `.qdc` the source fields are zero or empty. `issues` lists everything skipped, clipped or left where it was, at most 500 entries.

A file that cannot be used answers 400 `{error: "file rejected: n problem(s) found", kind: "validation", issues: [{location, message}]}` with every problem found (malformed XML with its position, wrong root element, not a ZIP, no `project.qde`, duplicated GUIDs, a code without name). Nothing is written.

## V1 lot 5 and gaps

Server side of Analyse and Write (themes, matrix, typed links, write-up, final audit, documented decisions) and nine gaps reported by the front end. Routes and JSON shapes follow the client `web/src/lib/analysis-api.ts`; deviations are listed at the end of this section. Everything lives in `crates/cs-app/src/v1_analysis.rs`, merged by one line in `lib.rs`. Every write is one transaction with its audit event. Errors: 400 `validation`, 404 not found, 409 `guard`. Schema version 6 (separate idempotent migration `analysis.sql`).

### Gaps

| Gap | Route | Notes |
|---|---|---|
| 1 | (passages) | `whole_unit` is true only when `start` is 0 and `end` is the unit length (both offsets compared to the length). |
| 2 | `POST /api/codes` `{name, definition, includes?, excludes?, parent_id?}` | Creates a code with no passage. `name` is the given name, never the token (the token is generated). Same validation as a rename; definition required. Answer: `CodeNode`. |
| 3 | `GET/PUT /api/config` | `StudyConfig` gains `description` and `coding_rules` as explicit fields. A legacy config that had them in the header or footer of `prompt_template` is migrated once: only customised text is copied, only into empty fields; `prompt_template` is kept. |
| 4 | `PUT /api/interviews/{id}/units/{unit}/mark` `{note?}`, `DELETE` same path, `GET /api/interviews/{id}/marks` | "To review" mark on a respondent unit (400 on an interviewer unit), settable and clearable, audited. `SimpleProgress.n_to_review` counts them. |
| 5 | `GET /api/library/progress` (`coded` counts the phase 2 grid; `units_coded` and `units_later` count the units the Coder shows as coded and parked) | `{interviews: [{interview_id, units, respondent_units, relevant, not_relevant, coded, units_coded, units_later, to_review, memo_answered, memo_total, review: {state,...}, arbitrated}], totals}` in one call. |
| 6 | `GET/PUT /api/codes/{id}/memo` `{text}` | Own field `CodeNode.memo` with an append-only history (`{memo, history}`). `reason` is no longer overloaded. |
| 7 | `POST /api/codes/{id}/undo-create` | Undoes `create-from-passage`: removes the code, and the passage when the same call created it (`{code_deleted, passage_deleted}`). 400 for a code not created that way. Audit `codebook.code.undo_create_from_passage`. Code ids are never reused. |
| 8 | `PUT /api/passages/{pid}/qualification` `{origin?, role?}` | `Passage.origin`: `lived_experience`, `project`, `opinion`, `document`. `Passage.role`: fixed list in `cs_model::analysis`. `null` clears; any other value is 400. Claims take `nature` from the project list `evidence_natures` (a stored value stays readable). |
| 9 | none | The phase 1 category set is deprecated, not retired: the simple view (automatic phase 1), the config guard and `/api/codebook/phase1` still depend on it. New work uses the code tree (`/api/codes`). It will be removed when those three are migrated. |

### Themes

`GET /api/themes` (summaries with `codes_count`, `interviews_count`), `POST /api/themes`, `GET/PUT/DELETE /api/themes/{id}` (logical delete), `PUT /api/themes/order`, `POST /api/themes/{id}/status`, `PUT /api/themes/{id}/checks/{check}`, `POST /api/themes/{id}/codes`, `DELETE /api/themes/{id}/codes/{code}`, `GET /api/themes/{id}/passages`, `GET /api/themes/{id}/history`.

A title is a sentence. Status: `candidate`, `revised`, `retained`. Retained needs every project check done (`theme_checks`) or an override with a reason; otherwise 409. Each change appends to `theme_history` (append-only) and to the audit log.

### Matrix

`POST /api/matrix {rows, cols, filter}` (client shape), `POST /api/matrix/compute` (native `MatrixConfig`), `GET /api/matrix/options`, `GET/POST /api/matrix/views`, `DELETE /api/matrix/views/{id}`, `GET/PUT /api/matrix/cell`, `POST /api/matrix/cell/passages`, `DELETE /api/matrix/cell/passages/{pid}`.

Axis kinds: `interviews`, `cases`, `attribute`, `themes`, `codes`, `focused_codes`, `dimensions`. Entry keys are stable ids: `interview:INT-01`, `attr:sector=Public`, `theme:T-0001`, `code:C-0003`, `dim:who_pays`, `case:ID`. Closed cell values: `present`, `not_mentioned`, `contradicted`, `uncertain`. Counts are computed at read time; only the value, the summary and hand-attached passages are stored. Filters restrict interviews by attribute value.

### Links

`GET/POST /api/links`, `PUT/DELETE /api/links/{id}` (logical delete). A link joins a passage to a part of the write-up with a type from the project list `link_types`. The view carries the exact quotation and the verification state.

### Write-up

`GET /api/writing/themes/{id}`, `PUT /api/writing/themes/{id}/parts/{part}` (five parts per theme), `GET/PUT /api/writing/questions/{rq}`, `GET /api/writing/framework` and `GET/PUT /api/themes/{id}/framework` (only when the project has an interpretive framework), `POST /api/writing/export` answering `{dir, files, markdown}`. Each save appends a row to `writing_versions`.

### Final audit

`POST /api/final-audit` draws 3 claims, seeded and recorded (replayable). `GET /api/final-audit`, `GET /api/final-audit/latest`, `GET /api/final-audit/{id}`, `POST /api/final-audit/{id}/verdicts`; verdicts `supported`, `to_nuance`, `to_withdraw`. Each claim shows the full chain (part, theme, codes, passages, quotation, interview, verification) and `gaps[]` (`no_text`, `no_link`, `unverified_passage`, `stale_passage`). Linked parts are preferred; written unlinked parts fill the draw.

### Decisions

`GET /api/decisions`, `GET/PUT /api/decisions/{id}`, `GET/POST /api/interviews/{id}/decisions`. Fields in the words of the guide: `name` (alias of `label`), `outcome` (`adopt`, `defer`, `reject`; derived from the old free-text `decision`, which is kept), characteristics, the five synthesis fields, units and passages.

### Exports

The cross-case package and the write-up package (`POST /api/writing/export`, directory `<date>_writeup`) gain `themes.csv`, `theme_sheets.json`, `matrix.csv`, `links.csv`, `final_audit.json`, `decisions.csv`, `decisions.json`, and `writeup.md` (write-up package). All are in `manifest.json` with their SHA-256; `state.json` parts gain `themes`, `writing_versions`, `links`, `matrix`, `final_audit`, `unit_marks`. Formats: `docs/CONTRACTS.md`, section 11.

### Deviations from the client

- Native matrix configuration at `/api/matrix/compute`; saved views are stored in native form and shown to the client in single-axis form.
- `focused_codes` means codes with sub-codes, or all codes when none has any.
- Framework cells use the client shape `{explains, note}`.
- The passage origin list is this server's reading of the guide.
- Decisions still need an interview.

## V2 transcripts and audio

Server side of F-24 (versioned transcripts, projection of passages) and F-62 (audio linked to the transcript). Rules in `docs/CONTRACTS.md`, section 15. Routes in `crates/cs-app/src/v2_transcripts.rs` and `v2_audio.rs`, logic in `crates/cs-store/src/transcripts.rs`, `projection.rs` and `audio.rs`. Errors: 400 `validation`, 404 not found, 409 `guard`. Every write is one transaction with its audit event.

### Transcript versions

A version is one state of the units of an interview. The first segmentation is version 1. A new segmentation (a rescan with a modified file, then `POST /api/interviews/{id}/segment`) and each unit correction (`POST .../units/{unit}/corrections`) append a version, and project the passages from the previous version onto it in the same transaction. An identical segmentation adds nothing. A database made before versioning gets a `baseline` version when its first change arrives.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/transcript/versions` | | `{interview_id, current, versions: [{version, cause: baseline\|segmentation\|correction, text_sha256, n_units, target, reason, created_at, created_by, reanchored, flagged}]}`. `target` is the unit of a correction or the source file of a segmentation; `reanchored` and `flagged` count what the projection onto this version did |
| GET | `/api/interviews/{id}/transcript/versions/{n}` | | `{interview_id, version, units: [Unit]}`, the units as they read in that version (404 for an unknown version) |

### Review of passages

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/transcript/review?interview_id=&status=` | | `{interview_id, status, open, items: [ProjectionItem]}`. `status` is `needs_review` (default), `reanchored`, `accepted`, `adjusted`, `detached`, `superseded` or `all` (400 otherwise). `open` counts the items still to review in the answer |
| GET | `/api/interviews/{id}/transcript/review?status=` | | same, for one interview |
| GET | `/api/transcript/review/{pid}` | | one `ProjectionItem` (`pid` is the `projection_id`) |
| POST | `/api/transcript/review/{pid}/accept` | `{note?}` | the item, settled. 400 when there is no proposed span |
| POST | `/api/transcript/review/{pid}/adjust` | `{unit_id?, start, end, offset_unit?: scalar\|utf16, note?}` | the item, settled. The span is on the current text of `unit_id` (default: the passage's unit). 400 for an empty, reversed or outside span, an interviewer unit, or a span another passage already covers |
| POST | `/api/transcript/review/{pid}/detach` | `{note?}` | the item, settled. The passage leaves every list and count (deleted logically); its text, codes and old reading stay in the item and the audit |

An item already settled answers 409 to a new decision.

`ProjectionItem`: `{projection_id, interview_id, passage_id, status, method: diff\|moved_unit\|search, reason, from_version, to_version, old, proposed, final, current_unit_text, codes: [{code_id, code_name}], created_at, resolved_at, resolved_by, resolution_note}`. `old`, `proposed` and `final` are `{unit_id, start, end, start_utf16, end_utf16, text, unit_text}`: the span as it read in `from_version`, the span the diff proposes in `to_version` (null when nothing of the passage is left), and the span finally taken. `unit_text` is the whole text of that unit in that version, to show the two readings side by side. `reason` is `text_changed`, `unit_moved` (the exact words occur once in another unit), `unit_removed` (no unit holds them) or `unit_became_interviewer`; it is empty for a passage re-anchored on its own.

A passage that needs review stays flagged `stale` with its old offsets until the decision. After `accept` or `adjust` it is re-anchored (`stale` false, new unit, offsets, text, `anchor_digest`) and the unit-level coding of the units involved is synchronised from the passages as usual.

Audit events: `transcript.version`, `passage.reanchor` (automatic, accepted or adjusted, with before and after), `passage.review_flag`, `passage.detach`, `transcript.projection.resolve`.

### Audio

Recordings are the `m4a`, `mp3` and `wav` files of the interview folder, sub-folders included. They are read from the folder at each call (the scan inventory does not list them). No transcription.

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/audio` | | the player's one call: `{interview_id, has_audio, files: [{rel_path, size, mime, url}], offset: {offset_seconds, note, set, updated_at, updated_by}, timed_units, units: [{unit_id, seq, role, timestamp, start_seconds, audio_seconds, end_seconds}], warnings}` |
| GET | `/api/interviews/{id}/audio/file?path=<rel_path>` | | the recording, with `Range` support. Without `path` the first recording. `url` in the map is this route with the path encoded |
| PUT | `/api/interviews/{id}/audio/offset` | `{offset_seconds, note?}` | the stored offset. 400 for a value that is not a finite number within one day (86 400 s); milliseconds are the finest grain. A repeated value writes nothing. Audit `audio.offset` with before and after |

Units: `start_seconds` is the transcript timestamp read as seconds (`h:mm:ss`, `mm:ss`, a decimal part after `.` or `,`), `null` when the stamp is not a time (`--:--`, the `P01` stamps of written responses). `audio_seconds` is `start_seconds + offset_seconds`, never below 0, and is where the player seeks. `end_seconds` is the `audio_seconds` of the next timed unit (null for the last), to highlight the current unit while playing. Interviewer units are listed too. The offset is one value per interview. `warnings` says when there is no audio file, several files (the offset applies to the interview, not to a file), units without a time, or timestamps that go backwards.

`GET .../audio/file`:

- 200 with `Content-Type` (`audio/mp4`, `audio/mpeg`, `audio/wav`), `Content-Length` and `Accept-Ranges: bytes`.
- A `Range: bytes=a-b`, `a-` or `-n` gives 206 with `Content-Range: bytes a-b/total` and only those bytes. Only the first range of a list is served; a malformed range is ignored (200, whole file). A range past the end gives 416 with `Content-Range: bytes */total`. HEAD answers the headers without a body.
- Only a file that appears in the audio map is served. A name with `..`, a backslash, a leading `/` or an empty name is 400; a name that is not an audio file of this interview (a transcript, another interview's file, a missing file) is 404; a link is never followed, so a link leading out of the folder is neither listed nor served; an interview folder outside the library folder is 400.

For the player: `GET /api/interviews/{id}/audio` once, set `<audio src={files[0].url}>`, seek to `units[i].audio_seconds` on click, find the current unit by comparing `currentTime` with `audio_seconds` and `end_seconds`, and send `PUT .../audio/offset` when the researcher corrects the shift (then add the new offset to every `start_seconds`, or call the map again).

### Exports

`transcript_versions.json` in the interview and cross-case packages (versions, and every projection with its old, proposed and final span and how it was settled), a `transcript_versions` part in `state.json`, and a last column `transcript_version` in `passages.csv`.

## V2 codes and search

Server side of F-09 (configurable passage origins and roles), F-39 (split a code, undo), F-41 (code search), F-45 (counts per code), F-55 (full-text search) and F-56 (co-occurrence). Rules in `docs/CONTRACTS.md`, section 16. Routes in `crates/cs-app/src/v2_codes.rs`, logic in `crates/cs-store/src/{codestats,codesearch,fulltext,cooccurrence,codeops,textfold}.rs`. Errors: 400 `validation`, 404 not found, 409 `guard`. Booleans in a query string are `true` or `false`; lists are comma separated.

### Passage origins and roles (F-09)

`GET`/`PUT /api/project-config` carry two new lists, `passage_origins` and `passage_roles`: `[{id, label}]`. A project without them gets the built-in defaults (the previous fixed values, with French labels), so an old config still loads and the shipped `examples/project_template.example.json` lists them. The same checks as for the other lists apply (non-empty ids, no duplicates). A passage is accepted with an origin or role from the list, or one it already carries. Removing a value that a passage uses answers 409 (`guard`) from the config save. The origin and role lists of claims (provenance) are not configurable and stay as they were.

### Counts per code (F-45)

Every code of `GET /api/codes` (and `/api/codes/{id}`) gains, next to `passages` and `interviews`, the fields `cases`, `cases_rollup`, `attribute_values` and `attribute_values_rollup`. A case counts when one of its attributed units carries a passage of the code. `attribute_values` is `{attribute_id: n}`: the number of distinct values found on the interviews of the code (interviews without a value count in none); the `_rollup` variants add the descendants. The existing fields are unchanged.

| Method | Path | Answer |
|---|---|---|
| GET | `/api/codes/{id}/stats?attribute=&value=` | `{code_id, name, own: StatBlock, rollup: StatBlock, filter, note}`. `StatBlock` is `{passages, interviews, cases, interview_ids, case_ids, attributes: [{attribute_id, label, distinct_values, values: [{value, interviews}], interviews_without_value}]}`. With `attribute` and `value` (both or neither, 400 otherwise) the blocks count only the interviews carrying that value, and `filter` is `{attribute, value}` (null otherwise) |

### Code search (F-41)

| Method | Path | Answer |
|---|---|---|
| GET | `/api/codes/search?q=&limit=&include_inactive=&include_system=` | `{query, total, items: [{code_id, name, token, definition, parent_id, path, active, system, passages, interviews, score, matched: {field, text}}]}`. `path` is the names of the ancestors, root first |

Case and accent insensitive. Order: name, then alias, then token, then label, then definition; ties by name. `matched.field` says which of these hit, `text` is the matching alias or fragment. Inactive and merged codes and the reserved code are left out unless asked. An empty `q` lists every usable code in codebook order (`score` 0), for a picker.

### Split a code and undo (F-39)

| Method | Path | Body | Answer |
|---|---|---|---|
| POST | `/api/codes/{id}/split` | `{parts: [{name, definition, includes?, excludes?, parent_id?}], rationale}` | `{op_id, original, new_codes: [CodeNode], worklist}`. Two or more parts with distinct names that no code uses, a rationale, an active non-system code. `parent_id` defaults to the parent of the original |
| GET | `/api/splits/{op_id}/worklist?status=` | | `{op_id, original, new_codes, rationale, total, pending, assigned, left, gone, undone, items: [{passage, status: pending\|assigned\|left, targets, note, decided_at, decided_by, n_decisions}]}`. `status` filters the items (400 otherwise); the counts are always for the whole list |
| PUT | `/api/splits/{op_id}/worklist` | `{decisions: [{passage_id, action: assign\|leave, targets?: [id or name], note?}]}` | the worklist. All or nothing (400 and nothing written on any bad decision). `assign` needs one or more of the new codes; it removes the original coding and adds the new ones. `leave` keeps the original. A passage can be decided again; the earlier decision stays on record. 409 once the split is undone |
| GET | `/api/code-ops?code=` | | `{operations: [{op_id, kind: merge\|split, code_id, code_name, other_ids, rationale, created_at, created_by, undone: {rationale, at, by}\|null, summary}]}`, oldest first; with `code` only the operations that involve it |
| POST | `/api/code-ops/{op_id}/undo` | `{rationale}` | `{report: {op_id, kind, codings_restored, codings_removed, codings_moved_back, unit_codings_restored, decisions_restored, skipped}, codebook}` |

The worklist holds the live passages that carried the original at the time of the split. Merges made through `POST /api/codes/{id}/merge` are journaled too (that route is unchanged). An undo is refused with 409 when it was already undone or when a later operation not undone touches the same codes (undo the latest first), and with 404 for a merge made before the journal existed. `skipped` lists what the undo could not put back because it changed since (a coding, a unit coding or a review decision edited after the merge).

### Full-text search (F-55)

| Method | Path | Answer |
|---|---|---|
| GET | `/api/search/text?q=&mode=&prefix=&include_interviewer=&interviews=&attribute=&value=&coded=&code=&descendants=&limit=&offset=&context=` | `SearchResult` |

- `mode`: `all_words` (default, every word in the unit, any order) or `phrase` (the words in sequence). `prefix`: the last word matches word starts. Case and accent insensitive. Respondent units only, unless `include_interviewer`.
- Filters: `interviews` (ids), `attribute` and `value` (both), `coded` (`true` keeps the turns where a match lies in a coded passage, `false` those where a match lies in no coded passage), `code` (units whose passages carry the code; with `descendants` its sub-codes too).
- Answer: `{query, mode, words, total_units, total_hits, truncated, interviews: [{interview_id, units, hits, mentioned}], items: [{interview_id, unit_id, seq, role, speaker, text, matches: [{start, end, start_utf16, end_utf16}], excerpt: {offset, text, highlights, truncated_before, truncated_after}, previous, next, coded, uncovered_matches, passage_ids, code_ids}]}`. Offsets are Unicode scalar values on the unit text (UTF-16 for the browser). `uncovered_matches` counts matches outside every passage: the search is a way to find what is not coded yet. `previous` and `next` are the neighbouring turns `{unit_id, role, speaker, text}`. `limit` (default 100, at most 1000) and `offset` page the hits; `context` is the number of characters of context on each side of the first match in `excerpt`.
- The index is built in memory from the segmentations and corrections and rebuilt when they change (fingerprint). SQLite FTS5 is not used.

### Co-occurrence (F-56)

| Method | Path | Answer |
|---|---|---|
| GET | `/api/codes/cooccurrence?relation=&group_by=&attribute=&codes=&rollup=&min_units=&interviews=` | `{codes, groups: [{key, label, interviews, cells: [{a, b, units, interviews, passage_pairs, jaccard}], diagonal}], note}` |
| GET | `/api/codes/cooccurrence/passages?a=&b=&relation=&rollup=&interviews=` | `{a, b, relation, rollup, n_units, items: [{interview_id, unit_id, ..., pairs: [{a, b, overlap}]}]}`, the evidence behind a cell |

`relation` is `same_passage` (both codes on one passage), `overlap` (two passages of the unit whose spans overlap) or `same_unit` (default). `group_by` is `interview` or `attribute` (with `attribute`); without it one group `all`. `codes` restricts the matrix; `rollup` counts a code with its descendants; `min_units` hides cells below it. `diagonal` is the single-code count. `jaccard` is `units(a and b) / units(a or b)`.

### Exports

`co-occurrence.csv` in the interview and cross-case packages (and in their manifest and README): `relation,group,kind,code_a_id,code_a_name,code_b_id,code_b_name,passages,units,interviews,passage_pairs,jaccard`, for the three relations; `kind` is `single` or `pair`.

## V2 exports and CLI

### Write-up, anonymised export, whole project (F-104, F-102, F-101)

| Method | Path | Answer |
|---|---|---|
| GET | `/api/writing/export.docx` | The write-up as a Word document (`writeup.docx`) |
| POST | `/api/export/anonymised` | `{scope: interview or cross-case or writeup, interview_id?, researcher_names?, researcher_label?}` writes a pseudonymised copy of the package under `exports/`; `{dir, files, manifest_sha256, replacements: {person, organisation, researcher}, left_out}`; the names themselves are never returned |
| GET | `/api/project/bundle` | The whole project as one `.csproj.zip` (database snapshot, source transcripts, template, configuration, state, audit chain, `manifest.json`); header `x-export-sha256` |
| POST | `/api/project/bundle/import?apply=&target=` | The body is the bundle. Without `apply`, a dry run: the digests, the chain of the bundle and of the database, the state digest and the counts are checked and nothing is written. With `apply`, the bundle is restored; `target` is the path of a new database, absent the current one, which must be empty. `{ok, files, checks...}` |

### Example project (F-113)

| Method | Path | Answer |
|---|---|---|
| POST | `/api/example/load` | Body `{library?}`: the folder the two example transcripts are copied to (created if needed), absent a folder in the system temporary directory. Into an empty database only (409 otherwise, nothing changed): the example project template, the transcripts of `examples/library` scanned and segmented, the ten codes of `examples/codebook.csv` and three coded passages of `INT-01`. `{library, interview_id, interviews, units, codes, passages, research_questions}` |

### Command line (F-109)

`coding-studio --db <file> [options] <command>`. Without a command, or with `serve`, the server starts as before. Every other command opens the database, calls the same routes as the UI in process (no port is opened) and exits. `--json` prints the result as JSON; an error prints `{"ok": false, "error"}` with `--json`, a line on standard error otherwise, and the exit code is 1. `--library`, `--actor` and `--interviewer-name` apply as for the server.

| Command | Does |
|---|---|
| `scan [--root DIR]` | `POST /api/library/scan` |
| `segment ID [--source REL]` | `POST /api/interviews/{id}/segment` |
| `export ID\|all [--anonymised] [--format md\|docx] [--researcher-name N]...` | `md` (default): the package of one interview, or of every interview and the cross-case package with `all` (an interview without segmentation is reported and the exit code is 1). `--anonymised`: `POST /api/export/anonymised`. `--format docx`: the write-up, with `all` only, not with `--anonymised` |
| `verify` | Audit chain and the digests of every `exports/**/manifest.json` against the files beside it. Exit code 1 when the chain is broken or a file is missing or changed |
| `project export [--out PATH]`, `project import FILE [--apply] [--target DB]` | The bundle; import is a dry run unless `--apply` |
| `qdpx export [--out PATH] [--highlights]`, `qdpx import FILE [--apply]` | REFI-QDA project; import is a dry run unless `--apply` |
| `config export [--out PATH]`, `config import FILE [--force]` | The study configuration as JSON |
| `example load [--library DIR]` | `POST /api/example/load` |

Files go to `--out`, or to the `exports` folder beside the database.


## Personnes et groupes

The register of the people interviewed: persons and organisations (records of one table, `kind` = `person` or `organisation`), a tree of groups, links to interviews, and the pseudonymisation list derived from them. Schema version 8. Errors follow the usual mapping: 400 for an invalid value, 404 for an unknown id, 409 for a refused deletion.

**Person record.** `person_id` (`P-0001`, never reused), `kind`, `name`, `aliases` (list; each at least 3 characters), `organisation` (free text), `function`, `email`, `phone`, `notes`, `consent_status` (empty, or `pending`, `given`, `refused`, `withdrawn`), `consent_date` (`YYYY-MM-DD`), `consent_document` (a path, never read by the tool), `anonymise` (default true), `pseudonym` (generated as `Répondant 03` or `Participant 03` from the lexicon language, editable, unique), `group_ids`, `attributes` (values of the person-scoped attributes).

| Route | Does |
|---|---|
| `GET /api/persons` | List with `n_interviews`, groups and pseudonym. Filters: `kind`, `group_id` (the group and its descendants), `interview_id`, `anonymise` (true or false), `q` (name, alias or organisation) |
| `POST /api/persons` | Create from a body with the fields above (`name` required) |
| `GET /api/persons/{pid}` | The record, its groups (with path) and its interviews with roles |
| `PUT /api/persons/{pid}` | Partial update; absent fields are unchanged. Renaming keeps the id and the pseudonym |
| `DELETE /api/persons/{pid}` | 409 while the person is linked to an interview, unless `?with_links=true` |
| `POST /api/persons/{pid}/interviews` | `{interview_id, role}`; returns the record |
| `DELETE /api/persons/{pid}/interviews/{iv}` | Remove the link |
| `GET /api/interviews/{id}/persons` | Persons of one interview with their roles |
| `PUT /api/interviews/{id}/persons` | Replace the list: `{persons: [{person_id, role}]}` |
| `POST /api/interviews/{id}/persons` | Add one: `{person_id, role}` |
| `DELETE /api/interviews/{id}/persons/{pid}` | Remove one |
| `POST /api/persons/import` | `{csv, dry_run}`, `dry_run` true by default (see below) |

Roles: `respondent` (default), `co_respondent`, `mentioned`, `interviewer`. A person has one role per interview. `respondent` and `co_respondent` count as respondents for the prompt context and the exports.

**Groups.** A tree: a group has `group_id` (`G-0001`), `name`, `parent_id`, `description`. A person may belong to several groups (`group_ids` in the person body). Rules: sibling names are unique ignoring case (400); moving a group under itself or a descendant is refused (400); deleting a group that has children or members is refused (409) unless `?reassign_to=G-xxxx` names another group, which receives the sub-groups and the members (the target may not be the group itself or below it). Renaming and moving are `PUT` with `{name, parent_id, description}`.

| Route | Does |
|---|---|
| `GET /api/person-groups` | The tree. Each node: `group`, `n_direct` (members), `n_total` (distinct persons in the subtree, a person in two sub-groups counted once), `children` |
| `POST /api/person-groups` | `{name, parent_id?, description?}` |
| `GET /api/person-groups/{gid}` | The group, its node with counts and its members (subtree) |
| `PUT /api/person-groups/{gid}` | Rename, move, describe |
| `DELETE /api/person-groups/{gid}` | See the rules above |

**Person attributes.** `PUT /api/project-config` accepts `scope: "person"` on an attribute definition (default `"interview"`, omitted in the stored form). A person attribute has the same kinds and choices as an interview attribute, appears in `attributes` of the person and as `attr:<id>` columns of the CSV files, and never appears in the interview attribute table, the matrix or the code statistics. With `shared_with_model` the value of every respondent of the interview is added to the prompt context under `shared_person_attributes`, by pseudonym when the person is flagged. Removing a person attribute that holds values is a configuration conflict, as for interviews.

**Pseudonymisation list.** `GET /api/pseudonymisation` and the per-interview variant return the effective list, the one used by prompts, provider sends and anonymised exports. Each entry has `source`: `person` (derived from a flagged record: its name and each alias, all mapped to its pseudonym; `person_id` is given and the entry cannot be edited here) or `manual`. A manual entry that repeats a person name or alias is ignored and listed in `shadowed_manual`. The list is enabled when the manual switch is on or any record is flagged (`enabled_manual` gives the switch itself); `n_manual_entries` and `n_person_entries` count each part. `PUT` replaces the manual part only; entries with `source: "person"` are ignored. Validation (400): pseudonyms are unique; a name is not shared between two records; no name appears inside a pseudonym; an unflagged record may not carry a name or alias that a flagged record already replaces. A name that belongs to an unflagged record is never replaced. Replacement is case-insensitive, on word boundaries, longest name first.

**CSV import.** `POST /api/persons/import` with `{csv, dry_run}`. Delimiter comma or semicolon. Headers in English or French: `person_id`, `name`/`nom`, `kind`/`type`, `organisation`, `function`/`fonction`, `email`, `phone`, `notes`, `aliases` (separated by `;` or `|`), `groups`/`groupes` (several separated by `;`, a path with `/` such as `Détaillants/Suisse romande`, groups created when missing), `anonymise`/`anonymiser` (oui, non, true, false, 1, 0), `pseudonym`, `interviews`/`entretiens` (`ECO-16` or `ECO-16:co_respondent`, separated by `;`), `consent_status`, `consent_date`, `consent_document`, and `attr:<id>` for person attributes. A row is matched by `person_id`, else by exact name ignoring case; otherwise it creates. An empty cell leaves the field unchanged; groups and interviews are added, never removed. The report: `ok`, `dry_run`, `applied`, `n_rows`, `n_create`, `n_update`, `n_unchanged`, `n_links`, `groups_to_create`, `rows` (one per row with the id it gets), `problems` (`row`, `message`; row 1 is the header), `csv_sha256`. Nothing is written when any row has a problem, nor on a dry run. An apply is one transaction and one audit event. Applying the same file again changes nothing.

**Files in the packages.** The interview and cross-case packages and the project bundle (`config/persons.csv`, `config/groups.csv`) carry:

- `persons.csv`: `person_id, kind, name, aliases, organisation, function, email, phone, notes, consent_status, consent_date, consent_document, anonymise, pseudonym, groups, interviews`, then one `attr:<id>` column per person attribute. In a package of interviews only the persons linked to those interviews appear. These packages are not anonymised: names, aliases, email, phone and notes are written in full, and the README says so.
- `groups.csv`: `group_id, name, parent_id, path, description, n_direct, n_total`.
- In the anonymised export (`POST /api/export/anonymised`) `persons.csv` is replaced by a public version: `person_id, kind, name, anonymise, pseudonym, organisation, function, groups, interviews, consent_status, consent_date`, then the attributes. For a flagged record `name` and `organisation` read `[masquée]` and the pseudonym stands; email, phone, notes, aliases and the consent document are not written. Unflagged records keep their names.

**Verification package.** `POST /api/export/verification` with `{interview_ids: [...], all: bool, out_dir?}` for examiners. Layout under the output folder:

```
README.md
manifest.json
persons.csv            (public version)
groups.csv
interviews/<ID>/original/<rel_path>   (the source, byte for byte when unmodified)
interviews/<ID>/transcript_coded.md   (masked with the effective list)
```

The mask applies whatever the state of the manual switch. `manifest.json` lists per source `original_sha256` (as recorded at import), `recorded_sha256`, `intact` (the file on disk still has the recorded digest), `delivered` (false for a source that is missing or not copied, with a reason), `delivered_as` (the path in the package), `delivered_sha256`, `replaced` (true when the delivered text differs from the original) and `replacements` (pseudonym and count; never a name). The coded transcript has its own entry with `delivered`, `delivered_as`, `delivered_sha256` and `replaced`. An unmodified source is delivered unchanged, so its delivered digest equals its original digest. A `.docx` or `.pdf` source is listed with its original digest and not copied.

CLI: `coding-studio verification-export <ids|all> --out DIR` calls this route. `all` cannot be combined with ids.

**Audit.** Every write is one event in the same transaction as the write: `person.create`, `person.update`, `person.delete`, `person.link`, `person.unlink`, `persons.import`, `person_group.create`, `person_group.update`, `person_group.delete`, `pseudonymisation.update`. Events hold ids, the names of the fields changed, flags, pseudonyms and digests; never a name, an alias, an email, a phone number or a note.

### Analysis report for Word

`GET /api/analysis/export.docx` streams `analysis-report.docx` (Word OOXML).
`POST /api/analysis/export` writes a new versioned `exports/YYYY-MM-DD_analysis-report/`
folder (a suffix is used when it exists) and returns `{dir,files}`. The folder
contains the Word report, a Markdown companion, complete `analysis-evidence.json`
(source links, notes, audit, matrices, decisions and codebook), and a SHA-256/size
manifest. The Analysis screen uses this saved package so it also works in the
native desktop shell.

The report assembles recorded retained-theme writing and typed evidence,
research-question answers, theme-sheet boundaries and checks, interview/theme
comparisons with manual judgements, manual matrix evidence, framework readings,
documented decisions, the active codebook and the latest audit. It never calls
an external model or invents missing analysis. Missing text and unassessed matrix
cells remain explicit. Counts reflect current code coverage, not population
prevalence, causal support or saturation. Source references retain interview,
unit, passage, link role and verification state. Configured pseudonymisation is
applied to every text; unlisted identifying information still needs review.
Export does not establish audio or independent researcher validation, and does
not assert that the underlying analysis was produced without AI assistance.


### Protected reviewer export

`GET /api/export/reviewer/catalogue?pseudonymise=true` returns a whitelisted research-data projection, stable field IDs, a content snapshot, default selections, source-file IDs/sizes, pseudonym-rule count and transcript coverage. No raw database, provider configuration or audit history is offered.

`POST /api/export/reviewer` accepts `{snapshot, selected: string[], password, pseudonymise: boolean, url?: string}`. The server rebuilds the projection and rejects stale, duplicate, empty or unknown selections. The password needs 16 Unicode characters and is never logged or persisted. Sources are checked against their recorded hashes and encrypted in 8 MiB chunks. Paths are confined to interview folders; symlinks and files over 1 GiB are refused. Text JSON is limited to 256 MiB explicitly. A staging directory is removed on failure. The returned local package has `published:false`, an encrypted reader and an upload ZIP. URL.txt and QR.svg exist only with an explicit HTTPS address without credentials, query or fragment. AES-256-GCM; PBKDF2-HMAC-SHA256 at 600,000 iterations, random 16-byte salt and fresh 12-byte IV per part, authenticated part identity.

`POST /api/export/reviewer/open` accepts `{dir, kind: "folder" | "reader"}` and opens only an immediate reviewer-package folder under the current database’s exports directory. No publication occurs.

## Local anonymisation and release gates

`GET /api/anonymisation` returns the local-only effective and manual replacement table, interview review states and the reference prompt. `PUT` replaces its manual entries (`person|organisation|contact|other`, `original`, `pseudonym`); identity mappings are retained only in this setting, with SHA-256-only audit payloads. Personal register names remain protected irrespective of the older anonymise opt-out. Existing organisation rules are retained. Generic speaker roles remain unchanged.

`GET /api/anonymisation/interviews/{id}` previews original and anonymised units and speakers, source-character replacement spans, capitalisation hints and a content digest. Hints are not complete NER. `PUT {digest,reviewed:true}` records a researcher attestation for that exact transcript and effective rules. Changes invalidate it. `POST .../{id}/export` writes only a reviewed segmented text/JSON copy and manifest, without private mapping or original attachments.

`GET /api/anonymisation/interviews/{id}/batches/{bid}` returns safe prompt parts and a digest. `PUT {digest,reviewed:true}` requires transcript approval and records exact prompt plus source/configuration/codebook/attributes approval. `POST .../copy` rechecks it and returns only safe parts. Every configured provider uses this same guard before invocation. Old stored prompts/AI runs remain unchanged; exposed prompt copies are remasked locally. An incoming AI answer is never reverse-masked for another outgoing request.

Reviewer catalogue/export refuse `pseudonymise:false`; original source files and contact/consent fields are excluded. Export requires all current transcript approvals and `privacy_reviewed:true` for the exact selected snapshot. Its passage offsets are translated onto the anonymised unit text. Academic anonymised packages also require transcript approval. Complete project backups are explicitly private and remain separate from reviewer exports. No detector claims complete anonymity: indirect identifiers and unknown names require local human review. Raw transcripts and identity tables are never sent to a model for detection.

## Local transcript preparation during import

- `GET|PUT /api/local-preparation/config`: per-database local endpoint/model and independent `anonymise`, `cleanup`, `target_language` options. Empty language means retain original language.
- `POST /api/local-preparation/models {url}`: discovery only, never sends a transcript. Bare root URL gets `/v1`.
- `POST /api/interviews/{id}/local-preparation {url,model,anonymise,cleanup,target_language,context_tokens?,output_tokens?,reasoning?,reasoning_effort?}`: prepare a private draft. Default context is 131072 tokens. Anonymisation prefers the entire interview, splitting only when the estimated input plus instructions, dictionary, output reserve and safety margin exceed context. Cleanup/translation also bound the repeated output using the configured initial `output_tokens` budget (32768 by default, capped to half the context). Token estimates use UTF-8 bytes/3, not an exact model tokenizer. All direct calls use OpenAI Chat Completions (`/chat/completions`) with `max_completion_tokens` and `store:false`. `reasoning:"server"` omits the effort override, `"off"` sends `reasoning_effort:"none"`, and `"on"` sends the selected `reasoning_effort` (`minimal`, `low`, `medium`, `high`, `xhigh`, `max`; default medium). Server/model support is required; there is no fallback to LM Studio native APIs. A `finish_reason:"length"` triggers up to two retries with a larger output budget within estimated remaining context. Partial replies never become drafts. The HTTP timeout is 600 seconds per request. Source units are loaded without saving a segmentation for a newly scanned interview. Existing human coding, passages or prompts are preserved: approval then saves a separate reading/export derivative instead of replacing the coded segmentation. No cloud credentials are read. Configured DNS hostnames and all IP addresses are accepted without network-scope restrictions, no userinfo/query/fragment, no environment proxy, no redirects, finite per-request timeout and response limit. A selected local server can itself relay elsewhere; trust remains the user's responsibility.
- `GET .../local-preparation`: resume the draft, including `draft_digest`, original, anonymised verbatim and proposed derivative versions, private correspondence table, options and accepted variant. This endpoint is local/private and never part of the reviewer projection.
- `GET .../local-preparation/progress` / `POST .../local-preparation/cancel`: per-database/interview progress and cancellation between model calls. Duplicate concurrent jobs refuse. Closing the project cancels its jobs between requests.
- `POST .../local-preparation/apply {digest,reviewed:true,use_prepared:boolean}`: verify exact draft and unchanged source/rules; atomically save the private mapping and receipt, plus the chosen segmentation only for uncoded interviews; coded/reviewed interviews receive a reading copy (`applied_to_coding:false`). An anonymised verbatim copy can be chosen instead of the cleaned/translated proposal. Existing corrections are retained and a new overlay records the accepted derivative, so old corrections cannot silently override it. All identity/preparation setting audit payloads contain hashes, never the private mapping.

AI identifies names, while deterministic replacement preserves all other characters for the verbatim copy. Cleanup/translation are separate model passes; unit order/count/IDs, labels and numeric values are checked. These checks do not establish semantic fidelity. Full human review remains mandatory, and phase-2 external-review guards remain in force. Original files are never overwritten. The whitelisted reviewer projection includes only approved preparation metadata (model/options/chosen version), no raw draft or private table; its reader and Word report mark derivatives as such.

Desktop projects each own one window, one bound loopback listener, one router/store, one database path and independent project session storage. Opening a canonical database twice focuses its existing window. Newly opened projects never inherit another project's library or legacy codebook. Closing a window stops that server and its preparation jobs; application quit closes all project windows.

### Participant spreadsheets and identified reviewer copies

`POST /api/persons/import-table {data:[byte,...]}` reads XLSX/XLSM locally and returns `sheets:[{name,rows}]` plus warnings, without modifying the register. Limits: 5 MiB compressed, 50 MiB expanded, 12 MiB per XML member, 5000 rows per sheet, 1024 columns. Macros, formulas and external relationships never execute; formulas use cached results only. Standard date formats are converted to ISO dates. The UI maps columns (including joined first/last names and interview IDs) into the existing CSV dry-run/atomic-apply API. Manual person–interview links remain available in the interview fiche.

Reviewer catalogue/export `pseudonymise:false` explicitly selects an identified copy. Its catalogue includes selectable original source files, participant details and participant–interview links. Approved local preparation adds optional `Original transcript` (identified mode only) and `Prepared transcript` records; identity correspondence tables, credentials and arbitrary settings stay excluded. Both export modes require reviewed selection and a password of at least 16 characters; all records and source chunks are encrypted with the existing AES-GCM package. No plaintext identified-export option or automatic hosting is provided. The reader distinguishes coding reference, original and prepared reading versions; highlights are displayed only on the coding reference.

`GET /api/interviews/{id}/participant-review.docx?codes=true&themes=true&comments=true` deterministically renders an editable Word review for that interview only. It includes the retained original and an approved accepted translation when present, applied passages/codes, related themes and optional scoped comments. There are correction spaces and no implied participant approval. No other interview transcript, AI prompt, corpus report or automatic sending is included. This is a private local document download, distinct from the encrypted reviewer-site export.

### Staged local preparation

Preparation accepts `stage: "anonymisation" | "transformation" | "all"` (legacy default), `feedback: [{unit_id,instruction}]` and `draft_digest` for revisions. Transformation with anonymisation enabled requires the approved anonymisation draft.

`POST /api/interviews/{id}/local-preparation/review` accepts `{digest,reviewed:true,pending_diffs:0,units:[{unit_id,text,speaker_raw}]}`. It verifies the exact draft, source and unit order before saving the researcher-reviewed stage privately. Final apply requires every requested stage to be approved. Stage review does not change the coded segmentation or the shared identity dictionary.

Local preparation and model discovery accept optional `bearer_token` (with or without the Bearer prefix). Saving configuration keeps the token only in process memory, scoped to database and normalized endpoint. GET config returns `has_bearer_token`, never the token. Omitting it reuses the session token; saving an explicit empty string removes it. Tokens are not serialized into configuration, drafts, audit payloads or exports. Re-enter after application restart. Batch preparation may pass earlier private `batch_entries` to reserve labels and reuse names; unrelated pending mappings are never committed by another interview. Compatible additive dictionary changes do not invalidate a different batch draft; source edits, removal of inherited mappings and conflicting name mappings still block approval.

### Manual transcript preparation (copy/paste)

The library defaults to manual preparation. POST local-preparation with the usual operations, an explicit `stage` and `prompt_only:true` returns `{prompt,prompt_digest}`. It makes no model request, includes the whole interview and saves the private prompt receipt. Endpoint/model configuration is not required. Submit the same operations/stage with `pasted_response` and `prompt_digest` to create a private draft from the JSON response. The source, identity rules, stage and feedback must still match; invalid JSON, invented names, omitted/reordered turns and fidelity errors refuse without applying a transcript. The draft then follows the existing per-difference review and stage approval. Manual prompt/response exchanges are retained privately across revisions and stages; no claim is made about the external model used. Batch manual preparation generates all selected prompts before displaying their response fields and opens the grouped review after every response is imported.

### JSON review decisions and editable codebook tree

The review screen accepts a pasted JSON document (or a `.json` file):

```json
{"interview_id":"INT-01","decisions":[{"unit_id":"INT-01-U002","decision":"accept_reviewer","final_codes":["C-0001"],"comment":"Reason for this decision."}]}
```

`decision` accepts `keep_mine`, `accept_reviewer`, `merge`, or `custom`. Code
identifiers may be stable ids or exact tokens; `NA` is used on its own. The final
code list is authoritative. `keep_mine` is recorded as a human choice only if it
matches the frozen human reference; `accept_reviewer` selects a matching complete
AI run, otherwise the decision is stored as custom. Optional `run_label` makes
that AI selection explicit and requires the final codes to match that run.
Comments are required for disagreements. Duplicate units, missing units, inactive
or merged codes, and a different interview id reject the complete batch.

- `POST /api/interviews/{id}/review/import/preview` takes `{raw}` and returns
  `{digest, count, replacements, changes}` without writing.
- `POST /api/interviews/{id}/review/import` takes
  `{raw, expected_digest, replace_existing}`. The digest must match the exact
  reviewed state and input. Existing decisions require `replace_existing:true`.
  Decisions and their audit/history rows are committed in one SQLite transaction.
  Frozen human snapshots and AI answers are unchanged.

The codebook toolbar exports `GET /api/codebook/json`, using schema
`coding-studio.codebook.v1`. Each node has stable `code_id` and `token`, editable
`name`, `label`, `definition`, `includes`, `excludes`, `toe`,
`research_questions`, `active`, `in_prompt`, and a nested `children` array.
Children arrays determine parentage and sibling order. `merged_into` is read-only;
merges must use the merge operation. No aliases are introduced by this import.
Existing ids/tokens remain unchanged; new nodes use a fresh `C-0001` style id as
both id and token. Omitted codes are retained; this is not a deletion operation.
Questions must already exist in the target study. Memos, counts and historical
metadata are not part of this editable exchange; they remain in the database.

- `POST /api/codebook/json/import/preview` takes `{raw}` and returns the same
  preview envelope with before/after values and tree positions.
- `POST /api/codebook/json/import` takes `{raw, expected_digest}` and commits the
  validated tree changes together. Definition changes open the existing
  harmonisation worklist. Passage coding and frozen evidence are retained.

Both importers accept either strict JSON or one `json` Markdown fence. Editing
pasted content invalidates its UI preview. Validation or storage failure leaves
the entire batch unapplied, including its history records.

`PUT /api/interviews/{id}/harmonisation/confirm` takes
`{items:[{item_id, note?}]}` and returns `{confirmed}`. It confirms only the
specified open work items, preserving current codings and existing resolutions.
Items outside the interview, duplicate ids, or already resolved items reject the
entire request. Resolutions and audit records commit together in one transaction.
Work items created after the client loaded its list are not implicitly included.
