//! F-62 (V2): the manual correction of the audio offset of an interview.
//!
//! A unit's start time comes from the transcript. The recording may not start
//! where the transcript's clock does (a trimmed file, a recording started late),
//! so the researcher can shift the whole interview by a number of seconds,
//! positive or negative. The audio time of a unit is its start time plus that
//! offset. One value per interview, every change audited with before and after.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use crate::{append_event, now, validation, Store};

/// Largest correction accepted, in seconds (one day).
pub const MAX_AUDIO_OFFSET: f64 = 86_400.0;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS audio_offsets (
    interview_id    TEXT PRIMARY KEY REFERENCES interviews(interview_id),
    offset_seconds  REAL NOT NULL,
    note            TEXT NOT NULL DEFAULT '',
    updated_at      TEXT NOT NULL,
    updated_by      TEXT NOT NULL
);
";

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

/// The offset of an interview. `set` is false when the researcher never set one
/// (the offset is then 0).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioOffset {
    pub interview_id: String,
    pub offset_seconds: f64,
    pub note: String,
    pub set: bool,
    pub updated_at: Option<String>,
    pub updated_by: Option<String>,
}

impl Store {
    /// Offset of the interview's audio against its transcript clock.
    pub fn audio_offset(&self, interview_id: &str) -> Result<AudioOffset> {
        self.require_interview(interview_id)?;
        let row: Option<(f64, String, String, String)> = self
            .conn
            .query_row(
                "SELECT offset_seconds, note, updated_at, updated_by FROM audio_offsets
                 WHERE interview_id = ?1",
                [interview_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        Ok(match row {
            Some((offset_seconds, note, at, by)) => AudioOffset {
                interview_id: interview_id.to_string(),
                offset_seconds,
                note,
                set: true,
                updated_at: Some(at),
                updated_by: Some(by),
            },
            None => AudioOffset {
                interview_id: interview_id.to_string(),
                offset_seconds: 0.0,
                note: String::new(),
                set: false,
                updated_at: None,
                updated_by: None,
            },
        })
    }

    /// Sets the offset (seconds, positive when the recording runs later than
    /// the transcript clock). Unchanged values write nothing.
    pub fn set_audio_offset(
        &self,
        interview_id: &str,
        offset_seconds: f64,
        note: &str,
        actor: &str,
    ) -> Result<AudioOffset> {
        if !offset_seconds.is_finite() || offset_seconds.abs() > MAX_AUDIO_OFFSET {
            return Err(validation(format!(
                "the audio offset must be a number of seconds between -{MAX_AUDIO_OFFSET} and {MAX_AUDIO_OFFSET}"
            )));
        }
        // Milliseconds are the finest grain a player can use.
        let offset_seconds = (offset_seconds * 1000.0).round() / 1000.0;
        let note = note.trim();
        let before = self.audio_offset(interview_id)?;
        if before.offset_seconds == offset_seconds && before.note == note {
            return Ok(before);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO audio_offsets (interview_id, offset_seconds, note, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(interview_id) DO UPDATE SET offset_seconds = excluded.offset_seconds,
               note = excluded.note, updated_at = excluded.updated_at,
               updated_by = excluded.updated_by",
            params![interview_id, offset_seconds, note, now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            "audio.offset",
            Some(interview_id),
            Some(interview_id),
            json!({
                "before": {"offset_seconds": before.offset_seconds, "note": before.note},
                "after": {"offset_seconds": offset_seconds, "note": note},
            }),
        )?;
        tx.commit()?;
        self.audio_offset(interview_id)
    }
}
