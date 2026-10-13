# DEVGRAPH V1 lots 3 and 4 (13.10.2026): lexical workbench and review, server side

Branch `worktree-agent-a8e5d24b69a263e0e`. Routes and shapes: `docs/API.md`, "V1 lots 3 and 4". Contracts: `docs/CONTRACTS.md`, section 14 (lot 4).

## Done

- Lot 3: crate `cs-lex` (tokens, stop words, stems, YAKE, TF-IDF, cosine), store module and routes in `cs-app/src/lexicon.rs`: keywords, KWIC, term frequency, config, status, lexical panel of a passage, similar passages and units.
- Merge of `main` (lot 5, schema 6, `cs-qdpx`, ingest formats). The schema is now 7: `review_schema.sql` and `review_schema::migrate`, idempotent, run on every open after the lot 5 migration. Tables `concept_candidates`, `concept_decisions`, `harmonisation_items`; columns `human_codings.source` and `ai_runs.pseudonymisation`. Append-only triggers on candidates and decisions.
- Lot 4 model and prompts: `cs-model/src/pseudonym.rs`, `candidate_concepts` in `AiOutput`, `PromptContext` and `build_prompts_configured` in `cs-ai`, quote check (A9).
- Lot 4 store (`cs-store/src/lot4.rs`): passage to unit sync, candidates and decisions, pseudonymisation list and usage, harmonisation worklist, review state. Hooks in `codetree.rs`, `codebook.rs`, `passages.rs`, `coding.rs`, `review.rs`.
- Lot 4 app (`cs-app/src/v1_review.rs`): routes, prompt context, refusal of an unmasked send, run record, answer check. Wiring in `lib.rs` and `providers.rs`.
- Exports: `review.json` and the `review` state part.
- Tests: `cs-store/src/tests/lot4_review.rs`, `cs-ai/tests/prompt_configured.rs`, `cs-app/tests/review_lot4.rs` (A9, A10, A13 to A18, A28, sync, worklist, export), `lexicon_api.rs`, `lexicon_v1.rs`.

## Choices and deviations

- The prompt version string stays `CS-P04-EN v3.1`. The configuration is traced by digests in the meta lines and in the audit.
- The preceding interviewer turns sit in a `## Interviewer questions` block before `## Units`, not on the unit lines, so that the unit line format and `e2e.rs` stay unchanged. The test in `simple_api.rs` was updated for that.
- A flat codebook keeps the Technology, Organisation, Environment grouping; the tree rendering is used when at least one code has a parent.
- The snapshot is taken in the same transaction as the prompt, but its audit event is written first.
- The sync overwrites a hand unit coding when passages exist for the unit.
- A `create` decision on a candidate is atomic with the code creation.
- `interviewer_units` now uses the corrected segmentation (role corrections apply).
- The per-interview pseudonymisation view lists only the names found in that interview; the project route lists the whole list. The PUT is the same on both.
- Candidate texts are exported pseudonymised.
- Not covered: the disk-heavy `cs-desktop` crate, which needs GTK, is not built here.

## Open items

- Pseudonymisation does not reach free text typed by the researcher (memos, notes); the interface says so.
- The wording of the declaration sentence of the review state is to be read by the researcher.
