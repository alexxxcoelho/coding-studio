//! V1 lot 2: the project configuration (attributes, memo templates, editable
//! lists, framework), the data it governs per interview (attribute values,
//! familiarisation memo, end-of-interview checklist) and the project template
//! import and export.
//!
//! - The configuration is JSON in `settings` under `PROJECT_CONFIG_KEY`, next
//!   to `study_config`. Each change is validated, versioned (memo templates)
//!   and audited as `project_config.update` with `{before, after}`.
//! - Data never silently disappears: an attribute that has values cannot be
//!   removed, change type or lose a value in use; a memo template used by an
//!   interview cannot be removed.
//! - Per-interview data is one JSON document per kind in `interview_data`.
//!   Every write is one transaction with its audit event.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::project::{
    AttributeDef, MemoQuestion, ProjectConfig, ProjectTemplate, TemplateQuestion,
    PROJECT_CONFIG_KEY, TEMPLATE_FORMAT, TEMPLATE_FORMAT_VERSION,
};
use cs_model::StudyConfig;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{append_event, from_json, guard_err, not_found, now, to_json, validation, Store};

const ATTRIBUTES: &str = "attributes";
const MEMO: &str = "memo";
const CHECKLIST: &str = "checklist";

// ---------------------------------------------------------------- views

/// Attribute definitions and the values of one interview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttributeView {
    pub interview_id: String,
    pub definitions: Vec<AttributeDef>,
    /// Attribute id to value (only the attributes that have one).
    pub values: BTreeMap<String, String>,
}

/// An answer with the time it was last written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoAnswer {
    pub text: String,
    pub at: String,
}

/// What an interview changes of its memo template (for this interview only).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoOverrides {
    /// Questions added for this interview. An empty id gets `x1`, `x2`...
    #[serde(default)]
    pub added: Vec<MemoQuestion>,
    /// Ids of template questions removed for this interview.
    #[serde(default)]
    pub removed: Vec<String>,
    /// Template question id to the reworded text.
    #[serde(default)]
    pub reworded: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredMemo {
    template_id: String,
    template_version: u32,
    #[serde(default)]
    overrides: MemoOverrides,
    #[serde(default)]
    answers: BTreeMap<String, MemoAnswer>,
}

/// A question as the interview sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoQuestionView {
    pub id: String,
    pub text: String,
    /// `template` or `added`.
    pub source: String,
    pub reworded: bool,
    pub answer: Option<MemoAnswer>,
}

/// The familiarisation memo of an interview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoView {
    pub interview_id: String,
    pub template_id: String,
    pub template_name: String,
    /// Version the memo is pinned to.
    pub template_version: u32,
    pub current_version: u32,
    /// True when the template changed since the memo was pinned. Writing the
    /// same template again (`PUT .../memo/template`) moves to the new version.
    pub outdated: bool,
    pub questions: Vec<MemoQuestionView>,
    /// Template questions removed for this interview, with their answers kept.
    pub removed: Vec<MemoQuestionView>,
    /// Answers to questions that are no longer shown, kept.
    pub orphan_answers: BTreeMap<String, MemoAnswer>,
    pub overrides: MemoOverrides,
    pub saved: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StoredChecklist {
    #[serde(default)]
    checked: BTreeMap<String, String>,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChecklistItemView {
    pub id: String,
    pub label: String,
    pub checked: bool,
    pub checked_at: Option<String>,
}

/// End-of-interview checklist state. Never a lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChecklistView {
    pub interview_id: String,
    pub items: Vec<ChecklistItemView>,
    pub note: String,
    pub complete: bool,
}

/// One line of the report of a template import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateChange {
    pub part: String,
    /// `added`, `updated`, `removed`, `kept` or `unchanged`.
    pub action: String,
    pub detail: String,
}

/// Result of [`Store::import_template`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateReport {
    pub dry_run: bool,
    /// True when the template was written.
    pub applied: bool,
    pub changes: Vec<TemplateChange>,
    /// Every reason the template cannot be imported; empty when it can.
    pub problems: Vec<String>,
}

fn change(part: &str, action: &str, detail: impl Into<String>) -> TemplateChange {
    TemplateChange {
        part: part.into(),
        action: action.into(),
        detail: detail.into(),
    }
}

impl Store {
    // ------------------------------------------------- configuration

    /// The stored project configuration, or the defaults when unset.
    pub fn project_config(&self) -> Result<ProjectConfig> {
        match self.setting(PROJECT_CONFIG_KEY)? {
            Some(raw) => from_json::<ProjectConfig>(&raw),
            None => Ok(ProjectConfig::default()),
        }
    }

    /// Validates, versions, stores and audits the configuration; returns what
    /// was stored. Refused (Guard) when it would orphan data: removing or
    /// retyping an attribute that has values, dropping a choice in use,
    /// removing a memo template an interview uses.
    pub fn set_project_config(&self, cfg: &ProjectConfig, actor: &str) -> Result<ProjectConfig> {
        let (stored, before) = self.prepare_project_config(cfg)?;
        if stored == before {
            return Ok(stored);
        }
        self.write_project_config(&before, &stored, actor, None)?;
        Ok(stored)
    }

    /// Checks a candidate configuration against the data and returns the
    /// version-stamped result with the current one.
    fn prepare_project_config(
        &self,
        cfg: &ProjectConfig,
    ) -> Result<(ProjectConfig, ProjectConfig)> {
        cfg.validate()?;
        let before = self.project_config()?;
        let stored = cfg.versioned_after(&before);
        let problems = self.data_conflicts(&before, &stored)?;
        if let Some(first) = problems.first() {
            return Err(guard_err(first.clone()));
        }
        Ok((stored, before))
    }

    fn write_project_config(
        &self,
        before: &ProjectConfig,
        after: &ProjectConfig,
        actor: &str,
        extra: Option<Value>,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![PROJECT_CONFIG_KEY, to_json(after), now()],
        )?;
        let mut detail = json!({
            "before": serde_json::to_value(before)?,
            "after": serde_json::to_value(after)?,
        });
        if let (Some(extra), Some(map)) = (extra, detail.as_object_mut()) {
            map.insert("via".into(), extra);
        }
        append_event(
            &tx,
            actor,
            "project_config.update",
            None,
            Some(PROJECT_CONFIG_KEY),
            detail,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Reasons `after` would orphan data that exists under `before`.
    fn data_conflicts(&self, before: &ProjectConfig, after: &ProjectConfig) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let values = self.all_attribute_values()?;
        for def in &before.attributes {
            let used: Vec<(&String, &String)> = values
                .iter()
                .filter_map(|(iv, m)| m.get(&def.id).map(|v| (iv, v)))
                .collect();
            if used.is_empty() {
                continue;
            }
            match after.attribute(&def.id) {
                None => out.push(format!(
                    "attribute {} has values on {} interview(s) and cannot be removed",
                    def.id,
                    used.len()
                )),
                Some(new) if new.kind != def.kind => out.push(format!(
                    "attribute {} has values and its type cannot change",
                    def.id
                )),
                Some(new) => {
                    for (iv, v) in &used {
                        if new.check_value(v).is_err() {
                            out.push(format!(
                                "attribute {}: the value {v:?} of {iv} would no longer be allowed",
                                def.id
                            ));
                        }
                    }
                }
            }
        }
        if !after.case_fields.is_empty() {
            for case in self.cases(None)? {
                for (k, v) in &case.characteristics {
                    match after.case_field(k) {
                        None => out.push(format!(
                            "case field {k} is used by {} and cannot be left out of the list",
                            case.case_id
                        )),
                        Some(def) => {
                            if let Err(e) = def.check_value(v) {
                                out.push(format!(
                                    "case field {k}: the value {v:?} of {} would no longer be allowed ({e})",
                                    case.case_id
                                ));
                            }
                        }
                    }
                }
            }
        }
        for (iv, memo) in self.all_memos()? {
            if after.template(&memo.template_id).is_none() {
                out.push(format!(
                    "memo template {} is used by {iv} and cannot be removed",
                    memo.template_id
                ));
            }
        }
        Ok(out)
    }

    // ------------------------------------------------- interview data

    fn data(&self, interview_id: &str, kind: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT body FROM interview_data WHERE interview_id = ?1 AND kind = ?2",
                params![interview_id, kind],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn all_data(&self, kind: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT interview_id, body FROM interview_data WHERE kind = ?1 ORDER BY interview_id",
        )?;
        let rows = stmt.query_map([kind], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn put_data(
        &self,
        tx: &rusqlite::Connection,
        interview_id: &str,
        kind: &str,
        body: &str,
        actor: &str,
    ) -> Result<()> {
        tx.execute(
            "INSERT INTO interview_data (interview_id, kind, body, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(interview_id, kind) DO UPDATE SET body = excluded.body,
               updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            params![interview_id, kind, body, now(), actor],
        )?;
        Ok(())
    }

    // ------------------------------------------------- attributes

    pub(crate) fn all_attribute_values(
        &self,
    ) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
        let mut out = BTreeMap::new();
        for (iv, body) in self.all_data(ATTRIBUTES)? {
            out.insert(iv, from_json::<BTreeMap<String, String>>(&body)?);
        }
        Ok(out)
    }

    /// Definitions and values of one interview.
    pub fn interview_attributes(&self, interview_id: &str) -> Result<AttributeView> {
        self.require_interview(interview_id)?;
        let values = match self.data(interview_id, ATTRIBUTES)? {
            Some(b) => from_json(&b)?,
            None => BTreeMap::new(),
        };
        Ok(AttributeView {
            interview_id: interview_id.into(),
            definitions: self.project_config()?.attributes,
            values,
        })
    }

    /// Sets and clears attribute values. `None` or an empty text clears.
    /// All values are validated before any is written. Audited as
    /// `interview.attributes`.
    pub fn set_interview_attributes(
        &self,
        interview_id: &str,
        changes: &BTreeMap<String, Option<String>>,
        actor: &str,
    ) -> Result<AttributeView> {
        let current = self.interview_attributes(interview_id)?;
        let cfg = self.project_config()?;
        let mut values = current.values.clone();
        for (id, raw) in changes {
            let def = cfg
                .attribute(id)
                .ok_or_else(|| validation(format!("unknown attribute {id}")))?;
            match raw
                .as_deref()
                .map(|r| def.check_value(r))
                .transpose()?
                .flatten()
            {
                Some(v) => {
                    values.insert(id.clone(), v);
                }
                None => {
                    values.remove(id);
                }
            }
        }
        if values == current.values {
            return Ok(current);
        }
        let tx = self.conn.unchecked_transaction()?;
        self.put_data(&tx, interview_id, ATTRIBUTES, &to_json(&values), actor)?;
        append_event(
            &tx,
            actor,
            "interview.attributes",
            Some(interview_id),
            None,
            json!({"before": current.values, "after": values}),
        )?;
        tx.commit()?;
        self.interview_attributes(interview_id)
    }

    /// Interviews whose attribute has the given value.
    pub fn interviews_with_attribute(
        &self,
        attribute: &str,
        value: &str,
    ) -> Result<BTreeSet<String>> {
        if self.project_config()?.attribute(attribute).is_none() {
            return Err(validation(format!("unknown attribute {attribute}")));
        }
        Ok(self
            .all_attribute_values()?
            .into_iter()
            .filter(|(_, m)| m.get(attribute).is_some_and(|v| v == value))
            .map(|(iv, _)| iv)
            .collect())
    }

    // ------------------------------------------------- memo

    fn all_memos(&self) -> Result<Vec<(String, StoredMemo)>> {
        let mut out = Vec::new();
        for (iv, body) in self.all_data(MEMO)? {
            out.push((iv, from_json::<StoredMemo>(&body)?));
        }
        Ok(out)
    }

    fn stored_memo(&self, interview_id: &str, cfg: &ProjectConfig) -> Result<(StoredMemo, bool)> {
        if let Some(body) = self.data(interview_id, MEMO)? {
            return Ok((from_json(&body)?, true));
        }
        let t = cfg
            .default_template()
            .ok_or_else(|| validation("no default memo template"))?;
        Ok((
            StoredMemo {
                template_id: t.id.clone(),
                template_version: t.version,
                overrides: MemoOverrides::default(),
                answers: BTreeMap::new(),
            },
            false,
        ))
    }

    fn memo_view(&self, interview_id: &str) -> Result<MemoView> {
        self.require_interview(interview_id)?;
        let cfg = self.project_config()?;
        let (memo, saved) = self.stored_memo(interview_id, &cfg)?;
        let snap = cfg
            .template_at(&memo.template_id, memo.template_version)
            .ok_or_else(|| {
                not_found(format!(
                    "memo template {} version {}",
                    memo.template_id, memo.template_version
                ))
            })?;
        let current = cfg
            .template(&memo.template_id)
            .map(|t| t.version)
            .unwrap_or(snap.version);
        let ov = &memo.overrides;
        let view =
            |q: &MemoQuestion, source: &str, text: String, reworded: bool| MemoQuestionView {
                id: q.id.clone(),
                text,
                source: source.into(),
                reworded,
                answer: memo.answers.get(&q.id).cloned(),
            };
        let mut questions = Vec::new();
        let mut removed = Vec::new();
        for q in &snap.questions {
            if ov.removed.contains(&q.id) {
                removed.push(view(q, "template", q.text.clone(), false));
            } else if let Some(t) = ov.reworded.get(&q.id) {
                questions.push(view(q, "template", t.clone(), true));
            } else {
                questions.push(view(q, "template", q.text.clone(), false));
            }
        }
        for q in &ov.added {
            questions.push(view(q, "added", q.text.clone(), false));
        }
        let shown: BTreeSet<&str> = questions
            .iter()
            .chain(removed.iter())
            .map(|q| q.id.as_str())
            .collect();
        let orphan_answers = memo
            .answers
            .iter()
            .filter(|(id, _)| !shown.contains(id.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(MemoView {
            interview_id: interview_id.into(),
            template_id: memo.template_id.clone(),
            template_name: snap.name,
            template_version: memo.template_version,
            current_version: current,
            outdated: current != memo.template_version,
            questions,
            removed,
            orphan_answers,
            overrides: memo.overrides.clone(),
            saved,
        })
    }

    /// The memo of an interview: default template until another is chosen.
    pub fn interview_memo(&self, interview_id: &str) -> Result<MemoView> {
        self.memo_view(interview_id)
    }

    fn save_memo(
        &self,
        interview_id: &str,
        memo: &StoredMemo,
        action: &str,
        detail: Value,
        actor: &str,
    ) -> Result<MemoView> {
        let tx = self.conn.unchecked_transaction()?;
        self.put_data(&tx, interview_id, MEMO, &to_json(memo), actor)?;
        append_event(&tx, actor, action, Some(interview_id), None, detail)?;
        tx.commit()?;
        self.memo_view(interview_id)
    }

    /// Chooses the memo template (and moves to its current version). The
    /// interview's own edits are reset, the answers are kept by question id.
    pub fn set_memo_template(
        &self,
        interview_id: &str,
        template_id: &str,
        actor: &str,
    ) -> Result<MemoView> {
        self.require_interview(interview_id)?;
        let cfg = self.project_config()?;
        let t = cfg
            .template(template_id)
            .ok_or_else(|| validation(format!("unknown memo template {template_id}")))?;
        let (mut memo, _) = self.stored_memo(interview_id, &cfg)?;
        let same = memo.template_id == t.id;
        let before = json!({"template_id": memo.template_id, "version": memo.template_version});
        memo.template_id = t.id.clone();
        memo.template_version = t.version;
        if !same {
            memo.overrides = MemoOverrides::default();
        } else {
            // Keep the edits that still apply to the new version.
            let ids: BTreeSet<&str> = t.questions.iter().map(|q| q.id.as_str()).collect();
            memo.overrides.removed.retain(|i| ids.contains(i.as_str()));
            memo.overrides
                .reworded
                .retain(|i, _| ids.contains(i.as_str()));
        }
        self.save_memo(
            interview_id,
            &memo,
            "memo.template",
            json!({"before": before, "after": {"template_id": t.id, "version": t.version}}),
            actor,
        )
    }

    /// Replaces the interview's own edits of its template.
    pub fn set_memo_overrides(
        &self,
        interview_id: &str,
        overrides: &MemoOverrides,
        actor: &str,
    ) -> Result<MemoView> {
        self.require_interview(interview_id)?;
        let cfg = self.project_config()?;
        let (mut memo, _) = self.stored_memo(interview_id, &cfg)?;
        let snap = cfg
            .template_at(&memo.template_id, memo.template_version)
            .ok_or_else(|| not_found(format!("memo template {}", memo.template_id)))?;
        let tids: BTreeSet<&str> = snap.questions.iter().map(|q| q.id.as_str()).collect();
        let mut ov = MemoOverrides::default();
        for id in &overrides.removed {
            if !tids.contains(id.as_str()) {
                return Err(validation(format!("cannot remove unknown question {id}")));
            }
            if !ov.removed.contains(id) {
                ov.removed.push(id.clone());
            }
        }
        for (id, text) in &overrides.reworded {
            if !tids.contains(id.as_str()) {
                return Err(validation(format!("cannot reword unknown question {id}")));
            }
            let t = text.trim();
            if t.is_empty() || t.contains('\n') || t.chars().count() > 200 {
                return Err(validation(format!(
                    "question {id}: the text is empty, on several lines or too long"
                )));
            }
            ov.reworded.insert(id.clone(), t.to_string());
        }
        let mut used: BTreeSet<String> = tids.iter().map(|s| s.to_string()).collect();
        let mut n = 0;
        let mut pending = Vec::new();
        for q in &overrides.added {
            let t = q.text.trim();
            if t.is_empty() || t.contains('\n') || t.chars().count() > 200 {
                return Err(validation(
                    "an added question is empty, on several lines or too long",
                ));
            }
            if !q.id.is_empty() {
                if !cs_model::project::valid_id(&q.id) {
                    return Err(validation(format!("question id {:?} is not valid", q.id)));
                }
                if !used.insert(q.id.clone()) {
                    return Err(validation(format!("duplicate question id {}", q.id)));
                }
            }
            pending.push((q.id.clone(), t.to_string()));
        }
        for (id, text) in pending {
            let id = if id.is_empty() {
                loop {
                    n += 1;
                    let cand = format!("x{n}");
                    if used.insert(cand.clone()) {
                        break cand;
                    }
                }
            } else {
                id
            };
            ov.added.push(MemoQuestion { id, text });
        }
        if ov == memo.overrides {
            return self.memo_view(interview_id);
        }
        let before = serde_json::to_value(&memo.overrides)?;
        memo.overrides = ov;
        let after = serde_json::to_value(&memo.overrides)?;
        self.save_memo(
            interview_id,
            &memo,
            "memo.overrides",
            json!({"before": before, "after": after}),
            actor,
        )
    }

    /// Writes answers (question id to text). An empty text clears the answer.
    /// Each answer carries the time it last changed. Only questions shown for
    /// the interview can be answered.
    pub fn answer_memo(
        &self,
        interview_id: &str,
        answers: &BTreeMap<String, String>,
        actor: &str,
    ) -> Result<MemoView> {
        let view = self.memo_view(interview_id)?;
        let shown: BTreeSet<&str> = view.questions.iter().map(|q| q.id.as_str()).collect();
        for id in answers.keys() {
            if !shown.contains(id.as_str()) {
                return Err(validation(format!(
                    "question {id} is not part of this memo"
                )));
            }
        }
        let cfg = self.project_config()?;
        let (mut memo, _) = self.stored_memo(interview_id, &cfg)?;
        let stamp = now();
        let mut changed = Vec::new();
        for (id, text) in answers {
            let text = text.trim();
            let had = memo.answers.get(id).map(|a| a.text.as_str());
            if text.is_empty() {
                if memo.answers.remove(id).is_some() {
                    changed.push(id.clone());
                }
            } else if had != Some(text) {
                memo.answers.insert(
                    id.clone(),
                    MemoAnswer {
                        text: text.into(),
                        at: stamp.clone(),
                    },
                );
                changed.push(id.clone());
            }
        }
        if changed.is_empty() {
            return Ok(view);
        }
        self.save_memo(
            interview_id,
            &memo,
            "memo.answer",
            json!({
                "template_id": memo.template_id,
                "template_version": memo.template_version,
                "questions": changed,
            }),
            actor,
        )
    }

    // ------------------------------------------------- checklist

    fn checklist_view(&self, interview_id: &str) -> Result<ChecklistView> {
        self.require_interview(interview_id)?;
        let stored: StoredChecklist = match self.data(interview_id, CHECKLIST)? {
            Some(b) => from_json(&b)?,
            None => StoredChecklist::default(),
        };
        let items: Vec<ChecklistItemView> = self
            .project_config()?
            .checklist
            .into_iter()
            .map(|i| ChecklistItemView {
                checked: stored.checked.contains_key(&i.id),
                checked_at: stored.checked.get(&i.id).cloned(),
                id: i.id,
                label: i.label,
            })
            .collect();
        Ok(ChecklistView {
            interview_id: interview_id.into(),
            complete: !items.is_empty() && items.iter().all(|i| i.checked),
            items,
            note: stored.note,
        })
    }

    /// The end-of-interview checklist: the items of the configuration, which
    /// are checked, and the note. `complete` informs, it never locks.
    pub fn interview_checklist(&self, interview_id: &str) -> Result<ChecklistView> {
        self.checklist_view(interview_id)
    }

    /// Checks or unchecks items (item id to flag) and sets the note.
    pub fn set_interview_checklist(
        &self,
        interview_id: &str,
        checked: &BTreeMap<String, bool>,
        note: Option<&str>,
        actor: &str,
    ) -> Result<ChecklistView> {
        let view = self.checklist_view(interview_id)?;
        let known: BTreeSet<&str> = view.items.iter().map(|i| i.id.as_str()).collect();
        for id in checked.keys() {
            if !known.contains(id.as_str()) {
                return Err(validation(format!("unknown checklist item {id}")));
            }
        }
        if note.is_some_and(|n| n.chars().count() > 5000) {
            return Err(validation("the note is longer than 5000 characters"));
        }
        let mut stored: StoredChecklist = match self.data(interview_id, CHECKLIST)? {
            Some(b) => from_json(&b)?,
            None => StoredChecklist::default(),
        };
        let before = stored.clone();
        for (id, flag) in checked {
            if *flag {
                stored.checked.entry(id.clone()).or_insert_with(now);
            } else {
                stored.checked.remove(id);
            }
        }
        if let Some(n) = note {
            stored.note = n.trim().to_string();
        }
        if stored == before {
            return Ok(view);
        }
        let tx = self.conn.unchecked_transaction()?;
        self.put_data(&tx, interview_id, CHECKLIST, &to_json(&stored), actor)?;
        append_event(
            &tx,
            actor,
            "interview.checklist",
            Some(interview_id),
            None,
            json!({
                "checked": checked,
                "items_checked": stored.checked.keys().collect::<Vec<_>>(),
                "note_changed": stored.note != before.note,
            }),
        )?;
        tx.commit()?;
        self.checklist_view(interview_id)
    }

    // ------------------------------------------------- template

    /// The project template: the whole study configuration, no interview
    /// data. Deterministic: the same configuration gives the same bytes.
    pub fn export_template(&self) -> Result<ProjectTemplate> {
        let mut project_config = self.project_config()?;
        project_config.memo_history.clear();
        Ok(ProjectTemplate {
            format: TEMPLATE_FORMAT.into(),
            version: TEMPLATE_FORMAT_VERSION,
            study_config: self.study_config()?,
            research_questions: self
                .research_questions()?
                .into_iter()
                .map(|q| TemplateQuestion {
                    id: q.id,
                    text: q.text,
                })
                .collect(),
            project_config,
        })
    }

    /// Imports a template into this project (new or with data). All or
    /// nothing: every problem is reported at once and nothing is written when
    /// there is one, or on `dry_run`. Parts that carry data are kept: an
    /// attribute with values, or a memo template an interview uses, that the
    /// template does not list stays (reported as `kept`).
    pub fn import_template(
        &self,
        template: &Value,
        dry_run: bool,
        actor: &str,
    ) -> Result<TemplateReport> {
        let mut problems = Vec::new();
        let t: ProjectTemplate = match serde_json::from_value(template.clone()) {
            Ok(t) => t,
            Err(e) => return Err(validation(format!("invalid project template: {e}"))),
        };
        if t.format != TEMPLATE_FORMAT {
            problems.push(format!(
                "format must be {TEMPLATE_FORMAT}, not {:?}",
                t.format
            ));
        }
        if t.version == 0 || t.version > TEMPLATE_FORMAT_VERSION {
            problems.push(format!(
                "template version {} is not supported (this tool reads up to {TEMPLATE_FORMAT_VERSION})",
                t.version
            ));
        }
        if let Err(e) = t.study_config.validate() {
            problems.push(format!("study configuration: {e}"));
        }
        let mut seen = BTreeSet::new();
        for q in &t.research_questions {
            let id = q.id.trim();
            if id.is_empty()
                || id
                    .chars()
                    .any(|c| c.is_whitespace() || c == ',' || c == ';')
            {
                problems.push(format!("research question id {:?} is not valid", q.id));
            } else if !seen.insert(id.to_string()) {
                problems.push(format!("duplicate research question {id}"));
            }
        }

        let before = self.project_config()?;
        let mut incoming = t.project_config.clone();
        incoming.memo_history.clear();
        problems.extend(incoming.problems());
        let mut changes = Vec::new();

        // Keep what carries data.
        let values = self.all_attribute_values()?;
        for def in &before.attributes {
            let has_values = values.values().any(|m| m.contains_key(&def.id));
            if has_values && incoming.attribute(&def.id).is_none() {
                incoming.attributes.push(def.clone());
                changes.push(change(
                    "attributes",
                    "kept",
                    format!("{} (it has values, the template does not list it)", def.id),
                ));
            }
        }
        for (_, memo) in self.all_memos()? {
            if incoming.template(&memo.template_id).is_none() {
                if let Some(old) = before.template(&memo.template_id) {
                    let mut keep = old.clone();
                    keep.default = false;
                    incoming.memo_templates.push(keep);
                    changes.push(change(
                        "memo_templates",
                        "kept",
                        format!("{} (an interview uses it)", memo.template_id),
                    ));
                }
            }
        }
        if problems.is_empty() {
            problems.extend(incoming.problems());
        }
        let next = incoming.versioned_after(&before);
        if problems.is_empty() {
            problems.extend(self.data_conflicts(&before, &next)?);
        }

        // Study configuration (a phase 1 category change is guarded).
        let study_before = self.study_config()?;
        let study_after = t.study_config.normalised();
        if study_after != study_before && problems.is_empty() {
            let names = |c: &StudyConfig| -> BTreeSet<String> {
                c.effective_categories()
                    .into_iter()
                    .map(|c| c.name)
                    .collect()
            };
            if names(&study_after) != names(&study_before) && self.phase1_coding_exists()? {
                problems.push(
                    "the phase 1 category names cannot change once phase 1 coding exists".into(),
                );
            }
        }

        // Report.
        report_study(&study_before, &study_after, &mut changes);
        let rq_before = self.research_questions()?;
        for q in &t.research_questions {
            match rq_before.iter().find(|r| r.id == q.id.trim()) {
                None => changes.push(change("research_questions", "added", q.id.trim())),
                Some(r) if r.text != q.text && !q.text.trim().is_empty() => {
                    changes.push(change("research_questions", "updated", q.id.trim()))
                }
                Some(_) => {}
            }
        }
        report_config(&before, &next, &mut changes);

        let applied = problems.is_empty() && !dry_run;
        if applied {
            self.set_study_config_with(&study_after, false, actor)?;
            for q in &t.research_questions {
                let id = q.id.trim();
                match rq_before.iter().find(|r| r.id == id) {
                    None => self.create_research_question(id, &q.text, actor)?,
                    Some(r) if r.text != q.text && !q.text.trim().is_empty() => {
                        self.update_research_question(id, &q.text, None, actor)?
                    }
                    Some(_) => {}
                }
            }
            let summary = json!({"template_version": t.version, "changes": changes.len()});
            if next != before {
                self.write_project_config(&before, &next, actor, Some(summary.clone()))?;
            }
            let tx = self.conn.unchecked_transaction()?;
            append_event(
                &tx,
                actor,
                "project.template.import",
                None,
                None,
                json!({"changes": serde_json::to_value(&changes)?}),
            )?;
            tx.commit()?;
        }
        Ok(TemplateReport {
            dry_run,
            applied,
            changes,
            problems,
        })
    }
}

fn report_study(before: &StudyConfig, after: &StudyConfig, out: &mut Vec<TemplateChange>) {
    if before == after {
        return;
    }
    let (b, a) = (
        serde_json::to_value(before).unwrap_or_default(),
        serde_json::to_value(after).unwrap_or_default(),
    );
    if let (Some(b), Some(a)) = (b.as_object(), a.as_object()) {
        for (k, v) in a {
            if b.get(k) != Some(v) {
                out.push(change("study_config", "updated", k.clone()));
            }
        }
    }
}

fn report_config(before: &ProjectConfig, after: &ProjectConfig, out: &mut Vec<TemplateChange>) {
    let b = serde_json::to_value(before).unwrap_or_default();
    let a = serde_json::to_value(after).unwrap_or_default();
    let (Some(b), Some(a)) = (b.as_object(), a.as_object()) else {
        return;
    };
    for part in [
        "attributes",
        "case_fields",
        "memo_templates",
        "checklist",
        "theme_checks",
        "evidence_natures",
        "link_types",
        "matrix_dimensions",
        "interpretive_framework",
    ] {
        let (pb, pa) = (
            b.get(part).unwrap_or(&Value::Null),
            a.get(part).unwrap_or(&Value::Null),
        );
        let ids = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        if pb == pa {
            continue;
        }
        if part == "interpretive_framework" {
            let name = |v: &Value| {
                v.get("preset")
                    .and_then(Value::as_str)
                    .unwrap_or("none")
                    .to_string()
            };
            out.push(change(
                part,
                "updated",
                format!("{} to {}", name(pb), name(pa)),
            ));
            continue;
        }
        let (ib, ia) = (ids(pb), ids(pa));
        for id in ia.iter().filter(|i| !ib.contains(i)) {
            out.push(change(part, "added", id.clone()));
        }
        for id in ib.iter().filter(|i| !ia.contains(i)) {
            out.push(change(part, "removed", id.clone()));
        }
        let find = |v: &Value, id: &str| {
            v.as_array()
                .and_then(|l| {
                    l.iter()
                        .find(|i| i.get("id").and_then(Value::as_str) == Some(id))
                })
                .cloned()
        };
        for id in ia.iter().filter(|i| ib.contains(i)) {
            if find(pb, id) != find(pa, id) {
                out.push(change(part, "updated", id.clone()));
            }
        }
        if ib == ia && ib.iter().all(|i| find(pb, i) == find(pa, i)) {
            out.push(change(part, "updated", "order"));
        }
    }
}
