//! Case syntheses (trigger, alternatives, constraints, choice, result, each
//! backed by units) and the cross-case comparison by research question.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::{Case, CaseField, CrossCase, CrossCaseCell, CrossCaseCode};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{append_event, clean_opt, from_json, not_found, now, to_json, validation, Store};

/// Stored form of the five synthesis fields.
#[derive(Serialize, Deserialize, Default)]
struct Fields {
    trigger: CaseField,
    alternatives: CaseField,
    constraints: CaseField,
    choice: CaseField,
    result: CaseField,
}

fn row_to_case(r: &rusqlite::Row<'_>) -> rusqlite::Result<(Case, String)> {
    Ok((
        Case {
            case_id: r.get(0)?,
            interview_id: r.get(1)?,
            label: r.get(2)?,
            trigger: CaseField::default(),
            alternatives: CaseField::default(),
            constraints: CaseField::default(),
            choice: CaseField::default(),
            result: CaseField::default(),
            stage: r.get(4)?,
            outcome_status: r.get(5)?,
            notes: r.get(6)?,
            updated_at: r.get(7)?,
        },
        r.get(3)?,
    ))
}

fn with_fields(mut case: Case, fields: &str) -> Result<Case> {
    let f: Fields = from_json(fields)?;
    case.trigger = f.trigger;
    case.alternatives = f.alternatives;
    case.constraints = f.constraints;
    case.choice = f.choice;
    case.result = f.result;
    Ok(case)
}

const CASE_COLUMNS: &str =
    "case_id, interview_id, label, fields, stage, outcome_status, notes, updated_at";

impl Store {
    /// Cases of one interview, or of every interview, by case_id.
    pub fn cases(&self, interview_id: Option<&str>) -> Result<Vec<Case>> {
        let rows: Vec<(Case, String)> = match interview_id {
            Some(id) => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {CASE_COLUMNS} FROM cases WHERE interview_id = ?1 ORDER BY case_id"
                ))?;
                let r = stmt.query_map([id], row_to_case)?;
                r.collect::<rusqlite::Result<_>>()?
            }
            None => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {CASE_COLUMNS} FROM cases ORDER BY interview_id, case_id"
                ))?;
                let r = stmt.query_map([], row_to_case)?;
                r.collect::<rusqlite::Result<_>>()?
            }
        };
        rows.into_iter().map(|(c, f)| with_fields(c, &f)).collect()
    }

    pub fn case(&self, case_id: &str) -> Result<Case> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {CASE_COLUMNS} FROM cases WHERE case_id = ?1"),
                [case_id],
                row_to_case,
            )
            .optional()?
            .ok_or_else(|| not_found(format!("case {case_id}")))?;
        with_fields(row.0, &row.1)
    }

    /// Normalises a case and checks its evidence against the current units.
    fn clean_case(&self, interview_id: &str, case: &Case) -> Result<Case> {
        let label = case.label.trim();
        if label.is_empty() {
            return Err(validation("a case needs a label"));
        }
        let units: BTreeSet<String> = self
            .segmentation(interview_id)?
            .map(|s| s.units.into_iter().map(|u| u.unit_id).collect())
            .unwrap_or_default();
        let clean = |f: &CaseField, name: &str| -> Result<CaseField> {
            let mut evidence: Vec<String> = Vec::new();
            for u in &f.evidence {
                let u = u.trim().to_string();
                if !units.contains(&u) {
                    return Err(validation(format!(
                        "{name}: unit {u} is not in the current segmentation of {interview_id}"
                    )));
                }
                if !evidence.contains(&u) {
                    evidence.push(u);
                }
            }
            Ok(CaseField {
                text: f.text.trim().to_string(),
                evidence,
            })
        };
        Ok(Case {
            case_id: case.case_id.trim().to_string(),
            interview_id: interview_id.to_string(),
            label: label.to_string(),
            trigger: clean(&case.trigger, "trigger")?,
            alternatives: clean(&case.alternatives, "alternatives")?,
            constraints: clean(&case.constraints, "constraints")?,
            choice: clean(&case.choice, "choice")?,
            result: clean(&case.result, "result")?,
            stage: clean_opt(&case.stage),
            outcome_status: clean_opt(&case.outcome_status),
            notes: clean_opt(&case.notes),
            updated_at: now(),
        })
    }

    fn write_case(&self, case: &Case, before: Option<&Case>, actor: &str) -> Result<()> {
        let fields = Fields {
            trigger: case.trigger.clone(),
            alternatives: case.alternatives.clone(),
            constraints: case.constraints.clone(),
            choice: case.choice.clone(),
            result: case.result.clone(),
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO cases (case_id, interview_id, label, fields, stage, outcome_status, notes, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(case_id) DO UPDATE SET label = excluded.label, fields = excluded.fields,
               stage = excluded.stage, outcome_status = excluded.outcome_status,
               notes = excluded.notes, updated_at = excluded.updated_at",
            params![
                case.case_id,
                case.interview_id,
                case.label,
                to_json(&fields),
                case.stage,
                case.outcome_status,
                case.notes,
                case.updated_at
            ],
        )?;
        append_event(
            &tx,
            actor,
            if before.is_some() {
                "case.update"
            } else {
                "case.create"
            },
            Some(&case.interview_id),
            Some(&case.case_id),
            json!({"before": before, "after": case}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// New case of an interview; `case_id` generated (`<ID>-C<n>`) when empty.
    pub fn create_case(&self, interview_id: &str, case: &Case, actor: &str) -> Result<Case> {
        self.require_interview(interview_id)?;
        let mut c = self.clean_case(interview_id, case)?;
        if c.case_id.is_empty() {
            let existing: BTreeSet<String> = self
                .cases(Some(interview_id))?
                .into_iter()
                .map(|c| c.case_id)
                .collect();
            let mut n = existing.len() + 1;
            while existing.contains(&format!("{interview_id}-C{n}")) {
                n += 1;
            }
            c.case_id = format!("{interview_id}-C{n}");
        }
        if c.case_id.chars().any(char::is_whitespace) {
            return Err(validation("a case id cannot contain spaces"));
        }
        if self.case(&c.case_id).is_ok() {
            return Err(validation(format!("case {} already exists", c.case_id)));
        }
        self.write_case(&c, None, actor)?;
        Ok(c)
    }

    /// Replaces the content of a case (its id and interview stay).
    pub fn update_case(&self, case_id: &str, case: &Case, actor: &str) -> Result<Case> {
        let before = self.case(case_id)?;
        let mut c = self.clean_case(&before.interview_id, case)?;
        c.case_id = before.case_id.clone();
        let mut cmp = c.clone();
        cmp.updated_at = before.updated_at.clone();
        if cmp == before {
            return Ok(before);
        }
        self.write_case(&c, Some(&before), actor)?;
        Ok(c)
    }

    /// For each research question and each case: the codes mapped to the
    /// question found in the final phase 2 coding of the case's interview, or
    /// of its evidence units when the case lists any.
    pub fn cross_case(&self) -> Result<CrossCase> {
        let questions = self.research_questions()?;
        let cases = self.cases(None)?;
        let book = self.codebook()?;
        let mut by_rq: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for c in &book.codes {
            for q in &c.research_questions {
                by_rq.entry(q.as_str()).or_default().insert(c.code.as_str());
            }
        }
        let mut finals_by_interview: BTreeMap<String, Vec<cs_model::FinalCoding>> = BTreeMap::new();
        let mut cells = Vec::new();
        for case in &cases {
            if !finals_by_interview.contains_key(&case.interview_id) {
                let (finals, _) = self.final_codings(&case.interview_id)?;
                finals_by_interview.insert(case.interview_id.clone(), finals);
            }
            let finals = &finals_by_interview[&case.interview_id];
            let evidence: BTreeSet<&String> = case
                .fields()
                .iter()
                .flat_map(|(_, f)| f.evidence.iter())
                .collect();
            for q in &questions {
                let mapped = by_rq.get(q.id.as_str());
                let mut codes: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for f in finals {
                    if !evidence.is_empty() && !evidence.contains(&f.unit_id) {
                        continue;
                    }
                    for code in &f.codes {
                        if mapped.is_some_and(|m| m.contains(code.as_str())) {
                            codes
                                .entry(code.clone())
                                .or_default()
                                .push(f.unit_id.clone());
                        }
                    }
                }
                cells.push(CrossCaseCell {
                    rq: q.id.clone(),
                    case_id: case.case_id.clone(),
                    codes: codes
                        .into_iter()
                        .map(|(code, units)| CrossCaseCode { code, units })
                        .collect(),
                });
            }
        }
        Ok(CrossCase {
            questions,
            cases,
            cells,
        })
    }
}
