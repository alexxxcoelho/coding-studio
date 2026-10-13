//! V1 lot 5, API gaps 4 and 5: the "to review" mark on respondent units, and
//! the progress of the whole library in one call.

use anyhow::Result;
use cs_model::SpeakerRole;
use rusqlite::params;
use serde::Serialize;
use serde_json::json;

use crate::{append_event, now, validation, Store};

/// The only mark a unit can carry today.
pub const MARK_TO_REVIEW: &str = "to_review";

/// A unit marked "to review".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnitMark {
    pub interview_id: String,
    pub unit_id: String,
    pub mark: String,
    pub note: String,
    pub at: String,
    pub by: String,
}

/// Where the review of an interview stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryReview {
    /// `none` (no complete AI run), `in_progress` or `done`.
    pub state: String,
    pub disagreements: usize,
    /// Disagreements arbitrated.
    pub arbitrated: usize,
    pub sample: usize,
    pub sample_reviewed: usize,
}

/// One interview of the library with its progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryRow {
    pub interview_id: String,
    pub units: usize,
    pub respondent_units: usize,
    /// Respondent units in the scope of the analysis.
    pub relevant: usize,
    /// Respondent units outside it.
    pub not_relevant: usize,
    /// Relevant units the candidate has coded.
    pub coded: usize,
    /// Respondent units that carry a passage with a real code (what the Coder
    /// shows as coded); `coded` counts the phase 2 grid instead.
    pub units_coded: usize,
    /// Respondent units parked under "Plus tard" and carrying no real code.
    pub units_later: usize,
    /// Respondent units marked "to review".
    pub to_review: usize,
    pub memo_answered: usize,
    pub memo_total: usize,
    pub review: LibraryReview,
    /// Every disagreement has a decision (true also when there is none).
    pub arbitrated: bool,
    pub done: bool,
}

/// The library at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryProgress {
    pub interviews: Vec<LibraryRow>,
    pub totals: LibraryTotals,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LibraryTotals {
    pub interviews: usize,
    pub units: usize,
    pub respondent_units: usize,
    pub relevant: usize,
    pub not_relevant: usize,
    pub coded: usize,
    pub to_review: usize,
    pub memo_answered: usize,
    pub memo_total: usize,
    pub interviews_done: usize,
}

impl Store {
    fn respondent_unit_ids(&self, interview_id: &str) -> Result<Vec<String>> {
        Ok(self
            .segmentation(interview_id)?
            .map(|s| {
                s.units
                    .into_iter()
                    .filter(|u| !matches!(u.role, SpeakerRole::Interviewer))
                    .map(|u| u.unit_id)
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Marks a respondent unit "to review". Interviewer turns carry no mark.
    pub fn set_unit_mark(
        &self,
        interview_id: &str,
        unit_id: &str,
        note: &str,
        actor: &str,
    ) -> Result<UnitMark> {
        self.require_interview(interview_id)?;
        let respondents = self.respondent_unit_ids(interview_id)?;
        if !respondents.iter().any(|u| u == unit_id) {
            return Err(validation(format!(
                "{unit_id} is not a respondent unit of {interview_id}: only respondent units can be marked"
            )));
        }
        let note = note.trim().to_string();
        let old = self.unit_mark(interview_id, unit_id)?;
        if let Some(m) = old.as_ref().filter(|m| m.note == note) {
            return Ok(m.clone());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO unit_marks (interview_id, unit_id, mark, note, at, by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(interview_id, unit_id, mark) DO UPDATE SET note = excluded.note,
               at = excluded.at, by = excluded.by",
            params![interview_id, unit_id, MARK_TO_REVIEW, note, now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            "unit.mark",
            Some(interview_id),
            Some(unit_id),
            json!({"unit_id": unit_id, "mark": MARK_TO_REVIEW, "note": note,
                   "was_marked": old.is_some()}),
        )?;
        tx.commit()?;
        self.unit_mark(interview_id, unit_id)?
            .ok_or_else(|| validation("the mark was not saved"))
    }

    /// Clears the mark. Clearing a unit that carries none changes nothing.
    pub fn clear_unit_mark(&self, interview_id: &str, unit_id: &str, actor: &str) -> Result<bool> {
        self.require_interview(interview_id)?;
        if self.unit_mark(interview_id, unit_id)?.is_none() {
            return Ok(false);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM unit_marks WHERE interview_id = ?1 AND unit_id = ?2 AND mark = ?3",
            params![interview_id, unit_id, MARK_TO_REVIEW],
        )?;
        append_event(
            &tx,
            actor,
            "unit.unmark",
            Some(interview_id),
            Some(unit_id),
            json!({"unit_id": unit_id, "mark": MARK_TO_REVIEW}),
        )?;
        tx.commit()?;
        Ok(true)
    }

    fn unit_mark(&self, interview_id: &str, unit_id: &str) -> Result<Option<UnitMark>> {
        Ok(self
            .unit_marks(interview_id)?
            .into_iter()
            .find(|m| m.unit_id == unit_id))
    }

    /// Units of an interview marked "to review", in unit order. A mark on a
    /// unit that left the segmentation or turned interviewer is not listed.
    pub fn unit_marks(&self, interview_id: &str) -> Result<Vec<UnitMark>> {
        self.require_interview(interview_id)?;
        let respondents = self.respondent_unit_ids(interview_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT interview_id, unit_id, mark, note, at, by FROM unit_marks
             WHERE interview_id = ?1 AND mark = ?2",
        )?;
        let mut out: Vec<UnitMark> = stmt
            .query_map(params![interview_id, MARK_TO_REVIEW], |r| {
                Ok(UnitMark {
                    interview_id: r.get(0)?,
                    unit_id: r.get(1)?,
                    mark: r.get(2)?,
                    note: r.get(3)?,
                    at: r.get(4)?,
                    by: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        out.retain(|m| respondents.contains(&m.unit_id));
        out.sort_by_key(|m| {
            respondents
                .iter()
                .position(|u| *u == m.unit_id)
                .unwrap_or(usize::MAX)
        });
        Ok(out)
    }

    /// Progress of every interview in one call.
    pub fn library_progress(&self) -> Result<LibraryProgress> {
        let mut rows = Vec::new();
        let mut totals = LibraryTotals::default();
        for summary in self.summaries()? {
            let id = summary.interview_id.clone();
            let simple = self.simple_progress(&id)?;
            let memo = self.interview_memo(&id)?;
            let memo_total = memo.questions.len();
            let memo_answered = memo
                .questions
                .iter()
                .filter(|q| q.answer.as_ref().is_some_and(|a| !a.text.trim().is_empty()))
                .count();
            let to_review = self.unit_marks(&id)?.len();
            let respondents: std::collections::BTreeSet<String> =
                self.respondent_unit_ids(&id)?.into_iter().collect();
            let mut with_code = std::collections::BTreeSet::new();
            let mut with_later = std::collections::BTreeSet::new();
            for p in self.passages(&crate::PassageFilter {
                interview_id: Some(id.clone()),
                ..Default::default()
            })? {
                if p.stale || !respondents.contains(&p.unit_id) {
                    continue;
                }
                if p.codings.iter().any(|c| !c.system) {
                    with_code.insert(p.unit_id.clone());
                } else if p
                    .codings
                    .iter()
                    .any(|c| c.code_id == cs_model::passage::LATER_ID)
                {
                    with_later.insert(p.unit_id.clone());
                }
            }
            let units_later = with_later.difference(&with_code).count();
            let review_state = if simple.n_runs_complete == 0 {
                "none"
            } else if simple.n_decided == simple.n_disagreements
                && simple.n_sample_reviewed == simple.n_sample
            {
                "done"
            } else {
                "in_progress"
            };
            let row = LibraryRow {
                interview_id: id,
                units: summary.n_units,
                respondent_units: simple.n_respondent_units,
                relevant: simple.n_relevant,
                not_relevant: simple.n_respondent_units.saturating_sub(simple.n_relevant),
                coded: simple.n_coded,
                units_coded: with_code.len(),
                units_later,
                to_review,
                memo_answered,
                memo_total,
                review: LibraryReview {
                    state: review_state.into(),
                    disagreements: simple.n_disagreements,
                    arbitrated: simple.n_decided,
                    sample: simple.n_sample,
                    sample_reviewed: simple.n_sample_reviewed,
                },
                arbitrated: simple.n_decided == simple.n_disagreements,
                done: simple.done,
            };
            totals.interviews += 1;
            totals.units += row.units;
            totals.respondent_units += row.respondent_units;
            totals.relevant += row.relevant;
            totals.not_relevant += row.not_relevant;
            totals.coded += row.coded;
            totals.to_review += row.to_review;
            totals.memo_answered += row.memo_answered;
            totals.memo_total += row.memo_total;
            totals.interviews_done += usize::from(row.done);
            rows.push(row);
        }
        Ok(LibraryProgress {
            interviews: rows,
            totals,
        })
    }
}

/// Origin and role of a passage (guide: qualify a quotation). Omitted fields
/// are unchanged; an explicit null clears.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PassageQualify {
    #[serde(default, deserialize_with = "crate::codebook::present")]
    pub origin: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::codebook::present")]
    pub role: Option<Option<String>>,
}

impl Store {
    /// Sets the origin (`PASSAGE_ORIGINS`) and/or the role (`PASSAGE_ROLES`) of
    /// a passage. No event when nothing changes.
    pub fn qualify_passage(
        &self,
        passage_id: &str,
        patch: &PassageQualify,
        actor: &str,
    ) -> Result<cs_model::passage::Passage> {
        use cs_model::analysis::{PASSAGE_ORIGINS, PASSAGE_ROLES};
        let before = self.passage(passage_id)?;
        let pick = |what: &str,
                    wanted: &Option<Option<String>>,
                    list: &[&str],
                    current: &Option<String>|
         -> Result<Option<String>> {
            match wanted {
                None => Ok(current.clone()),
                Some(None) => Ok(None),
                Some(Some(v)) => {
                    let v = v.trim();
                    if v.is_empty() {
                        Ok(None)
                    } else if list.contains(&v) {
                        Ok(Some(v.to_string()))
                    } else {
                        Err(validation(format!(
                            "{what} must be one of {}, not {v}",
                            list.join(", ")
                        )))
                    }
                }
            }
        };
        let origin = pick("origin", &patch.origin, &PASSAGE_ORIGINS, &before.origin)?;
        let role = pick("role", &patch.role, &PASSAGE_ROLES, &before.role)?;
        if origin == before.origin && role == before.role {
            return Ok(before);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE passages SET origin = ?2, role = ?3 WHERE passage_id = ?1",
            params![passage_id, origin, role],
        )?;
        append_event(
            &tx,
            actor,
            "passage.qualify",
            Some(&before.interview_id),
            Some(passage_id),
            json!({
                "passage_id": passage_id,
                "before": {"origin": before.origin, "role": before.role},
                "after": {"origin": origin, "role": role},
            }),
        )?;
        tx.commit()?;
        self.passage(passage_id)
    }
}
