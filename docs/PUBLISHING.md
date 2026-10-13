# Publishing checklist

For the first public release (0.1.0). Work through the list in order; every item that is not ticked blocks the release.

## 1. Tree

- [ ] No personal path, name or organisation in the tree. Run the search below; the only acceptable result is the author's name in the licence and in `Cargo.toml`.

  ```bash
  grep -rn "/Users/alex\|kDrive\|VapeShop\|Dos Santos" --include=*.rs --include=*.ts --include=*.tsx --include=*.md --include=*.toml . | grep -v node_modules
  ```

- [ ] Files that belong to the authoring environment are not tracked: `.claude/`, `.simple-workflow/`, `.veritas/`, `.graph-indexer/` and any local handoff or session log. Add them to `.gitignore` and remove them from the index.
- [ ] Thesis-only documents are removed or rewritten: `docs/CONTRACTS.md`, `docs/DECISION_DRAFT.md`, `docs/DESIGN_V3.md`, the `DEVGRAPH_*` notes, and the one-off scripts in `tools/` other than `check.sh`.
- [ ] `LICENSE` (MIT, 2026), `README.md`, `CONTRIBUTING.md` and `examples/` are present and reviewed.
- [ ] The `run` target of the Makefile and the `THESIS` variable are removed or replaced by generic paths.
- [ ] No database, export, transcript or coding file is tracked (`git ls-files | grep -E "\.(db|sqlite|vtt)$"` returns nothing).

## 2. Continuous integration

- [ ] The workflow `.github/workflows/ci.yml` is green on `main` on ubuntu-latest, macos-latest and windows-latest, and the web job passes.
- [x] The Windows job runs the same commands as the other platforms: `tests/providers.rs` (cs-ai) and `tests/providers_api.rs` (cs-app) are gated with `#![cfg(unix)]`.
- [ ] Action pins are current; the commit SHA and the tag in each comment agree.

## 3. Examples

- [ ] `cargo run --release -p cs-app -- --library examples/library --db /tmp/example.db` starts.
- [ ] `POST /api/library/scan` lists `INT-01`, and segmentation of `INT-01` gives 28 units.
- [ ] `POST /api/codebook/import` with `examples/codebook.csv` loads 10 codes.
- [ ] `examples/providers.example.json` is accepted by `PUT /api/providers`.

## 4. Release

- [ ] `CHANGELOG.md` lists what changed since the first commit, with the protocol-relevant changes (prompt version, segmentation, audit chain) first.
- [ ] `Cargo.toml` version and the changelog agree (0.1.0).
- [ ] The web build committed in `crates/cs-app/web-dist` matches `web/dist` (the CI job checks this).
- [ ] Tag `v0.1.0` on the commit that passed CI: `git tag -a v0.1.0 -m "Coding Studio 0.1.0"`, then push the tag.
- [ ] Desktop bundle: the bundle identifier, the copyright and `bundle.macOS.minimumSystemVersion` in `crates/cs-desktop/tauri.conf.json` are reviewed. `make desktop` (`cargo tauri build`, see "Lancer l'application" in the README) writes `Coding Studio.app` and the `.dmg` under `$CARGO_TARGET_DIR/release/bundle/`. The CI job `Desktop app (macOS)` has built the `.app` on the tagged commit. The bundle is not signed or notarised; say so in the release notes.

## Open items at the first draft

Matches of the search above at the time this checklist was written. Each needs a decision by the owner.

| Where | What | Suggested action |
|---|---|---|
| `crates/cs-model/src/config.rs` | Default `interviewers` contains the author's speaker labels | Default to an empty list, or to `INT` |
| `crates/cs-ingest/tests/real_library.rs` | Test reads a personal library path; skipped when absent | Gate it behind an environment variable |
| `crates/cs-store/src/tests.rs` | Test reads a personal `coding.db` and schema; skipped when absent | Same as above |
| `crates/cs-app/tests/api.rs`, `config_api.rs`, `e2e.rs`, `providers_api.rs`; `crates/cs-ingest/src/*`; `crates/cs-model/tests/study_config.rs`; `crates/cs-ingest/tests/study_config.rs` | The speaker label "Dos Santos Alexandre" in fixtures | Rename the fixture label to a generic one |
| `crates/cs-ingest/src/scan.rs` | Message mentions "kDrive" | Reword to "file looks offline (synchronised folder?)" |
| `crates/cs-desktop/tauri.conf.json` | Copyright holder | Keep, if the author's name is intended to appear |
| `crates/cs-desktop/src/config.rs`, `crates/cs-desktop/tauri.conf.json` | The configuration folder and the bundle use the author's reverse-domain identifier `ch.dossantos.codingstudio` | Decide whether the identifier changes before the first release |
| `docs/CONTRACTS.md` | Binds the thesis manuscript and a commercial application | Remove from the public tree |
| `docs/METHOD.md` | Written for the thesis, refers to thesis folders and scripts | Rewrite as a generic procedure, keeping the protocol rules |
| `tools/*.py` | One-off scripts for the thesis development | Remove |
| `Makefile`, `web/vite.config.ts` | Comments mention kDrive; `run` target uses thesis paths | Reword comments; remove or generalise `run` |
| `.claude/`, `.simple-workflow/` | Tracked session and handoff files | Untrack and ignore (see section 1) |
