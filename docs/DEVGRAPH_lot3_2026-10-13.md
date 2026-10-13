# Dev graph, lot 3 (13.10): end to end, providers, generic configuration, publication

Source: plan v5 (`06_Writing/generated/2026-10-13_plan_correction_v5_chantiers_paralleles.md`, chantier B) and `docs/CONTRACTS.md`. Pipeline: Sonnet writes the red tests, Haiku implements, the orchestrator reviews, runs `tools/check.sh` and commits. Branch `main`, pushed to GitHub.

## Tasks

| id | task | scope | owner | status |
|---|---|---|---|---|
| B1 | End-to-end test of the v4 procedure through the API (527 checks) | cs-app tests/e2e.rs | Sonnet (test), green at first run | green |
| B2a | `ModelProvider` trait, isolated CLI provider, OpenAI-compatible provider, registry with copy_paste first (22 tests) | cs-ai providers/ | Sonnet tests, Haiku impl | green |
| B2b | ChatGPT OAuth port from munrel-connectors: PKCE, dynamic registration, JWKS, refresh, token stores, loopback (20 tests) | cs-ai providers/oauth_flow.rs | Sonnet | green |
| B3 | `StudyConfig`: frameworks, phase 1 categories, id prefixes, interviewers, segmentation limit, prompt template; codebook import CSV and JSON; `GET/PUT /api/config`; defaults byte-identical to the thesis behaviour (72 tests) | cs-model, cs-ingest, cs-store, cs-ai template, cs-app | Sonnet tests, Haiku impl | green |
| B3b | Providers registry routes, isolation probe, run of a batch through a provider (all parts validated before any write), OAuth begin/status, settings page, "Run with a provider" control (13 Rust tests, 69 web tests) | cs-app providers.rs, web settings | Sonnet tests, Haiku impl | green |
| B4 | Generic README, MIT licence, CONTRIBUTING, fictional example library, CI on three OS, publishing checklist | root, examples/, .github/ | Haiku | green |

## State after lot 3

Rust: cs-store 98, cs-app 42 (api 11, config_api 17, e2e 1, providers_api 13), cs-ai 75 (26 unit, 22 providers, 20 oauth, 7 template), cs-ingest 32, cs-model 26; clippy 0. Web: 136 tests, typecheck and lint clean, `web-dist` rebuilt and identical to `web/dist`.

## Choices to keep in mind

- `AiRunMeta.provider` is optional and skipped when absent, so existing run records serialise unchanged; `ai_runs` gains a `provider` column added in place on older databases.
- The CLI provider lists `model` among the parameters it cannot set; the OpenAI-compatible and OAuth providers set it from the configuration or per call.
- `POST /api/library/scan` now returns only the interviews found by that scan; `GET /api/interviews` still lists everything in the database.
- Codebook import never deactivates or deletes codes absent from the file; they are reported under `absent`.
- Default interviewer labels in `StudyConfig::default()` are still the author's names (kept for byte-identical behaviour); the owner should choose a generic default before publication (`docs/PUBLISHING.md`).
- Unix-only provider tests are gated with `#![cfg(unix)]`.

## Open for the owner

- Manual pass in the web UI on the example library, then the pilot on two or three real interviews with full human time measured.
- Decide the generic default interviewers and the retention of the thesis-specific docs (`DECISION_DRAFT.md`, `DESIGN_V3.md`, dev graphs) before the public release; see `docs/PUBLISHING.md`.
- Verify `examples/providers.example.json` against `PUT /api/providers` in the UI.
