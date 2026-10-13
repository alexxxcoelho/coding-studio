# DEVGRAPH V1 lot 5 (13.10.2026): Analyse and Write, server side

Branch `worktree-agent-a4e0506ef70961e55`. Routes and shapes: `docs/API.md`, "V1 lot 5 and gaps". Contracts: `docs/CONTRACTS.md`, section 11.

## Done

- Schema v6, a separate idempotent migration (`cs-store/src/analysis.sql`, `analysis_schema.rs`), so that a parallel bump 4 to 5 merges without touching it. Append-only tables guarded by triggers: `theme_history`, `writing_versions`, `final_audits`, `final_audit_verdicts`, `code_memo_history`. Logical deletion for themes, links and matrix views.
- Themes (sheet, codes, research questions, checks, status with override and reason, order, history, passages).
- Matrix over interviews, cases, attribute values, themes, codes, focused codes and custom dimensions; closed values; computed counts; hand-attached passages; saved views; filters by attribute.
- Typed links, write-up parts, RQ syntheses, framework reading, Markdown export with footnotes.
- Final audit: seeded draw of 3 claims, full chain, gaps, verdicts log.
- Decisions in the words of the guide (outcome adopt, defer, reject).
- Nine gaps (see API.md). Gap 9: the legacy phase 1 category set is still used by the simple view, the config guard and `/api/codebook/phase1`, so it is only documented as deprecated.
- Exports and state parts, README lines, manifest digests.

## Choices

- Routes live in `cs-app/src/v1_analysis.rs`, merged by one line; client shapes (`analysis-api.ts`) adopted through adapters (`theme_json`, `matrix_json`, `view_json`, `decision_json`, `client_config`).
- `focused_codes` = codes with sub-codes, else all codes.
- Audit eligibility prefers linked parts and tops up with written, unlinked ones.
- Passage origin list (`lived_experience`, `project`, `opinion`, `document`) is an own reading of the guide.
- Legacy header/footer migration copies only customised text into empty fields.
- Code ids are never reused, including after an undo.

## Open items

- Remove the phase 1 category set once the three dependants are migrated.
- Decisions still require an interview.
- Origin list and framework cell shape to confirm with the guide author.
- `README` wording of the exports to be reviewed by the researcher.
