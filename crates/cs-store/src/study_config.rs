//! Study configuration in the database, and codebook import from CSV and JSON.
//!
//! - The configuration is JSON in the `settings` table under
//!   `cs_model::config::STUDY_CONFIG_KEY`. Unset: the default.
//! - `set_study_config_with` validates, normalises, stores and audits
//!   `config.update` with detail `{before, after, force}`. No event when
//!   nothing changes. A change of the phase 1 category names is refused
//!   (Guard) once phase 1 codings exist, unless `force`.
//! - Imports are all or nothing and audited as `codebook.import` with detail
//!   `{format, source, sha256, n_codes, added, updated, absent, ...}`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use cs_model::config::STUDY_CONFIG_KEY;
use cs_model::phase::PHASE1_CODEBOOK_VERSION;
use cs_model::{
    family_for_toe, sha256_hex, Category, Code, Codebook, StudyConfig, PHASE_1, TOE_LETTERS,
};
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;

use crate::codebook::ImportReport;
use crate::{append_event, from_json, guard_err, now, to_json, validation, Store};

/// Origins a codebook file may declare (empty means `field`).
const IMPORT_ORIGINS: [&str; 3] = ["literature", "field", "legacy"];

impl Store {
    /// The stored configuration, or `StudyConfig::default()` when unset. The
    /// phase 1 categories are those in force (see `StudyConfig::normalised`).
    pub fn study_config(&self) -> Result<StudyConfig> {
        match self.setting(STUDY_CONFIG_KEY)? {
            Some(raw) => Ok(from_json::<StudyConfig>(&raw)?.normalised()),
            None => Ok(StudyConfig::default()),
        }
    }

    /// `set_study_config_with(cfg, false, actor)`.
    pub fn set_study_config(&self, cfg: &StudyConfig, actor: &str) -> Result<StudyConfig> {
        self.set_study_config_with(cfg, false, actor)
    }

    /// Validates, normalises, stores and audits the configuration; returns
    /// what was stored. `force` allows a change of the phase 1 category names
    /// after phase 1 coding exists and is recorded in the audit event.
    pub fn set_study_config_with(
        &self,
        cfg: &StudyConfig,
        force: bool,
        actor: &str,
    ) -> Result<StudyConfig> {
        cfg.validate()?;
        let stored = cfg.normalised();
        let before = self.study_config()?;
        if stored == before {
            return Ok(stored);
        }
        let names = |c: &StudyConfig| -> BTreeSet<String> {
            c.effective_categories()
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        if names(&stored) != names(&before) && !force && self.phase1_coding_exists()? {
            return Err(guard_err(
                "the phase 1 category names cannot change once phase 1 coding exists; \
                 pass force to change them anyway (the existing codings are kept)",
            ));
        }

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![STUDY_CONFIG_KEY, to_json(&stored), now()],
        )?;
        append_event(
            &tx,
            actor,
            "config.update",
            None,
            Some(STUDY_CONFIG_KEY),
            json!({
                "before": serde_json::to_value(&before)?,
                "after": serde_json::to_value(&stored)?,
                "force": force,
            }),
        )?;
        tx.commit()?;
        Ok(stored)
    }

    /// Whether any candidate phase 1 coding has been recorded. Automatic
    /// codings (see `simple`) do not count.
    pub(crate) fn phase1_coding_exists(&self) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM human_codings WHERE phase = ?1 AND COALESCE(modifier, '') <> ?2",
            rusqlite::params![PHASE_1, crate::simple::AUTO_MODIFIER],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// The phase 1 categories of the configuration as a `Codebook`
    /// (`origin: "phase1"`, `family` = category name, `toe` = attachment).
    /// Default: equal to `cs_model::phase::phase1_codebook()`.
    pub fn phase1_codebook(&self) -> Result<Codebook> {
        let codes = self
            .study_config()?
            .effective_categories()
            .into_iter()
            .map(|c: Category| Code {
                family: c.name.clone(),
                code: c.name,
                label: c.label,
                definition: c.definition,
                origin: "phase1".into(),
                toe: c.attachment,
                active: true,
                in_prompt: false,
                research_questions: Vec::new(),
                merged_into: None,
                version: 1,
                example: None,
                reason: None,
                created_at: None,
            })
            .collect();
        Ok(Codebook {
            source: PHASE1_CODEBOOK_VERSION.to_string(),
            codes,
        })
    }

    /// Imports a codebook from CSV. Columns are read by name, in any order.
    /// Required: `code`, `definition`. Optional: `family` (ignored, derived
    /// from `toe`), `label`, `origin` (literature | field | legacy, default
    /// field), `toe` (T, O, E or empty), `research_questions` (separated by
    /// `;` or `,`; absent questions are created), `version`, `example`,
    /// `reason`, and the columns written by `codebook_export` (`active`,
    /// `in_prompt`, `merged_into`, `created_at`). RFC 4180 quoting.
    /// All or nothing: any invalid row refuses the whole file.
    pub fn import_codebook_csv(&self, path: &Path, actor: &str) -> Result<Codebook> {
        Ok(self.import_codebook_csv_reported(path, actor)?.0)
    }

    /// Like [`Self::import_codebook_csv`], with the report of the import.
    pub fn import_codebook_csv_reported(
        &self,
        path: &Path,
        actor: &str,
    ) -> Result<(Codebook, ImportReport)> {
        let bytes = read_file(path)?;
        let sha = sha256_hex(&bytes);
        let text = String::from_utf8(bytes)
            .map_err(|_| validation(format!("{} is not valid UTF-8", path.display())))?;
        let rows = parse_csv(&text)?;
        let (header, data) = rows
            .split_first()
            .ok_or_else(|| validation(format!("{} has no header row", path.display())))?;
        let column: BTreeMap<String, usize> = header
            .iter()
            .enumerate()
            .map(|(i, h)| (h.trim().to_lowercase(), i))
            .collect();
        for required in ["code", "definition"] {
            if !column.contains_key(required) {
                return Err(validation(format!(
                    "missing required column {required} in {}",
                    path.display()
                )));
            }
        }
        let mut raws = Vec::new();
        for (k, row) in data.iter().enumerate() {
            let at = format!("row {}", k + 1);
            if row.len() > header.len() {
                return Err(validation(format!(
                    "{at} has {} fields, the header has {}",
                    row.len(),
                    header.len()
                )));
            }
            let cell = |name: &str| -> String {
                column
                    .get(name)
                    .and_then(|&i| row.get(i))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            };
            let flag = |name: &str| -> Result<Option<bool>> {
                match cell(name).to_lowercase().as_str() {
                    "" => Ok(None),
                    "1" | "true" | "yes" => Ok(Some(true)),
                    "0" | "false" | "no" => Ok(Some(false)),
                    other => Err(validation(format!(
                        "row {}: {name} must be 0 or 1, not {other}",
                        k + 1
                    ))),
                }
            };
            let version = match cell("version").as_str() {
                "" => None,
                v => Some(v.parse::<u32>().map_err(|_| {
                    validation(format!("{at}: version must be a whole number, not {v}"))
                })?),
            };
            raws.push((
                at.clone(),
                RawCode {
                    code: cell("code"),
                    label: cell("label"),
                    definition: cell("definition"),
                    origin: cell("origin"),
                    toe: cell("toe"),
                    research_questions: split_questions(&cell("research_questions")),
                    version,
                    active: flag("active")?,
                    in_prompt: flag("in_prompt")?,
                    merged_into: cell("merged_into"),
                    example: cell("example"),
                    reason: cell("reason"),
                    created_at: cell("created_at"),
                },
            ));
        }
        if raws.is_empty() {
            return Err(validation(format!("{} contains no codes", path.display())));
        }
        self.store_import(raws, "csv", path, &sha, actor)
    }

    /// Imports a codebook from the JSON form of `Codebook`
    /// (`{source, codes: Code[]}`), with the same checks as the CSV import.
    pub fn import_codebook_json(&self, path: &Path, actor: &str) -> Result<Codebook> {
        Ok(self.import_codebook_json_reported(path, actor)?.0)
    }

    /// Like [`Self::import_codebook_json`], with the report of the import.
    pub fn import_codebook_json_reported(
        &self,
        path: &Path,
        actor: &str,
    ) -> Result<(Codebook, ImportReport)> {
        let bytes = read_file(path)?;
        let sha = sha256_hex(&bytes);
        let text = String::from_utf8(bytes)
            .map_err(|_| validation(format!("{} is not valid UTF-8", path.display())))?;
        let book: JsonBook = serde_json::from_str(&text)
            .map_err(|e| validation(format!("invalid JSON codebook: {e}")))?;
        if book.codes.is_empty() {
            return Err(validation("the codebook has no codes"));
        }
        let raws = book
            .codes
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                (
                    format!("codes[{i}]"),
                    RawCode {
                        code: c.code.trim().to_string(),
                        label: c.label.trim().to_string(),
                        definition: c.definition.trim().to_string(),
                        origin: c.origin.trim().to_string(),
                        toe: c.toe.unwrap_or_default().trim().to_string(),
                        research_questions: c
                            .research_questions
                            .into_iter()
                            .map(|q| q.trim().to_string())
                            .filter(|q| !q.is_empty())
                            .collect(),
                        version: c.version,
                        active: c.active,
                        in_prompt: c.in_prompt,
                        merged_into: c.merged_into.unwrap_or_default(),
                        example: c.example.unwrap_or_default(),
                        reason: c.reason.unwrap_or_default(),
                        created_at: c.created_at.unwrap_or_default(),
                    },
                )
            })
            .collect();
        self.store_import(raws, "json", path, &sha, actor)
    }

    /// Validates every row, then writes the whole file in one transaction:
    /// codes of the file are created or replaced, codes of the store absent
    /// from the file are kept (never deleted or deactivated), research
    /// questions named by the file are created when absent.
    fn store_import(
        &self,
        raws: Vec<(String, RawCode)>,
        format: &str,
        path: &Path,
        sha: &str,
        actor: &str,
    ) -> Result<(Codebook, ImportReport)> {
        let mut codes: Vec<Code> = Vec::new();
        let mut first_row: BTreeMap<String, String> = BTreeMap::new();
        for (at, raw) in raws {
            let code = raw.into_code(&at)?;
            if let Some(first) = first_row.get(&code.code) {
                return Err(validation(format!(
                    "duplicate code {} ({first} and {at})",
                    code.code
                )));
            }
            first_row.insert(code.code.clone(), at);
            codes.push(code);
        }

        let source = path.display().to_string();
        let known_rq: BTreeSet<String> = self
            .research_questions()?
            .into_iter()
            .map(|q| q.id)
            .collect();
        let existing: BTreeSet<String> =
            self.codebook()?.codes.into_iter().map(|c| c.code).collect();
        let in_file: BTreeSet<&str> = codes.iter().map(|c| c.code.as_str()).collect();
        let mut added = Vec::new();
        let mut updated = Vec::new();
        for c in &codes {
            if existing.contains(&c.code) {
                updated.push(c.code.clone());
            } else {
                added.push(c.code.clone());
            }
        }
        let absent: Vec<String> = existing
            .iter()
            .filter(|c| !in_file.contains(c.as_str()))
            .cloned()
            .collect();
        let mut new_rq: Vec<String> = Vec::new();
        for q in codes.iter().flat_map(|c| c.research_questions.iter()) {
            if !known_rq.contains(q) && !new_rq.contains(q) {
                new_rq.push(q.clone());
            }
        }

        let tx = self.conn.unchecked_transaction()?;
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
        for (ord, c) in codes.iter().enumerate() {
            tx.execute(
                "INSERT INTO codes (code, family, label, definition, origin, toe, active, in_prompt,
                   research_questions, merged_into, version, example, reason, created_at, ord, local)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 1)
                 ON CONFLICT(code) DO UPDATE SET family = excluded.family, label = excluded.label,
                   definition = excluded.definition, origin = excluded.origin, toe = excluded.toe,
                   active = excluded.active, in_prompt = excluded.in_prompt,
                   research_questions = excluded.research_questions, merged_into = excluded.merged_into,
                   version = excluded.version, example = excluded.example, reason = excluded.reason,
                   created_at = excluded.created_at, ord = excluded.ord, local = 1",
                params![
                    c.code,
                    c.family,
                    c.label,
                    c.definition,
                    c.origin,
                    c.toe,
                    c.active as i64,
                    c.in_prompt as i64,
                    to_json(&c.research_questions),
                    c.merged_into,
                    c.version,
                    c.example,
                    c.reason,
                    c.created_at,
                    ord as i64,
                ],
            )?;
        }
        append_event(
            &tx,
            actor,
            "codebook.import",
            None,
            Some(&source),
            json!({
                "format": format,
                "source": source,
                "sha256": sha,
                "n_codes": codes.len(),
                "added": added,
                "updated": updated,
                "absent": absent,
                "research_questions_created": new_rq,
            }),
        )?;
        tx.commit()?;
        Ok((
            self.codebook()?,
            ImportReport {
                added,
                updated,
                absent,
            },
        ))
    }
}

/// A code as read from a file, before validation.
struct RawCode {
    code: String,
    label: String,
    definition: String,
    origin: String,
    toe: String,
    research_questions: Vec<String>,
    version: Option<u32>,
    active: Option<bool>,
    in_prompt: Option<bool>,
    merged_into: String,
    example: String,
    reason: String,
    created_at: String,
}

impl RawCode {
    fn into_code(self, at: &str) -> Result<Code> {
        check_import_name(&self.code, at)?;
        if self.definition.is_empty() {
            return Err(validation(format!("{at}: definition is empty")));
        }
        let origin = match self.origin.as_str() {
            "" => "field",
            o if IMPORT_ORIGINS.contains(&o) => o,
            other => {
                return Err(validation(format!(
                    "{at}: origin must be literature, field or legacy, not {other}"
                )))
            }
        };
        let toe = match self.toe.as_str() {
            "" => None,
            t if TOE_LETTERS.contains(&t) => Some(t.to_string()),
            other => {
                return Err(validation(format!(
                    "{at}: toe must be T, O, E or empty, not {other}"
                )))
            }
        };
        let version = self.version.unwrap_or(1);
        if version == 0 {
            return Err(validation(format!("{at}: version must be at least 1")));
        }
        let mut research_questions: Vec<String> = Vec::new();
        for q in self.research_questions {
            if !research_questions.contains(&q) {
                research_questions.push(q);
            }
        }
        let active = self.active.unwrap_or(true);
        let origin = origin.to_string();
        let in_prompt = self.in_prompt.unwrap_or(active && origin != "legacy");
        Ok(Code {
            family: family_for_toe(toe.as_deref()).to_string(),
            code: self.code,
            label: self.label,
            definition: self.definition,
            origin,
            toe,
            active,
            in_prompt,
            research_questions,
            merged_into: non_empty(self.merged_into),
            version,
            example: non_empty(self.example),
            reason: non_empty(self.reason),
            created_at: non_empty(self.created_at),
        })
    }
}

/// Code names of an imported file: the prompt schema pattern
/// `^[A-Z][A-Z0-9-]*$`, and not NA. Phase 1 category names are not refused
/// here: a codebook exported by the tool may carry legacy codes such as TECH.
fn check_import_name(code: &str, at: &str) -> Result<()> {
    let mut chars = code.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let rest_ok = chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-');
    if code == cs_model::NA {
        return Err(validation(format!(
            "{at}: NA is reserved and cannot be a code"
        )));
    }
    if !first_ok || !rest_ok {
        return Err(validation(format!(
            "{at}: code {code:?} must match ^[A-Z][A-Z0-9-]*$"
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct JsonBook {
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    codes: Vec<JsonCode>,
}

#[derive(Deserialize)]
struct JsonCode {
    code: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    definition: String,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    toe: Option<String>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    in_prompt: Option<bool>,
    #[serde(default)]
    research_questions: Vec<String>,
    #[serde(default)]
    merged_into: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    example: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))
}

/// Research question ids of a CSV cell, separated by `;` or `,`.
fn split_questions(cell: &str) -> Vec<String> {
    cell.split([';', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// RFC 4180 parser: quoted fields with doubled quotes and embedded line
/// breaks, LF or CRLF line ends. Rows made of one empty field are dropped.
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let end_row = |row: &mut Vec<String>, field: &mut String, rows: &mut Vec<Vec<String>>| {
        row.push(std::mem::take(field));
        if !(row.len() == 1 && row[0].is_empty()) {
            rows.push(std::mem::take(row));
        }
        row.clear();
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_row(&mut row, &mut field, &mut rows);
            }
            '\n' => end_row(&mut row, &mut field, &mut rows),
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err(validation("the CSV file ends inside a quoted field"));
    }
    if !(row.is_empty() && field.is_empty()) {
        end_row(&mut row, &mut field, &mut rows);
    }
    Ok(rows)
}

/// Trimmed text, `None` when empty.
fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}
