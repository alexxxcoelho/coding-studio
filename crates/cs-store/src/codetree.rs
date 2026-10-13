//! V1 lot 1: stable code identity, names, aliases, the code tree, roll-up
//! counts and the codebook version.
//!
//! - Every code has an opaque stable id (`C-0001`), a free display name, and
//!   aliases (former names, synonyms, merged codes, the legacy token). The
//!   legacy upper-case token (`codes.code`) stays the key of the unit-level
//!   coding and of the AI pass; it never changes.
//! - The reserved system code "Plus tard" (`LATER`) is not stored in `codes`;
//!   it is a virtual node that is never counted in an analysis.
//! - Moving a code under a parent never touches a coding. Merging a code
//!   re-points its names and aliases to the target.
//! - The codebook version is the number of the last row of
//!   `codebook_revisions`; every change of the codebook adds one.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::passage::{later_node, CodeAlias, CodeCounts, CodeNode, CodeTree, LATER_ID};
use cs_model::NA;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::json;

use crate::codebook::{present, CODE_COLUMNS};
use crate::{append_event, clean_opt, not_found, now, validation, Store};

/// Longest accepted display name, in characters.
const MAX_NAME: usize = 200;

/// Partial edit of a code through its stable id. Omitted fields are
/// unchanged; for `toe`, `example` and `reason` an explicit null clears.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodeUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub definition: Option<String>,
    #[serde(default)]
    pub includes: Option<String>,
    #[serde(default)]
    pub excludes: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub toe: Option<Option<String>>,
    #[serde(default)]
    pub research_questions: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub example: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub reason: Option<Option<String>>,
    #[serde(default)]
    pub in_prompt: Option<bool>,
}

/// A new code made from a passage: from an existing one (`passage_id`, a
/// highlight for instance) or from a selection (`passage`). The passage
/// becomes the origin and the first example of the code and is coded with it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewCodeFromPassage {
    pub name: String,
    pub definition: String,
    #[serde(default)]
    pub includes: String,
    #[serde(default)]
    pub excludes: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub passage_id: Option<String>,
    #[serde(default)]
    pub passage: Option<crate::passages::NewPassage>,
    #[serde(default)]
    pub research_questions: Vec<String>,
    /// Why the code is created (free text).
    #[serde(default)]
    pub reason: Option<String>,
}

/// One row of `codebook_revisions`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Revision {
    pub rev: i64,
    pub ts: String,
    pub actor: String,
    pub reason: String,
    pub target: Option<String>,
}

/// A code found by id, name, alias or token, after following merges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCode {
    pub code_id: String,
    pub token: String,
    pub name: String,
    pub system: bool,
    pub active: bool,
}

/// Row of `codes` as the identity layer needs it.
#[derive(Debug, Clone)]
struct Row {
    code_id: String,
    token: String,
    name: String,
    parent_id: Option<String>,
    active: bool,
    merged_into: Option<String>,
}

fn rows(conn: &Connection) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare(
        "SELECT code_id, code, COALESCE(name, code), parent_id, active, merged_into
         FROM codes WHERE code_id IS NOT NULL ORDER BY ord, code",
    )?;
    let out = stmt
        .query_map([], |r| {
            Ok(Row {
                code_id: r.get(0)?,
                token: r.get(1)?,
                name: r.get(2)?,
                parent_id: r.get(3)?,
                active: r.get::<_, i64>(4)? != 0,
                merged_into: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(out)
}

fn aliases(conn: &Connection) -> Result<Vec<(String, CodeAlias)>> {
    let mut stmt = conn.prepare(
        "SELECT code_id, alias, reason, created_at FROM code_aliases
         ORDER BY created_at, rowid",
    )?;
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                CodeAlias {
                    alias: r.get(1)?,
                    reason: r.get(2)?,
                    created_at: r.get(3)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(out)
}

/// Display name of every code by stable id.
pub(crate) fn code_names(conn: &Connection) -> Result<BTreeMap<String, String>> {
    Ok(rows(conn)?
        .into_iter()
        .map(|r| (r.code_id, r.name))
        .collect())
}

fn fold(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Gives an id, a name and a token alias to every code row that lacks them.
/// Idempotent. Returns how many rows got a new id.
pub(crate) fn backfill_identity(conn: &Connection) -> Result<usize> {
    let mut next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(CAST(substr(code_id, 3) AS INTEGER)), 0) FROM codes
         WHERE code_id GLOB 'C-[0-9]*'",
        [],
        |r| r.get(0),
    )?;
    let pending: Vec<String> = {
        let mut stmt =
            conn.prepare("SELECT code FROM codes WHERE code_id IS NULL ORDER BY ord, code")?;
        let v = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        v
    };
    let ts = now();
    for token in &pending {
        next += 1;
        conn.execute(
            "UPDATE codes SET code_id = ?2 WHERE code = ?1",
            params![token, format!("C-{next:04}")],
        )?;
    }
    conn.execute("UPDATE codes SET name = code WHERE name IS NULL", [])?;
    conn.execute(
        "INSERT OR IGNORE INTO code_aliases (alias, code_id, reason, created_at)
         SELECT code, code_id, 'legacy_token', ?1 FROM codes
         WHERE code_id IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM code_aliases a WHERE a.alias = codes.code)",
        [ts],
    )?;
    Ok(pending.len())
}

/// Adds one row to the codebook revisions (the codebook version advances).
pub(crate) fn bump_revision(
    conn: &Connection,
    actor: &str,
    reason: &str,
    target: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO codebook_revisions (ts, actor, reason, target) VALUES (?1, ?2, ?3, ?4)",
        params![now(), actor, reason, target],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Display name rules: non-empty, one line, not reserved.
fn check_display_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(validation("a code needs a name"));
    }
    if name.chars().count() > MAX_NAME {
        return Err(validation(format!(
            "a code name is at most {MAX_NAME} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(validation(
            "a code name is one line without control characters",
        ));
    }
    let f = fold(name);
    if f == fold(NA) || f == fold(LATER_ID) || f == fold(cs_model::passage::LATER_NAME) {
        return Err(validation(format!(
            "{name} is reserved and cannot be a code name"
        )));
    }
    Ok(name.to_string())
}

impl Store {
    // ------------------------------------------------------------ version

    /// The codebook version: the number of the last revision, 0 for a
    /// codebook never changed since the revisions exist.
    pub fn codebook_version(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(rev), 0) FROM codebook_revisions",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn codebook_revisions(&self) -> Result<Vec<Revision>> {
        let mut stmt = self.conn.prepare(
            "SELECT rev, ts, actor, reason, target FROM codebook_revisions ORDER BY rev",
        )?;
        let out = stmt
            .query_map([], |r| {
                Ok(Revision {
                    rev: r.get(0)?,
                    ts: r.get(1)?,
                    actor: r.get(2)?,
                    reason: r.get(3)?,
                    target: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    // --------------------------------------------------------- resolution

    /// Finds a code by stable id, then display name, alias or legacy token
    /// (case-insensitive for names and aliases), and follows merges to the code
    /// that stands for it now. The reserved code is found by `LATER` or
    /// "Plus tard".
    pub fn resolve_code(&self, key: &str) -> Result<ResolvedCode> {
        let key = key.trim();
        if key.is_empty() {
            return Err(validation("no code given"));
        }
        let f = fold(key);
        if f == fold(LATER_ID) || f == fold(cs_model::passage::LATER_NAME) {
            return Ok(ResolvedCode {
                code_id: LATER_ID.into(),
                token: LATER_ID.into(),
                name: cs_model::passage::LATER_NAME.into(),
                system: true,
                active: true,
            });
        }
        let all = rows(&self.conn)?;
        let alias_list = aliases(&self.conn)?;
        let found = all
            .iter()
            .find(|r| r.code_id == key)
            .or_else(|| all.iter().find(|r| fold(&r.name) == f))
            .or_else(|| {
                alias_list
                    .iter()
                    .find(|(_, a)| fold(&a.alias) == f)
                    .and_then(|(id, _)| all.iter().find(|r| &r.code_id == id))
            })
            .or_else(|| all.iter().find(|r| r.token == key));
        let mut cur = found.ok_or_else(|| not_found(format!("code {key}")))?;
        for _ in 0..64 {
            let Some(target) = &cur.merged_into else {
                break;
            };
            match all.iter().find(|r| &r.token == target) {
                Some(next) => cur = next,
                None => break,
            }
        }
        Ok(ResolvedCode {
            code_id: cur.code_id.clone(),
            token: cur.token.clone(),
            name: cur.name.clone(),
            system: false,
            active: cur.active && cur.merged_into.is_none(),
        })
    }

    /// The row of a code by stable id or exact token, merges not followed.
    fn find_exact(&self, key: &str) -> Result<Row> {
        let key = key.trim();
        rows(&self.conn)?
            .into_iter()
            .find(|r| r.code_id == key || r.token == key)
            .ok_or_else(|| not_found(format!("code {key}")))
    }

    // -------------------------------------------------------------- nodes

    pub(crate) fn raw_nodes(&self) -> Result<Vec<CodeNode>> {
        let sql = format!(
            "SELECT {CODE_COLUMNS}, code_id, COALESCE(name, code), parent_id, includes, excludes,
                    origin_passage_id, example_passage_id
             FROM codes WHERE code_id IS NOT NULL ORDER BY ord, code"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let found = stmt
            .query_map([], |r| {
                let c = crate::codebook::row_to_code(r)?;
                Ok((
                    c,
                    r.get::<_, String>(14)?,
                    r.get::<_, String>(15)?,
                    r.get::<_, Option<String>>(16)?,
                    r.get::<_, String>(17)?,
                    r.get::<_, String>(18)?,
                    r.get::<_, Option<String>>(19)?,
                    r.get::<_, Option<String>>(20)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let id_of: BTreeMap<String, String> = found
            .iter()
            .map(|(c, id, ..)| (c.code.clone(), id.clone()))
            .collect();
        let mut alias_map: BTreeMap<String, Vec<CodeAlias>> = BTreeMap::new();
        for (id, a) in aliases(&self.conn)? {
            alias_map.entry(id).or_default().push(a);
        }
        let mut memos = self.current_code_memos()?;
        Ok(found
            .into_iter()
            .map(
                |(c, code_id, name, parent_id, includes, excludes, origin_p, example_p)| CodeNode {
                    aliases: alias_map.remove(&code_id).unwrap_or_default(),
                    memo: memos.remove(&code_id).unwrap_or_default(),
                    merged_into: c.merged_into.as_ref().and_then(|t| id_of.get(t).cloned()),
                    code_id,
                    token: c.code,
                    name,
                    label: c.label,
                    definition: c.definition,
                    includes,
                    excludes,
                    parent_id,
                    origin: c.origin,
                    toe: c.toe,
                    research_questions: c.research_questions,
                    active: c.active,
                    system: false,
                    in_prompt: c.in_prompt,
                    version: c.version,
                    example: c.example,
                    reason: c.reason,
                    origin_passage_id: origin_p,
                    example_passage_id: example_p,
                    created_at: c.created_at,
                    counts: CodeCounts::default(),
                },
            )
            .collect())
    }

    /// Every code in codebook order with aliases and counts, then the
    /// reserved code "Plus tard" last.
    pub fn code_nodes(&self) -> Result<Vec<CodeNode>> {
        self.code_nodes_filtered(None)
    }

    /// Like [`Self::code_nodes`], counting only the passages of the given
    /// interviews (a filter by attribute value, for instance).
    pub fn code_nodes_filtered(
        &self,
        interviews: Option<&BTreeSet<String>>,
    ) -> Result<Vec<CodeNode>> {
        let mut nodes = self.raw_nodes()?;
        let counts = self.code_counts(&nodes, interviews)?;
        for n in &mut nodes {
            n.counts = counts.get(&n.code_id).cloned().unwrap_or_default();
        }
        nodes.push(later_node());
        Ok(nodes)
    }

    pub fn code_tree(&self) -> Result<CodeTree> {
        Ok(CodeTree {
            codebook_version: self.codebook_version()?,
            codes: self.code_nodes()?,
        })
    }

    /// One code by stable id or exact token (merges are not followed).
    pub fn code_node(&self, key: &str) -> Result<CodeNode> {
        if fold(key) == fold(LATER_ID) {
            return Ok(later_node());
        }
        let row = self.find_exact(key)?;
        self.code_nodes()?
            .into_iter()
            .find(|n| n.code_id == row.code_id)
            .ok_or_else(|| not_found(format!("code {key}")))
    }

    /// Passages, interviews, cases and attribute values per code, direct and
    /// rolled up over the descendants (see `codestats.rs`).
    fn code_counts(
        &self,
        nodes: &[CodeNode],
        interviews: Option<&BTreeSet<String>>,
    ) -> Result<BTreeMap<String, CodeCounts>> {
        self.code_counts_v2(nodes, interviews)
    }

    // --------------------------------------------------------------- edits

    /// Edits a code by its stable id. A new name keeps the old one as an
    /// alias. The version of the code and the codebook version advance.
    pub fn update_code(&self, key: &str, patch: &CodeUpdate, actor: &str) -> Result<CodeNode> {
        if fold(key) == fold(LATER_ID) {
            return Err(validation(
                "the reserved code \"Plus tard\" cannot be edited",
            ));
        }
        let row = self.find_exact(key)?;
        if row.merged_into.is_some() {
            return Err(validation(format!(
                "code {} was merged and cannot be edited",
                row.code_id
            )));
        }
        let before = self.code_node(&row.code_id)?;
        let mut after = before.clone();
        let mut new_alias: Option<String> = None;
        if let Some(raw) = &patch.name {
            let name = check_display_name(raw)?;
            if name != before.name {
                let f = fold(&name);
                let taken = rows(&self.conn)?.iter().any(|r| {
                    r.code_id != row.code_id && (fold(&r.name) == f || fold(&r.token) == f)
                }) || aliases(&self.conn)?
                    .iter()
                    .any(|(id, a)| id != &row.code_id && fold(&a.alias) == f);
                if taken {
                    return Err(validation(format!("the name {name} is already used")));
                }
                new_alias = Some(before.name.clone());
                after.name = name;
            }
        }
        if let Some(l) = &patch.label {
            after.label = l.trim().to_string();
        }
        if let Some(d) = &patch.definition {
            if d.trim().is_empty() {
                return Err(validation("a code needs a definition"));
            }
            after.definition = d.trim().to_string();
        }
        if let Some(v) = &patch.includes {
            after.includes = v.trim().to_string();
        }
        if let Some(v) = &patch.excludes {
            after.excludes = v.trim().to_string();
        }
        if let Some(t) = &patch.toe {
            after.toe = crate::codebook::check_toe(t)?;
        }
        if let Some(q) = &patch.research_questions {
            after.research_questions = self.check_questions(q)?;
        }
        if let Some(e) = &patch.example {
            after.example = clean_opt(e);
        }
        if let Some(r) = &patch.reason {
            after.reason = clean_opt(r);
        }
        if let Some(p) = patch.in_prompt {
            after.in_prompt = p;
        }
        let unchanged = after.name == before.name
            && after.label == before.label
            && after.definition == before.definition
            && after.includes == before.includes
            && after.excludes == before.excludes
            && after.toe == before.toe
            && after.research_questions == before.research_questions
            && after.example == before.example
            && after.reason == before.reason
            && after.in_prompt == before.in_prompt;
        if unchanged {
            return Ok(before);
        }
        after.version = before.version + 1;
        let renamed = after.name != before.name;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE codes SET name = ?2, label = ?3, definition = ?4, includes = ?5, excludes = ?6,
               toe = ?7, family = ?8, research_questions = ?9, example = ?10, reason = ?11,
               in_prompt = ?12, version = ?13, local = 1
             WHERE code_id = ?1",
            params![
                row.code_id,
                after.name,
                after.label,
                after.definition,
                after.includes,
                after.excludes,
                after.toe,
                cs_model::family_for_toe(after.toe.as_deref()),
                crate::to_json(&after.research_questions),
                after.example,
                after.reason,
                after.in_prompt as i64,
                after.version
            ],
        )?;
        if let Some(old) = &new_alias {
            tx.execute(
                "INSERT OR IGNORE INTO code_aliases (alias, code_id, reason, created_at)
                 VALUES (?1, ?2, 'former_name', ?3)",
                params![old, row.code_id, now()],
            )?;
        }
        let action = if renamed {
            "codebook.code.rename"
        } else {
            "codebook.code.edit"
        };
        bump_revision(
            &tx,
            actor,
            if renamed { "rename" } else { "edit" },
            Some(&row.code_id),
        )?;
        append_event(
            &tx,
            actor,
            action,
            None,
            Some(&row.code_id),
            json!({
                "code_id": row.code_id,
                "token": row.token,
                "before": strip(&before),
                "after": strip(&after),
                "alias_added": new_alias,
            }),
        )?;
        let redefined = after.definition != before.definition
            || after.includes != before.includes
            || after.excludes != before.excludes;
        if redefined {
            self.open_harmonisation_for_code_in(
                &tx,
                &row.code_id,
                "redefine",
                "définition, inclusions ou exclusions modifiées",
                actor,
            )?;
        }
        tx.commit()?;
        self.code_node(&row.code_id)
    }

    /// Adds a synonym to a code. The alias must not be used by any code.
    pub fn add_alias(&self, key: &str, alias: &str, actor: &str) -> Result<CodeNode> {
        let row = self.find_exact(key)?;
        let alias = check_display_name(alias)?;
        let f = fold(&alias);
        let taken = rows(&self.conn)?
            .iter()
            .any(|r| fold(&r.name) == f || fold(&r.token) == f)
            || aliases(&self.conn)?
                .iter()
                .any(|(_, a)| fold(&a.alias) == f);
        if taken {
            return Err(validation(format!("{alias} is already a name or an alias")));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO code_aliases (alias, code_id, reason, created_at)
             VALUES (?1, ?2, 'synonym', ?3)",
            params![alias, row.code_id, now()],
        )?;
        bump_revision(&tx, actor, "alias", Some(&row.code_id))?;
        append_event(
            &tx,
            actor,
            "codebook.code.alias_add",
            None,
            Some(&row.code_id),
            json!({"code_id": row.code_id, "alias": alias}),
        )?;
        tx.commit()?;
        self.code_node(&row.code_id)
    }

    /// Removes a synonym. Former names, merged names and the legacy token
    /// stay: they keep old references resolvable.
    pub fn remove_alias(&self, key: &str, alias: &str, actor: &str) -> Result<CodeNode> {
        let row = self.find_exact(key)?;
        let f = fold(alias);
        let found: Option<(String, String)> = aliases(&self.conn)?
            .into_iter()
            .find(|(id, a)| id == &row.code_id && fold(&a.alias) == f)
            .map(|(_, a)| (a.alias, a.reason));
        let (exact, reason) =
            found.ok_or_else(|| not_found(format!("alias {alias} of {}", row.code_id)))?;
        if reason != "synonym" {
            return Err(validation(format!(
                "{exact} is a {reason} and stays as an alias; only synonyms can be removed"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM code_aliases WHERE alias = ?1", [&exact])?;
        bump_revision(&tx, actor, "alias", Some(&row.code_id))?;
        append_event(
            &tx,
            actor,
            "codebook.code.alias_remove",
            None,
            Some(&row.code_id),
            json!({"code_id": row.code_id, "alias": exact}),
        )?;
        tx.commit()?;
        self.code_node(&row.code_id)
    }

    /// Moves a code under a parent, or to the root with `None`. No coding
    /// changes. Refused when the move would make a cycle, when the parent is
    /// unknown, merged or the reserved code, or without a rationale.
    pub fn move_code(
        &self,
        key: &str,
        parent: Option<&str>,
        rationale: &str,
        actor: &str,
    ) -> Result<CodeNode> {
        if fold(key) == fold(LATER_ID) {
            return Err(validation(
                "the reserved code \"Plus tard\" cannot be moved",
            ));
        }
        let row = self.find_exact(key)?;
        if row.merged_into.is_some() {
            return Err(validation(format!(
                "code {} was merged and cannot be moved",
                row.code_id
            )));
        }
        let rationale = rationale.trim();
        if rationale.is_empty() {
            return Err(validation("a move needs a rationale"));
        }
        let all = rows(&self.conn)?;
        let parent_id = match parent.map(str::trim).filter(|p| !p.is_empty()) {
            None => None,
            Some(p) => {
                if fold(p) == fold(LATER_ID) {
                    return Err(validation(
                        "the reserved code \"Plus tard\" cannot be a parent",
                    ));
                }
                let pr = all
                    .iter()
                    .find(|r| r.code_id == p || r.token == p)
                    .ok_or_else(|| validation(format!("unknown parent code {p}")))?;
                if pr.merged_into.is_some() {
                    return Err(validation(format!(
                        "code {} was merged and cannot be a parent",
                        pr.code_id
                    )));
                }
                // Walk up from the parent: meeting the moved code is a cycle.
                let mut cur = Some(pr.code_id.clone());
                let mut guard = 0;
                while let Some(id) = cur {
                    if id == row.code_id {
                        return Err(validation(format!(
                            "{} cannot go under {}: that would make a cycle",
                            row.code_id, pr.code_id
                        )));
                    }
                    cur = all
                        .iter()
                        .find(|r| r.code_id == id)
                        .and_then(|r| r.parent_id.clone());
                    guard += 1;
                    if guard > all.len() + 1 {
                        break;
                    }
                }
                Some(pr.code_id.clone())
            }
        };
        if parent_id == row.parent_id {
            return self.code_node(&row.code_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE codes SET parent_id = ?2, local = 1 WHERE code_id = ?1",
            params![row.code_id, parent_id],
        )?;
        bump_revision(&tx, actor, "move", Some(&row.code_id))?;
        append_event(
            &tx,
            actor,
            "codebook.code.move",
            None,
            Some(&row.code_id),
            json!({
                "code_id": row.code_id,
                "parent_before": row.parent_id,
                "parent_after": parent_id,
                "rationale": rationale,
            }),
        )?;
        tx.commit()?;
        self.code_node(&row.code_id)
    }

    /// Identity side of a merge, inside the caller's transaction: children of
    /// the merged code go to the target, the names and aliases of the merged
    /// code point to the target, passage codings move to the target (a passage
    /// already carrying the target keeps one coding; the other is removed with
    /// the reason `merged`). Returns (moved, deduplicated) codings.
    pub(crate) fn merge_identity(
        &self,
        conn: &Connection,
        from_token: &str,
        into_token: &str,
        actor: &str,
    ) -> Result<(usize, usize)> {
        let get = |token: &str| -> Result<(String, String, Option<String>)> {
            Ok(conn.query_row(
                "SELECT code_id, COALESCE(name, code), parent_id FROM codes WHERE code = ?1",
                [token],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        };
        let (from_id, from_name, from_parent) = get(from_token)?;
        let (into_id, _, into_parent) = get(into_token)?;
        let ts = now();
        // The target itself sat under the merged code: it takes the merged
        // code's place, so that no code becomes its own parent.
        if into_parent.as_deref() == Some(from_id.as_str()) {
            conn.execute(
                "UPDATE codes SET parent_id = ?2 WHERE code_id = ?1",
                params![into_id, from_parent],
            )?;
        }
        conn.execute(
            "UPDATE codes SET parent_id = ?2 WHERE parent_id = ?1 AND code_id <> ?2",
            params![from_id, into_id],
        )?;
        conn.execute(
            "UPDATE codes SET parent_id = NULL WHERE code_id = ?1",
            [&from_id],
        )?;
        conn.execute(
            "UPDATE code_aliases SET code_id = ?2, reason = 'merge' WHERE code_id = ?1",
            params![from_id, into_id],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO code_aliases (alias, code_id, reason, created_at)
             VALUES (?1, ?2, 'merge', ?3)",
            params![from_name, into_id, ts],
        )?;
        // Passage codings.
        let moving: Vec<(i64, String)> = {
            let mut stmt = conn.prepare(
                "SELECT coding_id, passage_id FROM passage_codings
                 WHERE code_id = ?1 AND removed_at IS NULL ORDER BY coding_id",
            )?;
            let v = stmt
                .query_map([&from_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        let (mut moved, mut dedup) = (0usize, 0usize);
        let touched: Vec<String> = moving.iter().map(|(_, p)| p.clone()).collect();
        for (coding_id, passage_id) in moving {
            let has_target: i64 = conn.query_row(
                "SELECT COUNT(*) FROM passage_codings
                 WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                params![passage_id, into_id],
                |r| r.get(0),
            )?;
            if has_target > 0 {
                conn.execute(
                    "UPDATE passage_codings SET removed_at = ?2, removed_by = ?3, remove_reason = 'merged'
                     WHERE coding_id = ?1",
                    params![coding_id, ts, actor],
                )?;
                dedup += 1;
            } else {
                conn.execute(
                    "UPDATE passage_codings SET code_id = ?2 WHERE coding_id = ?1",
                    params![coding_id, into_id],
                )?;
                moved += 1;
            }
        }
        for passage_id in touched {
            self.sync_passage_unit_in(conn, &passage_id, actor)?;
        }
        Ok((moved, dedup))
    }

    /// Creates a code from a passage and codes the passage with it, in one
    /// transaction: the passage is the origin and the example of the code, the
    /// codebook version advances, one audit event is written. Nothing is kept
    /// when any part is refused.
    pub fn create_code_from_passage(
        &self,
        req: &NewCodeFromPassage,
        actor: &str,
    ) -> Result<(CodeNode, cs_model::passage::Passage)> {
        let (name, definition, parent_id, questions) = self.validate_new_code(
            &req.name,
            &req.definition,
            req.parent_id.as_deref(),
            &req.research_questions,
        )?;
        let definition = definition.as_str();
        if req.passage_id.is_none() && req.passage.is_none() {
            return Err(validation(
                "give the passage to code: an existing passage_id or a selection",
            ));
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut passage_created = false;
        let passage_id = match (&req.passage_id, &req.passage) {
            (Some(id), _) => {
                self.passage(id)?;
                id.clone()
            }
            (None, Some(sel)) => {
                let (id, created) = self.create_passage_in(&tx, sel, actor)?;
                passage_created = created;
                id
            }
            (None, None) => unreachable!("checked above"),
        };

        let passage = self.passage(&passage_id)?;
        let code_id = Self::next_code_id(&tx)?;
        let ord = tx
            .query_row(
                "SELECT COALESCE(MAX(ord), 0) FROM codes WHERE local = 1",
                [],
                |r| r.get::<_, i64>(0),
            )?
            .max(100_000)
            + 1;
        tx.execute(
            "INSERT INTO codes (code, family, label, definition, origin, toe, active, in_prompt,
               research_questions, version, ord, local, example, reason, created_at,
               code_id, name, parent_id, includes, excludes, origin_passage_id, example_passage_id)
             VALUES (?1, 'OTHER', '', ?2, 'field', NULL, 1, 1, ?3, 1, ?4, 1, ?5, ?6, ?7,
               ?1, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                code_id,
                definition,
                crate::to_json(&questions),
                ord,
                passage.text,
                clean_opt(&req.reason),
                now(),
                name,
                parent_id,
                req.includes.trim(),
                req.excludes.trim(),
                passage_id
            ],
        )?;
        self.add_coding_in(&tx, &passage_id, &code_id, "", actor)?;
        let rev = bump_revision(&tx, actor, "create_from_passage", Some(&code_id))?;
        append_event(
            &tx,
            actor,
            "codebook.code.create_from_passage",
            Some(&passage.interview_id),
            Some(&code_id),
            json!({
                "code_id": code_id,
                "name": name,
                "definition": definition,
                "includes": req.includes.trim(),
                "excludes": req.excludes.trim(),
                "parent_id": parent_id,
                "passage_id": passage_id,
                "unit_id": passage.unit_id,
                "passage_created": passage_created,
                "codebook_version": rev,
            }),
        )?;
        tx.commit()?;
        Ok((self.code_node(&code_id)?, self.passage(&passage_id)?))
    }

    /// The passages carrying a code (or one of its descendants), each with
    /// the speaker, the previous and next turn and the other codes present,
    /// in interview and unit order. Deleted passages and the reserved code
    /// are not part of it.
    pub fn passages_of_code(
        &self,
        key: &str,
        include_descendants: bool,
    ) -> Result<Vec<cs_model::passage::PassageContext>> {
        use cs_model::passage::{PassageContext, UnitBrief};
        let resolved = self.resolve_code(key)?;
        let mut wanted: BTreeSet<String> = BTreeSet::from([resolved.code_id.clone()]);
        if include_descendants && !resolved.system {
            let all = rows(&self.conn)?;
            let mut grew = true;
            while grew {
                grew = false;
                for r in &all {
                    if r.parent_id.as_ref().is_some_and(|p| wanted.contains(p))
                        && wanted.insert(r.code_id.clone())
                    {
                        grew = true;
                    }
                }
            }
        }
        let mut out = Vec::new();
        let mut segs: BTreeMap<String, Vec<cs_model::Unit>> = BTreeMap::new();
        for p in self.passages(&crate::passages::PassageFilter::default())? {
            if !p.codings.iter().any(|c| wanted.contains(&c.code_id)) {
                continue;
            }
            if !segs.contains_key(&p.interview_id) {
                let units = self
                    .segmentation(&p.interview_id)?
                    .map(|s| s.units)
                    .unwrap_or_default();
                segs.insert(p.interview_id.clone(), units);
            }
            let units = &segs[&p.interview_id];
            let idx = units.iter().position(|u| u.unit_id == p.unit_id);
            let brief = |i: usize| UnitBrief {
                unit_id: units[i].unit_id.clone(),
                speaker: units[i].speaker_raw.clone(),
                text: units[i].text.clone(),
            };
            out.push(PassageContext {
                speaker: idx
                    .map(|i| units[i].speaker_raw.clone())
                    .unwrap_or_default(),
                previous: idx.filter(|i| *i > 0).map(|i| brief(i - 1)),
                next: idx.filter(|i| i + 1 < units.len()).map(|i| brief(i + 1)),
                other_codes: p
                    .codings
                    .iter()
                    .filter(|c| !wanted.contains(&c.code_id))
                    .map(|c| c.code_name.clone())
                    .collect(),
                passage: p,
            });
        }
        Ok(out)
    }

    /// Next free stable id.
    pub(crate) fn next_code_id(conn: &Connection) -> Result<String> {
        let max: i64 = conn.query_row(
            "SELECT COALESCE(MAX(CAST(substr(code_id, 3) AS INTEGER)), 0) FROM codes
             WHERE code_id GLOB 'C-[0-9]*'",
            [],
            |r| r.get(0),
        )?;
        // A stable id is never reused: the ids of codes removed by an undo are
        // still in the audit log.
        let gone: i64 = conn.query_row(
            "SELECT COALESCE(MAX(CAST(substr(target, 3) AS INTEGER)), 0) FROM audit_log
             WHERE action = 'codebook.code.undo_create_from_passage' AND target GLOB 'C-[0-9]*'",
            [],
            |r| r.get(0),
        )?;
        Ok(format!("C-{:04}", max.max(gone) + 1))
    }
}

/// The audit form of a node: no counts, which are derived.
fn strip(n: &CodeNode) -> serde_json::Value {
    let mut v = serde_json::to_value(n).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("counts");
    }
    v
}

/// A code made without a passage: the codebook entry alone.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewCode {
    pub name: String,
    pub definition: String,
    #[serde(default)]
    pub includes: String,
    #[serde(default)]
    pub excludes: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub research_questions: Vec<String>,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// What an undo of "create a code from a passage" did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UndoCreateOutcome {
    pub code_id: String,
    pub passage_id: String,
    pub coding_removed: bool,
    pub code_deleted: bool,
    /// Why the code stays, when it does.
    pub code_kept_because: Vec<String>,
    pub passage_deleted: bool,
}

/// One version of the memo of a code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CodeMemoVersion {
    pub memo_id: i64,
    pub code_id: String,
    pub text: String,
    pub at: String,
    pub by: String,
}

impl Store {
    /// Name, definition, parent and questions of a new code, checked.
    fn validate_new_code(
        &self,
        name: &str,
        definition: &str,
        parent: Option<&str>,
        questions: &[String],
    ) -> Result<(String, String, Option<String>, Vec<String>)> {
        let name = check_display_name(name)?;
        let definition = definition.trim();
        if definition.is_empty() {
            return Err(validation("a code needs a definition"));
        }
        let f = fold(&name);
        let taken = rows(&self.conn)?
            .iter()
            .any(|r| fold(&r.name) == f || fold(&r.token) == f)
            || aliases(&self.conn)?
                .iter()
                .any(|(_, a)| fold(&a.alias) == f);
        if taken {
            return Err(validation(format!("the name {name} is already used")));
        }
        let parent_id = match parent.map(str::trim).filter(|p| !p.is_empty()) {
            None => None,
            Some(p) => {
                let row = rows(&self.conn)?
                    .into_iter()
                    .find(|r| r.code_id == p || r.token == p)
                    .ok_or_else(|| validation(format!("unknown parent code {p}")))?;
                if row.merged_into.is_some() {
                    return Err(validation(format!(
                        "code {} was merged and cannot be a parent",
                        row.code_id
                    )));
                }
                Some(row.code_id)
            }
        };
        let questions = self.check_questions(questions)?;
        Ok((name, definition.to_string(), parent_id, questions))
    }

    /// Latest memo of every code that has one.
    pub(crate) fn current_code_memos(&self) -> Result<BTreeMap<String, String>> {
        let mut stmt = self.conn.prepare(
            "SELECT code_id, text FROM code_memo_history
             WHERE memo_id IN (SELECT MAX(memo_id) FROM code_memo_history GROUP BY code_id)",
        )?;
        let out = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Creates a code in the codebook without any passage. The name is the
    /// given name; the stable id is the only key.
    pub fn create_code_v1(&self, req: &NewCode, actor: &str) -> Result<CodeNode> {
        let tx = self.conn.unchecked_transaction()?;
        let code_id = self.create_code_v1_in(&tx, req, actor)?;
        tx.commit()?;
        self.code_node(&code_id)
    }

    /// [`Self::create_code_v1`] inside the caller's transaction; returns the id.
    pub(crate) fn create_code_v1_in(
        &self,
        tx: &Connection,
        req: &NewCode,
        actor: &str,
    ) -> Result<String> {
        let (name, definition, parent_id, questions) = self.validate_new_code(
            &req.name,
            &req.definition,
            req.parent_id.as_deref(),
            &req.research_questions,
        )?;
        let memo = req.memo.trim().to_string();
        let code_id = Self::next_code_id(tx)?;
        let ord = tx
            .query_row(
                "SELECT COALESCE(MAX(ord), 0) FROM codes WHERE local = 1",
                [],
                |r| r.get::<_, i64>(0),
            )?
            .max(100_000)
            + 1;
        tx.execute(
            "INSERT INTO codes (code, family, label, definition, origin, toe, active, in_prompt,
               research_questions, version, ord, local, example, reason, created_at,
               code_id, name, parent_id, includes, excludes, origin_passage_id, example_passage_id)
             VALUES (?1, 'OTHER', '', ?2, 'field', NULL, 1, 1, ?3, 1, ?4, 1, NULL, ?5, ?6,
               ?1, ?7, ?8, ?9, ?10, NULL, NULL)",
            params![
                code_id,
                definition,
                crate::to_json(&questions),
                ord,
                clean_opt(&req.reason),
                now(),
                name,
                parent_id,
                req.includes.trim(),
                req.excludes.trim(),
            ],
        )?;
        if !memo.is_empty() {
            tx.execute(
                "INSERT INTO code_memo_history (code_id, text, at, by) VALUES (?1, ?2, ?3, ?4)",
                params![code_id, memo, now(), actor],
            )?;
        }
        let rev = bump_revision(tx, actor, "create", Some(&code_id))?;
        append_event(
            tx,
            actor,
            "codebook.code.create",
            None,
            Some(&code_id),
            json!({
                "code_id": code_id,
                "name": name,
                "definition": definition,
                "includes": req.includes.trim(),
                "excludes": req.excludes.trim(),
                "parent_id": parent_id,
                "research_questions": questions,
                "memo": memo,
                "codebook_version": rev,
            }),
        )?;
        if let Some(parent) = parent_id.as_deref() {
            self.open_harmonisation_for_code_in(
                tx,
                parent,
                "new_subcode",
                &format!("nouveau sous-code {name}"),
                actor,
            )?;
        }
        Ok(code_id)
    }

    /// Sets the memo of a code. Each change is a new version in the history;
    /// the codebook version does not move (a memo is not a definition).
    pub fn set_code_memo(&self, key: &str, text: &str, actor: &str) -> Result<CodeNode> {
        let row = self.find_exact(key)?;
        let text = text.trim().to_string();
        if text.chars().count() > 20_000 {
            return Err(validation("a code memo is at most 20000 characters"));
        }
        let before = self
            .current_code_memos()?
            .remove(&row.code_id)
            .unwrap_or_default();
        if before == text {
            return self.code_node(&row.code_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO code_memo_history (code_id, text, at, by) VALUES (?1, ?2, ?3, ?4)",
            params![row.code_id, text, now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.code.memo",
            None,
            Some(&row.code_id),
            json!({"code_id": row.code_id, "before": before, "after": text}),
        )?;
        tx.commit()?;
        self.code_node(&row.code_id)
    }

    /// Every version of the memo of a code, oldest first.
    pub fn code_memo_history(&self, key: &str) -> Result<Vec<CodeMemoVersion>> {
        let row = self.find_exact(key)?;
        let mut stmt = self.conn.prepare(
            "SELECT memo_id, code_id, text, at, by FROM code_memo_history
             WHERE code_id = ?1 ORDER BY memo_id",
        )?;
        let out = stmt
            .query_map([&row.code_id], |r| {
                Ok(CodeMemoVersion {
                    memo_id: r.get(0)?,
                    code_id: r.get(1)?,
                    text: r.get(2)?,
                    at: r.get(3)?,
                    by: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Undoes "create a code from a passage": removes the coding the call made,
    /// the code when nothing else uses it, and the passage when that call made
    /// it and nothing else uses it. One transaction, one audit event.
    pub fn undo_create_code_from_passage(
        &self,
        key: &str,
        actor: &str,
    ) -> Result<UndoCreateOutcome> {
        let row = self.find_exact(key)?;
        let code_id = row.code_id.clone();
        let created = self
            .audit_events(None)?
            .into_iter()
            .find(|e| {
                e.action == "codebook.code.create_from_passage"
                    && e.target.as_deref() == Some(code_id.as_str())
            })
            .ok_or_else(|| validation(format!("code {code_id} was not created from a passage")))?;
        let passage_id = created.detail["passage_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let passage_made = created.detail["passage_created"].as_bool().unwrap_or(false);
        let coding: Option<i64> = self
            .conn
            .query_row(
                "SELECT coding_id FROM passage_codings
                 WHERE passage_id = ?1 AND code_id = ?2 AND removed_at IS NULL",
                params![passage_id, code_id],
                |r| r.get(0),
            )
            .ok();
        // Other uses of the code.
        let mut kept: Vec<String> = Vec::new();
        let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<i64> {
            Ok(self.conn.query_row(sql, p, |r| r.get(0))?)
        };
        if count(
            "SELECT COUNT(*) FROM passage_codings pc JOIN passages p USING (passage_id)
             WHERE pc.code_id = ?1 AND pc.removed_at IS NULL AND p.deleted_at IS NULL
               AND pc.passage_id <> ?2",
            &[&code_id, &passage_id],
        )? > 0
        {
            kept.push("coded_elsewhere".into());
        }
        if count(
            "SELECT COUNT(*) FROM codes WHERE parent_id = ?1",
            &[&code_id],
        )? > 0
        {
            kept.push("has_children".into());
        }
        if count(
            "SELECT COUNT(*) FROM codes WHERE merged_into = ?1",
            &[&row.token],
        )? > 0
        {
            kept.push("received_a_merge".into());
        }
        if count(
            "SELECT COUNT(*) FROM theme_codes tc JOIN themes t USING (theme_id)
             WHERE tc.code_id = ?1 AND t.deleted_at IS NULL",
            &[&code_id],
        )? > 0
        {
            kept.push("used_by_a_theme".into());
        }
        if count(
            "SELECT COUNT(*) FROM code_aliases WHERE code_id = ?1 AND reason <> 'legacy_token'",
            &[&code_id],
        )? > 0
        {
            kept.push("has_aliases".into());
        }
        if count(
            "SELECT COUNT(*) FROM to_clarify_resolutions WHERE code = ?1",
            &[&row.token],
        )? > 0
        {
            kept.push("used_in_resolutions".into());
        }
        let delete_code = kept.is_empty();
        let mut passage_deleted = false;
        let tx = self.conn.unchecked_transaction()?;
        if let Some(coding_id) = coding {
            tx.execute(
                "UPDATE passage_codings SET removed_at = ?2, removed_by = ?3,
                   remove_reason = 'undo create code'
                 WHERE coding_id = ?1",
                params![coding_id, now(), actor],
            )?;
        }
        if passage_made && !passage_id.is_empty() {
            let other: i64 = tx.query_row(
                "SELECT (SELECT COUNT(*) FROM passage_codings WHERE passage_id = ?1 AND removed_at IS NULL)
                      + (SELECT COUNT(*) FROM passage_links WHERE passage_id = ?1 AND removed_at IS NULL)",
                [&passage_id],
                |r| r.get(0),
            )?;
            if other == 0 {
                tx.execute(
                    "UPDATE passages SET deleted_at = ?2, deleted_by = ?3,
                       delete_reason = 'undo create code'
                     WHERE passage_id = ?1 AND deleted_at IS NULL",
                    params![passage_id, now(), actor],
                )?;
                passage_deleted = true;
            }
        }
        if !passage_id.is_empty() {
            self.sync_passage_unit_in(&tx, &passage_id, actor)?;
        }
        if delete_code {
            tx.execute("DELETE FROM code_aliases WHERE code_id = ?1", [&code_id])?;
            tx.execute("DELETE FROM codes WHERE code_id = ?1", [&code_id])?;
            bump_revision(&tx, actor, "undo_create", Some(&code_id))?;
        }
        append_event(
            &tx,
            actor,
            "codebook.code.undo_create_from_passage",
            None,
            Some(&code_id),
            json!({
                "code_id": code_id,
                "name": row.name,
                "passage_id": passage_id,
                "coding_removed": coding.is_some(),
                "code_deleted": delete_code,
                "code_kept_because": kept,
                "passage_deleted": passage_deleted,
            }),
        )?;
        tx.commit()?;
        Ok(UndoCreateOutcome {
            code_id,
            passage_id,
            coding_removed: coding.is_some(),
            code_deleted: delete_code,
            code_kept_because: kept,
            passage_deleted,
        })
    }
}
