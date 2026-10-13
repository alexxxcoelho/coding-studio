# Coding Studio

Coding Studio is a local tool for qualitative coding of interview transcripts. It takes each interview from a raw transcript to an arbitrated, auditable coding. A language model can act as a blind second coder, but it never sees the human coding before its own answer is recorded, and every step is written to a tamper-evident log that can be exported with the interview.

The tool does five things:

1. Scans a folder holding one sub-folder per interview and builds a library, with an inventory of every source file and its SHA-256 digest.
2. Segments a transcript into numbered units with a fixed, deterministic rule.
3. Lets the coder code every unit against a codebook (a theoretical framework such as TOE, a custom list, or none, plus codes that emerge from the data).
4. Generates a prompt for a blind AI challenger with a strict JSON answer format, and imports the answer after validating it.
5. Lists every unit where the human and the AI codings differ, and records the coder's decision and rationale for each.

Every action is written to an append-only, hash-chained audit log. Each interview can be exported as a self-describing package whose files are never overwritten.

## Who it is for

Researchers doing qualitative interview studies who want a documented coding procedure, including a record of how any AI assistance was used. It suits small teams and single coders. It does not transcribe audio, and it does not call a model unless you configure one (see AI providers).

## Install

Requirements: a recent stable Rust toolchain installed with `rustup` (tested with Rust 1.95). Node.js and pnpm are needed only to rebuild the web interface; the built interface is committed and embedded in the binary.

```bash
git clone <repository-url> coding-studio
cd coding-studio
make check     # format check, clippy and the tests that run without GTK
cargo build --release -p cs-app
```

The Makefile sets `CARGO_TARGET_DIR` to `$HOME/.cache/coding-studio/target` so that build artefacts stay out of the source tree and out of any synchronised folder. Without the Makefile, export that variable yourself before calling `cargo`.

Run `make desktop-check` for the full workspace, including the desktop shell. That target needs GTK on Linux.

## Run

The server needs a database file (created when absent) and, normally, a library folder:

```bash
cargo run --release -p cs-app -- \
  --db ./data/coding_studio.db \
  --library ./interviews
```

The server listens on `http://127.0.0.1:7878/` and opens the browser. Stop it with Ctrl+C.

| Option | Default | Meaning |
|---|---|---|
| `--db` | required | The tool's own SQLite database, created when absent |
| `--library` | none | Folder with one sub-folder per interview; saved as the default library |
| `--coding-db` | none | Optional legacy codebook database, opened read only (see Codebook) |
| `--port` | 7878 | Local port; the server listens on 127.0.0.1 only |
| `--actor` | candidate | Name recorded on every action |
| `--interviewer-name` | none | Speaker label of the interviewer in platform transcripts (repeatable) |
| `--no-open` | off | Do not open the browser |

The database uses `journal_mode = DELETE`, which is safe in a synchronised folder. Keep the database and the exports together, and keep the interview folder read only if you can.

A ready-to-scan example is in `examples/`, which contains one fictional interview, a codebook and a providers file. See `examples/README.md`.

### Desktop app

The same server and interface run in a native window (Tauri 2), with native dialogs to choose folders and files.

```bash
make desktop       # builds the desktop bundle for the current platform
make desktop-dev   # runs the desktop app without bundling
```

On first launch the app asks where to keep its database. The choice is saved in `config.json` in the platform configuration folder, which can also hold `library`, `coding_db`, `actor` and `interviewer_names`. The Database menu opens or creates another database; the app restarts on it. `CODING_STUDIO_DB=/path/to.db` overrides the saved choice without changing it. The server inside the desktop app listens on a free port of 127.0.0.1, so the app and a browser session can run side by side on different databases. Unsigned builds need an explicit confirmation the first time they are opened on macOS.

### Appearance

The interface follows the system light or dark setting. The switch in the top bar (System, Light, Dark) overrides it; the choice is kept by the browser or the desktop app.

## Study configuration

The study is configured through the interface or with `GET /api/config` and `PUT /api/config`. The configuration is stored in the database, and every change is written to the audit log.

| Field | Meaning |
|---|---|
| `framework` | Phase 1 categories: `toe` (technology, organisation, environment, plus CONTEXT), `tam_utaut` (the four TAM and UTAUT constructs, plus CONTEXT), `none` (CONTEXT only), or `custom` |
| `phase1_categories` | Used only with `custom`: a list of `{code, label, definition, attachment}`, where `attachment` is `T`, `O`, `E` or null |
| `id_prefixes` | Interview identifier prefixes, letters and digits only; default `INT`, `ECO`, `RET`, `ADJ`. A folder whose name starts with one of them becomes an interview (for example `INT-01_Example_Retailer`) |
| `interviewers` | Speaker labels that mark the interviewer in platform transcripts |
| `max_words` | Longest unit before it is split at sentence ends; default 150, minimum 20 |
| `prompt_template` | The static parts of the AI prompt: `version`, `header`, `codebook_intro`, `units_intro` and `footer`. The schema and the code and unit lines are added by the tool and cannot be removed by a template |
| `study_name` | Free text, shown in the interface and in exports |

Changing `max_words` or the categories affects only interviews not yet segmented or coded; a segmentation cannot be redone once coding has started.

## Codebook

The codebook holds the codes that the coder applies in the second pass. Codes are named in upper case (`^[A-Z][A-Z0-9-]*$`), and each has a definition, an origin (`literature`, `field` or `legacy`), an optional TOE attachment (`T`, `O`, `E`), the research questions it bears on, and a version. Codes can be created, edited, merged into another code, or deactivated; each change increments the version and is logged.

Three ways to load a codebook, all through `POST /api/codebook/import` with a JSON body `{"kind": ..., "path": ...}`:

**CSV** (`"kind": "csv"`). Columns are read by name, in any order. Required: `code` and `definition`. Optional: `label`, `origin` (default `field`), `toe` (`T`, `O`, `E` or empty), `research_questions` (separated by `;` or `,`; questions that do not exist yet are created), `version`, `example`, `reason`. `family` is accepted and ignored, because it is derived from `toe`. RFC 4180 quoting applies. The import is all or nothing: one invalid row refuses the whole file, and every problem is reported. `examples/codebook.csv` shows the format.

**JSON** (`"kind": "json"`). An object with a `codes` array; each entry takes the same fields as the CSV, plus `active` and `in_prompt`. Values are validated exactly as in CSV.

**Legacy database** (`"kind": "coding_db"`, or the `--coding-db` option). Reads the codes of an older `coding.db` SQLite file. The file is opened read only and nothing is ever written back to it. Codes removed upstream are deactivated, never deleted.

The codebook can be exported as CSV and SQL with `POST /api/codebook/export`, written to `<db folder>/exports/codebook/` with a timestamp and never overwritten.

## AI providers

The second coder is optional. The procedure works with the copy-and-paste workflow alone, and that is the default.

| Provider | How it runs | What leaves the machine |
|---|---|---|
| `copy_paste` (always present) | You copy the prompt into the model interface you choose and paste the answer back. The tool runs nothing and sends nothing. | Whatever you send in that interface |
| CLI | A command you configure, run with the prompt on standard input or as its last argument. It runs in a fresh, empty temporary directory outside any project folder, with an environment reduced to `PATH`, `LANG` and a new `HOME`. Extra flags that switch off the client's own tools and memory are appended. | The prompt, to wherever the command sends it. A local command can call a hosted model, so the configuration declares where execution happens, and the declaration is recorded |
| OpenAI-compatible API | An HTTPS request to an endpoint. Presets: `openai` (`https://api.openai.com/v1`) and `zai` (`https://api.z.ai/api/paas/v4`); `custom` takes any base URL. | The prompt, to that endpoint |
| ChatGPT sign-in (OAuth) | You sign in with your ChatGPT account in the browser (PKCE, with a callback on a loopback address). Tokens are kept in a token folder you specify. | The prompt, to the OpenAI API, under that account |

Keys are never written in the configuration. The OpenAI-compatible provider names the environment variable that holds the key (`api_key_env`), reads it at call time, and never stores or journals its value. The OAuth tokens live only in the token folder.

A CLI provider comes with an isolation probe: the model is asked what files and instructions it can see from its working directory, and the probe passes only if the answer is exactly `NONE`.

Each provider call reports the model name, the parameters it could set (temperature, seed, maximum tokens) and those it could not, where the execution happens, the SHA-256 of the prompt and the raw answer exactly as returned, which the run then records.

Providers are described by a JSON document, `examples/providers.example.json`, with one entry of each kind. The copy-and-paste workflow is always listed first. The adapters are implemented and tested in `cs-ai`; their HTTP wiring (`GET` and `PUT /api/providers`, and running a batch through a provider) is being completed, and until it is released the interface offers the copy-and-paste workflow only.

## Workflow

1. **Library.** Scan the interview folder. Each sub-folder whose name starts with a configured prefix becomes an interview. Supported transcripts: platform `.vtt`, raw speech-recognition `.txt` with timestamps, cleaned Markdown named `02_transcript_cleaned_diarised.md` (turns written as `**INT `[mm:ss]`:** text` or `**RESP** `[mm:ss]` text`), Budgie Echo `.md` exports, and written responses. Summaries and record sheets are listed and viewable read only; they are never sent to an AI.
2. **Segmentation.** Choose the primary transcript and segment it: one unit per continuous speaker turn, with turns over `max_words` split at sentence ends. Each unit has an identifier, a speaker, a timestamp and its text, unchanged except for redundant spaces.
3. **Phase 1, pre-classification.** Assign each unit to a framework category or NA. These assignments orient the reading and decide which units receive detailed coding.
4. **Phase 2, coding.** Code each unit in scope with one or more codes, or NA. Optional fields: sub-code, modifier, evidence level, provenance, memo, and a verbatim excerpt that must be an exact substring of the unit. A provisional label can be placed on a unit when no code fits; labels are resolved later into a new field code or a mapping to an existing one. Keyboard: `j` and `k` move, `/` filters codes, `Enter` toggles the highlighted code, `Ctrl Enter` saves and moves on.
5. **AI challenge.** Generate the prompt for a batch of units (optionally in parts), record the run (model, version, temperature, seed, context length, execution time), and import each answer. The method recommends three runs: the main model, a reworded prompt, and a second, open-weight model. See `docs/METHOD.md`.
6. **Review.** Decide every disagreement between the frozen human reference and a run, with a written rationale, and look at the recorded sample of agreements. Agreement figures (exact agreement, Cohen's kappa per code) are reported, not optimised.
7. **Claims and verification.** On each unit, mark the exact passages that carry a statement and qualify them (evidence level, provenance, role in the decision). Flag uncertain, second-hand, unfinished or co-constructed units. Record corrections against the recording with a reason. Mark units verified or unusable. The Verification tab lists what must be checked and the coverage reached.
8. **Cases.** Attribute units, from any interview, to each case (a decision, such as adopting a technology). Fill the descriptive fields and the five synthesis fields with what supports, contradicts and is missing. A case without attributed units is undocumented in the cross-case matrix. Then export the audit package from the Audit tab.
9. **Answers.** Keep a memo per research question (documented, interpretation, competing explanations, limits) and export the cross-case package: answers, matrix and saturation record.

## Protocol guarantees

These rules are enforced by the storage layer, not by the interface, and are covered by tests:

- **Blind guard, at prompt time.** A prompt batch cannot be generated while any of its units lacks the coder's phase 2 coding. Generating it freezes a copy of that coding (`human_snapshots`, with the digests of the units and of the codebook) as the reference of every comparison with the answers to the batch. An AI answer is also refused while any unit of its part lacks the coder's coding.
- **Immutable reference.** The coder's coding stays editable after the prompt and after the answer, but disagreements, the agreement sample and the agreement figures are computed on the frozen reference. A revised unit is shown with its frozen and its current coding and marked as revised. The initial comparison, the revisions and the final decision are three distinct records.
- **Frozen units.** An interview cannot be segmented again once coding has started, so every code always refers to the same unit text.
- **Append-only, hash-chained log.** Every write is recorded in the same transaction as the change. Each event carries the SHA-256 of the previous one. Database triggers refuse any update or deletion of the log, of prompts, of frozen references, of AI answers and of arbitration records. The chain can be verified at any time from the interface or with `GET /api/audit/verify`.
- **Read-only codebook source.** A legacy `coding.db` is opened read only; codes removed upstream are deactivated, never deleted.
- **Corrections never rewrite the segmentation.** They are append-only overlays, and a corrected unit returns to the unverified state.
- **Redaction.** The prompt redacts unit identifiers found in code texts, so the codebook cannot reveal the coder's coding of the challenged units.
- **Raw answers kept verbatim.** Each AI answer is stored exactly as pasted, with its SHA-256, before anything is parsed from it.
- **Determinism.** Segmentation and prompts are byte-identical for identical inputs. Prompts and segmentations are bound together by hashes, and an answer built on another segmentation is refused.
- **Strict validation.** An AI answer must name the right interview and segmentation, cover every unit of its part exactly once, and use only codes offered in the prompt or NA. All problems are reported at once.

## Export package

`POST /api/interviews/<ID>/export`, or the Export button, writes a new folder `<db folder>/exports/<YYYY-MM-DD>_<ID>/` (a time suffix is added if it exists; nothing is overwritten):

| File | Content |
|---|---|
| `units.csv` | `unit_id,speaker,timestamp,text` |
| `coding_human.csv` | `unit_id,codes`, codes separated by `;` |
| `human_full.csv` | Every field of the human coding |
| `prompt_part<p>_of<n>_<hash>.txt` | Each prompt, byte for byte |
| `run<id>_<label>.json` | Run parameters |
| `coding_ai_run<id>_<label>.csv` | Validated AI codes |
| `ai_run<id>_<label>_part<p>_raw.txt` | Raw AI answers as pasted |
| `agreement.md` | Exact agreement and kappa per code |
| `adjudication_log.md` | Every divergence and its decision |
| `coding_final.csv` | Final coding with its basis (agreed or arbitrated), once complete |
| `human_snapshot_batch<id>.csv`, `.json` | The frozen reference of each prompt batch |
| `cases.csv`, `case_units.csv`, `cases.json`, `cases.md` | Cases, their attributed units and their syntheses |
| `claims.csv`, `ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv` | Claims and their distributions (counting unit: the claim) |
| `units_verification.csv`, `unit_corrections.csv` | Verification states, flags, corrections |
| `state.json`, `db_snapshot.sqlite` | Digest of the whole state behind the package, generator version, database copy |
| `audit_log.jsonl`, `audit_chain.json` | Events of the interview and chain verification |
| `README.md`, `manifest.json` | Explanation, SHA-256 and size of every file |

The CSV formats are plain and documented, so the results can be recomputed independently of the tool.

## Architecture

```
crates/
  cs-model   shared types, hashing, error kinds, study configuration
  cs-ingest  library scan, transcript parsers, segmentation
  cs-store   SQLite schema, protocol guards, audit chain, codebook import
  cs-ai      prompt builder, answer validation, agreement, adjudication, providers
  cs-app     local HTTP server (axum), embedded web UI, export, command line
  cs-desktop Tauri 2 window around cs-app, native dialogs, Database menu
web/         React interface (TypeScript, Vite); built into crates/cs-app/web-dist
```

The JSON API is described in `docs/API.md`. The procedure, and how the tool executes each step, is in `docs/METHOD.md`.

## Development

```bash
make check            # fmt --check, clippy and tests, without cs-desktop
make desktop-check    # the same for the full workspace (needs GTK on Linux)
tools/check.sh        # the subset of make check, with a short summary
make web              # rebuild the interface into crates/cs-app/web-dist (needs pnpm)
make web-test         # interface tests and lint (needs pnpm)
```

The web build is committed, so `cargo build` needs no Node.js. After any change in `web/`, run `make web` and commit the result; CI fails when `web/dist` and `crates/cs-app/web-dist` differ.

Continuous integration (`.github/workflows/ci.yml`) runs formatting, clippy and the tests on macOS, Linux and Windows (Windows runs a reduced set, see the workflow), and checks the web interface and its embedded build. CONTRIBUTING.md describes the workflow for changes.

## Licence

MIT. See `LICENSE`.

## Origin

Coding Studio was built for an EMBA thesis on the adoption of item-level identification with RAIN RFID, and it implements the coding procedure described in `docs/METHOD.md`. The interviews of that study are not part of this repository, and the examples are fictional.
