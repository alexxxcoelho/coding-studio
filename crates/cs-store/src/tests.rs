use super::*;
use cs_model::{SourceKind, SEGMENTATION_RULE};
use tempfile::TempDir;

const ACTOR: &str = "candidate";
const ID: &str = "ECO-16";

fn set(codes: &[&str]) -> CodeSet {
    codes.iter().map(|s| s.to_string()).collect()
}

/// A minimal thesis-like coding.db with four codes, one legacy.
fn coding_db(dir: &TempDir, codes: &[(&str, &str, &str, i64)]) -> std::path::PathBuf {
    let path = dir.path().join("coding.db");
    let _ = std::fs::remove_file(&path);
    let c = Connection::open(&path).unwrap();
    c.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT,
         definition TEXT, sub_question TEXT, cycle INTEGER, origin TEXT, active INTEGER, notes TEXT);",
    )
    .unwrap();
    for (code, family, origin, active) in codes {
        c.execute(
            "INSERT INTO codes (code, family, label, definition, origin, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![code, family, format!("{code} label"), format!("{code} definition"), origin, active],
        )
        .unwrap();
    }
    path
}

fn default_codes() -> Vec<(&'static str, &'static str, &'static str, i64)> {
    vec![
        ("TECH-ADV", "TECH", "codebook", 1),
        ("ORG-SKILLS", "ORG", "codebook", 1),
        ("ENV-PARTNER", "ENV", "codebook", 1),
        ("TECH", "TECH", "legacy", 1),
    ]
}

fn entry(sources: &[(&str, SourceKind, &str)]) -> InterviewEntry {
    InterviewEntry {
        interview_id: ID.into(),
        folder_name: "ECO-16_Yan".into(),
        folder_path: "/tmp/ECO-16_Yan".into(),
        sources: sources
            .iter()
            .map(|(p, k, h)| SourceFile {
                rel_path: p.to_string(),
                kind: *k,
                sha256: h.to_string(),
                size: 10,
                modified: None,
            })
            .collect(),
        primary_source: Some("02_transcript_cleaned_diarised.md".into()),
        warnings: vec![],
    }
}

fn default_entry() -> InterviewEntry {
    entry(&[
        (
            "02_transcript_cleaned_diarised.md",
            SourceKind::CleanedMarkdown,
            "aa",
        ),
        ("01_transcript_raw.vtt", SourceKind::Vtt, "bb"),
        ("04_parts/b_summary.md", SourceKind::Summary, "cc"),
    ])
}

fn segmentation(texts: &[&str]) -> Segmentation {
    let units: Vec<Unit> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| Unit {
            unit_id: format!("{ID}-U{:03}", i + 1),
            seq: i as u32 + 1,
            speaker_raw: if i % 2 == 0 { "INT" } else { "RESP" }.into(),
            role: if i % 2 == 0 {
                SpeakerRole::Interviewer
            } else {
                SpeakerRole::Respondent
            },
            timestamp: format!("00:0{i}"),
            text: t.to_string(),
        })
        .collect();
    Segmentation {
        interview_id: ID.into(),
        source_rel_path: "02_transcript_cleaned_diarised.md".into(),
        source_sha256: "aa".into(),
        rule: SEGMENTATION_RULE.into(),
        hash: Segmentation::units_hash(&units),
        units,
        warnings: vec!["skipped note".into()],
    }
}

/// Store with codebook, one interview, three units.
fn ready() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    s.import_codebook(&coding_db(&dir, &default_codes()), ACTOR)
        .unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    s.save_segmentation(
        &segmentation(&[
            "So tell me about the pilot.",
            "The tags paid back within a year.",
            "Our staff had no RFID skills at all.",
        ]),
        ACTOR,
    )
    .unwrap();
    (s, dir)
}

fn hc(unit: &str, codes: &[&str]) -> HumanCoding {
    HumanCoding {
        unit_id: unit.into(),
        codes: set(codes),
        ..Default::default()
    }
}

fn code_all(s: &Store) {
    s.set_human_coding(ID, &hc("ECO-16-U001", &["NA"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, &hc("ECO-16-U002", &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, &hc("ECO-16-U003", &["ORG-SKILLS"]), ACTOR)
        .unwrap();
}

fn output(s: &Store, part: u32, of: u32, codings: &[(&str, &[&str])]) -> AiOutput {
    AiOutput {
        interview_id: ID.into(),
        segmentation_hash: s.segmentation(ID).unwrap().unwrap().hash,
        part,
        of,
        codings: codings
            .iter()
            .map(|(u, c)| (u.to_string(), set(c)))
            .collect(),
    }
}

fn meta(label: &str) -> AiRunMeta {
    AiRunMeta {
        run_label: label.into(),
        model_name: "model-a".into(),
        model_version: "2026-09".into(),
        temperature: Some(0.0),
        prompt_id_version: "CS-P04-EN v1.0".into(),
        executed_at: "2026-10-10T10:00:00Z".into(),
        ..Default::default()
    }
}

fn kind(err: &anyhow::Error) -> &'static str {
    match err.downcast_ref::<CsError>() {
        Some(CsError::Validation(_)) => "validation",
        Some(CsError::Guard(_)) => "guard",
        Some(CsError::NotFound(_)) => "not_found",
        None => "other",
    }
}

fn actions(s: &Store) -> Vec<String> {
    s.audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect()
}

// ------------------------------------------------------------------ library

#[test]
fn open_file_database_uses_delete_journal_and_reopens() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sub/studio.db");
    {
        let s = Store::open(&path).unwrap();
        s.set_setting("library_root", "/x", ACTOR).unwrap();
        let mode: String = s
            .connection()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "delete");
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.setting("library_root").unwrap().as_deref(), Some("/x"));
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn settings_log_only_changes() {
    let s = Store::open_in_memory().unwrap();
    s.set_setting("k", "v1", ACTOR).unwrap();
    s.set_setting("k", "v1", ACTOR).unwrap();
    s.set_setting("k", "v2", ACTOR).unwrap();
    assert_eq!(actions(&s), vec!["settings.set", "settings.set"]);
    let ev = &s.audit_events(None).unwrap()[1];
    assert_eq!(ev.detail["before"], "v1");
    assert_eq!(ev.detail["after"], "v2");
}

#[test]
fn rescan_logs_only_changes_and_keeps_primary_choice() {
    let s = Store::open_in_memory().unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    assert_eq!(actions(&s), vec!["library.interview.add"]);

    s.set_primary_source(ID, "01_transcript_raw.vtt", ACTOR)
        .unwrap();
    // Changed hash, removed summary, added file.
    let e2 = entry(&[
        (
            "02_transcript_cleaned_diarised.md",
            SourceKind::CleanedMarkdown,
            "a2",
        ),
        ("01_transcript_raw.vtt", SourceKind::Vtt, "bb"),
        ("07_pipeline_log.md", SourceKind::Other, "dd"),
    ]);
    s.upsert_interview(&e2, ACTOR).unwrap();
    let evs = s.audit_events(Some(ID)).unwrap();
    let last = evs.last().unwrap();
    assert_eq!(last.action, "library.interview.update");
    assert_eq!(last.detail["changed"][0]["sha256_after"], "a2");
    assert_eq!(
        last.detail["removed"][0]["rel_path"],
        "04_parts/b_summary.md"
    );
    assert_eq!(last.detail["added"][0]["rel_path"], "07_pipeline_log.md");

    let got = s.interview(ID).unwrap().unwrap();
    assert_eq!(got.primary_source.as_deref(), Some("01_transcript_raw.vtt"));
    assert_eq!(got.sources.len(), 3);
    assert!(s.interview("NOPE").unwrap().is_none());
}

#[test]
fn primary_source_must_be_a_transcript() {
    let s = Store::open_in_memory().unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    let e = s
        .set_primary_source(ID, "04_parts/b_summary.md", ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    let e = s.set_primary_source(ID, "missing.vtt", ACTOR).unwrap_err();
    assert_eq!(kind(&e), "validation");
    let e = s.set_primary_source("X-1", "a.vtt", ACTOR).unwrap_err();
    assert_eq!(kind(&e), "not_found");
}

// ------------------------------------------------------------------ codebook

#[test]
fn codebook_import_defaults_and_keeps_choices() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let cb = s
        .import_codebook(&coding_db(&dir, &default_codes()), ACTOR)
        .unwrap();
    assert_eq!(cb.codes.len(), 4);
    assert_eq!(cb.codes[0].code, "TECH-ADV"); // upstream order kept
    assert!(!cb.get("TECH").unwrap().in_prompt); // legacy not offered to AI
    assert!(cb.get("TECH-ADV").unwrap().in_prompt);

    s.set_code_in_prompt("ENV-PARTNER", false, ACTOR).unwrap();
    s.add_code(
        &Code {
            code: "BUSINESS-CASE".into(),
            family: "whatever".into(),
            label: "Business case".into(),
            definition: "d".into(),
            origin: "x".into(),
            active: true,
            in_prompt: true,
        },
        ACTOR,
    )
    .unwrap();

    // Upstream drops ORG-SKILLS and adds TECH-COST.
    let path = coding_db(
        &dir,
        &[
            ("TECH-ADV", "TECH", "codebook", 1),
            ("ENV-PARTNER", "ENV", "codebook", 1),
            ("TECH", "TECH", "legacy", 1),
            ("TECH-COST", "TECH", "codebook", 1),
        ],
    );
    let cb = s.import_codebook(&path, ACTOR).unwrap();
    assert!(!cb.get("ENV-PARTNER").unwrap().in_prompt, "choice kept");
    let bc = cb.get("BUSINESS-CASE").unwrap();
    assert!(bc.active && bc.family == "INDUCTIVE" && bc.origin == "inductive");
    assert!(
        !cb.get("ORG-SKILLS").unwrap().active,
        "deactivated, not deleted"
    );
    assert!(cb.get("TECH-COST").unwrap().in_prompt);
    assert_eq!(cb.codes.last().unwrap().code, "BUSINESS-CASE");
    let ev = s.audit_events(None).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "codebook.import");
    assert_eq!(ev.detail["added"], json!(["TECH-COST"]));
    assert_eq!(ev.detail["removed"], json!(["ORG-SKILLS"]));
    assert_eq!(ev.detail["sha256"].as_str().unwrap().len(), 64);
    assert!(cb.source.ends_with("coding.db"));
}

#[test]
fn add_code_rejects_duplicates_and_bad_names() {
    let (s, _d) = ready();
    let mut c = Code {
        code: "TECH-ADV".into(),
        family: String::new(),
        label: String::new(),
        definition: String::new(),
        origin: String::new(),
        active: true,
        in_prompt: false,
    };
    assert_eq!(kind(&s.add_code(&c, ACTOR).unwrap_err()), "validation");
    c.code = "NA".into();
    assert_eq!(kind(&s.add_code(&c, ACTOR).unwrap_err()), "validation");
    c.code = "TWO WORDS".into();
    assert_eq!(kind(&s.add_code(&c, ACTOR).unwrap_err()), "validation");
    assert_eq!(
        kind(&s.set_code_in_prompt("NOPE", true, ACTOR).unwrap_err()),
        "not_found"
    );
}

#[test]
fn import_never_writes_the_source_database() {
    let dir = TempDir::new().unwrap();
    let path = coding_db(&dir, &default_codes());
    let before = std::fs::read(&path).unwrap();
    let s = Store::open_in_memory().unwrap();
    s.import_codebook(&path, ACTOR).unwrap();
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn real_coding_db_smoke_read_only() {
    let real = Path::new("/Users/alex/kDrive/_EMBA/Thesis/05_Analysis/coding/coding.db");
    if !real.exists() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let copy = dir.path().join("coding.db");
    std::fs::copy(real, &copy).unwrap();
    let s = Store::open_in_memory().unwrap();
    let cb = s.import_codebook(&copy, ACTOR).unwrap();
    assert!(cb.codes.len() >= 40, "got {}", cb.codes.len());
    assert!(cb
        .codes
        .iter()
        .any(|c| c.origin == "legacy" && !c.in_prompt));
}

// -------------------------------------------------------------- segmentation

#[test]
fn segmentation_round_trip_and_idempotent_save() {
    let (s, _d) = ready();
    let seg = s.segmentation(ID).unwrap().unwrap();
    assert_eq!(seg.units.len(), 3);
    assert_eq!(seg.units[1].role, SpeakerRole::Respondent);
    assert_eq!(seg.warnings, vec!["skipped note".to_string()]);
    let n = actions(&s).len();
    s.save_segmentation(&seg, ACTOR).unwrap();
    assert_eq!(actions(&s).len(), n, "same hash: no-op, no log");
    let ev = s
        .audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .find(|e| e.action == "segmentation.save")
        .unwrap();
    assert_eq!(ev.detail["n_units"], 3);
    assert_eq!(ev.detail["source_sha256"], "aa");
}

#[test]
fn resegmentation_allowed_before_coding_refused_after() {
    let (s, _d) = ready();
    s.save_segmentation(&segmentation(&["One.", "Two."]), ACTOR)
        .unwrap();
    assert_eq!(s.segmentation(ID).unwrap().unwrap().units.len(), 2);
    s.set_human_coding(ID, &hc("ECO-16-U001", &["NA"]), ACTOR)
        .unwrap();
    let e = s
        .save_segmentation(&segmentation(&["A.", "B.", "C."]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard");
}

#[test]
fn segmentation_with_wrong_hash_or_unknown_interview_is_refused() {
    let (s, _d) = ready();
    let mut seg = segmentation(&["x"]);
    seg.hash = "bad".into();
    assert_eq!(
        kind(&s.save_segmentation(&seg, ACTOR).unwrap_err()),
        "validation"
    );
    let mut seg = segmentation(&["x"]);
    seg.interview_id = "RET-99".into();
    assert_eq!(
        kind(&s.save_segmentation(&seg, ACTOR).unwrap_err()),
        "not_found"
    );
}

// -------------------------------------------------------------- human coding

#[test]
fn human_coding_validation() {
    let (s, _d) = ready();
    let cases: Vec<HumanCoding> = vec![
        hc("ECO-16-U002", &[]),                 // empty
        hc("ECO-16-U002", &["NA", "TECH-ADV"]), // NA mixed
        hc("ECO-16-U002", &["UNKNOWN"]),        // unknown code
        hc("ECO-16-U999", &["TECH-ADV"]),       // unknown unit
        HumanCoding {
            excerpt: Some("paid back within two years".into()),
            ..hc("ECO-16-U002", &["TECH-ADV"])
        },
    ];
    for c in cases {
        let e = s.set_human_coding(ID, &c, ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{c:?}: {e}");
    }
    assert!(s.human_codings(ID).unwrap().is_empty());
}

#[test]
fn inactive_code_is_refused() {
    let (s, d) = ready();
    s.import_codebook(
        &coding_db(&d, &[("TECH-ADV", "TECH", "codebook", 1)]),
        ACTOR,
    )
    .unwrap();
    let e = s
        .set_human_coding(ID, &hc("ECO-16-U003", &["ORG-SKILLS"]), ACTOR)
        .unwrap_err();
    assert!(e.to_string().contains("inactive"));
}

#[test]
fn human_coding_trims_and_logs_before_after() {
    let (s, _d) = ready();
    let c = HumanCoding {
        unit_id: "ECO-16-U002".into(),
        codes: set(&[" TECH-ADV "]),
        subcode: Some("  payback ".into()),
        modifier: Some("   ".into()),
        ev_level: Some("EV1".into()),
        provenance: Some("SPONTANEOUS".into()),
        memo: Some("first-hand".into()),
        excerpt: Some(" paid back within a year".into()),
    };
    s.set_human_coding(ID, &c, ACTOR).unwrap();
    let got = s.human_codings(ID).unwrap()["ECO-16-U002"].clone();
    assert_eq!(got.codes, set(&["TECH-ADV"]));
    assert_eq!(got.subcode.as_deref(), Some("payback"));
    assert_eq!(got.modifier, None);
    assert_eq!(got.excerpt.as_deref(), Some("paid back within a year"));

    let n = actions(&s).len();
    s.set_human_coding(ID, &c, ACTOR).unwrap();
    assert_eq!(actions(&s).len(), n, "unchanged: no log");

    s.set_human_coding(ID, &hc("ECO-16-U002", &["TECH-ADV", "ORG-SKILLS"]), ACTOR)
        .unwrap();
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "coding.set");
    assert_eq!(ev.target.as_deref(), Some("ECO-16-U002"));
    assert_eq!(ev.detail["before"]["codes"], json!(["TECH-ADV"]));
    assert_eq!(
        ev.detail["after"]["codes"],
        json!(["ORG-SKILLS", "TECH-ADV"])
    );
}

#[test]
fn guard_reports_missing_units() {
    let (s, _d) = ready();
    let g = s.guard(ID).unwrap();
    assert!(!g.complete);
    assert_eq!(g.n_units, 3);
    assert_eq!(g.missing.len(), 3);
    code_all(&s);
    let g = s.guard(ID).unwrap();
    assert!(g.complete && g.missing.is_empty());
    let none = s.guard("RET-01").unwrap();
    assert!(!none.complete && none.n_units == 0);
}

// ------------------------------------------------------------ prompts, runs

#[test]
fn prompts_are_recorded_and_checked() {
    let (s, _d) = ready();
    let seg = s.segmentation(ID).unwrap().unwrap();
    let text = "PROMPT TEXT".to_string();
    let mut p = PromptPart {
        interview_id: ID.into(),
        part: 1,
        of: 1,
        unit_ids: seg.units.iter().map(|u| u.unit_id.clone()).collect(),
        segmentation_hash: seg.hash.clone(),
        codebook_hash: "cb".into(),
        prompt_version: "CS-P04-EN v1.0".into(),
        sha256: sha256_hex(text.as_bytes()),
        text,
    };
    s.record_prompt(&p, ACTOR).unwrap();
    assert_eq!(s.prompts(ID).unwrap(), vec![p.clone()]);
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "prompt.generate");
    assert_eq!(ev.detail["n_units"], 3);
    p.sha256 = "tampered".into();
    assert_eq!(kind(&s.record_prompt(&p, ACTOR).unwrap_err()), "validation");
}

#[test]
fn ai_run_labels_are_unique_and_required() {
    let (s, _d) = ready();
    s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    assert_eq!(
        kind(&s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap_err()),
        "validation"
    );
    assert_eq!(
        kind(&s.create_ai_run(ID, &meta(" "), ACTOR).unwrap_err()),
        "validation"
    );
    let mut m = meta("R2");
    m.model_name = String::new();
    assert_eq!(
        kind(&s.create_ai_run(ID, &m, ACTOR).unwrap_err()),
        "validation"
    );
    assert_eq!(
        kind(&s.create_ai_run("X-9", &meta("R2"), ACTOR).unwrap_err()),
        "not_found"
    );
}

#[test]
fn blind_guard_blocks_ai_import_until_human_coding_complete() {
    let (s, _d) = ready();
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    s.set_human_coding(ID, &hc("ECO-16-U001", &["NA"]), ACTOR)
        .unwrap();
    let out = output(&s, 1, 1, &[("ECO-16-U001", &["NA"])]);
    let e = s.record_ai_output(run, "{}", &out, ACTOR).unwrap_err();
    assert_eq!(kind(&e), "guard");
    assert!(e.to_string().contains("2 units not yet coded"), "{e}");
}

#[test]
fn human_coding_locked_after_ai_import() {
    let (s, _d) = ready();
    code_all(&s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(
        &s,
        1,
        1,
        &[
            ("ECO-16-U001", &["NA"]),
            ("ECO-16-U002", &["TECH-ADV"]),
            ("ECO-16-U003", &["ENV-PARTNER"]),
        ],
    );
    s.record_ai_output(run, "raw answer", &out, ACTOR).unwrap();
    let e = s
        .set_human_coding(ID, &hc("ECO-16-U003", &["TECH-ADV"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard");
    assert!(e.to_string().contains("locked"));
    let e = s
        .save_segmentation(&segmentation(&["new"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard");
}

#[test]
fn ai_output_checks_and_multi_part_merge() {
    let (s, _d) = ready();
    code_all(&s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();

    // Wrong segmentation hash.
    let mut bad = output(&s, 1, 2, &[("ECO-16-U001", &["NA"])]);
    bad.segmentation_hash = "other".into();
    assert_eq!(
        kind(&s.record_ai_output(run, "x", &bad, ACTOR).unwrap_err()),
        "guard"
    );
    // Wrong interview.
    let mut bad = output(&s, 1, 2, &[("ECO-16-U001", &["NA"])]);
    bad.interview_id = "RET-22".into();
    assert_eq!(
        kind(&s.record_ai_output(run, "x", &bad, ACTOR).unwrap_err()),
        "validation"
    );
    // Unknown unit.
    let bad = output(&s, 1, 2, &[("ECO-16-U009", &["NA"])]);
    assert_eq!(
        kind(&s.record_ai_output(run, "x", &bad, ACTOR).unwrap_err()),
        "validation"
    );
    // Unknown run.
    assert_eq!(
        kind(&s.record_ai_output(999, "x", &bad, ACTOR).unwrap_err()),
        "not_found"
    );

    let p1 = output(
        &s,
        1,
        2,
        &[("ECO-16-U001", &["NA"]), ("ECO-16-U002", &["TECH-ADV"])],
    );
    s.record_ai_output(run, "part one raw\n", &p1, ACTOR)
        .unwrap();
    let r = &s.ai_runs(ID).unwrap()[0];
    assert!(!r.complete);
    assert_eq!(r.parts_expected, Some(2));

    // Same part again, wrong `of`, overlapping unit.
    assert_eq!(
        kind(&s.record_ai_output(run, "x", &p1, ACTOR).unwrap_err()),
        "validation"
    );
    let wrong_of = output(&s, 2, 3, &[("ECO-16-U003", &["NA"])]);
    assert_eq!(
        kind(&s.record_ai_output(run, "x", &wrong_of, ACTOR).unwrap_err()),
        "validation"
    );
    let overlap = output(
        &s,
        2,
        2,
        &[("ECO-16-U002", &["NA"]), ("ECO-16-U003", &["NA"])],
    );
    assert_eq!(
        kind(&s.record_ai_output(run, "x", &overlap, ACTOR).unwrap_err()),
        "validation"
    );

    let p2 = output(&s, 2, 2, &[("ECO-16-U003", &["ORG-SKILLS"])]);
    s.record_ai_output(run, "part two raw", &p2, ACTOR).unwrap();
    let r = &s.ai_runs(ID).unwrap()[0];
    assert!(r.complete);
    assert_eq!(r.parts_received, vec![1, 2]);
    assert_eq!(r.codings.len(), 3);
    assert_eq!(r.meta.model_name, "model-a");
    assert_eq!(r.meta.temperature, Some(0.0));

    let raws = s.ai_raw_outputs(run).unwrap();
    assert_eq!(raws[0].1, "part one raw\n", "kept verbatim");
    assert_eq!(raws[0].2, sha256_hex(b"part one raw\n"));
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "ai.output.import");
    assert_eq!(ev.detail["raw_sha256"], sha256_hex(b"part two raw"));
}

// -------------------------------------------------------- arbitration, summary

#[test]
fn arbitration_history_and_validation() {
    let (s, _d) = ready();
    code_all(&s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(
        &s,
        1,
        1,
        &[
            ("ECO-16-U001", &["NA"]),
            ("ECO-16-U002", &["TECH-ADV"]),
            ("ECO-16-U003", &["ENV-PARTNER"]),
        ],
    );
    s.record_ai_output(run, "raw", &out, ACTOR).unwrap();

    let summary = &s.summaries().unwrap()[0];
    assert_eq!(summary.n_units, 3);
    assert!(summary.human_complete);
    assert_eq!(summary.n_ai_runs_complete, 1);
    assert_eq!(summary.n_divergences, 1);
    assert_eq!(summary.n_arbitrated, 0);

    let mut arb = Arbitration {
        unit_id: "ECO-16-U003".into(),
        choice: ArbitrationChoice::Human,
        final_codes: set(&["ORG-SKILLS"]),
        rationale: "  ".into(),
        decided_by: String::new(),
        decided_at: String::new(),
    };
    assert_eq!(
        kind(&s.record_arbitration(ID, &arb, ACTOR).unwrap_err()),
        "validation"
    );
    arb.rationale = "Skills gap stated first-hand.".into();
    arb.final_codes = set(&["NOPE"]);
    assert_eq!(
        kind(&s.record_arbitration(ID, &arb, ACTOR).unwrap_err()),
        "validation"
    );
    arb.final_codes = set(&["ORG-SKILLS"]);
    arb.choice = ArbitrationChoice::Ai {
        run_label: "R9".into(),
    };
    assert_eq!(
        kind(&s.record_arbitration(ID, &arb, ACTOR).unwrap_err()),
        "validation"
    );
    arb.choice = ArbitrationChoice::Human;
    s.record_arbitration(ID, &arb, ACTOR).unwrap();

    let second = Arbitration {
        choice: ArbitrationChoice::Custom,
        final_codes: set(&["ORG-SKILLS", "ENV-PARTNER"]),
        rationale: "Both readings hold on re-listening at 00:02.".into(),
        ..arb.clone()
    };
    s.record_arbitration(ID, &second, ACTOR).unwrap();

    let current = s.arbitrations(ID).unwrap();
    assert_eq!(current.len(), 1);
    let c = &current["ECO-16-U003"];
    assert_eq!(c.choice, ArbitrationChoice::Custom);
    assert_eq!(c.decided_by, ACTOR);
    assert!(!c.decided_at.is_empty());
    let hist = s.arbitration_history(ID).unwrap();
    assert_eq!(hist.len(), 2);
    assert_eq!(hist[0].choice, ArbitrationChoice::Human);
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "arbitration.record");
    assert_eq!(ev.detail["previous"]["choice"]["type"], "human");
    assert_eq!(s.summaries().unwrap()[0].n_arbitrated, 1);
}

// --------------------------------------------------------------------- audit

#[test]
fn append_only_tables_refuse_update_and_delete() {
    let (s, _d) = ready();
    code_all(&s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(
        &s,
        1,
        1,
        &[
            ("ECO-16-U001", &["NA"]),
            ("ECO-16-U002", &["NA"]),
            ("ECO-16-U003", &["NA"]),
        ],
    );
    s.record_ai_output(run, "raw", &out, ACTOR).unwrap();
    s.record_arbitration(
        ID,
        &Arbitration {
            unit_id: "ECO-16-U002".into(),
            choice: ArbitrationChoice::Human,
            final_codes: set(&["TECH-ADV"]),
            rationale: "r".into(),
            decided_by: String::new(),
            decided_at: String::new(),
        },
        ACTOR,
    )
    .unwrap();
    let text = "p".to_string();
    s.record_prompt(
        &PromptPart {
            interview_id: ID.into(),
            part: 1,
            of: 1,
            unit_ids: vec![],
            segmentation_hash: "h".into(),
            codebook_hash: "c".into(),
            prompt_version: "v".into(),
            sha256: sha256_hex(text.as_bytes()),
            text,
        },
        ACTOR,
    )
    .unwrap();
    let c = s.connection();
    for sql in [
        "UPDATE audit_log SET actor = 'x'",
        "DELETE FROM audit_log",
        "UPDATE prompts SET text = 'x'",
        "DELETE FROM prompts",
        "UPDATE ai_outputs SET raw_text = 'x'",
        "DELETE FROM ai_outputs",
        "UPDATE arbitrations SET rationale = 'x'",
        "DELETE FROM arbitrations",
    ] {
        let e = c.execute(sql, []).unwrap_err();
        assert!(e.to_string().contains("append-only"), "{sql}: {e}");
    }
}

#[test]
fn chain_verifies_and_detects_tampering() {
    let (s, _d) = ready();
    code_all(&s);
    s.log(
        ACTOR,
        "export.package",
        Some(ID),
        Some("dir"),
        json!({"files": 3, "t": 0.1}),
    )
    .unwrap();
    let check = s.verify_chain().unwrap();
    assert!(check.ok, "{check:?}");
    let events = s.audit_events(None).unwrap();
    assert_eq!(check.n_events, events.len());
    assert_eq!(events[0].prev_hash, GENESIS_HASH);
    for w in events.windows(2) {
        assert_eq!(w[1].prev_hash, w[0].hash);
        assert_eq!(w[1].seq, w[0].seq + 1);
    }

    let c = s.connection();
    c.execute_batch("DROP TRIGGER audit_log_no_update").unwrap();
    c.execute(
        "UPDATE audit_log SET detail = '{\"before\":null,\"after\":\"forged\"}' WHERE seq = 3",
        [],
    )
    .unwrap();
    let check = s.verify_chain().unwrap();
    assert!(!check.ok);
    assert_eq!(check.first_bad_seq, Some(3));
}

#[test]
fn audit_filter_by_interview() {
    let (s, _d) = ready();
    let all = s.audit_events(None).unwrap();
    let mine = s.audit_events(Some(ID)).unwrap();
    assert!(all.len() > mine.len()); // codebook import is global
    assert!(mine.iter().all(|e| e.interview_id.as_deref() == Some(ID)));
}

#[test]
fn write_and_audit_are_atomic() {
    // A failing domain write leaves neither a row nor an event.
    let (s, _d) = ready();
    let n = actions(&s).len();
    let _ = s.set_human_coding(ID, &hc("ECO-16-U002", &["UNKNOWN"]), ACTOR);
    assert_eq!(actions(&s).len(), n);
    assert!(s.human_codings(ID).unwrap().is_empty());
}
