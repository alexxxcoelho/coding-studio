#![allow(clippy::field_reassign_with_default, clippy::type_complexity)]
//! The ingest functions honour `StudyConfig` (v5); the old functions keep
//! their behaviour, which is the default configuration. Red until implemented.

use std::path::Path;

use cs_ingest::{
    build_segmentation, build_segmentation_with, default_interviewer_names, parse_interview_id,
    parse_interview_id_with, scan_library, scan_library_with, segment, segment_with,
};
use cs_model::{SpeakerRole, StudyConfig, Turn};

fn turn(speaker: &str, role: SpeakerRole, text: &str) -> Turn {
    Turn {
        speaker_raw: speaker.into(),
        role,
        timestamp: "01:00".into(),
        text: text.into(),
    }
}

fn config(prefixes: &[&str]) -> StudyConfig {
    StudyConfig {
        id_prefixes: prefixes.iter().map(|s| s.to_string()).collect(),
        ..StudyConfig::default()
    }
}

// ------------------------------------------------------- interview ids

#[test]
fn default_config_parses_ids_like_the_old_function() {
    let cfg = StudyConfig::default();
    for name in [
        "INT33_RomanEppenberger",
        "INT35-Schwarz",
        "RET-22_von-Grone",
        "INT-05_Vieweger",
        "ADJ-01_Bilal",
        "INT-01_Test",
        "int-05",
        "Pasted clipping",
        "XYZ12_Someone",
    ] {
        assert_eq!(
            parse_interview_id_with(&cfg, name),
            parse_interview_id(name),
            "{name}"
        );
    }
    assert_eq!(
        parse_interview_id_with(&cfg, "INT33_RomanEppenberger").as_deref(),
        Some("INT-33")
    );
}

#[test]
fn configured_prefixes_replace_the_hard_coded_ones() {
    let cfg = config(&["XYZ", "AB"]);
    assert_eq!(
        parse_interview_id_with(&cfg, "XYZ12_Someone").as_deref(),
        Some("XYZ-12")
    );
    assert_eq!(
        parse_interview_id_with(&cfg, "AB-7_Jane").as_deref(),
        Some("AB-7")
    );
    assert_eq!(parse_interview_id_with(&cfg, "INT33_Roman"), None);
    // The old function is unchanged.
    assert_eq!(parse_interview_id("XYZ12_Someone"), None);
    assert_eq!(parse_interview_id("INT33_Roman").as_deref(), Some("INT-33"));
}

fn make_library(root: &Path) {
    for dir in ["INT-02_Other", "XYZ-05_Custom", "Notes"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("INT-02_Other/note.txt"), "x").unwrap();
    std::fs::write(root.join("XYZ-05_Custom/note.txt"), "y").unwrap();
}

#[test]
fn scan_uses_the_configured_prefixes() {
    let dir = tempfile::tempdir().unwrap();
    make_library(dir.path());
    let ids = |v: Vec<cs_model::InterviewEntry>| -> Vec<String> {
        v.into_iter().map(|e| e.interview_id).collect()
    };
    assert_eq!(
        ids(scan_library_with(&config(&["XYZ"]), dir.path()).unwrap()),
        ["XYZ-05"]
    );
    assert_eq!(
        ids(scan_library_with(&config(&["XYZ", "INT"]), dir.path()).unwrap()),
        ["INT-02", "XYZ-05"]
    );
    // Old behaviour: default prefixes only.
    assert_eq!(ids(scan_library(dir.path()).unwrap()), ["INT-02"]);
    assert_eq!(
        ids(scan_library_with(&StudyConfig::default(), dir.path()).unwrap()),
        ["INT-02"]
    );
}

// --------------------------------------------------------- segmentation

fn long_turn() -> Turn {
    let sentence = "word ".repeat(30).trim().to_string() + "."; // 30 words
    turn(
        "RESP",
        SpeakerRole::Respondent,
        &vec![sentence; 6].join(" "), // 180 words
    )
}

#[test]
fn segment_with_honours_max_words() {
    let turns = [long_turn()];
    let mut cfg = StudyConfig::default();

    cfg.max_words = 60; // two sentences per unit
    let units = segment_with(&cfg, "X-01", &turns);
    assert_eq!(units.len(), 3);
    assert!(units
        .iter()
        .all(|u| u.text.split_whitespace().count() == 60));
    assert_eq!(units[2].unit_id, "X-01-U003");

    cfg.max_words = 100; // three sentences per unit
    assert_eq!(segment_with(&cfg, "X-01", &turns).len(), 2);

    cfg.max_words = 200; // the whole turn
    assert_eq!(segment_with(&cfg, "X-01", &turns).len(), 1);
}

#[test]
fn segment_keeps_its_behaviour_through_the_default_config() {
    let turns = [
        long_turn(),
        turn("INT", SpeakerRole::Interviewer, "Short one."),
        turn("RESP", SpeakerRole::Respondent, "  "),
    ];
    // 180 words > 150: the old function splits, and so does the default config.
    let old = segment("INT-16", &turns);
    assert_eq!(old.len(), 3);
    assert_eq!(segment_with(&StudyConfig::default(), "INT-16", &turns), old);
}

// ------------------------------------------------- roles from the config

const VTT: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\n<v Interviewer Person A>Hello, can you hear me?</v>\n\n00:00:04.000 --> 00:00:08.000\n<v Jane Roe>Yes, perfectly. We use RFID in two stores.</v>\n\n00:00:09.000 --> 00:00:12.000\n<v Interviewer Person A>Why only two?</v>\n";

fn entry_with_vtt() -> (tempfile::TempDir, cs_model::InterviewEntry) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("INT-02_Other");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("01_transcript_raw.vtt"), VTT).unwrap();
    let entry = scan_library(dir.path()).unwrap().remove(0);
    (dir, entry)
}

fn roles(seg: &cs_model::Segmentation) -> Vec<SpeakerRole> {
    seg.units.iter().map(|u| u.role).collect()
}

#[test]
fn interviewer_labels_come_from_the_config() {
    let (_d, entry) = entry_with_vtt();

    let mut cfg = StudyConfig::default();
    cfg.interviewers = vec!["Jane Roe".into()];
    let seg = build_segmentation_with(&cfg, &entry, "01_transcript_raw.vtt").unwrap();
    assert_eq!(
        roles(&seg),
        [
            SpeakerRole::Respondent,
            SpeakerRole::Interviewer,
            SpeakerRole::Respondent
        ]
    );

    cfg.interviewers = vec!["Interviewer Person A".into()];
    let seg = build_segmentation_with(&cfg, &entry, "01_transcript_raw.vtt").unwrap();
    assert_eq!(
        roles(&seg),
        [
            SpeakerRole::Interviewer,
            SpeakerRole::Respondent,
            SpeakerRole::Interviewer
        ]
    );

    // Nobody listed: two unknown speakers.
    cfg.interviewers = vec![];
    let seg = build_segmentation_with(&cfg, &entry, "01_transcript_raw.vtt").unwrap();
    assert!(roles(&seg).iter().all(|r| *r == SpeakerRole::Unknown));
}

#[test]
fn old_build_segmentation_equals_the_default_config() {
    let (_d, entry) = entry_with_vtt();
    let old = build_segmentation(
        &entry,
        "01_transcript_raw.vtt",
        &default_interviewer_names(),
    )
    .unwrap();
    let new =
        build_segmentation_with(&StudyConfig::default(), &entry, "01_transcript_raw.vtt").unwrap();
    assert_eq!(new, old);
    // Neutral defaults: nobody is listed, so every speaker is unknown.
    assert!(roles(&old).iter().all(|r| *r == SpeakerRole::Unknown));
}

#[test]
fn default_config_interviewers_equal_the_old_defaults() {
    assert_eq!(
        StudyConfig::default().interviewers,
        default_interviewer_names()
    );
}

#[test]
fn max_words_reaches_the_segmentation_hash() {
    let (_d, entry) = entry_with_vtt();
    let a =
        build_segmentation_with(&StudyConfig::default(), &entry, "01_transcript_raw.vtt").unwrap();
    let mut cfg = StudyConfig::default();
    cfg.max_words = 3; // every sentence of a turn becomes its own unit
    let b = build_segmentation_with(&cfg, &entry, "01_transcript_raw.vtt").unwrap();
    assert!(b.units.len() > a.units.len());
    assert_ne!(a.hash, b.hash);
}
