# Coding Studio

A local tool that takes interviews from raw transcript files to an arbitrated, auditable coding. It was built for the EMBA thesis "RAIN RFID UHF Adoption in Switzerland: Best Practices and SME Transferability" and implements the coding protocol of `00_Tools/Prompts/P-04_blind_llm_coding_v1.0.md` and decisions D-003 and D-011.

The tool does five things:

1. Scans a folder holding one sub-folder per interview and builds a library, with an inventory of every source file and its SHA-256.
2. Segments a transcript into numbered units with a fixed, deterministic rule.
3. Lets the candidate code every unit against the thesis codebook (TOE and inductive codes).
4. Generates a copy-paste prompt for an AI coder with a strict JSON answer format, and imports the answer after validating it.
5. Lists every unit where the human and AI codings differ and records the candidate's decision and rationale for each.

Every action is written to an append-only, hash-chained audit log, and each interview can be exported as a self-describing audit package.

## Install

Requirements: a recent stable Rust toolchain installed with `rustup` (tested with Rust 1.95). No Node.js, no network access at run time; the web UI is embedded in the binary.

```bash
cd 00_Tools/CodingStudio
make check     # format check, clippy, every test
make build     # release binary
```

The Makefile sets `CARGO_TARGET_DIR=$HOME/.cache/coding-studio/target`, so build artefacts stay outside kDrive. Without the Makefile, export that variable yourself before calling `cargo`.

The workspace builds on macOS, Linux and Windows.

## Run

```bash
make run
```

This starts the server on `http://127.0.0.1:7878/` and opens the browser, with the thesis paths:

| Option | Default in `make run` | Meaning |
|---|---|---|
| `--library` | `04_Fieldwork/Interviews` | Folder with one sub-folder per interview |
| `--db` | `05_Analysis/coding_studio/coding_studio.db` | The tool's own database, created when absent |
| `--coding-db` | `05_Analysis/coding/coding.db` | Source of the codebook, opened read only |
| `--port` | 7878 | Local port; the server listens on 127.0.0.1 only |
| `--actor` | candidate | Name recorded on every action |
| `--interviewer-name` | the candidate's names | Speaker name of the interviewer in platform transcripts (repeatable) |
| `--no-open` | off | Do not open the browser |

The SQLite database uses `journal_mode = DELETE`, which is safe in a synchronised folder. Stop the server with Ctrl+C.

## Desktop app

The same server and interface in a native window (Tauri 2), with native dialogs to choose folders and files.

```bash
make desktop       # builds "Coding Studio.app" (macOS) in $CARGO_TARGET_DIR/release/bundle/
make desktop-dev   # runs the desktop app without bundling
```

On first launch the app asks where to keep its database (for the thesis: `05_Analysis/coding_studio/coding_studio.db`). The choice is saved in `config.json` in the platform configuration folder (`~/Library/Application Support/ch.dossantos.codingstudio/` on macOS), which can also hold `library`, `coding_db`, `actor` and `interviewer_names`. The Database menu opens or creates another database (the app restarts on it) and shows the database folder. `CODING_STUDIO_DB=/path/to.db` overrides the saved choice without changing it.

The server listens on a free port of 127.0.0.1, so the desktop app and `make run` can run side by side, each on its own database. The bundle is not signed: on first opening, macOS asks for confirmation (right click, Open).

## Appearance

The interface follows the system light or dark setting. The switch in the top bar (System, Light, Dark) overrides it; the choice is kept by the browser or the desktop app.

## Workflow

1. Library. Scan the interview folder. Each sub-folder whose name starts with an ID (`INT-`, `ECO-`, `RET-`, `ADJ-`) becomes an entry. Supported transcripts: platform `.vtt`, raw ASR `.txt` with timestamps, cleaned `02_transcript_cleaned_diarised.md`, Budgie Echo `.md` exports, written responses. Summaries and record sheets are listed and viewable read only; they never reach the AI.
2. Segmentation. Choose the primary transcript and segment it: one unit per continuous speaker turn, turns over 150 words split at sentence ends (the rule of `Scripts/segment_transcript.py`).
3. Coding. Code each unit with one or more codes, or NA. Optional fields: sub-code, modifier, evidence level, provenance, memo, and a verbatim excerpt that must be an exact substring of the unit. Keyboard: `j` and `k` move, `/` filters codes, `Enter` toggles the highlighted code, `Ctrl Enter` saves and moves on.
4. AI coding. Generate the prompt (optionally in parts), record the run (model, version, temperature, seed, context length, execution time), paste each answer. P-04 asks for three runs: R1 main model, R2 reworded prompt, R3 a second, open-weight model.
5. Review. Decide every disagreement between the frozen reference and the run with a written rationale, and look at the recorded sample of agreements; agreement figures (exact agreement, Cohen's kappa per code) are reported, not optimised.
6. Cases. Attribute units, from any interview, to each case (a decision about item-level identification), fill the descriptive fields and the five synthesis fields with what supports, contradicts and is missing; a case without attributed units is undocumented in the cross-case matrix. Then export the audit package from the Audit tab.

## Protocol guarantees

These rules are enforced by the storage layer, not by the interface, and are covered by tests:

- Blind guard, at prompt time. A prompt batch cannot be generated while any of its units lacks the candidate's phase 2 coding, and generating it freezes a copy of that coding (`human_snapshots`, with the digests of the units and of the codebook) as the reference of every comparison with the answers to the batch. An AI answer is also refused while any unit of its part lacks the candidate's coding.
- Immutable reference. The candidate's coding stays editable after the prompt and after the answer, but disagreements, the agreement sample and the agreement figures are computed on the frozen reference; a revised unit is shown with its frozen and its current coding and marked as revised. The initial comparison, the revisions and the final decision are three distinct records.
- Frozen units. An interview cannot be segmented again once coding has started, so every code always refers to the same unit text.
- Append-only, hash-chained log. Every write is recorded in the same transaction as the change. Each event carries the SHA-256 of the previous one; database triggers refuse any update or deletion of the log, of prompts, of frozen references, of AI answers and of arbitration records. The chain can be verified at any time from the interface.
- Read-only codebook source. `coding.db` is opened read only; codes removed upstream are deactivated, never deleted.
- Raw answers kept verbatim. Each AI answer is stored exactly as pasted, with its SHA-256, before anything is parsed from it.
- Determinism. Segmentation and prompts are byte-identical for identical inputs; prompts and segmentations are bound together by hashes, and an answer built on another segmentation is refused.
- Strict validation. An AI answer must name the right interview and segmentation, cover every unit of its part exactly once, and use only codes offered in the prompt or NA. All problems are reported at once.

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
| `audit_log.jsonl`, `audit_chain.json` | Events of the interview and chain verification |
| `README.md`, `manifest.json` | Explanation, SHA-256 and size of every file |

The CSV formats match the Python scripts of `00_Tools/Scripts/`, so `compute_agreement.py` and `verify_quotes.py` can recompute the results independently.

## Architecture

```
crates/
  cs-model   shared types, hashing, error kinds
  cs-ingest  library scan, transcript parsers, segmentation
  cs-store   SQLite schema, protocol guards, audit chain
  cs-ai      prompt builder, answer validation, agreement, adjudication, CSV and reports
  cs-app     local HTTP server (axum), embedded web UI, export
  cs-desktop Tauri 2 window around cs-app, native dialogs, Database menu
```

The JSON API is documented by the routes in `crates/cs-app/src/lib.rs`. The method, written for the thesis, is in `docs/METHOD.md`.
