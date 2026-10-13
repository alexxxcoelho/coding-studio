# Contributing

Thank you for considering a change. Coding Studio is small, and its rules are strict on purpose: the audit trail and the blind guard are the reason it exists, so changes that touch them need a test that shows they still hold.

## Before you write code

- Read `docs/METHOD.md` (the procedure) and `docs/API.md` (the routes). The procedure decides what the tool may and may not do.
- Split larger changes into work items with explicit dependencies, the way the repository's dev graph describes them. Keep each item small enough to review on its own, and let independent items proceed in separate branches or worktrees.
- Write the test before the code when you change behaviour: a failing test names the rule, and the fix makes it pass.

## Before you open a pull request

```bash
make check            # format, clippy and the tests that run without GTK
```

Run `make desktop-check` as well if you touched `crates/cs-desktop`. If you touched `web/`, run `make web-test`, then `make web` and commit the rebuilt `crates/cs-app/web-dist`, because the release binary embeds that build. CI fails when the two differ.

Checks must be green on macOS, Linux and Windows. Do not add `#[allow(...)]` or `--no-verify` to get past a check; if a check is wrong, change the check in its own pull request and explain why.

## Rules for the repository

- **No research data in the repository.** Interviews, transcripts, codings, exports, databases and identifiable quotations never belong here, not even as test fixtures. Use invented text, as `examples/` does. The same goes for personal paths, names and organisations: the tree must stay free of them.
- **No secrets.** API keys, tokens and credentials are read from environment variables or from a token folder outside the repository. Examples name variables; they never contain values.
- **Protocol changes are deliberate.** Any change to the segmentation, the prompt, the answer validation or the audit chain changes the meaning of past exports. Say so in the pull request, bump the prompt version when the prompt changes, and add a test that a previous export still verifies.
- **Build artefacts stay out of the tree.** Use the Makefile's `CARGO_TARGET_DIR`, and do not commit `target/` or `node_modules/`.

## Commits and reviews

Write commit messages in the imperative, with the reason in the body when it is not obvious. Keep formatting-only changes apart from behaviour changes. Reviews check the rules above first, then the code.

## Licence

By contributing you agree that your contribution is released under the MIT licence of this project.
