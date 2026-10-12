//! Memos per research question (C5): one written memo per question, citing the
//! units that support and those that contradict the answer.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::RqMemo;
use rusqlite::{params, OptionalExtension};
use serde_json::json;

use crate::{append_event, from_json, not_found, now, to_json, validation, Store};

type MemoRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

const MEMO_COLUMNS: &str =
    "rq_id, documented, interpretation, rivals, limits, units_for, units_against, updated_at";

fn memo_from_row(row: MemoRow) -> Result<RqMemo> {
    let (rq_id, documented, interpretation, rivals, limits, units_for, units_against, updated_at) =
        row;
    Ok(RqMemo {
        rq_id,
        documented,
        interpretation,
        rivals,
        limits,
        units_for: from_json(&units_for)?,
        units_against: from_json(&units_against)?,
        updated_at,
    })
}

impl Store {
    /// Memo of one research question, if written.
    pub fn rq_memo(&self, rq_id: &str) -> Result<Option<RqMemo>> {
        let row: Option<MemoRow> = self
            .conn
            .query_row(
                &format!("SELECT {MEMO_COLUMNS} FROM rq_memos WHERE rq_id = ?1"),
                [rq_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )
            .optional()?;
        row.map(memo_from_row).transpose()
    }

    /// Writes the memo of a research question. Every unit id must exist in the
    /// current segmentation of its interview. No event when nothing changed.
    pub fn set_rq_memo(&self, memo: &RqMemo, actor: &str) -> Result<RqMemo> {
        let rq_id = memo.rq_id.clone();
        let known: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM research_questions WHERE id = ?1",
            [&rq_id],
            |r| r.get(0),
        )?;
        if known == 0 {
            return Err(not_found(format!("research question {rq_id}")));
        }
        let units_for = self.check_memo_units(&memo.units_for)?;
        let units_against = self.check_memo_units(&memo.units_against)?;
        let before = self.rq_memo(&rq_id)?;
        let mut after = RqMemo {
            rq_id: rq_id.clone(),
            documented: memo.documented.clone(),
            interpretation: memo.interpretation.clone(),
            rivals: memo.rivals.clone(),
            limits: memo.limits.clone(),
            units_for,
            units_against,
            updated_at: now(),
        };
        if let Some(b) = &before {
            let mut probe = after.clone();
            probe.updated_at = b.updated_at.clone();
            if &probe == b {
                return Ok(b.clone());
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO rq_memos (rq_id, documented, interpretation, rivals, limits, units_for, units_against, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(rq_id) DO UPDATE SET documented = excluded.documented,
               interpretation = excluded.interpretation, rivals = excluded.rivals,
               limits = excluded.limits, units_for = excluded.units_for,
               units_against = excluded.units_against, updated_at = excluded.updated_at",
            params![
                after.rq_id,
                after.documented,
                after.interpretation,
                after.rivals,
                after.limits,
                to_json(&after.units_for),
                to_json(&after.units_against),
                after.updated_at
            ],
        )?;
        append_event(
            &tx,
            actor,
            "rq_memo.update",
            None,
            Some(&rq_id),
            json!({"rq_id": rq_id, "before": before, "after": after}),
        )?;
        tx.commit()?;
        after.rq_id = rq_id;
        Ok(after)
    }

    /// Memos of all research questions, in question order.
    pub fn rq_memos(&self) -> Result<Vec<RqMemo>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.rq_id, m.documented, m.interpretation, m.rivals, m.limits, m.units_for, m.units_against, m.updated_at
             FROM rq_memos m JOIN research_questions q ON q.id = m.rq_id
             ORDER BY q.ord, q.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(memo_from_row(row?)?);
        }
        Ok(out)
    }

    /// Trims, drops empty and repeated ids, and checks that each id is a unit of
    /// the current segmentation of the interview named by its prefix.
    fn check_memo_units(&self, ids: &[String]) -> Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        let mut segmented: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for raw in ids {
            let id = raw.trim().to_string();
            if id.is_empty() || out.contains(&id) {
                continue;
            }
            let interview = id
                .rsplit_once("-U")
                .map(|(interview, _)| interview.to_string())
                .ok_or_else(|| {
                    validation(format!(
                        "unit id {id} is not of the form <interview_id>-U<nnn>"
                    ))
                })?;
            if !segmented.contains_key(&interview) {
                let units: BTreeSet<String> = self
                    .segmentation(&interview)?
                    .map(|s| s.units.into_iter().map(|u| u.unit_id).collect())
                    .unwrap_or_default();
                segmented.insert(interview.clone(), units);
            }
            if !segmented[&interview].contains(&id) {
                return Err(validation(format!(
                    "unit {id} is not in the current segmentation of {interview}"
                )));
            }
            out.push(id);
        }
        Ok(out)
    }
}
