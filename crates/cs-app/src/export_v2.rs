//! F-102: anonymised academic export.
//!
//! The same packages as the normal export (interview, cross-case, write-up),
//! built in a private temporary folder and then rewritten with the project's
//! pseudonymisation list applied to every text: transcripts, passages,
//! quotations, memos, claims, the write-up. Interview ids and unit ids are
//! kept. The names of the researcher (the speaker labels of the interviewer
//! and the actor of the audit) are replaced by one configurable label. Left
//! out because they cannot be pseudonymised: the database snapshot and the
//! audit log. The README of the package states what was replaced and what was
//! not (free text is outside the reach of a list of names); the manifest
//! carries the digests of the new files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cs_model::pseudonym::{PseudonymEntry, Pseudonymiser, FREE_TEXT_NOTE};
use cs_model::{sha256_hex, CsError};
use cs_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::export::{csv_field, target_dir, ExportedFile, PackageOpts, Writer};

/// Default replacement of the researcher's names.
pub const DEFAULT_RESEARCHER_LABEL: &str = "Researcher";

/// Files that are never copied into an anonymised package.
const EXCLUDED: [(&str, &str); 2] = [
    (
        "db_snapshot.sqlite",
        "a database copy holds every text as typed and cannot be pseudonymised",
    ),
    (
        "audit_log.jsonl",
        "audit events hold the texts they changed (before and after)",
    ),
];

/// What to export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Interview(String),
    CrossCase,
    Writeup,
}

impl Scope {
    fn name(&self) -> String {
        match self {
            Scope::Interview(id) => id.clone(),
            Scope::CrossCase => "cross-case".into(),
            Scope::Writeup => "writeup".into(),
        }
    }
}

/// Options of the anonymised export.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnonymiseOptions {
    /// Names under which the researcher appears, replaced by the label. The
    /// interviewer labels of the study configuration and the actor of the
    /// session are always added.
    #[serde(default)]
    pub researcher_names: Vec<String>,
    /// Replacement of the researcher's names (default `Researcher`).
    #[serde(default)]
    pub researcher_label: Option<String>,
}

/// Replacements made, by kind of name. Never the names themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Replacements {
    pub person: usize,
    pub organisation: usize,
    pub researcher: usize,
}

#[derive(Debug, Serialize)]
pub struct AnonymisedExport {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub manifest_sha256: String,
    pub replacements: Replacements,
    /// Files of the normal package left out, with the reason.
    pub excluded: Vec<Value>,
    /// Digest of the pseudonymisation list applied.
    pub pseudonymisation_digest: String,
}

/// The masking rules of an export: the project list (whether or not the
/// prompt switch is on: asking for an anonymised export is the decision) and
/// the researcher's names.
struct Masker {
    rules: Pseudonymiser,
    kinds: Vec<String>,
    counts: std::cell::RefCell<Vec<usize>>,
}

impl Masker {
    fn new(store: &Store, opts: &AnonymiseOptions, actor: &str) -> Result<(Masker, String)> {
        let cfg = store.pseudonym_config()?;
        let label = opts
            .researcher_label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .unwrap_or(DEFAULT_RESEARCHER_LABEL)
            .to_string();
        let mut entries: Vec<PseudonymEntry> = cfg.entries.clone();
        let mut names: Vec<String> = opts.researcher_names.clone();
        names.extend(store.study_config()?.interviewers);
        names.push(actor.to_string());
        let mut seen: Vec<String> = Vec::new();
        for n in names {
            let n = n.trim().to_string();
            if n.is_empty() || seen.iter().any(|s| s.eq_ignore_ascii_case(&n)) {
                continue;
            }
            if entries.iter().any(|e| e.original.eq_ignore_ascii_case(&n)) {
                continue;
            }
            seen.push(n.clone());
            entries.push(PseudonymEntry {
                kind: "researcher".into(),
                original: n,
                pseudonym: label.clone(),
            });
        }
        let rules = Pseudonymiser::new(&entries);
        let kinds: Vec<String> = rules
            .originals()
            .iter()
            .map(|o| {
                entries
                    .iter()
                    .find(|e| e.original == *o)
                    .map(|e| e.kind.clone())
                    .unwrap_or_default()
            })
            .collect();
        Ok((
            Masker {
                rules,
                kinds,
                counts: std::cell::RefCell::new(Vec::new()),
            },
            cfg.digest(),
        ))
    }

    fn mask(&self, text: &str) -> String {
        self.rules.mask_counted(text, &mut self.counts.borrow_mut())
    }

    fn residual(&self, text: &str) -> bool {
        self.rules.contains_original(text)
    }

    fn replacements(&self) -> Replacements {
        let counts = self.counts.borrow();
        let mut r = Replacements::default();
        for (i, kind) in self.kinds.iter().enumerate() {
            let n = counts.get(i).copied().unwrap_or(0);
            match kind.as_str() {
                "person" => r.person += n,
                "organisation" => r.organisation += n,
                _ => r.researcher += n,
            }
        }
        r
    }
}

/// Splits CSV text into records of fields (quotes, doubled quotes, line breaks
/// inside quotes).
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut any = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            c => field.push(c),
        }
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn mask_csv(text: &str, m: &Masker) -> String {
    let mut out = String::with_capacity(text.len());
    for row in parse_csv(text) {
        let fields: Vec<String> = row.iter().map(|f| csv_field(&m.mask(f))).collect();
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

fn mask_json_value(v: &Value, m: &Masker) -> Value {
    match v {
        Value::String(s) => Value::String(m.mask(s)),
        Value::Array(a) => Value::Array(a.iter().map(|x| mask_json_value(x, m)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), mask_json_value(x, m)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `state.json` with its parts masked and its digest recomputed on them.
fn mask_state(text: &str, m: &Masker) -> Result<String> {
    let mut doc: Value = serde_json::from_str(text)?;
    let parts = doc
        .get("parts")
        .map(|p| mask_json_value(p, m))
        .unwrap_or(Value::Null);
    let sha = sha256_hex(serde_json::to_string(&parts)?.as_bytes());
    if let Some(o) = doc.as_object_mut() {
        o.insert("parts".into(), parts);
        o.insert("sha256".into(), json!(sha));
        o.insert("anonymised".into(), json!(true));
    }
    Ok(serde_json::to_string_pretty(&doc)?)
}

fn readme(
    scope: &Scope,
    label: &str,
    r: &Replacements,
    n_entries: usize,
    n_files: usize,
    excluded: &[(String, String)],
    original: &str,
) -> String {
    let mut s = String::from("# Anonymised export\n\n");
    s.push_str(&format!(
        "Package `{}` of Coding Studio {}, pseudonymised with the project's list of names ({n_entries} entries) and written to {n_files} files. Interview ids and unit ids are kept as they are, so that the package can be checked against the original one.\n\n",
        scope.name(),
        crate::VERSION
    ));
    s.push_str("## What was replaced\n\n");
    s.push_str(&format!(
        "- Names of persons in the list: {} replacements.\n- Names of organisations in the list: {} replacements.\n- Names of the researcher (interviewer labels of the study configuration, the actor of the session, and the names given for this export): {} replacements, by `{label}`.\n",
        r.person, r.organisation, r.researcher
    ));
    s.push_str("- Where: every text file of the package (transcript units, passages, quotations, claims, cases, memos, themes, the write-up, prompts, answers, the configuration), in CSV fields, JSON strings and text, matching whole names without regard to case. The digests of `manifest.json` and the state digest of `state.json` are those of the anonymised files.\n\n");
    s.push_str("## What was not replaced\n\n");
    s.push_str(&format!("- {FREE_TEXT_NOTE}\n"));
    s.push_str("- A name in a form the list does not contain (a nickname, a misspelling, an initial), a name that is not in the list, and anything that identifies a person or an organisation indirectly (a job title, a place, a date, a figure). Read the files before sharing them.\n");
    s.push_str("- Values of interview attributes and the study configuration are kept except for listed names.\n\n");
    s.push_str("## Files of the normal package that are left out\n\n");
    for (name, why) in excluded {
        s.push_str(&format!("- `{name}`: {why}.\n"));
    }
    s.push_str("\n## Description of the files\n\n");
    s.push_str(original);
    s.push_str("\n\nTo check the export, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}

fn temp_dir() -> Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("cs-anon-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// Builds the normal package of `scope` in a temporary folder (not logged,
/// without the database snapshot) and returns its folder.
fn build_source(
    store: &Store,
    scope: &Scope,
    tmp: &Path,
    db_path: &Path,
    actor: &str,
) -> Result<PathBuf> {
    let opts = PackageOpts {
        log: false,
        snapshot: false,
    };
    let dir = match scope {
        Scope::Interview(id) => {
            crate::export::write_package_with(store, id, tmp, db_path, actor, opts)?.dir
        }
        Scope::CrossCase => {
            crate::export::write_cross_case_with(store, tmp, db_path, actor, opts)?.folder
        }
        Scope::Writeup => crate::export::write_writeup_with(store, tmp, db_path, actor, opts)?.dir,
    };
    Ok(PathBuf::from(dir))
}

/// Writes the anonymised package of `scope` to `<base>/<date>_<scope>_anonymised/`.
pub fn write_anonymised(
    store: &Store,
    scope: &Scope,
    base: &Path,
    db_path: &Path,
    actor: &str,
    opts: &AnonymiseOptions,
) -> Result<AnonymisedExport> {
    if let Scope::Interview(id) = scope {
        store
            .interview(id)?
            .ok_or_else(|| CsError::NotFound(format!("interview {id}")))?;
    }
    let (masker, digest) = Masker::new(store, opts, actor)?;
    let label = opts
        .researcher_label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or(DEFAULT_RESEARCHER_LABEL)
        .to_string();
    // Interview ids are kept: a listed name inside one would rename it.
    for s in store.summaries()? {
        if masker.rules.contains_original(&s.interview_id) {
            return Err(CsError::Validation(format!(
                "a name of the list occurs in the interview id {}; interview ids are kept, so the list cannot be applied",
                s.interview_id
            ))
            .into());
        }
    }
    let tmp = temp_dir()?;
    let result = (|| -> Result<AnonymisedExport> {
        let source = build_source(store, scope, &tmp, db_path, actor)?;
        fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
        let dir = target_dir(base, &format!("{}_anonymised", scope.name()));
        fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let mut w = Writer {
            dir: dir.clone(),
            files: Vec::new(),
        };
        let mut names: Vec<String> = fs::read_dir(&source)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        let mut excluded: Vec<(String, String)> = Vec::new();
        let mut original_readme = String::new();
        let mut text_files: Vec<(String, String)> = Vec::new();
        for name in names {
            if name == "manifest.json" {
                continue; // replaced by the manifest of the anonymised files
            }
            if let Some((n, why)) = EXCLUDED.iter().find(|(n, _)| *n == name) {
                excluded.push((n.to_string(), why.to_string()));
                continue;
            }
            let path = source.join(&name);
            if name == "writeup.docx" {
                continue; // regenerated below from the masked document
            }
            let bytes = fs::read(&path)?;
            let Ok(text) = String::from_utf8(bytes) else {
                excluded.push((
                    name,
                    "not a text file, so it cannot be pseudonymised".into(),
                ));
                continue;
            };
            let masked = if name == "README.md" {
                original_readme = masker.mask(&text);
                continue;
            } else if name == "state.json" {
                mask_state(&text, &masker)?
            } else if name.ends_with(".csv") {
                mask_csv(&text, &masker)
            } else if name.ends_with(".json") {
                let v: Value = serde_json::from_str(&text)
                    .with_context(|| format!("reading {name} as JSON"))?;
                serde_json::to_string_pretty(&mask_json_value(&v, &masker))?
            } else {
                masker.mask(&text)
            };
            text_files.push((name, masked));
        }
        for (name, text) in &text_files {
            w.write(name, text.as_bytes())?;
        }
        if scope == &Scope::Writeup {
            let doc = store.writeup_doc()?.map_text(&|t| masker.mask(t));
            w.write("writeup.docx", &crate::docx::render(&doc)?)?;
        }
        // The last check: no listed name is left in any text written.
        for (name, text) in &text_files {
            if masker.residual(text) {
                anyhow::bail!("a listed name is still present in {name}; the export is refused");
            }
        }
        let n_entries = store.pseudonym_config()?.entries.len();
        let replacements = masker.replacements();
        w.write(
            "README.md",
            readme(
                scope,
                &label,
                &replacements,
                n_entries,
                w.files.len() + 2,
                &excluded,
                &original_readme,
            )
            .as_bytes(),
        )?;
        let excluded_json: Vec<Value> = excluded
            .iter()
            .map(|(n, why)| json!({"name": n, "reason": why}))
            .collect();
        let manifest = json!({
            "tool": "Coding Studio",
            "version": crate::VERSION,
            "kind": "anonymised",
            "scope": scope.name(),
            "generated_at": crate::now(),
            "anonymisation": {
                "pseudonymisation_digest": digest,
                "n_entries": n_entries,
                "researcher_label": label,
                "replacements": replacements,
                "interview_ids_kept": true,
                "free_text_covered": false,
                "note": FREE_TEXT_NOTE,
                "excluded": excluded_json,
            },
            "files": w.files,
        });
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        let manifest_sha = sha256_hex(&manifest_bytes);
        w.write("manifest.json", &manifest_bytes)?;
        store.log(
            actor,
            "export.anonymised",
            match scope {
                Scope::Interview(id) => Some(id.as_str()),
                _ => None,
            },
            Some(&dir.display().to_string()),
            json!({
                "scope": scope.name(),
                "dir": dir.display().to_string(),
                "manifest_sha256": manifest_sha,
                "pseudonymisation_digest": digest,
                "replacements": replacements,
                "n_files": w.files.len(),
            }),
        )?;
        Ok(AnonymisedExport {
            dir: dir.display().to_string(),
            files: w.files,
            manifest_sha256: manifest_sha,
            replacements,
            excluded: excluded_json,
            pseudonymisation_digest: digest,
        })
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Counts of the replacements by kind, for callers that only need the summary.
pub fn summary(e: &AnonymisedExport) -> BTreeMap<&'static str, usize> {
    BTreeMap::from([
        ("person", e.replacements.person),
        ("organisation", e.replacements.organisation),
        ("researcher", e.replacements.researcher),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_round_trips_quotes_and_line_breaks() {
        let text = "a,b\n\"x, y\",\"he said \"\"hi\"\"\nthen left\"\n,\n";
        let rows = parse_csv(text);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["x, y", "he said \"hi\"\nthen left"]);
        assert_eq!(rows[2], vec!["", ""]);
        let m = Masker {
            rules: Pseudonymiser::new(&[]),
            kinds: vec![],
            counts: Default::default(),
        };
        assert_eq!(mask_csv(text, &m), text);
    }

    #[test]
    fn json_strings_are_masked_and_keys_are_not() {
        let m = Masker {
            rules: Pseudonymiser::new(&[PseudonymEntry {
                kind: "person".into(),
                original: "Anna".into(),
                pseudonym: "P1".into(),
            }]),
            kinds: vec!["person".into()],
            counts: Default::default(),
        };
        let v = json!({"Anna": ["Anna said hi", 3, null], "n": "x"});
        let out = mask_json_value(&v, &m);
        assert_eq!(out, json!({"Anna": ["P1 said hi", 3, null], "n": "x"}));
        assert_eq!(m.replacements().person, 1);
    }
}
