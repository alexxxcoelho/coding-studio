# Protected reviewer export

This library owns exact field selection, authenticated encryption and the static reader. The application adapter in `cs-app/src/reviewer_export.rs` owns the explicit research-data whitelist, current-state projection, source checks, optional URL/QR and package creation.

- PBKDF2-HMAC-SHA256 at 600,000 iterations, random 16-byte salt per package.
- AES-256-GCM with a fresh random 12-byte IV for every part. The manifest and each source chunk have authenticated identities.
- Selected paths, values, source names and attachment descriptors are inside the encrypted manifest. Published chunk names are opaque. No database, provider secret, password or internal backup is included.
- Text pseudonymisation affects only the exported copy. Original source files remain intact. Stored source digests refer to originals, including when exported text was pseudonymised.
- Reader content uses DOM text nodes. No CDN, tracking or AI. Attachment requests stay on the same origin, refuse redirects and omit credentials. Local attachments use a user-selected data folder.
- The snapshot must still match at generation. Unknown, duplicate and empty selections fail. Changed, absent, oversized or out-of-root sources fail without a partial final package.
- Limits are explicit: 256 MiB text JSON, 1 GiB per source file, 8 MiB source chunks, 32 MiB selection request. No silent truncation. Downloads are assembled in browser memory.
- Hosting is separate. Use trusted HTTPS, retain the whole package structure and send the password separately. This is neither account management nor revocation of downloaded copies. Without an address, no URL or QR is generated.

From the workspace root:

```sh
cargo test -p coding-studio-final-export
cargo test -p cs-app --test reviewer_api
cargo run -p coding-studio-final-export --example fixture -- /tmp/reviewer-test.html
READER_FIXTURE=/tmp/reviewer-test.html node crates/cs-reviewer-export/web/reader.test.mjs
```

The fixture and its password are synthetic public test data. Never use that password for research exports. JavaScript runtime tests are not a substitute for real browser QA.
