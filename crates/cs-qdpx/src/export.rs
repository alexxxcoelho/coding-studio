//! Export of the codebook (`.qdc`) and of the project (`.qdpx`).
//!
//! Mapping, from Coding Studio to REFI-QDA:
//!
//! - A code of the tree becomes a `Code` with its stable id turned into a GUID
//!   (see [`crate::guid`]), its name, its definition as `Description` and its
//!   children nested. Inactive and merged codes and the reserved code "Plus
//!   tard" are not exported; a child of a code that is not exported moves up
//!   to its nearest exported ancestor.
//! - An interview becomes one `TextSource`. Its plain text is every unit of the
//!   interview, interviewer turns included, joined by a blank line (see
//!   [`crate::layout`]); interviewer turns are never coded. The offset map is
//!   written next to the project as `coding-studio-offsets.json`.
//! - A passage becomes a `PlainTextSelection` with one `Coding` per exported
//!   code. Stale passages, passages with only "Plus tard", and highlights are
//!   not exported; with [`ExportOptions::include_highlights`] highlights are
//!   exported with a code named "Surlignages".
//!
//! The output is a pure function of the stored data: same data, same bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use cs_model::passage::{CodeNode, Passage};
use cs_model::{sha256_hex, Unit};
use cs_store::{PassageFilter, Store};
use serde::Serialize;
use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::guid;
use crate::issue::{Error, Result};
use crate::layout::{SourceText, UnitSpan, SEPARATOR};
use crate::refi::{
    project_document, qdc_document, CodeEntry, CodingEntry, ProjectEntry, SelectionEntry,
    SourceEntry, UserEntry,
};

/// Name of the code that carries highlights when they are exported.
pub const HIGHLIGHTS_NAME: &str = "Surlignages";

/// File of the zip that holds the offset map.
pub const OFFSETS_FILE: &str = "coding-studio-offsets.json";

/// Origin written in the files.
pub fn origin() -> String {
    format!("Coding Studio {}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// `name` of the REFI project.
    pub project_name: String,
    /// Export highlights (passages without code) under the code "Surlignages".
    pub include_highlights: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            project_name: "Coding Studio".into(),
            include_highlights: false,
        }
    }
}

/// What an export contains and what it left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExportSummary {
    pub format: String,
    pub codes: usize,
    pub sources: usize,
    pub selections: usize,
    pub codings: usize,
    /// Passages whose text changed since they were made (never moved).
    pub skipped_stale: usize,
    /// Highlights, when the option is off.
    pub skipped_highlights: usize,
    /// Passages with only "Plus tard" or only codes that are not exported.
    pub skipped_without_exported_code: usize,
    pub bytes: usize,
    pub sha256: String,
}

pub struct Export {
    pub bytes: Vec<u8>,
    pub summary: ExportSummary,
    /// Suggested file name.
    pub file_name: String,
}

// ------------------------------------------------------------- collection

struct CodeTree {
    entries: Vec<CodeEntry>,
    /// code_id -> GUID of every exported code.
    guids: BTreeMap<String, String>,
}

fn collect_codes(store: &Store, include_highlights: bool) -> Result<CodeTree> {
    let nodes: Vec<CodeNode> = store
        .code_nodes()?
        .into_iter()
        .filter(|n| !n.system)
        .collect();
    let exported = |n: &CodeNode| n.active && n.merged_into.is_none();
    let by_id: BTreeMap<&str, &CodeNode> = nodes.iter().map(|n| (n.code_id.as_str(), n)).collect();
    // Nearest exported ancestor.
    let parent_of = |n: &CodeNode| -> Option<String> {
        let mut cur = n.parent_id.as_deref();
        for _ in 0..256 {
            let p = by_id.get(cur?)?;
            if exported(p) {
                return Some(p.code_id.clone());
            }
            cur = p.parent_id.as_deref();
        }
        None
    };
    let mut kids: BTreeMap<Option<String>, Vec<&CodeNode>> = BTreeMap::new();
    for n in nodes.iter().filter(|n| exported(n)) {
        kids.entry(parent_of(n)).or_default().push(n);
    }
    fn build(
        parent: Option<String>,
        kids: &BTreeMap<Option<String>, Vec<&CodeNode>>,
        guids: &mut BTreeMap<String, String>,
    ) -> Vec<CodeEntry> {
        kids.get(&parent)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|n| {
                let g = guid::code(&n.code_id);
                guids.insert(n.code_id.clone(), g.clone());
                CodeEntry {
                    guid: g,
                    name: n.name.clone(),
                    description: n.definition.clone(),
                    is_codable: true,
                    children: build(Some(n.code_id.clone()), kids, guids),
                }
            })
            .collect()
    }
    let mut guids = BTreeMap::new();
    let mut entries = build(None, &kids, &mut guids);
    if include_highlights {
        entries.push(CodeEntry {
            guid: guid::highlights_code(),
            name: HIGHLIGHTS_NAME.into(),
            description: "Passages surlignés sans code, exportés par Coding Studio.".into(),
            is_codable: true,
            children: vec![],
        });
    }
    Ok(CodeTree { entries, guids })
}

fn count_codes(entries: &[CodeEntry]) -> usize {
    entries.iter().map(|e| 1 + count_codes(&e.children)).sum()
}

/// Units of an interview as the coder sees them: the segmentation with the
/// latest correction of each unit applied.
pub(crate) fn current_units(store: &Store, interview_id: &str) -> Result<Vec<Unit>> {
    let Some(seg) = store.segmentation(interview_id)? else {
        return Ok(Vec::new());
    };
    let mut units = seg.units;
    for u in &mut units {
        if let Some(c) = store.unit_corrections(interview_id, &u.unit_id)?.pop() {
            u.text = c.text;
            u.role = c.role;
            u.corrected = true;
        }
    }
    Ok(units)
}

/// Interviews that have a segmentation, by id.
pub(crate) fn interview_ids(store: &Store) -> Result<Vec<String>> {
    let mut ids: Vec<String> = store
        .summaries()?
        .into_iter()
        .filter(|s| s.n_units > 0)
        .map(|s| s.interview_id)
        .collect();
    ids.sort();
    Ok(ids)
}

struct Collected {
    entries: Vec<SourceEntry>,
    texts: Vec<(String, String, SourceText)>,
    users: BTreeSet<String>,
}

fn collect_sources(
    store: &Store,
    tree: &CodeTree,
    include_highlights: bool,
    summary: &mut ExportSummary,
) -> Result<Collected> {
    let mut out = Collected {
        entries: Vec::new(),
        texts: Vec::new(),
        users: BTreeSet::new(),
    };
    for id in interview_ids(store)? {
        let units = current_units(store, &id)?;
        let text = SourceText::build(&units);
        let src_guid = guid::source(&id);
        let passages: Vec<Passage> = store.passages(&PassageFilter {
            interview_id: Some(id.clone()),
            ..Default::default()
        })?;
        let mut selections = Vec::new();
        for p in passages {
            let Some(span) = text.span(&p.unit_id) else {
                summary.skipped_stale += 1;
                continue;
            };
            let (start, end) = (span.start + p.start, span.start + p.end);
            if p.stale || text.slice(start, end) != Some(p.text.as_str()) {
                summary.skipped_stale += 1;
                continue;
            }
            let mut codings: Vec<CodingEntry> = p
                .codings
                .iter()
                .filter(|c| !c.system)
                .filter_map(|c| {
                    tree.guids.get(&c.code_id).map(|g| CodingEntry {
                        guid: guid::coding(&p.passage_id, &c.code_id),
                        code_guid: g.clone(),
                    })
                })
                .collect();
            if codings.is_empty() {
                if p.codings.is_empty() && include_highlights {
                    codings.push(CodingEntry {
                        guid: guid::coding(&p.passage_id, "highlights"),
                        code_guid: guid::highlights_code(),
                    });
                } else if p.codings.is_empty() {
                    summary.skipped_highlights += 1;
                    continue;
                } else {
                    summary.skipped_without_exported_code += 1;
                    continue;
                }
            }
            out.users.insert(p.created_by.clone());
            summary.codings += codings.len();
            selections.push(SelectionEntry {
                guid: guid::selection(&p.passage_id),
                start,
                end,
                user_guid: Some(guid::user(&p.created_by)),
                created: Some(p.created_at.clone()),
                codings,
            });
        }
        summary.selections += selections.len();
        out.entries.push(SourceEntry {
            guid: src_guid.clone(),
            name: id.clone(),
            plain_text_path: format!("internal://{src_guid}.txt"),
            selections,
        });
        out.texts.push((id, src_guid, text));
    }
    summary.sources = out.entries.len();
    Ok(out)
}

// ----------------------------------------------------------------- outputs

fn finish(
    store: &Store,
    actor: &str,
    format: &str,
    bytes: Vec<u8>,
    mut summary: ExportSummary,
    file_name: &str,
) -> Result<Export> {
    summary.format = format.to_string();
    summary.bytes = bytes.len();
    summary.sha256 = sha256_hex(&bytes);
    store.log(
        actor,
        &format!("refi.export.{format}"),
        None,
        Some(file_name),
        json!(summary),
    )?;
    Ok(Export {
        bytes,
        summary,
        file_name: file_name.to_string(),
    })
}

/// Codebook export (`.qdc`).
pub fn export_qdc(store: &Store, actor: &str) -> Result<Export> {
    let tree = collect_codes(store, false)?;
    let xml = qdc_document(&origin(), &tree.entries);
    let summary = ExportSummary {
        codes: count_codes(&tree.entries),
        ..Default::default()
    };
    finish(
        store,
        actor,
        "qdc",
        xml.into_bytes(),
        summary,
        "codebook.qdc",
    )
}

#[derive(Serialize)]
struct OffsetsSource<'a> {
    guid: &'a str,
    interview_id: &'a str,
    path: String,
    sha256: String,
    length: usize,
    units: &'a [UnitSpan],
}

#[derive(Serialize)]
struct OffsetsFile<'a> {
    format: &'static str,
    guid_namespace: String,
    position_unit: &'static str,
    separator: &'static str,
    sources: Vec<OffsetsSource<'a>>,
}

fn zip_err(e: impl std::fmt::Display) -> Error {
    Error::Store(anyhow::anyhow!("cannot write the zip archive: {e}"))
}

/// Project export (`.qdpx`): `project.qde`, `Sources/<guid>.txt` per
/// interview and the offset map.
pub fn export_qdpx(store: &Store, opts: &ExportOptions, actor: &str) -> Result<Export> {
    let tree = collect_codes(store, opts.include_highlights)?;
    let mut summary = ExportSummary {
        codes: count_codes(&tree.entries),
        ..Default::default()
    };
    let collected = collect_sources(store, &tree, opts.include_highlights, &mut summary)?;
    let project = ProjectEntry {
        name: opts.project_name.clone(),
        origin: origin(),
        users: collected
            .users
            .iter()
            .map(|u| UserEntry {
                guid: guid::user(u),
                name: u.clone(),
            })
            .collect(),
        codes: tree.entries.clone(),
        sources: collected.entries,
    };
    let offsets = OffsetsFile {
        format: "coding-studio-offsets/1",
        guid_namespace: guid::NAMESPACE.hyphenated().to_string(),
        position_unit: "unicode_scalar_values, from 0, end exclusive",
        separator: SEPARATOR,
        sources: collected
            .texts
            .iter()
            .map(|(id, g, t)| OffsetsSource {
                guid: g,
                interview_id: id,
                path: format!("Sources/{g}.txt"),
                sha256: t.digest(),
                length: t.len(),
                units: &t.spans,
            })
            .collect(),
    };
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    // Fixed time and permissions: the archive does not depend on when or
    // where it was written.
    let opt = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let mut put = |name: &str, data: &[u8]| -> Result<()> {
        zip.start_file(name, opt).map_err(zip_err)?;
        zip.write_all(data).map_err(zip_err)
    };
    put("project.qde", project_document(&project).as_bytes())?;
    for (_, g, t) in &collected.texts {
        put(&format!("Sources/{g}.txt"), t.text.as_bytes())?;
    }
    put(
        OFFSETS_FILE,
        &serde_json::to_vec_pretty(&offsets).map_err(zip_err)?,
    )?;
    let bytes = zip.finish().map_err(zip_err)?.into_inner();
    finish(store, actor, "qdpx", bytes, summary, "project.qdpx")
}
