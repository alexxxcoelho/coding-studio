//! The simple view (v5): automatic phase 1 at segmentation, the TOE contexts
//! derived from the phase 2 codes, the relevance shorthand, the three-step
//! progress (code, confront, arbitrate).
//!
//! Automatic phase 1 is stored with `modifier = "auto"`. That marker is the only
//! thing that tells an automatic coding from a candidate's: any candidate write
//! replaces the row without it, and the config guard and the re-segmentation
//! lock ignore rows that still carry it.

use std::collections::BTreeSet;

use anyhow::Result;
use cs_model::phase::{is_relevant, CONTEXT};
use cs_model::{
    family_for_toe, Category, CodeSet, HumanCoding, ReviewKind, SpeakerRole, Unit, NA, PHASE_1,
    PHASE_2,
};
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::{now, validation, Store};

/// `modifier` of an automatic phase 1 coding.
pub(crate) const AUTO_MODIFIER: &str = "auto";

/// Counts behind the three-step header. Served by `GET /api/interviews/{id}/simple`.
///
/// - `n_respondent_units`: units whose current role is not interviewer.
/// - `n_relevant`: units in phase 2 scope.
/// - `n_coded`: relevant units the candidate has coded in phase 2.
/// - `n_batches`: prompt batches generated.
/// - `n_runs_complete`: complete AI runs.
/// - `n_disagreements`, `n_decided`: disagreements with a complete run, and those decided.
/// - `n_sample`, `n_sample_reviewed`: sampled agreements, and those decided.
/// - `done`: every relevant unit coded, no pending mark or harmonisation,
///   and every existing disagreement, sample and concept candidate decided.
///   AI coverage is reported separately: an AI review is optional.
///   Completion does not certify audio verification or independent validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleProgress {
    pub n_respondent_units: usize,
    pub n_relevant: usize,
    pub n_coded: usize,
    pub n_batches: usize,
    pub n_runs_complete: usize,
    pub n_disagreements: usize,
    pub n_decided: usize,
    pub n_sample: usize,
    pub n_sample_reviewed: usize,
    /// Respondent units marked "to review".
    pub n_to_review: usize,
    pub done: bool,
}

/// Units filled by one automatic phase 1 pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AutoFill {
    pub n_filled: usize,
    pub n_na: usize,
    pub n_context: usize,
    pub n_kept: usize,
}

/// The category given to respondent and unknown units: CONTEXT when configured,
/// else the first category without attachment, else the first category.
pub(crate) fn context_category(categories: &[Category]) -> Result<String> {
    categories
        .iter()
        .find(|c| c.name == CONTEXT)
        .or_else(|| categories.iter().find(|c| c.attachment.is_none()))
        .or_else(|| categories.first())
        .map(|c| c.name.clone())
        .ok_or_else(|| validation("no phase 1 category is configured"))
}

/// Fills the phase 1 coding of every unit of a segmentation that has none:
/// interviewer turns get NA, the others the context category. Rows written
/// here carry the automatic marker. Units with a coding are kept untouched.
pub(crate) fn fill_phase1(
    conn: &Connection,
    interview_id: &str,
    units: &[Unit],
    categories: &[Category],
) -> Result<AutoFill> {
    let context = context_category(categories)?;
    let existing: BTreeSet<String> = {
        let mut stmt = conn
            .prepare("SELECT unit_id FROM human_codings WHERE interview_id = ?1 AND phase = ?2")?;
        let rows = stmt.query_map(params![interview_id, PHASE_1], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut out = AutoFill::default();
    for unit in units {
        if existing.contains(&unit.unit_id) {
            out.n_kept += 1;
            continue;
        }
        let code = match unit.role {
            SpeakerRole::Interviewer => NA.to_string(),
            _ => context.clone(),
        };
        let codes: CodeSet = [code.clone()].into_iter().collect();
        conn.execute(
            "INSERT INTO human_codings (interview_id, phase, unit_id, codes, subcode, modifier, ev_level,
               provenance, memo, excerpt, revised_after_ai, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, NULL, NULL, NULL, NULL, 0, ?6)",
            params![
                interview_id,
                PHASE_1,
                unit.unit_id,
                crate::to_json(&codes),
                AUTO_MODIFIER,
                now()
            ],
        )?;
        out.n_filled += 1;
        if code == NA {
            out.n_na += 1;
        } else {
            out.n_context += 1;
        }
    }
    Ok(out)
}

/// Order of the derived contexts.
fn toe_rank(family: &str) -> Option<u8> {
    match family {
        "TECH" => Some(0),
        "ORG" => Some(1),
        "ENV" => Some(2),
        _ => None,
    }
}

impl Store {
    /// Units of the current segmentation whose role is interviewer.
    pub fn interviewer_units(&self, interview_id: &str) -> Result<BTreeSet<String>> {
        // The role of a unit may have been corrected: the corrected role rules.
        Ok(self
            .corrected_segmentation(interview_id)?
            .map(|s| {
                s.units
                    .into_iter()
                    .filter(|u| matches!(u.role, SpeakerRole::Interviewer))
                    .map(|u| u.unit_id)
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Category given to respondent and unknown units by the configuration.
    pub(crate) fn context_category_name(&self) -> Result<String> {
        context_category(&self.study_config()?.effective_categories())
    }

    /// Records the relevance of a unit in phase 1. `false` stores NA. `true`
    /// stores the context category unless the unit already carries a category
    /// other than NA, which is kept. Interviewer turns are never relevant.
    /// Written as a candidate coding (the automatic marker is cleared).
    pub fn set_relevance(
        &self,
        interview_id: &str,
        phase: u8,
        unit_id: &str,
        relevant: bool,
        actor: &str,
    ) -> Result<HumanCoding> {
        if phase != PHASE_1 {
            return Err(validation(
                "relevance is a phase 1 setting; phase 2 is coded with codes",
            ));
        }
        self.require_interview(interview_id)?;
        let unit = self.current_unit(interview_id, unit_id)?;
        let before = self.human_codings(interview_id, PHASE_1)?.remove(unit_id);
        let codes: CodeSet = if !relevant {
            [NA.to_string()].into_iter().collect()
        } else {
            if matches!(unit.role, SpeakerRole::Interviewer) {
                return Err(validation(format!(
                    "{unit_id} is an interviewer turn and cannot be relevant"
                )));
            }
            let kept: CodeSet = before
                .as_ref()
                .map(|b| {
                    b.codes
                        .iter()
                        .filter(|c| c.as_str() != NA)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if kept.is_empty() {
                [self.context_category_name()?].into_iter().collect()
            } else {
                kept
            }
        };
        let mut coding = before.unwrap_or_default();
        coding.unit_id = unit_id.to_string();
        coding.codes = codes;
        coding.modifier = None;
        self.set_human_coding(interview_id, PHASE_1, &coding, actor)
    }

    /// TOE contexts derived from the attachments of the unit's current phase 2
    /// codes: T gives TECH, O gives ORG, E gives ENV, deduplicated, in the
    /// order TECH, ORG, ENV. A relevant unit with no attached code (or only NA,
    /// or no phase 2 coding yet) gives `["CONTEXT"]`. A unit out of phase 2
    /// scope (interviewer, or NA in phase 1) gives an empty list.
    pub fn phase2_context_for_unit(
        &self,
        interview_id: &str,
        unit_id: &str,
    ) -> Result<Vec<String>> {
        let unit = self.current_unit(interview_id, unit_id)?;
        if matches!(unit.role, SpeakerRole::Interviewer) {
            return Ok(Vec::new());
        }
        let p1 = self
            .human_coding_map(interview_id, PHASE_1)?
            .remove(unit_id)
            .unwrap_or_default();
        if !is_relevant(&p1) {
            return Ok(Vec::new());
        }
        let p2 = self
            .human_codings(interview_id, PHASE_2)?
            .remove(unit_id)
            .map(|c| c.codes)
            .unwrap_or_default();
        let mut families: Vec<&'static str> = Vec::new();
        for code in p2.iter().filter(|c| c.as_str() != NA) {
            let Some(row) = self.code_row(code)? else {
                continue;
            };
            let family = family_for_toe(row.toe.as_deref());
            if toe_rank(family).is_some() && !families.contains(&family) {
                families.push(family);
            }
        }
        if families.is_empty() {
            return Ok(vec![CONTEXT.to_string()]);
        }
        families.sort_by_key(|f| toe_rank(f));
        Ok(families.into_iter().map(str::to_string).collect())
    }

    /// Progress of an interview for the three-step header.
    pub fn simple_progress(&self, interview_id: &str) -> Result<SimpleProgress> {
        self.require_interview(interview_id)?;
        let units = self
            .segmentation(interview_id)?
            .map(|s| s.units)
            .unwrap_or_default();
        let n_respondent_units = units
            .iter()
            .filter(|u| !matches!(u.role, SpeakerRole::Interviewer))
            .count();

        let relevant = self.phase_units(interview_id, PHASE_2)?;
        let coded = self.coded_units(interview_id, PHASE_2)?;
        let n_coded = relevant.iter().filter(|u| coded.contains(*u)).count();

        let n_batches: i64 = self.connection().query_row(
            "SELECT COUNT(DISTINCT batch_id) FROM prompts WHERE interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        let runs = self.ai_runs(interview_id)?;
        let complete: Vec<_> = runs.iter().filter(|r| r.complete).collect();

        let review = self.review(interview_id)?;
        let disagreements: Vec<_> = review
            .items
            .iter()
            .filter(|e| e.item.kind == ReviewKind::Disagreement)
            .collect();
        let samples: Vec<_> = review
            .items
            .iter()
            .filter(|e| e.item.kind == ReviewKind::AgreementSample)
            .collect();
        let n_decided = disagreements
            .iter()
            .filter(|e| e.item.decision.is_some())
            .count();
        let n_sample_reviewed = samples.iter().filter(|e| e.item.decision.is_some()).count();

        let n_to_review = self.unit_marks(interview_id)?.len();
        let (harmonisation_open, _) = self.harmonisation_counts(interview_id)?;
        let candidates = self.concept_candidates(interview_id)?;
        let done = n_respondent_units > 0
            && n_coded == relevant.len()
            && n_to_review == 0
            && harmonisation_open == 0
            && n_decided == disagreements.len()
            && n_sample_reviewed == samples.len()
            && candidates.iter().all(|c| c.decision.is_some());

        Ok(SimpleProgress {
            n_respondent_units,
            n_relevant: relevant.len(),
            n_coded,
            n_batches: n_batches as usize,
            n_runs_complete: complete.len(),
            n_disagreements: disagreements.len(),
            n_decided,
            n_sample: samples.len(),
            n_sample_reviewed,
            n_to_review,
            done,
        })
    }
}
