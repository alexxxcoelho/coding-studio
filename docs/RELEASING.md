# Releasing

A release is a version number in the tree and a run of the release workflow, started by hand, (`.github/workflows/release.yml`) that builds the desktop app and attaches it to a GitHub Release.

## 1. Before releasing

1. Check that `main` is green in CI (`.github/workflows/ci.yml`), including the web job.
2. Choose the version (Semantic Versioning) and set it in both places:
   - `Cargo.toml`, the `version` under `[workspace.package]` (every crate inherits it);
   - `crates/cs-desktop/tauri.conf.json`, the `version` field.
3. Move the `Unreleased` section of `CHANGELOG.md` under a new heading `## [X.Y.Z] - YYYY-MM-DD`, add a fresh empty `Unreleased` section above it, and update the comparison links at the bottom if any.
4. Commit these changes on `main` and wait for CI to pass on that commit.

## 2. Run the release workflow

There is no tag to push. On GitHub, open Actions, then Release, then "Run workflow", choose the branch (main) and start it. From a terminal: `gh workflow run release.yml` (add `-f prerelease=true` for a pre-release).

The `prepare` job reads the version from `Cargo.toml` and `crates/cs-desktop/tauri.conf.json` (both must agree), then creates a draft release and its tag `vX.Y.Z` on the commit the run started from. If that release already exists, the run stops; to rebuild it and replace its files, run again with "rebuild" checked (`-f rebuild=true`).

## 3. What the workflow produces

| Job | Runner | Output | Blocks the release |
|---|---|---|---|
| Create release | ubuntu-latest | A draft release, with notes | Yes |
| macOS (Apple silicon, aarch64 only; Intel is not built) | macos-latest | `Coding-Studio_X.Y.Z_<arch>.dmg` and `Coding-Studio_X.Y.Z_<arch>.app.zip` | Yes |
| Linux | ubuntu-latest | `.AppImage` and `.deb` | No (`continue-on-error`) |
| Windows | windows-latest | `.msi` | Yes |
| Publish | ubuntu-latest | Publishes after verifying the macOS and Windows assets | Yes |

The web interface is built from `web/` during the workflow and embedded in the app, so the release always contains the current sources. The committed `crates/cs-app/web-dist` is still checked by the CI web job.

Each file is uploaded by its build job with `gh release upload`, and a rerun replaces the file of the same name.

## 4. Signing and notarisation

The macOS build is signed and notarised only when all six repository secrets exist. Without them the build is unsigned and the release notes say so.

| Secret | Use |
|---|---|
| `APPLE_CERTIFICATE` | Base64 of the Developer ID Application certificate (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | Password of the `.p12` |
| `APPLE_SIGNING_IDENTITY` | For example `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` | Apple ID used for notarisation |
| `APPLE_PASSWORD` | App-specific password of that Apple ID |
| `APPLE_TEAM_ID` | Team identifier |

Tauri reads these variables from the environment. The workflow exports them only when every secret is set.

Without signing, the first launch is blocked by Gatekeeper. Users open the app with right-click, then Open. The release notes give the same instruction.

## 5. After the workflow

1. Open the release, download the `.dmg` on a Mac that has not run the app before, and check that the app starts and that the database dialog appears.
2. For a signed build, check the signature: `codesign --verify --deep --strict "Coding Studio.app"` and `spctl --assess --type execute "Coding Studio.app"`.
3. Edit the release notes if the changelog section is needed in the notes.

## 6. Checking the workflow file

Before a change to `release.yml` is pushed, validate it locally:

```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml')); print('ok')"
actionlint .github/workflows/release.yml
```

Action versions are pinned to commit SHAs, as in `ci.yml`. Update a pin deliberately, with the tag it came from in the comment.
