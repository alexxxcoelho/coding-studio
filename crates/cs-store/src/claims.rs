//! Claims: qualifications of one statement inside a unit (C4). A claim quotes
//! an excerpt of the unit's current text, lists the phase 2 codes it bears on
//! and carries closed-list qualifications (EV level, provenance, role).

use std::collections::BTreeSet;

use anyhow::Result;
use cs_model::{Claim, CodeSet, CLAIM_ROLES, EV_LEVELS, PHASE_2, PROVENANCES};
use rusqlite::params;
use serde_json::json;

use crate::{
    append_event, clean_codes, clean_opt, from_json, not_found, now, to_json, validation, Store,
};

const CLAIM_COLUMNS: &str = "claim_id, interview_id, unit_id, excerpt, codes, ev_level, provenance, role, memo, created_at, updated_at, nature";

fn closed(field: &str, value: &Option<String>, allowed: &[&str]) -> Result<()> {
    match value {
        Some(v) if !allowed.contains(&v.as_str()) => Err(validation(format!(
            "{field} must be one of {}, not {v}",
            allowed.join(", ")
        ))),
        _ => Ok(()),
    }
}

impl Store {
    /// Claims of one interview, oldest first.
    pub fn claims(&self, interview_id: &str) -> Result<Vec<Claim>> {
        self.query_claims("WHERE interview_id = ?1", params![interview_id])
    }

    /// Claims of one unit, oldest first.
    pub fn unit_claims(&self, interview_id: &str, unit_id: &str) -> Result<Vec<Claim>> {
        self.query_claims(
            "WHERE interview_id = ?1 AND unit_id = ?2",
            params![interview_id, unit_id],
        )
    }

    /// Adds a claim to a unit. The excerpt must be a substring of the unit's
    /// current text; the codes must be among the unit's current phase 2 codes.
    pub fn create_claim(&self, interview_id: &str, claim: &Claim, actor: &str) -> Result<Claim> {
        self.require_interview(interview_id)?;
        let mut made = self.check_claim(interview_id, claim, None)?;
        let ts = now();
        made.created_at = ts.clone();
        made.updated_at = ts.clone();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO claims (interview_id, unit_id, excerpt, codes, ev_level, provenance, role, memo, created_at, updated_at, nature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                interview_id,
                made.unit_id,
                made.excerpt,
                to_json(&made.codes),
                made.ev_level,
                made.provenance,
                made.role,
                made.memo,
                ts,
                ts,
                made.nature
            ],
        )?;
        made.claim_id = tx.last_insert_rowid();
        made.interview_id = interview_id.to_string();
        append_event(
            &tx,
            actor,
            "claim.create",
            Some(interview_id),
            Some(&made.unit_id),
            json!({"before": null, "after": made}),
        )?;
        tx.commit()?;
        Ok(made)
    }

    /// Replaces the content of a claim (its interview stays fixed). No event
    /// when nothing changed.
    pub fn update_claim(&self, claim_id: i64, claim: &Claim, actor: &str) -> Result<Claim> {
        let before = self.get_claim(claim_id)?;
        let mut after = self.check_claim(&before.interview_id, claim, before.nature.as_deref())?;
        after.claim_id = before.claim_id;
        after.interview_id = before.interview_id.clone();
        after.created_at = before.created_at.clone();
        let mut probe = after.clone();
        probe.updated_at = before.updated_at.clone();
        if probe == before {
            return Ok(before);
        }
        after.updated_at = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE claims SET unit_id = ?2, excerpt = ?3, codes = ?4, ev_level = ?5, provenance = ?6,
               role = ?7, memo = ?8, updated_at = ?9, nature = ?10 WHERE claim_id = ?1",
            params![
                claim_id,
                after.unit_id,
                after.excerpt,
                to_json(&after.codes),
                after.ev_level,
                after.provenance,
                after.role,
                after.memo,
                after.updated_at,
                after.nature
            ],
        )?;
        append_event(
            &tx,
            actor,
            "claim.update",
            Some(&after.interview_id),
            Some(&after.unit_id),
            json!({"before": before, "after": after}),
        )?;
        tx.commit()?;
        Ok(after)
    }

    /// Deletes a claim. The audit event keeps the deleted content.
    pub fn delete_claim(&self, claim_id: i64, actor: &str) -> Result<()> {
        let before = self.get_claim(claim_id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM claims WHERE claim_id = ?1", params![claim_id])?;
        append_event(
            &tx,
            actor,
            "claim.delete",
            Some(&before.interview_id),
            Some(&before.unit_id),
            json!({"before": before, "after": null}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Claims that no longer hold on the current coding or text, with the
    /// reasons: a code no longer on the unit, or an excerpt no longer in the text.
    pub fn claims_to_review(&self, interview_id: &str) -> Result<Vec<(i64, String)>> {
        let claims = self.claims(interview_id)?;
        if claims.is_empty() {
            return Ok(Vec::new());
        }
        let coded = self.human_codings(interview_id, PHASE_2)?;
        let unit_ids: BTreeSet<String> = self
            .segmentation(interview_id)?
            .map(|s| s.units.into_iter().map(|u| u.unit_id).collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for claim in claims {
            let mut reasons = Vec::new();
            if !unit_ids.contains(&claim.unit_id) {
                reasons.push(format!(
                    "unit {} is no longer in the segmentation",
                    claim.unit_id
                ));
            } else {
                let unit = self.current_unit(interview_id, &claim.unit_id)?;
                if !unit.text.contains(claim.excerpt.as_str()) {
                    reasons.push("excerpt no longer in the unit text".to_string());
                }
                let current: CodeSet = coded
                    .get(&claim.unit_id)
                    .map(|h| h.codes.clone())
                    .unwrap_or_default();
                for code in claim.codes.iter().filter(|c| !current.contains(*c)) {
                    reasons.push(format!("code {code} no longer on the unit"));
                }
            }
            if !reasons.is_empty() {
                out.push((claim.claim_id, reasons.join("; ")));
            }
        }
        Ok(out)
    }

    fn get_claim(&self, claim_id: i64) -> Result<Claim> {
        self.query_claims("WHERE claim_id = ?1", params![claim_id])?
            .pop()
            .ok_or_else(|| not_found(format!("claim {claim_id}")))
    }

    fn query_claims(&self, sql_where: &str, p: &[&dyn rusqlite::ToSql]) -> Result<Vec<Claim>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CLAIM_COLUMNS} FROM claims {sql_where} ORDER BY claim_id"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok((
                Claim {
                    claim_id: r.get(0)?,
                    interview_id: r.get(1)?,
                    unit_id: r.get(2)?,
                    excerpt: r.get(3)?,
                    codes: CodeSet::new(),
                    ev_level: r.get(5)?,
                    provenance: r.get(6)?,
                    role: r.get(7)?,
                    memo: r.get(8)?,
                    created_at: r.get(9)?,
                    updated_at: r.get(10)?,
                    nature: r.get(11)?,
                },
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (mut claim, codes) = row?;
            claim.codes = from_json(&codes)?;
            out.push(claim);
        }
        Ok(out)
    }

    /// Validates a claim against the unit's current text and phase 2 coding
    /// and returns it cleaned. Identity fields are taken from the caller.
    fn check_claim(
        &self,
        interview_id: &str,
        claim: &Claim,
        stored_nature: Option<&str>,
    ) -> Result<Claim> {
        let unit = self.current_unit(interview_id, &claim.unit_id)?;
        if claim.excerpt.trim().is_empty() {
            return Err(validation(
                "excerpt is empty: quote the statement word for word",
            ));
        }
        if !unit.text.contains(claim.excerpt.as_str()) {
            return Err(validation(format!(
                "excerpt is not found word for word in unit {}",
                unit.unit_id
            )));
        }
        let ev_level = clean_opt(&claim.ev_level);
        closed("ev_level", &ev_level, &EV_LEVELS)?;
        let provenance = clean_opt(&claim.provenance);
        closed("provenance", &provenance, &PROVENANCES)?;
        let role = clean_opt(&claim.role);
        closed("role", &role, &CLAIM_ROLES)?;
        // The nature comes from the project list; a value already stored stays
        // accepted when the list changed afterwards.
        let nature = clean_opt(&claim.nature);
        if let Some(n) = nature.as_deref().filter(|n| Some(*n) != stored_nature) {
            let list = self.project_config()?.evidence_natures;
            if !list.iter().any(|i| i.id == n) {
                return Err(validation(format!(
                    "nature must be one of {}, not {n}",
                    list.iter()
                        .map(|i| i.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }

        let codes = clean_codes(&claim.codes);
        let current: CodeSet = self
            .human_codings(interview_id, PHASE_2)?
            .remove(&claim.unit_id)
            .map(|h| h.codes)
            .unwrap_or_default();
        let issues: Vec<String> = codes
            .iter()
            .filter(|c| !current.contains(*c))
            .map(|c| format!("code {c} is not a current phase 2 code of the unit"))
            .collect();
        if !issues.is_empty() {
            return Err(validation(format!(
                "{}: {}",
                unit.unit_id,
                issues.join("; ")
            )));
        }
        Ok(Claim {
            claim_id: 0,
            interview_id: interview_id.to_string(),
            unit_id: unit.unit_id,
            excerpt: claim.excerpt.clone(),
            codes,
            ev_level,
            provenance,
            role,
            memo: clean_opt(&claim.memo),
            nature,
            created_at: String::new(),
            updated_at: String::new(),
        })
    }
}
