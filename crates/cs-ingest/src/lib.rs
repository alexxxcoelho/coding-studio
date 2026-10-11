//! Library scan, source parsing and deterministic segmentation.
//!
//! Contract (do not change signatures without updating cs-app):
//! - `scan_library(root)` lists one `InterviewEntry` per interview folder.
//! - `parse_source(path, kind, interviewer_names)` reads speaker turns.
//! - `segment(interview_id, turns)` applies `cs_model::SEGMENTATION_RULE`.
//! - `build_segmentation(entry, rel_path, interviewer_names)` does parse + segment + hashes.

use std::path::Path;

use anyhow::Result;
use cs_model::{InterviewEntry, Segmentation, SourceKind, Turn, Unit};

/// Names under which the candidate appears in platform transcripts.
pub const DEFAULT_INTERVIEWER_NAMES: &[&str] = &[
    "Dos Santos Alexandre",
    "Alexandre Dos Santos",
    "Alexandre Dos Santos Coelho",
    "Alex",
];

/// Normalises an interview ID from a folder name: `ECO33_RomanEppenberger` -> `ECO-33`,
/// `ECO35-Schwarz` -> `ECO-35`, `RET-22_von-Grone` -> `RET-22`. `None` when no ID.
pub fn parse_interview_id(folder_name: &str) -> Option<String> {
    let _ = folder_name;
    todo!()
}

/// Classifies a file by its name and extension (path relative to the interview folder).
pub fn classify(rel_path: &str) -> SourceKind {
    let _ = rel_path;
    todo!()
}

/// Scans `root`: every direct sub-folder with an interview ID becomes an entry.
/// Skips `archive/`, `__pycache__/`, hidden files, scripts and media in the inventory
/// rules described in docs; records skipped or unreadable items as warnings.
pub fn scan_library(root: &Path) -> Result<Vec<InterviewEntry>> {
    let _ = root;
    todo!()
}

/// Reads the speaker turns of a transcript file.
pub fn parse_source(
    path: &Path,
    kind: SourceKind,
    interviewer_names: &[String],
) -> Result<(Vec<Turn>, Vec<String>)> {
    let _ = (path, kind, interviewer_names);
    todo!()
}

/// Deterministic segmentation: one unit per turn, turns over `MAX_WORDS` split at sentence ends.
pub fn segment(interview_id: &str, turns: &[Turn]) -> Vec<Unit> {
    let _ = (interview_id, turns);
    todo!()
}

/// Parse + segment one source of an interview and compute hashes.
pub fn build_segmentation(
    entry: &InterviewEntry,
    rel_path: &str,
    interviewer_names: &[String],
) -> Result<Segmentation> {
    let _ = (entry, rel_path, interviewer_names);
    todo!()
}
