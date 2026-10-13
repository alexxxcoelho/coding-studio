//! Library scan, source parsing and deterministic segmentation.
//!
//! Contract (do not change signatures without updating cs-app):
//! - `scan_library(root)` lists one `InterviewEntry` per interview folder.
//! - `parse_source(path, kind, interviewer_names)` reads speaker turns.
//! - `segment(interview_id, turns)` applies `cs_model::SEGMENTATION_RULE`.
//! - `build_segmentation(entry, rel_path, interviewer_names)` does parse + segment + hashes.
//! - `parse_document(path, interviewer_names)` reads a DOCX, text-layer PDF, SRT or VTT file.
//!
//! Accepted transcript formats:
//! - WebVTT (`.vtt`, `<v Name>` voice tags), SRT (`.srt`, `Name:` prefix), cues merged per speaker.
//! - DOCX (`.docx`): paragraphs; speaker from a `Name:` prefix or a leading bold run.
//! - Text-layer PDF (`.pdf`): one line per PDF line; a PDF without text is read as no turns, with a warning.
//! - Markdown and text kinds listed in `SourceKind` (cleaned, Budgie, timestamped text, written answers).
//!
//! Start times come from the file when present, otherwise `--:--`.
//!
//! Reading never modifies a source file. Text is only trimmed, whitespace
//! collapsed, markdown bold markers removed and HTML entities decoded, so a
//! verbatim quoted from a unit can be found in the source word for word.

use std::path::Path;

use anyhow::{Context, Result};
use cs_model::{
    sha256_hex, CsError, InterviewEntry, Segmentation, SourceKind, Turn, Unit, MAX_WORDS,
    SEGMENTATION_RULE,
};

mod formats;
mod parse;
mod scan;
mod segment;
mod text;

pub use segment::{split_long, split_sentences};

/// Names under which the candidate appears in platform transcripts.
pub const DEFAULT_INTERVIEWER_NAMES: &[&str] = cs_model::config::DEFAULT_INTERVIEWERS;

/// `DEFAULT_INTERVIEWER_NAMES` as owned strings, for `parse_source`.
pub fn default_interviewer_names() -> Vec<String> {
    DEFAULT_INTERVIEWER_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Normalises an interview ID from a folder name: `ECO33_RomanEppenberger` -> `ECO-33`,
/// `ECO35-Schwarz` -> `ECO-35`, `RET-22_von-Grone` -> `RET-22`. `None` when no ID.
pub fn parse_interview_id(folder_name: &str) -> Option<String> {
    scan::parse_interview_id(folder_name)
}

/// Classifies a file by its name and extension (path relative to the interview folder).
pub fn classify(rel_path: &str) -> SourceKind {
    scan::classify(rel_path)
}

/// Scans `root`: every direct sub-folder with an interview ID becomes an entry,
/// sorted by ID. Files are inventoried recursively with their SHA-256, except
/// `archive/`, `sources/`, `__pycache__/`, `generated/`, hidden files and
/// scripts (skipped silently) and media, zip, pdf, `.budgie` and clippings
/// (skipped with a warning). Empty or zero-filled files are reported as
/// probably offline and never proposed as the primary transcript.
pub fn scan_library(root: &Path) -> Result<Vec<InterviewEntry>> {
    scan::scan_library(root)
}

/// Reads the speaker turns of a transcript file. Returns the turns and the
/// warnings raised (skipped notes, missing markers...). Refuses non-transcript kinds.
pub fn parse_source(
    path: &Path,
    kind: SourceKind,
    interviewer_names: &[String],
) -> Result<(Vec<Turn>, Vec<String>)> {
    if !kind.is_transcript() {
        return Err(CsError::Validation(format!(
            "{} is not a transcript ({kind:?}) and cannot be segmented",
            path.display()
        ))
        .into());
    }
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let (content, mut warnings) = decode(&bytes);
    let (turns, more) = match kind {
        SourceKind::Vtt if has_extension(path, "srt") => {
            formats::parse_srt(&content, interviewer_names)
        }
        SourceKind::Vtt => parse::parse_vtt(&content, interviewer_names),
        SourceKind::TimestampedText => parse::parse_timestamped_text(&content),
        SourceKind::CleanedMarkdown => parse::parse_cleaned_markdown(&content),
        SourceKind::BudgieMarkdown => parse::parse_budgie_markdown(&content),
        SourceKind::WrittenResponse => parse::parse_written_response(&content),
        _ => unreachable!("checked by is_transcript"),
    };
    warnings.extend(more);
    Ok((turns, warnings))
}

/// Reads the speaker turns of a DOCX, text-layer PDF, SRT or VTT file, with
/// the same turn shape as `parse_source`. Other extensions are refused. A
/// PDF without a text layer returns no turns and a warning; an unreadable
/// file returns an error.
pub fn parse_document(
    path: &Path,
    interviewer_names: &[String],
) -> Result<(Vec<Turn>, Vec<String>)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !matches!(ext.as_str(), "docx" | "pdf" | "srt" | "vtt") {
        return Err(CsError::Validation(format!(
            "unsupported transcript format for {}: expected DOCX, PDF, SRT or VTT",
            path.display()
        ))
        .into());
    }
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    match ext.as_str() {
        "docx" => formats::parse_docx(&bytes, interviewer_names),
        "pdf" => formats::parse_pdf(&bytes, interviewer_names),
        _ => {
            let (content, mut warnings) = decode(&bytes);
            let (turns, more) = if ext == "srt" {
                formats::parse_srt(&content, interviewer_names)
            } else {
                parse::parse_vtt(&content, interviewer_names)
            };
            warnings.extend(more);
            Ok((turns, warnings))
        }
    }
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(wanted))
}

/// UTF-8 decoding with BOM removal; invalid bytes are replaced and reported.
fn decode(bytes: &[u8]) -> (String, Vec<String>) {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), Vec::new()),
        Err(_) => (
            String::from_utf8_lossy(bytes).into_owned(),
            vec!["file is not valid UTF-8; invalid bytes replaced by U+FFFD".to_string()],
        ),
    }
}

/// Deterministic segmentation: one unit per turn, turns over `MAX_WORDS` split at sentence ends.
pub fn segment(interview_id: &str, turns: &[Turn]) -> Vec<Unit> {
    segment::segment(interview_id, turns)
}

/// Parses and segments one source of an interview, with the source hash, the
/// rule identifier and the hash of the resulting unit list.
pub fn build_segmentation(
    entry: &InterviewEntry,
    rel_path: &str,
    interviewer_names: &[String],
) -> Result<Segmentation> {
    build_segmentation_inner(entry, rel_path, interviewer_names, MAX_WORDS)
}

/// Rule identifier stored with a segmentation: `SEGMENTATION_RULE` for the
/// default limit, the same rule with the configured limit otherwise.
pub fn segmentation_rule(max_words: usize) -> String {
    if max_words == MAX_WORDS {
        SEGMENTATION_RULE.to_string()
    } else {
        format!(
            "turns-v1: one unit per continuous speaker turn, turns over {max_words} words split at sentence boundaries"
        )
    }
}

fn build_segmentation_inner(
    entry: &InterviewEntry,
    rel_path: &str,
    interviewer_names: &[String],
    max_words: usize,
) -> Result<Segmentation> {
    let kind = entry
        .sources
        .iter()
        .find(|s| s.rel_path == rel_path)
        .map(|s| s.kind)
        .unwrap_or_else(|| classify(rel_path));
    if !kind.is_transcript() {
        return Err(CsError::Validation(format!(
            "{rel_path} is not a transcript ({kind:?}) and cannot be segmented"
        ))
        .into());
    }
    let path = Path::new(&entry.folder_path).join(rel_path);
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let source_sha256 = sha256_hex(&bytes);
    let (turns, warnings) = parse_source(&path, kind, interviewer_names)?;
    let units = segment::segment_max(&entry.interview_id, &turns, max_words);
    if units.is_empty() {
        return Err(CsError::Validation(format!(
            "{rel_path} produced no unit: format not recognised or file offline"
        ))
        .into());
    }
    Ok(Segmentation {
        interview_id: entry.interview_id.clone(),
        source_rel_path: rel_path.to_string(),
        source_sha256,
        rule: segmentation_rule(max_words),
        hash: Segmentation::units_hash(&units),
        units,
        warnings,
    })
}

// ------------------------------------------------- configurable variants (v5)

/// `parse_interview_id` with the prefixes of `config.id_prefixes`.
pub fn parse_interview_id_with(
    config: &cs_model::StudyConfig,
    folder_name: &str,
) -> Option<String> {
    scan::parse_interview_id_re(&config.id_regex(), folder_name)
}

/// `scan_library` with the prefixes of `config.id_prefixes`.
pub fn scan_library_with(
    config: &cs_model::StudyConfig,
    root: &Path,
) -> Result<Vec<InterviewEntry>> {
    scan::scan_library_re(&config.id_regex(), root)
}

/// `segment` with `config.max_words` instead of `MAX_WORDS`.
pub fn segment_with(
    config: &cs_model::StudyConfig,
    interview_id: &str,
    turns: &[Turn],
) -> Vec<Unit> {
    segment::segment_max(interview_id, turns, config.max_words)
}

/// `build_segmentation` with `config.interviewers` for role detection and
/// `config.max_words` for the split.
pub fn build_segmentation_with(
    config: &cs_model::StudyConfig,
    entry: &InterviewEntry,
    rel_path: &str,
) -> Result<Segmentation> {
    build_segmentation_inner(entry, rel_path, &config.interviewers, config.max_words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_segmentation_hashes_and_refuses_non_transcripts() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("ECO-16_Yan");
        std::fs::create_dir_all(folder.join("04_parts")).unwrap();
        std::fs::write(
            folder.join("01_transcript_raw.vtt"),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<v Dos Santos Alexandre>Hello.</v>\n\n00:00:03.000 --> 00:00:04.000\n<v Youshuo Yan>Hi there.</v>\n",
        )
        .unwrap();
        std::fs::write(folder.join("04_parts/b_summary.md"), "summary").unwrap();
        let lib = scan_library(dir.path()).unwrap();
        let entry = &lib[0];
        let names = default_interviewer_names();
        let seg = build_segmentation(entry, "01_transcript_raw.vtt", &names).unwrap();
        assert_eq!(seg.units.len(), 2);
        assert_eq!(seg.units[1].unit_id, "ECO-16-U002");
        assert_eq!(seg.hash, Segmentation::units_hash(&seg.units));
        assert_eq!(seg.rule, SEGMENTATION_RULE);
        assert_eq!(
            seg.source_sha256,
            entry
                .sources
                .iter()
                .find(|s| s.rel_path == "01_transcript_raw.vtt")
                .unwrap()
                .sha256
        );
        // Same input, same hash.
        let again = build_segmentation(entry, "01_transcript_raw.vtt", &names).unwrap();
        assert_eq!(again.hash, seg.hash);

        let err = build_segmentation(entry, "04_parts/b_summary.md", &names).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<CsError>(),
            Some(CsError::Validation(_))
        ));
    }

    #[test]
    fn empty_source_yields_validation_error() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("INT-01_X");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("01_transcript_raw.vtt"), "WEBVTT\n").unwrap();
        let lib = scan_library(dir.path()).unwrap();
        let err = build_segmentation(&lib[0], "01_transcript_raw.vtt", &[]).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<CsError>(),
            Some(CsError::Validation(_))
        ));
    }
}
