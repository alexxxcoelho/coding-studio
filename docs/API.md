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
| POST | `/api/interviews/{id}/segment` | `{rel_path?}` | `Segmentation` |

## Coding

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/codebook/phase1` | | `Codebook` (TECH, ORG, ENV, CONTEXT) |
| GET | `/api/interviews/{id}/phases/{p}` | | `{phase, summary: PhaseSummary, guard: GuardStatus}` |
| GET | `/api/interviews/{id}/phases/1/units` | | `[{unit, coding: HumanCoding\|null, allowed: Code[]}]`, every unit; `allowed` = the four phase 1 categories (NA always valid besides) |
| GET | `/api/interviews/{id}/phases/2/units` | | `[{unit, coding: HumanCoding\|null, allowed: Code[], phase1: string[]\|null, in_scope: boolean, to_clarify: ToClarify[]}]`: the units in scope (phase 1 category not NA) and the out-of-scope units that still carry a phase 2 coding, in segmentation order. `allowed` = every phase 2 code, those attached to the unit's phase 1 contexts first |
| PUT | `/api/interviews/{id}/phases/{p}/coding/{unit_id}` | `HumanCoding` | `HumanCoding` as stored. Phase 1: TECH/ORG/ENV/CONTEXT or NA, always editable. Phase 2: phase 2 codes or NA; `codes` may be empty only if the unit has an open label to clarify |

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

The interview export package holds `units.csv`, `phase1_contexts.csv`, `phase2_coding_human.csv` (with `revised_after_ai`), `phase2_human_full.csv`, `phase2_batch<b>_prompt_part<n>_of<m>_<hash>.txt`, per run `phase2_run<id>_<label>.json`, `phase2_coding_ai_run<id>_<label>.csv` and `phase2_ai_run<id>_<label>_part<n>_raw.txt`, `phase2_agreement.md` and `phase2_review.md` (once a run is complete), `phase2_coding_final.csv`, `to_clarify.csv`, `cases.json`, `cases.md`, `codebook.csv`, `audit_log.jsonl`, `audit_chain.json`, `manifest.json` and `README.md`. The codebook export writes `<db dir>/exports/codebook/<YYYY-MM-DD_HHMMSS>_codebook.csv` and `.sql`, never overwriting.

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

