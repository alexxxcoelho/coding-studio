//! SQLite persistence and the append-only, hash-chained audit trail.
//!
//! Every write method takes the `actor` and records one audit event in the same
//! transaction as the change. The rules of the coding procedure are enforced
//! here, not in the UI:
//! - Phase 1 is the candidate's provisional TOE pre-classification (TECH, ORG,
//!   ENV, CONTEXT or NA), always editable, never challenged by the AI.
//! - Phase 2 codes the units whose phase 1 category is not NA with concrete
//!   codes; edits stay possible after an AI answer and are then flagged
//!   `revised_after_ai`.
//! - Blind guard (P-04): an AI answer is imported only when every unit of its
//!   prompt part carries the candidate's phase 2 coding.
//! - Re-segmentation is refused once any coding exists.
//!
//! Protocol refusals are returned as `cs_model::CsError::Guard`, bad input as
//! `CsError::Validation`, unknown ids as `CsError::NotFound` (wrapped in anyhow).

mod analysis_index;
mod analysis_schema;
mod cases;
mod claims;
mod codebook;
mod codetree;
mod coding;
mod final_audit;
mod lexicon;
mod library;
mod links;
mod matrix;
mod memos;
mod passages;
mod project;
mod review;
mod saturation;
mod simple;
mod study_config;
mod themes;
mod verification;
mod writing;

pub use codebook::{CodePatch, ImportReport, MergeOutcome};
pub use codetree::{
    CodeMemoVersion, CodeUpdate, NewCode, NewCodeFromPassage, ResolvedCode, Revision,
    UndoCreateOutcome,
};
pub use coding::ResolveRequest;
pub use final_audit::{AuditChainLink, AuditClaim, AuditGap, FinalAudit};
pub use lexicon::{CodeRef, LexiconConfig, SimilarItem, SimilarOptions, LEXICON_CONFIG_KEY};
pub use library::{
    LibraryProgress, LibraryReview, LibraryRow, LibraryTotals, PassageQualify, UnitMark,
    MARK_TO_REVIEW,
};
pub use links::{LinkView, PassageBrief};
pub use matrix::{
    CellDetail, CellPassage, CellPatch, Matrix, MatrixAxis, MatrixCell, MatrixConfig, MatrixEntry,
    MatrixFilter, MatrixOptions, MatrixViewRecord,
};
pub use passages::{NewPassage, PassageFilter};
pub use project::{
    AttributeView, ChecklistItemView, ChecklistView, MemoAnswer, MemoOverrides, MemoQuestionView,
    MemoView, TemplateChange, TemplateReport,
};
pub use review::{Review, ReviewEntry};
pub use simple::SimpleProgress;
pub use themes::{
    NewTheme, ThemeCheckView, ThemeCodeView, ThemeCounts, ThemeHistoryEntry, ThemePassageView,
    ThemePatch, ThemeView,
};
pub use writing::{
    FrameworkCellReading, FrameworkReading, FrameworkRow, FrameworkTable, PartEdit, PartState,
    QuestionSynthesis, ThemeWriting,
};

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cs_model::{
    AuditEvent, ChainCheck, CodeSet, CsError, InterviewEntry, InterviewSummary, Segmentation,
    SourceFile, SpeakerRole, Unit, PHASE_1, PHASE_2,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

/// Schema applied on every open (idempotent).
pub const SCHEMA: &str = include_str!("schema.sql");

/// Schema version stored in `PRAGMA user_version`. Versions 1 and 2 used the
/// earlier, locked two-phase procedure. Version 4 adds the V1 lot 1 and 2
/// tables and columns to version 3, which is migrated in place. Version 5 is
/// reserved for the lexical and review lots. Version 6 adds the tables of
/// lot 5 (Analyse and Write) and the columns of the API gaps; its migration
/// (`analysis_schema::migrate`) is idempotent and runs on every open, so the
/// result does not depend on the order the lots are merged in.
pub const SCHEMA_VERSION: i64 = 6;

/// Oldest schema version that is migrated in place.
const MIN_SCHEMA_VERSION: i64 = 3;

/// `prev_hash` of the first audit event.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub struct Store {
    conn: Connection,
    /// Lexical index of the corpus with the fingerprint it was built for
    /// (see `lexicon.rs`). Never persisted.
    lex_cache: Mutex<Option<(String, Arc<cs_lex::LexIndex>)>>,
}

/// Result of the blind-coding guard of one phase.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GuardStatus {
    pub phase: u8,
    pub complete: bool,
    /// Units in scope for the phase (see `Store::phase_units`).
    pub n_units: usize,
    pub missing: Vec<String>,
}

// ----------------------------------------------------------------- helpers

/// Adds the V1 lot 1 columns to the `codes` table of an older database.
fn migrate_codes_v4(conn: &Connection) -> Result<()> {
    let have: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('codes')")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    for (column, ddl) in [
        ("code_id", "TEXT"),
        ("name", "TEXT"),
        ("parent_id", "TEXT"),
        ("includes", "TEXT NOT NULL DEFAULT ''"),
        ("excludes", "TEXT NOT NULL DEFAULT ''"),
        ("origin_passage_id", "TEXT"),
        ("example_passage_id", "TEXT"),
    ] {
        if !have.iter().any(|n| n == column) {
            conn.execute_batch(&format!("ALTER TABLE codes ADD COLUMN {column} {ddl};"))?;
        }
    }
    Ok(())
}

/// Current time, RFC 3339 UTC, seconds precision.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub(crate) fn validation(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Validation(msg.into()))
}

pub(crate) fn guard_err(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Guard(msg.into()))
}

pub(crate) fn not_found(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::NotFound(msg.into()))
}

pub(crate) fn is_not_found(err: &anyhow::Error) -> bool {
    matches!(err.downcast_ref::<CsError>(), Some(CsError::NotFound(_)))
}

pub(crate) fn check_phase(phase: u8) -> Result<()> {
    if phase == PHASE_1 || phase == PHASE_2 {
        Ok(())
    } else {
        Err(validation(format!("unknown phase {phase}; use 1 or 2")))
    }
}

/// Serialises a unit-like enum (`SourceKind`, `SpeakerRole`) to its snake_case name.
pub(crate) fn enum_str<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

pub(crate) fn enum_from<T: DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_value(Value::String(s.to_string()))
        .with_context(|| format!("unknown enum value in database: {s}"))
}

pub(crate) fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("model types always serialise")
}

pub(crate) fn from_json<T: DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_str(s).with_context(|| format!("corrupt JSON in database: {s}"))
}

/// Trims an optional string; empty becomes `None`.
pub(crate) fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Trims codes and drops empty entries.
pub(crate) fn clean_codes(codes: &CodeSet) -> CodeSet {
    codes
        .iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// Appends one audit event on `conn` (normally inside the caller's transaction).
pub(crate) fn append_event(
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
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let n_tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if n_tables > 0 && version < MIN_SCHEMA_VERSION {
            return Err(validation(
                "this database was created by an earlier version of Coding Studio; start a new database",
            ));
        }
        // Version 3 databases lack the V1 columns of `codes`: added before the
        // schema runs, so that its indexes and the backfill find them.
        let had_codes: bool = n_tables > 0
            && conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'codes'")?
                .exists([])?;
        if had_codes {
            migrate_codes_v4(&conn)?;
        }
        conn.execute_batch(SCHEMA)?;
        analysis_schema::migrate(&conn)?;
        // Databases made before providers existed lack the column: added in
        // place, so that their content is kept.
        let has_provider: bool = conn
            .prepare("SELECT name FROM pragma_table_info('ai_runs')")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|n| n == "provider");
        if !has_provider {
            conn.execute_batch("ALTER TABLE ai_runs ADD COLUMN provider TEXT;")?;
        }
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_codes_code_id ON codes(code_id);",
        )?;
        let n_identified = if had_codes || n_tables == 0 {
            codetree::backfill_identity(&conn)?
        } else {
            0
        };
        if n_identified > 0 && version > 0 && version < SCHEMA_VERSION {
            append_event(
                &conn,
                "system",
                "schema.migrate",
                None,
                None,
                json!({"from": version, "to": SCHEMA_VERSION, "codes_identified": n_identified}),
            )?;
        }
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        Ok(Store {
            conn,
            lex_cache: Mutex::new(None),
        })
    }

    /// Raw connection, for read-only queries by callers and tests.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn require_interview(&self, interview_id: &str) -> Result<()> {
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

    /// One summary per interview, sorted by interview_id, with the progress of
    /// each coding phase.
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
            let n_units = self
                .segmentation(&id)?
                .map(|s| s.units.len())
                .unwrap_or_default();
            out.push(InterviewSummary {
                interview_id: id.clone(),
                folder_name: entry.folder_name.clone(),
                n_sources: entry.sources.len(),
                primary_source: entry.primary_source.clone(),
                n_units,
                n_warnings: entry.warnings.len(),
                phase1: self.phase_summary(&id, PHASE_1)?,
                phase2: self.phase_summary(&id, PHASE_2)?,
            });
        }
        Ok(out)
    }

    // -------------------------------------------------------- segmentation

    // -------------------------------------------------------- segmentation

    pub(crate) fn current_seg_id(&self, interview_id: &str) -> Result<Option<i64>> {
        Ok(self.conn.query_row(
            "SELECT MAX(seg_id) FROM segmentations WHERE interview_id = ?1",
            [interview_id],
            |r| r.get::<_, Option<i64>>(0),
        )?)
    }

    /// Any AI output.
    fn has_ai_output(&self, interview_id: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ai_outputs o JOIN ai_runs r USING (run_id) WHERE r.interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// With an imported AI output, a re-segmentation may add units but not
    /// change or drop a unit that an AI run has answered (text and role kept).
    fn check_answered_units_kept(
        &self,
        interview_id: &str,
        current: Option<&Segmentation>,
        units: &[Unit],
    ) -> Result<()> {
        let answered: std::collections::BTreeSet<String> = self
            .ai_runs(interview_id)?
            .into_iter()
            .filter(|r| !r.parts_received.is_empty())
            .flat_map(|r| r.unit_ids)
            .collect();
        let Some(current) = current else {
            return Ok(());
        };
        for unit_id in answered {
            let before = current.units.iter().find(|u| u.unit_id == unit_id);
            let after = units.iter().find(|u| u.unit_id == unit_id);
            let same = matches!((before, after), (Some(b), Some(a)) if a.text == b.text && a.role == b.role);
            if !same {
                return Err(guard_err(format!(
                    "segmentation of {interview_id} is locked: unit {unit_id} was answered by an AI run and would change"
                )));
            }
        }
        Ok(())
    }

    /// Saves a new segmentation version. Candidate codings are kept (units keep
    /// their codings by unit id) and the units without a phase 1 coding get one
    /// automatically (see `simple`), audited once as `coding.auto_phase1`. Once
    /// an AI output exists, the units it answered must keep their text and role
    /// (see `check_answered_units_kept`). A segmentation identical to the current
    /// one is a silent no-op.
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
        if self.has_ai_output(id)? {
            self.check_answered_units_kept(id, current.as_ref(), &seg.units)?;
        }
        let categories = self.study_config()?.effective_categories();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO segmentations (interview_id, source_rel_path, source_sha256, rule, hash, warnings, saved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, seg.source_rel_path, seg.source_sha256, seg.rule, seg.hash, to_json(&seg.warnings), now()],
        )?;
        let seg_id = tx.last_insert_rowid();
        let filled = simple::fill_phase1(&tx, id, &seg.units, &categories)?;
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
        if filled.n_filled > 0 {
            append_event(
                &tx,
                actor,
                "coding.auto_phase1",
                Some(id),
                Some(id),
                json!({
                    "n_filled": filled.n_filled,
                    "n_na": filled.n_na,
                    "n_context": filled.n_context,
                    "n_kept": filled.n_kept,
                }),
            )?;
        }
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
                corrected: false,
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

    pub(crate) fn current_unit(&self, interview_id: &str, unit_id: &str) -> Result<Unit> {
        let seg = self.segmentation(interview_id)?.ok_or_else(|| {
            validation(format!(
                "{interview_id} has no segmentation yet; segment it first"
            ))
        })?;
        let mut unit = seg
            .units
            .into_iter()
            .find(|u| u.unit_id == unit_id)
            .ok_or_else(|| {
                validation(format!(
                    "unit {unit_id} is not in the current segmentation of {interview_id}"
                ))
            })?;
        if let Some(latest) = self.corrections_where(interview_id, Some(unit_id))?.pop() {
            verification::apply_correction(&mut unit, &latest);
        }
        Ok(unit)
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
