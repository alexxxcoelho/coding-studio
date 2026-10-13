//! V2, F-39: split of a code with a reassignment worklist, and undo of a
//! merge or a split from an append-only journal.
//!
//! - `code_ops` is the journal: one row per merge or split with what is
//!   needed to take it back. Rows are never edited or deleted. An undo is a
//!   row of `code_op_undos` (one per operation at most) and an audit event.
//! - A split creates two or more new codes (the original stays, active) and
//!   a worklist: every live passage carrying the original. Each passage is
//!   then assigned to one or more of the new codes (the original coding is
//!   removed, logically) or left on the original. `split_decisions` is
//!   append-only: a revision is a new row, the earlier ones stay on record.
//! - The undo of a split gives the original coding back, moves the codings
//!   made on the new codes since to the original, and retires the new codes
//!   (inactive, merged into the original). Nothing is deleted, so a stable
//!   id is never reused.
//! - The undo of a merge gives back the codings that moved, the ones that
//!   were deduplicated, the aliases, the children, the unit codings of the
//!   candidate and the current review decisions, and reactivates the code.
//! - An operation can be undone only when no later operation (not undone)
//!   touches one of its codes: undo the latest first (409 otherwise). A
//!   merge made before the journal existed cannot be undone.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::passage::{CodeNode, Passage};
use cs_model::PHASE_2;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::codetree::{bump_revision, NewCode};
use crate::{append_event, from_json, guard_err, not_found, now, to_json, validation, Store};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS code_ops (
    op_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL,              -- merge | split
    code_id     TEXT NOT NULL,              -- merge: the merged code; split: the original
    other_ids   TEXT NOT NULL,              -- JSON array. merge: [target]; split: the new codes
    rationale   TEXT NOT NULL,
    payload     TEXT NOT NULL,              -- JSON, what the undo needs
    created_at  TEXT NOT NULL,
    created_by  TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS code_op_undos (
    undo_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    op_id       INTEGER NOT NULL UNIQUE REFERENCES code_ops(op_id),
    rationale   TEXT NOT NULL,
    payload     TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    created_by  TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS split_decisions (
    decision_id         INTEGER PRIMARY KEY AUTOINCREMENT,
    op_id               INTEGER NOT NULL REFERENCES code_ops(op_id),
    passage_id          TEXT NOT NULL,
    action              TEXT NOT NULL,      -- assign | leave
    target_ids          TEXT NOT NULL,      -- JSON array of the new codes
    coding_ids          TEXT NOT NULL,      -- JSON array of the codings this decision created
    original_coding_id  INTEGER,            -- the coding it took off the original
    note                TEXT NOT NULL DEFAULT '',
    decided_at          TEXT NOT NULL,
    decided_by          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_split_decisions ON split_decisions(op_id, passage_id);
CREATE TRIGGER IF NOT EXISTS code_ops_no_update BEFORE UPDATE ON code_ops
BEGIN SELECT RAISE(ABORT, 'append-only: code_ops'); END;
CREATE TRIGGER IF NOT EXISTS code_ops_no_delete BEFORE DELETE ON code_ops
BEGIN SELECT RAISE(ABORT, 'append-only: code_ops'); END;
CREATE TRIGGER IF NOT EXISTS code_op_undos_no_update BEFORE UPDATE ON code_op_undos
BEGIN SELECT RAISE(ABORT, 'append-only: code_op_undos'); END;
CREATE TRIGGER IF NOT EXISTS code_op_undos_no_delete BEFORE DELETE ON code_op_undos
BEGIN SELECT RAISE(ABORT, 'append-only: code_op_undos'); END;
CREATE TRIGGER IF NOT EXISTS split_decisions_no_update BEFORE UPDATE ON split_decisions
BEGIN SELECT RAISE(ABORT, 'append-only: split_decisions'); END;
CREATE TRIGGER IF NOT EXISTS split_decisions_no_delete BEFORE DELETE ON split_decisions
BEGIN SELECT RAISE(ABORT, 'append-only: split_decisions'); END;
";

/// Idempotent; runs on every open.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

// ------------------------------------------------------------------ types

#[derive(Debug, Clone, Deserialize)]
pub struct SplitPart {
    pub name: String,
    pub definition: String,
    #[serde(default)]
    pub includes: String,
    #[serde(default)]
    pub excludes: String,
    /// Defaults to the parent of the original.
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SplitRequest {
    pub parts: Vec<SplitPart>,
    pub rationale: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SplitDecision {
    pub passage_id: String,
    /// `assign` or `leave`.
    pub action: String,
    /// New codes (id or name) for `assign`.
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitItem {
    pub passage: Passage,
    /// `pending`, `assigned` or `left`.
    pub status: String,
    pub targets: Vec<String>,
    pub note: String,
    pub decided_at: Option<String>,
    pub decided_by: Option<String>,
    /// Decisions on record for this passage (revisions included).
    pub n_decisions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitWorklist {
    pub op_id: i64,
    pub original: CodeNode,
    pub new_codes: Vec<CodeNode>,
    pub rationale: String,
    pub total: usize,
    pub pending: usize,
    pub assigned: usize,
    pub left: usize,
    /// Passages of the worklist deleted since the split.
    pub gone: usize,
    pub undone: Option<UndoView>,
    pub items: Vec<SplitItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitOutcome {
    pub op_id: i64,
    pub original: CodeNode,
    pub new_codes: Vec<CodeNode>,
    pub worklist: SplitWorklist,
}

#[derive(Debug, Clone, Serialize)]
pub struct UndoView {
    pub rationale: String,
    pub at: String,
    pub by: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpView {
    pub op_id: i64,
    /// `merge` or `split`.
    pub kind: String,
    pub code_id: String,
    pub code_name: String,
    pub other_ids: Vec<String>,
    pub rationale: String,
    pub created_at: String,
    pub created_by: String,
    pub undone: Option<UndoView>,
    /// Counts taken from the journal (codings moved, passages in the worklist).
    pub summary: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct UndoReport {
    pub op_id: i64,
    pub kind: String,
    pub codings_restored: usize,
    pub codings_removed: usize,
    pub codings_moved_back: usize,
    pub unit_codings_restored: usize,
    pub decisions_restored: usize,
    /// Things the undo could not put back because they changed since.
    pub skipped: Vec<String>,
}

// ------------------------------------------------------------ merge journal

/// What a merge is about to change, read before it changes it.
pub(crate) struct MergeBefore {
    from_id: String,
    into_id: String,
    from_token: String,
    into_token: String,
    from_name: String,
    from_parent: Option<String>,
    into_parent: Option<String>,
    from_active: bool,
    children: Vec<String>,
    aliases: Vec<(String, String)>,
    name_alias_inserted: bool,
    moved: Vec<i64>,
    dedup: Vec<i64>,
}

impl Store {
    pub(crate) fn merge_before(
        &self,
        tx: &Connection,
        from_id: &str,
        into_id: &str,
    ) -> Result<MergeBefore> {
        let row = |id: &str| -> Result<(String, String, Option<String>, bool)> {
            Ok(tx.query_row(
                "SELECT code, COALESCE(name, code), parent_id, active FROM codes WHERE code_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0)),
            )?)
        };
        let (from_token, from_name, from_parent, from_active) = row(from_id)?;
        let (into_token, _, into_parent, _) = row(into_id)?;
        let children: Vec<String> = {
            let mut st =
                tx.prepare("SELECT code_id FROM codes WHERE parent_id = ?1 ORDER BY ord")?;
            let v = st
                .query_map([from_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        let aliases: Vec<(String, String)> = {
            let mut st = tx.prepare(
                "SELECT alias, reason FROM code_aliases WHERE code_id = ?1 ORDER BY created_at, rowid",
            )?;
            let v = st
                .query_map([from_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        let name_alias_inserted = tx.query_row(
            "SELECT COUNT(*) FROM code_aliases WHERE alias = ?1",
            [&from_name],
            |r| r.get::<_, i64>(0),
        )? == 0;
        let mut moved = Vec::new();
        let mut dedup = Vec::new();
        let mut st = tx.prepare(
            "SELECT coding_id, passage_id FROM passage_codings
             WHERE code_id = ?1 AND removed_at IS NULL ORDER BY coding_id",
        )?;
        let list: Vec<(i64, String)> = st
            .query_map([from_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (cid, pid) in list {
            let has: i64 = tx.query_row(
                "SELECT COUNT(*) FROM passage_codings WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                params![pid, into_id],
                |r| r.get(0),
            )?;
            if has > 0 {
                dedup.push(cid);
            } else {
                moved.push(cid);
            }
        }
        Ok(MergeBefore {
            from_id: from_id.into(),
            into_id: into_id.into(),
            from_token,
            into_token,
            from_name,
            from_parent,
            into_parent,
            from_active,
            children,
            aliases,
            name_alias_inserted,
            moved,
            dedup,
        })
    }

    /// Writes the journal row of a merge, inside the merge's transaction.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn merge_record(
        &self,
        tx: &Connection,
        b: &MergeBefore,
        unit_codings: &[(String, String, Vec<String>, Vec<String>)],
        decisions: &[(String, String, Vec<String>, Vec<String>)],
        rationale: &str,
        actor: &str,
    ) -> Result<i64> {
        let payload = json!({
            "from_id": b.from_id, "into_id": b.into_id,
            "from_token": b.from_token, "into_token": b.into_token,
            "from_name": b.from_name,
            "from_parent": b.from_parent, "into_parent": b.into_parent,
            "from_active": b.from_active,
            "children": b.children,
            "aliases": b.aliases,
            "name_alias_inserted": b.name_alias_inserted,
            "moved": b.moved, "dedup": b.dedup,
            "unit_codings": unit_codings.iter().map(|(iv, unit, before, after)| json!({
                "interview_id": iv, "unit_id": unit,
                "before": before, "after": after,
            })).collect::<Vec<_>>(),
            "decisions": decisions.iter().map(|(iv, unit, before, after)| json!({
                "interview_id": iv, "unit_id": unit, "before": before, "after": after,
            })).collect::<Vec<_>>(),
        });
        tx.execute(
            "INSERT INTO code_ops (kind, code_id, other_ids, rationale, payload, created_at, created_by)
             VALUES ('merge', ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                b.from_id,
                to_json(&[&b.into_id]),
                rationale,
                payload.to_string(),
                now(),
                actor
            ],
        )?;
        Ok(tx.last_insert_rowid())
    }

    // ------------------------------------------------------------- journal

    fn undo_of(&self, op_id: i64) -> Result<Option<UndoView>> {
        Ok(self
            .conn
            .query_row(
                "SELECT rationale, created_at, created_by FROM code_op_undos WHERE op_id = ?1",
                [op_id],
                |r| {
                    Ok(UndoView {
                        rationale: r.get(0)?,
                        at: r.get(1)?,
                        by: r.get(2)?,
                    })
                },
            )
            .ok())
    }

    fn op_row(&self, op_id: i64) -> Result<(String, String, Vec<String>, String, Value)> {
        self.conn
            .query_row(
                "SELECT kind, code_id, other_ids, rationale, payload FROM code_ops WHERE op_id = ?1",
                [op_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .map_err(|_| not_found(format!("code operation {op_id}")))
            .and_then(|(k, c, o, ra, p)| {
                Ok((k, c, from_json::<Vec<String>>(&o)?, ra, from_json::<Value>(&p)?))
            })
    }

    /// The journal, oldest first; with `code` only the operations that
    /// involve it.
    pub fn code_ops(&self, code: Option<&str>) -> Result<Vec<OpView>> {
        let wanted = match code {
            Some(c) => Some(self.resolve_code_exact(c)?),
            None => None,
        };
        let names: BTreeMap<String, String> = crate::codetree::code_names(&self.conn)?;
        let ids: Vec<i64> = {
            let mut st = self
                .conn
                .prepare("SELECT op_id FROM code_ops ORDER BY op_id")?;
            let v = st
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        let mut out = Vec::new();
        for id in ids {
            let (kind, code_id, others, rationale, payload) = self.op_row(id)?;
            if wanted
                .as_ref()
                .is_some_and(|w| *w != code_id && !others.contains(w))
            {
                continue;
            }
            let (created_at, created_by): (String, String) = self.conn.query_row(
                "SELECT created_at, created_by FROM code_ops WHERE op_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let n = |k: &str| payload.get(k).and_then(Value::as_array).map_or(0, Vec::len);
            let summary = if kind == "merge" {
                json!({"codings_moved": n("moved"), "codings_deduplicated": n("dedup"),
                       "unit_codings": n("unit_codings")})
            } else {
                json!({"worklist": n("passages")})
            };
            out.push(OpView {
                op_id: id,
                kind,
                code_name: names.get(&code_id).cloned().unwrap_or_default(),
                code_id,
                other_ids: others,
                rationale,
                created_at,
                created_by,
                undone: self.undo_of(id)?,
                summary,
            });
        }
        Ok(out)
    }

    /// Stable id of a code given by id, name, alias or token, merges not
    /// followed (the journal is about the code itself).
    fn resolve_code_exact(&self, key: &str) -> Result<String> {
        Ok(self
            .code_node(
                &self
                    .resolve_code(key)
                    .map(|r| r.code_id)
                    .or_else(|_| self.code_node(key).map(|n| n.code_id))?,
            )?
            .code_id)
    }

    // --------------------------------------------------------------- split

    /// Splits a code: creates the new codes and the worklist of its passages.
    pub fn split_code(&self, key: &str, req: &SplitRequest, actor: &str) -> Result<SplitOutcome> {
        let node = self.code_node(&self.resolve_code(key)?.code_id)?;
        if node.system {
            return Err(validation("the reserved code cannot be split"));
        }
        if node.merged_into.is_some() || !node.active {
            return Err(validation(format!(
                "code {} is inactive or merged and cannot be split",
                node.name
            )));
        }
        let rationale = req.rationale.trim();
        if rationale.is_empty() {
            return Err(validation("a split needs a rationale"));
        }
        if req.parts.len() < 2 {
            return Err(validation("a split gives two or more new codes"));
        }
        let mut seen = BTreeSet::new();
        for p in &req.parts {
            if !seen.insert(p.name.trim().to_lowercase()) {
                return Err(validation(format!(
                    "the name {} is given twice",
                    p.name.trim()
                )));
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut new_ids = Vec::new();
        for p in &req.parts {
            let id = self.create_code_v1_in(
                &tx,
                &NewCode {
                    name: p.name.clone(),
                    definition: p.definition.clone(),
                    includes: p.includes.clone(),
                    excludes: p.excludes.clone(),
                    parent_id: p.parent_id.clone().or_else(|| node.parent_id.clone()),
                    research_questions: node.research_questions.clone(),
                    memo: String::new(),
                    reason: Some(format!("split of {}: {rationale}", node.name)),
                },
                actor,
            )?;
            new_ids.push(id);
        }
        let passages: Vec<Value> = {
            let mut st = tx.prepare(
                "SELECT pc.coding_id, pc.passage_id FROM passage_codings pc
                 JOIN passages p USING (passage_id)
                 WHERE pc.code_id = ?1 AND pc.removed_at IS NULL AND p.deleted_at IS NULL
                 ORDER BY p.interview_id, p.unit_id, p.start_offset, p.passage_id",
            )?;
            let v = st
                .query_map([&node.code_id], |r| {
                    Ok(json!({"coding_id": r.get::<_, i64>(0)?, "passage_id": r.get::<_, String>(1)?}))
                })?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        let payload =
            json!({"original_id": node.code_id, "new_ids": new_ids, "passages": passages});
        tx.execute(
            "INSERT INTO code_ops (kind, code_id, other_ids, rationale, payload, created_at, created_by)
             VALUES ('split', ?1, ?2, ?3, ?4, ?5, ?6)",
            params![node.code_id, to_json(&new_ids), rationale, payload.to_string(), now(), actor],
        )?;
        let op_id = tx.last_insert_rowid();
        bump_revision(&tx, actor, "split", Some(&node.code_id))?;
        append_event(
            &tx,
            actor,
            "codebook.code.split",
            None,
            Some(&node.code_id),
            json!({
                "op_id": op_id, "original": node.code_id, "new_codes": new_ids,
                "rationale": rationale, "worklist": passages.len(),
            }),
        )?;
        tx.commit()?;
        let worklist = self.split_worklist(op_id, None)?;
        Ok(SplitOutcome {
            op_id,
            original: worklist.original.clone(),
            new_codes: worklist.new_codes.clone(),
            worklist,
        })
    }

    fn split_op(&self, op_id: i64) -> Result<(String, Vec<String>, String, Vec<(i64, String)>)> {
        let (kind, code_id, others, rationale, payload) = self.op_row(op_id)?;
        if kind != "split" {
            return Err(validation(format!("operation {op_id} is not a split")));
        }
        let passages = payload["passages"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some((
                            p["coding_id"].as_i64()?,
                            p["passage_id"].as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok((code_id, others, rationale, passages))
    }

    /// Latest decision of each passage of an operation.
    #[allow(clippy::type_complexity)]
    fn latest_decisions(
        &self,
        op_id: i64,
    ) -> Result<
        BTreeMap<
            String,
            (
                String,
                Vec<String>,
                Vec<i64>,
                Option<i64>,
                String,
                String,
                String,
            ),
        >,
    > {
        let mut st = self.conn.prepare(
            "SELECT passage_id, action, target_ids, coding_ids, original_coding_id, note, decided_at, decided_by
             FROM split_decisions WHERE op_id = ?1
               AND decision_id IN (SELECT MAX(decision_id) FROM split_decisions WHERE op_id = ?1 GROUP BY passage_id)",
        )?;
        let rows = st
            .query_map([op_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = BTreeMap::new();
        for (p, a, t, c, o, n, at, by) in rows {
            out.insert(p, (a, from_json(&t)?, from_json(&c)?, o, n, at, by));
        }
        Ok(out)
    }

    /// The worklist of a split. `status` keeps `pending`, `assigned` or `left`.
    pub fn split_worklist(&self, op_id: i64, status: Option<&str>) -> Result<SplitWorklist> {
        if let Some(s) = status {
            if !["pending", "assigned", "left"].contains(&s) {
                return Err(validation("status is pending, assigned or left"));
            }
        }
        let (original, new_ids, rationale, passages) = self.split_op(op_id)?;
        let latest = self.latest_decisions(op_id)?;
        let counts: BTreeMap<String, usize> = {
            let mut st = self.conn.prepare(
                "SELECT passage_id, COUNT(*) FROM split_decisions WHERE op_id = ?1 GROUP BY passage_id",
            )?;
            let v = st
                .query_map([op_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
                })?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        let (mut pending, mut assigned, mut left, mut gone) = (0, 0, 0, 0);
        let mut items = Vec::new();
        for (_, pid) in &passages {
            let Ok(passage) = self.passage(pid) else {
                gone += 1;
                continue;
            };
            let (st, targets, note, at, by) = match latest.get(pid) {
                None => ("pending", vec![], String::new(), None, None),
                Some((a, t, _, _, n, at, by)) => (
                    if a == "assign" { "assigned" } else { "left" },
                    t.clone(),
                    n.clone(),
                    Some(at.clone()),
                    Some(by.clone()),
                ),
            };
            match st {
                "pending" => pending += 1,
                "assigned" => assigned += 1,
                _ => left += 1,
            }
            if status.is_none_or(|s| s == st) {
                items.push(SplitItem {
                    passage,
                    status: st.into(),
                    targets,
                    note,
                    decided_at: at,
                    decided_by: by,
                    n_decisions: counts.get(pid).copied().unwrap_or(0),
                });
            }
        }
        Ok(SplitWorklist {
            op_id,
            original: self.code_node(&original)?,
            new_codes: new_ids
                .iter()
                .map(|i| self.code_node(i))
                .collect::<Result<_>>()?,
            rationale,
            total: pending + assigned + left,
            pending,
            assigned,
            left,
            gone,
            undone: self.undo_of(op_id)?,
            items,
        })
    }

    /// Records decisions on passages of a split, all or nothing. A passage
    /// may be decided again; the earlier decision stays on record and its
    /// effect is reversed before the new one is applied.
    pub fn split_decide(
        &self,
        op_id: i64,
        decisions: &[SplitDecision],
        actor: &str,
    ) -> Result<SplitWorklist> {
        let (original, new_ids, _, passages) = self.split_op(op_id)?;
        if self.undo_of(op_id)?.is_some() {
            return Err(guard_err(format!("split {op_id} was undone")));
        }
        if decisions.is_empty() {
            return Err(validation("no decision given"));
        }
        let listed: BTreeMap<&str, i64> = passages.iter().map(|(c, p)| (p.as_str(), *c)).collect();
        let mut seen = BTreeSet::new();
        let mut plan = Vec::new();
        for d in decisions {
            if !seen.insert(d.passage_id.clone()) {
                return Err(validation(format!(
                    "passage {} is decided twice",
                    d.passage_id
                )));
            }
            let Some(orig_coding) = listed.get(d.passage_id.as_str()) else {
                return Err(validation(format!(
                    "passage {} is not in the worklist of split {op_id}",
                    d.passage_id
                )));
            };
            self.passage(&d.passage_id)
                .map_err(|_| validation(format!("passage {} was deleted", d.passage_id)))?;
            match d.action.as_str() {
                "assign" => {
                    if d.targets.is_empty() {
                        return Err(validation("assign needs one or more of the new codes"));
                    }
                }
                "leave" => {
                    if !d.targets.is_empty() {
                        return Err(validation("leave takes no target"));
                    }
                }
                other => {
                    return Err(validation(format!(
                        "action is assign or leave, not {other}"
                    )))
                }
            }
            let mut targets = Vec::new();
            for t in &d.targets {
                let id = self.resolve_code(t)?.code_id;
                if !new_ids.contains(&id) {
                    return Err(validation(format!(
                        "{t} is not one of the new codes of split {op_id}"
                    )));
                }
                if !targets.contains(&id) {
                    targets.push(id);
                }
            }
            plan.push((d, targets, *orig_coding));
        }
        let latest = self.latest_decisions(op_id)?;
        let tx = self.conn.unchecked_transaction()?;
        let ts = now();
        for (d, targets, orig_coding) in &plan {
            if let Some((a, _, coding_ids, orig, ..)) = latest.get(&d.passage_id) {
                if a == "assign" {
                    self.revert_assignment(
                        &tx,
                        &d.passage_id,
                        coding_ids,
                        *orig,
                        "split revised",
                        actor,
                    )?;
                }
            }
            let mut created = Vec::new();
            let mut original_removed = None;
            if d.action == "assign" {
                for t in targets {
                    let before: Option<i64> = tx
                        .query_row(
                            "SELECT coding_id FROM passage_codings WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                            params![d.passage_id, t],
                            |r| r.get(0),
                        )
                        .ok();
                    self.add_coding_in(&tx, &d.passage_id, t, "", actor)?;
                    if before.is_none() {
                        created.push(tx.query_row(
                            "SELECT coding_id FROM passage_codings WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                            params![d.passage_id, t],
                            |r| r.get::<_, i64>(0),
                        )?);
                    }
                }
                let n = tx.execute(
                    "UPDATE passage_codings SET removed_at = ?2, removed_by = ?3, remove_reason = 'split'
                     WHERE coding_id = ?1 AND removed_at IS NULL",
                    params![orig_coding, ts, actor],
                )?;
                if n > 0 {
                    original_removed = Some(*orig_coding);
                }
                self.sync_passage_unit_in(&tx, &d.passage_id, actor)?;
            }
            tx.execute(
                "INSERT INTO split_decisions (op_id, passage_id, action, target_ids, coding_ids, original_coding_id, note, decided_at, decided_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    op_id, d.passage_id, d.action, to_json(targets), to_json(&created),
                    original_removed, d.note.trim(), ts, actor
                ],
            )?;
        }
        append_event(
            &tx,
            actor,
            "codebook.code.split_decide",
            None,
            Some(&original),
            json!({
                "op_id": op_id,
                "decisions": plan.iter().map(|(d, t, _)| json!({
                    "passage_id": d.passage_id, "action": d.action, "targets": t,
                })).collect::<Vec<_>>(),
            }),
        )?;
        tx.commit()?;
        self.split_worklist(op_id, None)
    }

    /// Removes the codings an assignment created and gives the original back.
    fn revert_assignment(
        &self,
        tx: &Connection,
        passage_id: &str,
        coding_ids: &[i64],
        original_coding: Option<i64>,
        reason: &str,
        actor: &str,
    ) -> Result<usize> {
        let ts = now();
        let mut removed = 0;
        for cid in coding_ids {
            removed += tx.execute(
                "UPDATE passage_codings SET removed_at = ?2, removed_by = ?3, remove_reason = ?4
                 WHERE coding_id = ?1 AND removed_at IS NULL",
                params![cid, ts, actor, reason],
            )?;
        }
        if let Some(o) = original_coding {
            tx.execute(
                "UPDATE passage_codings SET removed_at = NULL, removed_by = NULL, remove_reason = NULL
                 WHERE coding_id = ?1 AND removed_at IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM passage_codings x
                                   WHERE x.passage_id = passage_codings.passage_id
                                     AND x.code_id = passage_codings.code_id AND x.removed_at IS NULL)",
                [o],
            )?;
        }
        self.sync_passage_unit_in(tx, passage_id, actor)?;
        Ok(removed)
    }

    // ---------------------------------------------------------------- undo

    /// Undoes a merge or a split. See the module documentation.
    pub fn undo_code_op(&self, op_id: i64, rationale: &str, actor: &str) -> Result<UndoReport> {
        let (kind, code_id, others, _, payload) = self.op_row(op_id)?;
        let rationale = rationale.trim();
        if rationale.is_empty() {
            return Err(validation("an undo needs a rationale"));
        }
        if self.undo_of(op_id)?.is_some() {
            return Err(guard_err(format!("operation {op_id} was already undone")));
        }
        // The latest first.
        let mine: BTreeSet<&String> = std::iter::once(&code_id).chain(others.iter()).collect();
        for later in self.code_ops(None)?.into_iter().filter(|o| o.op_id > op_id) {
            if later.undone.is_none()
                && (mine.contains(&later.code_id)
                    || later.other_ids.iter().any(|i| mine.contains(i)))
            {
                return Err(guard_err(format!(
                    "operation {} came after and touches the same codes: undo it first",
                    later.op_id
                )));
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut rep = UndoReport {
            op_id,
            kind: kind.clone(),
            codings_restored: 0,
            codings_removed: 0,
            codings_moved_back: 0,
            unit_codings_restored: 0,
            decisions_restored: 0,
            skipped: vec![],
        };
        if kind == "merge" {
            self.undo_merge_in(&tx, &payload, rationale, actor, &mut rep)?;
        } else {
            self.undo_split_in(&tx, op_id, &code_id, &others, actor, &mut rep)?;
        }
        tx.execute(
            "INSERT INTO code_op_undos (op_id, rationale, payload, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                op_id,
                rationale,
                serde_json::to_string(&rep.skipped)?,
                now(),
                actor
            ],
        )?;
        bump_revision(&tx, actor, &format!("undo_{kind}"), Some(&code_id))?;
        append_event(
            &tx,
            actor,
            &format!("codebook.code.undo_{kind}"),
            None,
            Some(&code_id),
            json!({
                "op_id": op_id, "rationale": rationale,
                "codings_restored": rep.codings_restored,
                "codings_removed": rep.codings_removed,
                "codings_moved_back": rep.codings_moved_back,
                "unit_codings_restored": rep.unit_codings_restored,
                "decisions_restored": rep.decisions_restored,
                "skipped": rep.skipped,
            }),
        )?;
        tx.commit()?;
        Ok(rep)
    }

    fn undo_split_in(
        &self,
        tx: &Connection,
        op_id: i64,
        original: &str,
        new_ids: &[String],
        actor: &str,
        rep: &mut UndoReport,
    ) -> Result<()> {
        let ts = now();
        let token: String = tx.query_row(
            "SELECT code FROM codes WHERE code_id = ?1",
            [original],
            |r| r.get(0),
        )?;
        for (pid, (action, _, coding_ids, orig, ..)) in self.latest_decisions(op_id)? {
            if action == "assign" {
                rep.codings_removed += self.revert_assignment(
                    tx,
                    &pid,
                    coding_ids.as_slice(),
                    orig,
                    "split undone",
                    actor,
                )?;
                if orig.is_some() {
                    rep.codings_restored += 1;
                }
            }
        }
        // Codings made on the new codes since: back to the original.
        for id in new_ids {
            let list: Vec<(i64, String)> = {
                let mut st = tx.prepare(
                    "SELECT coding_id, passage_id FROM passage_codings WHERE code_id = ?1 AND removed_at IS NULL",
                )?;
                let v = st
                    .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                v
            };
            for (cid, pid) in list {
                let has: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM passage_codings WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                    params![pid, original],
                    |r| r.get(0),
                )?;
                if has > 0 {
                    tx.execute(
                        "UPDATE passage_codings SET removed_at = ?2, removed_by = ?3, remove_reason = 'split undone' WHERE coding_id = ?1",
                        params![cid, ts, actor],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE passage_codings SET code_id = ?2 WHERE coding_id = ?1",
                        params![cid, original],
                    )?;
                    rep.codings_moved_back += 1;
                }
                self.sync_passage_unit_in(tx, &pid, actor)?;
            }
            tx.execute(
                "UPDATE codes SET active = 0, merged_into = ?2, local = 1, version = version + 1,
                        name = COALESCE(name, code) || ?3 WHERE code_id = ?1",
                params![id, token, format!(" [split {op_id} undone]")],
            )?;
        }
        Ok(())
    }

    fn undo_merge_in(
        &self,
        tx: &Connection,
        p: &Value,
        rationale: &str,
        actor: &str,
        rep: &mut UndoReport,
    ) -> Result<()> {
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_string();
        let (from_id, into_id, into_token) = (s("from_id"), s("into_id"), s("into_token"));
        let current: Option<String> = tx
            .query_row(
                "SELECT merged_into FROM codes WHERE code_id = ?1",
                [&from_id],
                |r| r.get(0),
            )
            .map_err(|_| not_found(format!("code {from_id}")))?;
        if current.as_deref() != Some(into_token.as_str()) {
            return Err(guard_err("the code is no longer merged into that target"));
        }
        let ids = |k: &str| -> Vec<i64> {
            p[k].as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default()
        };
        let strs = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let ts = now();
        // The code itself and its place in the tree.
        let from_parent = p["from_parent"].as_str().map(str::to_string);
        tx.execute(
            "UPDATE codes SET merged_into = NULL, active = ?2, parent_id = ?3, version = version + 1 WHERE code_id = ?1",
            params![from_id, p["from_active"].as_bool().unwrap_or(true) as i64, from_parent],
        )?;
        // The target sat under the merged code: put it back.
        if p["into_parent"].as_str() == Some(from_id.as_str()) {
            tx.execute(
                "UPDATE codes SET parent_id = ?2 WHERE code_id = ?1",
                params![into_id, from_id],
            )?;
        }
        for child in strs(&p["children"]) {
            tx.execute(
                "UPDATE codes SET parent_id = ?2 WHERE code_id = ?1 AND parent_id = ?3",
                params![child, from_id, into_id],
            )?;
        }
        // Aliases.
        for a in p["aliases"].as_array().into_iter().flatten() {
            let (alias, reason) = (
                a[0].as_str().unwrap_or_default(),
                a[1].as_str().unwrap_or_default(),
            );
            tx.execute(
                "UPDATE code_aliases SET code_id = ?2, reason = ?3 WHERE alias = ?1 AND code_id = ?4",
                params![alias, from_id, reason, into_id],
            )?;
        }
        if p["name_alias_inserted"].as_bool().unwrap_or(false) {
            tx.execute(
                "DELETE FROM code_aliases WHERE alias = ?1 AND code_id = ?2 AND reason = 'merge'",
                params![s("from_name"), into_id],
            )?;
        }
        // Passage codings.
        let mut touched = BTreeSet::new();
        for cid in ids("moved") {
            let row: Option<(String, String, Option<String>)> = tx
                .query_row(
                    "SELECT passage_id, code_id, removed_at FROM passage_codings WHERE coding_id = ?1",
                    [cid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok();
            match row {
                Some((pid, code, None)) if code == into_id => {
                    let has: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM passage_codings WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                        params![pid, from_id],
                        |r| r.get(0),
                    )?;
                    if has > 0 {
                        rep.skipped.push(format!(
                            "coding {cid}: the passage already carries the code again"
                        ));
                    } else {
                        tx.execute(
                            "UPDATE passage_codings SET code_id = ?2 WHERE coding_id = ?1",
                            params![cid, from_id],
                        )?;
                        rep.codings_restored += 1;
                        touched.insert(pid);
                    }
                }
                _ => rep
                    .skipped
                    .push(format!("coding {cid} changed since the merge")),
            }
        }
        for cid in ids("dedup") {
            let n = tx.execute(
                "UPDATE passage_codings SET removed_at = NULL, removed_by = NULL, remove_reason = NULL
                 WHERE coding_id = ?1 AND remove_reason = 'merged' AND removed_at IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM passage_codings x WHERE x.passage_id = passage_codings.passage_id
                                   AND x.code_id = passage_codings.code_id AND x.removed_at IS NULL)",
                [cid],
            )?;
            if n > 0 {
                rep.codings_restored += 1;
                if let Ok(pid) = tx.query_row(
                    "SELECT passage_id FROM passage_codings WHERE coding_id = ?1",
                    [cid],
                    |r| r.get::<_, String>(0),
                ) {
                    touched.insert(pid);
                }
            } else {
                rep.skipped
                    .push(format!("coding {cid} changed since the merge"));
            }
        }
        for pid in &touched {
            self.sync_passage_unit_in(tx, pid, actor)?;
        }
        // Unit codings of the candidate (phase 2).
        let (from_token, into_tok) = (s("from_token"), s("into_token"));
        for u in p["unit_codings"].as_array().into_iter().flatten() {
            let (iv, unit) = (
                u["interview_id"].as_str().unwrap_or_default(),
                u["unit_id"].as_str().unwrap_or_default(),
            );
            let before = strs(&u["before"]);
            let after = strs(&u["after"]);
            let cur: Option<String> = tx
                .query_row(
                    "SELECT codes FROM human_codings WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3",
                    params![iv, PHASE_2, unit],
                    |r| r.get(0),
                )
                .ok();
            let cur_codes: Vec<String> = cur
                .as_deref()
                .map(from_json)
                .transpose()?
                .unwrap_or_default();
            let mut a: Vec<String> = after.clone();
            a.sort();
            let mut c = cur_codes.clone();
            c.sort();
            if a == c {
                tx.execute(
                    "UPDATE human_codings SET codes = ?4, updated_at = ?5 WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3",
                    params![iv, PHASE_2, unit, to_json(&before), ts],
                )?;
                rep.unit_codings_restored += 1;
            } else {
                rep.skipped.push(format!(
                    "unit {unit}: coding changed since the merge ({from_token} / {into_tok})"
                ));
            }
        }
        // Review decisions: a new decision with the earlier codes, when the
        // current one is still the one the merge wrote.
        for d in p["decisions"].as_array().into_iter().flatten() {
            let (iv, unit) = (
                d["interview_id"].as_str().unwrap_or_default(),
                d["unit_id"].as_str().unwrap_or_default(),
            );
            let before = strs(&d["before"]);
            let after = strs(&d["after"]);
            let cur: Option<String> = tx
                .query_row(
                    "SELECT final_codes FROM arbitrations WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3 ORDER BY arb_id DESC LIMIT 1",
                    params![iv, PHASE_2, unit],
                    |r| r.get(0),
                )
                .ok();
            let mut cur_codes: Vec<String> = cur
                .as_deref()
                .map(from_json)
                .transpose()?
                .unwrap_or_default();
            cur_codes.sort();
            let mut a = after.clone();
            a.sort();
            if cur_codes == a {
                tx.execute(
                    "INSERT INTO arbitrations (interview_id, phase, unit_id, choice, final_codes, rationale, decided_by, decided_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        iv, PHASE_2, unit,
                        to_json(&cs_model::ArbitrationChoice::Custom),
                        to_json(&before),
                        format!("Merge undone: {rationale}"),
                        actor, ts
                    ],
                )?;
                rep.decisions_restored += 1;
            } else {
                rep.skipped.push(format!(
                    "unit {unit}: review decision changed since the merge"
                ));
            }
        }
        Ok(())
    }
}
