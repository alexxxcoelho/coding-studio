# Changelog

All notable changes to Coding Studio are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-10-20

### Added

- Independent project windows for working with different databases.
- Staged transcript preparation: anonymisation, optional cleanup and translation, per-change review, and manual prompt/response exchange.
- OpenAI-compatible model endpoints with optional session-only Bearer authentication, reasoning controls and bounded retries for truncated responses.
- Participant spreadsheet import, interview links, and participant validation exports.
- Navigable reviewer workspace exports with optional password protection.
- Column sorting and filters throughout the application, with drag and drop for codebook hierarchy changes.
- Keyboard code search with C, arrow navigation, repeat coding, and creation of a new code from the last search result.

### Changed

- Restore keyboard focus to the current verbatim when code search or code creation closes.
- Simplify codebook and analysis workflows and adopt LGPL-2.1-or-later.
- Publish releases only after macOS and Windows packages have both been attached.

## [0.1.0] - 2026-10-13

### Added

- Library scan of a folder with one sub-folder per interview, with an inventory of every source file and its SHA-256 digest, and detection of the primary transcript.
- Transcript formats: platform VTT, raw speech-recognition text with timestamps, cleaned Markdown with speaker turns, Budgie Echo Markdown, and written responses.
- Deterministic segmentation into numbered units, with long turns split at sentence ends. Interviewer turns are recognised by configured labels and excluded from coding.
- Coding of every unit against a codebook: a theoretical framework (TOE, TAM and UTAUT), a custom list, or none, plus codes that emerge from the data. Keyboard navigation of the coding screen.
- Codebook import from CSV, JSON and a legacy `coding.db` (read only), all or nothing; export to CSV and SQL, timestamped and never overwritten.
- Blind AI second coder: prompts generated from the configuration, with the human coding frozen before the prompt exists; answers stored byte for byte and validated strictly.
- Model providers: copy and paste (always available, the default), an isolated command-line provider, an OpenAI-compatible HTTP provider, and ChatGPT sign-in.
- Confrontation of human and AI codings on the frozen reference, arbitration with a written reason, and agreement figures (exact agreement, Cohen's kappa per code).
- Append-only, hash-chained audit log, with verification from the interface and from `GET /api/audit/verify`.
- Interview export package (units, codings, prompts, raw answers, agreement, adjudication log, final coding, manifest with digests), and cross-corpus package.
- Corrections as dated overlays on the transcript, with verification states (unverified, verified, unusable).
- Cases, cross-case matrix, saturation record and research-question memos.
- Local HTTP server on 127.0.0.1 with the embedded web interface, and a command-line launcher.
- Desktop application for macOS (Tauri 2) with native dialogs, a Database menu, and light and dark appearance that follows the system.
- Continuous integration on macOS, Linux and Windows, including the web interface checks and the check that the embedded build matches the sources.
- Release workflow (`.github/workflows/release.yml`): on a `v*` tag, builds the macOS app and disk image for Apple silicon and Intel, and attaches them to a GitHub Release. Signing and notarisation run when the Apple secrets are configured. Linux (AppImage, deb) and Windows (msi) builds are attempted and do not block the release.
- This changelog and `docs/RELEASING.md`.

### Known gaps

- The desktop bundles are not signed or notarised until the Apple secrets are configured in the repository.
- Command-line equivalents of the main actions and the whole-project export and re-import are not complete (see `docs/SPEC.md`, sections 4.9 and 4.10).
- Some interface screens are still being rebuilt on the single four-step flow (see `docs/SPEC.md`, section 4.12).
