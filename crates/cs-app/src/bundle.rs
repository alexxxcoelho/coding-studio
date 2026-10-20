//! F-101: whole-project export and import.
//!
//! One `.csproj.zip` holds everything needed to move a project: a snapshot of
//! the database, the source transcripts, the configuration (project template
//! and study configuration), the audit chain and a manifest with the digest
//! of every file, the state digest and the counts. Import goes into a new
//! database and verifies what it reads before it writes: digests of the
//! files, the audit chain of the bundle and of the database, the state
//! digest, the counts. A dry run reports; `apply` writes.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cs_model::{sha256_hex, AuditEvent, ChainCheck, CsError};
use cs_store::{Store, GENESIS_HASH};
use serde::Serialize;
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// Value of `format` in the manifest of a bundle.
pub const BUNDLE_FORMAT: &str = "coding-studio-project-bundle";
/// Version of the layout.
pub const BUNDLE_VERSION: u32 = 1;
/// Extension of the file.
pub const BUNDLE_EXT: &str = ".csproj.zip";

/// The exported bundle.
pub struct BundleOut {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub sha256: String,
    pub manifest: Value,
}

/// Row counts compared before and after a transfer.
pub fn project_counts(store: &Store) -> Result<BTreeMap<String, i64>> {
    let c = store.connection();
    let mut out = BTreeMap::new();
    for (key, sql) in [
        ("interviews", "SELECT COUNT(*) FROM interviews"),
        ("sources", "SELECT COUNT(*) FROM sources"),
        ("units", "SELECT COUNT(*) FROM units"),
        ("codes", "SELECT COUNT(*) FROM codes"),
        ("passages", "SELECT COUNT(*) FROM passages"),
        ("passage_codings", "SELECT COUNT(*) FROM passage_codings"),
        ("human_codings", "SELECT COUNT(*) FROM human_codings"),
        ("claims", "SELECT COUNT(*) FROM claims"),
        ("cases", "SELECT COUNT(*) FROM cases"),
        ("themes", "SELECT COUNT(*) FROM themes"),
        ("links", "SELECT COUNT(*) FROM passage_links"),
        (
            "attributes",
            "SELECT COUNT(*) FROM interview_data WHERE kind = 'attributes'",
        ),
        (
            "memos",
            "SELECT COUNT(*) FROM interview_data WHERE kind = 'memo'",
        ),
        ("persons", "SELECT COUNT(*) FROM persons"),
        ("person_groups", "SELECT COUNT(*) FROM person_groups"),
        ("person_links", "SELECT COUNT(*) FROM person_interviews"),
        ("audit_events", "SELECT COUNT(*) FROM audit_log"),
    ] {
        let n: i64 = c.query_row(sql, [], |r| r.get(0)).unwrap_or(-1);
        out.insert(key.to_string(), n);
    }
    Ok(out)
}

/// A project with nothing in it: a database an import may fill.
pub fn is_empty_project(store: &Store) -> Result<bool> {
    let c = project_counts(store)?;
    Ok(["interviews", "codes", "passages", "themes", "cases"]
        .iter()
        .all(|k| c.get(*k).copied().unwrap_or(0) == 0))
}

/// Recomputes the hash chain of a list of events.
pub fn verify_events(events: &[AuditEvent]) -> ChainCheck {
    let mut prev = GENESIS_HASH.to_string();
    for (i, e) in events.iter().enumerate() {
        let ok = e.prev_hash == prev
            && AuditEvent::compute_hash(
                &e.prev_hash,
                e.seq,
                &e.ts,
                &e.actor,
                &e.action,
                e.interview_id.as_deref(),
                e.target.as_deref(),
                &e.detail,
            ) == e.hash;
        if !ok {
            return ChainCheck {
                ok: false,
                n_events: i + 1,
                first_bad_seq: Some(e.seq),
            };
        }
        prev = e.hash.clone();
    }
    ChainCheck {
        ok: true,
        n_events: events.len(),
        first_bad_seq: None,
    }
}

fn file_entry(name: &str, data: &[u8]) -> Value {
    json!({"name": name, "sha256": sha256_hex(data), "size": data.len()})
}

/// Builds the bundle of the project in `store`.
pub fn export_bundle(store: &Store, actor: &str) -> Result<BundleOut> {
    let interview_ids: Vec<String> = store
        .summaries()?
        .into_iter()
        .map(|s| s.interview_id)
        .collect();
    let cases = store.cases(None)?;
    let (state_bytes, state_sha) = crate::export::state_document(store, &interview_ids, &cases)?;
    let events = store.audit_events(None)?;
    let chain = store.verify_chain()?;
    let counts = project_counts(store)?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    // Concurrent exports can observe the same clock tick. Reserve an isolated
    // directory instead of deriving a shared SQLite path from the timestamp.
    let snapshot_dir = tempfile::tempdir()?;
    let tmp = snapshot_dir.path().join("database.sqlite");
    crate::export::write_snapshot(store, &tmp)?;
    let db_bytes = fs::read(&tmp);
    let _ = fs::remove_file(&tmp);
    entries.push(("database.sqlite".into(), db_bytes?));
    entries.push((
        "config/project_template.json".into(),
        serde_json::to_vec_pretty(&store.export_template()?)?,
    ));
    entries.push((
        "config/study_config.json".into(),
        crate::export_v1::study_config_json(store)?,
    ));
    entries.push(("state.json".into(), state_bytes));
    // The register in readable form (the database copy holds it too).
    entries.push((
        "config/persons.csv".into(),
        crate::export_persons::persons_csv_full(store, None)?.into_bytes(),
    ));
    entries.push((
        "config/groups.csv".into(),
        crate::export_persons::groups_csv(store)?.into_bytes(),
    ));
    let mut jsonl = String::new();
    for e in &events {
        jsonl.push_str(&serde_json::to_string(e)?);
        jsonl.push('\n');
    }
    entries.push(("audit/audit_log.jsonl".into(), jsonl.into_bytes()));
    entries.push((
        "audit/audit_chain.json".into(),
        serde_json::to_vec_pretty(&chain)?,
    ));

    let mut interviews = Vec::new();
    for id in &interview_ids {
        let Some(entry) = store.interview(id)? else {
            continue;
        };
        let mut sources = Vec::new();
        for s in &entry.sources {
            let path = Path::new(&entry.folder_path).join(&s.rel_path);
            let name = format!("sources/{id}/{}", s.rel_path);
            let bundled = match fs::read(&path) {
                Ok(bytes) => {
                    let intact = sha256_hex(&bytes) == s.sha256;
                    entries.push((name.clone(), bytes));
                    Some(intact)
                }
                Err(_) => None,
            };
            sources.push(json!({
                "rel_path": s.rel_path,
                "sha256": s.sha256,
                "size": s.size,
                "bundled": bundled.is_some(),
                // The file on disk no longer matches the digest the scan recorded.
                "changed_since_scan": bundled == Some(false),
            }));
        }
        interviews.push(json!({
            "interview_id": id,
            "folder_name": entry.folder_name,
            "primary_source": entry.primary_source,
            "sources": sources,
        }));
    }

    let files: Vec<Value> = entries.iter().map(|(n, d)| file_entry(n, d)).collect();
    let manifest = json!({
        "format": BUNDLE_FORMAT,
        "version": BUNDLE_VERSION,
        "tool": "Coding Studio",
        "tool_version": crate::VERSION,
        "generated_at": crate::now(),
        "generated_by": actor,
        "state_sha256": state_sha,
        "audit": {
            "n_events": events.len(),
            "head_hash": events.last().map(|e| e.hash.clone()).unwrap_or_else(|| GENESIS_HASH.to_string()),
            "chain_ok": chain.ok,
        },
        "counts": counts,
        "interviews": interviews,
        "files": files,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opt = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    zip.start_file("manifest.json", opt)?;
    zip.write_all(&manifest_bytes)?;
    for (name, data) in &entries {
        zip.start_file(name.as_str(), opt)?;
        zip.write_all(data)?;
    }
    let bytes = zip.finish()?.into_inner();
    let sha256 = sha256_hex(&bytes);
    let file_name = format!(
        "coding-studio-{}{BUNDLE_EXT}",
        chrono::Local::now().format("%Y-%m-%d")
    );
    store.log(
        actor,
        "project.export",
        None,
        Some(&file_name),
        json!({
            "sha256": sha256,
            "state_sha256": state_sha,
            "n_files": entries.len() + 1,
            "counts": counts,
        }),
    )?;
    Ok(BundleOut {
        bytes,
        file_name,
        sha256,
        manifest,
    })
}

/// One file of the bundle checked against its manifest entry.
#[derive(Debug, Clone, Serialize)]
pub struct FileCheck {
    pub name: String,
    pub expected: String,
    pub actual: Option<String>,
    pub ok: bool,
}

/// Result of reading (and perhaps applying) a bundle.
#[derive(Debug, Clone, Serialize)]
pub struct BundleReport {
    pub dry_run: bool,
    pub applied: bool,
    /// Every check passed and the target accepts the project.
    pub ok: bool,
    pub format: Option<String>,
    pub tool_version: Option<String>,
    pub files_checked: usize,
    pub files_bad: Vec<FileCheck>,
    pub audit_chain_ok: bool,
    pub audit_events: usize,
    pub database_chain_ok: bool,
    pub state_ok: bool,
    pub expected_state_sha256: Option<String>,
    pub actual_state_sha256: Option<String>,
    pub counts_expected: BTreeMap<String, i64>,
    pub counts_actual: BTreeMap<String, i64>,
    pub target: String,
    pub target_is_empty: bool,
    /// Where the source transcripts were (or would be) restored.
    pub sources_dir: Option<String>,
    pub sources_restored: usize,
    pub sources_missing: Vec<String>,
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}

fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut v = Vec::new();
    f.read_to_end(&mut v).ok()?;
    Some(v)
}

/// Safe relative path inside the archive: no `..`, no root.
fn safe_rel(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path.split('/').all(|p| !p.is_empty() && p != "..")
}

/// Reads a bundle, verifies it, and with `apply` writes the project to
/// `target_db` (a path that does not exist, or a database with no project in
/// it) and restores the source transcripts next to it.
pub fn import_bundle(
    bytes: &[u8],
    target_db: &Path,
    apply: bool,
    actor: &str,
) -> Result<BundleReport> {
    let mut rep = BundleReport {
        dry_run: !apply,
        applied: false,
        ok: false,
        format: None,
        tool_version: None,
        files_checked: 0,
        files_bad: Vec::new(),
        audit_chain_ok: false,
        audit_events: 0,
        database_chain_ok: false,
        state_ok: false,
        expected_state_sha256: None,
        actual_state_sha256: None,
        counts_expected: BTreeMap::new(),
        counts_actual: BTreeMap::new(),
        target: target_db.display().to_string(),
        target_is_empty: false,
        sources_dir: None,
        sources_restored: 0,
        sources_missing: Vec::new(),
        problems: Vec::new(),
        warnings: Vec::new(),
    };
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| CsError::Validation(format!("not a project bundle (not a ZIP file: {e})")))?;
    let manifest: Value = match read_entry(&mut zip, "manifest.json") {
        Some(b) => serde_json::from_slice(&b)
            .map_err(|e| CsError::Validation(format!("manifest.json is not valid JSON: {e}")))?,
        None => {
            return Err(CsError::Validation("not a project bundle: no manifest.json".into()).into())
        }
    };
    rep.format = manifest["format"].as_str().map(str::to_string);
    rep.tool_version = manifest["tool_version"].as_str().map(str::to_string);
    if rep.format.as_deref() != Some(BUNDLE_FORMAT) {
        return Err(CsError::Validation(format!(
            "not a Coding Studio project bundle (format {:?})",
            rep.format
        ))
        .into());
    }
    if manifest["version"].as_u64() != Some(BUNDLE_VERSION as u64) {
        rep.problems.push(format!(
            "bundle layout version {} is not supported (expected {BUNDLE_VERSION})",
            manifest["version"]
        ));
    }
    rep.counts_expected = serde_json::from_value(manifest["counts"].clone()).unwrap_or_default();
    rep.expected_state_sha256 = manifest["state_sha256"].as_str().map(str::to_string);

    // 1. digests of every file
    let listed: Vec<Value> = manifest["files"].as_array().cloned().unwrap_or_default();
    let mut contents: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for f in &listed {
        let name = f["name"].as_str().unwrap_or("").to_string();
        let expected = f["sha256"].as_str().unwrap_or("").to_string();
        if !safe_rel(&name) {
            rep.problems
                .push(format!("unsafe file name {name:?} in the manifest"));
            continue;
        }
        let data = read_entry(&mut zip, &name);
        let actual = data.as_ref().map(|d| sha256_hex(d));
        rep.files_checked += 1;
        if actual.as_deref() != Some(expected.as_str()) {
            rep.files_bad.push(FileCheck {
                name: name.clone(),
                expected,
                actual,
                ok: false,
            });
            rep.problems.push(match data {
                None => format!("{name} is listed in the manifest but missing from the bundle"),
                Some(_) => format!("{name} does not match its digest: the file was modified"),
            });
        }
        if let Some(d) = data {
            contents.insert(name, d);
        }
    }
    let known: Vec<&str> = listed.iter().filter_map(|f| f["name"].as_str()).collect();
    for i in 0..zip.len() {
        if let Ok(f) = zip.by_index(i) {
            if let Ok(n) = f.name() {
                if n != "manifest.json" && !known.contains(&n.as_ref()) && !n.ends_with('/') {
                    rep.warnings.push(format!(
                        "{n} is in the bundle but not in the manifest; it is ignored"
                    ));
                }
            }
        }
    }

    // 2. the audit log of the bundle and its head
    match contents.get("audit/audit_log.jsonl") {
        None => rep.problems.push("audit/audit_log.jsonl is missing".into()),
        Some(b) => {
            let mut events = Vec::new();
            let mut parse_ok = true;
            for (i, line) in String::from_utf8_lossy(b).lines().enumerate() {
                match serde_json::from_str::<AuditEvent>(line) {
                    Ok(e) => events.push(e),
                    Err(e) => {
                        parse_ok = false;
                        rep.problems.push(format!("audit log line {}: {e}", i + 1));
                        break;
                    }
                }
            }
            let chain = verify_events(&events);
            rep.audit_events = events.len();
            rep.audit_chain_ok = parse_ok && chain.ok;
            if !chain.ok {
                rep.problems.push(format!(
                    "the audit chain of the bundle breaks at event {}",
                    chain.first_bad_seq.unwrap_or(0)
                ));
            }
            let head = events
                .last()
                .map(|e| e.hash.clone())
                .unwrap_or_else(|| GENESIS_HASH.to_string());
            if manifest["audit"]["head_hash"].as_str() != Some(head.as_str()) {
                rep.audit_chain_ok = false;
                rep.problems
                    .push("the head of the audit chain differs from the manifest".into());
            }
        }
    }

    // 3. the database: chain, state digest, counts
    let tmp = std::env::temp_dir().join(format!(
        "cs-import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&tmp)?;
    let tmp_db = tmp.join("database.sqlite");
    let outcome = (|| -> Result<()> {
        let Some(db) = contents.get("database.sqlite") else {
            rep.problems.push("database.sqlite is missing".into());
            return Ok(());
        };
        fs::write(&tmp_db, db)?;
        let store = match Store::open(&tmp_db) {
            Ok(s) => s,
            Err(e) => {
                rep.problems
                    .push(format!("the database cannot be opened: {e:#}"));
                return Ok(());
            }
        };
        // Counts and state are read before the store appends anything.
        rep.counts_actual = project_counts(&store)?;
        let ids: Vec<String> = store
            .summaries()?
            .into_iter()
            .map(|s| s.interview_id)
            .collect();
        let cases = store.cases(None)?;
        let (_, state_sha) = crate::export::state_document(&store, &ids, &cases)?;
        rep.actual_state_sha256 = Some(state_sha.clone());
        rep.state_ok = rep.expected_state_sha256.as_deref() == Some(state_sha.as_str());
        if !rep.state_ok {
            rep.problems
                .push("the state digest of the database differs from the manifest".into());
        }
        rep.database_chain_ok = store.verify_chain()?.ok;
        if !rep.database_chain_ok {
            rep.problems
                .push("the audit chain of the database is broken".into());
        }
        for (k, expected) in &rep.counts_expected {
            // The audit count may have grown by a migration on open.
            let actual = rep.counts_actual.get(k).copied().unwrap_or(-1);
            if actual != *expected && k != "audit_events" {
                rep.problems.push(format!(
                    "{k}: the manifest says {expected}, the database holds {actual}"
                ));
            }
        }
        // Source files against the digests the scan recorded.
        let mut missing = Vec::new();
        for iv in manifest["interviews"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let id = iv["interview_id"].as_str().unwrap_or("");
            for s in iv["sources"].as_array().cloned().unwrap_or_default() {
                let rel = s["rel_path"].as_str().unwrap_or("");
                if s["bundled"].as_bool() != Some(true) {
                    missing.push(format!("{id}/{rel}"));
                } else if s["changed_since_scan"].as_bool() == Some(true) {
                    rep.warnings.push(format!(
                        "{id}/{rel} had changed since the scan when the bundle was made"
                    ));
                }
            }
        }
        rep.sources_missing = missing;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&tmp);
    outcome?;

    // 4. the target
    let target_exists = target_db.exists();
    rep.target_is_empty = if target_exists {
        let existing =
            Store::open(target_db).with_context(|| format!("opening {}", target_db.display()))?;
        is_empty_project(&existing)?
    } else {
        true
    };
    if !rep.target_is_empty {
        rep.problems.push(format!(
            "{} already holds a project; import into a new database",
            target_db.display()
        ));
    }
    let stem = target_db
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());
    let sources_dir = target_db
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("{stem}_sources"));
    rep.sources_dir = Some(sources_dir.display().to_string());
    rep.ok = rep.problems.is_empty();
    if !apply || !rep.ok {
        return Ok(rep);
    }

    // 5. apply
    if let Some(parent) = target_db.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{suffix}", target_db.display()));
        if p.exists() {
            fs::remove_file(&p)?;
        }
    }
    let db = contents
        .get("database.sqlite")
        .context("database.sqlite vanished")?;
    fs::write(target_db, db)?;
    let mut restored = 0usize;
    let mut roots: BTreeMap<String, String> = BTreeMap::new();
    for iv in manifest["interviews"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let id = iv["interview_id"].as_str().unwrap_or("").to_string();
        let folder = iv["folder_name"].as_str().unwrap_or(&id).to_string();
        if !safe_rel(&folder) || folder.contains('/') {
            rep.warnings.push(format!(
                "{id}: folder name {folder:?} is unsafe; sources not restored"
            ));
            continue;
        }
        let dest_dir = sources_dir.join(&folder);
        for s in iv["sources"].as_array().cloned().unwrap_or_default() {
            let rel = s["rel_path"].as_str().unwrap_or("");
            let name = format!("sources/{id}/{rel}");
            let Some(data) = contents.get(&name) else {
                continue;
            };
            if !safe_rel(rel) {
                rep.warnings
                    .push(format!("{id}: unsafe source path {rel:?} skipped"));
                continue;
            }
            let path = dest_dir.join(rel);
            if let Some(p) = path.parent() {
                fs::create_dir_all(p)?;
            }
            fs::write(&path, data)?;
            restored += 1;
        }
        roots.insert(id, dest_dir.display().to_string());
    }
    let store = Store::open(target_db)?;
    {
        // Folder paths now point at the restored sources.
        let tx = store.connection().unchecked_transaction()?;
        for (id, dir) in &roots {
            tx.execute(
                "UPDATE interviews SET folder_path = ?1 WHERE interview_id = ?2",
                rusqlite::params![dir, id],
            )?;
        }
        tx.commit()?;
    }
    store.set_setting("library_root", &sources_dir.display().to_string(), actor)?;
    store.log(
        actor,
        "project.import",
        None,
        Some(&rep.target),
        json!({
            "bundle_sha256": sha256_hex(bytes),
            "state_sha256": rep.actual_state_sha256,
            "counts": rep.counts_actual,
            "sources_restored": restored,
            "sources_dir": sources_dir.display().to_string(),
            "from_tool_version": rep.tool_version,
        }),
    )?;
    rep.sources_restored = restored;
    rep.applied = true;
    Ok(rep)
}
