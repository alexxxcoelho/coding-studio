//! Audio verification of units (C3): flags, corrections of text and role,
//! verification status computed from the latest row, the verification targets
//! and the coverage of the eligible corpus.
//!
//! The segmentation is never rewritten. A correction overlays the unit: its
//! current text and role are those of the latest correction (see
//! `Store::current_unit`). A verification is stale when the text or the role it
//! was made on no longer match the current unit.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::reconcile::{sample_agreements, sample_seed};
use cs_model::{
    sha256_hex, ReviewKind, SpeakerRole, Unit, UnitCorrection, UnitFlags, VerificationCoverage,
    VerificationStatus, VerificationTarget, CO_CONSTRUCTED_KINDS, PHASE_2, UNIT_FLAGS,
};
use rusqlite::params;
use serde_json::json;

use crate::{
    append_event, clean_opt, enum_from, enum_str, from_json, now, to_json, validation, Store,
};

/// Latest verification row of a unit, as stored.
struct Stored {
    state: String,
    text_sha256: String,
    role: String,
    note: Option<String>,
    at: String,
    by: String,
}

/// Overlays a correction on a segmented unit.
pub(crate) fn apply_correction(unit: &mut Unit, correction: &UnitCorrection) {
    unit.text = correction.text.clone();
    unit.role = correction.role;
    unit.corrected = true;
}

/// Status of a unit given its latest verification row (if any).
fn status_for(unit: &Unit, row: Option<&Stored>) -> VerificationStatus {
    let Some(row) = row else {
        return VerificationStatus {
            unit_id: unit.unit_id.clone(),
            state: "unverified".into(),
            ..Default::default()
        };
    };
    let stale =
        row.text_sha256 != sha256_hex(unit.text.as_bytes()) || row.role != enum_str(&unit.role);
    VerificationStatus {
        unit_id: unit.unit_id.clone(),
        state: if stale {
            "unverified".into()
        } else {
            row.state.clone()
        },
        stale,
        at: Some(row.at.clone()),
        by: Some(row.by.clone()),
        note: row.note.clone(),
    }
}

impl Store {
    // -------------------------------------------------------------- flags

    /// Current flags of the interview's units, ordered by unit id.
    pub fn unit_flags(&self, interview_id: &str) -> Result<Vec<UnitFlags>> {
        let mut stmt = self.conn.prepare(
            "SELECT unit_id, flags, co_constructed_kind, updated_at FROM unit_flags
             WHERE interview_id = ?1 ORDER BY unit_id",
        )?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (unit_id, flags, kind, updated_at) = row?;
            out.push(UnitFlags {
                interview_id: interview_id.to_string(),
                unit_id,
                flags: from_json(&flags)?,
                co_constructed_kind: kind,
                updated_at,
            });
        }
        Ok(out)
    }

    /// Replaces the flags of a unit. Flags come from the closed list
    /// `UNIT_FLAGS`; `CO-CONSTRUCTED` requires a kind from `CO_CONSTRUCTED_KINDS`,
    /// and a kind is refused without it. Audited with before and after.
    pub fn set_unit_flags(
        &self,
        interview_id: &str,
        unit_id: &str,
        flags: &[String],
        co_constructed_kind: Option<&str>,
        actor: &str,
    ) -> Result<UnitFlags> {
        self.require_interview(interview_id)?;
        self.current_unit(interview_id, unit_id)?;
        let mut wanted: Vec<String> = Vec::new();
        for flag in flags {
            let flag = flag.trim();
            if !UNIT_FLAGS.contains(&flag) {
                return Err(validation(format!(
                    "unknown flag {flag}; allowed: {}",
                    UNIT_FLAGS.join(", ")
                )));
            }
            wanted.push(flag.to_string());
        }
        wanted.sort();
        wanted.dedup();
        let has_co_constructed = wanted.iter().any(|f| f == "CO-CONSTRUCTED");
        let kind = clean_opt(&co_constructed_kind.map(str::to_string));
        let kind = match (has_co_constructed, kind) {
            (true, None) => {
                return Err(validation(
                    "the CO-CONSTRUCTED flag needs a kind: assent or content",
                ))
            }
            (true, Some(k)) => {
                if !CO_CONSTRUCTED_KINDS.contains(&k.as_str()) {
                    return Err(validation(format!(
                        "unknown kind {k}; allowed: {}",
                        CO_CONSTRUCTED_KINDS.join(", ")
                    )));
                }
                Some(k)
            }
            (false, Some(_)) => {
                return Err(validation(
                    "a kind is accepted only with the CO-CONSTRUCTED flag",
                ))
            }
            (false, None) => None,
        };
        let before = self
            .unit_flags(interview_id)?
            .into_iter()
            .find(|f| f.unit_id == unit_id);
        let after = UnitFlags {
            interview_id: interview_id.to_string(),
            unit_id: unit_id.to_string(),
            flags: wanted,
            co_constructed_kind: kind,
            updated_at: now(),
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO unit_flags (interview_id, unit_id, flags, co_constructed_kind, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(interview_id, unit_id) DO UPDATE SET flags = excluded.flags,
               co_constructed_kind = excluded.co_constructed_kind, updated_at = excluded.updated_at",
            params![
                interview_id,
                unit_id,
                to_json(&after.flags),
                after.co_constructed_kind,
                after.updated_at
            ],
        )?;
        append_event(
            &tx,
            actor,
            "unit.flags",
            Some(interview_id),
            Some(unit_id),
            json!({"before": before, "after": after}),
        )?;
        tx.commit()?;
        Ok(after)
    }

    // -------------------------------------------------------- corrections

    /// Corrections of a unit (or of all units of the interview when `unit_id`
    /// is `None`), oldest first.
    pub(crate) fn corrections_where(
        &self,
        interview_id: &str,
        unit_id: Option<&str>,
    ) -> Result<Vec<UnitCorrection>> {
        let mut stmt = self.conn.prepare(
            "SELECT correction_id, unit_id, text, role, reason, created_at, created_by
             FROM unit_corrections
             WHERE interview_id = ?1 AND (?2 IS NULL OR unit_id = ?2)
             ORDER BY correction_id",
        )?;
        let rows = stmt.query_map(params![interview_id, unit_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (correction_id, uid, text, role, reason, created_at, created_by) = row?;
            out.push(UnitCorrection {
                correction_id,
                interview_id: interview_id.to_string(),
                unit_id: uid,
                text,
                role: enum_from::<SpeakerRole>(&role)?,
                reason,
                created_at,
                created_by,
            });
        }
        Ok(out)
    }

    /// Corrections of a unit, oldest first.
    pub fn unit_corrections(
        &self,
        interview_id: &str,
        unit_id: &str,
    ) -> Result<Vec<UnitCorrection>> {
        self.require_interview(interview_id)?;
        self.corrections_where(interview_id, Some(unit_id))
    }

    /// Appends a correction of the text and role of a unit. The segmentation is
    /// untouched. Both the text and the reason are required.
    pub fn correct_unit(
        &self,
        interview_id: &str,
        unit_id: &str,
        text: &str,
        role: SpeakerRole,
        reason: &str,
        actor: &str,
    ) -> Result<UnitCorrection> {
        self.require_interview(interview_id)?;
        let before = self.current_unit(interview_id, unit_id)?;
        let text = text.trim();
        if text.is_empty() {
            return Err(validation("the corrected text is empty"));
        }
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(validation("a reason is required for a correction"));
        }
        let created_at = now();
        let tx = self.conn.unchecked_transaction()?;
        self.ensure_baseline_in(&tx, interview_id, actor)?;
        tx.execute(
            "INSERT INTO unit_corrections (interview_id, unit_id, text, role, reason, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                interview_id,
                unit_id,
                text,
                enum_str(&role),
                reason,
                created_at,
                actor
            ],
        )?;
        let correction_id = tx.last_insert_rowid();
        append_event(
            &tx,
            actor,
            "unit.correct",
            Some(interview_id),
            Some(unit_id),
            json!({
                "correction_id": correction_id,
                "before": {"text": before.text, "role": enum_str(&before.role)},
                "after": {"text": text, "role": enum_str(&role)},
                "reason": reason,
            }),
        )?;
        self.version_after_change_in(
            &tx,
            interview_id,
            "correction",
            Some(unit_id),
            Some(reason),
            actor,
        )?;
        tx.commit()?;
        Ok(UnitCorrection {
            correction_id,
            interview_id: interview_id.to_string(),
            unit_id: unit_id.to_string(),
            text: text.to_string(),
            role,
            reason: reason.to_string(),
            created_at,
            created_by: actor.to_string(),
        })
    }

    // ------------------------------------------------------ verification

    /// Latest verification row of every unit of the interview.
    fn latest_verifications(&self, interview_id: &str) -> Result<BTreeMap<String, Stored>> {
        let mut stmt = self.conn.prepare(
            "SELECT unit_id, state, text_sha256, role, note, created_at, created_by
             FROM unit_verifications WHERE interview_id = ?1 ORDER BY verification_id",
        )?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Stored {
                    state: r.get(1)?,
                    text_sha256: r.get(2)?,
                    role: r.get(3)?,
                    note: r.get(4)?,
                    at: r.get(5)?,
                    by: r.get(6)?,
                },
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (unit_id, stored) = row?;
            out.insert(unit_id, stored);
        }
        Ok(out)
    }

    /// Records a verification of the unit's current text and role. `state` is
    /// `verified` or `unusable`. Returns the status it gives.
    pub fn verify_unit(
        &self,
        interview_id: &str,
        unit_id: &str,
        state: &str,
        note: Option<&str>,
        actor: &str,
    ) -> Result<VerificationStatus> {
        self.require_interview(interview_id)?;
        if state != "verified" && state != "unusable" {
            return Err(validation(format!(
                "unknown verification state {state}; use verified or unusable"
            )));
        }
        let unit = self.current_unit(interview_id, unit_id)?;
        let row = Stored {
            state: state.to_string(),
            text_sha256: sha256_hex(unit.text.as_bytes()),
            role: enum_str(&unit.role),
            note: clean_opt(&note.map(str::to_string)),
            at: now(),
            by: actor.to_string(),
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO unit_verifications (interview_id, unit_id, state, text_sha256, role, note, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                interview_id,
                unit_id,
                row.state,
                row.text_sha256,
                row.role,
                row.note,
                row.at,
                row.by
            ],
        )?;
        append_event(
            &tx,
            actor,
            "unit.verify",
            Some(interview_id),
            Some(unit_id),
            json!({
                "state": row.state,
                "note": row.note,
                "text_sha256": row.text_sha256,
                "role": row.role,
            }),
        )?;
        tx.commit()?;
        Ok(status_for(&unit, Some(&row)))
    }

    /// Verification status of every unit, in segmentation order.
    pub fn verification_status(&self, interview_id: &str) -> Result<Vec<VerificationStatus>> {
        self.require_interview(interview_id)?;
        let Some(seg) = self.segmentation(interview_id)? else {
            return Ok(Vec::new());
        };
        let mut latest_corrections: BTreeMap<String, UnitCorrection> = BTreeMap::new();
        for c in self.corrections_where(interview_id, None)? {
            latest_corrections.insert(c.unit_id.clone(), c);
        }
        let latest = self.latest_verifications(interview_id)?;
        Ok(seg
            .units
            .into_iter()
            .map(|mut unit| {
                if let Some(c) = latest_corrections.get(&unit.unit_id) {
                    apply_correction(&mut unit, c);
                }
                status_for(&unit, latest.get(&unit.unit_id))
            })
            .collect())
    }

    /// Units that must be verified, in segmentation order, with their reasons:
    /// `quoted`, `flagged`, `disagreement`, `case`, and `sample` (a seeded draw
    /// of max(5, ceil(10%)) among the units with no other reason). The seed is
    /// stored on first draw, which is audited once as `verification.sample`.
    pub fn verification_targets(&self, interview_id: &str) -> Result<Vec<VerificationTarget>> {
        self.require_interview(interview_id)?;
        let Some(seg) = self.segmentation(interview_id)? else {
            return Ok(Vec::new());
        };
        // Interviewer turns are never verification targets.
        let order: Vec<String> = seg
            .units
            .iter()
            .filter(|u| !matches!(u.role, SpeakerRole::Interviewer))
            .map(|u| u.unit_id.clone())
            .collect();
        let mut tags: Vec<(String, &'static str)> = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT unit_id FROM human_codings
             WHERE interview_id = ?1 AND phase = ?2 AND excerpt IS NOT NULL AND excerpt <> ''
             UNION
             SELECT DISTINCT unit_id FROM claims WHERE interview_id = ?1",
        )?;
        let quoted: Vec<String> = stmt
            .query_map(params![interview_id, PHASE_2], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        tags.extend(quoted.into_iter().map(|u| (u, "quoted")));

        for flags in self.unit_flags(interview_id)? {
            if flags.flags.iter().any(|f| f == "VERIFY") {
                tags.push((flags.unit_id, "flagged"));
            }
        }

        for entry in self.review(interview_id)?.items {
            if entry.item.kind == ReviewKind::Disagreement {
                tags.push((entry.item.unit_id, "disagreement"));
            }
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT unit_id FROM case_units WHERE interview_id = ?1")?;
        let in_cases: Vec<String> = stmt
            .query_map([interview_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        tags.extend(in_cases.into_iter().map(|u| (u, "case")));

        let mut reasons: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
        for (unit, reason) in tags {
            reasons.entry(unit).or_default().insert(reason);
        }

        let remaining: Vec<String> = order
            .iter()
            .filter(|u| !reasons.contains_key(*u))
            .cloned()
            .collect();
        let seed = self.verification_seed(interview_id, &seg.hash, &remaining)?;
        for unit in sample_agreements(&seed, &remaining) {
            reasons.entry(unit).or_default().insert("sample");
        }

        Ok(order
            .iter()
            .filter_map(|u| {
                reasons.get(u).map(|rs| VerificationTarget {
                    unit_id: u.clone(),
                    reasons: rs.iter().map(|r| r.to_string()).collect(),
                })
            })
            .collect())
    }

    /// The sample seed of the interview. On first draw it is stored in
    /// `settings` and the draw is audited once, with the units drawn.
    fn verification_seed(
        &self,
        interview_id: &str,
        segmentation_hash: &str,
        remaining: &[String],
    ) -> Result<String> {
        let key = format!("verification_sample_seed:{interview_id}");
        if let Some(seed) = self.setting(&key)? {
            return Ok(seed);
        }
        let seed = sample_seed(
            0,
            &format!("verification:{interview_id}:{segmentation_hash}"),
        );
        let drawn = sample_agreements(&seed, remaining);
        let tx = self.conn.unchecked_transaction()?;
        let inserted = tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO NOTHING",
            params![key, seed, now()],
        )?;
        if inserted == 1 {
            append_event(
                &tx,
                "system",
                "verification.sample",
                Some(interview_id),
                None,
                json!({
                    "seed": seed,
                    "drawn": drawn,
                    "n_remaining": remaining.len(),
                    "rule": "max(5, ceil(10%)) of the units with no other reason, ordered by sha256(seed + unit_id)",
                }),
            )?;
        }
        tx.commit()?;
        Ok(seed)
    }

    /// Counts of the verification of the interview's units and targets.
    /// A target is handled when it is verified or unusable and not stale.
    pub fn verification_coverage(&self, interview_id: &str) -> Result<VerificationCoverage> {
        let statuses = self.verification_status(interview_id)?;
        let targets = self.verification_targets(interview_id)?;
        let handled: BTreeSet<&str> = statuses
            .iter()
            .filter(|s| !s.stale && (s.state == "verified" || s.state == "unusable"))
            .map(|s| s.unit_id.as_str())
            .collect();
        Ok(VerificationCoverage {
            n_units: statuses.len(),
            n_verified: statuses
                .iter()
                .filter(|s| !s.stale && s.state == "verified")
                .count(),
            n_unusable: statuses
                .iter()
                .filter(|s| !s.stale && s.state == "unusable")
                .count(),
            n_stale: statuses.iter().filter(|s| s.stale).count(),
            n_targets: targets.len(),
            n_targets_verified: targets
                .iter()
                .filter(|t| handled.contains(t.unit_id.as_str()))
                .count(),
        })
    }
}
