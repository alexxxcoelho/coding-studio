//! SQLite persistence and the append-only, hash-chained audit trail.
//!
//! Every write method takes the `actor` and records one audit event in the same
//! transaction as the change. Protocol locks are enforced here, not in the UI:
//! - AI output import refused while human coding is incomplete (blind guard, P-04).
//! - Human coding frozen once any AI output exists for the interview.
//! - Re-segmentation refused once any coding exists for the interview.
//!
//! Protocol refusals are returned as `cs_model::CsError::Guard`, bad input as
//! `CsError::Validation`, unknown ids as `CsError::NotFound` (wrapped in anyhow).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use cs_model::{
    check_code_set, sha256_hex, AiOutput, AiRun, AiRunMeta, Arbitration, ArbitrationChoice,
    AuditEvent, ChainCheck, Code, CodeSet, Codebook, CodingMap, CsError, HumanCoding,
    InterviewEntry, InterviewSummary, PromptPart, Segmentation, SourceFile, SpeakerRole, Unit, NA,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

/// Schema applied on every open (idempotent).
pub const SCHEMA: &str = include_str!("schema.sql");

/// `prev_hash` of the first audit event.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Settings key holding the path the codebook was imported from.
const CODEBOOK_SOURCE_KEY: &str = "codebook_source";

/// `ord` given to codes created locally, so they sort after the imported codebook.
const LOCAL_ORD_BASE: i64 = 100_000;

pub struct Store {
    conn: Connection,
}

/// Result of the blind-coding guard.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GuardStatus {
    pub complete: bool,
    pub n_units: usize,
    pub missing: Vec<String>,
}

// ----------------------------------------------------------------- helpers

/// Current time, RFC 3339 UTC, seconds precision.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn validation(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Validation(msg.into()))
}

fn guard_err(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Guard(msg.into()))
}

fn not_found(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::NotFound(msg.into()))
}

/// Serialises a unit-like enum (`SourceKind`, `SpeakerRole`) to its snake_case name.
fn enum_str<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

fn enum_from<T: DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_value(Value::String(s.to_string()))
        .with_context(|| format!("unknown enum value in database: {s}"))
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("model types always serialise")
}

fn from_json<T: DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_str(s).with_context(|| format!("corrupt JSON in database: {s}"))
}

/// Trims an optional string; empty becomes `None`.
fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Trims codes and drops empty entries.
fn clean_codes(codes: &CodeSet) -> CodeSet {
    codes
        .iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// Appends one audit event on `conn` (normally inside the caller's transaction).
fn append_event(
    conn: &Connection,
    actor: &str,
    action: &str,
    interview_id: Option<&str>,
    target: Option<&str>,
    detail: Value,
) -> Result<AuditEvent> {
    let last: Option<(i64, String)> = conn
        .query_row(
            "SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (seq, prev_hash) = match last {
        Some((s, h)) => (s + 1, h),
        None => (1, GENESIS_HASH.to_string()),
    };
    // Round-trip the detail through its stored text so the hash is computed on
    // exactly what `verify_chain` will read back.
    let detail_text = to_json(&detail);
    let detail: Value = serde_json::from_str(&detail_text)?;
    let ts = now();
    let hash = AuditEvent::compute_hash(
        &prev_hash,
        seq,
        &ts,
        actor,
        action,
        interview_id,
        target,
        &detail,
    );
    conn.execute(
        "INSERT INTO audit_log (seq, ts, actor, action, interview_id, target, detail, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![seq, ts, actor, action, interview_id, target, detail_text, prev_hash, hash],
    )?;
    Ok(AuditEvent {
        seq,
        ts,
        actor: actor.to_string(),
        action: action.to_string(),
        interview_id: interview_id.map(str::to_string),
        target: target.map(str::to_string),
        detail,
        prev_hash,
        hash,
    })
}

fn row_to_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<(AuditEvent, String)> {
    let detail_text: String = r.get(6)?;
    let detail = serde_json::from_str(&detail_text).unwrap_or(Value::Null);
    Ok((
        AuditEvent {
            seq: r.get(0)?,
            ts: r.get(1)?,
            actor: r.get(2)?,
            action: r.get(3)?,
            interview_id: r.get(4)?,
            target: r.get(5)?,
            detail,
            prev_hash: r.get(7)?,
            hash: r.get(8)?,
        },
        detail_text,
    ))
}

const EVENT_COLUMNS: &str = "seq, ts, actor, action, interview_id, target, detail, prev_hash, hash";

impl Store {
    /// Opens (or creates) the database, applies the schema, `journal_mode = DELETE`
    /// (safe on synced folders), `foreign_keys = ON`.
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("cannot open database {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        let _mode: String = conn.query_row("PRAGMA journal_mode = DELETE", [], |r| r.get(0))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Raw connection, for read-only queries by callers and tests.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn require_interview(&self, interview_id: &str) -> Result<()> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM interviews WHERE interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        if n == 0 {
            return Err(not_found(format!("interview {interview_id}")));
        }
        Ok(())
    }

    // ------------------------------------------------------------ settings

    /// Sets a key/value setting. Logged as `settings.set`; no event when unchanged.
    pub fn set_setting(&self, key: &str, value: &str, actor: &str) -> Result<()> {
        let before = self.setting(key)?;
        if before.as_deref() == Some(value) {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now()],
        )?;
        append_event(
            &tx,
            actor,
            "settings.set",
            None,
            Some(key),
            json!({"key": key, "before": before, "after": value}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    // ------------------------------------------------------------- library

    fn sources(&self, interview_id: &str) -> Result<Vec<SourceFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT rel_path, kind, sha256, size, modified FROM sources
             WHERE interview_id = ?1 ORDER BY rel_path",
        )?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (rel_path, kind, sha256, size, modified) = row?;
            out.push(SourceFile {
                rel_path,
                kind: enum_from(&kind)?,
                sha256,
                size: size as u64,
                modified,
            });
        }
        Ok(out)
    }

    /// Inserts or refreshes an interview and its source inventory; logs added,
    /// removed and changed (hash) sources. A rescan that changes nothing logs nothing.
    /// A primary source already chosen is kept as long as the file is still there.
    pub fn upsert_interview(&self, entry: &InterviewEntry, actor: &str) -> Result<()> {
        let id = entry.interview_id.as_str();
        let existing = self.interview(id)?;
        let tx = self.conn.unchecked_transaction()?;
        let ts = now();

        let primary = match &existing {
            Some(old) => match &old.primary_source {
                Some(p) if entry.sources.iter().any(|s| &s.rel_path == p) => Some(p.clone()),
                _ => entry.primary_source.clone(),
            },
            None => entry.primary_source.clone(),
        };

        tx.execute(
            "INSERT INTO interviews (interview_id, folder_name, folder_path, primary_source, warnings, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(interview_id) DO UPDATE SET folder_name = excluded.folder_name,
               folder_path = excluded.folder_path, primary_source = excluded.primary_source,
               warnings = excluded.warnings, scanned_at = excluded.scanned_at",
            params![id, entry.folder_name, entry.folder_path, primary, to_json(&entry.warnings), ts],
        )?;

        let old_sources: BTreeMap<String, SourceFile> = existing
            .as_ref()
            .map(|e| {
                e.sources
                    .iter()
                    .map(|s| (s.rel_path.clone(), s.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let new_sources: BTreeMap<String, &SourceFile> = entry
            .sources
            .iter()
            .map(|s| (s.rel_path.clone(), s))
            .collect();

        let mut added = Vec::new();
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for (path, src) in &new_sources {
            match old_sources.get(path) {
                None => added.push(json!({"rel_path": path, "kind": src.kind, "sha256": src.sha256, "size": src.size})),
                Some(old) if old.sha256 != src.sha256 || old.kind != src.kind => changed.push(json!({
                    "rel_path": path, "sha256_before": old.sha256, "sha256_after": src.sha256,
                    "size_before": old.size, "size_after": src.size,
                })),
                Some(_) => {}
            }
            tx.execute(
                "INSERT INTO sources (interview_id, rel_path, kind, sha256, size, modified)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(interview_id, rel_path) DO UPDATE SET kind = excluded.kind,
                   sha256 = excluded.sha256, size = excluded.size, modified = excluded.modified",
                params![
                    id,
                    path,
                    enum_str(&src.kind),
                    src.sha256,
                    src.size as i64,
                    src.modified
                ],
            )?;
        }
        for (path, old) in &old_sources {
            if !new_sources.contains_key(path) {
                removed.push(json!({"rel_path": path, "sha256": old.sha256}));
                tx.execute(
                    "DELETE FROM sources WHERE interview_id = ?1 AND rel_path = ?2",
                    params![id, path],
                )?;
            }
        }

        match &existing {
            None => {
                append_event(
                    &tx,
                    actor,
                    "library.interview.add",
                    Some(id),
                    Some(&entry.folder_name),
                    json!({
                        "folder_name": entry.folder_name,
                        "folder_path": entry.folder_path,
                        "primary_source": primary,
                        "sources": added,
                        "warnings": entry.warnings,
                    }),
                )?;
            }
            Some(old) => {
                let moved = old.folder_path != entry.folder_path;
                if moved || !added.is_empty() || !changed.is_empty() || !removed.is_empty() {
                    append_event(
                        &tx,
                        actor,
                        "library.interview.update",
                        Some(id),
                        Some(&entry.folder_name),
                        json!({
                            "folder_path_before": old.folder_path,
                            "folder_path_after": entry.folder_path,
                            "added": added,
                            "changed": changed,
                            "removed": removed,
                            "primary_source": primary,
                        }),
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn interview(&self, interview_id: &str) -> Result<Option<InterviewEntry>> {
        let row: Option<(String, String, Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT folder_name, folder_path, primary_source, warnings FROM interviews WHERE interview_id = ?1",
                [interview_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((folder_name, folder_path, primary_source, warnings)) = row else {
            return Ok(None);
        };
        Ok(Some(InterviewEntry {
            interview_id: interview_id.to_string(),
            folder_name,
            folder_path,
            sources: self.sources(interview_id)?,
            primary_source,
            warnings: from_json(&warnings)?,
        }))
    }

    /// Chooses the transcript to segment. Must be an inventoried transcript file.
    pub fn set_primary_source(
        &self,
        interview_id: &str,
        rel_path: &str,
        actor: &str,
    ) -> Result<()> {
        let entry = self
            .interview(interview_id)?
            .ok_or_else(|| not_found(format!("interview {interview_id}")))?;
        let src = entry
            .sources
            .iter()
            .find(|s| s.rel_path == rel_path)
            .ok_or_else(|| validation(format!("{rel_path} is not a source of {interview_id}")))?;
        if !src.kind.is_transcript() {
            return Err(validation(format!(
                "{rel_path} is not a transcript and cannot be segmented"
            )));
        }
        if entry.primary_source.as_deref() == Some(rel_path) {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE interviews SET primary_source = ?2 WHERE interview_id = ?1",
            params![interview_id, rel_path],
        )?;
        append_event(
            &tx,
            actor,
            "library.primary_source.set",
            Some(interview_id),
            Some(rel_path),
            json!({"before": entry.primary_source, "after": rel_path, "sha256": src.sha256}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// One summary per interview, sorted by interview_id.
    pub fn summaries(&self) -> Result<Vec<InterviewSummary>> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT interview_id FROM interviews ORDER BY interview_id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let entry = self.interview(&id)?.expect("listed interview exists");
            let seg = self.segmentation(&id)?;
            let units: Vec<String> = seg
                .as_ref()
                .map(|s| s.units.iter().map(|u| u.unit_id.clone()).collect())
                .unwrap_or_default();
            let human = self.human_coding_map(&id)?;
            let n_human_coded = units
                .iter()
                .filter(|u| human.get(*u).is_some_and(|c| !c.is_empty()))
                .count();
            let runs = self.ai_runs(&id)?;
            let complete_runs: Vec<&AiRun> = runs.iter().filter(|r| r.complete).collect();
            // A unit diverges when any complete run gives a set different from the
            // human set. This also covers runs disagreeing among themselves, since
            // at most one of two different sets can equal the human set.
            let empty = CodeSet::new();
            let n_divergences = if complete_runs.is_empty() {
                0
            } else {
                units
                    .iter()
                    .filter(|u| {
                        let h = human.get(*u).unwrap_or(&empty);
                        complete_runs
                            .iter()
                            .any(|r| r.codings.get(*u).unwrap_or(&empty) != h)
                    })
                    .count()
            };
            let n_arbitrated: i64 = self.conn.query_row(
                "SELECT COUNT(DISTINCT unit_id) FROM arbitrations WHERE interview_id = ?1",
                [&id],
                |r| r.get(0),
            )?;
            out.push(InterviewSummary {
                interview_id: id.clone(),
                folder_name: entry.folder_name.clone(),
                n_sources: entry.sources.len(),
                primary_source: entry.primary_source.clone(),
                n_units: units.len(),
                n_human_coded,
                human_complete: !units.is_empty() && n_human_coded == units.len(),
                n_ai_runs: runs.len(),
                n_ai_runs_complete: complete_runs.len(),
                n_divergences,
                n_arbitrated: n_arbitrated as usize,
                n_warnings: entry.warnings.len(),
            });
        }
        Ok(out)
    }

    // ------------------------------------------------------------ codebook

    /// Reads `codes` from the thesis coding.db (opened read-only) and replaces the
    /// local codebook, keeping local inductive codes and `in_prompt` choices.
    /// Codes absent upstream are deactivated, never deleted.
    pub fn import_codebook(&self, coding_db: &Path, actor: &str) -> Result<Codebook> {
        let bytes = std::fs::read(coding_db)
            .with_context(|| format!("cannot read {}", coding_db.display()))?;
        let file_sha = sha256_hex(&bytes);
        let upstream: Vec<(String, String, String, String, String, bool)> = {
            let src = Connection::open_with_flags(
                coding_db,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .with_context(|| format!("cannot open {} read-only", coding_db.display()))?;
            let mut stmt = src
                .prepare(
                    "SELECT code, family, label, definition, origin, active FROM codes ORDER BY rowid",
                )
                .map_err(|e| validation(format!("{} has no usable codes table: {e}", coding_db.display())))?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(4)?
                        .unwrap_or_else(|| "codebook".to_string()),
                    r.get::<_, Option<i64>>(5)?.unwrap_or(1) != 0,
                ))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if upstream.is_empty() {
            return Err(validation(format!(
                "{} contains no codes",
                coding_db.display()
            )));
        }

        // code -> (in_prompt, local, active)
        let existing: BTreeMap<String, (bool, bool, bool)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT code, in_prompt, local, active FROM codes")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, i64>(2)? != 0,
                        r.get::<_, i64>(3)? != 0,
                    ),
                ))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        let tx = self.conn.unchecked_transaction()?;
        let mut added = Vec::new();
        let mut reactivated = Vec::new();
        let upstream_codes: BTreeSet<&str> = upstream.iter().map(|u| u.0.as_str()).collect();
        for (i, (code, family, label, definition, origin, active)) in upstream.iter().enumerate() {
            let in_prompt = match existing.get(code) {
                Some((keep, _, was_active)) => {
                    if *active && !*was_active {
                        reactivated.push(code.clone());
                    }
                    *keep
                }
                None => {
                    added.push(code.clone());
                    *active && origin != "legacy"
                }
            };
            tx.execute(
                "INSERT INTO codes (code, family, label, definition, origin, active, in_prompt, ord, local)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
                 ON CONFLICT(code) DO UPDATE SET family = excluded.family, label = excluded.label,
                   definition = excluded.definition, origin = excluded.origin,
                   active = excluded.active, in_prompt = excluded.in_prompt, ord = excluded.ord, local = 0",
                params![code, family, label, definition, origin, *active as i64, in_prompt as i64, i as i64],
            )?;
        }
        let mut deactivated = Vec::new();
        for (code, (_, local, active)) in &existing {
            if !local && *active && !upstream_codes.contains(code.as_str()) {
                tx.execute("UPDATE codes SET active = 0 WHERE code = ?1", [code])?;
                deactivated.push(code.clone());
            }
        }
        let source = coding_db.display().to_string();
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![CODEBOOK_SOURCE_KEY, source, now()],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.import",
            None,
            Some(&source),
            json!({
                "source": source,
                "sha256": file_sha,
                "n_codes": upstream.len(),
                "added": added,
                "removed": deactivated,
                "reactivated": reactivated,
            }),
        )?;
        tx.commit()?;
        self.codebook()
    }

    pub fn codebook(&self) -> Result<Codebook> {
        let mut stmt = self.conn.prepare(
            "SELECT code, family, label, definition, origin, active, in_prompt FROM codes ORDER BY ord, code",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Code {
                code: r.get(0)?,
                family: r.get(1)?,
                label: r.get(2)?,
                definition: r.get(3)?,
                origin: r.get(4)?,
                active: r.get::<_, i64>(5)? != 0,
                in_prompt: r.get::<_, i64>(6)? != 0,
            })
        })?;
        let codes = rows.collect::<rusqlite::Result<_>>()?;
        Ok(Codebook {
            source: self.setting(CODEBOOK_SOURCE_KEY)?.unwrap_or_default(),
            codes,
        })
    }

    /// New inductive code (family INDUCTIVE, origin inductive). Refuses duplicates.
    pub fn add_code(&self, code: &Code, actor: &str) -> Result<()> {
        let name = code.code.trim();
        if name.is_empty() {
            return Err(validation("code name is empty"));
        }
        if name == NA {
            return Err(validation("NA is reserved and cannot be a code"));
        }
        if name
            .chars()
            .any(|c| c.is_whitespace() || c == ';' || c == ',')
        {
            return Err(validation(
                "a code name cannot contain spaces, commas or semicolons",
            ));
        }
        if self.code_row(name)?.is_some() {
            return Err(validation(format!("code {name} already exists")));
        }
        let max_ord: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(ord), 0) FROM codes WHERE local = 1",
            [],
            |r| r.get(0),
        )?;
        let ord = max_ord.max(LOCAL_ORD_BASE) + 1;
        let stored = Code {
            code: name.to_string(),
            family: "INDUCTIVE".to_string(),
            label: code.label.trim().to_string(),
            definition: code.definition.trim().to_string(),
            origin: "inductive".to_string(),
            active: true,
            in_prompt: code.in_prompt,
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO codes (code, family, label, definition, origin, active, in_prompt, ord, local)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, 1)",
            params![stored.code, stored.family, stored.label, stored.definition, stored.origin, stored.in_prompt as i64, ord],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.code.add",
            None,
            Some(&stored.code),
            json!({"code": stored}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Whether a code is offered to the AI coder. Logged; no event when unchanged.
    pub fn set_code_in_prompt(&self, code: &str, in_prompt: bool, actor: &str) -> Result<()> {
        let current = self
            .code_row(code)?
            .ok_or_else(|| not_found(format!("code {code}")))?;
        if current.in_prompt == in_prompt {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE codes SET in_prompt = ?2 WHERE code = ?1",
            params![code, in_prompt as i64],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.code.in_prompt",
            None,
            Some(code),
            json!({"before": current.in_prompt, "after": in_prompt}),
        )?;
        tx.commit()?;
        Ok(())
    }

    fn code_row(&self, code: &str) -> Result<Option<Code>> {
        Ok(self
            .conn
            .query_row(
                "SELECT code, family, label, definition, origin, active, in_prompt FROM codes WHERE code = ?1",
                [code],
                |r| {
                    Ok(Code {
                        code: r.get(0)?,
                        family: r.get(1)?,
                        label: r.get(2)?,
                        definition: r.get(3)?,
                        origin: r.get(4)?,
                        active: r.get::<_, i64>(5)? != 0,
                        in_prompt: r.get::<_, i64>(6)? != 0,
                    })
                },
            )
            .optional()?)
    }

    /// Checks a code set: rules of `check_code_set`, every code known and active (or NA).
    fn validate_codes(&self, codes: &CodeSet) -> Result<()> {
        let mut issues = check_code_set(codes);
        for code in codes {
            if code == NA {
                continue;
            }
            match self.code_row(code)? {
                None => issues.push(format!("unknown code {code}")),
                Some(c) if !c.active => issues.push(format!("code {code} is inactive")),
                Some(_) => {}
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(validation(issues.join("; ")))
        }
    }

    // -------------------------------------------------------- segmentation

    fn current_seg_id(&self, interview_id: &str) -> Result<Option<i64>> {
        Ok(self.conn.query_row(
            "SELECT MAX(seg_id) FROM segmentations WHERE interview_id = ?1",
            [interview_id],
            |r| r.get::<_, Option<i64>>(0),
        )?)
    }

    fn has_human_coding(&self, interview_id: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM human_codings WHERE interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    fn has_ai_output(&self, interview_id: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ai_outputs o JOIN ai_runs r USING (run_id) WHERE r.interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Saves a new segmentation version. Refused once coding exists; a segmentation
    /// identical to the current one is a silent no-op.
    pub fn save_segmentation(&self, seg: &Segmentation, actor: &str) -> Result<()> {
        let id = seg.interview_id.as_str();
        self.require_interview(id)?;
        if Segmentation::units_hash(&seg.units) != seg.hash {
            return Err(validation(
                "segmentation hash does not match its units; rebuild the segmentation",
            ));
        }
        if seg.units.is_empty() {
            return Err(validation("segmentation has no units"));
        }
        let current = self.segmentation(id)?;
        if current.as_ref().is_some_and(|c| c.hash == seg.hash) {
            return Ok(());
        }
        if self.has_human_coding(id)? || self.has_ai_output(id)? {
            return Err(guard_err(format!(
                "segmentation of {id} is locked: coding already exists for its units"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO segmentations (interview_id, source_rel_path, source_sha256, rule, hash, warnings, saved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, seg.source_rel_path, seg.source_sha256, seg.rule, seg.hash, to_json(&seg.warnings), now()],
        )?;
        let seg_id = tx.last_insert_rowid();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO units (seg_id, unit_id, seq, speaker_raw, role, timestamp, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for u in &seg.units {
                stmt.execute(params![
                    seg_id,
                    u.unit_id,
                    u.seq,
                    u.speaker_raw,
                    enum_str(&u.role),
                    u.timestamp,
                    u.text
                ])
                .map_err(|e| validation(format!("unit {}: {e}", u.unit_id)))?;
            }
        }
        append_event(
            &tx,
            actor,
            "segmentation.save",
            Some(id),
            Some(&seg.source_rel_path),
            json!({
                "source_rel_path": seg.source_rel_path,
                "source_sha256": seg.source_sha256,
                "rule": seg.rule,
                "hash": seg.hash,
                "previous_hash": current.map(|c| c.hash),
                "n_units": seg.units.len(),
                "n_warnings": seg.warnings.len(),
                "warnings": seg.warnings,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Current (latest) segmentation of an interview.
    pub fn segmentation(&self, interview_id: &str) -> Result<Option<Segmentation>> {
        let Some(seg_id) = self.current_seg_id(interview_id)? else {
            return Ok(None);
        };
        let (source_rel_path, source_sha256, rule, hash, warnings): (
            String,
            String,
            String,
            String,
            String,
        ) = self.conn.query_row(
            "SELECT source_rel_path, source_sha256, rule, hash, warnings FROM segmentations WHERE seg_id = ?1",
            [seg_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT unit_id, seq, speaker_raw, role, timestamp, text FROM units WHERE seg_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([seg_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut units = Vec::new();
        for row in rows {
            let (unit_id, seq, speaker_raw, role, timestamp, text) = row?;
            units.push(Unit {
                unit_id,
                seq,
                speaker_raw,
                role: enum_from::<SpeakerRole>(&role)?,
                timestamp,
                text,
            });
        }
        Ok(Some(Segmentation {
            interview_id: interview_id.to_string(),
            source_rel_path,
            source_sha256,
            rule,
            units,
            hash,
            warnings: from_json(&warnings)?,
        }))
    }

    fn current_unit(&self, interview_id: &str, unit_id: &str) -> Result<Unit> {
        let seg = self.segmentation(interview_id)?.ok_or_else(|| {
            validation(format!(
                "{interview_id} has no segmentation yet; segment it first"
            ))
        })?;
        seg.units
            .into_iter()
            .find(|u| u.unit_id == unit_id)
            .ok_or_else(|| {
                validation(format!(
                    "unit {unit_id} is not in the current segmentation of {interview_id}"
                ))
            })
    }

    // -------------------------------------------------------- human coding

    /// Validates codes against the codebook (or NA), NA exclusivity, excerpt is a
    /// substring of the unit text; logs before/after. No event when unchanged.
    pub fn set_human_coding(
        &self,
        interview_id: &str,
        coding: &HumanCoding,
        actor: &str,
    ) -> Result<()> {
        self.require_interview(interview_id)?;
        let unit = self.current_unit(interview_id, &coding.unit_id)?;
        if self.has_ai_output(interview_id)? {
            return Err(guard_err(
                "human coding is locked: AI output already imported; changes go through arbitration",
            ));
        }
        let after = HumanCoding {
            unit_id: coding.unit_id.clone(),
            codes: clean_codes(&coding.codes),
            subcode: clean_opt(&coding.subcode),
            modifier: clean_opt(&coding.modifier),
            ev_level: clean_opt(&coding.ev_level),
            provenance: clean_opt(&coding.provenance),
            memo: clean_opt(&coding.memo),
            excerpt: clean_opt(&coding.excerpt),
        };
        self.validate_codes(&after.codes)?;
        if let Some(ex) = &after.excerpt {
            if !unit.text.contains(ex.as_str()) {
                return Err(validation(format!(
                    "excerpt is not found word for word in unit {}",
                    unit.unit_id
                )));
            }
        }
        let before = self.human_codings(interview_id)?.remove(&after.unit_id);
        if before.as_ref() == Some(&after) {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO human_codings (interview_id, unit_id, codes, subcode, modifier, ev_level, provenance, memo, excerpt, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(interview_id, unit_id) DO UPDATE SET codes = excluded.codes,
               subcode = excluded.subcode, modifier = excluded.modifier, ev_level = excluded.ev_level,
               provenance = excluded.provenance, memo = excluded.memo, excerpt = excluded.excerpt,
               updated_at = excluded.updated_at",
            params![
                interview_id,
                after.unit_id,
                to_json(&after.codes),
                after.subcode,
                after.modifier,
                after.ev_level,
                after.provenance,
                after.memo,
                after.excerpt,
                now()
            ],
        )?;
        append_event(
            &tx,
            actor,
            "coding.set",
            Some(interview_id),
            Some(&after.unit_id),
            json!({"before": before, "after": after}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn human_codings(&self, interview_id: &str) -> Result<BTreeMap<String, HumanCoding>> {
        let mut stmt = self.conn.prepare(
            "SELECT unit_id, codes, subcode, modifier, ev_level, provenance, memo, excerpt
             FROM human_codings WHERE interview_id = ?1",
        )?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (unit_id, codes, subcode, modifier, ev_level, provenance, memo, excerpt) = row?;
            out.insert(
                unit_id.clone(),
                HumanCoding {
                    unit_id,
                    codes: from_json(&codes)?,
                    subcode,
                    modifier,
                    ev_level,
                    provenance,
                    memo,
                    excerpt,
                },
            );
        }
        Ok(out)
    }

    pub fn human_coding_map(&self, interview_id: &str) -> Result<CodingMap> {
        Ok(self
            .human_codings(interview_id)?
            .into_iter()
            .map(|(k, v)| (k, v.codes))
            .collect())
    }

    /// Blind-coding guard: complete when every unit of the current segmentation
    /// carries at least one code (NA included).
    pub fn guard(&self, interview_id: &str) -> Result<GuardStatus> {
        let Some(seg) = self.segmentation(interview_id)? else {
            return Ok(GuardStatus {
                complete: false,
                n_units: 0,
                missing: Vec::new(),
            });
        };
        let human = self.human_coding_map(interview_id)?;
        let missing: Vec<String> = seg
            .units
            .iter()
            .filter(|u| human.get(&u.unit_id).is_none_or(|c| c.is_empty()))
            .map(|u| u.unit_id.clone())
            .collect();
        Ok(GuardStatus {
            complete: !seg.units.is_empty() && missing.is_empty(),
            n_units: seg.units.len(),
            missing,
        })
    }

    // ---------------------------------------------------- prompts and runs

    /// Records a generated prompt part (append-only).
    pub fn record_prompt(&self, part: &PromptPart, actor: &str) -> Result<()> {
        self.require_interview(&part.interview_id)?;
        if sha256_hex(part.text.as_bytes()) != part.sha256 {
            return Err(validation("prompt sha256 does not match its text"));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO prompts (interview_id, part, of, unit_ids, segmentation_hash, codebook_hash, prompt_version, text, sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                part.interview_id,
                part.part,
                part.of,
                to_json(&part.unit_ids),
                part.segmentation_hash,
                part.codebook_hash,
                part.prompt_version,
                part.text,
                part.sha256,
                now()
            ],
        )?;
        append_event(
            &tx,
            actor,
            "prompt.generate",
            Some(&part.interview_id),
            Some(&format!("part {}/{}", part.part, part.of)),
            json!({
                "part": part.part,
                "of": part.of,
                "sha256": part.sha256,
                "codebook_hash": part.codebook_hash,
                "segmentation_hash": part.segmentation_hash,
                "prompt_version": part.prompt_version,
                "n_units": part.unit_ids.len(),
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every prompt part ever generated for the interview, oldest first.
    pub fn prompts(&self, interview_id: &str) -> Result<Vec<PromptPart>> {
        let mut stmt = self.conn.prepare(
            "SELECT interview_id, part, of, unit_ids, segmentation_hash, codebook_hash, prompt_version, text, sha256
             FROM prompts WHERE interview_id = ?1 ORDER BY prompt_id",
        )?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (interview_id, part, of, unit_ids, sh, ch, pv, text, sha) = row?;
            out.push(PromptPart {
                interview_id,
                part,
                of,
                unit_ids: from_json(&unit_ids)?,
                segmentation_hash: sh,
                codebook_hash: ch,
                prompt_version: pv,
                text,
                sha256: sha,
            });
        }
        Ok(out)
    }

    /// Declares an AI run (R1, R2...) and its parameters. Run labels are unique per interview.
    pub fn create_ai_run(&self, interview_id: &str, meta: &AiRunMeta, actor: &str) -> Result<i64> {
        self.require_interview(interview_id)?;
        let mut meta = meta.clone();
        meta.run_label = meta.run_label.trim().to_string();
        meta.model_name = meta.model_name.trim().to_string();
        meta.model_version = meta.model_version.trim().to_string();
        meta.prompt_id_version = meta.prompt_id_version.trim().to_string();
        meta.executed_at = meta.executed_at.trim().to_string();
        meta.seed = clean_opt(&meta.seed);
        meta.notes = clean_opt(&meta.notes);
        if meta.run_label.is_empty() {
            return Err(validation("run label is required (R1, R2, R3...)"));
        }
        if meta.model_name.is_empty() {
            return Err(validation("model name is required"));
        }
        if meta.prompt_id_version.is_empty() {
            return Err(validation("prompt id and version are required"));
        }
        let taken: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ai_runs WHERE interview_id = ?1 AND run_label = ?2",
            params![interview_id, meta.run_label],
            |r| r.get(0),
        )?;
        if taken > 0 {
            return Err(validation(format!(
                "run label {} is already used for {interview_id}",
                meta.run_label
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO ai_runs (interview_id, run_label, model_name, model_version, temperature, seed,
               context_length, batch_size, prompt_id_version, executed_at, notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                interview_id,
                meta.run_label,
                meta.model_name,
                meta.model_version,
                meta.temperature,
                meta.seed,
                meta.context_length,
                meta.batch_size,
                meta.prompt_id_version,
                meta.executed_at,
                meta.notes,
                now()
            ],
        )?;
        let run_id = tx.last_insert_rowid();
        append_event(
            &tx,
            actor,
            "ai.run.create",
            Some(interview_id),
            Some(&meta.run_label),
            json!({"run_id": run_id, "meta": meta}),
        )?;
        tx.commit()?;
        Ok(run_id)
    }

    fn run_row(&self, run_id: i64) -> Result<(String, AiRunMeta)> {
        self.conn
            .query_row(
                "SELECT interview_id, run_label, model_name, model_version, temperature, seed,
                   context_length, batch_size, prompt_id_version, executed_at, notes
                 FROM ai_runs WHERE run_id = ?1",
                [run_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        AiRunMeta {
                            run_label: r.get(1)?,
                            model_name: r.get(2)?,
                            model_version: r.get(3)?,
                            temperature: r.get(4)?,
                            seed: r.get(5)?,
                            context_length: r.get(6)?,
                            batch_size: r.get(7)?,
                            prompt_id_version: r.get(8)?,
                            executed_at: r.get(9)?,
                            notes: r.get(10)?,
                        },
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| not_found(format!("AI run {run_id}")))
    }

    /// Stores the raw answer verbatim (and its hash) with the parsed codings.
    /// Refused by the blind guard; refuses a part already received for the run.
    pub fn record_ai_output(
        &self,
        run_id: i64,
        raw: &str,
        parsed: &AiOutput,
        actor: &str,
    ) -> Result<()> {
        let (interview_id, meta) = self.run_row(run_id)?;
        if parsed.interview_id != interview_id {
            return Err(validation(format!(
                "answer is for {} but run {} belongs to {interview_id}",
                parsed.interview_id, meta.run_label
            )));
        }
        let guard = self.guard(&interview_id)?;
        if !guard.complete {
            let n = if guard.n_units == 0 {
                "all".to_string()
            } else {
                guard.missing.len().to_string()
            };
            return Err(guard_err(format!(
                "blind coding guard: {n} units not yet coded by the candidate"
            )));
        }
        let seg = self
            .segmentation(&interview_id)?
            .expect("guard complete implies a segmentation");
        if parsed.segmentation_hash != seg.hash {
            return Err(guard_err(format!(
                "answer was produced on another segmentation (hash {}), current is {}",
                parsed.segmentation_hash, seg.hash
            )));
        }
        if parsed.part == 0 || parsed.of == 0 || parsed.part > parsed.of {
            return Err(validation(format!(
                "invalid part {}/{}",
                parsed.part, parsed.of
            )));
        }
        let previous: Vec<(u32, u32, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT part, of, parsed FROM ai_outputs WHERE run_id = ?1 ORDER BY output_id",
            )?;
            let rows = stmt.query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if let Some((_, of, _)) = previous.first() {
            if *of != parsed.of {
                return Err(validation(format!(
                    "run {} expects {of} parts, this answer says {}",
                    meta.run_label, parsed.of
                )));
            }
        }
        if previous.iter().any(|(p, _, _)| *p == parsed.part) {
            return Err(validation(format!(
                "part {} of run {} was already imported",
                parsed.part, meta.run_label
            )));
        }
        let unit_ids: BTreeSet<&str> = seg.units.iter().map(|u| u.unit_id.as_str()).collect();
        let mut already: BTreeSet<String> = BTreeSet::new();
        for (_, _, p) in &previous {
            let out: AiOutput = from_json(p)?;
            already.extend(out.codings.into_keys());
        }
        if parsed.codings.is_empty() {
            return Err(validation("answer contains no unit"));
        }
        for unit in parsed.codings.keys() {
            if !unit_ids.contains(unit.as_str()) {
                return Err(validation(format!("unknown unit {unit} in answer")));
            }
            if already.contains(unit) {
                return Err(validation(format!(
                    "unit {unit} already coded in another part of run {}",
                    meta.run_label
                )));
            }
        }
        let raw_sha = sha256_hex(raw.as_bytes());
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO ai_outputs (run_id, part, of, raw_text, raw_sha256, segmentation_hash, parsed, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![run_id, parsed.part, parsed.of, raw, raw_sha, parsed.segmentation_hash, to_json(parsed), now()],
        )?;
        append_event(
            &tx,
            actor,
            "ai.output.import",
            Some(&interview_id),
            Some(&meta.run_label),
            json!({
                "run_id": run_id,
                "run_label": meta.run_label,
                "part": parsed.part,
                "of": parsed.of,
                "raw_sha256": raw_sha,
                "n_units": parsed.codings.len(),
                "segmentation_hash": parsed.segmentation_hash,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// AI runs of an interview, with codings merged across the parts received.
    pub fn ai_runs(&self, interview_id: &str) -> Result<Vec<AiRun>> {
        let ids: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT run_id FROM ai_runs WHERE interview_id = ?1 ORDER BY run_id")?;
            let rows = stmt.query_map([interview_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let unit_ids: Vec<String> = self
            .segmentation(interview_id)?
            .map(|s| s.units.into_iter().map(|u| u.unit_id).collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for run_id in ids {
            let (_, meta) = self.run_row(run_id)?;
            let mut stmt = self.conn.prepare(
                "SELECT part, of, parsed FROM ai_outputs WHERE run_id = ?1 ORDER BY output_id",
            )?;
            let rows: Vec<(u32, u32, String)> = stmt
                .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let mut codings = CodingMap::new();
            let mut parts = Vec::new();
            let parts_expected = rows.first().map(|(_, of, _)| *of);
            for (part, _, parsed) in rows {
                let o: AiOutput = from_json(&parsed)?;
                codings.extend(o.codings);
                parts.push(part);
            }
            parts.sort_unstable();
            let complete = !unit_ids.is_empty() && unit_ids.iter().all(|u| codings.contains_key(u));
            out.push(AiRun {
                run_id,
                interview_id: interview_id.to_string(),
                meta,
                codings,
                parts_received: parts,
                parts_expected,
                complete,
            });
        }
        Ok(out)
    }

    /// Raw answers of a run, in the order received: (part, raw_text, sha256, imported_at).
    pub fn ai_raw_outputs(&self, run_id: i64) -> Result<Vec<(u32, String, String, String)>> {
        self.run_row(run_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT part, raw_text, raw_sha256, imported_at FROM ai_outputs WHERE run_id = ?1 ORDER BY output_id",
        )?;
        let rows = stmt.query_map([run_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // --------------------------------------------------------- arbitration

    /// Rationale must not be empty. A new decision on the same unit is appended;
    /// the latest one is current, earlier ones stay in the history.
    /// `decided_at` is set by the store (server clock); `decided_by` defaults to `actor`.
    pub fn record_arbitration(
        &self,
        interview_id: &str,
        arb: &Arbitration,
        actor: &str,
    ) -> Result<()> {
        self.require_interview(interview_id)?;
        self.current_unit(interview_id, &arb.unit_id)?;
        let rationale = arb.rationale.trim();
        if rationale.is_empty() {
            return Err(validation("a rationale is required for every arbitration"));
        }
        let final_codes = clean_codes(&arb.final_codes);
        self.validate_codes(&final_codes)?;
        if let ArbitrationChoice::Ai { run_label } = &arb.choice {
            let n: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM ai_runs WHERE interview_id = ?1 AND run_label = ?2",
                params![interview_id, run_label],
                |r| r.get(0),
            )?;
            if n == 0 {
                return Err(validation(format!("unknown AI run {run_label}")));
            }
        }
        let decided_by = if arb.decided_by.trim().is_empty() {
            actor.to_string()
        } else {
            arb.decided_by.trim().to_string()
        };
        let stored = Arbitration {
            unit_id: arb.unit_id.clone(),
            choice: arb.choice.clone(),
            final_codes,
            rationale: rationale.to_string(),
            decided_by,
            decided_at: now(),
        };
        let previous = self.arbitrations(interview_id)?.remove(&arb.unit_id);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO arbitrations (interview_id, unit_id, choice, final_codes, rationale, decided_by, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                interview_id,
                stored.unit_id,
                to_json(&stored.choice),
                to_json(&stored.final_codes),
                stored.rationale,
                stored.decided_by,
                stored.decided_at
            ],
        )?;
        append_event(
            &tx,
            actor,
            "arbitration.record",
            Some(interview_id),
            Some(&stored.unit_id),
            json!({"arbitration": stored, "previous": previous}),
        )?;
        tx.commit()?;
        Ok(())
    }

    fn query_arbitrations(&self, sql: &str, interview_id: &str) -> Result<Vec<Arbitration>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (unit_id, choice, final_codes, rationale, decided_by, decided_at) = row?;
            out.push(Arbitration {
                unit_id,
                choice: from_json(&choice)?,
                final_codes: from_json(&final_codes)?,
                rationale,
                decided_by,
                decided_at,
            });
        }
        Ok(out)
    }

    /// Current decision per unit (the latest recorded).
    pub fn arbitrations(&self, interview_id: &str) -> Result<BTreeMap<String, Arbitration>> {
        Ok(self
            .query_arbitrations(
                "SELECT unit_id, choice, final_codes, rationale, decided_by, decided_at FROM arbitrations
                 WHERE arb_id IN (SELECT MAX(arb_id) FROM arbitrations WHERE interview_id = ?1 GROUP BY unit_id)
                 ORDER BY unit_id",
                interview_id,
            )?
            .into_iter()
            .map(|a| (a.unit_id.clone(), a))
            .collect())
    }

    /// Every decision ever recorded, oldest first.
    pub fn arbitration_history(&self, interview_id: &str) -> Result<Vec<Arbitration>> {
        self.query_arbitrations(
            "SELECT unit_id, choice, final_codes, rationale, decided_by, decided_at FROM arbitrations
             WHERE interview_id = ?1 ORDER BY arb_id",
            interview_id,
        )
    }

    // --------------------------------------------------------------- audit

    /// Appends an event outside of a domain write (e.g. `export.package`).
    pub fn log(
        &self,
        actor: &str,
        action: &str,
        interview_id: Option<&str>,
        target: Option<&str>,
        detail: serde_json::Value,
    ) -> Result<AuditEvent> {
        let tx = self.conn.unchecked_transaction()?;
        let ev = append_event(&tx, actor, action, interview_id, target, detail)?;
        tx.commit()?;
        Ok(ev)
    }

    /// Audit events, oldest first; all of them or those of one interview.
    pub fn audit_events(&self, interview_id: Option<&str>) -> Result<Vec<AuditEvent>> {
        let mut out = Vec::new();
        match interview_id {
            Some(id) => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {EVENT_COLUMNS} FROM audit_log WHERE interview_id = ?1 ORDER BY seq"
                ))?;
                for row in stmt.query_map([id], row_to_event)? {
                    out.push(row?.0);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {EVENT_COLUMNS} FROM audit_log ORDER BY seq"
                ))?;
                for row in stmt.query_map([], row_to_event)? {
                    out.push(row?.0);
                }
            }
        }
        Ok(out)
    }

    /// Recomputes every hash in order and checks the links between events.
    pub fn verify_chain(&self) -> Result<ChainCheck> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {EVENT_COLUMNS} FROM audit_log ORDER BY seq"
        ))?;
        let rows = stmt.query_map([], row_to_event)?;
        let mut expected_prev = GENESIS_HASH.to_string();
        let mut n = 0usize;
        for row in rows {
            let (ev, detail_text) = row?;
            n += 1;
            let detail: Option<Value> = serde_json::from_str(&detail_text).ok();
            let ok = detail.is_some_and(|d| {
                ev.prev_hash == expected_prev
                    && AuditEvent::compute_hash(
                        &ev.prev_hash,
                        ev.seq,
                        &ev.ts,
                        &ev.actor,
                        &ev.action,
                        ev.interview_id.as_deref(),
                        ev.target.as_deref(),
                        &d,
                    ) == ev.hash
            });
            if !ok {
                return Ok(ChainCheck {
                    ok: false,
                    n_events: n,
                    first_bad_seq: Some(ev.seq),
                });
            }
            expected_prev = ev.hash;
        }
        Ok(ChainCheck {
            ok: true,
            n_events: n,
            first_bad_seq: None,
        })
    }
}

#[cfg(test)]
mod tests;
