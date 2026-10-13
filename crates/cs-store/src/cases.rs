//! Case syntheses (trigger, alternatives, constraints, choice, result, each
//! backed by units) and the cross-case comparison by research question.
//!
//! A case attributes units from any interview (`case_units`). Its
//! `interviews` are derived from those units plus its `interview_id`, never
//! trusted from input.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::{Case, CaseField, CrossCase, CrossCaseCell, CrossCaseCode, FinalCoding};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{append_event, clean_opt, from_json, not_found, now, to_json, validation, Store};

/// Stored form of the synthesis fields (the `cases.fields` JSON blob).
#[derive(Serialize, Deserialize, Default)]
struct Fields {
    trigger: CaseField,
    alternatives: CaseField,
    constraints: CaseField,
    choice: CaseField,
    result: CaseField,
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    granularity: Option<String>,
    #[serde(default)]
    carrier_chosen: Option<String>,
    #[serde(default)]
    carriers_rejected: Vec<String>,
    #[serde(default)]
    account_type: Option<String>,
    #[serde(default)]
    swiss_link: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    characteristics: BTreeMap<String, String>,
    #[serde(default)]
    passages: Vec<String>,
}

/// Interview of a unit id: the prefix before `-U` (`ECO-16-U002` gives `ECO-16`).
fn interview_of(unit_id: &str) -> Option<&str> {
    unit_id.rsplit_once("-U").map(|(interview, _)| interview)
}

/// Trims each entry and drops the empty ones.
fn clean_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
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
            units: Vec::new(),
            interviews: Vec::new(),
            decision: None,
            granularity: None,
            carrier_chosen: None,
            carriers_rejected: Vec::new(),
            account_type: None,
            swiss_link: None,
            stage: r.get(4)?,
            outcome_status: r.get(5)?,
            notes: r.get(6)?,
            outcome: None,
            characteristics: BTreeMap::new(),
            passages: Vec::new(),
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
    case.decision = f.decision;
    case.granularity = f.granularity;
    case.carrier_chosen = f.carrier_chosen;
    case.carriers_rejected = f.carriers_rejected;
    case.account_type = f.account_type;
    case.swiss_link = f.swiss_link;
    // Data written before `outcome` existed: read it from the free-text decision.
    case.outcome = f.outcome.or_else(|| {
        case.decision
            .as_deref()
            .and_then(cs_model::analysis::normalise_outcome)
            .map(str::to_string)
    });
    case.characteristics = f.characteristics;
    case.passages = f.passages;
    Ok(case)
}

const CASE_COLUMNS: &str =
    "case_id, interview_id, label, fields, stage, outcome_status, notes, updated_at";

impl Store {
    /// Cases of one interview (created from it, or attributing one of its
    /// units), or of every interview, by case_id. Each case appears once.
    pub fn cases(&self, interview_id: Option<&str>) -> Result<Vec<Case>> {
        let rows: Vec<(Case, String)> = match interview_id {
            Some(id) => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {CASE_COLUMNS} FROM cases
                     WHERE interview_id = ?1
                        OR case_id IN (SELECT case_id FROM case_units WHERE interview_id = ?1)
                     ORDER BY case_id"
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
        rows.into_iter().map(|(c, f)| self.hydrate(c, &f)).collect()
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
        self.hydrate(row.0, &row.1)
    }

    /// Fills the JSON fields, the attributed units (from `case_units`) and
    /// the derived `interviews`.
    fn hydrate(&self, case: Case, fields: &str) -> Result<Case> {
        let mut case = with_fields(case, fields)?;
        let mut stmt = self.conn.prepare(
            "SELECT unit_id, interview_id FROM case_units WHERE case_id = ?1 ORDER BY unit_id",
        )?;
        let rows = stmt.query_map([&case.case_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut units = Vec::new();
        let mut interviews = BTreeSet::from([case.interview_id.clone()]);
        for row in rows {
            let (unit, interview) = row?;
            units.push(unit);
            interviews.insert(interview);
        }
        case.units = units;
        case.interviews = interviews.into_iter().collect();
        Ok(case)
    }

    /// Normalises a case and checks its units and evidence against the
    /// current segmentations.
    fn clean_case(&self, interview_id: &str, case: &Case) -> Result<Case> {
        let label = case.label.trim();
        if label.is_empty() {
            return Err(validation("a case needs a label"));
        }

        let mut units: BTreeSet<String> = BTreeSet::new();
        let mut current: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for unit in case
            .units
            .iter()
            .map(|u| u.trim())
            .filter(|u| !u.is_empty())
        {
            let owner = interview_of(unit).ok_or_else(|| {
                validation(format!(
                    "unit {unit} is not in the current segmentation of any interview"
                ))
            })?;
            if !current.contains_key(owner) {
                let ids: BTreeSet<String> = self
                    .segmentation(owner)?
                    .map(|s| s.units.into_iter().map(|u| u.unit_id).collect())
                    .unwrap_or_default();
                current.insert(owner.to_string(), ids);
            }
            if !current.get(owner).is_some_and(|ids| ids.contains(unit)) {
                return Err(validation(format!(
                    "unit {unit} is not in the current segmentation of {owner}"
                )));
            }
            units.insert(unit.to_string());
        }

        let clean = |f: &CaseField, name: &str| -> Result<CaseField> {
            let mut evidence: Vec<String> = Vec::new();
            for u in &f.evidence {
                let u = u.trim().to_string();
                if !units.contains(&u) {
                    return Err(validation(format!(
                        "{name}: unit {u} is not attributed to the case (add it to units first)"
                    )));
                }
                if !evidence.contains(&u) {
                    evidence.push(u);
                }
            }
            Ok(CaseField {
                text: f.text.trim().to_string(),
                contradictions: f.contradictions.trim().to_string(),
                missing: f.missing.trim().to_string(),
                evidence,
            })
        };

        let mut interviews = BTreeSet::from([interview_id.to_string()]);
        for unit in &units {
            if let Some(owner) = interview_of(unit) {
                interviews.insert(owner.to_string());
            }
        }

        // Outcome: the guide's three words. Earlier free text is understood.
        let outcome = match clean_opt(&case.outcome) {
            Some(o) => Some(
                cs_model::analysis::normalise_outcome(&o)
                    .ok_or_else(|| {
                        validation(format!(
                            "outcome must be one of {}, not {o}",
                            cs_model::analysis::DECISION_OUTCOMES.join(", ")
                        ))
                    })?
                    .to_string(),
            ),
            None => case
                .decision
                .as_deref()
                .and_then(cs_model::analysis::normalise_outcome)
                .map(str::to_string),
        };
        let mut characteristics = BTreeMap::new();
        for (k, v) in &case.characteristics {
            let (k, v) = (k.trim(), v.trim());
            if k.is_empty() {
                return Err(validation("a characteristic needs a name"));
            }
            if k.chars().count() > 100 || k.contains('\n') || v.chars().count() > 1000 {
                return Err(validation(format!(
                    "characteristic {k}: name at most 100 characters on one line, value at most 1000"
                )));
            }
            characteristics.insert(k.to_string(), v.to_string());
        }
        if characteristics.len() > 40 {
            return Err(validation("at most 40 characteristics per decision"));
        }
        let mut passages: Vec<String> = Vec::new();
        for p in case
            .passages
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
        {
            self.passage(p)
                .map_err(|_| validation(format!("unknown passage {p}")))?;
            if !passages.iter().any(|x| x == p) {
                passages.push(p.to_string());
            }
        }
        Ok(Case {
            outcome,
            characteristics,
            passages,
            case_id: case.case_id.trim().to_string(),
            interview_id: interview_id.to_string(),
            label: label.to_string(),
            trigger: clean(&case.trigger, "trigger")?,
            alternatives: clean(&case.alternatives, "alternatives")?,
            constraints: clean(&case.constraints, "constraints")?,
            choice: clean(&case.choice, "choice")?,
            result: clean(&case.result, "result")?,
            units: units.into_iter().collect(),
            interviews: interviews.into_iter().collect(),
            decision: clean_opt(&case.decision),
            granularity: clean_opt(&case.granularity),
            carrier_chosen: clean_opt(&case.carrier_chosen),
            carriers_rejected: clean_list(&case.carriers_rejected),
            account_type: clean_opt(&case.account_type),
            swiss_link: clean_opt(&case.swiss_link),
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
            decision: case.decision.clone(),
            granularity: case.granularity.clone(),
            carrier_chosen: case.carrier_chosen.clone(),
            carriers_rejected: case.carriers_rejected.clone(),
            account_type: case.account_type.clone(),
            swiss_link: case.swiss_link.clone(),
            outcome: case.outcome.clone(),
            characteristics: case.characteristics.clone(),
            passages: case.passages.clone(),
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
        tx.execute(
            "DELETE FROM case_units WHERE case_id = ?1",
            params![case.case_id],
        )?;
        for unit in &case.units {
            let owner = interview_of(unit).ok_or_else(|| {
                validation(format!("unit {unit} has no interview: not a unit id"))
            })?;
            tx.execute(
                "INSERT INTO case_units (case_id, unit_id, interview_id) VALUES (?1, ?2, ?3)",
                params![case.case_id, unit, owner],
            )?;
        }
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
    /// question found in the final phase 2 coding of the units attributed to
    /// the case. A case without attributed units gets undocumented cells.
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
        let mut finals_by_interview: BTreeMap<String, Vec<FinalCoding>> = BTreeMap::new();
        let mut cells = Vec::new();
        for case in &cases {
            let documented = !case.units.is_empty();
            let unit_set: BTreeSet<&str> = case.units.iter().map(String::as_str).collect();
            let interviews: BTreeSet<&str> =
                case.units.iter().filter_map(|u| interview_of(u)).collect();
            let mut finals: Vec<FinalCoding> = Vec::new();
            for interview in interviews {
                if !finals_by_interview.contains_key(interview) {
                    let (list, _) = self.final_codings(interview)?;
                    finals_by_interview.insert(interview.to_string(), list);
                }
                finals.extend(
                    finals_by_interview[interview]
                        .iter()
                        .filter(|f| unit_set.contains(f.unit_id.as_str()))
                        .cloned(),
                );
            }
            finals.sort_by(|a, b| a.unit_id.cmp(&b.unit_id));
            for q in &questions {
                let mapped = by_rq.get(q.id.as_str());
                let mut codes: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for f in &finals {
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
                    documented,
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
