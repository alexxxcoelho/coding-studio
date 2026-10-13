# Coding Studio: product specification

Version 1.0, 13 October 2026. This document is the single reference for what Coding Studio must do, what it already does, and in which order the rest is built. It supersedes `docs/DESIGN_V3.md` and `docs/DECISION_DRAFT.md`. It merges three inputs: an external requirements document for a lightweight qualitative analysis tool, an operational guide from interviews to conclusions, and the plain-language specification of the author's own process (revision 3, which wins wherever the inputs conflict). Statuses were checked against the code on the date above. `docs/CONTRACTS.md` and `docs/API.md` describe the engine and its routes; `docs/METHOD.md` describes the procedure as implemented for the first study.

## 1. Purpose, users, principles

Coding Studio is a local application for the qualitative analysis of semi-structured interviews. It takes a researcher from a set of transcripts (typically 10 to 30) to a defensible set of themes and written answers, where every statement can be followed back to an exact passage of an interview. It is the lightweight counterpart of a commercial qualitative data analysis package: it keeps the few functions that a thematic analysis of interviews needs, and leaves out the rest.

Two things set it apart from such packages.

1. An optional blind AI second coder. The model receives the transcript turns and the codebook, never the human coding. The human coding is frozen before the prompt exists, the model's answer is stored byte for byte, disagreements are confronted, and the researcher arbitrates with a written reason. Every step is written to a tamper-evident audit log.
2. A lexical workbench that uses no generative AI: highlights, keyphrases (YAKE), stems (Snowball), similar passages (TF-IDF), and a keyword table across highlights. It helps the researcher build codes from the data. It never proposes themes and never draws a word cloud.

Users. The primary user is one researcher on one machine. Later versions serve small teams (a second coder, a methodological reviewer). The tool does not transcribe audio and does not need a network. A model is called only when the researcher configures a provider and starts a run.

Principles.

- The researcher is the author of the interpretation. The tool records, retrieves and compares; it does not conclude.
- The evidence stays in the data. A theme, a matrix cell or a sentence of the final text links to passages, and a passage links to its interview, its speaker turn and its exact position.
- A code describes content. A theme is an analytical proposition built from several codes, after coding. A theoretical framework reads themes; it does not shape the codebook.
- Nothing is silently overwritten. Corrections, merges, decisions and revisions are appended, dated and justified.
- Counts are for exploring coverage, not for estimating prevalence. The tool always separates the number of passages from the number of interviews, and it never reads "not mentioned" as "absent in reality".
- One flow, no modes. The same four steps serve every study; what differs between studies is configuration.
- Four states are kept apart in the documents and in the exports: designed, implemented, tested, established by the data.

## 2. Workflow overview

The workflow has four steps in one single flow, preceded by project preparation. Steps 1 and 2 are done interview by interview; steps 3 and 4 work across the corpus. Going back to earlier steps is normal.

**Step 0, Prepare the project.** The researcher names the study, enters the research questions, chooses or edits the configuration (attributes, memo templates, checklist, framework, matrix defaults, link types, prompt), imports the transcripts, and fills the attribute values of each interview. A project template can be imported to do most of this in one gesture.

**Step 1, Code.** For each interview the researcher first reads it and answers the familiarisation memo (the template selected for that interview). Then every respondent turn is examined. A turn that carries nothing useful is marked not relevant; every other turn is relevant by default. The researcher codes whole turns, or selects precise passages inside a turn, with one or several codes. Codes are created on the fly with a plain-words name and a one-sentence definition, and live in a tree. A turn that hesitates gets the LATER code and is revisited. Highlights mark passages that matter before any code exists, and the lexical panel proposes names and finds similar passages. Interviewer turns are shown for context, are never coded and are never sent to a model. At the end of the interview the researcher goes through the end-of-interview checklist.

**Step 2, Review (optional).** Two parts. The first is harmonisation without AI: after a merge, a redefinition or a late new code, the tool lists the earlier turns that carry the affected codes, and the researcher rereads them. The second is the blind AI review: generate the prompt (which freezes the human coding), obtain the answer by copy and paste or through a configured provider, then confront and arbitrate. Concepts that the model reports as missing from the codebook arrive as candidates, never applied automatically. The step is never a lock. The documents of the study state on which interviews it was done.

**Step 3, Analyse.** Across interviews. Codes are grouped into themes, each stated as an analytical proposition with a theme sheet and a status (candidate, revised, retained), and checked against a list of theme checks. A configurable matrix compares interviews, cases, attributes, themes and codes. The researcher explores with keyword in context, passages by code, similar passages and codes by attribute. Documented decisions (cases) complete the picture. If the project has an interpretive framework, it is applied here to themes, and only here.

**Step 4, Write.** Organised by theme, then by research question. For each theme the tool shows the theme sheet and its linked passages; the researcher writes the structured parts of the result. Each passage is linked to a part with a typed link (supports, nuances, conditions, mechanism, exception, contradicts) and a one-line note. A synthesis per research question and, if the project has a framework, a reading table follow. A final audit draws random claims and shows the full path from the sentence to the source.

**Export and audit.** At any point the researcher exports a package per interview and a cross-corpus package, each with a manifest of SHA-256 digests, and verifies the audit chain.

## 3. Configuration model

Coding Studio contains no study. Everything specific to a study is project configuration, stored in the project database, editable in Settings, written to the audit log when it changes, and exportable as a JSON project template. A thesis template is one template among others. The repository ships a fictional example in `examples/`; a second fictional template with different questions is part of the acceptance tests.

### 3.1 What a project defines

| Element | Content | Notes |
|---|---|---|
| Study | Name, description, research questions as a free list with stable ids (for example RQ1, SQ1) | The description and questions feed the AI prompt and the Write step. Questions are entered by the researcher; they are never generated from codes. |
| Interview attributes | A list of fields: name, type (text, choice list, number, date), and for a choice list its values | Examples: sector, role, position in a chain, size, guide version, statement status (lived deployment, project, opinion, indirect observation). Values are filled once per interview and are filters everywhere. |
| Familiarisation memo templates | One or more templates, each a list of questions, with a version; one is the default | An interview selects a template in its sheet and may add, remove or reword questions for itself only. Answers are stored per interview together with the template version and the interview's own question edits. A generic default asks what the respondent tells, what surprised the researcher, which contradictions appear, and what to ask in the next interviews. |
| End-of-interview checklist | An editable list of items | Default items cover: relevant turns examined, existing codes reused, doubtful passages flagged, contradictions noted, memo written. |
| Interpretive framework | None by default; or a list of dimensions | Presets: TOE, TAM and UTAUT, custom. A dimension has a name and a definition. Used only in Analyse and Write, never in the coding screen, never in the codebook. |
| Matrix defaults | Which rows and which columns to start from | Rows and columns can each be: interviews, cases, any interview attribute, themes, codes, or a custom dimension list defined by the project (for example "who imposes", "who pays"). |
| Theme checks | An editable list; default of seven | Default: coherence of the passages, distinction from other themes, a title that asserts, centrality for at least one research question, several observations or one justified single case, data that nuance it, passages that support exactly the extent of the claim. |
| Evidence natures | An editable list with presets | Natures describe, they do not rank. Default preset: first-hand account, reported statement, figure estimate, quoted document, opinion. Origin and role lists are configurable in the same way. |
| Link types | An editable list | Default: supports, nuances, conditions, mechanism, exception, contradicts. |
| Segmentation and ingestion | Identifier prefixes, interviewer speaker labels, longest unit in words | Already configurable. |
| AI prompt | A template with placeholders filled from the configuration, plus a pseudonymisation option | See 3.3. |
| Providers | The list of model providers and what each sends | Already configurable. Not part of a shared template by default, because it names machines and key variables. |

### 3.2 Matrix cells

A matrix cell has a value from a fixed closed set, a passage count, and an optional one-line summary. The values are: present, not mentioned, contradicted, uncertain. The set is deliberately not configurable, so that matrices from different projects read the same way. The interface states that "not mentioned" does not mean "absent in reality". A cell opens the passages that justify it. A row that is not yet coded shows "not yet coded", not "not mentioned".

### 3.3 Prompt from configuration

The prompt is built from a template with placeholders. The template supplies the fixed parts (role, task, rules, closing). The tool supplies, from the configuration and the data: the study description, the research questions, the number of codes, the codebook as a tree with definitions and examples, the attribute values that the project marks as "shared with the model" (for example sector and role), the units, and for each respondent unit the interviewer turn that precedes it. The answer schema and the code and unit lines are produced by the tool and cannot be removed by the template. The human coding is never in the prompt. A pseudonymisation option replaces person and organisation names, from an editable list, before any text leaves the machine; the mapping stays local.

### 3.4 Project template (JSON)

A project template is one JSON file with a version number, holding: study description and research questions, attribute definitions, memo templates, checklist, framework, matrix defaults, theme checks, evidence natures, link types, segmentation settings, the prompt template, and optionally a starter codebook. It holds no interview text, no coding and no provider secret. Import is all or nothing, reports every problem at once, and is audited; export never overwrites an earlier file. Importing a template into a project that already has data is allowed only for the parts that do not invalidate existing data (for example it may add attributes or memo templates, but it will not remove an attribute that has values).

## 4. Functional requirements

Reading the tables. Status is **done** (works, checked in the code or its tests), **partial** (some of it exists, the gap is named) or **missing**. Status describes the engine and the HTTP API (`crates/`) and the web interface (`web/src`), as they stand on `main` at the date above. A row whose status changed carries an evidence note (file or route). A row that is still partial or missing says what is missing. Target is **V1** (needed for the author's thesis, in the four-step flow, before 30 October 2026) or **V2** (after the thesis, for the public release). Where a requirement was adapted from the external requirements document, it is noted as "(ext.)".

**Status counts**, over the 124 requirements of sections 4.1 to 4.13. "Before" is the table as it stood on `main` before this update; the notes of the changed rows give the evidence.

| Status | Before | After |
|---|---|---|
| done | 37 | 90 |
| partial | 28 | 20 |
| missing | 59 | 14 |
| Total | 124 | 124 |

### 4.1 Project and configuration

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-01 | Study name, description and research questions with stable ids, editable. Evidence: study name and description fields in `web/src/components/settings/study-config.tsx`; research questions on the Settings page. | done | V1 |
| F-02 | Project template export and import as JSON, all or nothing, audited, as in 3.4. Evidence: routes `/api/project/template` and `/api/project/template/import`; `web/src/components/settings/project-template.tsx`; tests `crates/cs-app/tests/template_api.rs`. | done | V1 |
| F-03 | Interview attribute definitions: name, type (text, choice list, number, date), values of a choice list, order. Evidence: `ProjectConfig.attributes` in `crates/cs-model/src/project.rs`; route `/api/project-config`; editor `web/src/components/settings/project-lists.tsx`. | done | V1 |
| F-04 | Attribute values per interview, entered in the library and the interview sheet, usable as filter and as matrix axis. Evidence: values edited in `web/src/components/library/fiche.tsx`; filters in `/api/lexicon/kwic` and the codes-by-attribute table in `web/src/components/analysis/explorer.tsx`. | done | V1 |
| F-05 | Interpretive framework: none, TOE, TAM and UTAUT, or custom list of dimensions, applied to themes in Analyse and Write only. Evidence: `interpretive_framework` in `ProjectConfig`; route `/api/writing/framework`; `web/src/components/writing/framework-table.tsx` and `web/src/components/analysis/framework-reading.tsx`. The framework stays out of the coding screen and the prompt. | done | V1 |
| F-06 | Familiarisation memo templates: several, versioned, one default; per-interview selection and per-interview edits of questions; answers stored with the template version. Evidence: memo templates with versions and `memo_history` in `crates/cs-model/src/project.rs`; per-interview answers in `web/src/components/library/fiche.tsx`; editor in `project-lists.tsx`. | done | V1 |
| F-07 | Editable end-of-interview checklist, with the state of each item stored per interview. Evidence: checklist in `ProjectConfig`; per-interview state in `crates/cs-store/src/project.rs`; `web/src/components/coder/end-panel.tsx`. | done | V1 |
| F-08 | Editable list of theme checks (default of seven). Evidence: `theme_checks` in `ProjectConfig`; route `/api/themes/{id}/checks/{check}`; `web/src/lib/theme-status.ts` asks for a reason to retain a theme with a check not done. | done | V1 |
| F-09 | Editable evidence natures, origins and roles with presets. Closed lists exist (evidence levels EV0 to EV4, four provenances, five roles) but are hard-coded and differ from the default natures of 3.1. Evidence: evidence natures are editable (`ProjectConfig.evidence_natures`). Missing: the origin (provenance) and role lists are not in `ProjectConfig`; they stay hard-coded (four provenances, five roles), as do the evidence levels EV0 to EV4. | partial | V1 |
| F-10 | Editable link types (default of six). Evidence: `ProjectConfig.link_types`; editor in `project-lists.tsx`; typed links through `/api/links`. | done | V1 |
| F-11 | Prompt template with placeholders filled from the configuration (3.3): study description, research questions, codebook tree, attributes marked as shared with the model, and the preceding interviewer turn. Evidence: `crates/cs-ai/src/prompt.rs` (study description, research questions, interviewer questions block); shared attributes read in `crates/cs-app/src/v1_review.rs`. | done | V1 |
| F-12 | Pseudonymisation of person and organisation names before any text leaves the machine, with an editable list and a local mapping. Evidence: `crates/cs-model/src/pseudonym.rs`; routes `/api/pseudonymisation` and `/api/interviews/{id}/pseudonymisation` (`crates/cs-app/src/v1_review.rs`); UI in `web/src/components/revoir/revue-ia.tsx`. Limit: free text typed by the researcher is not masked. | done | V1 |
| F-13 | Identifier prefixes, interviewer speaker labels and longest unit length, configurable and validated. | done | V1 |
| F-14 | Every configuration change is written to the audit log with before and after values. | done | V1 |
| F-15 | Neutral defaults for a public release: the default interviewer labels still contain the author's names, and a few messages and fixtures mention the author's environment. Missing: the default interviewer labels (`DEFAULT_INTERVIEWERS` in `crates/cs-model/src/config.rs`) still carry the author's names. | partial | V2 |
| F-16 | Settings screen covering study, attributes, memo templates, checklist, framework, matrix defaults, lists, prompt, pseudonymisation and providers. A settings page exists for study, codebook import and providers only. Missing: matrix defaults have no editor; the pseudonymisation list is edited on the Review screen and the framework in Analyse, not in Settings (`web/src/pages/settings.tsx`). | partial | V1 |

### 4.2 Corpus and library

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-17 | Library scan of a folder with one sub-folder per interview, inventory of every source file with its SHA-256, detection of the primary transcript. | done | V1 |
| F-18 | Transcript formats: platform VTT, raw speech-recognition text with timestamps, cleaned Markdown with speaker turns, Budgie Echo Markdown, written responses. | done | V1 |
| F-19 | Further formats: plain TXT and Markdown without a convention, DOCX, text PDF, SRT. (ext.) Evidence: DOCX, text PDF and SRT readers in `crates/cs-ingest/src/formats.rs`, dispatched by `scan.rs`. Missing: plain TXT and Markdown without a convention are not recognised (a `.txt` file is read only when its name contains "transcript", a `.md` file only when its name is a known one). | partial | V2 |
| F-20 | Deterministic segmentation into speaker turns, long turns split at sentence ends above the configured length; unit text unchanged except redundant spaces. | done | V1 |
| F-21 | Interviewer turns recognised by configured labels, shown for context, excluded from coding, from prompts, from review and from verification targets. | done | V1 |
| F-22 | Re-segmentation keeps codings by unit id and is refused only when an AI answer would be invalidated. Source files are never modified. | done | V1 |
| F-23 | Library view with one line per interview and three counters (units coded, complete AI runs, decisions taken), the attribute values, and the state of the interview in the four steps. Missing: the three counters (units coded, complete AI runs, decisions taken) are not on the library line of `web/src/pages/library.tsx`. The route `/api/library/progress` exists. | partial | V1 |
| F-24 | Immutable transcript versions: a correction creates a new version, and annotations of the previous version are projected to the new one by text and context matching, with conflicts marked "needs review". (ext.) Today corrections are overlays on units and never move codes. Missing: versioned transcripts and the projection of passages onto a new version. | partial | V2 |

### 4.3 Coding (step 1)

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-25 | Relevance by default: every respondent turn is relevant, N marks it not relevant, R marks it "to review", and the three states plus "not yet examined" are counted. Evidence: route `PUT /api/interviews/{id}/units/{unit}/mark`; R key and mark in `web/src/components/coder/turn.tsx`; counts by state in `web/src/lib/coder-model.ts`. | done | V1 |
| F-26 | Code a whole turn with one or several codes, the default gesture. | done | V1 |
| F-27 | Passages: select a span inside a turn and code it, with several spans per turn, overlapping spans, and several codes per span. A passage stores interview, unit, start and end offsets, the exact text, and the version of the segmentation it belongs to. Nothing of this exists; the unit is the finest grain. Evidence: `crates/cs-store/src/passages.rs`; routes `/api/passages/{pid}/codings`; selection menu `web/src/components/coder/selection-menu.tsx`. | done | V1 |
| F-28 | Anchors are versioned and verified: a passage is rejected if its offsets do not give the expected text, offsets use one tested convention (Unicode scalar positions on the server, converted from the browser's UTF-16), and tests cover accents, apostrophes, emoji and line breaks. (ext.) Evidence: `crates/cs-model/src/passage.rs` (scalar and UTF-16 offsets, anchor digest, refusal of a split surrogate pair); stale-anchor check in `crates/cs-store/src/passages.rs`. | done | V1 |
| F-29 | Code identity: a stable internal id, a free-text name in plain words, a definition, and aliases (former names and synonyms). Evidence: stable `code_id`, name, aliases and parent in `crates/cs-store/src/codetree.rs`; routes `/api/codes/{id}/aliases`. Aliases do not feed the code picker yet (see F-41). | done | V1 |
| F-30 | Code tree: parent and child codes by drag and drop, with moving a code under a parent never changing any coding, and a passage coded with a child counting for the parent in counts and exports. Not in the engine. Evidence: route `/api/codes/{id}/move` (`crates/cs-store/src/codetree.rs`); tree helpers in `web/src/lib/code-tree.ts`. | done | V1 |
| F-31 | Code definition with includes, excludes, examples, neighbouring codes, origin, history and a memo per code. Evidence: `includes` and `excludes` in `crates/cs-model/src/passage.rs`. Missing: neighbouring codes have no field. | partial | V1 |
| F-32 | Create a code on the fly from a turn, a passage or a highlight: name, definition, optional parent; the creating passage becomes the origin and the example; the codebook version advances. Evidence: route `/api/codes/from-passage`; `web/src/components/coder/new-code-dialog.tsx`; action in `web/src/components/coder/selection-menu.tsx`. | done | V1 |
| F-33 | The LATER code and its worklist: a system code that parks a hesitation with an optional note, listed until resolved. The "to clarify" labels (label, note, resolution to a new or an existing code, applied to every unit with the same label) cover the same need at unit level. Evidence: LATER as a virtual code, routes `/api/later` and `/api/interviews/{id}/later`; unit labels in `web/src/pages/to-clarify.tsx`. | done | V1 |
| F-34 | Codebook import from CSV and JSON, and from a legacy database opened read only; all or nothing, with a report of added, updated and absent codes; absent codes are never deleted. | done | V1 |
| F-35 | Codebook export to CSV and SQL, timestamped, never overwritten. | done | V1 |
| F-36 | Merge a code into another: every coding and decision is rewritten, the source is deactivated and keeps a pointer, the change is audited with a rationale. Undo of a merge is not available. | done | V1 |
| F-37 | Deactivate and reactivate a code without touching past codings. | done | V1 |
| F-38 | Harmonisation worklist: after a merge, a redefinition or a late new code, list the earlier turns that carry the affected codes or LATER, for targeted rereading. Evidence: route `/api/harmonisation`; table `harmonisation_items` in `crates/cs-store/src/lot4.rs`. | done | V1 |
| F-39 | Split a code into two or more, with a worklist to reassign its passages, and undo of merge and split from the journal. (ext.) Missing: the whole requirement: split of a code, the reassignment worklist, and undo of a merge or a split. | missing | V2 |
| F-40 | Codebook version number for the whole codebook (displayed, stored with exports and prompts), in addition to the per-code counter. Prompts already store the codebook digest. Evidence: table `codebook_revisions` and route `/api/codebook/revisions`. | done | V1 |
| F-41 | Code autocomplete by name, alias and definition; keys 1 to 9 for the first nine codes and a keyboard search for the rest. A code picker with a filter exists; aliases do not. Missing: the code picker does not match aliases (`web/src/components/code-picker.tsx`). Keys 1 to 9 and keyboard search exist (`web/src/lib/coder-keys.ts`). | partial | V1 |
| F-42 | Highlights: select a passage and mark it without a code (key H); highlights stay visible after coding and are candidates for citation. Evidence: route `/api/interviews/{id}/highlights`; `web/src/pages/highlights.tsx`. | done | V1 |
| F-43 | Coding stripes: a margin column showing the codes on each turn, optional. Evidence: coding stripes in `web/src/components/coder/turn.tsx`. | done | V1 |
| F-44 | Passages of a code: all turns and passages carrying the code, with the interview, the speaker, the previous and next turn, the other codes present, and a link to the source. Evidence: `crates/cs-store/src/codetree.rs` returns speaker, previous and next turn and the other codes; route `/api/codes/{id}/passages`. | done | V1 |
| F-45 | Counts that never mislead: for each code, the number of passages, of distinct interviews and, when cases or attributes exist, of distinct cases and attribute values. (ext.) Missing: counts of distinct cases and distinct attribute values per code. Passage and interview counts exist in the theme export (`crates/cs-app/src/export_analysis.rs`). | partial | V1 |
| F-46 | Interview reflexive memo and unit-level memos, dated and attributed. Evidence: interview memo and dated answers at `/api/interviews/{id}/memo` and `/api/interviews/{id}/memo/answers`; unit notes and marks at `/api/interviews/{id}/marks`. | done | V1 |
| F-47 | Keyboard operation of the coding screen: next and previous turn, N, R, H, slash for code search, Enter to confirm, Escape to close. Evidence: key map in `web/src/lib/coder-keys.ts` (N, R, H, search, next and previous, Enter, Escape, undo, new code). | done | V1 |
| F-48 | Familiarisation memo at the start of each interview, from the template selected for it (F-06), required before coding the first three interviews and optional afterwards. Evidence: memo template choice and answers per interview in `web/src/components/library/fiche.tsx`. Missing: the rule that the memo is required before coding the first three interviews is not enforced in `web/src/pages/coder.tsx`. | partial | V1 |
| F-49 | End-of-interview checklist shown when the last turn is reached (F-07). Evidence: `web/src/components/coder/end-panel.tsx`, shown when every respondent turn is examined; state kept in the interview checklist. | done | V1 |

### 4.4 Lexical workbench

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-50 | Lexical panel for a selected highlight: YAKE keyphrases, Snowball stems, and a proposed provisional label that the researcher may use or ignore. Statistical and deterministic. Nothing exists. Evidence: route `/api/passages/{pid}/lexical`; `crates/cs-lex` (YAKE, Snowball); `web/src/components/lexicon/passage-panel.tsx`. | done | V1 |
| F-51 | Languages: stemming and stop words for French, English and German; the language is an attribute of the interview or of the project. Evidence: `crates/cs-lex/src/lang.rs` (French, English, German); configuration route `/api/lexicon/config`. | done | V1 |
| F-52 | Keyword table across all highlights: stem, number of passages, interviews concerned, with check boxes to show the passages and to create a code from a selection. A table with counts, never a word cloud. Evidence: route `/api/lexicon/keywords`; `web/src/components/lexicon/keyword-table.tsx`. | done | V1 |
| F-53 | Similar passages: TF-IDF cosine similarity on stems across all interviews, with the score; used to check the coverage of a code and to find what was missed. It does not see synonyms or other languages, and says so. Evidence: route `/api/passages/{pid}/similar`; TF-IDF cosine in `crates/cs-lex`. | done | V1 |
| F-54 | Keyword in context (KWIC) search across all interviews, filterable by attribute. Evidence: route `/api/lexicon/kwic` with attribute filter; `web/src/components/lexicon/kwic-search.tsx`. | done | V1 |
| F-55 | Full-text search with filters on attributes, codes and speakers, including interviews that do not mention a subject. (ext.) Missing: the whole requirement. No full-text search route or screen exists; only KWIC (F-54). | missing | V2 |
| F-56 | Code co-occurrence in the same passage or within a configured proximity, documented as co-occurrence and never as causation. (ext.) Missing: the whole requirement. No co-occurrence computation exists. | missing | V2 |

### 4.5 Evidence qualification and verification

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-57 | Detail drawer for a unit that will be cited: closed by default, opened only for cited passages. Evidence: Detail tab in `web/src/components/coder/detail-panel.tsx` (passage, codes, note, copy as quotation). Missing: it is a tab, not a drawer that stays closed and opens only for cited passages. | partial | V1 |
| F-58 | Quotation as an exact substring of the unit, with evidence nature, origin and role, and the codes it supports. Claims with these fields, exact-substring validation and distributions exist. | done | V1 |
| F-59 | Flags on a unit: uncertain attribution or word, reported statement, unfinished sentence, acquiescence to the interviewer's wording (with a "content or only assent" kind). Optional. | done | V1 |
| F-60 | Corrections of text or speaker as dated overlays with a reason; the original stays readable; the unit returns to unverified. | done | V1 |
| F-61 | Verification states (unverified, verified, unusable), the list of targets (cited, flagged, disagreed, attributed to a case, plus a seeded sample), and coverage figures exported next to any count. | done | V1 |
| F-62 | Audio playback at the timestamp of a unit, and audio synchronised with the transcript. (ext.) Missing: the whole requirement. No audio playback or timestamp synchronisation exists in `web/src`. | missing | V2 |

### 4.6 Blind AI review (step 2)

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-63 | Blind guard: a prompt batch is refused while any unit of it lacks the human coding; generating it freezes the human coding with the digests of the units and of the codebook, append-only. | done | V1 |
| F-64 | Deterministic prompts: identical inputs give byte-identical prompts, bound to the segmentation and codebook by hashes; unit identifiers found in code texts are redacted. | done | V1 |
| F-65 | Prompt content from configuration (F-11), with attributes shared with the model and the preceding interviewer turn. Evidence: `crates/cs-ai/src/prompt.rs` (description, research questions, interviewer questions); shared attributes in `crates/cs-app/src/v1_review.rs`. | done | V1 |
| F-66 | Model providers behind one interface: copy and paste (always first, the reference), isolated command-line, OpenAI-compatible HTTP, ChatGPT sign-in. Each run records model, settable and unsettable parameters, place of execution (a local command may call a hosted model), prompt digest and raw answer. Keys are named by environment variable and never stored. Isolation probe for command-line providers. | done | V1 |
| F-67 | Strict answer validation: right interview and segmentation, every unit exactly once, only offered codes or NA; all problems reported together; a refused answer is kept nowhere; accepted answers are stored verbatim with a digest. | done | V1 |
| F-68 | Candidate concepts: the model may report a concept absent from the codebook; the report arrives in Review as a candidate to create a code, attach to an existing code, or discard, with a dated reason. Never applied automatically. Not in the prompt or the parser. Evidence: tables `concept_candidates` and `concept_decisions` (`crates/cs-store/src/lot4.rs`); `web/src/components/revoir/concept-card.tsx`. | done | V1 |
| F-69 | Confrontation on the frozen reference: units where the human and the model differ, a seeded random sample of agreements (at least five or a tenth), and revisions shown as such. | done | V1 |
| F-70 | Arbitration: keep mine, take the model's, or choose other codes; a written reason is required for a disagreement; the frozen reading, the model's reading and the decision are three separate records. | done | V1 |
| F-71 | Agreement figures (exact agreement, Cohen's kappa per code), reported and not optimised. | done | V1 |
| F-72 | Review state per interview and per corpus (not done, in progress, complete) with the declared coverage, so that the final text can say on which interviews the review was done. Evidence: route `/api/review-state`; review screen `web/src/components/revoir/revue-ia.tsx`. | done | V1 |
| F-73 | Review remains optional and is never a lock on Analyse or Write. | done | V1 |
| F-74 | Second human coder and reviewer role with intercoder comparison. (ext.) Missing: the whole requirement. No second coder account, reviewer role or intercoder comparison exists. | missing | V2 |

### 4.7 Analyse (step 3)

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-75 | Themes as analytical propositions: title, a proposition of two to four sentences, linked codes (a code may serve two themes), research questions concerned. Evidence: routes `/api/themes` (`crates/cs-app/src/v1_analysis.rs`); `web/src/components/analysis/theme-list.tsx`. | done | V1 |
| F-76 | Theme sheet: supporting codes, supporting cases, main passages, divergent cases, conditions, alternative interpretation, limits, decision memo, status (candidate, revised, retained). Evidence: `web/src/components/analysis/theme-sheet.tsx`; theme routes in `crates/cs-app/src/v1_analysis.rs`. | done | V1 |
| F-77 | Theme checks: the configured list is shown on the sheet and each check is answered before a theme can be set to retained. Evidence: route `/api/themes/{id}/checks/{check}`; `web/src/lib/theme-status.ts` asks for a reason to retain a theme with a check not done (a recorded override, not a block). | done | V1 |
| F-78 | Framework reading per theme: for each dimension of the project's framework, what the theme explains, plus "outside the framework". Absent when the project has no framework. Evidence: route `/api/writing/framework`; `web/src/components/analysis/framework-reading.tsx`. | done | V1 |
| F-79 | Matrix definition: rows and columns chosen among interviews, cases, attributes, themes, codes and custom dimension lists; saved as named views; filters by attribute. Evidence: routes `/api/matrix/views` and `/api/matrix/compute`; `web/src/components/analysis/matrix-view.tsx`. | done | V1 |
| F-80 | Matrix cells: closed value (3.2), passage count, optional one-line summary, a click that opens the passages, citations and memos. Evidence: routes `/api/matrix/cell` and `/api/matrix/cell/passages`. | done | V1 |
| F-81 | Custom dimension lists (for example who imposes, who pays) with their own cell values and passages. Evidence: custom dimensions in `ProjectConfig.matrix_dimensions`; route `/api/matrix/options`. | done | V1 |
| F-82 | Codes by attribute table: number of passages and interviews per code and attribute value. Evidence: `CodesByAttribute` in `web/src/components/analysis/explorer.tsx`. | done | V1 |
| F-83 | Passages by code, and cross-interview exploration by KWIC and similar passages (F-53, F-54). Evidence: `web/src/components/analysis/explorer.tsx` (passages by code, KWIC, similar passages). | done | V1 |
| F-84 | Cases (documented decisions): a case is documented only by units explicitly attributed to it, from any interview, with descriptive fields and five synthesis fields (trigger, alternatives, constraints, choice, result), each with what supports, contradicts and is missing. A case without units is shown as undocumented. The field names are currently those of the first study. | done | V1 |
| F-85 | Cross-case view of research questions against cases, built from final codings. | done | V1 |
| F-86 | Saturation record: for each interview in coding order, codes used for the first time and field codes created, provisional by design. | done | V1 |
| F-87 | Generic case fields: the vocabulary of F-84 (adopted, deferred, rejected, granularity, carrier) becomes a configurable set of fields. Missing: the whole requirement. The case fields are still fixed to the first study. | missing | V2 |

### 4.8 Write (step 4)

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-88 | Writing by theme: for each retained theme, the sheet and its passages beside the written parts (analytical statement, mechanism, evidence, variations and divergent cases, mini-synthesis). Evidence: `web/src/components/writing/by-theme.tsx` and `part-editor.tsx`; route `/api/writing/themes/{id}/parts/{part}`. | done | V1 |
| F-89 | Typed links between a passage and a part of the text, chosen from the configured list, with a one-line note; a free link, not a binary for or against. Evidence: routes `/api/links` and `/api/links/{id}`; link types from `ProjectConfig.link_types`; `by-theme.tsx` creates the link. | done | V1 |
| F-90 | Passage picker for a research question: all final-coded passages carrying a code of the question, across interviews, with verified state and a link to open the unit in its interview. Missing: a passage picker for a research question. The Write screen lists passages by theme (`web/src/components/writing/by-theme.tsx`); no route or screen lists the final-coded passages of a question across interviews. | missing | V1 |
| F-91 | Synthesis per research question: memo with what the data document, interpretation, competing explanations and limits, each with units for and against. Evidence: table `rq_memos` and route `/api/research-questions/{id}/memo`; `web/src/components/writing/question-synthesis.tsx`. Missing: no link from each synthesis to the themes that answer its question. The framework reading is a separate table (F-92), not part of the synthesis. | partial | V1 |
| F-92 | Framework reading table: theme, observation, reading by framework dimension, complementary dimension, limit of evidence. Generated from F-78 and the links. Evidence: `web/src/components/writing/framework-table.tsx`; route `/api/writing/framework`. | done | V1 |
| F-93 | Final audit: draw a configurable number of random claims (default three) from the written text and show the whole path to sentence, theme, codes, passages, interview version, without manual reconstruction. Evidence: routes `/api/final-audit` and `/api/final-audit/{id}`; `web/src/components/writing/final-audit.tsx`; seeded draw of three claims in `crates/cs-store/src/final_audit.rs`. | done | V1 |
| F-94 | Export of the written parts with their links, so that figures and quotations of a document can be generated from the package. Evidence: route `/api/writing/export` (Markdown with footnotes); `crates/cs-app/src/export_analysis.rs`. | done | V1 |

### 4.9 Audit, export, portability

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-95 | Append-only audit log: each event carries the SHA-256 of the previous one; database triggers refuse update and delete of the log, prompts, frozen references, AI answers and arbitration records; each write and its event share one transaction. | done | V1 |
| F-96 | Chain verification from the interface and from `GET /api/audit/verify`. | done | V1 |
| F-97 | Interview export package: units, codings, prompts and raw answers, agreement and review, final coding, cases, claims and distributions, verification and corrections, audit log, state digest, database snapshot, manifest with SHA-256 and sizes, README. Never overwritten. | done | V1 |
| F-98 | Cross-corpus export package: answers, cross-case matrix, saturation, cases, state, manifest. | done | V1 |
| F-99 | Export of passages, highlights, themes and sheets, matrix views, links, memo answers, attribute values and checklist state, in the same package format (CSV and Markdown, plus JSON for ids). Evidence: `passages.csv`, `highlights.csv`, `attributes.csv`, `memos.json` (checklist included) in `crates/cs-app/src/export_v1.rs`; themes, matrix and links in `export_analysis.rs`. | done | V1 |
| F-100 | Counting unit stated in every distribution file header and in the package README. | done | V1 |
| F-101 | Whole-project export and re-import (a ZIP with a JSON manifest and the sources) that reproduces codes, passages, themes, memos, attributes and file digests. A database copy is already part of each package; import is not. (ext.) | partial | V2 |
| F-102 | Anonymised academic export that masks names of persons and organisations and sensitive attributes, with a manual check of free text. (ext.) Missing: the whole requirement. Only candidate texts are pseudonymised in exports; no anonymised academic export exists. | missing | V2 |
| F-103 | Import of codebooks and codings from NVivo and MAXQDA exports, and export in REFI-QDA project and codebook formats. Evidence: REFI-QDA codebook and project export and import in `crates/cs-qdpx` (routes `/api/export/qdc`, `/api/export/qdpx`, and the import routes in `crates/cs-app/src/qdpx.rs`). Missing: the tests use hand-written MAXQDA-shaped and NVivo-shaped fixtures; import is not checked against files exported by NVivo or MAXQDA themselves. | partial | V2 |
| F-104 | Export of the text and tables a final document needs (docx or Markdown) from themes, links and matrices. (ext.) Evidence: Markdown export with footnotes (`/api/writing/export`). Missing: no docx output. | partial | V2 |

### 4.10 Application, platforms, command line

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-105 | Local HTTP server on 127.0.0.1 with embedded web interface; one SQLite file with a rollback journal that is safe in a synchronised folder. | done | V1 |
| F-106 | Command-line launcher: database, library, legacy coding database, port, actor name, interviewer labels, no-browser flag. | done | V1 |
| F-107 | Desktop application (Tauri 2) with native folder and file dialogs and a Database menu; unsigned builds. | done | V1 |
| F-108 | Continuous integration on macOS, Linux and Windows. | done | V1 |
| F-109 | Command-line equivalents of the main actions (scan, segment, export, verify) for scripted use. Missing: the whole requirement. The `coding-studio` binary takes launcher options only (`crates/cs-app/src/main.rs`); there are no scan, segment, export or verify commands. | missing | V2 |
| F-110 | Signed and notarised bundles, installers for the three platforms, and a release process. Missing: the whole requirement. CI builds an unsigned macOS app (`.github/workflows/ci.yml`); there is no signing, notarisation, installer for the three platforms or release workflow. | missing | V2 |
| F-111 | Light and dark appearance following the system, with a manual override. | done | V1 |

### 4.11 In-app guide

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-112 | In-app guide, in context: for each step a short explanation of what to do and what not to do, the keys, and the method rules (describe before interpreting, repetition is not importance, framework is not structure). Written from the configuration, so it follows the project's own lists. Evidence: in-app guide `web/src/pages/guide.tsx`, help drawer `web/src/components/help-drawer.tsx`, text from `docs/guide/fr`. Missing: the text is static and is not written from the project configuration (`web/src/lib/guide.ts` does not read it). | partial | V1 |
| F-113 | Worked fictional example project that the guide walks through, loadable with one action. Evidence: `examples/` (one fictional interview, a codebook, a providers file, a project template). Missing: no single action in the interface loads the example as a project. | partial | V1 |
| F-114 | English translation of the user guide (written in French first, in docs/guide/fr) for the public release. Evidence: `docs/guide/en/` (eight files, same structure as `docs/guide/fr/`). | done | V2 |

The example library holds one fictional interview, a codebook and a providers file, which makes F-113 partial.

### 4.12 Interface (single flow)

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-115 | Single navigation: Library, Codebook, Analyse, Write, Settings; an Interview entry appears when one is open. No simple and advanced modes. Evidence: `web/src/components/sidebar.tsx` and `web/src/lib/nav.ts` (Library, Codebook, Analyser, Rediger, Settings; Lexique and Guide are extra entries). No simple or advanced mode switch. | done | V1 |
| F-116 | Interview screen with the transcript list, the code panel, resizable panels, progress counters, the four-step bar, and the Detail drawer. Evidence: interview tabs in `web/src/pages/interview.tsx`; coder in `web/src/pages/coder.tsx`; step capsule `StepsCapsule`; Detail tab. Missing: no resizable panels in `web/src`. | partial | V1 |
| F-117 | Review screens (generate prompt, paste or run, confront, arbitrate) rebuilt on the single flow. Evidence: `web/src/components/revoir/` (prompt, answers, decisions, concept cards); tabs `ai` and `review` in `web/src/pages/interview.tsx`. | done | V1 |
| F-118 | Analyse and Write screens for F-75 to F-94. Evidence: `web/src/pages/analyser.tsx`, `web/src/pages/rediger.tsx`, `web/src/components/analysis/`, `web/src/components/writing/`. | done | V1 |
| F-119 | Interface language selection (English, French) with translated strings. Missing: the whole requirement. The interface is French only, enforced by `web/src/i18n.test.tsx`, and has no language selection. | missing | V2 |
| F-120 | Keyboard accessibility and screen-reader labels for all actions, contrast checked in both appearances. Missing: no accessibility audit and no screen-reader or contrast check is recorded in the repository. | partial | V2 |
| F-121 | Three-panel layout (navigation, list, detail) with right-click actions and coding stripes. (ext.) Evidence: coding stripes in `web/src/components/coder/turn.tsx`. Missing: the three-panel layout and the right-click actions. | partial | V2 |

### 4.13 Teams (V2)

| ID | Requirement | Status | Target |
|---|---|---|---|
| F-122 | Several users on one project with roles (analyst, reviewer), authentication, and per-user attribution in the audit log. The actor name is already recorded. Missing: the whole requirement. The audit log records a single actor name (`--actor`); there are no users, roles or authentication. | missing | V2 |
| F-123 | Concurrent editing or synchronisation strategy for a shared project folder, with conflict reporting. Missing: the whole requirement. No synchronisation or conflict reporting exists for a shared folder. | missing | V2 |
| F-124 | Reviewer role: read the codebook, decisions and justifications and annotate without changing sources or codings. (ext.) Missing: the whole requirement. No reviewer role exists. | missing | V2 |

## 5. Non-functional requirements

Local-first. The application runs on the user's machine. The project is one SQLite file plus the folders the user chooses. No telemetry, no account, no network call unless a provider is configured and started by an explicit gesture. The server binds to the loopback address only.

Privacy. Interview text is personal data. The tool never sends text anywhere on its own. Each provider declares what it sends and where it executes, and the declaration is recorded with every run. Person and organisation names are pseudonymised before any send when the option is on (F-12). Interviewer turns, memos, human codings, flags, corrections and claims never enter a prompt. The mapping from pseudonyms to names, if kept, stays local and separate from exports.

Pseudonymisation. Names are replaced in the outgoing prompt only; the stored transcript is not altered. The replacement list is editable and reported with each run, so that a reader of the audit trail can see what was masked.

Audit. Every write is an event in a hash-chained log, in the same transaction as the change. The log, prompts, frozen references, AI answers and arbitration records cannot be updated or deleted at the database level. Exports carry a manifest of SHA-256 digests, and a package is never modified. Reproducibility is a requirement: segmentation and prompts are byte-identical for identical inputs.

Data integrity. Anchors of passages are versioned and verified (F-28). A segmentation cannot be redone in a way that invalidates an AI answer. Corrections never rewrite the source. A deleted object is deactivated, not destroyed, unless the user exports and starts a new project.

Platforms. macOS, Windows and Linux, as a desktop bundle and as a command-line server with a browser. Continuous integration runs on the three. The database must work in a synchronised folder.

Accessibility. All actions are reachable by keyboard. Interface colours meet contrast guidance in light and dark. Text size follows the system. Colour is never the only carrier of meaning (code stripes carry the code name on focus).

Performance, for a project of 30 interviews (about 3,000 turns, 600,000 words, 15,000 passages), on a laptop of ordinary specification:

- Opening an interview and moving between turns: under 100 ms per action.
- Coding a turn or passage and seeing it highlighted: under 100 ms.
- Code tree, passages of a code, matrix of 30 rows by 30 columns: under 500 ms.
- Keyword in context, keyword table and similar passages across the corpus: under 2 s; the lexical index is built at import and updated incrementally.
- Scanning and segmenting an interview: under 3 s.
- Export of an interview package: under 10 s; cross-corpus package: under 30 s.
- Long transcripts are virtualised in the interface.

Robustness. Unicode text is handled correctly everywhere, including accents, apostrophes, emoji and mixed line endings. A crash never leaves a half-written change. Backups are the user's copy of the database file; restoring a backup is tested.

Maintainability. Rust workspace with shared model types, tests at every layer (`make check`), no network in tests, Windows tests gated where the platform differs. The web build committed in `crates/cs-app/web-dist` must equal `web/dist`.

## 6. Data model and export contracts

The binding contracts are in `docs/CONTRACTS.md` (versions and packages, the human reference and the challenger, verification, cases, saturation) and `docs/API.md` (types and routes). They remain valid. Where this document and CONTRACTS.md disagree on study-specific wording, this document wins; CONTRACTS.md will be cut down to the generic parts before the public release (see `docs/PUBLISHING.md`).

Existing entities, as in `crates/cs-store/src/schema.sql`: settings, interviews, sources, codes, research questions, research question memos, segmentations, units, human codings, unit flags, corrections, verifications, claims, to-clarify labels and resolutions, prompts, AI runs and outputs, human snapshots, review samples, arbitrations, cases and case units, audit log.

New entities needed for V1.

| Entity | Content | Notes |
|---|---|---|
| Attribute definition | id, name, type (text, choice, number, date), choice values, order, "share with the model" flag | Part of the project template. |
| Attribute value | interview id, attribute id, value | Filters and matrix axes. |
| Memo template | id, name, version, ordered questions, default flag | Versioned; an edit makes a new version. |
| Memo answer | interview id, template id and version, the interview's own question list (added, removed, reworded), one answer per question, dates | Answers survive later edits of the template. |
| Checklist item and checklist state | project items; per interview, item id and checked state with date | |
| Passage (span) | id, interview id, unit id, start and end offset in the unit, exact text, segmentation hash, kind (coded span or highlight), created by, created at, state (active, needs review) | Versioned anchor: the text and the hash allow detection of drift. The whole turn is a span covering the unit. |
| Passage code | passage id, code id | Many to many. |
| Code (extended) | stable id, display name, definition, includes, excludes, example passage id, parent id, status, origin, created at, version | The upper-case token today used as key becomes one alias of the stable id. |
| Code alias | code id, alias, reason (former name, synonym, merge) | Used by autocomplete and by imports. |
| Codebook revision | number, date, reason | Referenced by prompts and exports. |
| Theme | id, title (proposition), proposition text, status, decision memo, created at | |
| Theme code | theme id, code id | A code may serve several themes. |
| Theme sheet field | theme id, field (conditions, alternative, limits, and so on), text | Cases, divergent cases and main passages are links, not text. |
| Theme check answer | theme id, check id, answer, date | |
| Theme question | theme id, research question id | |
| Framework dimension and reading | dimension id, name, definition; per theme, dimension id and text | Only for projects with a framework. |
| Matrix view | id, name, row source, column source, filters, custom dimension list ids | A view is a definition, not stored truth. |
| Custom dimension list | id, name, ordered entries | |
| Matrix cell | view or list id, row key, column key, value (present, not mentioned, contradicted, uncertain), summary, state (set by the researcher, or not yet coded) | Passage count is derived; the passages are links. |
| Link | id, passage id, target kind (theme part, research question part), target id, link type, one-line note | Replaces a binary for and against. |
| Evidence nature, origin, role lists | configurable lists | Claims keep their current structure and read these lists. |
| Project template | JSON as in 3.4, with a format version | A file, not a table; import creates the entities above. |

Rules. Every new entity change is audited. Deleting a passage, a link or a code is logical. Counts distinguish passages, interviews, cases and attribute values. Matrices are recomputed from passages, links and cell values, so that the stored truth is the passages and the researcher's cell values.

Export contracts. Each new entity appears in the package as CSV with stable ids and a column for the version of the segmentation it refers to, and as JSON where nesting is needed. The manifest lists every file with its SHA-256 and size, and `state.json` includes digests for the new tables. The generator version is recorded in each package. The existing file names of CONTRACTS.md do not change.

## 7. Acceptance tests

Tests A1 to A12 are adapted from the external requirements document. A13 to A20 cover the AI second coder. A21 to A26 cover genericity and the configuration. All are automated where the engine allows, and run on the fictional example data.

**A1, Exact passage.** Select characters 120 to 245 of a turn and code them. Reopen: the highlight, the stored text and the surrounding context match exactly. A passage whose offsets do not return the expected text is refused.

**A2, Multi-coding.** Apply two codes to the same passage and to overlapping passages. Both references are visible and neither overwrites the other.

**A3, Merge without loss.** Code A has 3 passages in 2 interviews, code B has 4 passages in 3 interviews. After merging B into A, the 7 sources are traceable, identical passages are de-duplicated explicitly, and the interview count is recomputed from distinct ids.

**A4, Rename.** Renaming a code changes its display name, keeps its id and its codings, and keeps the old name as an alias found by autocomplete.

**A5, Hierarchy.** Move B under A. The number and ids of passages of B are unchanged, and A's roll-up count includes B's passages.

**A6, Source correction.** Correct a turn through the overlay. The original stays readable, the unit returns to unverified, and no passage moves silently. Where a versioned transcript is introduced (F-24), affected passages are remapped with verification or marked for review.

**A7, Codebook and passages export.** Export the codebook with definitions, passage counts and distinct interviews. Extract every passage of a code with ids, interview and context.

**A8, Chain of evidence.** From a theme, reach its codes, then its passages, then the interview and the exact version of the source, in at most three clicks. No dead link, including after a code merge.

**A9, No fabricated quotation.** An AI answer, a candidate concept or a link that cites text absent from the transcript is rejected. No suggestion becomes a coding without a human action.

**A10, Pseudonymisation.** With the option on, no name from the editable list appears in any prompt sent to a provider. A run record lists the masked categories. Free text still requires a manual check, and the test asserts that the tool says so.

**A11, Unicode.** Select passages containing accents, apostrophes, emoji and line breaks in turns; offsets round-trip between the browser and the server without shift.

**A12, Reproducibility.** Export a package, then verify its manifest digests; export again without changes and compare state digests. Import of a whole project (V2) restores codes, passages, themes, memos, attributes and file hashes.

**A13, Blind guard.** A prompt batch cannot be generated while a unit of it lacks the human coding. The generated prompt contains no human code, no memo, no highlight and no interviewer turn as a unit to code.

**A14, Frozen reference.** After a prompt is generated, editing the human coding does not change the comparison; the unit is shown as revised, with both codings.

**A15, Raw answer kept.** A pasted answer is stored byte for byte with its SHA-256 before anything is parsed, and exports return it unchanged.

**A16, Strict validation.** An answer for the wrong interview or segmentation, with a missing or duplicated unit, or with an unknown code is rejected as a whole and all its problems are listed. Nothing is stored.

**A17, Provider record.** A run through a configured provider records provider id, model, the parameters it could and could not set, the place of execution, and the prompt digest. A command-line provider that is not isolated fails the probe. A copy-and-paste run runs no model.

**A18, Candidate concept.** A model answer that reports a concept absent from the codebook creates a candidate in Review. Create, attach and discard each require a dated reason, and none changes any coding by itself.

**A19, Arbitration.** Every disagreement requires a written reason. The frozen reading, the model's reading and the decision remain three separate stored records, and a decision can be revised while the earlier one stays on record.

**A20, Tamper evidence.** Altering or deleting an audit event, a prompt, a frozen reference, an AI answer or an arbitration at the database level is refused, and a modified file breaks chain verification.

**A21, Second fictional study.** Import a project template for a different study (different research questions, a different set of attributes, a different memo template, no framework, other link types and evidence natures). Import a transcript, fill the sheet, answer the memo, code, build a theme, view a matrix and write with links, with no change to the code or to the build, and no mention of the first study anywhere in the interface or the exports.

**A22, Memo template per interview.** Select a template for an interview, remove one question and reword another for that interview only. The answers are stored with the template version, the template itself is unchanged, and a later edit of the template does not alter the stored answers.

**A23, Attributes as filters and axes.** Define a choice attribute and a number attribute, fill them, use them as matrix rows and as filters of the codes-by-attribute table. Changing the type of an attribute that has values is refused.

**A24, Framework only in Analyse and Write.** With a framework set, the coding screen, the codebook and the prompt show no framework dimension. The theme sheet and the reading table do. With no framework, none of these exists.

**A25, Matrix semantics.** A cell accepts only present, not mentioned, contradicted or uncertain. An uncoded interview shows "not yet coded". A cell with passages cannot be set to "not mentioned" without a warning.

**A26, Template round trip.** Export a template, import it into an empty project, export again: the two files are identical. An invalid template is refused as a whole with all problems listed. Importing never removes an attribute that has values.

**A27, Lexical workbench.** On the fictional interview, a highlight yields keyphrases and stems; the keyword table counts passages and interviews correctly; similar passages are ranked deterministically; no word cloud exists anywhere; a run with no model configured works completely.

**A28, Interviewer exclusion.** An interviewer turn cannot be coded, cannot enter a batch, cannot become a review item, and is shown for context in the prompt only as the preceding turn.

**A29, Final audit.** Draw three random claims from the written parts. For each, the tool shows sentence, theme, codes, passages, interview and source version, with no dead link.

## 8. Backlog

Status at the date above. Done items are marked "Done"; the remaining items keep their order.

### 8.1 V1, ordered

V1 is what the author needs for the thesis before 30 October 2026. The order puts what blocks the rest first. Each lot ends green on `make check`, with the web build regenerated and the acceptance tests of the lot added.

**Lot 1, Passages and coding.** Done: F-25, F-27, F-28, F-29, F-30, F-32, F-33, F-40, F-42, F-43, F-44, F-46, F-47. Remaining, in order: F-41 (aliases in the code picker), F-31 (neighbouring codes), F-45 (distinct cases and distinct attribute values per code). Tests A1, A2, A4, A5, A11 are covered on the server side (`docs/DEVGRAPH_v1_lots1-2.md`).

**Lot 2, Configuration and attributes.** Done: F-01, F-02, F-03, F-04, F-05, F-06, F-07, F-08, F-10, F-49. Remaining, in order: F-48 (memo required before coding the first three interviews), F-09 (origin and role lists), F-16 (matrix defaults and the pseudonymisation list in Settings). Tests A21 to A24, A26 are covered on the server side (`docs/DEVGRAPH_v1_lots1-2.md`).

**Lot 3, Lexical workbench.** Done: F-50, F-51, F-52, F-53, F-54. Remaining: none. Test A27 is covered on the server side (`docs/DEVGRAPH_v1_lots3-4.md`).

**Lot 4, Review.** Done: F-12, F-38, F-65, F-68, F-72, F-117. Remaining: none. Tests A9, A10, A13 to A19, A28 are covered on the server side (`docs/DEVGRAPH_v1_lots3-4.md`). Free text is not pseudonymised (see F-12).

**Lot 5, Analyse and Write.** Done: F-75 to F-83, F-88, F-89, F-92, F-93, F-94, F-118. Remaining, in order: F-90 (passage picker for a research question), F-91 (link from each synthesis to the themes that answer it). Tests A8, A25, A29 are covered on the server side (`docs/DEVGRAPH_v1_lot5.md`).

**Lot 6, Detail, export and single flow.** Done: F-99, F-115. Remaining, in order: F-57 (Detail drawer that opens for cited passages), F-23 (the three counters on the library line), F-116 (resizable panels in the interview screen).

**Lot 7, In-app guide.** Remaining, in order: F-112 (guide text written from the configuration), F-113 (loading the example project with one action).

Before the first thesis export, the whole V1 set needs one manual pass on the example library and a pilot on two or three real interviews with human time measured.

### 8.2 V2, ordered

V2 turns the thesis tool into a publishable one.

1. Neutral defaults, removal of study-specific documents, wording and fixtures (F-15), following `docs/PUBLISHING.md`; first public release 0.1.0 with the generic README, the licence and the example project. Remaining: F-15.
2. Done: English translation of the user guide (F-114). Remaining: interface localisation (F-119).
3. Remaining: signed and notarised bundles, installers, release automation (F-110), command-line equivalents of the main actions (F-109).
4. Remaining: versioned transcripts with projection of passages (F-24), undo of merges and split of codes (F-39).
5. Remaining: whole-project export and import (F-101), anonymised export (F-102).
6. Partly done: import and export in REFI-QDA project and codebook formats (F-103). Remaining: checks against real NVivo and MAXQDA exports.
7. Remaining: plain TXT and Markdown without a convention (F-19), audio synchronised with the transcript (F-62).
8. Remaining: full-text search with filters (F-55), code co-occurrence (F-56), export of documents for writing in docx (F-104).
9. Remaining: generic case fields (F-87).
10. Remaining: accessibility audit and fixes (F-120), the three-panel layout with context menus (F-121).
11. Remaining: several users with roles and a synchronisation strategy (F-122, F-123), reviewer role and a second human coder with intercoder comparison (F-74, F-124).
12. Optional, evaluated after the release and only with human validation: suggestions of existing codes for a passage, suggestions of close codes to merge, consistency checks of a code definition across its passages. These follow the external document's AI section and the rules of section 9.

## 9. Out of scope and deliberate refusals

Out of scope for this product: transcription of audio, social network analysis, sentiment scoring, real-time collaboration, a clone of every function of a commercial package, a web-hosted service, and statistical inference from a qualitative sample.

Deliberate refusals, which are design decisions and not gaps.

- No word clouds. The keyword table shows counts and the passages behind them.
- No automatic sentiment analysis.
- No AI-first coding. The human codes first; the model comes after the human reading is frozen, and it is never required.
- No automatic themes, no theme proposed from frequency, no theme generated by a model. A theme is written by the researcher.
- No framework imposed on the codebook or the coding screen.
- No merge, deletion or application of a code by a model. Every model output is a candidate that waits for a human decision with a reason.
- No generated text presented as a quotation. Quotations are exact substrings of the stored transcript.
- No frequency presented as a result. The tool will not say "most respondents" and gives no percentages of a purposive sample.
- No silent overwrite and no silent loss of history.
- No key stored in the database, and no network call that the user did not start.
- No silence read as absence: "not mentioned" is a coding state, not a finding.
- No decorative dashboard before the reader and the coding are reliable.

## 10. Glossary

**Anchor.** The position of a passage in a unit, with the text and the segmentation hash that allow drift to be detected.

**Arbitration.** The researcher's recorded decision, with a reason, on a unit where the human and the model codings differ.

**Attribute.** A field defined by the project and filled for each interview (sector, role, position in a chain, statement status).

**Blind coding.** Coding by the model without access to the human coding. Enforced by the storage layer.

**Case.** An identifiable unit of analysis documented by units attributed to it from any interview, for example a decision.

**Claim.** A quotation that carries a statement: an exact substring of a unit, qualified by evidence nature, origin and role.

**Code.** A label with a definition that describes the content of a passage. It has a stable id, a name, aliases and optionally a parent.

**Frozen reference.** The copy of the human coding taken when a prompt is generated, against which all comparisons with the model are made.

**Highlight.** A passage retained without a code, used to build codes and as a candidate quotation.

**KWIC.** Keyword in context: every occurrence of a word with its surrounding text.

**LATER.** A system code that parks a hesitation on a passage until the researcher resolves it.

**Matrix.** A table with rows and columns drawn from interviews, cases, attributes, themes, codes or custom dimensions; each cell has a closed value, a passage count and an optional summary.

**Passage (span).** A selected part of a unit, possibly the whole unit, that carries codes or a highlight.

**Project template.** A JSON file holding the configuration of a study, without data.

**Snowball.** A family of stemming algorithms, used to reduce words to a common stem in the lexical workbench.

**Theme.** An analytical proposition built by the researcher from codes, with a sheet, a status and links to passages.

**Theme sheet.** The structured record of a theme: proposition, codes, cases, passages, divergent cases, conditions, alternative interpretation, limits, decision memo, status.

**TF-IDF.** A weighting of words by their frequency in a passage and their rarity in the corpus, used here to rank passages by shared vocabulary.

**Typed link.** A link from a passage to a part of the written text, with a type from the configured list and a one-line note.

**Unit.** A speaker turn, split at sentence ends when it exceeds the configured length; the container and the unit of navigation.

**YAKE.** An unsupervised, statistical method of keyphrase extraction that needs no training and no model.
