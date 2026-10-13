//! F-24 (V2): versioned transcripts and the projection of passages.
//!
//! A transcript version is one state of the units of an interview: the
//! segmentation in force plus the unit corrections up to a given id. Every
//! change of that state (a new segmentation saved after a rescan, a unit
//! correction) appends a version and never rewrites an earlier one; an old
//! version is rebuilt from the rows that already stay (segmentations, units,
//! corrections), so nothing is stored twice.
//!
//! When a version is appended, each passage anchored on the previous text is
//! projected onto the new text with a character-level diff (see
//! [`crate::projection`]):
//!
//! - the characters of the passage survive and stay together (or the whole
//!   unit text was found unchanged in another unit): the passage is
//!   re-anchored at once, `reanchored`;
//! - the edit touches the passage, or its unit is gone: the passage stays
//!   flagged stale and a review item `needs_review` holds the old text, the
//!   new text and the proposed span, if one can be drawn. The researcher
//!   accepts the proposal, adjusts the offsets, or detaches the passage.
//!
//! A review item still open when yet another version arrives is closed as
//! `superseded` and replaced by a new one drawn from the text the passage was
//! anchored on. Every step is one audit event in the transaction of the change.

use std::collections::BTreeMap;

use anyhow::Result;
use cs_model::passage::{
    scalar_len, scalar_to_utf16, slice_scalars, text_digest, utf16_to_scalar, OffsetUnit,
};
use cs_model::{sha256_hex, SpeakerRole, Unit};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::projection::{project_span, SpanProjection};
use crate::{append_event, enum_from, enum_str, guard_err, not_found, now, validation, Store};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS transcript_versions (
    interview_id       TEXT NOT NULL REFERENCES interviews(interview_id),
    version            INTEGER NOT NULL,
    cause              TEXT NOT NULL,        -- baseline | segmentation | correction
    seg_id             INTEGER NOT NULL,
    max_correction_id  INTEGER NOT NULL DEFAULT 0,
    text_sha256        TEXT NOT NULL,
    n_units            INTEGER NOT NULL,
    target             TEXT,                 -- unit id (correction) or source file (segmentation)
    reason             TEXT,
    created_at         TEXT NOT NULL,
    created_by         TEXT NOT NULL,
    PRIMARY KEY (interview_id, version)
);

CREATE TABLE IF NOT EXISTS passage_projections (
    projection_id    INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id     TEXT NOT NULL REFERENCES interviews(interview_id),
    passage_id       TEXT NOT NULL REFERENCES passages(passage_id),
    from_version     INTEGER NOT NULL,
    to_version       INTEGER NOT NULL,
    method           TEXT NOT NULL,          -- diff | moved_unit | search
    status           TEXT NOT NULL,          -- reanchored | needs_review | accepted | adjusted | detached | superseded
    reason           TEXT NOT NULL DEFAULT '',
    old_unit_id      TEXT NOT NULL,
    old_start        INTEGER NOT NULL,
    old_end          INTEGER NOT NULL,
    old_text         TEXT NOT NULL,
    proposed_unit_id TEXT,
    proposed_start   INTEGER,
    proposed_end     INTEGER,
    proposed_text    TEXT,
    final_unit_id    TEXT,
    final_start      INTEGER,
    final_end        INTEGER,
    final_text       TEXT,
    created_at       TEXT NOT NULL,
    resolved_at      TEXT,
    resolved_by      TEXT,
    resolution_note  TEXT
);
CREATE INDEX IF NOT EXISTS idx_projections_passage ON passage_projections(passage_id, status);
CREATE INDEX IF NOT EXISTS idx_projections_interview ON passage_projections(interview_id, status);

CREATE TRIGGER IF NOT EXISTS transcript_versions_no_update BEFORE UPDATE ON transcript_versions
BEGIN SELECT RAISE(ABORT, 'append-only: transcript_versions'); END;
CREATE TRIGGER IF NOT EXISTS transcript_versions_no_delete BEFORE DELETE ON transcript_versions
BEGIN SELECT RAISE(ABORT, 'append-only: transcript_versions'); END;
CREATE TRIGGER IF NOT EXISTS passage_projections_no_delete BEFORE DELETE ON passage_projections
BEGIN SELECT RAISE(ABORT, 'append-only: passage_projections'); END;
";

/// Applies the V2 transcript tables. Safe to run any number of times.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

pub const STATUS_REANCHORED: &str = "reanchored";
pub const STATUS_NEEDS_REVIEW: &str = "needs_review";

/// One state of the transcript of an interview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TranscriptVersion {
    pub interview_id: String,
    pub version: i64,
    /// `baseline` (state found when versioning began), `segmentation` or `correction`.
    pub cause: String,
    pub text_sha256: String,
    pub n_units: usize,
    /// The unit of a correction, the source file of a segmentation.
    pub target: Option<String>,
    pub reason: Option<String>,
    pub created_at: String,
    pub created_by: String,
    /// Passages the projection onto this version re-anchored without asking.
    pub reanchored: usize,
    /// Passages it flagged for review (whatever their state now).
    pub flagged: usize,
}

/// A span of a unit text, with the browser's positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpanView {
    pub unit_id: String,
    pub start: usize,
    pub end: usize,
    pub start_utf16: Option<usize>,
    pub end_utf16: Option<usize>,
    pub text: String,
    /// The whole text of the unit the span refers to.
    pub unit_text: String,
}

/// A code of a passage under review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewCode {
    pub code_id: String,
    pub code_name: String,
}

/// One projection of a passage onto a new transcript version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectionItem {
    pub projection_id: i64,
    pub interview_id: String,
    pub passage_id: String,
    pub status: String,
    pub method: String,
    /// Why the passage needs review (empty when it was re-anchored).
    pub reason: String,
    pub from_version: i64,
    pub to_version: i64,
    pub old: SpanView,
    pub proposed: Option<SpanView>,
    #[serde(rename = "final")]
    pub final_span: Option<SpanView>,
    /// Text of the passage's unit in the current version, to adjust offsets on.
    pub current_unit_text: Option<String>,
    pub codes: Vec<ReviewCode>,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    pub resolution_note: Option<String>,
}

/// Filter of [`Store::transcript_review`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReviewFilter {
    #[serde(default)]
    pub interview_id: Option<String>,
    /// A status, or `all`. Default `needs_review`.
    #[serde(default)]
    pub status: Option<String>,
}

/// What the researcher decides about a flagged passage.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// Take the proposed span.
    Accept,
    /// Choose the span on the current text of a unit (default: the passage's unit).
    Adjust {
        unit_id: Option<String>,
        start: usize,
        end: usize,
        offset_unit: OffsetUnit,
    },
    /// Cut the passage loose from the text (it is deleted logically; its text,
    /// codes and the old reading stay in the audit trail and the review item).
    Detach,
}

/// A passage as the projection sees it.
struct PassageRow {
    passage_id: String,
    unit_id: String,
    start: usize,
    end: usize,
    text: String,
    anchor_digest: String,
}

fn units_digest(units: &[Unit]) -> String {
    let mut s = String::new();
    for u in units {
        s.push_str(&u.unit_id);
        s.push('\t');
        s.push_str(&enum_str(&u.role));
        s.push('\t');
        s.push_str(&u.text);
        s.push('\n');
    }
    sha256_hex(s.as_bytes())
}

fn span_view(unit_id: &str, start: usize, end: usize, unit_text: &str) -> SpanView {
    SpanView {
        unit_id: unit_id.to_string(),
        start,
        end,
        start_utf16: scalar_to_utf16(unit_text, start),
        end_utf16: scalar_to_utf16(unit_text, end),
        text: slice_scalars(unit_text, start, end)
            .unwrap_or_default()
            .to_string(),
        unit_text: unit_text.to_string(),
    }
}

impl Store {
    // ------------------------------------------------------------ versions

    /// Units of `seg_id` with the corrections up to `max_correction_id` applied.
    fn units_state(
        &self,
        conn: &Connection,
        interview_id: &str,
        seg_id: i64,
        max_correction_id: i64,
    ) -> Result<Vec<Unit>> {
        let mut stmt = conn.prepare(
            "SELECT unit_id, seq, speaker_raw, role, timestamp, text FROM units
             WHERE seg_id = ?1 ORDER BY seq",
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
        let mut stmt = conn.prepare(
            "SELECT unit_id, text, role FROM unit_corrections
             WHERE interview_id = ?1 AND correction_id <= ?2 ORDER BY correction_id",
        )?;
        let corrections = stmt
            .query_map(params![interview_id, max_correction_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (unit_id, text, role) in corrections {
            if let Some(u) = units.iter_mut().find(|u| u.unit_id == unit_id) {
                u.text = text;
                u.role = enum_from::<SpeakerRole>(&role)?;
                u.corrected = true;
            }
        }
        Ok(units)
    }

    fn version_row(
        &self,
        conn: &Connection,
        interview_id: &str,
        version: i64,
    ) -> Result<Option<(i64, i64, String)>> {
        Ok(conn
            .query_row(
                "SELECT seg_id, max_correction_id, text_sha256 FROM transcript_versions
                 WHERE interview_id = ?1 AND version = ?2",
                params![interview_id, version],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    fn units_at(&self, conn: &Connection, interview_id: &str, version: i64) -> Result<Vec<Unit>> {
        let (seg_id, max_corr, _) = self
            .version_row(conn, interview_id, version)?
            .ok_or_else(|| not_found(format!("version {version} of {interview_id}")))?;
        self.units_state(conn, interview_id, seg_id, max_corr)
    }

    fn latest_version_in(&self, conn: &Connection, interview_id: &str) -> Result<Option<i64>> {
        Ok(conn.query_row(
            "SELECT MAX(version) FROM transcript_versions WHERE interview_id = ?1",
            [interview_id],
            |r| r.get::<_, Option<i64>>(0),
        )?)
    }

    /// Appends a version describing the state in force now. `None` when the
    /// interview has no segmentation.
    fn record_version(
        &self,
        conn: &Connection,
        interview_id: &str,
        cause: &str,
        target: Option<&str>,
        reason: Option<&str>,
        actor: &str,
    ) -> Result<Option<i64>> {
        let seg_id: Option<i64> = conn.query_row(
            "SELECT MAX(seg_id) FROM segmentations WHERE interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        let Some(seg_id) = seg_id else {
            return Ok(None);
        };
        let max_corr: i64 = conn.query_row(
            "SELECT COALESCE(MAX(correction_id), 0) FROM unit_corrections WHERE interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        let units = self.units_state(conn, interview_id, seg_id, max_corr)?;
        let digest = units_digest(&units);
        let version = self.latest_version_in(conn, interview_id)?.unwrap_or(0) + 1;
        conn.execute(
            "INSERT INTO transcript_versions (interview_id, version, cause, seg_id, max_correction_id,
               text_sha256, n_units, target, reason, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                interview_id,
                version,
                cause,
                seg_id,
                max_corr,
                digest,
                units.len() as i64,
                target,
                reason,
                now(),
                actor
            ],
        )?;
        append_event(
            conn,
            actor,
            "transcript.version",
            Some(interview_id),
            Some(&version.to_string()),
            json!({
                "version": version,
                "cause": cause,
                "target": target,
                "reason": reason,
                "text_sha256": digest,
                "n_units": units.len(),
            }),
        )?;
        Ok(Some(version))
    }

    /// Records the state in force as version 1 when the interview has a
    /// segmentation but no version yet (a database made before versioning).
    /// Called before a change, so that the change has a previous version to
    /// project from.
    pub(crate) fn ensure_baseline_in(
        &self,
        conn: &Connection,
        interview_id: &str,
        actor: &str,
    ) -> Result<()> {
        if self.latest_version_in(conn, interview_id)?.is_none() {
            self.record_version(conn, interview_id, "baseline", None, None, actor)?;
        }
        Ok(())
    }

    /// Appends the version a change has just produced and projects the passages
    /// of the interview from the previous version onto it. Runs inside the
    /// caller's transaction, after the change was written.
    pub(crate) fn version_after_change_in(
        &self,
        conn: &Connection,
        interview_id: &str,
        cause: &str,
        target: Option<&str>,
        reason: Option<&str>,
        actor: &str,
    ) -> Result<Option<i64>> {
        let Some(version) =
            self.record_version(conn, interview_id, cause, target, reason, actor)?
        else {
            return Ok(None);
        };
        if version > 1 {
            self.project_passages_in(conn, interview_id, version - 1, version, actor)?;
        }
        Ok(Some(version))
    }

    /// Latest transcript version number of the interview, `None` before the
    /// first segmentation.
    pub fn current_transcript_version(&self, interview_id: &str) -> Result<Option<i64>> {
        self.require_interview(interview_id)?;
        self.latest_version_in(&self.conn, interview_id)
    }

    /// Versions of the transcript, oldest first, with what each did to the passages.
    pub fn transcript_versions(&self, interview_id: &str) -> Result<Vec<TranscriptVersion>> {
        self.require_interview(interview_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT version, cause, text_sha256, n_units, target, reason, created_at, created_by
             FROM transcript_versions WHERE interview_id = ?1 ORDER BY version",
        )?;
        let mut out = stmt
            .query_map([interview_id], |r| {
                Ok(TranscriptVersion {
                    interview_id: interview_id.to_string(),
                    version: r.get(0)?,
                    cause: r.get(1)?,
                    text_sha256: r.get(2)?,
                    n_units: r.get::<_, i64>(3)? as usize,
                    target: r.get(4)?,
                    reason: r.get(5)?,
                    created_at: r.get(6)?,
                    created_by: r.get(7)?,
                    reanchored: 0,
                    flagged: 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for v in &mut out {
            let (re, fl): (i64, i64) = self.conn.query_row(
                "SELECT COALESCE(SUM(status = 'reanchored'), 0),
                        COALESCE(SUM(status != 'reanchored'), 0)
                 FROM passage_projections WHERE interview_id = ?1 AND to_version = ?2",
                params![interview_id, v.version],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            v.reanchored = re as usize;
            v.flagged = fl as usize;
        }
        Ok(out)
    }

    /// The units as they read in a version.
    pub fn transcript_units_at(&self, interview_id: &str, version: i64) -> Result<Vec<Unit>> {
        self.require_interview(interview_id)?;
        self.units_at(&self.conn, interview_id, version)
    }

    // ---------------------------------------------------------- projection

    fn active_passage_rows(
        &self,
        conn: &Connection,
        interview_id: &str,
    ) -> Result<Vec<PassageRow>> {
        let mut stmt = conn.prepare(
            "SELECT passage_id, unit_id, start_offset, end_offset, text, anchor_digest
             FROM passages WHERE interview_id = ?1 AND deleted_at IS NULL
             ORDER BY passage_id",
        )?;
        let rows = stmt
            .query_map([interview_id], |r| {
                Ok(PassageRow {
                    passage_id: r.get(0)?,
                    unit_id: r.get(1)?,
                    start: r.get::<_, i64>(2)? as usize,
                    end: r.get::<_, i64>(3)? as usize,
                    text: r.get(4)?,
                    anchor_digest: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn segmentation_hash_of_version(
        &self,
        conn: &Connection,
        interview_id: &str,
        version: i64,
    ) -> Result<String> {
        let (seg_id, _, _) = self
            .version_row(conn, interview_id, version)?
            .ok_or_else(|| not_found(format!("version {version} of {interview_id}")))?;
        Ok(conn.query_row(
            "SELECT hash FROM segmentations WHERE seg_id = ?1",
            [seg_id],
            |r| r.get(0),
        )?)
    }

    /// Moves a passage onto a span of the current text and records it.
    #[allow(clippy::too_many_arguments)]
    fn move_passage_in(
        &self,
        conn: &Connection,
        passage_id: &str,
        interview_id: &str,
        unit_id: &str,
        start: usize,
        end: usize,
        unit_text: &str,
        seg_hash: &str,
        actor: &str,
    ) -> Result<String> {
        let old_unit: String = conn.query_row(
            "SELECT unit_id FROM passages WHERE passage_id = ?1",
            [passage_id],
            |r| r.get(0),
        )?;
        let text = slice_scalars(unit_text, start, end)
            .ok_or_else(|| validation(format!("span {start}..{end} is outside {unit_id}")))?
            .to_string();
        conn.execute(
            "UPDATE passages SET unit_id = ?2, start_offset = ?3, end_offset = ?4, text = ?5,
               segmentation_hash = ?6, anchor_digest = ?7
             WHERE passage_id = ?1",
            params![
                passage_id,
                unit_id,
                start as i64,
                end as i64,
                text,
                seg_hash,
                text_digest(unit_text)
            ],
        )?;
        self.sync_unit_from_passages_in(conn, interview_id, unit_id, actor)?;
        if old_unit != unit_id {
            self.sync_unit_from_passages_in(conn, interview_id, &old_unit, actor)?;
        }
        Ok(text)
    }

    fn pending_projection(
        &self,
        conn: &Connection,
        passage_id: &str,
    ) -> Result<Option<(i64, i64, String)>> {
        Ok(conn
            .query_row(
                "SELECT projection_id, from_version, old_unit_id FROM passage_projections
                 WHERE passage_id = ?1 AND status = 'needs_review'",
                [passage_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    /// Projects the active passages of the interview from version `from` onto
    /// version `to` (the one just appended).
    fn project_passages_in(
        &self,
        conn: &Connection,
        interview_id: &str,
        from: i64,
        to: i64,
        actor: &str,
    ) -> Result<()> {
        let mut versions: BTreeMap<i64, BTreeMap<String, Unit>> = BTreeMap::new();
        let mut units_of = |this: &Store, v: i64| -> Result<BTreeMap<String, Unit>> {
            if let Some(m) = versions.get(&v) {
                return Ok(m.clone());
            }
            let m: BTreeMap<String, Unit> = this
                .units_at(conn, interview_id, v)?
                .into_iter()
                .map(|u| (u.unit_id.clone(), u))
                .collect();
            versions.insert(v, m.clone());
            Ok(m)
        };
        let prev = units_of(self, from)?;
        let next = units_of(self, to)?;
        let seg_hash = self.segmentation_hash_of_version(conn, interview_id, to)?;
        for p in self.active_passage_rows(conn, interview_id)? {
            let pending = self.pending_projection(conn, &p.passage_id)?;
            // The text the passage is anchored on, and the version it comes from.
            let source: Option<(i64, String, String)> = match &pending {
                Some((_, v, old_unit)) => {
                    let m = units_of(self, *v)?;
                    m.get(old_unit)
                        .filter(|u| text_digest(&u.text) == p.anchor_digest)
                        .map(|u| (*v, u.unit_id.clone(), u.text.clone()))
                }
                None => prev
                    .get(&p.unit_id)
                    .filter(|u| text_digest(&u.text) == p.anchor_digest)
                    .map(|u| (from, u.unit_id.clone(), u.text.clone())),
            };
            let Some((src_version, old_unit_id, old_text)) = source else {
                continue;
            };
            let same_unit_now = next.get(&old_unit_id);
            if same_unit_now.is_some_and(|u| text_digest(&u.text) == p.anchor_digest) {
                // The unit reads as it did when the passage was made (or again does).
                if let Some((pid, _, _)) = pending {
                    conn.execute(
                        "UPDATE passage_projections SET status = 'superseded', resolved_at = ?2,
                           resolved_by = ?3, resolution_note = 'the unit text is back to the anchored text'
                         WHERE projection_id = ?1",
                        params![pid, now(), actor],
                    )?;
                }
                continue;
            }
            if let Some((pid, _, _)) = pending {
                conn.execute(
                    "UPDATE passage_projections SET status = 'superseded', resolved_at = ?2,
                       resolved_by = ?3, resolution_note = ?4 WHERE projection_id = ?1",
                    params![
                        pid,
                        now(),
                        actor,
                        format!("replaced by the projection onto version {to}")
                    ],
                )?;
            }
            let src_units = units_of(self, src_version)?;
            let twins: Vec<&Unit> = next
                .values()
                .filter(|u| u.role != SpeakerRole::Interviewer && u.text == old_text)
                .collect();
            let old_twins = src_units.values().filter(|u| u.text == old_text).count();

            // (method, status, reason, target unit, proposal)
            let (method, auto, reason, proposal): (
                &str,
                bool,
                &str,
                Option<(String, usize, usize)>,
            ) = if twins.len() == 1 && old_twins == 1 {
                (
                    "moved_unit",
                    true,
                    "",
                    Some((twins[0].unit_id.clone(), p.start, p.end)),
                )
            } else if let Some(nu) = same_unit_now {
                if nu.role == SpeakerRole::Interviewer {
                    ("diff", false, "unit_became_interviewer", None)
                } else {
                    match project_span(&old_text, &nu.text, p.start, p.end) {
                        SpanProjection::Unchanged { start, end } => {
                            ("diff", true, "", Some((nu.unit_id.clone(), start, end)))
                        }
                        SpanProjection::Touched { proposal } => (
                            "diff",
                            false,
                            "text_changed",
                            proposal.map(|(a, b)| (nu.unit_id.clone(), a, b)),
                        ),
                    }
                }
            } else {
                // The unit left the transcript: look for the exact words elsewhere.
                let mut hits = Vec::new();
                for u in next.values().filter(|u| u.role != SpeakerRole::Interviewer) {
                    let chars: Vec<char> = u.text.chars().collect();
                    let needle: Vec<char> = p.text.chars().collect();
                    if needle.is_empty() || chars.len() < needle.len() {
                        continue;
                    }
                    for i in 0..=(chars.len() - needle.len()) {
                        if chars[i..i + needle.len()] == needle[..] {
                            hits.push((u.unit_id.clone(), i, i + needle.len()));
                        }
                    }
                }
                if hits.len() == 1 {
                    ("search", false, "unit_moved", hits.pop())
                } else {
                    ("search", false, "unit_removed", None)
                }
            };
            let proposed_text = proposal.as_ref().and_then(|(u, a, b)| {
                next.get(u)
                    .and_then(|nu| slice_scalars(&nu.text, *a, *b))
                    .map(str::to_string)
            });
            let status = if auto {
                STATUS_REANCHORED
            } else {
                STATUS_NEEDS_REVIEW
            };
            let (pu, ps, pe) = match &proposal {
                Some((u, a, b)) => (Some(u.as_str()), Some(*a as i64), Some(*b as i64)),
                None => (None, None, None),
            };
            let ts = now();
            conn.execute(
                "INSERT INTO passage_projections (interview_id, passage_id, from_version, to_version,
                   method, status, reason, old_unit_id, old_start, old_end, old_text,
                   proposed_unit_id, proposed_start, proposed_end, proposed_text,
                   final_unit_id, final_start, final_end, final_text, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20)",
                params![
                    interview_id,
                    p.passage_id,
                    src_version,
                    to,
                    method,
                    status,
                    reason,
                    old_unit_id,
                    p.start as i64,
                    p.end as i64,
                    old_text_slice(&old_text, p.start, p.end),
                    pu,
                    ps,
                    pe,
                    proposed_text,
                    auto.then_some(pu).flatten(),
                    auto.then_some(ps).flatten(),
                    auto.then_some(pe).flatten(),
                    auto.then(|| proposed_text.clone()).flatten(),
                    ts
                ],
            )?;
            let projection_id = conn.last_insert_rowid();
            if auto {
                let (u, a, b) = proposal
                    .as_ref()
                    .expect("an automatic projection has a span");
                let unit_text = &next[u].text;
                self.move_passage_in(
                    conn,
                    &p.passage_id,
                    interview_id,
                    u,
                    *a,
                    *b,
                    unit_text,
                    &seg_hash,
                    actor,
                )?;
                append_event(
                    conn,
                    actor,
                    "passage.reanchor",
                    Some(interview_id),
                    Some(&p.passage_id),
                    json!({
                        "projection_id": projection_id,
                        "passage_id": p.passage_id,
                        "from_version": src_version,
                        "to_version": to,
                        "method": method,
                        "before": {"unit_id": old_unit_id, "start": p.start, "end": p.end},
                        "after": {"unit_id": u, "start": a, "end": b},
                        "text": p.text,
                    }),
                )?;
            } else {
                append_event(
                    conn,
                    actor,
                    "passage.review_flag",
                    Some(interview_id),
                    Some(&p.passage_id),
                    json!({
                        "projection_id": projection_id,
                        "passage_id": p.passage_id,
                        "from_version": src_version,
                        "to_version": to,
                        "method": method,
                        "reason": reason,
                        "old": {"unit_id": old_unit_id, "start": p.start, "end": p.end, "text": p.text},
                        "proposed": proposal.as_ref().map(|(u, a, b)| json!({
                            "unit_id": u, "start": a, "end": b, "text": proposed_text,
                        })),
                    }),
                )?;
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------- review

    fn projection_item(
        &self,
        r: &ProjectionRow,
        cache: &mut BTreeMap<(String, i64), BTreeMap<String, Unit>>,
        codes: &BTreeMap<String, Vec<ReviewCode>>,
    ) -> Result<ProjectionItem> {
        let mut text_of = |this: &Store, version: i64, unit: &str| -> Result<Option<String>> {
            let key = (r.interview_id.clone(), version);
            if !cache.contains_key(&key) {
                let m = this
                    .units_at(&this.conn, &r.interview_id, version)?
                    .into_iter()
                    .map(|u| (u.unit_id.clone(), u))
                    .collect();
                cache.insert(key.clone(), m);
            }
            Ok(cache[&key].get(unit).map(|u| u.text.clone()))
        };
        let old_unit_text = text_of(self, r.from_version, &r.old_unit_id)?.unwrap_or_default();
        let old = span_view(&r.old_unit_id, r.old_start, r.old_end, &old_unit_text);
        let mut span = |unit: &Option<String>, s: Option<usize>, e: Option<usize>| {
            let (Some(u), Some(s), Some(e)) = (unit, s, e) else {
                return Ok::<_, anyhow::Error>(None);
            };
            let t = text_of(self, r.to_version, u)?.unwrap_or_default();
            Ok(Some(span_view(u, s, e, &t)))
        };
        let proposed = span(&r.proposed_unit_id, r.proposed_start, r.proposed_end)?;
        let final_span = span(&r.final_unit_id, r.final_start, r.final_end)?;
        let current_unit_text =
            self.current_transcript_unit_text(&r.interview_id, &r.passage_unit_id)?;
        Ok(ProjectionItem {
            projection_id: r.projection_id,
            interview_id: r.interview_id.clone(),
            passage_id: r.passage_id.clone(),
            status: r.status.clone(),
            method: r.method.clone(),
            reason: r.reason.clone(),
            from_version: r.from_version,
            to_version: r.to_version,
            old,
            proposed,
            final_span,
            current_unit_text,
            codes: codes.get(&r.passage_id).cloned().unwrap_or_default(),
            created_at: r.created_at.clone(),
            resolved_at: r.resolved_at.clone(),
            resolved_by: r.resolved_by.clone(),
            resolution_note: r.resolution_note.clone(),
        })
    }

    fn current_transcript_unit_text(
        &self,
        interview_id: &str,
        unit_id: &str,
    ) -> Result<Option<String>> {
        Ok(self
            .current_units(interview_id)?
            .get(unit_id)
            .map(|u| u.text.clone()))
    }

    fn projection_rows(
        &self,
        interview_id: Option<&str>,
        status: Option<&str>,
        projection_id: Option<i64>,
    ) -> Result<Vec<ProjectionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT pp.projection_id, pp.interview_id, pp.passage_id, pp.from_version, pp.to_version,
                    pp.method, pp.status, pp.reason, pp.old_unit_id, pp.old_start, pp.old_end,
                    pp.proposed_unit_id, pp.proposed_start, pp.proposed_end,
                    pp.final_unit_id, pp.final_start, pp.final_end,
                    pp.created_at, pp.resolved_at, pp.resolved_by, pp.resolution_note, p.unit_id,
                    p.deleted_at IS NOT NULL
             FROM passage_projections pp JOIN passages p USING (passage_id)
             WHERE (?1 IS NULL OR pp.interview_id = ?1)
               AND (?2 IS NULL OR pp.status = ?2)
               AND (?3 IS NULL OR pp.projection_id = ?3)
             ORDER BY pp.projection_id",
        )?;
        let rows = stmt
            .query_map(params![interview_id, status, projection_id], |r| {
                let opt = |i: usize| r.get::<_, Option<i64>>(i).map(|v| v.map(|x| x as usize));
                Ok(ProjectionRow {
                    projection_id: r.get(0)?,
                    interview_id: r.get(1)?,
                    passage_id: r.get(2)?,
                    from_version: r.get(3)?,
                    to_version: r.get(4)?,
                    method: r.get(5)?,
                    status: r.get(6)?,
                    reason: r.get(7)?,
                    old_unit_id: r.get(8)?,
                    old_start: r.get::<_, i64>(9)? as usize,
                    old_end: r.get::<_, i64>(10)? as usize,
                    proposed_unit_id: r.get(11)?,
                    proposed_start: opt(12)?,
                    proposed_end: opt(13)?,
                    final_unit_id: r.get(14)?,
                    final_start: opt(15)?,
                    final_end: opt(16)?,
                    created_at: r.get(17)?,
                    resolved_at: r.get(18)?,
                    resolved_by: r.get(19)?,
                    resolution_note: r.get(20)?,
                    passage_unit_id: r.get(21)?,
                    passage_deleted: r.get(22)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn items_for(
        &self,
        rows: Vec<ProjectionRow>,
        keep_deleted: bool,
    ) -> Result<Vec<ProjectionItem>> {
        let names = crate::codetree::code_names(&self.conn)?;
        let mut stmt = self.conn.prepare(
            "SELECT passage_id, code_id FROM passage_codings
             WHERE removed_at IS NULL ORDER BY coding_id",
        )?;
        let mut codes: BTreeMap<String, Vec<ReviewCode>> = BTreeMap::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (pid, code_id) = row?;
            let code_name = if code_id == cs_model::passage::LATER_ID {
                cs_model::passage::LATER_NAME.to_string()
            } else {
                names
                    .get(&code_id)
                    .cloned()
                    .unwrap_or_else(|| code_id.clone())
            };
            codes
                .entry(pid)
                .or_default()
                .push(ReviewCode { code_id, code_name });
        }
        let mut cache = BTreeMap::new();
        let mut out = Vec::new();
        for r in rows {
            if r.passage_deleted && !keep_deleted {
                continue;
            }
            out.push(self.projection_item(&r, &mut cache, &codes)?);
        }
        Ok(out)
    }

    /// Passages the projection could not move on its own (default), or any
    /// status (`all`, `reanchored`, `accepted`, `adjusted`, `detached`,
    /// `superseded`). Passages deleted since are left out of the open list.
    pub fn transcript_review(&self, filter: &ReviewFilter) -> Result<Vec<ProjectionItem>> {
        if let Some(id) = &filter.interview_id {
            self.require_interview(id)?;
        }
        let status = match filter.status.as_deref() {
            None | Some("") => Some(STATUS_NEEDS_REVIEW),
            Some("all") => None,
            Some(s @ ("reanchored" | "needs_review" | "accepted" | "adjusted" | "detached"
            | "superseded")) => Some(s),
            Some(other) => {
                return Err(validation(format!(
                    "unknown status {other}; use needs_review, reanchored, accepted, adjusted, detached, superseded or all"
                )))
            }
        };
        let rows = self.projection_rows(filter.interview_id.as_deref(), status, None)?;
        self.items_for(rows, status != Some(STATUS_NEEDS_REVIEW))
    }

    /// One review item.
    pub fn transcript_review_item(&self, projection_id: i64) -> Result<ProjectionItem> {
        let rows = self.projection_rows(None, None, Some(projection_id))?;
        if rows.is_empty() {
            return Err(not_found(format!("projection {projection_id}")));
        }
        Ok(self.items_for(rows, true)?.remove(0))
    }

    /// Settles a flagged passage: accept the proposed span, set the span by
    /// hand, or detach the passage. One transaction, audited.
    pub fn resolve_projection(
        &self,
        projection_id: i64,
        resolution: &Resolution,
        note: &str,
        actor: &str,
    ) -> Result<ProjectionItem> {
        let item = self.transcript_review_item(projection_id)?;
        if item.status != STATUS_NEEDS_REVIEW {
            return Err(guard_err(format!(
                "projection {projection_id} is already {}",
                item.status
            )));
        }
        let rows = self.projection_rows(None, None, Some(projection_id))?;
        if rows[0].passage_deleted {
            return Err(guard_err(format!(
                "passage {} was deleted; nothing to settle",
                item.passage_id
            )));
        }
        let interview_id = item.interview_id.clone();
        let current = self.current_units(&interview_id)?;
        let seg_hash = self
            .segmentation(&interview_id)?
            .map(|s| s.hash)
            .unwrap_or_default();
        let note = note.trim();
        let tx = self.conn.unchecked_transaction()?;
        let (status, target): (&str, Option<(String, usize, usize)>) = match resolution {
            Resolution::Accept => {
                let Some(p) = &item.proposed else {
                    return Err(validation(
                        "there is no proposed span to accept; adjust the offsets or detach the passage",
                    ));
                };
                ("accepted", Some((p.unit_id.clone(), p.start, p.end)))
            }
            Resolution::Adjust {
                unit_id,
                start,
                end,
                offset_unit,
            } => {
                let unit_id = unit_id
                    .clone()
                    .unwrap_or_else(|| rows[0].passage_unit_id.clone());
                let unit = current.get(&unit_id).ok_or_else(|| {
                    validation(format!("unit {unit_id} is not in the current transcript"))
                })?;
                if unit.role == SpeakerRole::Interviewer {
                    return Err(validation(format!(
                        "unit {unit_id} is an interviewer turn and is never coded"
                    )));
                }
                let (s, e) = match offset_unit {
                    OffsetUnit::Scalar => (*start, *end),
                    OffsetUnit::Utf16 => {
                        let conv = |x: usize| {
                            utf16_to_scalar(&unit.text, x).ok_or_else(|| {
                                validation(format!(
                                    "UTF-16 position {x} is outside the unit or inside a character"
                                ))
                            })
                        };
                        (conv(*start)?, conv(*end)?)
                    }
                };
                if s >= e {
                    return Err(validation(format!(
                        "the span {s}..{e} is empty or reversed"
                    )));
                }
                let slice = slice_scalars(&unit.text, s, e).ok_or_else(|| {
                    validation(format!(
                        "the span {s}..{e} is outside unit {unit_id} (length {})",
                        scalar_len(&unit.text)
                    ))
                })?;
                if slice.trim().is_empty() {
                    return Err(validation("a passage cannot be only blanks"));
                }
                ("adjusted", Some((unit_id, s, e)))
            }
            Resolution::Detach => ("detached", None),
        };
        let final_text;
        match &target {
            Some((unit_id, s, e)) => {
                let unit = current.get(unit_id).ok_or_else(|| {
                    validation(format!("unit {unit_id} is not in the current transcript"))
                })?;
                let digest = text_digest(&unit.text);
                let twin: Option<String> = tx
                    .query_row(
                        "SELECT passage_id FROM passages
                         WHERE interview_id = ?1 AND unit_id = ?2 AND start_offset = ?3
                           AND end_offset = ?4 AND anchor_digest = ?5 AND deleted_at IS NULL
                           AND passage_id != ?6",
                        params![
                            interview_id,
                            unit_id,
                            *s as i64,
                            *e as i64,
                            digest,
                            item.passage_id
                        ],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(other) = twin {
                    return Err(validation(format!(
                        "passage {other} already covers {s}..{e} of {unit_id}; detach this one instead"
                    )));
                }
                final_text = self.move_passage_in(
                    &tx,
                    &item.passage_id,
                    &interview_id,
                    unit_id,
                    *s,
                    *e,
                    &unit.text,
                    &seg_hash,
                    actor,
                )?;
                append_event(
                    &tx,
                    actor,
                    "passage.reanchor",
                    Some(&interview_id),
                    Some(&item.passage_id),
                    json!({
                        "projection_id": projection_id,
                        "passage_id": item.passage_id,
                        "from_version": item.from_version,
                        "to_version": item.to_version,
                        "method": status,
                        "before": {"unit_id": item.old.unit_id, "start": item.old.start,
                                   "end": item.old.end, "text": item.old.text},
                        "after": {"unit_id": unit_id, "start": s, "end": e, "text": final_text},
                    }),
                )?;
                tx.execute(
                    "UPDATE passage_projections SET status = ?2, final_unit_id = ?3, final_start = ?4,
                       final_end = ?5, final_text = ?6, resolved_at = ?7, resolved_by = ?8,
                       resolution_note = ?9 WHERE projection_id = ?1",
                    params![projection_id, status, unit_id, *s as i64, *e as i64, final_text, now(), actor, note],
                )?;
            }
            None => {
                tx.execute(
                    "UPDATE passages SET deleted_at = ?2, deleted_by = ?3, delete_reason = ?4
                     WHERE passage_id = ?1",
                    params![
                        item.passage_id,
                        now(),
                        actor,
                        format!("detached at transcript version {}: {note}", item.to_version)
                    ],
                )?;
                self.sync_unit_from_passages_in(
                    &tx,
                    &interview_id,
                    &rows[0].passage_unit_id,
                    actor,
                )?;
                append_event(
                    &tx,
                    actor,
                    "passage.detach",
                    Some(&interview_id),
                    Some(&item.passage_id),
                    json!({
                        "projection_id": projection_id,
                        "passage_id": item.passage_id,
                        "to_version": item.to_version,
                        "old": {"unit_id": item.old.unit_id, "start": item.old.start,
                                "end": item.old.end, "text": item.old.text},
                        "codes": item.codes.iter().map(|c| c.code_id.clone()).collect::<Vec<_>>(),
                        "note": note,
                    }),
                )?;
                tx.execute(
                    "UPDATE passage_projections SET status = 'detached', resolved_at = ?2,
                       resolved_by = ?3, resolution_note = ?4 WHERE projection_id = ?1",
                    params![projection_id, now(), actor, note],
                )?;
            }
        }
        append_event(
            &tx,
            actor,
            "transcript.projection.resolve",
            Some(&interview_id),
            Some(&item.passage_id),
            json!({
                "projection_id": projection_id,
                "decision": status,
                "note": note,
            }),
        )?;
        tx.commit()?;
        self.transcript_review_item(projection_id)
    }
}

fn old_text_slice(unit_text: &str, start: usize, end: usize) -> String {
    slice_scalars(unit_text, start, end)
        .unwrap_or_default()
        .to_string()
}

/// Row of `passage_projections` joined with its passage.
struct ProjectionRow {
    projection_id: i64,
    interview_id: String,
    passage_id: String,
    from_version: i64,
    to_version: i64,
    method: String,
    status: String,
    reason: String,
    old_unit_id: String,
    old_start: usize,
    old_end: usize,
    proposed_unit_id: Option<String>,
    proposed_start: Option<usize>,
    proposed_end: Option<usize>,
    final_unit_id: Option<String>,
    final_start: Option<usize>,
    final_end: Option<usize>,
    created_at: String,
    resolved_at: Option<String>,
    resolved_by: Option<String>,
    resolution_note: Option<String>,
    passage_unit_id: String,
    passage_deleted: bool,
}
