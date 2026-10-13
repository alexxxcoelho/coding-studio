//! V1 lot 4 (Review): the unit-level coding written from the passages,
//! candidate concepts, the pseudonymisation list, the harmonisation worklist
//! and the review state per interview.
//!
//! - A unit is coded with X when any of its passages carries X. The coding of
//!   the unit follows every change of a passage coding (see
//!   [`Store::sync_unit_from_passages_in`]); a coding written by hand on a unit
//!   without passage is not touched.
//! - Candidate concepts arrive with a model answer and are decided by the
//!   researcher: create a code, attach to a code, reject. A decision needs a
//!   written reason and never changes a coding.
//! - The pseudonymisation list belongs to the project; the audit log keeps its
//!   digest and size, never the names.
//! - The harmonisation worklist lists the units to reread after a merge, a
//!   redefinition or a new sub-code.

use std::collections::BTreeSet;

use anyhow::Result;
use cs_model::pseudonym::{PseudonymConfig, PSEUDONYMISATION_KEY};
use cs_model::{CandidateConcept, CodeSet, PHASE_2};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::codetree::NewCode;
use crate::{append_event, from_json, guard_err, not_found, now, to_json, validation, Store};

/// `human_codings.source` of a coding written from the passages.
pub const SOURCE_PASSAGES: &str = "passages";

/// Longest accepted reason or note, in characters.
const MAX_REASON: usize = 4_000;

// ------------------------------------------------------------------ views

/// The decision on a candidate concept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConceptDecisionView {
    pub action: String,
    /// Code created or attached to (stable id); null for a rejection.
    pub code_id: Option<String>,
    pub rationale: String,
    pub decided_at: String,
    pub decided_by: String,
}

/// A candidate concept with its current decision and the earlier ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConceptCandidateView {
    pub candidate_id: String,
    pub interview_id: String,
    pub label: String,
    pub description: Option<String>,
    pub run_label: Option<String>,
    pub unit_ids: Vec<String>,
    pub evidence: Vec<cs_model::CandidateEvidence>,
    pub decision: Option<ConceptDecisionView>,
    /// Every decision, oldest first; the last one is `decision`.
    pub history: Vec<ConceptDecisionView>,
}

/// Body of a decision on a candidate.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConceptDecisionRequest {
    pub action: String,
    #[serde(default)]
    pub code_id: Option<String>,
    #[serde(default)]
    pub new_code_name: Option<String>,
    #[serde(default)]
    pub rationale: String,
}

/// One unit to reread after a change of the codebook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarmonisationView {
    pub item_id: i64,
    pub interview_id: String,
    pub unit_id: String,
    pub text: String,
    /// The codes of the unit now.
    pub codes: Vec<HarmonisationCode>,
    /// The code the change concerns.
    pub code_id: String,
    pub code_name: String,
    /// `merge`, `redefine` or `new_subcode`.
    pub cause: String,
    pub detail: String,
    pub created_at: String,
    pub created_by: String,
    /// `open` or `done`.
    pub status: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    /// `confirmed` or `corrected`.
    pub resolution: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarmonisationCode {
    pub token: String,
    pub code_id: String,
    pub name: String,
}

/// The state of the review of one interview, with the coverage a report
/// has to declare.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewStateView {
    pub interview_id: String,
    /// `not_done`, `in_progress` or `complete`.
    pub state: String,
    /// Units in the scope of the review (respondent units of phase 2).
    pub n_relevant: usize,
    /// Of those, the units a complete AI run answered.
    pub n_covered: usize,
    pub coverage: f64,
    /// Labels of the complete runs.
    pub runs: Vec<String>,
    pub n_batches: usize,
    pub n_disagreements: usize,
    pub n_decided: usize,
    pub n_sample: usize,
    pub n_sample_reviewed: usize,
    pub n_candidates: usize,
    pub n_candidates_decided: usize,
    pub harmonisation_open: usize,
    pub harmonisation_done: usize,
    /// One sentence for the documents of the study.
    pub declaration: String,
}

/// The review state of the corpus.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorpusReviewState {
    pub interviews: Vec<ReviewStateView>,
    pub n_interviews: usize,
    pub n_complete: usize,
    pub n_in_progress: usize,
    pub n_not_done: usize,
    pub declaration: String,
}

/// A name of the list with the replacements it makes in an interview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PseudonymUsage {
    pub kind: String,
    pub original: String,
    pub pseudonym: String,
    pub count: usize,
}

// ------------------------------------------------- unit coding from passages

impl Store {
    /// Writes the phase 2 coding of a unit from its passages: the union of the
    /// codes of its live passages (the reserved "Plus tard" excluded). A unit
    /// whose passages carry no code loses a coding that was written by this
    /// function, never one written by hand. The change after an AI answer
    /// covering the unit is flagged `revised_after_ai`, as for a hand edit.
    /// Runs inside the caller's transaction.
    pub(crate) fn sync_unit_from_passages_in(
        &self,
        conn: &Connection,
        interview_id: &str,
        unit_id: &str,
        actor: &str,
    ) -> Result<()> {
        let tokens: CodeSet = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT c.code FROM passage_codings pc
                 JOIN passages p USING (passage_id)
                 JOIN codes c ON c.code_id = pc.code_id
                 WHERE p.interview_id = ?1 AND p.unit_id = ?2
                   AND p.deleted_at IS NULL AND pc.removed_at IS NULL
                   AND c.active = 1 AND c.merged_into IS NULL
                 ORDER BY c.code",
            )?;
            let rows = stmt.query_map(params![interview_id, unit_id], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let existing: Option<(String, Option<String>, i64)> = conn
            .query_row(
                "SELECT codes, source, revised_after_ai FROM human_codings
                 WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3",
                params![interview_id, PHASE_2, unit_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let from_passages = existing
            .as_ref()
            .is_some_and(|(_, s, _)| s.as_deref() == Some(SOURCE_PASSAGES));
        let before: CodeSet = match &existing {
            Some((codes, _, _)) => from_json(codes)?,
            None => CodeSet::new(),
        };
        if tokens.is_empty() {
            if from_passages {
                conn.execute(
                    "DELETE FROM human_codings WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3",
                    params![interview_id, PHASE_2, unit_id],
                )?;
                append_event(
                    conn,
                    actor,
                    "coding.sync_passages",
                    Some(interview_id),
                    Some(unit_id),
                    json!({"phase": PHASE_2, "unit_id": unit_id, "before": before, "after": []}),
                )?;
            }
            return Ok(());
        }
        if before == tokens {
            // Already what the passages say (written by them, or by hand and equal).
            return Ok(());
        }
        let revised = existing.as_ref().is_some_and(|(_, _, r)| *r != 0)
            || (existing.is_some() && self.ai_covered(interview_id, unit_id)?);
        conn.execute(
            "INSERT INTO human_codings (interview_id, phase, unit_id, codes, subcode, modifier, ev_level,
               provenance, memo, excerpt, revised_after_ai, updated_at, source)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, NULL, ?5, ?6, ?7)
             ON CONFLICT(interview_id, phase, unit_id) DO UPDATE SET codes = excluded.codes,
               revised_after_ai = excluded.revised_after_ai, updated_at = excluded.updated_at,
               source = excluded.source",
            params![
                interview_id,
                PHASE_2,
                unit_id,
                to_json(&tokens),
                revised as i64,
                now(),
                SOURCE_PASSAGES
            ],
        )?;
        append_event(
            conn,
            actor,
            "coding.sync_passages",
            Some(interview_id),
            Some(unit_id),
            json!({
                "phase": PHASE_2,
                "unit_id": unit_id,
                "before": before,
                "after": tokens,
                "revised_after_ai": revised,
            }),
        )?;
        Ok(())
    }

    /// [`Self::sync_unit_from_passages_in`] for the unit of a passage.
    pub(crate) fn sync_passage_unit_in(
        &self,
        conn: &Connection,
        passage_id: &str,
        actor: &str,
    ) -> Result<()> {
        let unit: Option<(String, String)> = conn
            .query_row(
                "SELECT interview_id, unit_id FROM passages WHERE passage_id = ?1",
                [passage_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((interview_id, unit_id)) = unit {
            self.sync_unit_from_passages_in(conn, &interview_id, &unit_id, actor)?;
        }
        Ok(())
    }
}

// -------------------------------------------------------------- segmentation

impl Store {
    /// The current segmentation with the corrections of the transcript applied
    /// to the text and the role of the units. The hash is the segmentation's:
    /// it still binds the prompts and the answers to the stored units.
    pub fn corrected_segmentation(
        &self,
        interview_id: &str,
    ) -> Result<Option<cs_model::Segmentation>> {
        let Some(mut seg) = self.segmentation(interview_id)? else {
            return Ok(None);
        };
        let current = self.current_units(interview_id)?;
        for unit in &mut seg.units {
            if let Some(c) = current.get(&unit.unit_id) {
                *unit = c.clone();
            }
        }
        Ok(Some(seg))
    }
}

// -------------------------------------------------------------- candidates

/// (candidate_id, label, description, run_label, unit_ids, evidence).
type CandidateRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

impl Store {
    /// Stores the candidate concepts of an accepted answer part, inside the
    /// caller's transaction.
    pub(crate) fn record_candidates_in(
        &self,
        conn: &Connection,
        interview_id: &str,
        run_id: i64,
        part: u32,
        concepts: &[CandidateConcept],
        actor: &str,
    ) -> Result<()> {
        for c in concepts {
            let n: i64 =
                conn.query_row("SELECT COUNT(*) FROM concept_candidates", [], |r| r.get(0))?;
            let candidate_id = format!("CC-{:04}", n + 1);
            conn.execute(
                "INSERT INTO concept_candidates (candidate_id, interview_id, run_id, part, label,
                   description, unit_ids, evidence, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    candidate_id,
                    interview_id,
                    run_id,
                    part,
                    c.label,
                    c.description,
                    to_json(&c.unit_ids),
                    to_json(&c.evidence),
                    now()
                ],
            )?;
            append_event(
                conn,
                actor,
                "concept.candidate.record",
                Some(interview_id),
                Some(&candidate_id),
                json!({
                    "candidate_id": candidate_id,
                    "run_id": run_id,
                    "part": part,
                    "label": c.label,
                    "unit_ids": c.unit_ids,
                    "n_evidence": c.evidence.len(),
                }),
            )?;
        }
        Ok(())
    }

    fn decisions_of(&self, candidate_id: &str) -> Result<Vec<ConceptDecisionView>> {
        let mut stmt = self.conn.prepare(
            "SELECT action, code_id, rationale, decided_at, decided_by FROM concept_decisions
             WHERE candidate_id = ?1 ORDER BY decision_id",
        )?;
        let rows = stmt.query_map([candidate_id], |r| {
            Ok(ConceptDecisionView {
                action: r.get(0)?,
                code_id: r.get(1)?,
                rationale: r.get(2)?,
                decided_at: r.get(3)?,
                decided_by: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The candidate concepts of an interview, in order of arrival.
    pub fn concept_candidates(&self, interview_id: &str) -> Result<Vec<ConceptCandidateView>> {
        self.require_interview(interview_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT c.candidate_id, c.label, c.description, r.run_label, c.unit_ids, c.evidence
             FROM concept_candidates c LEFT JOIN ai_runs r USING (run_id)
             WHERE c.interview_id = ?1 ORDER BY c.candidate_id",
        )?;
        let rows: Vec<CandidateRow> = stmt
            .query_map([interview_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for (candidate_id, label, description, run_label, units, evidence) in rows {
            let history = self.decisions_of(&candidate_id)?;
            out.push(ConceptCandidateView {
                candidate_id,
                interview_id: interview_id.to_string(),
                label,
                description,
                run_label,
                unit_ids: from_json(&units)?,
                evidence: from_json(&evidence)?,
                decision: history.last().cloned(),
                history,
            });
        }
        Ok(out)
    }

    /// One candidate of an interview.
    pub fn concept_candidate(
        &self,
        interview_id: &str,
        candidate_id: &str,
    ) -> Result<ConceptCandidateView> {
        self.concept_candidates(interview_id)?
            .into_iter()
            .find(|c| c.candidate_id == candidate_id)
            .ok_or_else(|| {
                not_found(format!(
                    "candidate concept {candidate_id} of {interview_id}"
                ))
            })
    }

    /// Decides on a candidate: `create` a code, `map` it to an existing code,
    /// `reject` it. A written reason is required. Nothing is coded: the
    /// decision touches the codebook at most (a new code), never a coding. A
    /// decision can be replaced by a later one; the earlier ones stay on
    /// record. A candidate that created a code cannot create a second one.
    pub fn decide_concept_candidate(
        &self,
        interview_id: &str,
        candidate_id: &str,
        req: &ConceptDecisionRequest,
        actor: &str,
    ) -> Result<ConceptCandidateView> {
        let cand = self.concept_candidate(interview_id, candidate_id)?;
        let rationale = req.rationale.trim();
        if rationale.is_empty() {
            return Err(validation(
                "a decision on a candidate concept needs a written reason",
            ));
        }
        if rationale.chars().count() > MAX_REASON {
            return Err(validation(format!(
                "the reason is longer than {MAX_REASON} characters"
            )));
        }
        let action = req.action.trim();
        let tx = self.conn.unchecked_transaction()?;
        let code_id: Option<String> = match action {
            "create" => {
                if cand
                    .history
                    .iter()
                    .any(|d| d.action == "create" && d.code_id.is_some())
                {
                    return Err(guard_err(format!(
                        "{candidate_id} already created a code; attach it to that code instead"
                    )));
                }
                let name = req
                    .new_code_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| validation("new_code_name is required to create a code"))?;
                let definition = cand
                    .description
                    .clone()
                    .filter(|d| !d.trim().is_empty())
                    .unwrap_or_else(|| cand.label.clone());
                let id = self.create_code_v1_in(
                    &tx,
                    &NewCode {
                        name: name.to_string(),
                        definition,
                        reason: Some(format!(
                            "From candidate concept {candidate_id}: {rationale}"
                        )),
                        ..NewCode::default()
                    },
                    actor,
                )?;
                Some(id)
            }
            "map" => {
                let key = req
                    .code_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| validation("code_id is required to attach to a code"))?;
                let code = self.resolve_code(key)?;
                if code.system || !code.active {
                    return Err(validation(format!(
                        "{} cannot receive a candidate concept (reserved, inactive or merged)",
                        code.name
                    )));
                }
                Some(code.code_id)
            }
            "reject" => None,
            other => {
                return Err(validation(format!(
                    "action must be create, map or reject, not {other:?}"
                )))
            }
        };
        let ts = now();
        tx.execute(
            "INSERT INTO concept_decisions (candidate_id, action, code_id, rationale, decided_at, decided_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![candidate_id, action, code_id, rationale, ts, actor],
        )?;
        append_event(
            &tx,
            actor,
            "concept.candidate.decide",
            Some(interview_id),
            Some(candidate_id),
            json!({
                "candidate_id": candidate_id,
                "label": cand.label,
                "action": action,
                "code_id": code_id,
                "rationale": rationale,
                "replaces": cand.decision.as_ref().map(|d| json!({
                    "action": d.action, "code_id": d.code_id, "decided_at": d.decided_at,
                })),
                "codings_changed": 0,
            }),
        )?;
        tx.commit()?;
        self.concept_candidate(interview_id, candidate_id)
    }
}

// --------------------------------------------------------- pseudonymisation

impl Store {
    /// The project list of names and its switch (empty and off by default).
    pub fn pseudonym_config(&self) -> Result<PseudonymConfig> {
        match self.setting(PSEUDONYMISATION_KEY)? {
            Some(raw) => from_json(&raw),
            None => Ok(PseudonymConfig::default()),
        }
    }

    /// Validates, stores and audits the list. The audit event holds the digest
    /// and the size of the list, never the names.
    pub fn set_pseudonym_config(
        &self,
        cfg: &PseudonymConfig,
        actor: &str,
    ) -> Result<PseudonymConfig> {
        let stored = cfg.normalised()?;
        let before = self.pseudonym_config()?;
        if stored == before {
            return Ok(stored);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![PSEUDONYMISATION_KEY, to_json(&stored), now()],
        )?;
        append_event(
            &tx,
            actor,
            "pseudonymisation.update",
            None,
            Some(PSEUDONYMISATION_KEY),
            json!({
                "enabled_before": before.enabled,
                "enabled_after": stored.enabled,
                "n_entries_before": before.entries.len(),
                "n_entries_after": stored.entries.len(),
                "digest_before": before.digest(),
                "digest_after": stored.digest(),
                "categories": stored.categories(),
            }),
        )?;
        tx.commit()?;
        Ok(stored)
    }

    /// The names of the list that occur in the text of an interview (turns of
    /// both speakers: the respondent units and the questions that precede
    /// them), with the number of replacements the prompt makes. Empty when the
    /// option is off.
    pub fn pseudonym_usage(&self, interview_id: &str) -> Result<Vec<PseudonymUsage>> {
        self.require_interview(interview_id)?;
        let cfg = self.pseudonym_config()?;
        if !cfg.enabled {
            return Ok(Vec::new());
        }
        let rules = cfg.pseudonymiser();
        let mut counts = Vec::new();
        for unit in self.current_units(interview_id)?.values() {
            rules.mask_counted(&unit.text, &mut counts);
        }
        let originals = rules.originals();
        let mut out = Vec::new();
        for (i, original) in originals.iter().enumerate() {
            let n = counts.get(i).copied().unwrap_or(0);
            if n == 0 {
                continue;
            }
            if let Some(e) = cfg.entries.iter().find(|e| e.original == *original) {
                out.push(PseudonymUsage {
                    kind: e.kind.clone(),
                    original: e.original.clone(),
                    pseudonym: e.pseudonym.clone(),
                    count: n,
                });
            }
        }
        out.sort_by_key(|u| u.original.to_lowercase());
        Ok(out)
    }
}

// ----------------------------------------------------------- harmonisation

impl Store {
    /// (interview, unit) of the live passages carrying a code.
    pub(crate) fn units_of_code_passages_in(
        &self,
        conn: &Connection,
        code_id: &str,
    ) -> Result<BTreeSet<(String, String)>> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT p.interview_id, p.unit_id FROM passage_codings pc
             JOIN passages p USING (passage_id)
             WHERE pc.code_id = ?1 AND pc.removed_at IS NULL AND p.deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([code_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Opens an item for every unit that carries the code (by hand or through
    /// a passage), inside the caller's transaction.
    pub(crate) fn open_harmonisation_for_code_in(
        &self,
        conn: &Connection,
        code_id: &str,
        cause: &str,
        detail: &str,
        actor: &str,
    ) -> Result<()> {
        let token: Option<String> = conn
            .query_row(
                "SELECT code FROM codes WHERE code_id = ?1",
                [code_id],
                |r| r.get(0),
            )
            .optional()?;
        let mut units = self.units_of_code_passages_in(conn, code_id)?;
        if let Some(token) = token {
            let mut stmt = conn.prepare(
                "SELECT interview_id, unit_id FROM human_codings
                 WHERE phase = ?1 AND instr(codes, ?2) > 0",
            )?;
            let rows = stmt.query_map(params![PHASE_2, format!("\"{token}\"")], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                units.insert(row?);
            }
        }
        self.open_harmonisation_in(conn, code_id, cause, detail, &units, actor)
    }

    /// Opens an item per unit for the code; a unit that already has an open
    /// item for that code keeps it. One audit event for the whole change.
    pub(crate) fn open_harmonisation_in(
        &self,
        conn: &Connection,
        code_id: &str,
        cause: &str,
        detail: &str,
        units: &BTreeSet<(String, String)>,
        actor: &str,
    ) -> Result<()> {
        let mut opened: Vec<(String, String)> = Vec::new();
        for (interview_id, unit_id) in units {
            let n = conn.execute(
                "INSERT OR IGNORE INTO harmonisation_items
                   (interview_id, unit_id, code_id, cause, detail, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![interview_id, unit_id, code_id, cause, detail, now(), actor],
            )?;
            if n > 0 {
                opened.push((interview_id.clone(), unit_id.clone()));
            }
        }
        if !opened.is_empty() {
            append_event(
                conn,
                actor,
                "harmonisation.open",
                None,
                Some(code_id),
                json!({
                    "code_id": code_id,
                    "cause": cause,
                    "detail": detail,
                    "n_units": opened.len(),
                    "units": opened.iter().map(|(i, u)| json!({"interview_id": i, "unit_id": u})).collect::<Vec<_>>(),
                }),
            )?;
        }
        Ok(())
    }

    /// The worklist: the items of an interview (all interviews when `None`),
    /// open ones first, in unit order.
    pub fn harmonisation(&self, interview_id: Option<&str>) -> Result<Vec<HarmonisationView>> {
        if let Some(id) = interview_id {
            self.require_interview(id)?;
        }
        type Row = (
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        );
        let mut stmt = self.conn.prepare(
            "SELECT item_id, interview_id, unit_id, code_id, cause, detail, created_at,
                    resolved_at, resolved_by, resolution, note, created_by
             FROM harmonisation_items WHERE (?1 IS NULL OR interview_id = ?1)
             ORDER BY (resolved_at IS NOT NULL), interview_id, unit_id, item_id",
        )?;
        let rows: Vec<Row> = stmt
            .query_map([interview_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for (
            item_id,
            iid,
            unit_id,
            code_id,
            cause,
            detail,
            created_at,
            resolved_at,
            resolved_by,
            resolution,
            note,
            created_by,
        ) in rows
        {
            let units = self.current_units(&iid)?;
            let text = units
                .get(&unit_id)
                .map(|u| u.text.clone())
                .unwrap_or_default();
            let tokens = self
                .human_codings(&iid, PHASE_2)?
                .remove(&unit_id)
                .map(|c| c.codes)
                .unwrap_or_default();
            let mut codes = Vec::new();
            for t in tokens.iter().filter(|t| t.as_str() != cs_model::NA) {
                if let Ok(n) = self.code_node(t) {
                    codes.push(HarmonisationCode {
                        token: n.token,
                        code_id: n.code_id,
                        name: n.name,
                    });
                } else {
                    codes.push(HarmonisationCode {
                        token: t.clone(),
                        code_id: t.clone(),
                        name: t.clone(),
                    });
                }
            }
            let code_name = self
                .code_node(&code_id)
                .map(|n| n.name)
                .unwrap_or_else(|_| code_id.clone());
            out.push(HarmonisationView {
                item_id,
                interview_id: iid,
                unit_id,
                text,
                codes,
                code_id,
                code_name,
                cause,
                detail,
                created_at,
                created_by,
                status: if resolved_at.is_some() {
                    "done"
                } else {
                    "open"
                }
                .into(),
                resolved_at,
                resolved_by,
                resolution,
                note,
            });
        }
        Ok(out)
    }

    /// (open, done) items of an interview.
    pub fn harmonisation_counts(&self, interview_id: &str) -> Result<(usize, usize)> {
        let (open, done): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(resolved_at IS NULL), 0), COALESCE(SUM(resolved_at IS NOT NULL), 0)
             FROM harmonisation_items WHERE interview_id = ?1",
            [interview_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((open as usize, done as usize))
    }

    /// Closes an item: the researcher reread the unit and `confirmed` its
    /// coding or `corrected` it.
    pub fn resolve_harmonisation(
        &self,
        interview_id: &str,
        item_id: i64,
        resolution: &str,
        note: Option<&str>,
        actor: &str,
    ) -> Result<HarmonisationView> {
        self.require_interview(interview_id)?;
        if !["confirmed", "corrected"].contains(&resolution) {
            return Err(validation("resolution must be confirmed or corrected"));
        }
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        if note.is_some_and(|n| n.chars().count() > MAX_REASON) {
            return Err(validation(format!(
                "the note is longer than {MAX_REASON} characters"
            )));
        }
        let row: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT unit_id, resolved_at FROM harmonisation_items
                 WHERE item_id = ?1 AND interview_id = ?2",
                params![item_id, interview_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((unit_id, resolved_at)) = row else {
            return Err(not_found(format!(
                "harmonisation item {item_id} of {interview_id}"
            )));
        };
        if resolved_at.is_some() {
            return Err(guard_err(format!(
                "harmonisation item {item_id} is already done"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE harmonisation_items SET resolved_at = ?2, resolved_by = ?3, resolution = ?4, note = ?5
             WHERE item_id = ?1",
            params![item_id, now(), actor, resolution, note],
        )?;
        append_event(
            &tx,
            actor,
            "harmonisation.resolve",
            Some(interview_id),
            Some(&unit_id),
            json!({"item_id": item_id, "unit_id": unit_id, "resolution": resolution, "note": note}),
        )?;
        tx.commit()?;
        self.harmonisation(Some(interview_id))?
            .into_iter()
            .find(|v| v.item_id == item_id)
            .ok_or_else(|| not_found(format!("harmonisation item {item_id}")))
    }
}

// ------------------------------------------------------------ review state

impl Store {
    /// Where the review of an interview stands and what it covers.
    pub fn review_state(&self, interview_id: &str) -> Result<ReviewStateView> {
        let progress = self.simple_progress(interview_id)?;
        let relevant = self.phase_units(interview_id, PHASE_2)?;
        let runs = self.ai_runs(interview_id)?;
        let complete: Vec<_> = runs.iter().filter(|r| r.complete).collect();
        let covered: BTreeSet<&String> = complete.iter().flat_map(|r| r.unit_ids.iter()).collect();
        let n_covered = relevant.iter().filter(|u| covered.contains(*u)).count();
        let candidates = self.concept_candidates(interview_id)?;
        let n_candidates_decided = candidates.iter().filter(|c| c.decision.is_some()).count();
        let (harmonisation_open, harmonisation_done) = self.harmonisation_counts(interview_id)?;
        let started = progress.n_batches > 0 || !runs.is_empty();
        let finished = !relevant.is_empty()
            && n_covered == relevant.len()
            && progress.n_decided == progress.n_disagreements
            && progress.n_sample_reviewed == progress.n_sample
            && n_candidates_decided == candidates.len();
        let state = if finished {
            "complete"
        } else if started {
            "in_progress"
        } else {
            "not_done"
        };
        let labels: Vec<String> = complete.iter().map(|r| r.meta.run_label.clone()).collect();
        let declaration = match state {
            "complete" => format!(
                "Revue par un modèle faite sur les {} unités de l'entretien ({}), {} désaccords décidés sur {}.",
                relevant.len(),
                labels.join(", "),
                progress.n_decided,
                progress.n_disagreements
            ),
            "in_progress" => format!(
                "Revue par un modèle en cours : {} unités sur {} relues, {} désaccords décidés sur {}.",
                n_covered,
                relevant.len(),
                progress.n_decided,
                progress.n_disagreements
            ),
            _ => "Pas de revue par un modèle pour cet entretien.".to_string(),
        };
        Ok(ReviewStateView {
            interview_id: interview_id.to_string(),
            state: state.into(),
            n_relevant: relevant.len(),
            n_covered,
            coverage: if relevant.is_empty() {
                0.0
            } else {
                n_covered as f64 / relevant.len() as f64
            },
            runs: labels,
            n_batches: progress.n_batches,
            n_disagreements: progress.n_disagreements,
            n_decided: progress.n_decided,
            n_sample: progress.n_sample,
            n_sample_reviewed: progress.n_sample_reviewed,
            n_candidates: candidates.len(),
            n_candidates_decided,
            harmonisation_open,
            harmonisation_done,
            declaration,
        })
    }

    /// The review state of every interview and the sentence a report can use.
    pub fn corpus_review_state(&self) -> Result<CorpusReviewState> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT interview_id FROM interviews ORDER BY interview_id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut interviews = Vec::new();
        for id in ids {
            interviews.push(self.review_state(&id)?);
        }
        let count = |s: &str| interviews.iter().filter(|v| v.state == s).count();
        let (n_complete, n_in_progress, n_not_done) =
            (count("complete"), count("in_progress"), count("not_done"));
        let done: Vec<&str> = interviews
            .iter()
            .filter(|v| v.state == "complete")
            .map(|v| v.interview_id.as_str())
            .collect();
        let declaration = if interviews.is_empty() {
            "Aucun entretien dans le projet.".to_string()
        } else if done.is_empty() {
            format!(
                "Aucune revue par un modèle n'est terminée ({} entretiens, {} en cours).",
                interviews.len(),
                n_in_progress
            )
        } else {
            format!(
                "Revue par un modèle terminée sur {} entretiens sur {} ({}), en cours sur {}.",
                done.len(),
                interviews.len(),
                done.join(", "),
                n_in_progress
            )
        };
        Ok(CorpusReviewState {
            n_interviews: interviews.len(),
            interviews,
            n_complete,
            n_in_progress,
            n_not_done,
            declaration,
        })
    }
}
