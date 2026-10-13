#![allow(dead_code)]
//! Fixtures of the REFI-QDA tests: a store with fictional interviews, a code
//! tree and passages, and readers for what the exporter writes.

use std::io::Read;

use cs_model::passage::OffsetUnit;
use cs_model::{
    Code, InterviewEntry, Segmentation, SourceFile, SourceKind, SpeakerRole, Unit,
    SEGMENTATION_RULE,
};
use cs_store::{CodeUpdate, NewPassage, PassageFilter, Store};

pub const ACTOR: &str = "candidate";

pub type Turn = (SpeakerRole, &'static str);

pub const INT: SpeakerRole = SpeakerRole::Interviewer;
pub const RESP: SpeakerRole = SpeakerRole::Respondent;

pub fn interview_one() -> Vec<Turn> {
    vec![
        (INT, "Merci de votre temps. Pouvez-vous décrire votre rôle ?"),
        (
            RESP,
            "Je gère la logistique d'un détaillant ; nos fournisseurs étiquettent chaque carton à la source.",
        ),
        (INT, "Qu'est-ce qui a justifié l'investissement ?"),
        (
            RESP,
            "La précision de l'inventaire est passée de soixante à quatre-vingt-dix-huit pour cent 😀 en un an & sans surcoût <majeur>.",
        ),
    ]
}

pub fn interview_two() -> Vec<Turn> {
    vec![
        (INT, "Parlez-moi de la bibliothèque."),
        (
            RESP,
            "Les bénévoles ont suivi deux soirées de formation, et le réseau décide du système.",
        ),
    ]
}

pub fn add_interview(store: &Store, id: &str, turns: &[Turn]) {
    store
        .upsert_interview(
            &InterviewEntry {
                interview_id: id.into(),
                folder_name: format!("{id}_Test"),
                folder_path: format!("/tmp/{id}_Test"),
                sources: vec![SourceFile {
                    rel_path: "02_transcript_cleaned_diarised.md".into(),
                    kind: SourceKind::CleanedMarkdown,
                    sha256: "aa".into(),
                    size: 10,
                    modified: None,
                }],
                primary_source: Some("02_transcript_cleaned_diarised.md".into()),
                warnings: vec![],
            },
            ACTOR,
        )
        .unwrap();
    let units: Vec<Unit> = turns
        .iter()
        .enumerate()
        .map(|(i, (role, text))| Unit {
            unit_id: format!("{id}-U{:03}", i + 1),
            seq: i as u32 + 1,
            speaker_raw: role.short().into(),
            role: *role,
            timestamp: format!("00:{:02}", i),
            text: (*text).into(),
            corrected: false,
        })
        .collect();
    store
        .save_segmentation(
            &Segmentation {
                interview_id: id.into(),
                source_rel_path: "02_transcript_cleaned_diarised.md".into(),
                source_sha256: "aa".into(),
                rule: SEGMENTATION_RULE.into(),
                hash: Segmentation::units_hash(&units),
                units,
                warnings: vec![],
            },
            ACTOR,
        )
        .unwrap();
}

/// A store with the two interviews and no code.
pub fn store_with_interviews() -> Store {
    let s = Store::open_in_memory().unwrap();
    add_interview(&s, "INT-01", &interview_one());
    add_interview(&s, "INT-02", &interview_two());
    s
}

/// Creates a code with a display name, a definition and an optional parent
/// (by stable id). Returns the stable id.
pub fn add_code(
    s: &Store,
    token: &str,
    name: &str,
    definition: &str,
    parent: Option<&str>,
) -> String {
    s.create_code(
        &Code {
            code: token.into(),
            family: "OTHER".into(),
            label: name.into(),
            definition: definition.into(),
            origin: "field".into(),
            toe: None,
            active: true,
            in_prompt: true,
            research_questions: vec![],
            merged_into: None,
            version: 1,
            example: None,
            reason: None,
            created_at: None,
        },
        ACTOR,
    )
    .unwrap();
    let node = s
        .update_code(
            token,
            &CodeUpdate {
                name: Some(name.into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    if let Some(p) = parent {
        s.move_code(&node.code_id, Some(p), "test", ACTOR).unwrap();
    }
    node.code_id
}

/// Codes `chars` start..end of the unit `unit` (1-based) of the interview.
pub fn code_span(
    s: &Store,
    interview: &str,
    unit: usize,
    range: Option<(usize, usize)>,
    code: &str,
) {
    s.code_span(
        &NewPassage {
            interview_id: interview.into(),
            unit_id: format!("{interview}-U{unit:03}"),
            start: range.map(|r| r.0),
            end: range.map(|r| r.1),
            offset_unit: OffsetUnit::Scalar,
            text: None,
        },
        code,
        "",
        ACTOR,
    )
    .unwrap();
}

/// A store with a three-level tree and passages: the reference project of
/// the round-trip tests.
pub fn reference_store() -> Store {
    let s = store_with_interviews();
    let cost = add_code(&s, "COUT", "Coûts", "Coûts directs & indirects.", None);
    let licence = add_code(
        &s,
        "LICENCE",
        "Licences",
        "Licences logicielles.",
        Some(&cost),
    );
    let _tag = add_code(
        &s,
        "TAGS",
        "Prix des étiquettes",
        "Prix unitaire des étiquettes.",
        Some(&cost),
    );
    let skills = add_code(&s, "SKILLS", "Compétences", "Savoir-faire <interne>.", None);
    let _ = licence;
    code_span(&s, "INT-01", 2, Some((0, 20)), &cost);
    code_span(&s, "INT-01", 2, Some((30, 60)), &skills);
    code_span(&s, "INT-01", 4, Some((0, 40)), &cost);
    code_span(&s, "INT-01", 4, Some((0, 40)), &skills);
    code_span(&s, "INT-01", 4, Some((60, 90)), "LICENCE");
    code_span(&s, "INT-02", 2, None, &skills);
    s
}

/// The code tree as `(name, definition, parent name)`, sorted.
pub fn tree(s: &Store) -> Vec<(String, String, Option<String>)> {
    let nodes = s.code_nodes().unwrap();
    let mut v: Vec<_> = nodes
        .iter()
        .filter(|n| !n.system && n.active)
        .map(|n| {
            (
                n.name.clone(),
                n.definition.clone(),
                n.parent_id
                    .as_ref()
                    .map(|p| nodes.iter().find(|x| &x.code_id == p).unwrap().name.clone()),
            )
        })
        .collect();
    v.sort();
    v
}

/// The coded passages as `(interview, unit, start, end, text, code names)`.
pub fn passages(s: &Store) -> Vec<(String, String, usize, usize, String, Vec<String>)> {
    let mut v: Vec<_> = s
        .passages(&PassageFilter::default())
        .unwrap()
        .into_iter()
        .filter(|p| !p.codings.is_empty())
        .map(|p| {
            let mut names: Vec<String> = p.codings.iter().map(|c| c.code_name.clone()).collect();
            names.sort();
            (p.interview_id, p.unit_id, p.start, p.end, p.text, names)
        })
        .collect();
    v.sort();
    v
}

/// Reads one entry of a zip.
pub fn zip_entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).unwrap();
    let mut out = Vec::new();
    f.read_to_end(&mut out).unwrap();
    out
}

pub fn zip_names(bytes: &[u8]) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| String::from_utf8_lossy(z.by_index_raw(i).unwrap().name_raw()).into_owned())
        .collect()
}

/// Builds a zip from named entries, for hand-made project files.
pub fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in entries {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

pub fn audit_actions(s: &Store) -> Vec<String> {
    s.audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect()
}
