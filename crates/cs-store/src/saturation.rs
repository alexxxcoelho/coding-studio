//! Saturation table (C6). Provisional: it counts first uses and field codes,
//! it does not compute any enrichment of meaning.
//!
//! Ordering: interviews are ordered by the audit sequence number of their first
//! phase 2 coding event (`coding.set` or `to_clarify.resolve`, both carry
//! `phase` in their detail). The sequence number is used instead of the
//! timestamp because timestamps have one-second precision.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::{SaturationRow, NA, PHASE_2};
use serde_json::Value;

use crate::Store;

impl Store {
    /// One row per interview with a phase 2 coding, in coding order. Interviews
    /// without phase 2 coding are not listed. Field codes that cannot be
    /// attributed to an interview are listed in one extra row with an empty
    /// `interview_id` and `order` 0.
    pub fn saturation(&self) -> Result<Vec<SaturationRow>> {
        // First phase 2 coding event per interview, in audit order.
        let mut first: Vec<(String, String)> = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT ts, interview_id, detail FROM audit_log
                 WHERE action IN ('coding.set', 'to_clarify.resolve') AND interview_id IS NOT NULL
                 ORDER BY seq",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            let mut seen = BTreeSet::new();
            for row in rows {
                let (ts, interview_id, detail) = row?;
                let phase = serde_json::from_str::<Value>(&detail)
                    .ok()
                    .and_then(|d| d["phase"].as_u64());
                if phase == Some(u64::from(PHASE_2)) && seen.insert(interview_id.clone()) {
                    first.push((interview_id, ts));
                }
            }
        }

        // Codes first used, in coding order, from the current phase 2 codings.
        let mut seen_codes: BTreeSet<String> = BTreeSet::new();
        let mut first_used: Vec<Vec<String>> = Vec::with_capacity(first.len());
        for (interview_id, _) in &first {
            let mut used = Vec::new();
            for codes in self.human_coding_map(interview_id, PHASE_2)?.values() {
                for code in codes.iter().filter(|c| c.as_str() != NA) {
                    if seen_codes.insert(code.clone()) {
                        used.push(code.clone());
                    }
                }
            }
            first_used.push(used);
        }

        // Field codes created in the tool (imported inductive codes have no
        // created_at and are part of the codebook, not of the coding): attributed
        // by the label resolution that created them, else by the coding window
        // (the last interview started at or before their creation), else left
        // unattributed.
        let mut owner_by_resolution: BTreeMap<String, String> = BTreeMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT r.code, t.interview_id FROM to_clarify_resolutions r
                 JOIN to_clarify t USING (tid)
                 WHERE r.kind = 'new_code' ORDER BY r.tid",
            )?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (code, interview_id) = row?;
                owner_by_resolution.entry(code).or_insert(interview_id);
            }
        }
        let mut field_created: Vec<Vec<String>> = vec![Vec::new(); first.len()];
        let mut unattributed: Vec<String> = Vec::new();
        for code in self
            .codebook()?
            .codes
            .into_iter()
            .filter(|c| c.origin == "field" && c.created_at.is_some())
        {
            let by_resolution = owner_by_resolution
                .get(&code.code)
                .and_then(|owner| first.iter().position(|(id, _)| id == owner));
            let by_window = code
                .created_at
                .as_deref()
                .and_then(|created| first.iter().rposition(|(_, ts)| ts.as_str() <= created));
            match by_resolution.or(by_window) {
                Some(i) => field_created[i].push(code.code),
                None => unattributed.push(code.code),
            }
        }

        // Labels to clarify, opened and resolved, per interview.
        let mut labels: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT t.interview_id, COUNT(*), COUNT(r.tid) FROM to_clarify t
                 LEFT JOIN to_clarify_resolutions r USING (tid) GROUP BY t.interview_id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            })?;
            for row in rows {
                let (interview_id, opened, resolved) = row?;
                labels.insert(interview_id, (opened, resolved));
            }
        }

        let mut out = Vec::with_capacity(first.len() + 1);
        for (i, (((interview_id, ts), used), created)) in first
            .into_iter()
            .zip(first_used)
            .zip(field_created)
            .enumerate()
        {
            let (labels_opened, labels_resolved) =
                labels.get(&interview_id).copied().unwrap_or_default();
            out.push(SaturationRow {
                interview_id,
                order: i as u32 + 1,
                first_coded_at: ts,
                codes_first_used: used,
                field_codes_created: created,
                labels_opened,
                labels_resolved,
            });
        }
        if !unattributed.is_empty() {
            out.push(SaturationRow {
                interview_id: String::new(),
                order: 0,
                field_codes_created: unattributed,
                ..Default::default()
            });
        }
        Ok(out)
    }
}
