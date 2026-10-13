use super::*;
use cs_model::{
    sha256_hex, AiOutput, AiRunMeta, Arbitration, ArbitrationChoice, Case, CaseField, Code,
    HumanCoding, PromptPart, ResolutionKind, ReviewKind, SourceKind, SEGMENTATION_RULE,
};
use rusqlite::params;
use tempfile::TempDir;

use crate::codebook::CodePatch;
use crate::coding::ResolveRequest;

const ACTOR: &str = "candidate";
const ID: &str = "ECO-16";
const U1: &str = "ECO-16-U001";
const U2: &str = "ECO-16-U002";
const U3: &str = "ECO-16-U003";

fn set(codes: &[&str]) -> CodeSet {
    codes.iter().map(|s| s.to_string()).collect()
}

type Upstream = (&'static str, &'static str, &'static str, i64, &'static str);

/// A minimal thesis-like coding.db.
fn coding_db(dir: &TempDir, codes: &[Upstream]) -> std::path::PathBuf {
    let path = dir.path().join("coding.db");
    let _ = std::fs::remove_file(&path);
    let c = Connection::open(&path).unwrap();
    c.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT,
         definition TEXT, sub_question TEXT, cycle INTEGER, origin TEXT, active INTEGER, notes TEXT);",
    )
    .unwrap();
    for (code, family, origin, active, sq) in codes {
        c.execute(
            "INSERT INTO codes (code, family, label, definition, sub_question, origin, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![code, family, format!("{code} label"), format!("{code} definition"), sq, origin, active],
        )
        .unwrap();
    }
    path
}

fn default_codes() -> Vec<Upstream> {
    vec![
        ("TECH-ADV", "TECH", "codebook", 1, "SQ1"),
        ("ORG-SKILLS", "ORG", "codebook", 1, "SQ2"),
        ("ENV-PARTNER", "ENV", "codebook", 1, "SQ3"),
        ("TECH", "TECH", "legacy", 1, ""),
        ("OUT-REALISED", "OUTCOME", "codebook", 1, "SQ4"),
        ("BUSINESS-CASE", "INDUCTIVE", "inductive", 1, "SQ1"),
        ("INDUCTIVE-PENDING", "INDUCTIVE", "bookkeeping", 1, ""),
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
            speaker_raw: "RESP".into(),
            role: SpeakerRole::Respondent,
            timestamp: format!("00:0{i}"),
            text: t.to_string(),
            corrected: false,
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

/// Phase 1: U001 NA, U002 TECH, U003 ORG. Phase 2 scope: U002, U003.
fn phase1(s: &Store) {
    s.set_human_coding(ID, 1, &hc(U1, &["NA"]), ACTOR).unwrap();
    s.set_human_coding(ID, 1, &hc(U2, &["TECH"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 1, &hc(U3, &["ORG"]), ACTOR).unwrap();
}

/// Phase 1 and phase 2 coded: U002 TECH-ADV, U003 ORG-SKILLS.
fn coded(s: &Store) {
    phase1(s);
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
}

fn part(s: &Store, units: &[&str], codes: &[&str], part: u32, of: u32) -> PromptPart {
    let text = format!("prompt part {part} of {of} for {units:?}");
    PromptPart {
        interview_id: ID.into(),
        phase: 2,
        batch_id: 0,
        codes: codes.iter().map(|c| c.to_string()).collect(),
        part,
        of,
        unit_ids: units.iter().map(|u| u.to_string()).collect(),
        segmentation_hash: s.segmentation(ID).unwrap().unwrap().hash,
        codebook_hash: "c".into(),
        prompt_version: "CS-P04-EN v3.0".into(),
        sha256: sha256_hex(text.as_bytes()),
        text,
    }
}

const OFFERED: [&str; 4] = ["TECH-ADV", "ORG-SKILLS", "ENV-PARTNER", "OUT-REALISED"];

/// One batch, one part covering U002 and U003.
fn batch(s: &Store) -> Vec<PromptPart> {
    s.record_prompts(&[part(s, &[U2, U3], &OFFERED, 1, 1)], ACTOR)
        .unwrap()
}

fn output(s: &Store, part: u32, of: u32, codings: &[(&str, &[&str])]) -> AiOutput {
    AiOutput {
        interview_id: ID.into(),
        phase: 2,
        segmentation_hash: s.segmentation(ID).unwrap().unwrap().hash,
        part,
        of,
        codings: codings
            .iter()
            .map(|(u, c)| (u.to_string(), set(c)))
            .collect(),
        candidate_concepts: Vec::new(),
    }
}

fn meta(label: &str) -> AiRunMeta {
    AiRunMeta {
        run_label: label.into(),
        model_name: "model-a".into(),
        model_version: "2026-09".into(),
        temperature: Some(0.0),
        prompt_id_version: "CS-P04-EN v3.0".into(),
        executed_at: "2026-10-10T10:00:00Z".into(),
        ..Default::default()
    }
}

/// Coded, batch, run R1 imported: U002 agrees, U003 disagrees (ENV-PARTNER).
fn challenged(s: &Store) -> i64 {
    coded(s);
    batch(s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["ENV-PARTNER"])]);
    s.record_ai_output(run, "raw answer", &out, ACTOR).unwrap();
    run
}

fn decision(unit: &str, choice: ArbitrationChoice, codes: &[&str], rationale: &str) -> Arbitration {
    Arbitration {
        unit_id: unit.into(),
        phase: 2,
        choice,
        final_codes: set(codes),
        rationale: rationale.into(),
        decided_by: String::new(),
        decided_at: String::new(),
    }
}

fn new_code(code: &str, toe: Option<&str>) -> Code {
    Code {
        code: code.into(),
        family: String::new(),
        label: "Legal obligation".into(),
        definition: "A law or regulation requires the identification.".into(),
        origin: String::new(),
        toe: toe.map(str::to_string),
        active: true,
        in_prompt: true,
        research_questions: vec![],
        merged_into: None,
        version: 1,
        example: None,
        reason: None,
        created_at: None,
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
fn database_of_an_earlier_version_is_refused() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("old.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE interviews (interview_id TEXT PRIMARY KEY);")
            .unwrap();
    }
    let e = Store::open(&path).err().expect("old database refused");
    assert_eq!(kind(&e), "validation");
    assert!(e.to_string().contains("earlier version"), "{e}");

    let fresh = dir.path().join("new.db");
    drop(Store::open(&fresh).unwrap());
    let s = Store::open(&fresh).unwrap();
    let v: i64 = s
        .connection()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
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

// ------------------------------------------------------------------ codebook

#[test]
fn codebook_import_maps_origin_toe_and_questions() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let cb = s
        .import_codebook(&coding_db(&dir, &default_codes()), ACTOR)
        .unwrap();
    assert_eq!(cb.codes.len(), 6, "bookkeeping code skipped");
    let adv = cb.get("TECH-ADV").unwrap();
    assert_eq!(
        (adv.origin.as_str(), adv.toe.as_deref()),
        ("literature", Some("T"))
    );
    assert_eq!(adv.family, "TECH");
    assert_eq!(adv.research_questions, ["SQ1"]);
    let out = cb.get("OUT-REALISED").unwrap();
    assert_eq!((out.toe.as_deref(), out.family.as_str()), (None, "OTHER"));
    assert_eq!(cb.get("BUSINESS-CASE").unwrap().origin, "field");
    let legacy = cb.get("TECH").unwrap();
    assert!(legacy.origin == "legacy" && !legacy.in_prompt);
    let rq: Vec<String> = s
        .research_questions()
        .unwrap()
        .into_iter()
        .map(|q| q.id)
        .collect();
    assert_eq!(rq, ["SQ1", "SQ2", "SQ3", "SQ4"]);

    // Local edits survive a re-import; codes dropped upstream are deactivated.
    s.set_code_in_prompt("ENV-PARTNER", false, ACTOR).unwrap();
    s.edit_code(
        "TECH-ADV",
        &CodePatch {
            definition: Some("Edited locally.".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    s.create_code(&new_code("LEGAL-OBLIGATION", Some("E")), ACTOR)
        .unwrap();
    let path = coding_db(
        &dir,
        &[
            ("TECH-ADV", "TECH", "codebook", 1, "SQ1"),
            ("ENV-PARTNER", "ENV", "codebook", 1, "SQ3"),
            ("TECH-COST", "TECH", "codebook", 1, "SQ5"),
        ],
    );
    let cb = s.import_codebook(&path, ACTOR).unwrap();
    assert_eq!(cb.get("TECH-ADV").unwrap().definition, "Edited locally.");
    assert!(!cb.get("ENV-PARTNER").unwrap().in_prompt, "choice kept");
    assert!(
        !cb.get("ORG-SKILLS").unwrap().active,
        "deactivated, not deleted"
    );
    assert!(
        cb.get("LEGAL-OBLIGATION").unwrap().active,
        "local code kept"
    );
    assert!(cb.get("TECH-COST").unwrap().in_prompt);
    let ev = s.audit_events(None).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "codebook.import");
    assert_eq!(ev.detail["kept_local"], json!(["TECH-ADV"]));
    assert_eq!(ev.detail["research_questions_created"], json!(["SQ5"]));
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
    // Set CS_REAL_CODING_DB to a legacy coding.db to run this smoke test.
    let Some(real_path) = std::env::var_os("CS_REAL_CODING_DB") else {
        return;
    };
    let real = Path::new(&real_path);
    if !real.exists() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let copy = dir.path().join("coding.db");
    std::fs::copy(real, &copy).unwrap();
    let s = Store::open_in_memory().unwrap();
    let cb = s.import_codebook(&copy, ACTOR).unwrap();
    assert!(cb.codes.len() >= 30, "got {}", cb.codes.len());
    assert!(cb
        .codes
        .iter()
        .any(|c| c.origin == "legacy" && !c.in_prompt));
    assert!(cb.codes.iter().any(|c| c.toe.as_deref() == Some("E")));
}

#[test]
fn create_code_validates_name_origin_toe_and_questions() {
    let (s, _d) = ready();
    for name in ["TECH-ADV", "NA", "TWO WORDS", "ENV", "CONTEXT"] {
        let e = s.create_code(&new_code(name, None), ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{name}");
    }
    let mut c = new_code("LEGAL", Some("X"));
    assert!(s
        .create_code(&c, ACTOR)
        .unwrap_err()
        .to_string()
        .contains("TOE"));
    c.toe = None;
    c.definition = " ".into();
    assert_eq!(kind(&s.create_code(&c, ACTOR).unwrap_err()), "validation");
    let mut c = new_code("LEGAL", Some("E"));
    c.research_questions = vec!["RQ9".into()];
    assert!(s
        .create_code(&c, ACTOR)
        .unwrap_err()
        .to_string()
        .contains("RQ9"));
    c.research_questions = vec!["SQ3".into()];
    let stored = s.create_code(&c, ACTOR).unwrap();
    assert_eq!(stored.origin, "field");
    assert_eq!(stored.family, "ENV");
    assert!(stored.created_at.is_some());
    assert_eq!(
        kind(&s.set_code_in_prompt("NOPE", true, ACTOR).unwrap_err()),
        "not_found"
    );
}

#[test]
fn edit_code_bumps_version_and_clears_toe_with_null() {
    let (s, _d) = ready();
    let patch: CodePatch =
        serde_json::from_value(json!({"toe": null, "label": "Advantage"})).unwrap();
    let after = s.edit_code("TECH-ADV", &patch, ACTOR).unwrap();
    assert_eq!(after.version, 2);
    assert_eq!(after.toe, None);
    assert_eq!(after.family, "OTHER");
    assert_eq!(after.label, "Advantage");
    let untouched: CodePatch = serde_json::from_value(json!({"label": "Advantage"})).unwrap();
    assert_eq!(
        s.edit_code("TECH-ADV", &untouched, ACTOR).unwrap().version,
        2,
        "no-op"
    );
    let ev = s.audit_events(None).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "codebook.code.edit");
    assert_eq!(ev.detail["before"]["toe"], "T");
}

#[test]
fn deactivate_and_merge_codes() {
    let (s, _d) = ready();
    coded(&s);
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS", "OUT-REALISED"]), ACTOR)
        .unwrap();
    assert_eq!(
        kind(
            &s.merge_code("ORG-SKILLS", "ORG-SKILLS", "x", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(&s.merge_code("ORG-SKILLS", "TECH", "x", ACTOR).unwrap_err()),
        "validation",
        "legacy target"
    );
    assert_eq!(
        kind(
            &s.merge_code("ORG-SKILLS", "OUT-REALISED", " ", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    s.set_code_active("ENV-PARTNER", false, ACTOR).unwrap();
    assert_eq!(
        kind(
            &s.merge_code("ORG-SKILLS", "ENV-PARTNER", "x", ACTOR)
                .unwrap_err()
        ),
        "validation",
        "inactive target"
    );
    s.set_code_active("ENV-PARTNER", true, ACTOR).unwrap();

    let out = s
        .merge_code("ORG-SKILLS", "OUT-REALISED", "Same mechanism.", ACTOR)
        .unwrap();
    assert_eq!(out.rewritten_codings, 1);
    assert_eq!(
        s.human_coding_map(ID, 2).unwrap()[U3],
        set(&["OUT-REALISED"])
    );
    let merged = s.code_row("ORG-SKILLS").unwrap().unwrap();
    assert_eq!(merged.merged_into.as_deref(), Some("OUT-REALISED"));
    assert!(!merged.active);
    assert_eq!(
        kind(&s.set_code_active("ORG-SKILLS", true, ACTOR).unwrap_err()),
        "validation"
    );
    assert_eq!(
        kind(
            &s.merge_code("ORG-SKILLS", "TECH-ADV", "x", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    let e = s
        .set_human_coding(ID, 2, &hc(U2, &["ORG-SKILLS"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation", "merged code no longer usable");
    let ev = s.audit_events(None).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "codebook.code.merge");
    assert_eq!(ev.detail["n_codings"], 1);
}

#[test]
fn code_units_lists_the_passages_to_revisit() {
    let (s, _d) = ready();
    coded(&s);
    let units = s.code_units("TECH-ADV").unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].0, ID);
    assert_eq!(units[0].1.unit_id, U2);
    assert_eq!(kind(&s.code_units("NOPE").unwrap_err()), "not_found");
}

#[test]
fn codebook_export_sql_applies_to_the_thesis_schema() {
    let (s, _d) = ready();
    s.create_code(&new_code("LEGAL-OBLIGATION", Some("E")), ACTOR)
        .unwrap();
    let (csv, sql) = s.codebook_export().unwrap();
    assert!(csv.starts_with("code,label,definition,toe,origin"));
    assert!(csv.contains("LEGAL-OBLIGATION,Legal obligation"));
    assert!(sql.contains("'LEGAL-OBLIGATION','ENV'"));
    assert!(sql.contains("'inductive'"), "field written as inductive");
    // Set CS_REAL_SCHEMA_SQL to the schema of a legacy coding.db to check the SQL export against it.
    let schema_env = std::env::var_os("CS_REAL_SCHEMA_SQL");
    let schema = schema_env
        .as_deref()
        .map(Path::new)
        .unwrap_or(Path::new("/nonexistent/schema.sql"));
    let c = Connection::open_in_memory().unwrap();
    if schema.exists() {
        c.execute_batch(&std::fs::read_to_string(schema).unwrap())
            .unwrap();
    } else {
        c.execute_batch(
            "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT, definition TEXT,
             sub_question TEXT, cycle INTEGER, origin TEXT, active INTEGER, notes TEXT);",
        )
        .unwrap();
    }
    c.execute_batch(&sql).unwrap();
    let (family, origin, sq): (String, String, Option<String>) = c
        .query_row(
            "SELECT family, origin, sub_question FROM codes WHERE code = 'TECH-ADV'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (family.as_str(), origin.as_str(), sq.as_deref()),
        ("TECH", "codebook", Some("SQ1"))
    );
}

#[test]
fn research_questions_crud_and_delete_refusal() {
    let (s, _d) = ready();
    s.create_research_question("RQ1", "Why do projects stall?", ACTOR)
        .unwrap();
    assert_eq!(
        kind(&s.create_research_question("RQ1", "x", ACTOR).unwrap_err()),
        "validation"
    );
    assert_eq!(
        kind(&s.create_research_question("R Q", "x", ACTOR).unwrap_err()),
        "validation"
    );
    s.update_research_question("RQ1", "Why do RFID projects stall?", Some(1), ACTOR)
        .unwrap();
    let q = s.research_questions().unwrap();
    assert_eq!(q[0].id, "RQ1");
    assert_eq!(q[0].text, "Why do RFID projects stall?");
    let e = s.delete_research_question("SQ1", ACTOR).unwrap_err();
    assert_eq!(kind(&e), "guard", "TECH-ADV maps to SQ1");
    s.delete_research_question("RQ1", ACTOR).unwrap();
    assert!(s
        .research_questions()
        .unwrap()
        .iter()
        .all(|q| q.id != "RQ1"));
    assert_eq!(
        kind(&s.delete_research_question("RQ1", ACTOR).unwrap_err()),
        "not_found"
    );
}

// -------------------------------------------------------------- segmentation

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

// -------------------------------------------------------------- coding

#[test]
fn phase1_accepts_the_four_categories_and_stays_editable() {
    let (s, _d) = ready();
    for bad in [
        &["TECH-ADV"][..],
        &["INDUCTIVE-PENDING"],
        &["NA", "TECH"],
        &[],
    ] {
        let e = s.set_human_coding(ID, 1, &hc(U2, bad), ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{bad:?}");
    }
    phase1(&s);
    challenged_after(&s);
    // Still editable after the AI pass of phase 2.
    s.set_human_coding(ID, 1, &hc(U2, &["TECH", "ENV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 1, &hc(U3, &["CONTEXT"]), ACTOR)
        .unwrap();
    assert_eq!(s.human_coding_map(ID, 1).unwrap()[U3], set(&["CONTEXT"]));
}

/// Phase 2 coding, batch and an AI answer on top of an existing phase 1.
fn challenged_after(s: &Store) {
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    batch(s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["ENV-PARTNER"])]);
    s.record_ai_output(run, "raw", &out, ACTOR).unwrap();
}

#[test]
fn phase1_has_no_runs_prompts_or_review() {
    let (s, _d) = ready();
    phase1(&s);
    let mut p = part(&s, &[U1], &[], 1, 1);
    p.phase = 1;
    assert_eq!(
        kind(&s.record_prompts(&[p], ACTOR).unwrap_err()),
        "validation"
    );
    let mut out = output(&s, 1, 1, &[(U1, &["NA"])]);
    out.phase = 1;
    batch_free_run_rejects_phase1(&s, &out);
}

fn batch_free_run_rejects_phase1(s: &Store, out: &AiOutput) {
    coded_after_phase1(s);
    batch(s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    assert_eq!(
        kind(&s.record_ai_output(run, "raw", out, ACTOR).unwrap_err()),
        "validation"
    );
}

fn coded_after_phase1(s: &Store) {
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
}

#[test]
fn phase2_scope_follows_phase1() {
    let (s, _d) = ready();
    // Automatic phase 1 (CONTEXT on each respondent unit): all three in scope.
    assert_eq!(s.phase_units(ID, 2).unwrap(), [U1, U2, U3]);
    phase1(&s);
    assert_eq!(s.phase_units(ID, 2).unwrap(), [U2, U3]);
    let e = s
        .set_human_coding(ID, 2, &hc(U1, &["TECH-ADV"]), ACTOR)
        .unwrap_err();
    assert!(e.to_string().contains("out of phase 2 scope"), "{e}");
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    // U003 leaves the scope; its coding is kept and stays editable.
    s.set_human_coding(ID, 1, &hc(U3, &["NA"]), ACTOR).unwrap();
    assert_eq!(s.phase_units(ID, 2).unwrap(), [U2]);
    assert!(s.has_phase2_coding(ID, U3).unwrap());
    s.set_human_coding(ID, 2, &hc(U3, &["OUT-REALISED"]), ACTOR)
        .unwrap();
}

#[test]
fn phase2_offers_every_code_attached_first() {
    let (s, _d) = ready();
    phase1(&s);
    let names: Vec<String> = s
        .allowed_codes(ID, 2, U3)
        .unwrap()
        .into_iter()
        .map(|c| c.code)
        .collect();
    assert_eq!(
        names,
        [
            "ORG-SKILLS",
            "TECH-ADV",
            "ENV-PARTNER",
            "OUT-REALISED",
            "BUSINESS-CASE"
        ]
    );
    // Any phase 2 code is accepted, whatever the phase 1 context.
    s.set_human_coding(ID, 2, &hc(U2, &["ENV-PARTNER", "BUSINESS-CASE"]), ACTOR)
        .unwrap();
    for bad in [&["TECH"][..], &["TECH-ADV", "NA"], &["UNKNOWN"]] {
        let e = s.set_human_coding(ID, 2, &hc(U2, bad), ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{bad:?}");
    }
    let e = s.set_human_coding(ID, 2, &hc(U3, &[]), ACTOR).unwrap_err();
    assert!(e.to_string().contains("label to clarify"), "{e}");
}

#[test]
fn human_coding_trims_checks_excerpt_and_logs_before_after() {
    let (s, _d) = ready();
    phase1(&s);
    let mut c = hc(U2, &[" TECH-ADV "]);
    c.memo = Some("  payback stated  ".into());
    c.excerpt = Some("paid back within a year".into());
    s.set_human_coding(ID, 2, &c, ACTOR).unwrap();
    let n = actions(&s).len();
    s.set_human_coding(ID, 2, &c, ACTOR).unwrap();
    assert_eq!(actions(&s).len(), n, "unchanged: no event");
    let got = &s.human_codings(ID, 2).unwrap()[U2];
    assert_eq!(got.codes, set(&["TECH-ADV"]));
    assert_eq!(got.memo.as_deref(), Some("payback stated"));
    c.excerpt = Some("paid off".into());
    assert_eq!(
        kind(&s.set_human_coding(ID, 2, &c, ACTOR).unwrap_err()),
        "validation"
    );
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "coding.set");
    assert_eq!(ev.detail["phase"], 2);
    assert_eq!(ev.detail["before"], Value::Null);
}

#[test]
fn edits_after_the_ai_answer_are_flagged() {
    let (s, _d) = ready();
    challenged(&s);
    assert!(!s.human_codings(ID, 2).unwrap()[U3].revised_after_ai);
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS", "ENV-PARTNER"]), ACTOR)
        .unwrap();
    let got = &s.human_codings(ID, 2).unwrap()[U3];
    assert!(got.revised_after_ai);
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.detail["revised_after_ai"], true);
    // The flag stays once set, even if the codes come back.
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    assert!(s.human_codings(ID, 2).unwrap()[U3].revised_after_ai);
    // The review still compares the frozen reference, not the edits.
    let review = s.review(ID).unwrap();
    let item = &review
        .items
        .iter()
        .find(|e| e.item.unit_id == U3)
        .unwrap()
        .item;
    assert_eq!(item.human, set(&["ORG-SKILLS"]));
    assert_eq!(item.human_current, set(&["ORG-SKILLS"]));
    assert!(
        !item.revised_since_snapshot,
        "codes came back to the snapshot value"
    );
}

#[test]
fn guard_reports_missing_units() {
    let (s, _d) = ready();
    // Automatic phase 1: complete as soon as the segmentation is saved.
    let g = s.guard(ID, 1).unwrap();
    assert!(g.complete);
    assert!(g.missing.is_empty());
    phase1(&s);
    assert!(s.guard(ID, 1).unwrap().complete);
    let g = s.guard(ID, 2).unwrap();
    assert_eq!(g.missing, [U2, U3]);
    s.create_to_clarify(ID, U3, "skills gap at the integrator", "", ACTOR)
        .unwrap();
    assert_eq!(
        s.guard(ID, 2).unwrap().missing,
        [U2],
        "an open label counts as coded"
    );
}

// ------------------------------------------------------------ to clarify

#[test]
fn to_clarify_create_update_and_resolve_to_a_new_code() {
    let (s, _d) = ready();
    phase1(&s);
    assert_eq!(
        kind(&s.create_to_clarify(ID, U1, "x", "", ACTOR).unwrap_err()),
        "validation",
        "U001 is NA"
    );
    assert_eq!(
        kind(&s.create_to_clarify(ID, U2, " ", "", ACTOR).unwrap_err()),
        "validation"
    );
    let t = s
        .create_to_clarify(ID, U2, "law requires it", "said twice", ACTOR)
        .unwrap();
    // Empty codes are fine while the label is open.
    s.set_human_coding(ID, 2, &hc(U2, &[]), ACTOR).unwrap();
    let t = s
        .update_to_clarify(t.id, "legal obligation", "said twice", ACTOR)
        .unwrap();
    assert_eq!(t.label, "legal obligation");
    let req = ResolveRequest {
        kind: ResolutionKind::NewCode,
        code: "LEGAL-OBLIGATION".into(),
        rationale: Some("Recurs in two interviews.".into()),
        all_with_label: false,
        new_code: Some(new_code("LEGAL-OBLIGATION", Some("E"))),
    };
    let done = s.resolve_to_clarify(t.id, &req, ACTOR).unwrap();
    assert_eq!(done.len(), 1);
    let r = done[0].resolved.as_ref().unwrap();
    assert_eq!(
        (r.kind, r.code.as_str()),
        (ResolutionKind::NewCode, "LEGAL-OBLIGATION")
    );
    let code = s.code_row("LEGAL-OBLIGATION").unwrap().unwrap();
    assert_eq!(code.origin, "field");
    assert!(code.reason.unwrap().contains("legal obligation"));
    assert_eq!(
        s.human_coding_map(ID, 2).unwrap()[U2],
        set(&["LEGAL-OBLIGATION"])
    );
    assert_eq!(
        kind(&s.resolve_to_clarify(t.id, &req, ACTOR).unwrap_err()),
        "validation",
        "already resolved"
    );
    assert_eq!(
        kind(&s.update_to_clarify(t.id, "y", "", ACTOR).unwrap_err()),
        "validation"
    );
    let e = s
        .connection()
        .execute("DELETE FROM to_clarify_resolutions", [])
        .unwrap_err();
    assert!(e.to_string().contains("append-only"));
    assert!(actions(&s).contains(&"to_clarify.resolve".to_string()));
}

#[test]
fn to_clarify_maps_every_item_with_the_same_label() {
    let (s, _d) = ready();
    phase1(&s);
    s.set_human_coding(ID, 2, &hc(U2, &["NA"]), ACTOR).unwrap();
    let a = s
        .create_to_clarify(ID, U2, "Skills gap", "", ACTOR)
        .unwrap();
    s.create_to_clarify(ID, U3, " skills GAP ", "", ACTOR)
        .unwrap();
    s.create_to_clarify(ID, U3, "other", "", ACTOR).unwrap();
    let req = ResolveRequest {
        kind: ResolutionKind::Mapped,
        code: "ORG-SKILLS".into(),
        rationale: None,
        all_with_label: true,
        new_code: None,
    };
    let done = s.resolve_to_clarify(a.id, &req, ACTOR).unwrap();
    assert_eq!(done.len(), 2);
    let map = s.human_coding_map(ID, 2).unwrap();
    assert_eq!(map[U2], set(&["ORG-SKILLS"]), "NA replaced");
    assert_eq!(map[U3], set(&["ORG-SKILLS"]));
    assert_eq!(s.to_clarify_items(Some(ID), true).unwrap().len(), 1);
    let bad = ResolveRequest {
        code: "TECH".into(),
        ..req
    };
    let other = s.to_clarify_items(Some(ID), true).unwrap()[0].id;
    assert_eq!(
        kind(&s.resolve_to_clarify(other, &bad, ACTOR).unwrap_err()),
        "validation",
        "legacy code"
    );
}

// ------------------------------------------------------------- AI and review

#[test]
fn prompts_get_a_batch_and_are_checked() {
    let (s, _d) = ready();
    coded(&s);
    let b1 = batch(&s);
    assert_eq!(b1[0].batch_id, 1);
    let b2 = s
        .record_prompts(
            &[
                part(&s, &[U2], &OFFERED, 1, 2),
                part(&s, &[U3], &OFFERED, 2, 2),
            ],
            ACTOR,
        )
        .unwrap();
    assert!(b2.iter().all(|p| p.batch_id == 2));
    assert_eq!(s.prompts(ID).unwrap().len(), 3);
    let mut bad = part(&s, &[U2], &OFFERED, 1, 1);
    bad.sha256 = "x".into();
    assert_eq!(
        kind(&s.record_prompts(&[bad], ACTOR).unwrap_err()),
        "validation"
    );
}

#[test]
fn runs_need_a_batch_and_unique_labels() {
    let (s, _d) = ready();
    coded(&s);
    assert_eq!(
        kind(&s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap_err()),
        "validation",
        "no prompt yet"
    );
    batch(&s);
    s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    assert_eq!(
        kind(&s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap_err()),
        "validation"
    );
    let mut m = meta("R2");
    m.batch_id = Some(9);
    assert_eq!(
        kind(&s.create_ai_run(ID, &m, ACTOR).unwrap_err()),
        "not_found"
    );
    let mut m = meta("R2");
    m.model_name = " ".into();
    assert_eq!(
        kind(&s.create_ai_run(ID, &m, ACTOR).unwrap_err()),
        "validation"
    );
    let runs = s.ai_runs(ID).unwrap();
    assert_eq!(runs[0].batch_id, 1);
    assert_eq!(runs[0].unit_ids, [U2, U3]);
    assert!(!runs[0].complete);
}

/// Blind coding guard. Since C2 the refusal happens at prompt generation: a
/// batch cannot be generated while a unit of it is uncoded (nothing is
/// written). The import then refuses any answer that does not cover exactly
/// the units frozen in the batch.
#[test]
fn blind_guard_blocks_import_until_the_part_is_coded() {
    let (s, _d) = ready();
    phase1(&s);
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    let p = part(&s, &[U2, U3], &OFFERED, 1, 1);
    let e = s
        .record_prompts(std::slice::from_ref(&p), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    assert!(e.to_string().contains(U3), "{e}");
    assert!(s.prompts(ID).unwrap().is_empty());
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    s.record_prompts(&[p], ACTOR).unwrap();
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    // An answer covering a unit absent from the batch (and so from the snapshot).
    let bad = output(&s, 1, 1, &[(U2, &["TECH-ADV"]), (U1, &["NA"])]);
    let e = s.record_ai_output(run, "raw", &bad, ACTOR).unwrap_err();
    assert!(matches!(kind(&e), "validation" | "guard"), "{e}");
    let out = output(&s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["NA"])]);
    s.record_ai_output(run, "raw", &out, ACTOR).unwrap();
}

#[test]
fn answers_are_checked_against_the_prompt_snapshot() {
    let (s, _d) = ready();
    coded(&s);
    s.record_prompts(
        &[
            part(&s, &[U2], &OFFERED, 1, 2),
            part(&s, &[U3], &OFFERED, 2, 2),
        ],
        ACTOR,
    )
    .unwrap();
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    // The codebook changes after the prompt: a new code exists but was not offered.
    s.create_code(&new_code("LEGAL-OBLIGATION", Some("E")), ACTOR)
        .unwrap();
    let e = s
        .record_ai_output(
            run,
            "raw",
            &output(&s, 1, 2, &[(U2, &["LEGAL-OBLIGATION"])]),
            ACTOR,
        )
        .unwrap_err();
    assert!(e.to_string().contains("not offered"), "{e}");
    // A code offered then deactivated is still valid for this prompt.
    s.set_code_active("OUT-REALISED", false, ACTOR).unwrap();
    s.record_ai_output(
        run,
        "raw",
        &output(&s, 1, 2, &[(U2, &["OUT-REALISED"])]),
        ACTOR,
    )
    .unwrap();
    for (out, what) in [
        (output(&s, 1, 2, &[(U2, &["TECH-ADV"])]), "already imported"),
        (output(&s, 2, 3, &[(U3, &["NA"])]), "parts"),
        (output(&s, 2, 2, &[(U2, &["NA"])]), "exactly"),
        (output(&s, 2, 2, &[(U3, &["NA", "TECH-ADV"])]), "NA"),
        (output(&s, 3, 3, &[(U3, &["NA"])]), "no part 3"),
    ] {
        let e = s.record_ai_output(run, "raw", &out, ACTOR).unwrap_err();
        assert!(e.to_string().contains(what), "{what}: {e}");
    }
    let mut other = output(&s, 2, 2, &[(U3, &["NA"])]);
    other.segmentation_hash = "old".into();
    assert_eq!(
        kind(&s.record_ai_output(run, "raw", &other, ACTOR).unwrap_err()),
        "guard"
    );
    s.record_ai_output(run, "raw 2", &output(&s, 2, 2, &[(U3, &["NA"])]), ACTOR)
        .unwrap();
    let r = s.ai_run(run).unwrap();
    assert!(r.complete);
    assert_eq!(r.parts_received, [1, 2]);
    assert_eq!(s.ai_raw_outputs(run).unwrap()[1].1, "raw 2");
}

#[test]
fn review_lists_disagreements_and_a_deterministic_sample() {
    let (s, _d) = ready();
    let run = challenged(&s);
    let review = s.review(ID).unwrap();
    assert_eq!(review.runs, ["R1"]);
    assert_eq!(review.n_disagreements, 1);
    assert_eq!(review.items[0].item.unit_id, U3);
    assert_eq!(review.items[0].item.kind, ReviewKind::Disagreement);
    assert_eq!(review.items[0].item.human, set(&["ORG-SKILLS"]));
    assert_eq!(review.items[0].item.human_current, set(&["ORG-SKILLS"]));
    assert!(!review.items[0].item.revised_since_snapshot);
    // One agreeing unit: the sample takes it (max(5, 10%) capped at 1).
    assert_eq!(review.n_sampled, 1);
    assert_eq!(review.items[1].item.unit_id, U2);
    assert_eq!(review.items[1].item.kind, ReviewKind::AgreementSample);
    assert_eq!(
        review.items[1].item.human_current,
        review.items[1].item.human
    );
    assert!(!review.items[1].item.revised_since_snapshot);
    assert_eq!(review.snapshots.len(), 1);
    let seg_hash = s.segmentation(ID).unwrap().unwrap().hash;
    assert_eq!(
        review.seeds,
        [(
            "R1".to_string(),
            cs_model::reconcile::sample_seed(run, &seg_hash)
        )]
    );
    let ev = s
        .audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .find(|e| e.action == "review.sample")
        .unwrap();
    assert_eq!(ev.detail["sample"], json!([U2]));
    let e = s
        .connection()
        .execute("UPDATE review_samples SET seed = 'x'", [])
        .unwrap_err();
    assert!(e.to_string().contains("append-only"));
}

#[test]
fn review_decisions_and_final_coding() {
    let (s, _d) = ready();
    challenged(&s);
    let e = s
        .record_decision(
            ID,
            &decision(U3, ArbitrationChoice::Human, &["ORG-SKILLS"], " "),
            ACTOR,
        )
        .unwrap_err();
    assert!(e.to_string().contains("rationale"), "{e}");
    let e = s
        .record_decision(
            ID,
            &decision(U1, ArbitrationChoice::Human, &["NA"], "x"),
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "validation", "not a review item");
    let e = s
        .record_decision(
            ID,
            &decision(
                U3,
                ArbitrationChoice::Ai {
                    run_label: "R9".into(),
                },
                &["NA"],
                "x",
            ),
            ACTOR,
        )
        .unwrap_err();
    assert!(e.to_string().contains("R9"));
    let e = s
        .record_decision(
            ID,
            &decision(U3, ArbitrationChoice::Custom, &["TECH"], "x"),
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    s.record_decision(
        ID,
        &decision(
            U3,
            ArbitrationChoice::Ai {
                run_label: "R1".into(),
            },
            &["ENV-PARTNER"],
            "The partner mandate drives it.",
        ),
        ACTOR,
    )
    .unwrap();
    // A sampled agreement may be confirmed without a note.
    s.record_decision(
        ID,
        &decision(U2, ArbitrationChoice::Human, &["TECH-ADV"], ""),
        ACTOR,
    )
    .unwrap();
    s.record_decision(
        ID,
        &decision(
            U3,
            ArbitrationChoice::Custom,
            &["ENV-PARTNER", "ORG-SKILLS"],
            "Both, on re-reading.",
        ),
        ACTOR,
    )
    .unwrap();
    assert_eq!(s.decision_history(ID).unwrap().len(), 3);
    let items = s.review(ID).unwrap().items;
    assert!(items
        .iter()
        .all(|e| !e.item.revised_since_snapshot && e.item.human_current == e.item.human));
    let (finals, uncoded) = s.final_codings(ID).unwrap();
    assert!(uncoded.is_empty());
    assert_eq!(finals[1].codes, set(&["ENV-PARTNER", "ORG-SKILLS"]));
    assert_eq!(finals[1].basis, cs_model::FinalBasis::Reviewed);
    let sum = s.phase_summary(ID, 2).unwrap();
    assert!(sum.reconciled);
    assert_eq!(
        (sum.n_divergences, sum.n_sampled, sum.n_arbitrated),
        (1, 1, 2)
    );
}

#[test]
fn summaries_report_both_phases() {
    let (s, _d) = ready();
    challenged(&s);
    let sum = &s.summaries().unwrap()[0];
    assert_eq!(sum.phase1.n_units, 3);
    assert!(sum.phase1.human_complete);
    assert_eq!(sum.phase2.n_units, 2);
    assert_eq!(sum.phase2.n_ai_runs_complete, 1);
    assert_eq!(sum.phase2.n_divergences, 1);
    assert!(!sum.phase2.reconciled);
}

// -------------------------------------------------------------------- cases

fn field(text: &str, evidence: &[&str]) -> CaseField {
    CaseField {
        text: text.into(),
        contradictions: String::new(),
        missing: String::new(),
        evidence: evidence.iter().map(|s| s.to_string()).collect(),
    }
}

fn case(label: &str) -> Case {
    Case {
        case_id: String::new(),
        interview_id: String::new(),
        label: label.into(),
        trigger: field("Shrink in the stores", &[U2]),
        alternatives: field("Barcodes", &[]),
        constraints: field("No skills in house", &[U3]),
        choice: field("Pilot with a partner", &[]),
        result: field("Paid back in a year", &[U2, U2]),
        units: vec![U2.into(), U3.into()],
        interviews: vec![],
        decision: None,
        granularity: None,
        carrier_chosen: None,
        carriers_rejected: vec![],
        account_type: None,
        swiss_link: None,
        stage: Some("STAGE-PILOT".into()),
        outcome_status: None,
        notes: None,
        outcome: None,
        characteristics: Default::default(),
        passages: vec![],
        updated_at: String::new(),
    }
}

#[test]
fn cases_crud_with_evidence_validation() {
    let (s, _d) = ready();
    let c = s.create_case(ID, &case("Store pilot"), ACTOR).unwrap();
    assert_eq!(c.case_id, "ECO-16-C1");
    assert_eq!(c.result.evidence, [U2], "duplicates removed");
    let c2 = s.create_case(ID, &case("Second project"), ACTOR).unwrap();
    assert_eq!(c2.case_id, "ECO-16-C2");
    let mut bad = case("x");
    bad.choice.evidence = vec!["ECO-16-U999".into()];
    assert!(s
        .create_case(ID, &bad, ACTOR)
        .unwrap_err()
        .to_string()
        .contains("U999"));
    assert_eq!(
        kind(&s.create_case(ID, &case(" "), ACTOR).unwrap_err()),
        "validation"
    );
    let mut edit = c.clone();
    edit.outcome_status = Some("STATUS-STALLED".into());
    let after = s.update_case(&c.case_id, &edit, ACTOR).unwrap();
    assert_eq!(after.outcome_status.as_deref(), Some("STATUS-STALLED"));
    assert_eq!(s.cases(Some(ID)).unwrap().len(), 2);
    assert_eq!(
        kind(&s.update_case("NOPE", &edit, ACTOR).unwrap_err()),
        "not_found"
    );
    let ev = s.audit_events(Some(ID)).unwrap().last().unwrap().clone();
    assert_eq!(ev.action, "case.update");
}

#[test]
fn cross_case_cells_follow_the_question_mapping() {
    let (s, _d) = ready();
    coded(&s);
    let mut narrow = case("Store pilot");
    narrow.units = vec![U2.into()];
    narrow.constraints.evidence.clear();
    s.create_case(ID, &narrow, ACTOR).unwrap();
    let mut wide = case("Both units");
    wide.units = vec![U2.into(), U3.into()];
    s.create_case(ID, &wide, ACTOR).unwrap();
    let mut empty = case("Undocumented");
    empty.units = vec![];
    for f in [
        &mut empty.trigger,
        &mut empty.alternatives,
        &mut empty.constraints,
        &mut empty.choice,
        &mut empty.result,
    ] {
        f.evidence.clear();
    }
    s.create_case(ID, &empty, ACTOR).unwrap();
    let cc = s.cross_case().unwrap();
    assert_eq!(cc.questions.len(), 4);
    let cell = |rq: &str, case_id: &str| {
        cc.cells
            .iter()
            .find(|c| c.rq == rq && c.case_id == case_id)
            .unwrap()
            .clone()
    };
    assert_eq!(cell("SQ1", "ECO-16-C1").codes[0].code, "TECH-ADV");
    assert_eq!(cell("SQ1", "ECO-16-C1").codes[0].units, [U2]);
    assert!(cell("SQ1", "ECO-16-C1").documented);
    assert!(
        cell("SQ2", "ECO-16-C1").codes.is_empty(),
        "U003 is not attributed to C1"
    );
    assert!(cell("SQ2", "ECO-16-C1").documented);
    assert_eq!(cell("SQ2", "ECO-16-C2").codes[0].code, "ORG-SKILLS");
    assert_eq!(cell("SQ2", "ECO-16-C2").codes[0].units, [U3]);
    assert!(cell("SQ3", "ECO-16-C2").codes.is_empty());
    assert!(cell("SQ3", "ECO-16-C2").documented);
    for rq in ["SQ1", "SQ2", "SQ3", "SQ4"] {
        let c = cell(rq, "ECO-16-C3");
        assert!(!c.documented, "no attributed unit: not documented");
        assert!(c.codes.is_empty());
    }
}

// -------------------------------------------------------------------- audit

#[test]
fn append_only_tables_refuse_update_and_delete() {
    let (s, _d) = ready();
    challenged(&s);
    s.record_decision(
        ID,
        &decision(U3, ArbitrationChoice::Human, &["ORG-SKILLS"], "x"),
        ACTOR,
    )
    .unwrap();
    s.create_to_clarify(ID, U2, "a label", "", ACTOR).unwrap();
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
        "DELETE FROM review_samples",
        "UPDATE human_snapshots SET sha256 = 'x'",
        "DELETE FROM human_snapshots",
        "DELETE FROM to_clarify",
    ] {
        let e = c.execute(sql, []).unwrap_err();
        assert!(e.to_string().contains("append-only"), "{sql}: {e}");
    }
}

#[test]
fn resegmentation_allowed_with_candidate_coding() {
    let (s, _d) = ready();
    s.save_segmentation(&segmentation(&["One.", "Two."]), ACTOR)
        .unwrap();
    assert_eq!(s.segmentation(ID).unwrap().unwrap().units.len(), 2);
    s.set_human_coding(ID, 1, &hc(U1, &["NA"]), ACTOR).unwrap();
    s.save_segmentation(&segmentation(&["A.", "B.", "C."]), ACTOR)
        .unwrap();
    assert_eq!(s.segmentation(ID).unwrap().unwrap().units.len(), 3);
    assert_eq!(s.human_codings(ID, 1).unwrap()[U1].codes, set(&["NA"]));
}

#[test]
fn write_and_audit_are_atomic() {
    // A failing domain write leaves neither a row nor an event.
    let (s, _d) = ready();
    let n = actions(&s).len();
    let before = s.human_coding_map(ID, 1).unwrap();
    let _ = s.set_human_coding(ID, 1, &hc(U2, &["UNKNOWN"]), ACTOR);
    assert_eq!(actions(&s).len(), n);
    assert_eq!(s.human_coding_map(ID, 1).unwrap(), before);
}

#[test]
fn chain_verifies_and_detects_tampering() {
    let (s, _d) = ready();
    phase1(&s);
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

// v4 modules (dev graph 12.10.2026); each may use the fixtures above through `super::*`.
mod case_fields_v2;
mod cases_v4;
mod claims_v4;
mod codebook_import_v5;
mod codes_from_passage_v1;
mod codes_v1;
mod lexicon_v1;
mod memos_saturation_v4;
mod passages_v1;
#[allow(clippy::field_reassign_with_default)]
mod project_v1;
mod simple_v5;
mod snapshots_v4;
#[allow(clippy::field_reassign_with_default)]
mod study_config_v5;
mod verification_v4;

// V2 (codes and search).
mod v2_code_ops;
mod v2_code_search;
mod v2_code_stats;
mod v2_common;
mod v2_cooccurrence;
mod v2_fulltext;
mod v2_lists_f09;

// V1 lot 5 (Analyse and Write) and the API gaps of the same lot.
mod l5_decisions;
mod l5_gaps;
mod l5_matrix;
mod l5_schema;
mod l5_study_text;
mod l5_themes;
mod l5_writing;
mod writeup_doc_v2;

// V1 lot 4 (Review).
mod lot4_review;

// V2: versioned transcripts and projection of passages (F-24).
mod audio_v2;
mod transcripts_v2;
