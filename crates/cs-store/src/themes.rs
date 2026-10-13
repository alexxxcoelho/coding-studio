//! V1 lot 5: themes. A theme is an analytical proposition built by the
//! researcher from codes, after coding. It has a sheet (proposition, cases,
//! conditions, alternative interpretation, limits, decision memo), a status
//! (candidate, revised, retained), research questions, and the checks of the
//! project list.
//!
//! - Nothing here is proposed by the tool: no theme from frequency, no theme
//!   from a model. The tool records, counts and compares.
//! - Moving a theme to "retained" needs every check of the project list done,
//!   or an explicit override with a reason. Both are kept in the history.
//! - Titles and statuses are append-only in `theme_history`; every write is one
//!   transaction with its audit event.
//! - A code may serve several themes. A code merged into another is followed
//!   at read time, so the theme never points at a dead code.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::analysis::{theme_status_label, THEME_STATUSES};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::analysis_index::Index;
use crate::{append_event, from_json, guard_err, not_found, now, to_json, validation, Store};

const MAX_TITLE: usize = 400;
const MAX_TEXT: usize = 20_000;

/// A theme to create: a title, and optionally the rest of the first draft.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewTheme {
    pub title: String,
    #[serde(default)]
    pub proposition: String,
    /// Codes (id, name, alias or token) that support the theme.
    #[serde(default)]
    pub codes: Vec<String>,
    #[serde(default)]
    pub research_questions: Vec<String>,
}

/// Partial edit of a theme sheet. Omitted fields are unchanged.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemePatch {
    #[serde(default)]
    pub title: Option<String>,
    /// Why the title changed (kept in the history).
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub proposition: Option<String>,
    #[serde(default)]
    pub conditions: Option<String>,
    #[serde(default)]
    pub alternative_interpretation: Option<String>,
    #[serde(default)]
    pub limits: Option<String>,
    #[serde(default)]
    pub decision_memo: Option<String>,
    /// Case (documented decision) ids that support the theme.
    #[serde(default)]
    pub supporting_cases: Option<Vec<String>>,
    /// Case ids that diverge from it.
    #[serde(default)]
    pub divergent_cases: Option<Vec<String>>,
    /// Passage ids the researcher put forward.
    #[serde(default)]
    pub main_passages: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeCodeView {
    pub code_id: String,
    pub name: String,
    pub active: bool,
    /// Passages carrying the code or a descendant (a passage counted once).
    pub passages: usize,
    /// Distinct interviews of those passages.
    pub interviews: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeCheckView {
    pub id: String,
    pub label: String,
    pub done: bool,
    pub note: String,
    pub at: Option<String>,
    pub by: Option<String>,
}

/// Counts of a theme, never mixing passages and interviews.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThemeCounts {
    pub passages: usize,
    pub interviews: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeView {
    pub theme_id: String,
    pub title: String,
    pub status: String,
    pub status_label: String,
    pub proposition: String,
    pub conditions: String,
    pub alternative_interpretation: String,
    pub limits: String,
    pub decision_memo: String,
    pub supporting_cases: Vec<String>,
    pub divergent_cases: Vec<String>,
    pub main_passages: Vec<String>,
    pub codes: Vec<ThemeCodeView>,
    pub research_questions: Vec<String>,
    pub checks: Vec<ThemeCheckView>,
    pub checks_done: usize,
    pub checks_total: usize,
    /// Every check of the project list is done: "retained" needs no override.
    pub checks_complete: bool,
    pub counts: ThemeCounts,
    /// Position in the document (the order of the chapter of results).
    pub order: i64,
    /// Things the guide warns about, as plain sentences: informative, never a lock.
    pub warnings: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One line of the history of a theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeHistoryEntry {
    pub hist_id: i64,
    pub theme_id: String,
    /// `created`, `title` or `status`.
    pub kind: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub reason: String,
    /// A move to "retained" with checks left undone.
    pub override_checks: bool,
    pub at: String,
    pub by: String,
}

struct Row {
    theme_id: String,
    title: String,
    status: String,
    proposition: String,
    conditions: String,
    alternative: String,
    limits: String,
    memo: String,
    supporting: String,
    divergent: String,
    main_passages: String,
    ord: i64,
    created_at: String,
    updated_at: String,
}

const ROW_COLUMNS: &str =
    "theme_id, title, status, proposition, conditions, alternative_interpretation, limits,
     decision_memo, supporting_cases, divergent_cases, main_passages, ord, created_at, updated_at";

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok(Row {
        theme_id: r.get(0)?,
        title: r.get(1)?,
        status: r.get(2)?,
        proposition: r.get(3)?,
        conditions: r.get(4)?,
        alternative: r.get(5)?,
        limits: r.get(6)?,
        memo: r.get(7)?,
        supporting: r.get(8)?,
        divergent: r.get(9)?,
        main_passages: r.get(10)?,
        ord: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
    })
}

fn check_title(title: &str) -> Result<String> {
    let t = title.trim();
    if t.is_empty() {
        return Err(validation(
            "a theme needs a title: write it as a proposition",
        ));
    }
    if t.chars().count() > MAX_TITLE {
        return Err(validation(format!(
            "a theme title is at most {MAX_TITLE} characters"
        )));
    }
    if t.chars().any(char::is_control) {
        return Err(validation("a theme title is one line"));
    }
    Ok(t.to_string())
}

fn check_text(what: &str, text: &str) -> Result<String> {
    if text.chars().count() > MAX_TEXT {
        return Err(validation(format!(
            "{what} is longer than {MAX_TEXT} characters"
        )));
    }
    Ok(text.trim().to_string())
}

fn dedup(list: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    list.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .collect()
}

impl Store {
    // ----------------------------------------------------------- reads

    fn theme_row(&self, theme_id: &str, with_deleted: bool) -> Result<Row> {
        let found = self
            .conn
            .query_row(
                &format!(
                    "SELECT {ROW_COLUMNS} FROM themes WHERE theme_id = ?1 AND (?2 OR deleted_at IS NULL)"
                ),
                params![theme_id.trim(), with_deleted],
                row_of,
            )
            .optional()?;
        found.ok_or_else(|| not_found(format!("theme {theme_id}")))
    }

    fn theme_rows(&self) -> Result<Vec<Row>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {ROW_COLUMNS} FROM themes WHERE deleted_at IS NULL ORDER BY ord, theme_id"
        ))?;
        let rows = stmt
            .query_map([], row_of)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The codes of a theme with merges followed: a stored code that was merged
    /// into another stands for the target, and a target listed twice counts once.
    pub(crate) fn theme_code_ids(&self, theme_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT code_id FROM theme_codes WHERE theme_id = ?1 ORDER BY added_at, rowid",
        )?;
        let stored: Vec<String> = stmt
            .query_map([theme_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out: Vec<String> = Vec::new();
        for id in stored {
            let now_id = self
                .resolve_code(&id)
                .map(|r| r.code_id)
                .unwrap_or(id.clone());
            if !out.contains(&now_id) {
                out.push(now_id);
            }
        }
        Ok(out)
    }

    fn theme_question_ids(&self, theme_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT rq_id FROM theme_questions WHERE theme_id = ?1 ORDER BY rq_id")?;
        let out = stmt
            .query_map([theme_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Passage positions (in `idx.passages`) of a theme: those carrying one of
    /// its codes or a descendant.
    pub(crate) fn theme_passage_positions(
        &self,
        idx: &Index,
        theme_id: &str,
    ) -> Result<BTreeSet<usize>> {
        let codes = idx.closure(self.theme_code_ids(theme_id)?);
        Ok(idx.with_codes(&codes))
    }

    fn view_of(
        &self,
        row: Row,
        idx: &Index,
        node_counts: &BTreeMap<String, (usize, usize, bool)>,
    ) -> Result<ThemeView> {
        let cfg = self.project_config()?;
        let answers: BTreeMap<String, (bool, String, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT check_id, done, note, at, by FROM theme_check_answers WHERE theme_id = ?1",
            )?;
            let rows = stmt.query_map([&row.theme_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ),
                ))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let checks: Vec<ThemeCheckView> = cfg
            .theme_checks
            .iter()
            .map(|c| {
                let a = answers.get(&c.id);
                ThemeCheckView {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    done: a.is_some_and(|a| a.0),
                    note: a.map(|a| a.1.clone()).unwrap_or_default(),
                    at: a.map(|a| a.2.clone()),
                    by: a.map(|a| a.3.clone()),
                }
            })
            .collect();
        let checks_done = checks.iter().filter(|c| c.done).count();
        let code_ids = self.theme_code_ids(&row.theme_id)?;
        let codes: Vec<ThemeCodeView> = code_ids
            .iter()
            .map(|id| {
                let (passages, interviews, active) =
                    node_counts.get(id).copied().unwrap_or((0, 0, false));
                ThemeCodeView {
                    code_id: id.clone(),
                    name: idx.name.get(id).cloned().unwrap_or_else(|| id.clone()),
                    active,
                    passages,
                    interviews,
                }
            })
            .collect();
        let positions = idx.with_codes(&idx.closure(code_ids.clone()));
        let interviews: BTreeSet<&str> = positions
            .iter()
            .map(|i| idx.passages[*i].interview_id.as_str())
            .collect();
        let supporting: Vec<String> = from_json(&row.supporting)?;
        let divergent: Vec<String> = from_json(&row.divergent)?;
        let mut warnings = Vec::new();
        if divergent.is_empty() && row.limits.trim().is_empty() {
            warnings.push(
                "Aucun cas divergent ni limite renseignés : un thème sans cas divergent examiné est suspect."
                    .to_string(),
            );
        }
        if code_ids.is_empty() {
            warnings.push("Aucun code ne soutient encore ce thème.".to_string());
        } else if interviews.len() == 1 {
            warnings.push(
                "Les passages viennent d'un seul entretien : justifiez le cas unique ou cherchez d'autres observations."
                    .to_string(),
            );
        }
        Ok(ThemeView {
            status_label: theme_status_label(&row.status).to_string(),
            theme_id: row.theme_id.clone(),
            title: row.title,
            status: row.status,
            proposition: row.proposition,
            conditions: row.conditions,
            alternative_interpretation: row.alternative,
            limits: row.limits,
            decision_memo: row.memo,
            supporting_cases: supporting,
            divergent_cases: divergent,
            main_passages: from_json(&row.main_passages)?,
            codes,
            research_questions: self.theme_question_ids(&row.theme_id)?,
            checks_total: checks.len(),
            checks_complete: checks_done == checks.len(),
            checks_done,
            checks,
            counts: ThemeCounts {
                passages: positions.len(),
                interviews: interviews.len(),
            },
            order: row.ord,
            warnings,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Passages and interviews per code (rolled up over descendants), and
    /// whether the code is active, for the theme views.
    fn node_counts(&self) -> Result<BTreeMap<String, (usize, usize, bool)>> {
        Ok(self
            .code_nodes()?
            .into_iter()
            .filter(|n| !n.system)
            .map(|n| {
                (
                    n.code_id,
                    (
                        n.counts.passages_rollup,
                        n.counts.interviews_rollup,
                        n.active && n.merged_into.is_none(),
                    ),
                )
            })
            .collect())
    }

    /// Every theme in document order, or those of one status.
    pub fn themes(&self, status: Option<&str>) -> Result<Vec<ThemeView>> {
        if let Some(s) = status {
            if !THEME_STATUSES.contains(&s) {
                return Err(validation(format!(
                    "unknown theme status {s}; use {}",
                    THEME_STATUSES.join(", ")
                )));
            }
        }
        let idx = Index::load(self)?;
        let counts = self.node_counts()?;
        let mut out = Vec::new();
        for row in self.theme_rows()? {
            if status.is_some_and(|s| s != row.status) {
                continue;
            }
            out.push(self.view_of(row, &idx, &counts)?);
        }
        Ok(out)
    }

    /// One theme.
    pub fn theme(&self, theme_id: &str) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let idx = Index::load(self)?;
        self.view_of(row, &idx, &self.node_counts()?)
    }

    /// The history of a theme (creation, titles, statuses), oldest first.
    pub fn theme_history(&self, theme_id: &str) -> Result<Vec<ThemeHistoryEntry>> {
        self.theme_row(theme_id, true)?;
        let mut stmt = self.conn.prepare(
            "SELECT hist_id, theme_id, kind, before, after, reason, override, at, by
             FROM theme_history WHERE theme_id = ?1 ORDER BY hist_id",
        )?;
        let out = stmt
            .query_map([theme_id.trim()], |r| {
                Ok(ThemeHistoryEntry {
                    hist_id: r.get(0)?,
                    theme_id: r.get(1)?,
                    kind: r.get(2)?,
                    before: r.get(3)?,
                    after: r.get(4)?,
                    reason: r.get(5)?,
                    override_checks: r.get::<_, i64>(6)? != 0,
                    at: r.get(7)?,
                    by: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    // ---------------------------------------------------------- writes

    fn next_theme_id(&self) -> Result<String> {
        let max: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(CAST(substr(theme_id, 3) AS INTEGER)), 0) FROM themes",
            [],
            |r| r.get(0),
        )?;
        Ok(format!("T-{:04}", max + 1))
    }

    /// Resolves codes given as id, name, alias or token to stable ids. A merged
    /// code gives its target; the reserved code and inactive codes are refused.
    fn theme_codes_resolved(&self, keys: &[String]) -> Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for key in keys.iter().map(|k| k.trim()).filter(|k| !k.is_empty()) {
            let r = self.resolve_code(key)?;
            if r.system {
                return Err(validation(
                    "the reserved code \"Plus tard\" cannot support a theme",
                ));
            }
            if !r.active {
                return Err(validation(format!(
                    "code {} is inactive and cannot support a theme",
                    r.name
                )));
            }
            if !out.contains(&r.code_id) {
                out.push(r.code_id);
            }
        }
        Ok(out)
    }

    fn check_case_ids(&self, ids: Vec<String>, what: &str) -> Result<Vec<String>> {
        let ids = dedup(ids);
        for id in &ids {
            self.case(id)
                .map_err(|_| validation(format!("{what}: unknown case {id}")))?;
        }
        Ok(ids)
    }

    fn check_passage_ids(&self, ids: Vec<String>) -> Result<Vec<String>> {
        let ids = dedup(ids);
        for id in &ids {
            self.passage(id)
                .map_err(|_| validation(format!("main passages: unknown passage {id}")))?;
        }
        Ok(ids)
    }

    /// Creates a theme (status candidate), last in the document order.
    pub fn create_theme(&self, req: &NewTheme, actor: &str) -> Result<ThemeView> {
        let title = check_title(&req.title)?;
        let proposition = check_text("the proposition", &req.proposition)?;
        let codes = self.theme_codes_resolved(&req.codes)?;
        let questions = self.check_questions(&dedup(req.research_questions.clone()))?;
        let id = self.next_theme_id()?;
        let ts = now();
        let ord: i64 =
            self.conn
                .query_row("SELECT COALESCE(MAX(ord), 0) + 1 FROM themes", [], |r| {
                    r.get(0)
                })?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO themes (theme_id, title, status, proposition, ord, created_at, created_by, updated_at)
             VALUES (?1, ?2, 'candidate', ?3, ?4, ?5, ?6, ?5)",
            params![id, title, proposition, ord, ts, actor],
        )?;
        for code in &codes {
            tx.execute(
                "INSERT INTO theme_codes (theme_id, code_id, added_at, added_by) VALUES (?1, ?2, ?3, ?4)",
                params![id, code, ts, actor],
            )?;
        }
        for rq in &questions {
            tx.execute(
                "INSERT INTO theme_questions (theme_id, rq_id) VALUES (?1, ?2)",
                params![id, rq],
            )?;
        }
        tx.execute(
            "INSERT INTO theme_history (theme_id, kind, before, after, reason, at, by)
             VALUES (?1, 'created', NULL, ?2, '', ?3, ?4)",
            params![id, title, ts, actor],
        )?;
        append_event(
            &tx,
            actor,
            "theme.create",
            None,
            Some(&id),
            json!({
                "theme_id": id, "title": title, "proposition": proposition,
                "codes": codes, "research_questions": questions, "order": ord,
            }),
        )?;
        tx.commit()?;
        self.theme(&id)
    }

    /// Edits the sheet. A new title is kept in the history with its reason.
    /// No event when nothing changed.
    pub fn update_theme(
        &self,
        theme_id: &str,
        patch: &ThemePatch,
        actor: &str,
    ) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let id = row.theme_id.clone();
        let mut before = serde_json::Map::new();
        let mut after = serde_json::Map::new();
        let mut set: Vec<(&str, String)> = Vec::new();
        let mut note = |name: &'static str, old: &str, new: String| {
            if old != new {
                before.insert(name.into(), json!(old));
                after.insert(name.into(), json!(new));
                set.push((name, new));
            }
        };
        let mut new_title: Option<String> = None;
        if let Some(t) = &patch.title {
            let t = check_title(t)?;
            if t != row.title {
                new_title = Some(t.clone());
            }
            note("title", &row.title, t);
        }
        if let Some(v) = &patch.proposition {
            note(
                "proposition",
                &row.proposition,
                check_text("the proposition", v)?,
            );
        }
        if let Some(v) = &patch.conditions {
            note(
                "conditions",
                &row.conditions,
                check_text("the conditions", v)?,
            );
        }
        if let Some(v) = &patch.alternative_interpretation {
            note(
                "alternative_interpretation",
                &row.alternative,
                check_text("the alternative interpretation", v)?,
            );
        }
        if let Some(v) = &patch.limits {
            note("limits", &row.limits, check_text("the limits", v)?);
        }
        if let Some(v) = &patch.decision_memo {
            note(
                "decision_memo",
                &row.memo,
                check_text("the decision memo", v)?,
            );
        }
        if let Some(v) = &patch.supporting_cases {
            let v = self.check_case_ids(v.clone(), "supporting cases")?;
            note("supporting_cases", &row.supporting, to_json(&v));
        }
        if let Some(v) = &patch.divergent_cases {
            let v = self.check_case_ids(v.clone(), "divergent cases")?;
            note("divergent_cases", &row.divergent, to_json(&v));
        }
        if let Some(v) = &patch.main_passages {
            let v = self.check_passage_ids(v.clone())?;
            note("main_passages", &row.main_passages, to_json(&v));
        }
        if set.is_empty() {
            return self.theme(&id);
        }
        let ts = now();
        let tx = self.conn.unchecked_transaction()?;
        for (column, value) in &set {
            let column = match *column {
                "title" => "title",
                "proposition" => "proposition",
                "conditions" => "conditions",
                "alternative_interpretation" => "alternative_interpretation",
                "limits" => "limits",
                "decision_memo" => "decision_memo",
                "supporting_cases" => "supporting_cases",
                "divergent_cases" => "divergent_cases",
                _ => "main_passages",
            };
            tx.execute(
                &format!("UPDATE themes SET {column} = ?2, updated_at = ?3 WHERE theme_id = ?1"),
                params![id, value, ts],
            )?;
        }
        if let Some(t) = &new_title {
            tx.execute(
                "INSERT INTO theme_history (theme_id, kind, before, after, reason, at, by)
                 VALUES (?1, 'title', ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    row.title,
                    t,
                    patch.reason.as_deref().unwrap_or("").trim(),
                    ts,
                    actor
                ],
            )?;
        }
        append_event(
            &tx,
            actor,
            "theme.update",
            None,
            Some(&id),
            json!({
                "theme_id": id,
                "before": before,
                "after": after,
                "reason": patch.reason.as_deref().unwrap_or("").trim(),
            }),
        )?;
        tx.commit()?;
        self.theme(&id)
    }

    /// Replaces the codes that support a theme.
    pub fn set_theme_codes(
        &self,
        theme_id: &str,
        codes: &[String],
        actor: &str,
    ) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let wanted = self.theme_codes_resolved(codes)?;
        let current = self.theme_code_ids(&row.theme_id)?;
        if wanted == current {
            return self.theme(&row.theme_id);
        }
        let ts = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM theme_codes WHERE theme_id = ?1",
            [&row.theme_id],
        )?;
        for code in &wanted {
            tx.execute(
                "INSERT INTO theme_codes (theme_id, code_id, added_at, added_by) VALUES (?1, ?2, ?3, ?4)",
                params![row.theme_id, code, ts, actor],
            )?;
        }
        tx.execute(
            "UPDATE themes SET updated_at = ?2 WHERE theme_id = ?1",
            params![row.theme_id, ts],
        )?;
        let added: Vec<&String> = wanted.iter().filter(|c| !current.contains(c)).collect();
        let removed: Vec<&String> = current.iter().filter(|c| !wanted.contains(c)).collect();
        append_event(
            &tx,
            actor,
            "theme.codes",
            None,
            Some(&row.theme_id),
            json!({
                "theme_id": row.theme_id, "before": current, "after": wanted,
                "added": added, "removed": removed,
            }),
        )?;
        tx.commit()?;
        self.theme(&row.theme_id)
    }

    /// Adds one code to a theme.
    pub fn add_theme_code(&self, theme_id: &str, code: &str, actor: &str) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let mut codes = self.theme_code_ids(&row.theme_id)?;
        codes.push(code.to_string());
        self.set_theme_codes(&row.theme_id, &codes, actor)
    }

    /// Removes one code from a theme.
    pub fn remove_theme_code(&self, theme_id: &str, code: &str, actor: &str) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let target = self.resolve_code(code)?.code_id;
        let codes: Vec<String> = self
            .theme_code_ids(&row.theme_id)?
            .into_iter()
            .filter(|c| *c != target)
            .collect();
        self.set_theme_codes(&row.theme_id, &codes, actor)
    }

    /// Replaces the research questions a theme answers.
    pub fn set_theme_questions(
        &self,
        theme_id: &str,
        questions: &[String],
        actor: &str,
    ) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let wanted = self.check_questions(&dedup(questions.to_vec()))?;
        let mut sorted = wanted.clone();
        sorted.sort();
        let current = self.theme_question_ids(&row.theme_id)?;
        if sorted == current {
            return self.theme(&row.theme_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM theme_questions WHERE theme_id = ?1",
            [&row.theme_id],
        )?;
        for rq in &sorted {
            tx.execute(
                "INSERT INTO theme_questions (theme_id, rq_id) VALUES (?1, ?2)",
                params![row.theme_id, rq],
            )?;
        }
        tx.execute(
            "UPDATE themes SET updated_at = ?2 WHERE theme_id = ?1",
            params![row.theme_id, now()],
        )?;
        append_event(
            &tx,
            actor,
            "theme.questions",
            None,
            Some(&row.theme_id),
            json!({"theme_id": row.theme_id, "before": current, "after": sorted}),
        )?;
        tx.commit()?;
        self.theme(&row.theme_id)
    }

    /// Answers one check of the project list for a theme: done or not, with a note.
    pub fn set_theme_check(
        &self,
        theme_id: &str,
        check_id: &str,
        done: bool,
        note: &str,
        actor: &str,
    ) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        let cfg = self.project_config()?;
        if !cfg.theme_checks.iter().any(|c| c.id == check_id) {
            return Err(validation(format!(
                "unknown theme check {check_id}; the project list has {}",
                cfg.theme_checks
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let note = check_text("the note of a check", note)?;
        let old: Option<(bool, String)> = self
            .conn
            .query_row(
                "SELECT done, note FROM theme_check_answers WHERE theme_id = ?1 AND check_id = ?2",
                params![row.theme_id, check_id],
                |r| Ok((r.get::<_, i64>(0)? != 0, r.get(1)?)),
            )
            .optional()?;
        if old.as_ref().is_some_and(|(d, n)| *d == done && *n == note)
            || (old.is_none() && !done && note.is_empty())
        {
            return self.theme(&row.theme_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO theme_check_answers (theme_id, check_id, done, note, at, by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(theme_id, check_id) DO UPDATE SET done = excluded.done,
               note = excluded.note, at = excluded.at, by = excluded.by",
            params![row.theme_id, check_id, done as i64, note, now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            "theme.check",
            None,
            Some(&row.theme_id),
            json!({
                "theme_id": row.theme_id, "check_id": check_id,
                "before": old.as_ref().map(|(d, n)| json!({"done": d, "note": n})),
                "after": {"done": done, "note": note},
            }),
        )?;
        tx.commit()?;
        self.theme(&row.theme_id)
    }

    /// Moves a theme to another status. Moving to "retained" needs every check
    /// of the project list done, or `override_reason` (an explicit decision of
    /// the researcher, kept in the history). The tool never retains a theme on
    /// its own.
    pub fn set_theme_status(
        &self,
        theme_id: &str,
        status: &str,
        reason: &str,
        override_reason: Option<&str>,
        actor: &str,
    ) -> Result<ThemeView> {
        let row = self.theme_row(theme_id, false)?;
        if !THEME_STATUSES.contains(&status) {
            return Err(validation(format!(
                "unknown theme status {status}; use {}",
                THEME_STATUSES.join(", ")
            )));
        }
        if row.status == status {
            return self.theme(&row.theme_id);
        }
        let view = self.theme(&row.theme_id)?;
        let missing: Vec<&ThemeCheckView> = view.checks.iter().filter(|c| !c.done).collect();
        let override_reason = override_reason.map(str::trim).filter(|r| !r.is_empty());
        let mut overridden = false;
        if status == "retained" && !missing.is_empty() {
            match override_reason {
                Some(_) => overridden = true,
                None => {
                    return Err(guard_err(format!(
                        "theme {} cannot be retained: {} check(s) not done ({}). \
                         Do them, or retain it anyway with override_reason",
                        row.theme_id,
                        missing.len(),
                        missing
                            .iter()
                            .map(|c| c.label.as_str())
                            .collect::<Vec<_>>()
                            .join("; ")
                    )))
                }
            }
        }
        let recorded = if overridden {
            override_reason.unwrap_or_default().to_string()
        } else {
            reason.trim().to_string()
        };
        let ts = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE themes SET status = ?2, updated_at = ?3 WHERE theme_id = ?1",
            params![row.theme_id, status, ts],
        )?;
        tx.execute(
            "INSERT INTO theme_history (theme_id, kind, before, after, reason, override, at, by)
             VALUES (?1, 'status', ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.theme_id,
                row.status,
                status,
                recorded,
                overridden as i64,
                ts,
                actor
            ],
        )?;
        append_event(
            &tx,
            actor,
            "theme.status",
            None,
            Some(&row.theme_id),
            json!({
                "theme_id": row.theme_id, "from": row.status, "to": status,
                "reason": reason.trim(), "override": overridden,
                "override_reason": override_reason,
                "checks_not_done": if overridden {
                    missing.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
                } else { Vec::new() },
            }),
        )?;
        tx.commit()?;
        self.theme(&row.theme_id)
    }

    /// Sets the document order: the listed themes first, in that order, the
    /// others after them in their current order.
    pub fn reorder_themes(&self, order: &[String], actor: &str) -> Result<Vec<ThemeView>> {
        let rows = self.theme_rows()?;
        let known: BTreeSet<&str> = rows.iter().map(|r| r.theme_id.as_str()).collect();
        let listed = dedup(order.to_vec());
        for id in &listed {
            if !known.contains(id.as_str()) {
                return Err(validation(format!("unknown theme {id}")));
            }
        }
        let mut full: Vec<String> = listed.clone();
        full.extend(
            rows.iter()
                .map(|r| r.theme_id.clone())
                .filter(|id| !listed.contains(id)),
        );
        let before: Vec<String> = rows.iter().map(|r| r.theme_id.clone()).collect();
        if full == before {
            return self.themes(None);
        }
        let tx = self.conn.unchecked_transaction()?;
        for (i, id) in full.iter().enumerate() {
            tx.execute(
                "UPDATE themes SET ord = ?2 WHERE theme_id = ?1",
                params![id, i as i64 + 1],
            )?;
        }
        append_event(
            &tx,
            actor,
            "theme.order",
            None,
            None,
            json!({"before": before, "after": full}),
        )?;
        tx.commit()?;
        self.themes(None)
    }

    /// Deletes a theme logically. Refused while links point at its parts.
    pub fn delete_theme(&self, theme_id: &str, reason: &str, actor: &str) -> Result<()> {
        let row = self.theme_row(theme_id, false)?;
        let links: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM passage_links
             WHERE target_kind = 'theme_part' AND target_id = ?1 AND removed_at IS NULL",
            [&row.theme_id],
            |r| r.get(0),
        )?;
        if links > 0 {
            return Err(guard_err(format!(
                "theme {} has {links} link(s) to passages; remove them first",
                row.theme_id
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE themes SET deleted_at = ?2, deleted_by = ?3, delete_reason = ?4 WHERE theme_id = ?1",
            params![row.theme_id, now(), actor, reason.trim()],
        )?;
        append_event(
            &tx,
            actor,
            "theme.delete",
            None,
            Some(&row.theme_id),
            json!({"theme_id": row.theme_id, "title": row.title, "status": row.status, "reason": reason.trim()}),
        )?;
        tx.commit()?;
        Ok(())
    }
}
