//! Library scan: one `InterviewEntry` per interview folder, with a hashed
//! inventory of its files.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use cs_model::{sha256_hex, InterviewEntry, SourceFile, SourceKind};
use regex::Regex;
use walkdir::WalkDir;

/// Folder names never inventoried (deliberate, not reported).
const SKIP_DIRS: &[&str] = &["archive", "sources", "__pycache__", "generated"];

/// Normalises an interview ID from a folder name.
pub fn parse_interview_id(folder_name: &str) -> Option<String> {
    static ID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(INT|ECO|RET|ADJ)-?(\d+)").unwrap());
    let caps = ID.captures(folder_name)?;
    Some(format!("{}-{}", &caps[1], &caps[2]))
}

/// Classifies a file by its name (path relative to the interview folder).
pub fn classify(rel_path: &str) -> SourceKind {
    let name = rel_path
        .rsplit('/')
        .next()
        .unwrap_or(rel_path)
        .to_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        // SRT cues are read by the subtitle reader, dispatched on the extension.
        "vtt" | "srt" => SourceKind::Vtt,
        "txt" if name.contains("transcript") => SourceKind::TimestampedText,
        "md" => {
            if name.starts_with("02_transcript_cleaned_diarised") {
                SourceKind::CleanedMarkdown
            } else if name.contains("budgie") {
                SourceKind::BudgieMarkdown
            } else if name.starts_with("01_written_response") {
                SourceKind::WrittenResponse
            } else if name.contains("b_summary")
                || (name.starts_with("05_") && name.contains("summary"))
            {
                SourceKind::Summary
            } else if name.contains("a_record_sheet") || name == "04_interview_record.md" {
                SourceKind::RecordSheet
            } else if name.contains("toe_coding") {
                SourceKind::ToeCoding
            } else {
                SourceKind::Other
            }
        }
        _ => SourceKind::Other,
    }
}

/// Why a file is left out of the inventory. `None` = inventoried.
/// `Some(None)` = skipped silently, `Some(Some(reason))` = skipped with a warning.
fn skip_reason(name: &str) -> Option<Option<&'static str>> {
    if name.starts_with('.') {
        return Some(None);
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "py" | "pyc" => Some(None),
        "mp4" | "m4a" | "wav" | "mp3" | "mov" => Some(Some("media")),
        "zip" => Some(Some("archive")),
        "pdf" => Some(Some("pdf")),
        "budgie" => Some(Some("Budgie project file")),
        "textclipping" => Some(Some("macOS clipping")),
        _ => None,
    }
}

/// Rank used to choose the transcript proposed for segmentation (lower wins).
fn primary_rank(source: &SourceFile) -> Option<(u8, String)> {
    if source.rel_path.contains('/') {
        return None;
    }
    let name = source.rel_path.to_lowercase();
    let rank = match source.kind {
        SourceKind::CleanedMarkdown => 0,
        SourceKind::Vtt if name == "01_transcript_raw.vtt" => 1,
        SourceKind::Vtt => 2,
        SourceKind::BudgieMarkdown => 3,
        SourceKind::TimestampedText => 4,
        SourceKind::WrittenResponse => 5,
        _ => return None,
    };
    Some((rank, name))
}

fn looks_offline(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}

fn modified_iso(meta: &std::fs::Metadata) -> Option<String> {
    let t = meta.modified().ok()?;
    let dt: DateTime<Utc> = t.into();
    Some(dt.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Inventories one interview folder.
fn scan_folder(id: String, folder: &Path) -> Result<InterviewEntry> {
    let folder_name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut sources = Vec::new();
    let mut warnings = Vec::new();
    let mut offline = std::collections::BTreeSet::new();
    let walker = WalkDir::new(folder)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && (name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref())))
        });
    for item in walker {
        let item = match item {
            Ok(i) => i,
            Err(e) => {
                warnings.push(format!("unreadable entry: {e}"));
                continue;
            }
        };
        if !item.file_type().is_file() {
            continue;
        }
        let rel = item
            .path()
            .strip_prefix(folder)
            .unwrap_or(item.path())
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let name = item.file_name().to_string_lossy().into_owned();
        match skip_reason(&name) {
            Some(None) => continue,
            Some(Some(reason)) => {
                warnings.push(format!("skipped: {rel} ({reason})"));
                continue;
            }
            None => {}
        }
        let bytes = match std::fs::read(item.path()) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(format!("unreadable file: {rel} ({e})"));
                continue;
            }
        };
        if looks_offline(&bytes) {
            offline.insert(rel.clone());
            warnings.push(format!(
                "{rel}: file looks offline (kDrive online-only?), empty or zero bytes"
            ));
        }
        let meta = item.metadata().ok();
        sources.push(SourceFile {
            kind: classify(&rel),
            sha256: sha256_hex(&bytes),
            size: bytes.len() as u64,
            modified: meta.as_ref().and_then(modified_iso),
            rel_path: rel,
        });
    }
    let primary_source = sources
        .iter()
        .filter(|s| !offline.contains(&s.rel_path))
        .filter_map(|s| primary_rank(s).map(|r| (r, s.rel_path.clone())))
        .min()
        .map(|(_, p)| p);
    if primary_source.is_none() {
        warnings.push("no transcript found for segmentation".to_string());
    }
    Ok(InterviewEntry {
        interview_id: id,
        folder_name,
        folder_path: folder.to_string_lossy().into_owned(),
        sources,
        primary_source,
        warnings,
    })
}

/// Scans `root`: every direct sub-folder with an interview ID becomes an entry.
pub fn scan_library(root: &Path) -> Result<Vec<InterviewEntry>> {
    let read = std::fs::read_dir(root)
        .with_context(|| format!("cannot read library folder {}", root.display()))?;
    let mut folders: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in read {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        folders.push((name, entry.path()));
    }
    folders.sort();
    let mut by_id: BTreeMap<String, InterviewEntry> = BTreeMap::new();
    for (name, path) in folders {
        let Some(id) = parse_interview_id(&name) else {
            continue;
        };
        if let Some(existing) = by_id.get_mut(&id) {
            existing.warnings.push(format!(
                "duplicate interview ID {id}: folder `{name}` ignored"
            ));
            continue;
        }
        let entry = scan_folder(id.clone(), &path)?;
        by_id.insert(id, entry);
    }
    Ok(by_id.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn interview_ids_are_normalised() {
        assert_eq!(
            parse_interview_id("ECO33_RomanEppenberger").as_deref(),
            Some("ECO-33")
        );
        assert_eq!(
            parse_interview_id("ECO35-Schwarz").as_deref(),
            Some("ECO-35")
        );
        assert_eq!(
            parse_interview_id("RET-22_von-Grone").as_deref(),
            Some("RET-22")
        );
        assert_eq!(
            parse_interview_id("ECO-05_Vieweger").as_deref(),
            Some("ECO-05")
        );
        assert_eq!(
            parse_interview_id("ADJ-01_Bilal").as_deref(),
            Some("ADJ-01")
        );
        assert_eq!(parse_interview_id("eco-05"), None);
        assert_eq!(parse_interview_id("Pasted clipping"), None);
    }

    #[test]
    fn files_are_classified_by_name() {
        assert_eq!(classify("01_transcript_raw.vtt"), SourceKind::Vtt);
        assert_eq!(
            classify("01_transcript_raw_A.txt"),
            SourceKind::TimestampedText
        );
        assert_eq!(classify("verify_report.txt"), SourceKind::Other);
        assert_eq!(
            classify("02_transcript_cleaned_diarised.md"),
            SourceKind::CleanedMarkdown
        );
        assert_eq!(
            classify("01b_transcript_raw_budgie.md"),
            SourceKind::BudgieMarkdown
        );
        assert_eq!(
            classify("01_written_response_raw.md"),
            SourceKind::WrittenResponse
        );
        assert_eq!(classify("04_parts/b_summary.md"), SourceKind::Summary);
        assert_eq!(
            classify("04_parts/a_record_sheet.md"),
            SourceKind::RecordSheet
        );
        assert_eq!(classify("04_interview_record.md"), SourceKind::RecordSheet);
        assert_eq!(classify("04_parts/c_toe_coding.md"), SourceKind::ToeCoding);
        assert_eq!(classify("07_pipeline_log.md"), SourceKind::Other);
    }

    fn write(p: &Path, content: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn scan_builds_inventory_primary_and_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let a = root.join("ECO33_Roman");
        write(&a.join("01_transcript_raw.vtt"), b"WEBVTT\n");
        write(&a.join("EMBA - Interview - Roman.vtt"), b"WEBVTT\n");
        write(
            &a.join("02_transcript_cleaned_diarised.md"),
            b"**INT `[00:01]`:** hi",
        );
        write(&a.join("04_parts/b_summary.md"), b"summary");
        write(&a.join("archive/old.md"), b"old");
        write(&a.join("__pycache__/x.pyc"), b"x");
        write(&a.join("vtt2md.py"), b"x");
        write(&a.join(".DS_Store"), b"x");
        write(&a.join("meeting.mp4"), b"x");
        let b = root.join("RET-22_von-Grone");
        write(&b.join("01_transcript_raw_B.txt"), b"[00:01] S1: b");
        write(&b.join("01_transcript_raw_A.txt"), b"[00:01] S1: a");
        write(&b.join("04_interview_record.md"), &[0u8; 16]);
        let dup = root.join("ECO-33_Duplicate");
        write(&dup.join("01_transcript_raw.vtt"), b"WEBVTT\n");
        write(&root.join("stray.textClipping"), b"x");
        fs::create_dir_all(root.join("No ID folder")).unwrap();

        let lib = scan_library(root).unwrap();
        let ids: Vec<_> = lib.iter().map(|e| e.interview_id.as_str()).collect();
        assert_eq!(ids, vec!["ECO-33", "RET-22"]);

        let eco = &lib[0];
        // `ECO-33_Duplicate` sorts before `ECO33_Roman`, so it is the one kept.
        assert_eq!(eco.folder_name, "ECO-33_Duplicate");
        assert!(eco
            .warnings
            .iter()
            .any(|w| w.contains("duplicate interview ID ECO-33")));

        let ret = &lib[1];
        assert_eq!(
            ret.primary_source.as_deref(),
            Some("01_transcript_raw_A.txt")
        );
        assert!(ret.warnings.iter().any(|w| w.contains("looks offline")));
        assert_eq!(ret.sources.len(), 3);
        assert!(ret
            .sources
            .iter()
            .all(|s| s.sha256.len() == 64 && s.modified.is_some()));
    }

    #[test]
    fn scan_prefers_cleaned_markdown_then_raw_vtt_and_reports_skips() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("ECO33_Roman");
        write(&a.join("EMBA - Interview - Roman.vtt"), b"WEBVTT\n");
        write(&a.join("01_transcript_raw.vtt"), b"WEBVTT\n");
        write(&a.join("meeting.mp4"), b"x");
        write(&a.join("archive/old.md"), b"old");
        write(&a.join("04_parts/b_summary.md"), b"s");
        let lib = scan_library(dir.path()).unwrap();
        let e = &lib[0];
        assert_eq!(e.primary_source.as_deref(), Some("01_transcript_raw.vtt"));
        assert!(e
            .warnings
            .iter()
            .any(|w| w == "skipped: meeting.mp4 (media)"));
        assert!(e
            .sources
            .iter()
            .any(|s| s.rel_path == "04_parts/b_summary.md"));
        assert!(!e.sources.iter().any(|s| s.rel_path.starts_with("archive/")));

        write(&a.join("02_transcript_cleaned_diarised.md"), b"x");
        let lib = scan_library(dir.path()).unwrap();
        assert_eq!(
            lib[0].primary_source.as_deref(),
            Some("02_transcript_cleaned_diarised.md")
        );
    }

    #[test]
    fn empty_transcript_is_not_proposed_as_primary() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("INT-16_X");
        write(&a.join("02_transcript_cleaned_diarised.md"), b"");
        write(&a.join("01_transcript_raw.vtt"), b"WEBVTT\n");
        let lib = scan_library(dir.path()).unwrap();
        assert_eq!(
            lib[0].primary_source.as_deref(),
            Some("01_transcript_raw.vtt")
        );
        assert!(lib[0].warnings.iter().any(|w| w.contains("looks offline")));
    }
}
