# Coding Studio v3: provisional TOE, concrete codes, AI as challenger, case synthesis

Supersedes the two-phase locking design of v2. Decided by the candidate on 2026-10-12, following supervisor-style guidance: keep TOE and the tool, change mostly the coding.

## Principles

1. Phase 1 is a provisional pre-classification by the candidate: TECH, ORG, ENV, CONTEXT or NA per unit. It is always revisable, never locked, has no AI pass and no arbitration.
2. Phase 2 codes units with concrete codes ("legal obligation", "customer requirement", "lack of skills", "cost without benefit for the payer"...). Each code carries a TOE attachment (T, O, E or none) and an origin (literature or field). A unit may carry several codes. Every active code is offered on every unit in scope; codes attached to the unit's phase 1 contexts are listed first.
3. "To clarify" replaces INDUCTIVE-PENDING: on a unit, the candidate records a provisional label and a short note. Later the label is resolved, either by creating a code (origin field) or by mapping it to an existing code; the resolution adds the code to every unit carrying that label. "Field" is a property of a code, not a category next to T/O/E.
4. The AI challenges the detailed codes, blind (it never sees the candidate's codes, phase 1 contexts, labels to clarify, memos or summaries). The candidate reviews every disagreement and a seeded sample of agreements, and keeps the final decision. The aim is better interpretation; agreement figures are reported, not optimised.
5. A synthesis per case (trigger, alternatives, constraints, choice, result), each field backed by units, then a cross-case comparison organised by research question.

No real database exists yet: schema version 3, older databases are refused.

## Model changes (cs-model)

- Phase 1 categories: `TECH`, `ORG`, `ENV`, `CONTEXT` (+ NA). `INDUCTIVE-PENDING` disappears. `PHASE1_CODEBOOK_VERSION = "TOE-contexts v2.0 (provisional pre-classification)"`.
- `Code` gains: `toe: Option<String>` ("T" | "O" | "E" | null), `origin` values `literature` | `field` | `legacy` (import maps coding.db `codebook` to `literature`, `inductive` to `field`, keeps `legacy`; `bookkeeping` codes are not imported), `research_questions: Vec<String>` (ids of research questions), `merged_into: Option<String>`, `version: u32` (bumped on each edit), plus existing `example`, `reason`, `created_at`. `family` stays for display: `TECH`/`ORG`/`ENV` from toe, else `OTHER`.
- New `ToClarify { id: i64, interview_id, unit_id, label, note, created_at, created_by, resolved: Option<Resolution> }`, `Resolution { kind: "new_code" | "mapped", code, at, by, rationale }`.
- New `ResearchQuestion { id: String /* RQ1 */, text: String, ord: u32 }`.
- New `Case { case_id: String /* ECO-05-C1 */, interview_id, label, trigger: CaseField, alternatives: CaseField, constraints: CaseField, choice: CaseField, result: CaseField, stage: Option<String>, outcome_status: Option<String>, notes: Option<String>, updated_at }`, `CaseField { text: String, evidence: Vec<String> /* unit_ids */ }`.
- Review: `ReviewItem { unit_id, kind: "disagreement" | "agreement_sample", human: CodeSet, ai: BTreeMap<run_label, CodeSet>, decision: Option<Arbitration> }`. Agreement sample: `max(5, ceil(10%))` of agreeing units, drawn with a seeded deterministic shuffle (seed stored and logged).
- `PhaseSummary` stays for phase 2; phase 1 summary only counts coded units.

## Store rules (cs-store)

- Phase 1 coding: TECH/ORG/ENV/CONTEXT/NA only, editable at any time, audited before/after. No runs, no prompts, no arbitration in phase 1 (Validation error if attempted).
- Phase 2 scope: units whose current phase 1 coding exists and is not NA. A unit leaving the scope keeps its phase 2 coding (shown as out of scope, excluded from new prompts).
- Phase 2 human coding: any active, non-merged code (not legacy) or NA; a unit counts as coded when it has codes or an open label to clarify. Editable at any time; an edit made after an AI output was imported for that unit is flagged `revised_after_ai = true` in the coding row and the audit detail.
- Prompts record their unit list; an AI output is validated against the unit list and the codebook snapshot (codes + hash) of the prompt part it answers, not the live scope.
- Blind guard kept: an AI output can be imported only when every unit of its prompt part has a phase 2 coding by the candidate.
- Review: disagreements = units of a complete run where the candidate's codes differ from the run; agreement sample drawn on first request per run and stored (append-only). A decision (keep, take AI, custom) needs a rationale for disagreements; for sampled agreements a short note is optional. Final coding = decided codes where a decision exists, otherwise the candidate's current codes. Nothing blocks on review completeness; the review status is reported.
- To clarify: create, edit (while open), resolve by `new_code` (creates the code with origin field, definition and toe required) or `mapped` (existing code), optionally for every open item with the same label in the corpus at once. Resolution adds the code to each unit's phase 2 coding (audited).
- Codebook: create, edit (definition, label, toe, research_questions, example, reason; version bump), deactivate, merge `from` into `into` (rewrites the candidate's phase 2 codings and open decisions, audited; AI outputs are kept as received). Export: CSV and a SQL file compatible with `05_Analysis/coding/schema.sql` `codes` table (family from toe or OTHER, origin field -> inductive, literature -> codebook, sub_question = research question ids).
- Research questions: CRUD, ordered.
- Cases: CRUD per interview; evidence unit ids must exist; audited.
- Cross-case matrix: for each research question, for each case, the codes mapped to that question found in the final phase 2 coding of the case's interview (or of its evidence units when the case lists any), with unit ids.

## AI (cs-ai)

- Only phase 2. The prompt lists every active, in-prompt code grouped by TOE attachment (T, O, E, other) with label and definition; units are plain (no phase 1 context, no allowed list). Same P-04 rules. `PROMPT_VERSION = "CS-P04-EN v3.0"`. Answer shape unchanged except `phase` is always 2.

## HTTP API v3 (cs-app) changes from docs/API.md

- Removed: `/phases/1/prompts`, `/phases/1/runs*`, `/phases/1/agreement`, `/phases/1/divergences`, `/phases/1/arbitration/*`, `/phases/1/final`.
- `GET /api/interviews/{id}/phases/{p}/units`: phase 2 rows add `phase1: string[]|null`, `in_scope: bool`, `to_clarify: ToClarify[]`, `revised_after_ai: bool`; `allowed` lists every active code, attached contexts first.
- Review (phase 2): `GET /api/interviews/{id}/review` -> `{runs, items: ReviewItem[] (with unit), n_disagreements, n_sampled, n_decided, seed}`; `PUT /api/interviews/{id}/review/{unit_id}` `{choice, final_codes, rationale}`; `GET /api/interviews/{id}/final` -> `{finals, n_decided, n_open_disagreements}`; `GET /api/interviews/{id}/agreement` kept (phase 2).
- To clarify: `GET /api/to-clarify?interview_id=&open=` ; `POST /api/interviews/{id}/units/{unit_id}/to-clarify` `{label, note}` ; `PUT /api/to-clarify/{tid}` `{label, note}` ; `POST /api/to-clarify/{tid}/resolve` `{kind, code, rationale, all_with_label: bool, new_code?: Code}`.
- Codebook: `PUT /api/codebook/codes/{code}` (edit), `POST /api/codebook/codes/{code}/merge` `{into, rationale}`, `POST /api/codebook/codes/{code}/active` `{active}`, `GET /api/codebook/codes/{code}/units` (every unit carrying the code, across interviews, with text and interview id), `POST /api/codebook/export` -> files written.
- Research questions: `GET/POST /api/research-questions`, `PUT/DELETE /api/research-questions/{id}` (delete refused while codes map to it).
- Cases: `GET /api/interviews/{id}/cases`, `POST /api/interviews/{id}/cases`, `PUT /api/cases/{case_id}`, `GET /api/cases` (all), `GET /api/cross-case` -> `{questions, cases, cells: [{rq, case_id, codes: [{code, units}]}]}`.
- Export package adds `phase1_contexts.csv`, `phase2_coding_human.csv` (+ `revised_after_ai` column), review files, `to_clarify.csv`, `cases.json`, `cases.md`.

## UI (web)

- Coding phase 1: unchanged keys (T, O, E, C, N; I removed), banner "provisional pre-classification, revisable at any time", no lock banners.
- Coding phase 2: code picker shows codes grouped by TOE (the unit's phase 1 contexts first), with origin badge (literature / field); "To clarify" button (label + note) with keyboard shortcut `?`; open labels shown on the unit.
- AI pass: phase 2 only.
- Review (replaces Reconciliation): disagreements then sampled agreements, decision with rationale, agreement figures in a secondary panel.
- To clarify page (sidebar): open labels grouped by label across interviews, resolve to a new or an existing code, all at once.
- Codebook page: edit codes inline (toe, origin, research questions, definition), merge, deactivate, "units with this code" drawer to revisit passages, export to coding.db format.
- Research questions editor (in Codebook page).
- Cases page per interview (tab "Cases") and a global "Cross-case" page: matrix by research question.
