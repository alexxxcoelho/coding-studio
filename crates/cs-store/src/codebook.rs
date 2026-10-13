//! Codebook (concrete codes with TOE attachment and origin), research
//! questions, and the export of the codebook to the thesis coding.db format.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use cs_model::phase::is_phase2_code;
use cs_model::{
    family_for_toe, sha256_hex, Code, CodeSet, Codebook, HumanCoding, ResearchQuestion, Unit, NA,
    PHASE_2, TOE_LETTERS,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::{append_event, clean_opt, not_found, now, to_json, validation, Store};

/// Settings key holding the path the codebook was imported from.
const CODEBOOK_SOURCE_KEY: &str = "codebook_source";

/// `ord` given to codes created locally, so they sort after the imported codebook.
const LOCAL_ORD_BASE: i64 = 100_000;

pub(crate) const CODE_COLUMNS: &str = "code, family, label, definition, origin, toe, active, in_prompt, research_questions, merged_into, version, example, reason, created_at";

pub(crate) fn row_to_code(r: &rusqlite::Row<'_>) -> rusqlite::Result<Code> {
    let rq: String = r.get(8)?;
    Ok(Code {
        code: r.get(0)?,
        family: r.get(1)?,
        label: r.get(2)?,
        definition: r.get(3)?,
        origin: r.get(4)?,
        toe: r.get(5)?,
        active: r.get::<_, i64>(6)? != 0,
        in_prompt: r.get::<_, i64>(7)? != 0,
        research_questions: serde_json::from_str(&rq).unwrap_or_default(),
        merged_into: r.get(9)?,
        version: r.get(10)?,
        example: r.get(11)?,
        reason: r.get(12)?,
        created_at: r.get(13)?,
    })
}

/// Partial edit of a code. Omitted fields are unchanged; for `toe`, `example`
/// and `reason`, an explicit null clears the value.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodePatch {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub definition: Option<String>,
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

/// A field present in the JSON (even null) becomes `Some(value)`.
fn present<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// What a codebook import changed, by code id.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportReport {
    /// Codes new to the codebook.
    pub added: Vec<String>,
    /// Codes that existed and were rewritten from the file.
    pub updated: Vec<String>,
    /// Codes of the codebook the file does not contain.
    pub absent: Vec<String>,
}

/// Result of a merge.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MergeOutcome {
    pub rewritten_codings: usize,
    pub rewritten_decisions: usize,
}

/// TOE letter from a coding.db family.
fn toe_of_family(family: &str) -> Option<String> {
    match family {
        "TECH" => Some("T".into()),
        "ORG" => Some("O".into()),
        "ENV" => Some("E".into()),
        _ => None,
    }
}

/// Research question ids written in a coding.db `sub_question` cell.
fn split_questions(cell: &str) -> Vec<String> {
    cell.split([',', ';', ' ', '/'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn check_toe(toe: &Option<String>) -> Result<Option<String>> {
    match clean_opt(toe) {
        None => Ok(None),
        Some(t) if TOE_LETTERS.contains(&t.as_str()) => Ok(Some(t)),
        Some(t) => Err(validation(format!(
            "TOE attachment must be T, O, E or empty, not {t}"
        ))),
    }
}

/// Name of a phase 2 code: non-empty, no separators, not NA, and not the name
/// of a phase 1 category of the study (`phase1`).
pub(crate) fn check_code_name(name: &str, phase1: &[String]) -> Result<()> {
    if name.is_empty() {
        return Err(validation("code name is empty"));
    }
    if name == NA {
        return Err(validation("NA is reserved and cannot be a code"));
    }
    if name
        .chars()
        .any(|c| c.is_whitespace() || c == ';' || c == ',' || c == '"')
    {
        return Err(validation(
            "a code name cannot contain spaces, quotes, commas or semicolons",
        ));
    }
    if phase1.iter().any(|p| p == name) {
        return Err(validation(format!(
            "{name} is a phase 1 category and cannot be a phase 2 code"
        )));
    }
    Ok(())
}

fn sql_text(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn sql_opt(s: Option<&str>) -> String {
    s.map(sql_text).unwrap_or_else(|| "NULL".to_string())
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl Store {
    // ------------------------------------------------------------ codebook

    /// Reads `codes` from the thesis coding.db (opened read-only). Mapping:
    /// origin codebook becomes literature, inductive becomes field, legacy is
    /// kept, bookkeeping codes are skipped; the TOE attachment follows the
    /// family (TECH, ORG, ENV); `sub_question` becomes research questions,
    /// created when absent. Codes created or edited in Coding Studio are never
    /// overwritten; codes absent upstream are deactivated, never deleted.
    pub fn import_codebook(&self, coding_db: &Path, actor: &str) -> Result<Codebook> {
        Ok(self.import_codebook_reported(coding_db, actor)?.0)
    }

    /// Like [`Self::import_codebook`], and says which codes were added,
    /// updated (rewritten from the file) and absent (codebook codes the file
    /// lacks, deactivated or kept as they were).
    pub fn import_codebook_reported(
        &self,
        coding_db: &Path,
        actor: &str,
    ) -> Result<(Codebook, ImportReport)> {
        let bytes = std::fs::read(coding_db)
            .with_context(|| format!("cannot read {}", coding_db.display()))?;
        let file_sha = sha256_hex(&bytes);
        type Row = (String, String, String, String, String, bool, String);
        let upstream: Vec<Row> = {
            let src = Connection::open_with_flags(
                coding_db,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .with_context(|| format!("cannot open {} read-only", coding_db.display()))?;
            let has_sq: bool = src
                .prepare("SELECT name FROM pragma_table_info('codes')")
                .and_then(|mut s| {
                    let names: Vec<String> = s
                        .query_map([], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    Ok(names.iter().any(|n| n == "sub_question"))
                })
                .unwrap_or(false);
            let sql = format!(
                "SELECT code, family, label, definition, origin, active, {} FROM codes ORDER BY rowid",
                if has_sq { "sub_question" } else { "NULL" }
            );
            let mut stmt = src.prepare(&sql).map_err(|e| {
                validation(format!(
                    "{} has no usable codes table: {e}",
                    coding_db.display()
                ))
            })?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(4)?
                        .unwrap_or_else(|| "codebook".to_string()),
                    r.get::<_, Option<i64>>(5)?.unwrap_or(1) != 0,
                    r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                ))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let upstream: Vec<Row> = upstream
            .into_iter()
            .filter(|r| r.4 != "bookkeeping")
            .collect();
        if upstream.is_empty() {
            return Err(validation(format!(
                "{} contains no codes",
                coding_db.display()
            )));
        }

        // code -> (in_prompt, local, active)
        let existing: BTreeMap<String, (bool, bool, bool)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT code, in_prompt, local, active FROM codes")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, i64>(2)? != 0,
                        r.get::<_, i64>(3)? != 0,
                    ),
                ))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let known_rq: BTreeSet<String> = self
            .research_questions()?
            .into_iter()
            .map(|q| q.id)
            .collect();

        let tx = self.conn.unchecked_transaction()?;
        let mut added = Vec::new();
        let mut updated = Vec::new();
        let mut kept_local = Vec::new();
        let mut new_rq: Vec<String> = Vec::new();
        let upstream_codes: BTreeSet<&str> = upstream.iter().map(|u| u.0.as_str()).collect();
        for (i, (code, family, label, definition, origin, active, sq)) in
            upstream.iter().enumerate()
        {
            let origin = match origin.as_str() {
                "inductive" => "field",
                "legacy" => "legacy",
                _ => "literature",
            };
            let toe = toe_of_family(family);
            let rqs = split_questions(sq);
            for q in &rqs {
                if !known_rq.contains(q) && !new_rq.contains(q) {
                    new_rq.push(q.clone());
                }
            }
            let in_prompt = match existing.get(code) {
                Some((_, true, _)) => {
                    kept_local.push(code.clone());
                    continue;
                }
                Some((keep, false, _)) => {
                    updated.push(code.clone());
                    *keep
                }
                None => {
                    added.push(code.clone());
                    *active && origin != "legacy"
                }
            };
            tx.execute(
                "INSERT INTO codes (code, family, label, definition, origin, toe, active, in_prompt, research_questions, ord, local)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
                 ON CONFLICT(code) DO UPDATE SET family = excluded.family, label = excluded.label,
                   definition = excluded.definition, origin = excluded.origin, toe = excluded.toe,
                   active = excluded.active, in_prompt = excluded.in_prompt,
                   research_questions = excluded.research_questions, ord = excluded.ord",
                params![
                    code,
                    family_for_toe(toe.as_deref()),
                    label,
                    definition,
                    origin,
                    toe,
                    *active as i64,
                    in_prompt as i64,
                    to_json(&rqs),
                    i as i64
                ],
            )?;
        }
        let absent: Vec<String> = existing
            .keys()
            .filter(|c| !upstream_codes.contains(c.as_str()))
            .cloned()
            .collect();
        let mut deactivated = Vec::new();
        for (code, (_, local, active)) in &existing {
            if !local && *active && !upstream_codes.contains(code.as_str()) {
                tx.execute("UPDATE codes SET active = 0 WHERE code = ?1", [code])?;
                deactivated.push(code.clone());
            }
        }
        let base: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ord), 0) FROM research_questions",
            [],
            |r| r.get(0),
        )?;
        for (i, q) in new_rq.iter().enumerate() {
            tx.execute(
                "INSERT INTO research_questions (id, text, ord) VALUES (?1, '', ?2)",
                params![q, base + 1 + i as i64],
            )?;
        }
        let source = coding_db.display().to_string();
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![CODEBOOK_SOURCE_KEY, source, now()],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.import",
            None,
            Some(&source),
            json!({
                "source": source,
                "sha256": file_sha,
                "n_codes": upstream.len(),
                "added": added,
                "updated": updated,
                "absent": absent,
                "removed": deactivated,
                "kept_local": kept_local,
                "research_questions_created": new_rq,
            }),
        )?;
        tx.commit()?;
        let report = ImportReport {
            added,
            updated,
            absent,
        };
        Ok((self.codebook()?, report))
    }

    pub fn codebook(&self) -> Result<Codebook> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CODE_COLUMNS} FROM codes ORDER BY ord, code"
        ))?;
        let rows = stmt.query_map([], row_to_code)?;
        let codes = rows.collect::<rusqlite::Result<_>>()?;
        Ok(Codebook {
            source: self.setting(CODEBOOK_SOURCE_KEY)?.unwrap_or_default(),
            codes,
        })
    }

    pub(crate) fn code_row(&self, code: &str) -> Result<Option<Code>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {CODE_COLUMNS} FROM codes WHERE code = ?1"),
                [code],
                row_to_code,
            )
            .optional()?)
    }

    fn check_questions(&self, ids: &[String]) -> Result<Vec<String>> {
        let known: BTreeSet<String> = self
            .research_questions()?
            .into_iter()
            .map(|q| q.id)
            .collect();
        let mut out = Vec::new();
        for id in ids {
            let id = id.trim().to_string();
            if id.is_empty() || out.contains(&id) {
                continue;
            }
            if !known.contains(&id) {
                return Err(validation(format!("unknown research question {id}")));
            }
            out.push(id);
        }
        Ok(out)
    }

    /// Validates a new code and inserts it on `conn` with its audit event.
    pub(crate) fn insert_code(&self, conn: &Connection, code: &Code, actor: &str) -> Result<Code> {
        let name = code.code.trim();
        let phase1: Vec<String> = self
            .study_config()?
            .effective_categories()
            .into_iter()
            .map(|c| c.name)
            .collect();
        check_code_name(name, &phase1)?;
        if self.code_row(name)?.is_some() {
            return Err(validation(format!("code {name} already exists")));
        }
        if code.definition.trim().is_empty() {
            return Err(validation("a code needs a definition"));
        }
        let origin = match code.origin.trim() {
            "" | "field" => "field",
            "literature" => "literature",
            other => {
                return Err(validation(format!(
                    "origin must be literature or field, not {other}"
                )))
            }
        };
        let toe = check_toe(&code.toe)?;
        let max_ord: i64 = conn.query_row(
            "SELECT COALESCE(MAX(ord), 0) FROM codes WHERE local = 1",
            [],
            |r| r.get(0),
        )?;
        let ord = max_ord.max(LOCAL_ORD_BASE) + 1;
        let stored = Code {
            code: name.to_string(),
            family: family_for_toe(toe.as_deref()).to_string(),
            label: code.label.trim().to_string(),
            definition: code.definition.trim().to_string(),
            origin: origin.to_string(),
            toe,
            active: true,
            in_prompt: code.in_prompt,
            research_questions: self.check_questions(&code.research_questions)?,
            merged_into: None,
            version: 1,
            example: clean_opt(&code.example),
            reason: clean_opt(&code.reason),
            created_at: Some(now()),
        };
        conn.execute(
            "INSERT INTO codes (code, family, label, definition, origin, toe, active, in_prompt,
               research_questions, version, ord, local, example, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, 1, ?9, 1, ?10, ?11, ?12)",
            params![
                stored.code,
                stored.family,
                stored.label,
                stored.definition,
                stored.origin,
                stored.toe,
                stored.in_prompt as i64,
                to_json(&stored.research_questions),
                ord,
                stored.example,
                stored.reason,
                stored.created_at
            ],
        )?;
        append_event(
            conn,
            actor,
            "codebook.code.create",
            None,
            Some(&stored.code),
            json!({"code": stored}),
        )?;
        Ok(stored)
    }

    /// New phase 2 code (origin literature or field, default field). The
    /// definition is required; the TOE attachment is T, O, E or none.
    pub fn create_code(&self, code: &Code, actor: &str) -> Result<Code> {
        let tx = self.conn.unchecked_transaction()?;
        let stored = self.insert_code(&tx, code, actor)?;
        tx.commit()?;
        Ok(stored)
    }

    /// Edits a code; its version is incremented and the code is marked local
    /// (a later import from coding.db no longer overwrites it).
    pub fn edit_code(&self, code: &str, patch: &CodePatch, actor: &str) -> Result<Code> {
        let before = self
            .code_row(code)?
            .ok_or_else(|| not_found(format!("code {code}")))?;
        if before.merged_into.is_some() {
            return Err(validation(format!(
                "code {code} was merged and cannot be edited"
            )));
        }
        let mut after = before.clone();
        if let Some(l) = &patch.label {
            after.label = l.trim().to_string();
        }
        if let Some(d) = &patch.definition {
            if d.trim().is_empty() {
                return Err(validation("a code needs a definition"));
            }
            after.definition = d.trim().to_string();
        }
        if let Some(t) = &patch.toe {
            after.toe = check_toe(t)?;
            after.family = family_for_toe(after.toe.as_deref()).to_string();
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
        if after == before {
            return Ok(before);
        }
        after.version = before.version + 1;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE codes SET family = ?2, label = ?3, definition = ?4, toe = ?5, in_prompt = ?6,
               research_questions = ?7, example = ?8, reason = ?9, version = ?10, local = 1
             WHERE code = ?1",
            params![
                code,
                after.family,
                after.label,
                after.definition,
                after.toe,
                after.in_prompt as i64,
                to_json(&after.research_questions),
                after.example,
                after.reason,
                after.version
            ],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.code.edit",
            None,
            Some(code),
            json!({"before": before, "after": after}),
        )?;
        tx.commit()?;
        Ok(after)
    }

    /// Whether a code is offered to the AI coder. Logged; no event when unchanged.
    pub fn set_code_in_prompt(&self, code: &str, in_prompt: bool, actor: &str) -> Result<()> {
        let current = self
            .code_row(code)?
            .ok_or_else(|| not_found(format!("code {code}")))?;
        if current.in_prompt == in_prompt {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE codes SET in_prompt = ?2 WHERE code = ?1",
            params![code, in_prompt as i64],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.code.in_prompt",
            None,
            Some(code),
            json!({"before": current.in_prompt, "after": in_prompt}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Activates or deactivates a code. A merged code stays inactive.
    pub fn set_code_active(&self, code: &str, active: bool, actor: &str) -> Result<()> {
        let current = self
            .code_row(code)?
            .ok_or_else(|| not_found(format!("code {code}")))?;
        if current.active == active {
            return Ok(());
        }
        if active && current.merged_into.is_some() {
            return Err(validation(format!(
                "code {code} was merged and cannot be reactivated"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE codes SET active = ?2, local = 1 WHERE code = ?1",
            params![code, active as i64],
        )?;
        append_event(
            &tx,
            actor,
            "codebook.code.active",
            None,
            Some(code),
            json!({"before": current.active, "after": active}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Merges `from` into `into`: every phase 2 coding of the candidate
    /// carrying `from` is rewritten, every current review decision carrying it
    /// is superseded by a rewritten decision, and `from` becomes inactive with
    /// `merged_into`. AI answers are kept as received.
    pub fn merge_code(
        &self,
        from: &str,
        into: &str,
        rationale: &str,
        actor: &str,
    ) -> Result<MergeOutcome> {
        if from == into {
            return Err(validation("a code cannot be merged into itself"));
        }
        let src = self
            .code_row(from)?
            .ok_or_else(|| not_found(format!("code {from}")))?;
        let dst = self
            .code_row(into)?
            .ok_or_else(|| not_found(format!("code {into}")))?;
        if src.merged_into.is_some() {
            return Err(validation(format!("code {from} was already merged")));
        }
        if !is_phase2_code(&dst) {
            return Err(validation(format!(
                "cannot merge into {into}: it is inactive, merged or not a phase 2 code"
            )));
        }
        let rationale = rationale.trim();
        if rationale.is_empty() {
            return Err(validation("a merge needs a rationale"));
        }
        let rewrite = |codes: &CodeSet| -> CodeSet {
            codes
                .iter()
                .map(|c| {
                    if c == from {
                        into.to_string()
                    } else {
                        c.clone()
                    }
                })
                .collect()
        };
        // Codings of every interview, phase 2.
        let codings: Vec<(String, HumanCoding)> = {
            let mut stmt = self.conn.prepare(
                "SELECT interview_id, unit_id FROM human_codings WHERE phase = ?1
                 AND instr(codes, ?2) > 0 ORDER BY interview_id, unit_id",
            )?;
            let keys: Vec<(String, String)> = stmt
                .query_map(params![PHASE_2, format!("\"{from}\"")], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            let mut out = Vec::new();
            for (iid, uid) in keys {
                if let Some(h) = self.human_codings(&iid, PHASE_2)?.remove(&uid) {
                    if h.codes.contains(from) {
                        out.push((iid, h));
                    }
                }
            }
            out
        };
        let decisions: Vec<(String, cs_model::Arbitration)> = {
            let ids: Vec<String> = {
                let mut stmt = self
                    .conn
                    .prepare("SELECT DISTINCT interview_id FROM arbitrations")?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            let mut out = Vec::new();
            for iid in ids {
                for (_, d) in self.decisions(&iid)? {
                    if d.final_codes.contains(from) {
                        out.push((iid.clone(), d));
                    }
                }
            }
            out
        };
        let tx = self.conn.unchecked_transaction()?;
        for (iid, before) in &codings {
            let mut after = before.clone();
            after.codes = rewrite(&before.codes);
            tx.execute(
                "UPDATE human_codings SET codes = ?4, updated_at = ?5
                 WHERE interview_id = ?1 AND phase = ?2 AND unit_id = ?3",
                params![iid, PHASE_2, before.unit_id, to_json(&after.codes), now()],
            )?;
        }
        for (iid, d) in &decisions {
            tx.execute(
                "INSERT INTO arbitrations (interview_id, phase, unit_id, choice, final_codes, rationale, decided_by, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    iid,
                    PHASE_2,
                    d.unit_id,
                    to_json(&cs_model::ArbitrationChoice::Custom),
                    to_json(&rewrite(&d.final_codes)),
                    format!("Code {from} merged into {into}: {rationale}"),
                    actor,
                    now()
                ],
            )?;
        }
        tx.execute(
            "UPDATE codes SET merged_into = ?2, active = 0, local = 1, version = version + 1 WHERE code = ?1",
            params![from, into],
        )?;
        let outcome = MergeOutcome {
            rewritten_codings: codings.len(),
            rewritten_decisions: decisions.len(),
        };
        append_event(
            &tx,
            actor,
            "codebook.code.merge",
            None,
            Some(from),
            json!({
                "from": from,
                "into": into,
                "rationale": rationale,
                "rewritten_codings": codings.iter().map(|(i, h)| json!({"interview_id": i, "unit_id": h.unit_id})).collect::<Vec<_>>(),
                "rewritten_decisions": decisions.iter().map(|(i, d)| json!({"interview_id": i, "unit_id": d.unit_id})).collect::<Vec<_>>(),
                "n_codings": outcome.rewritten_codings,
                "n_decisions": outcome.rewritten_decisions,
            }),
        )?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Every unit, across interviews, whose phase 2 coding carries `code`.
    pub fn code_units(&self, code: &str) -> Result<Vec<(String, Unit, HumanCoding)>> {
        self.code_row(code)?
            .ok_or_else(|| not_found(format!("code {code}")))?;
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT DISTINCT interview_id FROM human_codings WHERE phase = ?1
                 AND instr(codes, ?2) > 0 ORDER BY interview_id",
            )?;
            let rows = stmt.query_map(params![PHASE_2, format!("\"{code}\"")], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut out = Vec::new();
        for iid in ids {
            let Some(seg) = self.segmentation(&iid)? else {
                continue;
            };
            let codings = self.human_codings(&iid, PHASE_2)?;
            for unit in seg.units {
                if let Some(h) = codings.get(&unit.unit_id) {
                    if h.codes.contains(code) {
                        out.push((iid.clone(), unit, h.clone()));
                    }
                }
            }
        }
        Ok(out)
    }

    /// The codebook as CSV and as SQL for the `codes` table of
    /// `05_Analysis/coding/schema.sql` (family from the TOE attachment or
    /// OTHER; origin field written as inductive, literature as codebook;
    /// `sub_question` = research question ids).
    pub fn codebook_export(&self) -> Result<(String, String)> {
        let book = self.codebook()?;
        let mut csv = String::from(
            "code,label,definition,toe,origin,active,in_prompt,research_questions,merged_into,version,example,reason,created_at\n",
        );
        let mut sql = String::from(
            "-- Codebook exported by Coding Studio. Apply to 05_Analysis/coding/coding.db.\nBEGIN;\n",
        );
        for c in &book.codes {
            let rq = c.research_questions.join(",");
            csv.push_str(
                &[
                    csv_field(&c.code),
                    csv_field(&c.label),
                    csv_field(&c.definition),
                    csv_field(c.toe.as_deref().unwrap_or("")),
                    csv_field(&c.origin),
                    (c.active as u8).to_string(),
                    (c.in_prompt as u8).to_string(),
                    csv_field(&rq),
                    csv_field(c.merged_into.as_deref().unwrap_or("")),
                    c.version.to_string(),
                    csv_field(c.example.as_deref().unwrap_or("")),
                    csv_field(c.reason.as_deref().unwrap_or("")),
                    csv_field(c.created_at.as_deref().unwrap_or("")),
                ]
                .join(","),
            );
            csv.push('\n');
            let origin = match c.origin.as_str() {
                "field" => "inductive",
                "literature" => "codebook",
                other => other,
            };
            let mut notes = Vec::new();
            if let Some(m) = &c.merged_into {
                notes.push(format!("merged into {m}"));
            }
            if let Some(r) = &c.reason {
                notes.push(r.clone());
            }
            let notes = notes.join("; ");
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO codes (code,family,label,definition,sub_question,cycle,origin,active,notes) VALUES ({},{},{},{},{},1,{},{},{});\n",
                sql_text(&c.code),
                sql_text(family_for_toe(c.toe.as_deref())),
                sql_text(&c.label),
                sql_text(&c.definition),
                sql_opt(Some(rq.as_str()).filter(|s| !s.is_empty())),
                sql_text(origin),
                c.active as u8,
                sql_opt(Some(notes.as_str()).filter(|s| !s.is_empty())),
            ));
        }
        sql.push_str("COMMIT;\n");
        Ok((csv, sql))
    }

    // ------------------------------------------------- research questions

    pub fn research_questions(&self) -> Result<Vec<ResearchQuestion>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, text, ord FROM research_questions ORDER BY ord, id")?;
        let rows = stmt.query_map([], |r| {
            Ok(ResearchQuestion {
                id: r.get(0)?,
                text: r.get(1)?,
                ord: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn create_research_question(&self, id: &str, text: &str, actor: &str) -> Result<()> {
        let id = id.trim();
        if id.is_empty()
            || id
                .chars()
                .any(|c| c.is_whitespace() || c == ',' || c == ';')
        {
            return Err(validation(
                "a research question id is required, without spaces, commas or semicolons",
            ));
        }
        if self.research_questions()?.iter().any(|q| q.id == id) {
            return Err(validation(format!("research question {id} already exists")));
        }
        let tx = self.conn.unchecked_transaction()?;
        let ord: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ord), 0) + 1 FROM research_questions",
            [],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO research_questions (id, text, ord) VALUES (?1, ?2, ?3)",
            params![id, text.trim(), ord],
        )?;
        append_event(
            &tx,
            actor,
            "research_question.create",
            None,
            Some(id),
            json!({"id": id, "text": text.trim(), "ord": ord}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_research_question(
        &self,
        id: &str,
        text: &str,
        ord: Option<u32>,
        actor: &str,
    ) -> Result<()> {
        let before = self
            .research_questions()?
            .into_iter()
            .find(|q| q.id == id)
            .ok_or_else(|| not_found(format!("research question {id}")))?;
        let after = ResearchQuestion {
            id: id.to_string(),
            text: text.trim().to_string(),
            ord: ord.unwrap_or(before.ord),
        };
        if after == before {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE research_questions SET text = ?2, ord = ?3 WHERE id = ?1",
            params![id, after.text, after.ord],
        )?;
        append_event(
            &tx,
            actor,
            "research_question.update",
            None,
            Some(id),
            json!({"before": before, "after": after}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Refused while a code maps to the question.
    pub fn delete_research_question(&self, id: &str, actor: &str) -> Result<()> {
        let before = self
            .research_questions()?
            .into_iter()
            .find(|q| q.id == id)
            .ok_or_else(|| not_found(format!("research question {id}")))?;
        let users: Vec<String> = self
            .codebook()?
            .codes
            .into_iter()
            .filter(|c| c.research_questions.iter().any(|q| q == id))
            .map(|c| c.code)
            .collect();
        if !users.is_empty() {
            return Err(crate::guard_err(format!(
                "research question {id} is still mapped to {}",
                users.join(", ")
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM research_questions WHERE id = ?1", [id])?;
        append_event(
            &tx,
            actor,
            "research_question.delete",
            None,
            Some(id),
            json!({"before": before}),
        )?;
        tx.commit()?;
        Ok(())
    }
}
