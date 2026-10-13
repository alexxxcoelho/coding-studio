//! The candidate's coding: phase 1 pre-classification, phase 2 concrete
//! codes, scope, the blind guard and the labels to clarify.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::phase::{is_phase2_code, is_relevant, phase2_codes_for_categories};
use cs_model::{
    check_code_set, Code, CodeSet, CodingMap, HumanCoding, Resolution, ResolutionKind, SpeakerRole,
    ToClarify, NA, PHASE_1, PHASE_2,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use crate::{
    append_event, check_phase, clean_codes, clean_opt, enum_from, enum_str, from_json, not_found,
    now, to_json, validation, GuardStatus, Store,
};

/// How a label to clarify is resolved.
#[derive(Debug, Clone)]
pub struct ResolveRequest {
    pub kind: ResolutionKind,
    pub code: String,
    pub rationale: Option<String>,
    /// Resolve every open item with the same label (trimmed, case-insensitive).
    pub all_with_label: bool,
    /// Required for `NewCode`: the code to create (origin field).
    pub new_code: Option<Code>,
}

fn same_label(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Writes a coding row and its audit event on `conn`.
#[allow(clippy::too_many_arguments)]
fn write_coding(
    conn: &Connection,
    interview_id: &str,
    phase: u8,
    after: &HumanCoding,
    before: Option<&HumanCoding>,
    actor: &str,
    action: &str,
    extra: serde_json::Value,
) -> Result<()> {
    conn.execute(
        "INSERT INTO human_codings (interview_id, phase, unit_id, codes, subcode, modifier, ev_level,
           provenance, memo, excerpt, revised_after_ai, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(interview_id, phase, unit_id) DO UPDATE SET codes = excluded.codes, source = NULL,
           subcode = excluded.subcode, modifier = excluded.modifier, ev_level = excluded.ev_level,
           provenance = excluded.provenance, memo = excluded.memo, excerpt = excluded.excerpt,
           revised_after_ai = excluded.revised_after_ai, updated_at = excluded.updated_at",
        params![
            interview_id,
            phase,
            after.unit_id,
            to_json(&after.codes),
            after.subcode,
            after.modifier,
            after.ev_level,
            after.provenance,
            after.memo,
            after.excerpt,
            after.revised_after_ai as i64,
            now()
        ],
    )?;
    append_event(
        conn,
        actor,
        action,
        Some(interview_id),
        Some(&after.unit_id),
        json!({
            "phase": phase,
            "before": before,
            "after": after,
            "revised_after_ai": after.revised_after_ai,
            "extra": extra,
        }),
    )?;
    Ok(())
}

impl Store {
    // --------------------------------------------------------------- scope

    /// Units in scope for a phase, in segmentation order. Phase 1: every unit.
    /// Phase 2: the units whose current phase 1 category is not NA, never an
    /// interviewer turn.
    pub fn phase_units(&self, interview_id: &str, phase: u8) -> Result<Vec<String>> {
        check_phase(phase)?;
        let Some(seg) = self.segmentation(interview_id)? else {
            return Ok(Vec::new());
        };
        if phase == PHASE_1 {
            return Ok(seg.units.into_iter().map(|u| u.unit_id).collect());
        }
        let p1 = self.human_coding_map(interview_id, PHASE_1)?;
        Ok(seg
            .units
            .into_iter()
            .filter(|u| !matches!(u.role, SpeakerRole::Interviewer))
            .filter(|u| p1.get(&u.unit_id).is_some_and(is_relevant))
            .map(|u| u.unit_id)
            .collect())
    }

    /// Codes the candidate may use on a unit (NA is valid besides). Phase 1:
    /// the four categories. Phase 2: every phase 2 code, those attached to the
    /// unit's phase 1 contexts first.
    pub fn allowed_codes(&self, interview_id: &str, phase: u8, unit_id: &str) -> Result<Vec<Code>> {
        check_phase(phase)?;
        if phase == PHASE_1 {
            return Ok(self.phase1_codebook()?.codes);
        }
        let p1 = self
            .human_coding_map(interview_id, PHASE_1)?
            .remove(unit_id)
            .unwrap_or_default();
        let categories = self.study_config()?.effective_categories();
        let book = self.codebook()?;
        Ok(phase2_codes_for_categories(&book, &categories, &p1)
            .into_iter()
            .cloned()
            .collect())
    }

    fn validate_codes(&self, phase: u8, unit_id: &str, codes: &CodeSet) -> Result<()> {
        let mut issues = Vec::new();
        if codes.contains(NA) && codes.len() > 1 {
            issues.push("NA cannot be combined with other codes".to_string());
        }
        let phase1 = if phase == PHASE_1 {
            Some(self.phase1_codebook()?)
        } else {
            None
        };
        for code in codes.iter().filter(|c| c.as_str() != NA) {
            let ok = match &phase1 {
                Some(book) => book.get(code).is_some(),
                None => self.code_row(code)?.as_ref().is_some_and(is_phase2_code),
            };
            if !ok {
                issues.push(match &phase1 {
                    Some(book) => {
                        let names: Vec<&str> = book.codes.iter().map(|c| c.code.as_str()).collect();
                        format!(
                            "{code} is not a phase 1 category ({} or NA)",
                            names.join(", ")
                        )
                    }
                    None => format!("code {code} is unknown, inactive or merged"),
                });
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(validation(format!("{unit_id}: {}", issues.join("; "))))
        }
    }

    /// Whether an AI answer covering the unit has been imported.
    pub(crate) fn ai_covered(&self, interview_id: &str, unit_id: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ai_outputs o JOIN ai_runs r USING (run_id)
             WHERE r.interview_id = ?1 AND instr(o.parsed, ?2) > 0",
            params![interview_id, format!("\"{unit_id}\"")],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    // -------------------------------------------------------- human coding

    /// The candidate's coding of one unit. Phase 1: TECH, ORG, ENV, CONTEXT or
    /// NA, always editable. Phase 2: phase 2 codes or NA, on units in scope (or
    /// already coded); `codes` may be empty only while the unit has an open
    /// label to clarify. The excerpt must be found word for word in the unit.
    /// A phase 2 change after an AI answer covering the unit is flagged
    /// `revised_after_ai`. No event when unchanged.
    pub fn set_human_coding(
        &self,
        interview_id: &str,
        phase: u8,
        coding: &HumanCoding,
        actor: &str,
    ) -> Result<HumanCoding> {
        check_phase(phase)?;
        self.require_interview(interview_id)?;
        let unit = self.current_unit(interview_id, &coding.unit_id)?;
        let before = self
            .human_codings(interview_id, phase)?
            .remove(&coding.unit_id);
        let codes = clean_codes(&coding.codes);
        if phase == PHASE_2 {
            let in_scope = self
                .phase_units(interview_id, PHASE_2)?
                .contains(&coding.unit_id);
            if !in_scope && before.is_none() {
                return Err(validation(format!(
                    "unit {} is out of phase 2 scope: give it a phase 1 category other than NA first",
                    coding.unit_id
                )));
            }
            if codes.is_empty()
                && self
                    .open_to_clarify(interview_id, &coding.unit_id)?
                    .is_empty()
            {
                return Err(validation(
                    "no code given; use NA when no code applies, or add a label to clarify",
                ));
            }
        } else if let Some(issue) = check_code_set(&codes).into_iter().next() {
            return Err(validation(issue));
        }
        self.validate_codes(phase, &coding.unit_id, &codes)?;
        let mut after = HumanCoding {
            unit_id: coding.unit_id.clone(),
            codes,
            subcode: clean_opt(&coding.subcode),
            modifier: clean_opt(&coding.modifier),
            ev_level: clean_opt(&coding.ev_level),
            provenance: clean_opt(&coding.provenance),
            memo: clean_opt(&coding.memo),
            excerpt: clean_opt(&coding.excerpt),
            revised_after_ai: before.as_ref().is_some_and(|b| b.revised_after_ai),
        };
        if let Some(ex) = &after.excerpt {
            if !unit.text.contains(ex.as_str()) {
                return Err(validation(format!(
                    "excerpt is not found word for word in unit {}",
                    unit.unit_id
                )));
            }
        }
        let same = before.as_ref().is_some_and(|b| {
            let mut b = b.clone();
            b.revised_after_ai = after.revised_after_ai;
            b == after
        });
        if same {
            return Ok(before.expect("checked"));
        }
        if phase == PHASE_2 && self.ai_covered(interview_id, &after.unit_id)? {
            after.revised_after_ai = true;
        }
        let tx = self.conn.unchecked_transaction()?;
        write_coding(
            &tx,
            interview_id,
            phase,
            &after,
            before.as_ref(),
            actor,
            "coding.set",
            serde_json::Value::Null,
        )?;
        tx.commit()?;
        Ok(after)
    }

    pub fn human_codings(
        &self,
        interview_id: &str,
        phase: u8,
    ) -> Result<BTreeMap<String, HumanCoding>> {
        let mut stmt = self.conn.prepare(
            "SELECT unit_id, codes, subcode, modifier, ev_level, provenance, memo, excerpt, revised_after_ai
             FROM human_codings WHERE interview_id = ?1 AND phase = ?2",
        )?;
        let rows = stmt.query_map(params![interview_id, phase], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, i64>(8)? != 0,
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (unit_id, codes, subcode, modifier, ev_level, provenance, memo, excerpt, revised) =
                row?;
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
                    revised_after_ai: revised,
                },
            );
        }
        Ok(out)
    }

    pub fn human_coding_map(&self, interview_id: &str, phase: u8) -> Result<CodingMap> {
        Ok(self
            .human_codings(interview_id, phase)?
            .into_iter()
            .map(|(k, v)| (k, v.codes))
            .collect())
    }

    /// Units the candidate has coded in a phase (phase 2: codes, or an open
    /// label to clarify).
    pub(crate) fn coded_units(&self, interview_id: &str, phase: u8) -> Result<BTreeSet<String>> {
        let mut coded: BTreeSet<String> = self
            .human_coding_map(interview_id, phase)?
            .into_iter()
            .filter(|(_, c)| !c.is_empty())
            .map(|(u, _)| u)
            .collect();
        if phase == PHASE_2 {
            coded.extend(
                self.to_clarify_items(Some(interview_id), true)?
                    .into_iter()
                    .map(|t| t.unit_id),
            );
        }
        Ok(coded)
    }

    /// Coding progress of a phase: complete when every unit in scope is coded.
    pub fn guard(&self, interview_id: &str, phase: u8) -> Result<GuardStatus> {
        check_phase(phase)?;
        let units = self.phase_units(interview_id, phase)?;
        let coded = self.coded_units(interview_id, phase)?;
        let missing: Vec<String> = units
            .iter()
            .filter(|u| !coded.contains(*u))
            .cloned()
            .collect();
        Ok(GuardStatus {
            phase,
            complete: !units.is_empty() && missing.is_empty(),
            n_units: units.len(),
            missing,
        })
    }

    // ---------------------------------------------------------- to clarify

    fn to_clarify_query(
        &self,
        sql_where: &str,
        p: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<ToClarify>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT t.tid, t.interview_id, t.unit_id, t.label, t.note, t.created_at, t.created_by,
                    r.kind, r.code, r.rationale, r.at, r.by
             FROM to_clarify t LEFT JOIN to_clarify_resolutions r USING (tid)
             {sql_where} ORDER BY t.interview_id, t.tid"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok((
                ToClarify {
                    id: r.get(0)?,
                    interview_id: r.get(1)?,
                    unit_id: r.get(2)?,
                    label: r.get(3)?,
                    note: r.get(4)?,
                    created_at: r.get(5)?,
                    created_by: r.get(6)?,
                    resolved: None,
                },
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (mut item, kind, code, rationale, at, by) = row?;
            if let (Some(kind), Some(code), Some(at), Some(by)) = (kind, code, at, by) {
                item.resolved = Some(Resolution {
                    kind: enum_from(&kind)?,
                    code,
                    at,
                    by,
                    rationale,
                });
            }
            out.push(item);
        }
        Ok(out)
    }

    /// Labels to clarify, all or those of one interview, open ones only on request.
    pub fn to_clarify_items(
        &self,
        interview_id: Option<&str>,
        open_only: bool,
    ) -> Result<Vec<ToClarify>> {
        let open = if open_only { "r.tid IS NULL" } else { "1 = 1" };
        match interview_id {
            Some(id) => {
                self.to_clarify_query(&format!("WHERE t.interview_id = ?1 AND {open}"), &[&id])
            }
            None => self.to_clarify_query(&format!("WHERE {open}"), &[]),
        }
    }

    pub fn to_clarify_item(&self, tid: i64) -> Result<ToClarify> {
        self.to_clarify_query("WHERE t.tid = ?1", &[&tid])?
            .pop()
            .ok_or_else(|| not_found(format!("label to clarify {tid}")))
    }

    fn open_to_clarify(&self, interview_id: &str, unit_id: &str) -> Result<Vec<ToClarify>> {
        Ok(self
            .to_clarify_items(Some(interview_id), true)?
            .into_iter()
            .filter(|t| t.unit_id == unit_id)
            .collect())
    }

    /// Puts a provisional label and a short note on a unit in phase 2 scope.
    pub fn create_to_clarify(
        &self,
        interview_id: &str,
        unit_id: &str,
        label: &str,
        note: &str,
        actor: &str,
    ) -> Result<ToClarify> {
        self.require_interview(interview_id)?;
        self.current_unit(interview_id, unit_id)?;
        if !self
            .phase_units(interview_id, PHASE_2)?
            .iter()
            .any(|u| u == unit_id)
        {
            return Err(validation(format!(
                "unit {unit_id} is out of phase 2 scope: give it a phase 1 category other than NA first"
            )));
        }
        let label = label.trim();
        if label.is_empty() {
            return Err(validation("a label to clarify needs a provisional label"));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO to_clarify (interview_id, unit_id, label, note, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![interview_id, unit_id, label, note.trim(), now(), actor],
        )?;
        let tid = tx.last_insert_rowid();
        append_event(
            &tx,
            actor,
            "to_clarify.create",
            Some(interview_id),
            Some(unit_id),
            json!({"id": tid, "label": label, "note": note.trim()}),
        )?;
        tx.commit()?;
        self.to_clarify_item(tid)
    }

    /// Edits the label or the note of an open item.
    pub fn update_to_clarify(
        &self,
        tid: i64,
        label: &str,
        note: &str,
        actor: &str,
    ) -> Result<ToClarify> {
        let before = self.to_clarify_item(tid)?;
        if before.resolved.is_some() {
            return Err(validation("this label was already resolved"));
        }
        let label = label.trim();
        if label.is_empty() {
            return Err(validation("a label to clarify needs a provisional label"));
        }
        if before.label == label && before.note == note.trim() {
            return Ok(before);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE to_clarify SET label = ?2, note = ?3 WHERE tid = ?1",
            params![tid, label, note.trim()],
        )?;
        append_event(
            &tx,
            actor,
            "to_clarify.update",
            Some(&before.interview_id),
            Some(&before.unit_id),
            json!({"id": tid, "before": {"label": before.label, "note": before.note},
                   "after": {"label": label, "note": note.trim()}}),
        )?;
        tx.commit()?;
        self.to_clarify_item(tid)
    }

    /// Resolves an item (and, on request, every open item with the same label)
    /// to a new field code or to an existing code; the code is added to the
    /// phase 2 coding of each unit concerned. One transaction.
    pub fn resolve_to_clarify(
        &self,
        tid: i64,
        req: &ResolveRequest,
        actor: &str,
    ) -> Result<Vec<ToClarify>> {
        let item = self.to_clarify_item(tid)?;
        if item.resolved.is_some() {
            return Err(validation("this label was already resolved"));
        }
        let code = req.code.trim().to_string();
        let items: Vec<ToClarify> = if req.all_with_label {
            self.to_clarify_items(None, true)?
                .into_iter()
                .filter(|t| same_label(&t.label, &item.label))
                .collect()
        } else {
            vec![item.clone()]
        };
        let tx = self.conn.unchecked_transaction()?;
        match req.kind {
            ResolutionKind::NewCode => {
                let mut new = req
                    .new_code
                    .clone()
                    .ok_or_else(|| validation("new_code is required to create a code"))?;
                if new.code.trim() != code {
                    return Err(validation("code and new_code.code differ"));
                }
                if new.origin.trim().is_empty() {
                    new.origin = "field".to_string();
                }
                if new.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
                    new.reason = Some(format!(
                        "Created from the label to clarify \"{}\"",
                        item.label
                    ));
                }
                self.insert_code(&tx, &new, actor)?;
            }
            ResolutionKind::Mapped => {
                let target = self
                    .code_row(&code)?
                    .ok_or_else(|| not_found(format!("code {code}")))?;
                if !is_phase2_code(&target) {
                    return Err(validation(format!(
                        "code {code} is inactive, merged or not a phase 2 code"
                    )));
                }
            }
        }
        let rationale = clean_opt(&req.rationale);
        let at = now();
        for t in &items {
            tx.execute(
                "INSERT INTO to_clarify_resolutions (tid, kind, code, rationale, at, by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![t.id, enum_str(&req.kind), code, rationale, at, actor],
            )?;
            let before = self
                .human_codings(&t.interview_id, PHASE_2)?
                .remove(&t.unit_id);
            let mut after = before.clone().unwrap_or_else(|| HumanCoding {
                unit_id: t.unit_id.clone(),
                ..HumanCoding::default()
            });
            after.codes.remove(NA);
            after.codes.insert(code.clone());
            if self.ai_covered(&t.interview_id, &t.unit_id)? {
                after.revised_after_ai = true;
            }
            write_coding(
                &tx,
                &t.interview_id,
                PHASE_2,
                &after,
                before.as_ref(),
                actor,
                "to_clarify.resolve",
                json!({
                    "id": t.id,
                    "label": t.label,
                    "note": t.note,
                    "kind": req.kind,
                    "code": code,
                    "rationale": rationale,
                }),
            )?;
        }
        tx.commit()?;
        items.iter().map(|t| self.to_clarify_item(t.id)).collect()
    }

    /// Open labels of one unit, for the coding view.
    pub fn unit_to_clarify(&self, interview_id: &str, unit_id: &str) -> Result<Vec<ToClarify>> {
        Ok(self
            .to_clarify_items(Some(interview_id), false)?
            .into_iter()
            .filter(|t| t.unit_id == unit_id)
            .collect())
    }

    /// Whether a unit has any phase 2 coding row (used to keep out-of-scope rows visible).
    pub fn has_phase2_coding(&self, interview_id: &str, unit_id: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM human_codings WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3",
                params![interview_id, PHASE_2, unit_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}
