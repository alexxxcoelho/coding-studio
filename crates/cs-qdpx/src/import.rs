//! Import of a codebook (`.qdc`) and of the codes and text codings of a
//! project (`.qdpx`).
//!
//! Two steps with the same code path: [`Mode::DryRun`] reads the file,
//! compares it with the project and returns the report, writing nothing;
//! [`Mode::Apply`] does the same and then writes. The report of an apply is
//! the report of the dry run, completed with the stable ids of the codes.
//!
//! Rules:
//!
//! - A code of the file is matched, in this order, by its GUID (a GUID this
//!   crate wrote for a code of the project), then by name, alias or legacy
//!   token without regard to case, following merges. A code that is not found
//!   is created, with its description as definition, under the code the file
//!   puts it under. A matched code is never modified or moved: when the file
//!   places it elsewhere, a warning says so.
//! - A codebook is read as a tree. A code that cannot be created (reserved
//!   name, too long) is skipped and its children move up to its nearest
//!   imported ancestor.
//! - A text source is matched with an interview by name (interview id or
//!   folder name, or the GUID recorded in `coding-studio-offsets.json`) and its
//!   text must be identical to the interview's plain text (SHA-256). A source
//!   without such an interview is reported and skipped; so is a source whose
//!   text differs.
//! - A selection becomes one passage per respondent unit it overlaps, clipped
//!   to the unit. Interviewer turns and the blank line between units are never
//!   coded: that part of a selection is dropped and said so. A selection
//!   without any code of the file, or with only codes that were skipped, is
//!   skipped.
//! - Importing the same file twice changes nothing the second time.
//! - Every write goes through the store (and so through the audit log); the
//!   import adds one event `refi.import.<kind>` with the counts.
//!
//! The writes are not one transaction: the store commits each code and each
//! passage separately. The dry run exists to find problems before that.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use cs_model::passage::OffsetUnit;
use cs_model::{sha256_hex, CsError, SpeakerRole};
use cs_store::{CodeUpdate, NewPassage, PassageFilter, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::export::{current_units, interview_ids, OFFSETS_FILE};
use crate::guid;
use crate::issue::{Error, Issue, Result};
use crate::layout::SourceText;
use crate::refi::{parse_project, parse_qdc, ParsedCode, ParsedSelection, ParsedSource};

/// Largest entry read from an archive, in bytes (a guard against zip bombs).
const MAX_ENTRY: u64 = 256 * 1024 * 1024;

/// Most messages kept in one report.
const MAX_ISSUES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    DryRun,
    Apply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeOutcome {
    Create,
    Match,
    Skip,
}

/// What happens to one code of the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeReport {
    /// `A > B > C` in the file.
    pub path: String,
    pub name: String,
    pub outcome: CodeOutcome,
    /// Stable id: the matched code, or the created one once applied.
    pub code_id: Option<String>,
    /// `guid`, `name` or `file` (same name earlier in the file).
    pub matched_by: Option<String>,
    /// Path of the code it is (or would be) placed under.
    pub parent: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct SourceReport {
    pub name: String,
    pub interview_id: Option<String>,
    /// `guid`, `name` or `digest`.
    pub matched_by: Option<String>,
    pub imported: bool,
    pub reason: Option<String>,
    pub selections: usize,
    pub selections_skipped: usize,
    pub passages_new: usize,
    pub codings_new: usize,
    pub codings_existing: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ImportReport {
    /// `qdc` or `qdpx`.
    pub kind: String,
    /// False for a dry run.
    pub applied: bool,
    pub codes_created: usize,
    pub codes_matched: usize,
    pub codes_skipped: usize,
    pub sources_imported: usize,
    pub sources_skipped: usize,
    pub passages_created: usize,
    pub codings_added: usize,
    pub codings_existing: usize,
    pub selections_skipped: usize,
    pub codes: Vec<CodeReport>,
    pub sources: Vec<SourceReport>,
    /// Sources of the file that are not text (PDF, picture, audio, video).
    pub other_sources_ignored: Vec<String>,
    /// Warnings; nothing here stopped the import.
    pub issues: Vec<Issue>,
}

#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    fn warn(&mut self, location: impl Into<String>, message: impl Into<String>) {
        if self.0.len() < MAX_ISSUES {
            self.0.push(Issue::warning(location, message));
        } else if self.0.len() == MAX_ISSUES {
            self.0
                .push(Issue::warning("report", "further messages are not listed"));
        }
    }
}

fn fold(s: &str) -> String {
    s.trim().to_lowercase()
}

// -------------------------------------------------------------- code plan

#[derive(Debug, Clone)]
struct Existing {
    code_id: String,
    parent_id: Option<String>,
}

#[derive(Debug, Clone)]
struct Planned {
    outcome: CodeOutcome,
    /// Identity of the code this entry stands for: the stable id of a matched
    /// code, `new:<index>` of a code to create.
    key: Option<String>,
    existing: Option<Existing>,
    /// Nearest imported ancestor (index in the plan).
    parent: Option<usize>,
}

struct CodePlan {
    plan: Vec<Planned>,
    reports: Vec<CodeReport>,
}

fn is_not_found(e: &anyhow::Error) -> bool {
    matches!(
        e.chain().find_map(|c| c.downcast_ref::<CsError>()),
        Some(CsError::NotFound(_))
    )
}

fn check_new_name(name: &str) -> std::result::Result<(), String> {
    if name.chars().count() > 200 {
        return Err("name longer than 200 characters".into());
    }
    let f = fold(name);
    if f == "na" || f == "later" || f == "plus tard" {
        return Err(format!("{name} is a reserved name"));
    }
    Ok(())
}

fn plan_codes(store: &Store, codes: &[ParsedCode], issues: &mut Issues) -> Result<CodePlan> {
    let nodes: Vec<_> = store
        .code_nodes()?
        .into_iter()
        .filter(|n| !n.system)
        .collect();
    let by_guid: BTreeMap<String, &str> = nodes
        .iter()
        .map(|n| (guid::code(&n.code_id), n.code_id.as_str()))
        .collect();
    let mut plan: Vec<Planned> = Vec::with_capacity(codes.len());
    let mut reports: Vec<CodeReport> = Vec::with_capacity(codes.len());
    // Name (folded) -> plan index of the first entry of the file with it.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (i, c) in codes.iter().enumerate() {
        let parent = c.parent.and_then(|p| match plan[p].outcome {
            CodeOutcome::Skip => plan[p].parent,
            _ => Some(p),
        });
        let mut report = CodeReport {
            path: c.path.clone(),
            name: c.name.clone(),
            outcome: CodeOutcome::Skip,
            code_id: None,
            matched_by: None,
            parent: parent.map(|p| codes[p].path.clone()),
            reason: None,
        };
        let mut entry = Planned {
            outcome: CodeOutcome::Skip,
            key: None,
            existing: None,
            parent,
        };
        if let Some(&j) = seen.get(&fold(&c.name)) {
            // Same name earlier in the file: the same code.
            entry.outcome = plan[j].outcome;
            entry.key = plan[j].key.clone();
            entry.existing = plan[j].existing.clone();
            report.outcome = plan[j].outcome;
            report.code_id = reports[j].code_id.clone();
            report.matched_by = Some("file".into());
            if plan[j].outcome == CodeOutcome::Create {
                report.outcome = CodeOutcome::Match;
                entry.outcome = CodeOutcome::Match;
            }
            report.reason = Some(format!(
                "same name as {}: read as the same code",
                reports[j].path
            ));
            plan.push(entry);
            reports.push(report);
            continue;
        }
        let mut resolved = None;
        if let Some(code_id) = c
            .guid
            .as_ref()
            .and_then(|g| by_guid.get(&g.to_ascii_lowercase()))
        {
            resolved = Some((store.resolve_code(code_id)?, "guid"));
        }
        if resolved.is_none() {
            match store.resolve_code(&c.name) {
                Ok(r) => resolved = Some((r, "name")),
                Err(e) if is_not_found(&e) => {}
                Err(e) => return Err(e.into()),
            }
        }
        match resolved {
            Some((r, _)) if r.system => {
                report.reason = Some("reserved code \"Plus tard\", never imported".into());
                issues.warn(&c.path, "skipped: this is the reserved code \"Plus tard\"");
            }
            Some((r, _)) if !r.active => {
                report.reason = Some(format!("{} exists but is inactive", r.name));
                issues.warn(
                    &c.path,
                    format!("skipped: the code {} exists but is inactive", r.name),
                );
            }
            Some((r, by)) => {
                let node = nodes.iter().find(|n| n.code_id == r.code_id);
                entry.outcome = CodeOutcome::Match;
                entry.key = Some(r.code_id.clone());
                entry.existing = Some(Existing {
                    code_id: r.code_id.clone(),
                    parent_id: node.and_then(|n| n.parent_id.clone()),
                });
                report.outcome = CodeOutcome::Match;
                report.code_id = Some(r.code_id.clone());
                report.matched_by = Some(by.into());
                seen.insert(fold(&c.name), i);
            }
            None => match check_new_name(&c.name) {
                Ok(()) => {
                    entry.outcome = CodeOutcome::Create;
                    entry.key = Some(format!("new:{i}"));
                    report.outcome = CodeOutcome::Create;
                    seen.insert(fold(&c.name), i);
                }
                Err(why) => {
                    issues.warn(&c.path, format!("skipped: {why}"));
                    report.reason = Some(why);
                }
            },
        }
        plan.push(entry);
        reports.push(report);
    }
    // A code that exists keeps its place; say when the file differs.
    for (i, c) in codes.iter().enumerate() {
        let (Some(ex), Some(p)) = (&plan[i].existing, plan[i].parent) else {
            continue;
        };
        if plan[i].outcome != CodeOutcome::Match || reports[i].matched_by.as_deref() == Some("file")
        {
            continue;
        }
        let same = match (&plan[p].existing, &ex.parent_id) {
            (Some(pe), Some(cur)) => &pe.code_id == cur,
            _ => false,
        };
        if !same {
            issues.warn(
                &c.path,
                format!(
                    "the file puts {} under {}; the code stays where it is in the project",
                    c.name, codes[p].path
                ),
            );
        }
    }
    Ok(CodePlan { plan, reports })
}

/// Upper-case token of a name: letters and digits, other runs as one hyphen.
fn token_of(name: &str) -> String {
    let mut t = String::new();
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            t.extend(ch.to_uppercase());
        } else if !t.is_empty() && !t.ends_with('-') {
            t.push('-');
        }
    }
    let t = t.trim_end_matches('-');
    let t: String = t.chars().take(60).collect();
    if t.is_empty() {
        "CODE".into()
    } else {
        t
    }
}

fn validation_text(e: &anyhow::Error) -> Option<&str> {
    match e.chain().find_map(|c| c.downcast_ref::<CsError>()) {
        Some(CsError::Validation(m)) => Some(m.as_str()),
        _ => None,
    }
}

const DEFAULT_DEFINITION: &str = "Définition à compléter (import REFI-QDA).";

/// Creates the codes to create, in file order, and fills the stable ids.
fn apply_codes(
    store: &Store,
    codes: &[ParsedCode],
    plan: &CodePlan,
    origin: &str,
    reason: &str,
    actor: &str,
) -> Result<BTreeMap<String, String>> {
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    for (i, c) in codes.iter().enumerate() {
        let p = &plan.plan[i];
        let Some(key) = &p.key else { continue };
        if ids.contains_key(key) {
            continue;
        }
        if p.outcome != CodeOutcome::Create {
            ids.insert(key.clone(), key.clone());
            continue;
        }
        let definition = if c.description.trim().is_empty() {
            DEFAULT_DEFINITION.to_string()
        } else {
            c.description.trim().to_string()
        };
        let base = token_of(&c.name);
        let mut token = base.clone();
        let mut attempt = 1;
        loop {
            let model = cs_model::Code {
                code: token.clone(),
                family: "OTHER".into(),
                label: c.name.clone(),
                definition: definition.clone(),
                origin: origin.into(),
                toe: None,
                active: true,
                in_prompt: true,
                research_questions: vec![],
                merged_into: None,
                version: 1,
                example: None,
                reason: Some(reason.into()),
                created_at: None,
            };
            match store.create_code(&model, actor) {
                Ok(_) => break,
                Err(e)
                    if attempt < 50
                        && validation_text(&e).is_some_and(|m| {
                            m.contains("already exists")
                                || m.contains("phase 1 category")
                                || m.contains("reserved")
                        }) =>
                {
                    attempt += 1;
                    token = format!("{base}-{attempt}");
                }
                Err(e) => return Err(e.into()),
            }
        }
        let node = store.update_code(
            &token,
            &CodeUpdate {
                name: Some(c.name.clone()),
                ..Default::default()
            },
            actor,
        )?;
        if let Some(pi) = p.parent {
            let parent_key = plan.plan[pi].key.as_deref().unwrap_or_default();
            if let Some(parent_id) = ids.get(parent_key) {
                store.move_code(&node.code_id, Some(parent_id), reason, actor)?;
            }
        }
        ids.insert(key.clone(), node.code_id);
    }
    Ok(ids)
}

fn tally(plan: &CodePlan, report: &mut ImportReport) {
    for r in &plan.reports {
        match r.outcome {
            CodeOutcome::Create => report.codes_created += 1,
            CodeOutcome::Match => report.codes_matched += 1,
            CodeOutcome::Skip => report.codes_skipped += 1,
        }
    }
}

fn fill_ids(plan: &mut CodePlan, ids: &BTreeMap<String, String>) {
    for (p, r) in plan.plan.iter().zip(plan.reports.iter_mut()) {
        if let Some(id) = p.key.as_ref().and_then(|k| ids.get(k)) {
            r.code_id = Some(id.clone());
        }
    }
}

// ----------------------------------------------------------------- .qdc

/// Imports a codebook file.
pub fn import_qdc(store: &Store, bytes: &[u8], mode: Mode, actor: &str) -> Result<ImportReport> {
    let book = parse_qdc(bytes)?;
    let mut issues = Issues(book.warnings.clone());
    let mut plan = plan_codes(store, &book.codes, &mut issues)?;
    let mut report = ImportReport {
        kind: "qdc".into(),
        ..Default::default()
    };
    tally(&plan, &mut report);
    if mode == Mode::Apply {
        let ids = apply_codes(
            store,
            &book.codes,
            &plan,
            "literature",
            "Import REFI-QDA (.qdc)",
            actor,
        )?;
        fill_ids(&mut plan, &ids);
        report.applied = true;
        store.log(
            actor,
            "refi.import.qdc",
            None,
            None,
            json!({
                "sha256": sha256_hex(bytes),
                "codes_created": report.codes_created,
                "codes_matched": report.codes_matched,
                "codes_skipped": report.codes_skipped,
            }),
        )?;
    }
    report.codes = plan.reports;
    report.issues = issues.0;
    Ok(report)
}

// ---------------------------------------------------------------- .qdpx

fn read_entry(
    zip: &mut zip::ZipArchive<Cursor<&[u8]>>,
    names: &[String],
) -> Result<Option<Vec<u8>>> {
    for wanted in names {
        let mut found = None;
        for i in 0..zip.len() {
            if let Ok(f) = zip.by_index_raw(i) {
                let n = String::from_utf8_lossy(f.name_raw()).into_owned();
                if n.eq_ignore_ascii_case(wanted) {
                    found = Some(n);
                    break;
                }
            }
        }
        let Some(name) = found else { continue };
        let entry = zip.by_name(&name).map_err(|e| {
            Error::rejected(Issue::error(
                name.clone(),
                format!("cannot read the archive entry: {e}"),
            ))
        })?;
        if entry.size() > MAX_ENTRY {
            return Err(Error::rejected(Issue::error(
                name,
                format!("entry larger than {} MB", MAX_ENTRY / 1024 / 1024),
            )));
        }
        let mut buf = Vec::new();
        entry
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut buf)
            .map_err(|e| {
                Error::rejected(Issue::error(
                    name.clone(),
                    format!("cannot read the archive entry: {e}"),
                ))
            })?;
        if buf.len() as u64 > MAX_ENTRY {
            return Err(Error::rejected(Issue::error(name, "entry too large")));
        }
        return Ok(Some(buf));
    }
    Ok(None)
}

/// Text of a source: from `Sources/` of the archive, or inline.
fn source_text(
    zip: &mut zip::ZipArchive<Cursor<&[u8]>>,
    src: &ParsedSource,
) -> std::result::Result<String, String> {
    let bytes = if let Some(path) = &src.plain_text_path {
        let rel = path
            .strip_prefix("internal://")
            .map(|r| format!("Sources/{r}"))
            .unwrap_or_else(|| path.clone());
        if rel.contains("://") || rel.starts_with('/') {
            return Err(format!("the text is an external file ({path})"));
        }
        match read_entry(zip, &[rel.clone()]) {
            Ok(Some(b)) => b,
            Ok(None) => match &src.inline_text {
                Some(t) => t.clone().into_bytes(),
                None => return Err(format!("{rel} is not in the archive")),
            },
            Err(Error::Rejected(i)) => {
                return Err(i.first().map(|x| x.message.clone()).unwrap_or_default())
            }
            Err(Error::Store(e)) => return Err(e.to_string()),
        }
    } else if let Some(t) = &src.inline_text {
        t.clone().into_bytes()
    } else {
        return Err("the source has no text".into());
    };
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| "the text is not UTF-8".to_string())
}

#[derive(Deserialize)]
struct OffsetsHint {
    #[serde(default)]
    sources: Vec<OffsetsHintSource>,
}

#[derive(Deserialize)]
struct OffsetsHintSource {
    guid: String,
    interview_id: String,
}

/// Interview texts, built when first needed.
struct Texts<'a> {
    store: &'a Store,
    cache: BTreeMap<String, SourceText>,
}

impl Texts<'_> {
    fn get(&mut self, id: &str) -> Result<&SourceText> {
        if !self.cache.contains_key(id) {
            let units = current_units(self.store, id)?;
            self.cache.insert(id.to_string(), SourceText::build(&units));
        }
        Ok(&self.cache[id])
    }
}

/// One clipped part of a selection, in a respondent unit.
struct Piece {
    unit_id: String,
    start: usize,
    end: usize,
    text: String,
}

fn clip(
    sel: &ParsedSelection,
    text: &SourceText,
    issues: &mut Issues,
) -> std::result::Result<Vec<Piece>, String> {
    let (Some(start), Some(end)) = (sel.start, sel.end) else {
        return Err("start or end position missing or not a number".into());
    };
    if start >= end {
        return Err(format!("empty or reversed selection {start}..{end}"));
    }
    if end > text.len() {
        return Err(format!(
            "selection {start}..{end} runs past the end of the text ({})",
            text.len()
        ));
    }
    let mut pieces = Vec::new();
    let mut dropped = false;
    let mut covered = 0usize;
    for span in &text.spans {
        let (a, b) = (start.max(span.start), end.min(span.end));
        if a >= b {
            continue;
        }
        covered += b - a;
        if span.role == SpeakerRole::Interviewer {
            dropped = true;
            continue;
        }
        let Some(t) = text.slice(a, b) else { continue };
        if t.trim().is_empty() {
            dropped = true;
            continue;
        }
        pieces.push(Piece {
            unit_id: span.unit_id.clone(),
            start: a - span.start,
            end: b - span.start,
            text: t.to_string(),
        });
    }
    if pieces.is_empty() {
        return Err("falls on an interviewer turn or between turns, which are never coded".into());
    }
    if dropped || covered < end - start || pieces.len() > 1 {
        issues.warn(
            &sel.location,
            format!(
                "selection {start}..{end} split into {} passage(s): interviewer text and blank lines between turns are dropped",
                pieces.len()
            ),
        );
    }
    Ok(pieces)
}

/// Imports a project file.
pub fn import_qdpx(store: &Store, bytes: &[u8], mode: Mode, actor: &str) -> Result<ImportReport> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| Error::rejected(Issue::error("archive", format!("not a ZIP archive: {e}"))))?;
    let qde = read_entry(&mut zip, &["project.qde".into()])?.ok_or_else(|| {
        Error::rejected(Issue::error(
            "archive",
            "no project.qde at the root of the archive: not a REFI-QDA project",
        ))
    })?;
    let project = parse_project(&qde)?;
    let mut issues = Issues(project.warnings.clone());
    issues.0.extend(project.codebook.warnings.clone());
    let hints: BTreeMap<String, String> = read_entry(&mut zip, &[OFFSETS_FILE.into()])?
        .and_then(|b| serde_json::from_slice::<OffsetsHint>(&b).ok())
        .map(|h| {
            h.sources
                .into_iter()
                .map(|s| (s.guid.to_ascii_lowercase(), s.interview_id))
                .collect()
        })
        .unwrap_or_default();

    let mut plan = plan_codes(store, &project.codebook.codes, &mut issues)?;
    // GUID of the file -> plan index.
    let by_file_guid: BTreeMap<String, usize> = project
        .codebook
        .codes
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.guid.as_ref().map(|g| (g.to_ascii_lowercase(), i)))
        .collect();

    // Plan the sources.
    let ids = interview_ids(store)?;
    let folders: BTreeMap<String, String> = store
        .summaries()?
        .into_iter()
        .map(|s| (s.interview_id, s.folder_name))
        .collect();
    let mut texts = Texts {
        store,
        cache: BTreeMap::new(),
    };
    struct SourcePlan {
        interview_id: String,
        report_idx: usize,
        /// (unit, start, end) -> (text, plan indexes of codes)
        passages: BTreeMap<(String, usize, usize), (String, BTreeSet<usize>)>,
    }
    let mut source_plans: Vec<SourcePlan> = Vec::new();
    let mut source_reports: Vec<SourceReport> = Vec::new();
    for src in &project.text_sources {
        let mut sr = SourceReport {
            name: src.name.clone(),
            selections: src.selections.len(),
            ..Default::default()
        };
        let text = match source_text(&mut zip, src) {
            Ok(t) => t,
            Err(why) => {
                sr.reason = Some(why.clone());
                issues.warn(&src.name, format!("source skipped: {why}"));
                source_reports.push(sr);
                continue;
            }
        };
        let file_digest = sha256_hex(text.as_bytes());
        // Candidate by hint, then by name.
        let named = src
            .guid
            .as_ref()
            .and_then(|g| hints.get(&g.to_ascii_lowercase()))
            .filter(|id| ids.contains(id))
            .map(|id| (id.clone(), "guid"))
            .or_else(|| {
                let f = fold(&src.name);
                ids.iter()
                    .find(|id| fold(id) == f || folders.get(*id).is_some_and(|d| fold(d) == f))
                    .map(|id| (id.clone(), "name"))
            });
        let matched = match named {
            Some((id, by)) => {
                let digest = texts.get(&id)?.digest();
                if digest == file_digest {
                    Some((id, by))
                } else {
                    let why = format!(
                        "the text differs from interview {id} (file {}, interview {}): the interview was changed after the file was written, or the file was edited",
                        &file_digest[..12],
                        &digest[..12]
                    );
                    sr.interview_id = Some(id);
                    sr.reason = Some(why.clone());
                    issues.warn(&src.name, format!("source skipped: {why}"));
                    source_reports.push(sr);
                    continue;
                }
            }
            None => {
                let mut found = None;
                for id in &ids {
                    if texts.get(id)?.digest() == file_digest {
                        found = Some((id.clone(), "digest"));
                        break;
                    }
                }
                found
            }
        };
        let Some((interview_id, by)) = matched else {
            let why = "no interview of this name, and none with this text".to_string();
            sr.reason = Some(why.clone());
            issues.warn(&src.name, format!("source skipped: {why}"));
            source_reports.push(sr);
            continue;
        };
        sr.interview_id = Some(interview_id.clone());
        sr.matched_by = Some(by.into());
        sr.imported = true;
        let st = texts.get(&interview_id)?.clone();
        let mut sp = SourcePlan {
            interview_id,
            report_idx: source_reports.len(),
            passages: BTreeMap::new(),
        };
        for sel in &src.selections {
            let mut usable: BTreeSet<usize> = BTreeSet::new();
            for r in &sel.code_refs {
                match by_file_guid.get(&r.to_ascii_lowercase()) {
                    None => issues.warn(
                        &sel.location,
                        format!("coding refers to the unknown code {r}; ignored"),
                    ),
                    Some(&i) if plan.plan[i].outcome == CodeOutcome::Skip => issues.warn(
                        &sel.location,
                        format!(
                            "coding with {} ignored: that code was skipped",
                            project.codebook.codes[i].path
                        ),
                    ),
                    Some(&i) => {
                        usable.insert(i);
                    }
                }
            }
            if usable.is_empty() {
                sr.selections_skipped += 1;
                issues.warn(
                    &sel.location,
                    "selection skipped: it carries no usable code (a highlight or a note in the other tool)",
                );
                continue;
            }
            match clip(sel, &st, &mut issues) {
                Err(why) => {
                    sr.selections_skipped += 1;
                    issues.warn(&sel.location, format!("selection skipped: {why}"));
                }
                Ok(pieces) => {
                    for p in pieces {
                        let e = sp
                            .passages
                            .entry((p.unit_id, p.start, p.end))
                            .or_insert_with(|| (p.text, BTreeSet::new()));
                        e.1.extend(usable.iter().copied());
                    }
                }
            }
        }
        source_plans.push(sp);
        source_reports.push(sr);
    }

    // Existing passages, to count what is new.
    type Spans = BTreeMap<(String, usize, usize), BTreeSet<String>>;
    let mut existing: BTreeMap<String, Spans> = BTreeMap::new();
    for sp in &source_plans {
        let mut m: Spans = BTreeMap::new();
        for p in store.passages(&PassageFilter {
            interview_id: Some(sp.interview_id.clone()),
            ..Default::default()
        })? {
            m.entry((p.unit_id, p.start, p.end))
                .or_default()
                .extend(p.codings.into_iter().map(|c| c.code_id));
        }
        existing.insert(sp.interview_id.clone(), m);
    }

    let mut report = ImportReport {
        kind: "qdpx".into(),
        other_sources_ignored: project.other_sources.clone(),
        ..Default::default()
    };
    tally(&plan, &mut report);

    let mut code_ids: BTreeMap<String, String> = BTreeMap::new();
    if mode == Mode::Apply {
        code_ids = apply_codes(
            store,
            &project.codebook.codes,
            &plan,
            "field",
            "Import REFI-QDA (.qdpx)",
            actor,
        )?;
        fill_ids(&mut plan, &code_ids);
        report.applied = true;
    }

    for sp in &source_plans {
        let sr = &mut source_reports[sp.report_idx];
        let known = &existing[&sp.interview_id];
        for ((unit_id, start, end), (text, code_idxs)) in &sp.passages {
            let have = known.get(&(unit_id.clone(), *start, *end));
            if have.is_none() {
                sr.passages_new += 1;
            }
            // Distinct codes of the store behind the file's codes.
            let keys: BTreeSet<&str> = code_idxs
                .iter()
                .filter_map(|&i| plan.plan[i].key.as_deref())
                .collect();
            for key in keys {
                let present = have.is_some_and(|h| h.contains(key));
                if present {
                    sr.codings_existing += 1;
                    continue;
                }
                sr.codings_new += 1;
                if mode == Mode::Apply {
                    let Some(code_id) = code_ids.get(key) else {
                        continue;
                    };
                    let (_, _created) = store.code_span(
                        &NewPassage {
                            interview_id: sp.interview_id.clone(),
                            unit_id: unit_id.clone(),
                            start: Some(*start),
                            end: Some(*end),
                            offset_unit: OffsetUnit::Scalar,
                            text: Some(text.clone()),
                        },
                        code_id,
                        "",
                        actor,
                    )?;
                }
            }
        }
    }

    for sr in &source_reports {
        if sr.imported {
            report.sources_imported += 1;
        } else {
            report.sources_skipped += 1;
        }
        report.passages_created += sr.passages_new;
        report.codings_added += sr.codings_new;
        report.codings_existing += sr.codings_existing;
        report.selections_skipped += sr.selections_skipped;
    }
    if mode == Mode::Apply {
        store.log(
            actor,
            "refi.import.qdpx",
            None,
            None,
            json!({
                "sha256": sha256_hex(bytes),
                "codes_created": report.codes_created,
                "codes_matched": report.codes_matched,
                "codes_skipped": report.codes_skipped,
                "sources_imported": report.sources_imported,
                "sources_skipped": report.sources_skipped,
                "passages_created": report.passages_created,
                "codings_added": report.codings_added,
                "selections_skipped": report.selections_skipped,
            }),
        )?;
    }
    report.codes = plan.reports;
    report.sources = source_reports;
    report.issues = issues.0;
    Ok(report)
}
