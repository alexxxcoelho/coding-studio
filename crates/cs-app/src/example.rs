//! The example project: one action fills an empty database with the project
//! template, the example transcript, the codebook and a few coded passages,
//! all of it fictional. Behind `POST /api/example/load` and `example load`.

use std::path::{Path, PathBuf};

use cs_model::CsError;
use cs_store::NewPassage;
use serde::Serialize;

use crate::{bundle, AppState};

const TEMPLATE: &str = include_str!("../../../examples/project_template.example.json");
const CODEBOOK_CSV: &str = include_str!("../../../examples/codebook.csv");
const TRANSCRIPT_FILE: &str = "02_transcript_cleaned_diarised.md";
/// Folder and content of each example transcript.
const TRANSCRIPTS: &[(&str, &str)] = &[
    (
        "INT-01_Example_Retailer",
        include_str!(
            "../../../examples/library/INT-01_Example_Retailer/02_transcript_cleaned_diarised.md"
        ),
    ),
    (
        "INT-02_Exemple_Mediatheque",
        include_str!(
            "../../../examples/library/INT-02_Exemple_Mediatheque/02_transcript_cleaned_diarised.md"
        ),
    ),
];

/// What the load did.
#[derive(Debug, Serialize)]
pub struct ExampleReport {
    pub library: String,
    pub interview_id: String,
    pub interviews: usize,
    /// Units of `interview_id`, the interview that carries the coded passages.
    pub units: usize,
    pub codes: usize,
    pub passages: usize,
    pub research_questions: usize,
}

/// Passages to code: a word in the unit text, and the code applied to it.
const PASSAGES: &[(&str, &str)] = &[
    ("Stock accuracy", "TECH-ACC"),
    ("The tags are cheap", "TECH-COST"),
    ("Our largest supplier", "ENV-SUPP"),
];

/// The folder the transcripts go to when none is chosen.
pub fn default_library() -> PathBuf {
    std::env::temp_dir().join("coding-studio-example-library")
}

fn guard(msg: &str) -> anyhow::Error {
    anyhow::Error::new(CsError::Guard(msg.into()))
}

/// Loads the example into the empty database of `st`. `library` is the folder
/// the example transcript is copied to (created if needed); absent, a folder
/// in the system temporary directory.
pub fn load(st: &AppState, library: Option<&Path>) -> anyhow::Result<ExampleReport> {
    let store = st.store.lock().unwrap_or_else(|e| e.into_inner());
    if !bundle::is_empty_project(&store)? {
        return Err(guard(
            "the example loads into an empty project; this database already holds data",
        ));
    }
    let library = library
        .map(Path::to_path_buf)
        .unwrap_or_else(default_library);
    for (name, text) in TRANSCRIPTS {
        let folder = library.join(name);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(TRANSCRIPT_FILE), text)?;
    }

    let template: serde_json::Value = serde_json::from_str(TEMPLATE)?;
    let report = store.import_template(&template, false, &st.actor)?;
    if !report.applied {
        return Err(anyhow::anyhow!(
            "the example template was refused: {}",
            report.problems.join("; ")
        ));
    }

    let library_text = library.display().to_string();
    store.set_setting("library_root", &library_text, &st.actor)?;
    let cfg = store.study_config()?;
    let entries = cs_ingest::scan_library_with(&cfg, &library)?;
    for entry in &entries {
        store.upsert_interview(entry, &st.actor)?;
    }
    let mut first: Option<(String, cs_model::Segmentation)> = None;
    for entry in &entries {
        let Some(rel) = entry.primary_source.clone() else {
            continue;
        };
        let seg = cs_ingest::build_segmentation_with(&cfg, entry, &rel)?;
        store.save_segmentation(&seg, &st.actor)?;
        if first.is_none() {
            first = Some((entry.interview_id.clone(), seg));
        }
    }
    let (interview_id, seg) = first
        .ok_or_else(|| anyhow::anyhow!("the example transcripts were not found after the copy"))?;

    let csv =
        std::env::temp_dir().join(format!("coding-studio-example-{}.csv", std::process::id()));
    std::fs::write(&csv, CODEBOOK_CSV)?;
    let imported = store.import_codebook_csv_reported(&csv, &st.actor);
    let _ = std::fs::remove_file(&csv);
    let (book, _) = imported?;

    let mut passages = 0;
    for (needle, code) in PASSAGES {
        let Some(unit) = seg.units.iter().find(|u| u.text.contains(needle)) else {
            continue;
        };
        let req = NewPassage {
            interview_id: interview_id.clone(),
            unit_id: unit.unit_id.clone(),
            start: None,
            end: None,
            offset_unit: Default::default(),
            text: None,
        };
        store.code_span(&req, code, "", &st.actor)?;
        passages += 1;
    }
    Ok(ExampleReport {
        library: library_text,
        interview_id,
        interviews: entries.len(),
        units: seg.units.len(),
        codes: book.codes.len(),
        passages,
        research_questions: store.research_questions()?.len(),
    })
}
