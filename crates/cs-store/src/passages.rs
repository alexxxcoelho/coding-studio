//! V1 lot 1: passages (spans of a unit), their codings, highlights and the
//! LATER marker.
//!
//! - A passage stores interview, unit, start and end (Unicode scalar
//!   positions, end exclusive), the exact text, the segmentation hash and the
//!   SHA-256 of the unit text it was anchored on. A passage is stale when the
//!   current unit text no longer has that digest, or when the unit left the
//!   current segmentation. A stale passage is flagged, never moved.
//! - Several codes per passage; the same span is the same passage, an
//!   overlapping span is another one. A coding is removed logically.
//! - A passage without active coding is a highlight. Interviewer units can
//!   carry no passage at all.
//! - The reserved code "Plus tard" (`LATER`) parks a passage with a note.
//! - Every write is one transaction with its audit event.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::passage::{
    scalar_len, scalar_to_utf16, slice_scalars, text_digest, utf16_to_scalar, OffsetUnit, Passage,
    PassageCoding, LATER_ID, LATER_NAME,
};
use cs_model::{SpeakerRole, Unit};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::json;

use crate::{append_event, not_found, now, validation, Store};

/// Scalars of context kept before and after a passage.
const CONTEXT: usize = 60;

/// A span to create or find. Without `start` and `end` it covers the whole
/// unit. `text`, when given, must be what the offsets select.
#[derive(Debug, Clone, Deserialize)]
pub struct NewPassage {
    pub interview_id: String,
    pub unit_id: String,
    #[serde(default)]
    pub start: Option<usize>,
    #[serde(default)]
    pub end: Option<usize>,
    #[serde(default)]
    pub offset_unit: OffsetUnit,
    #[serde(default)]
    pub text: Option<String>,
}

/// Filter of [`Store::passages`]. `code` is an id, name, alias or token;
/// `kind` is `highlight`, `coded` or `later`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PassageFilter {
    #[serde(default)]
    pub interview_id: Option<String>,
    #[serde(default)]
    pub unit_id: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

struct Raw {
    passage_id: String,
    interview_id: String,
    unit_id: String,
    start: usize,
    end: usize,
    text: String,
    segmentation_hash: String,
    anchor_digest: String,
    created_at: String,
    created_by: String,
    origin: Option<String>,
    role: Option<String>,
}

/// Current units of an interview, corrections applied.
type UnitMap = BTreeMap<String, Unit>;

fn context_of(text: &str, start: usize, end: usize) -> (String, String) {
    let before = start.saturating_sub(CONTEXT);
    let after_end = (end + CONTEXT).min(scalar_len(text));
    (
        slice_scalars(text, before, start)
            .unwrap_or_default()
            .to_string(),
        slice_scalars(text, end, after_end)
            .unwrap_or_default()
            .to_string(),
    )
}

impl Store {
    pub(crate) fn current_units(&self, interview_id: &str) -> Result<UnitMap> {
        let Some(seg) = self.segmentation(interview_id)? else {
            return Ok(UnitMap::new());
        };
        let mut map: UnitMap = seg
            .units
            .into_iter()
            .map(|u| (u.unit_id.clone(), u))
            .collect();
        for c in self.corrections_where(interview_id, None)? {
            if let Some(u) = map.get_mut(&c.unit_id) {
                crate::verification::apply_correction(u, &c);
            }
        }
        Ok(map)
    }

    fn raw_passages(
        &self,
        interview_id: Option<&str>,
        unit_id: Option<&str>,
        passage_id: Option<&str>,
    ) -> Result<Vec<Raw>> {
        let mut stmt = self.conn.prepare(
            "SELECT passage_id, interview_id, unit_id, start_offset, end_offset, text,
                    segmentation_hash, anchor_digest, created_at, created_by, origin, role
             FROM passages
             WHERE deleted_at IS NULL
               AND (?1 IS NULL OR interview_id = ?1)
               AND (?2 IS NULL OR unit_id = ?2)
               AND (?3 IS NULL OR passage_id = ?3)
             ORDER BY interview_id, unit_id, start_offset, end_offset, passage_id",
        )?;
        let out = stmt
            .query_map(params![interview_id, unit_id, passage_id], |r| {
                Ok(Raw {
                    passage_id: r.get(0)?,
                    interview_id: r.get(1)?,
                    unit_id: r.get(2)?,
                    start: r.get::<_, i64>(3)? as usize,
                    end: r.get::<_, i64>(4)? as usize,
                    text: r.get(5)?,
                    segmentation_hash: r.get(6)?,
                    anchor_digest: r.get(7)?,
                    created_at: r.get(8)?,
                    created_by: r.get(9)?,
                    origin: r.get(10)?,
                    role: r.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Active codings of the passages in scope, by passage.
    fn codings_of(
        &self,
        interview_id: Option<&str>,
        passage_id: Option<&str>,
    ) -> Result<BTreeMap<String, Vec<PassageCoding>>> {
        let names = crate::codetree::code_names(&self.conn)?;
        let mut stmt = self.conn.prepare(
            "SELECT pc.coding_id, pc.passage_id, pc.code_id, pc.note, pc.created_at, pc.created_by
             FROM passage_codings pc JOIN passages p USING (passage_id)
             WHERE pc.removed_at IS NULL AND p.deleted_at IS NULL
               AND (?1 IS NULL OR p.interview_id = ?1)
               AND (?2 IS NULL OR p.passage_id = ?2)
             ORDER BY pc.coding_id",
        )?;
        let mut out: BTreeMap<String, Vec<PassageCoding>> = BTreeMap::new();
        for row in stmt.query_map(params![interview_id, passage_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })? {
            let (coding_id, passage, code_id, note, created_at, created_by) = row?;
            let system = code_id == LATER_ID;
            out.entry(passage).or_default().push(PassageCoding {
                coding_id,
                code_name: if system {
                    LATER_NAME.to_string()
                } else {
                    names
                        .get(&code_id)
                        .cloned()
                        .unwrap_or_else(|| code_id.clone())
                },
                code_id,
                system,
                note,
                created_at,
                created_by,
            });
        }
        Ok(out)
    }

    fn assemble(
        &self,
        raws: Vec<Raw>,
        mut codings: BTreeMap<String, Vec<PassageCoding>>,
    ) -> Result<Vec<Passage>> {
        let mut units: BTreeMap<String, UnitMap> = BTreeMap::new();
        let mut out = Vec::with_capacity(raws.len());
        for r in raws {
            if !units.contains_key(&r.interview_id) {
                units.insert(r.interview_id.clone(), self.current_units(&r.interview_id)?);
            }
            let unit = units[&r.interview_id].get(&r.unit_id);
            let (stale, reason) = match unit {
                None => (true, Some("unit_removed".to_string())),
                Some(u) if text_digest(&u.text) != r.anchor_digest => {
                    (true, Some("text_changed".to_string()))
                }
                Some(_) => (false, None),
            };
            let (mut before, mut after) = (String::new(), String::new());
            let (mut s16, mut e16) = (None, None);
            // Both offsets against the unit length: a passage that merely starts
            // at 0 is not the whole unit. A unit that left the segmentation has no
            // length to compare with.
            let whole_unit = unit.is_some_and(|u| r.start == 0 && r.end == scalar_len(&u.text));
            if let (false, Some(u)) = (stale, unit) {
                (before, after) = context_of(&u.text, r.start, r.end);
                s16 = scalar_to_utf16(&u.text, r.start);
                e16 = scalar_to_utf16(&u.text, r.end);
            }
            let codes = codings.remove(&r.passage_id).unwrap_or_default();
            out.push(Passage {
                passage_id: r.passage_id,
                interview_id: r.interview_id,
                unit_id: r.unit_id,
                start: r.start,
                end: r.end,
                start_utf16: s16,
                end_utf16: e16,
                text: r.text,
                whole_unit,
                segmentation_hash: r.segmentation_hash,
                anchor_digest: r.anchor_digest,
                created_at: r.created_at,
                created_by: r.created_by,
                kind: if codes.is_empty() {
                    "highlight"
                } else {
                    "coded"
                }
                .to_string(),
                stale,
                stale_reason: reason,
                before,
                after,
                origin: r.origin,
                role: r.role,
                codings: codes,
            });
        }
        Ok(out)
    }

    /// One passage by id (not deleted).
    pub fn passage(&self, passage_id: &str) -> Result<Passage> {
        let raws = self.raw_passages(None, None, Some(passage_id))?;
        if raws.is_empty() {
            return Err(not_found(format!("passage {passage_id}")));
        }
        let codings = self.codings_of(None, Some(passage_id))?;
        Ok(self.assemble(raws, codings)?.remove(0))
    }

    /// Passages in segmentation order of their interview and unit, then by
    /// position.
    pub fn passages(&self, filter: &PassageFilter) -> Result<Vec<Passage>> {
        if let Some(id) = &filter.interview_id {
            self.require_interview(id)?;
        }
        let wanted_code = match &filter.code {
            Some(c) => Some(self.resolve_code(c)?.code_id),
            None => None,
        };
        let raws = self.raw_passages(
            filter.interview_id.as_deref(),
            filter.unit_id.as_deref(),
            None,
        )?;
        let codings = self.codings_of(filter.interview_id.as_deref(), None)?;
        let mut all = self.assemble(raws, codings)?;
        // Segmentation order: sort by interview, then unit sequence.
        let mut seq: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
        for p in &all {
            if !seq.contains_key(&p.interview_id) {
                let m = self
                    .segmentation(&p.interview_id)?
                    .map(|s| s.units.into_iter().map(|u| (u.unit_id, u.seq)).collect())
                    .unwrap_or_default();
                seq.insert(p.interview_id.clone(), m);
            }
        }
        all.sort_by_key(|p| {
            (
                p.interview_id.clone(),
                seq[&p.interview_id]
                    .get(&p.unit_id)
                    .copied()
                    .unwrap_or(u32::MAX),
                p.start,
                p.end,
                p.passage_id.clone(),
            )
        });
        all.retain(|p| {
            wanted_code
                .as_ref()
                .is_none_or(|c| p.codings.iter().any(|k| &k.code_id == c))
                && match filter.kind.as_deref() {
                    Some("highlight") => p.codings.is_empty(),
                    Some("coded") => !p.codings.is_empty(),
                    Some("later") => p.codings.iter().any(|k| k.system),
                    _ => true,
                }
        });
        Ok(all)
    }

    /// Passages without any active coding.
    pub fn highlights(&self, interview_id: Option<&str>) -> Result<Vec<Passage>> {
        self.passages(&PassageFilter {
            interview_id: interview_id.map(str::to_string),
            kind: Some("highlight".into()),
            ..Default::default()
        })
    }

    /// Passages parked with "Plus tard", with their note in the coding.
    pub fn later_items(&self, interview_id: Option<&str>) -> Result<Vec<Passage>> {
        self.passages(&PassageFilter {
            interview_id: interview_id.map(str::to_string),
            kind: Some("later".into()),
            ..Default::default()
        })
    }

    /// Number of parked passages per interview, interviews with none left out.
    pub fn later_summary(&self) -> Result<Vec<(String, usize)>> {
        let mut counts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for p in self.later_items(None)? {
            counts
                .entry(p.interview_id)
                .or_default()
                .insert(p.passage_id);
        }
        Ok(counts.into_iter().map(|(k, v)| (k, v.len())).collect())
    }

    // -------------------------------------------------------------- writes

    /// Finds or creates the passage. Returns it and whether it was created.
    pub fn create_passage(&self, req: &NewPassage, actor: &str) -> Result<(Passage, bool)> {
        let tx = self.conn.unchecked_transaction()?;
        let (id, created) = self.create_passage_in(&tx, req, actor)?;
        tx.commit()?;
        Ok((self.passage(&id)?, created))
    }

    /// Finds or creates the passage and applies a code to it, in one
    /// transaction. `code` is an id, name, alias or token; `note` is free.
    pub fn code_span(
        &self,
        req: &NewPassage,
        code: &str,
        note: &str,
        actor: &str,
    ) -> Result<(Passage, bool)> {
        let tx = self.conn.unchecked_transaction()?;
        let (id, created) = self.create_passage_in(&tx, req, actor)?;
        self.add_coding_in(&tx, &id, code, note, actor)?;
        tx.commit()?;
        Ok((self.passage(&id)?, created))
    }

    /// Applies a code to an existing passage (a highlight becomes coded).
    pub fn code_passage(
        &self,
        passage_id: &str,
        code: &str,
        note: &str,
        actor: &str,
    ) -> Result<Passage> {
        self.passage(passage_id)?;
        let tx = self.conn.unchecked_transaction()?;
        self.add_coding_in(&tx, passage_id, code, note, actor)?;
        tx.commit()?;
        self.passage(passage_id)
    }

    /// Removes a code from a passage (logically: the row stays).
    pub fn uncode_passage(
        &self,
        passage_id: &str,
        code: &str,
        reason: &str,
        actor: &str,
    ) -> Result<Passage> {
        let p = self.passage(passage_id)?;
        let resolved = self.resolve_code(code)?;
        let Some(coding) = p.codings.iter().find(|c| c.code_id == resolved.code_id) else {
            return Err(validation(format!(
                "passage {passage_id} does not carry {}",
                resolved.name
            )));
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE passage_codings SET removed_at = ?2, removed_by = ?3, remove_reason = ?4
             WHERE coding_id = ?1",
            params![coding.coding_id, now(), actor, reason.trim()],
        )?;
        append_event(
            &tx,
            actor,
            "passage.uncode",
            Some(&p.interview_id),
            Some(passage_id),
            json!({
                "passage_id": passage_id,
                "unit_id": p.unit_id,
                "code_id": resolved.code_id,
                "code_name": resolved.name,
                "reason": reason.trim(),
            }),
        )?;
        self.sync_unit_from_passages_in(&tx, &p.interview_id, &p.unit_id, actor)?;
        tx.commit()?;
        self.passage(passage_id)
    }

    /// Deletes a passage logically: it leaves every list and count; its row,
    /// its codings and its audit events stay.
    pub fn delete_passage(&self, passage_id: &str, reason: &str, actor: &str) -> Result<()> {
        let p = self.passage(passage_id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE passages SET deleted_at = ?2, deleted_by = ?3, delete_reason = ?4
             WHERE passage_id = ?1",
            params![passage_id, now(), actor, reason.trim()],
        )?;
        append_event(
            &tx,
            actor,
            "passage.delete",
            Some(&p.interview_id),
            Some(passage_id),
            json!({
                "passage_id": passage_id,
                "unit_id": p.unit_id,
                "start": p.start,
                "end": p.end,
                "text": p.text,
                "codes": p.codings.iter().map(|c| c.code_id.clone()).collect::<Vec<_>>(),
                "reason": reason.trim(),
            }),
        )?;
        self.sync_unit_from_passages_in(&tx, &p.interview_id, &p.unit_id, actor)?;
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------ internals

    /// Validates the span against the current unit text and inserts the
    /// passage unless the same span (same text digest) already exists.
    pub(crate) fn create_passage_in(
        &self,
        conn: &Connection,
        req: &NewPassage,
        actor: &str,
    ) -> Result<(String, bool)> {
        self.require_interview(&req.interview_id)?;
        let seg_hash = self
            .segmentation(&req.interview_id)?
            .map(|s| s.hash)
            .ok_or_else(|| {
                validation(format!(
                    "{} has no segmentation yet; segment it first",
                    req.interview_id
                ))
            })?;
        let unit = self.current_unit(&req.interview_id, &req.unit_id)?;
        if unit.role == SpeakerRole::Interviewer {
            return Err(validation(format!(
                "unit {} is an interviewer turn: interviewer turns are shown for context and are never coded, highlighted or parked",
                req.unit_id
            )));
        }
        let len = scalar_len(&unit.text);
        let (start, end) = match (req.start, req.end) {
            (None, None) => (0, len),
            (Some(a), Some(b)) => match req.offset_unit {
                OffsetUnit::Scalar => (a, b),
                OffsetUnit::Utf16 => {
                    let conv = |x: usize| {
                        utf16_to_scalar(&unit.text, x).ok_or_else(|| {
                            validation(format!(
                                "UTF-16 position {x} is outside the unit or inside a character"
                            ))
                        })
                    };
                    (conv(a)?, conv(b)?)
                }
            },
            _ => {
                return Err(validation(
                    "give both start and end, or neither to cover the whole unit",
                ))
            }
        };
        if start >= end {
            return Err(validation(format!(
                "the passage {start}..{end} is empty or reversed"
            )));
        }
        let Some(slice) = slice_scalars(&unit.text, start, end) else {
            return Err(validation(format!(
                "the passage {start}..{end} is outside unit {} (length {len})",
                req.unit_id
            )));
        };
        if slice.trim().is_empty() {
            return Err(validation("a passage cannot be only blanks"));
        }
        if let Some(expected) = &req.text {
            if expected != slice {
                return Err(validation(format!(
                    "the offsets {start}..{end} select {slice:?}, not {expected:?}; the passage is refused"
                )));
            }
        }
        let digest = text_digest(&unit.text);
        let existing: Option<String> = conn
            .query_row(
                "SELECT passage_id FROM passages
                 WHERE interview_id = ?1 AND unit_id = ?2 AND start_offset = ?3 AND end_offset = ?4
                   AND anchor_digest = ?5 AND deleted_at IS NULL",
                params![
                    req.interview_id,
                    req.unit_id,
                    start as i64,
                    end as i64,
                    digest
                ],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = existing {
            return Ok((id, false));
        }
        let next: i64 = conn.query_row(
            "SELECT COALESCE(MAX(CAST(substr(passage_id, 3) AS INTEGER)), 0) + 1 FROM passages",
            [],
            |r| r.get(0),
        )?;
        let id = format!("P-{next:05}");
        conn.execute(
            "INSERT INTO passages (passage_id, interview_id, unit_id, start_offset, end_offset,
               text, segmentation_hash, anchor_digest, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                req.interview_id,
                req.unit_id,
                start as i64,
                end as i64,
                slice,
                seg_hash,
                digest,
                now(),
                actor
            ],
        )?;
        append_event(
            conn,
            actor,
            "passage.create",
            Some(&req.interview_id),
            Some(&id),
            json!({
                "passage_id": id,
                "unit_id": req.unit_id,
                "start": start,
                "end": end,
                "text": slice,
                "segmentation_hash": seg_hash,
                "anchor_digest": digest,
                "whole_unit": start == 0 && end == len,
            }),
        )?;
        Ok((id, true))
    }

    /// Applies a code to a passage inside the caller's transaction.
    pub(crate) fn add_coding_in(
        &self,
        conn: &Connection,
        passage_id: &str,
        code: &str,
        note: &str,
        actor: &str,
    ) -> Result<()> {
        let resolved = self.resolve_code(code)?;
        if !resolved.active {
            return Err(validation(format!(
                "code {} is inactive or merged and cannot be applied",
                resolved.name
            )));
        }
        let (interview_id, unit_id): (String, String) = conn.query_row(
            "SELECT interview_id, unit_id FROM passages WHERE passage_id = ?1 AND deleted_at IS NULL",
            [passage_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let note = note.trim();
        let existing: Option<(i64, String)> = conn
            .query_row(
                "SELECT coding_id, note FROM passage_codings
                 WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                params![passage_id, resolved.code_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        match existing {
            Some((coding_id, old)) => {
                if !note.is_empty() && note != old {
                    conn.execute(
                        "UPDATE passage_codings SET note = ?2 WHERE coding_id = ?1",
                        params![coding_id, note],
                    )?;
                    append_event(
                        conn,
                        actor,
                        "passage.code_note",
                        Some(&interview_id),
                        Some(passage_id),
                        json!({
                            "passage_id": passage_id,
                            "code_id": resolved.code_id,
                            "before": old,
                            "after": note,
                        }),
                    )?;
                }
            }
            None => {
                conn.execute(
                    "INSERT INTO passage_codings (passage_id, code_id, note, created_at, created_by)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![passage_id, resolved.code_id, note, now(), actor],
                )?;
                append_event(
                    conn,
                    actor,
                    "passage.code",
                    Some(&interview_id),
                    Some(passage_id),
                    json!({
                        "passage_id": passage_id,
                        "unit_id": unit_id,
                        "code_id": resolved.code_id,
                        "code_name": resolved.name,
                        "note": note,
                    }),
                )?;
            }
        }
        self.sync_unit_from_passages_in(conn, &interview_id, &unit_id, actor)?;
        Ok(())
    }
}
