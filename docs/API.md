# Coding Studio HTTP API (v2, two coding phases)

Local JSON API served by `cs-app` on 127.0.0.1. The web UI (React, `web/`) is its only client. Types are those of `crates/cs-model/src/lib.rs`, serialised with serde (snake_case fields, enums in snake_case). `{phase}` is `1` (TOE contexts) or `2` (study-specific codes).

Errors: status 400 (validation), 404 (not found), 409 (protocol guard), 500 (internal), 501 (native dialog unavailable), body `{"error": string, "kind": "validation"|"guard"|"not_found"|"internal", "issues"?: ValidationIssue[]}`.

## State and library

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/state` | | `{version, db_path, library_root, coding_db, actor, codebook_size, chain: ChainCheck, native_dialogs: bool}` |
| POST | `/api/pick` | `{kind: "folder"\|"file", title?}` | `{path: string\|null}` (501 in browser mode) |
| POST | `/api/library/scan` | `{root?}` | `{root, interviews: InterviewSummary[]}` |
| GET | `/api/interviews` | | `InterviewSummary[]` (with `phase1`, `phase2`: PhaseSummary) |
| GET | `/api/interviews/{id}` | | `{entry: InterviewEntry, segmentation: Segmentation\|null, phase1: PhaseSummary, phase2: PhaseSummary, phase2_started: bool}` |
| GET | `/api/interviews/{id}/file?path=rel` | | `{path, kind, sha256, changed_since_scan: bool, text}` (inventoried files only) |
| POST | `/api/interviews/{id}/primary` | `{rel_path}` | `InterviewEntry` |
| POST | `/api/interviews/{id}/segment` | `{rel_path?}` | `Segmentation` |

## Coding, per phase

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/phases/{phase}` | | `{phase, summary: PhaseSummary, guard: GuardStatus, reconciled: bool, frozen: bool, locked: bool}` where `locked` = human coding locked (AI output imported), `frozen` = phase 1 frozen because phase 2 started |
| GET | `/api/interviews/{id}/phases/{phase}/units` | | `[{unit: Unit, coding: HumanCoding\|null, allowed: Code[], phase1_final: string[]\|null}]` for the units in scope, in order. `allowed` = codes the candidate may use on this unit (phase 1: the four TOE categories; phase 2: `phase2_allowed` with inductive codes). NA is always valid and not listed. `phase1_final` is filled in phase 2 |
| PUT | `/api/interviews/{id}/phases/{phase}/coding/{unit_id}` | `HumanCoding` | `HumanCoding` |
| GET | `/api/codebook/phase1` | | `Codebook` (the four phase 1 categories) |

## AI pass, per phase

| Method | Path | Body | Answer |
|---|---|---|---|
| POST | `/api/interviews/{id}/phases/{phase}/prompts` | `{batch_size}` | `PromptPart[]` (text included), recorded |
| GET | `/api/interviews/{id}/phases/{phase}/prompts` | | `PromptPart[]` |
| GET | `/api/interviews/{id}/phases/{phase}/runs` | | `AiRun[]` |
| POST | `/api/interviews/{id}/phases/{phase}/runs` | `AiRunMeta` | `AiRun` (`prompt_id_version` defaults to the current prompt version) |
| POST | `/api/interviews/{id}/phases/{phase}/runs/{run_id}/outputs` | `{raw, part?, of?}` | `AiRun` updated, or 400 with `issues` |

## Reconciliation, per phase

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/interviews/{id}/phases/{phase}/agreement` | | `{reports: AgreementReport[], markdown}` (human vs each complete run, then runs pairwise) |
| GET | `/api/interviews/{id}/phases/{phase}/divergences` | | `{items: [{divergence, unit, arbitration\|null, history: Arbitration[], allowed: Code[]}], n_divergences, n_decided, runs: string[]}` |
| PUT | `/api/interviews/{id}/phases/{phase}/arbitration/{unit_id}` | `{choice: ArbitrationChoice, final_codes: string[], rationale}` | `Arbitration` |
| GET | `/api/interviews/{id}/phases/{phase}/final` | | `{reconciled: bool, finals: FinalCoding[], pending: string[]}` |

## Codebook and inductive log

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/codebook` | | `Codebook` (local copy of coding.db codes plus the candidate's inductive codes) |
| POST | `/api/codebook/import` | `{coding_db?}` | `Codebook` |
| POST | `/api/codebook/codes` | `Code` (origin `inductive`, family `INDUCTIVE`; `definition`, `reason` required, `example` optional) | `Codebook` |
| PUT | `/api/codebook/codes/{code}/in_prompt` | `{in_prompt}` | `Codebook` |

## Audit and export

| Method | Path | Body | Answer |
|---|---|---|---|
| GET | `/api/audit?interview_id=` | | `AuditEvent[]` |
| GET | `/api/audit/verify` | | `ChainCheck` |
| POST | `/api/interviews/{id}/export` | | `{dir, files: [{name, sha256, size}]}` |

The export package holds, per phase, `phase{p}_coding_human.csv`, `phase{p}_human_full.csv`, `phase{p}_coding_ai_<label>.csv`, raw answers, run metadata, prompts, `phase{p}_agreement.md`, `phase{p}_adjudication_log.md`, `phase{p}_coding_final.csv` when reconciled, plus `units.csv`, `inductive_log.csv`, `audit_log.jsonl`, `audit_chain.json`, `manifest.json` and `README.md`.
