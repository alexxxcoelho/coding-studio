//! Study configuration in the database (v5). Red until implemented.

use super::*;
use cs_model::{Category, FrameworkPreset, StudyConfig};

const KEY: &str = "study_config";

fn cat(name: &str) -> Category {
    Category {
        name: name.into(),
        label: format!("{name} label"),
        definition: format!("{name} definition"),
        attachment: None,
    }
}

fn custom(list: &[&str]) -> StudyConfig {
    StudyConfig {
        framework: FrameworkPreset::Custom,
        phase1_categories: list.iter().map(|n| cat(n)).collect(),
        study_name: "Pilot".into(),
        ..StudyConfig::default()
    }
}

fn config_events(s: &Store) -> Vec<AuditEvent> {
    s.audit_events(None)
        .unwrap()
        .into_iter()
        .filter(|e| e.action == "config.update")
        .collect()
}

fn names(book: &cs_model::Codebook) -> Vec<&str> {
    book.codes.iter().map(|c| c.code.as_str()).collect()
}

// ------------------------------------------------------- read and write

#[test]
fn unset_config_is_the_default() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(s.setting(KEY).unwrap(), None);
    assert_eq!(s.study_config().unwrap(), StudyConfig::default());
}

#[test]
fn set_config_stores_json_in_the_settings_table() {
    let (s, _d) = ready();
    let cfg = custom(&["ALPHA", "BETA"]);
    let stored = s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(stored, cfg.normalised());
    assert_eq!(s.study_config().unwrap(), stored);

    let raw = s.setting(KEY).unwrap().expect("stored under study_config");
    let parsed: StudyConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed, stored);
}

#[test]
fn a_preset_is_stored_normalised() {
    let (s, _d) = ready();
    let cfg = StudyConfig {
        framework: FrameworkPreset::TamUtaut,
        phase1_categories: vec![cat("STALE")],
        ..StudyConfig::default()
    };
    let stored = s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(stored.phase1_categories.len(), 5);
    assert!(stored.phase1_categories.iter().all(|c| c.name != "STALE"));
    assert_eq!(s.study_config().unwrap(), stored);
}

#[test]
fn invalid_config_is_refused_and_leaves_no_trace() {
    let (s, _d) = ready();
    let before_events = s.audit_events(None).unwrap().len();
    let mut cfg = StudyConfig::default();
    cfg.max_words = 5;
    let e = s.set_study_config(&cfg, ACTOR).unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");
    assert!(e.to_string().contains("max_words"), "{e}");

    let e = s
        .set_study_config(&custom(&["ALPHA", "NA"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");

    assert_eq!(s.setting(KEY).unwrap(), None);
    // Unchanged: the configuration in force when none is saved.
    assert!(s.setting(KEY).unwrap().is_none());
    assert_eq!(s.audit_events(None).unwrap().len(), before_events);
}

// ----------------------------------------------------------------- audit

#[test]
fn set_config_is_audited_with_before_and_after() {
    let (s, _d) = ready();
    let initial = s.study_config().unwrap();
    let cfg = custom(&["ALPHA", "BETA"]);
    let stored = s.set_study_config(&cfg, "alice").unwrap();

    let events = config_events(&s);
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(ev.actor, "alice");
    assert_eq!(ev.detail["before"], serde_json::to_value(&initial).unwrap());
    assert_eq!(ev.detail["after"], serde_json::to_value(&stored).unwrap());
    assert_eq!(ev.detail["force"], json!(false));
    assert!(s.verify_chain().unwrap().ok);

    // A second change chains from the first.
    let mut next = stored.clone();
    next.study_name = "Main study".into();
    s.set_study_config(&next, ACTOR).unwrap();
    let events = config_events(&s);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[1].detail["before"],
        serde_json::to_value(&stored).unwrap()
    );
    assert_eq!(events[1].detail["after"]["study_name"], "Main study");
}

#[test]
fn setting_the_same_config_again_writes_no_event() {
    let (s, _d) = ready();
    let cfg = custom(&["ALPHA", "BETA"]);
    s.set_study_config(&cfg, ACTOR).unwrap();
    s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(config_events(&s).len(), 1);
}

// ---------------------------------------------------- phase 1 categories

#[test]
fn default_phase1_codebook_is_the_current_one() {
    let (s, _d) = ready();
    assert_eq!(
        s.phase1_codebook().unwrap().codes,
        cs_model::phase::phase1_codebook().codes
    );
    let allowed = s.allowed_codes(ID, 1, U1).unwrap();
    let got: Vec<&str> = allowed.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(got, ["TECH", "ORG", "ENV", "CONTEXT"]);
}

#[test]
fn phase1_allowed_categories_come_from_the_config() {
    let (s, _d) = ready();
    s.set_study_config(&custom(&["ALPHA", "BETA"]), ACTOR)
        .unwrap();

    let book = s.phase1_codebook().unwrap();
    assert_eq!(names(&book), ["ALPHA", "BETA"]);
    assert!(book.codes.iter().all(|c| c.origin == "phase1"));
    assert_eq!(book.get("ALPHA").unwrap().label, "ALPHA label");
    assert_eq!(book.get("ALPHA").unwrap().definition, "ALPHA definition");

    let allowed = s.allowed_codes(ID, 1, U1).unwrap();
    let got: Vec<&str> = allowed.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(got, ["ALPHA", "BETA"]);

    // The configured categories are accepted, the former ones are not.
    s.set_human_coding(ID, 1, &hc(U1, &["NA"]), ACTOR).unwrap();
    s.set_human_coding(ID, 1, &hc(U2, &["ALPHA"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 1, &hc(U3, &["ALPHA", "BETA"]), ACTOR)
        .unwrap();
    let e = s
        .set_human_coding(ID, 1, &hc(U1, &["TECH"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");
    assert!(e.to_string().contains("TECH"), "{e}");

    // Phase 2 scope still follows "not NA".
    assert_eq!(s.phase_units(ID, 2).unwrap(), [U2, U3]);
}

#[test]
fn preset_without_framework_offers_context_only() {
    let (s, _d) = ready();
    let cfg = StudyConfig {
        framework: FrameworkPreset::None,
        ..StudyConfig::default()
    };
    s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(names(&s.phase1_codebook().unwrap()), ["CONTEXT"]);
    s.set_human_coding(ID, 1, &hc(U2, &["CONTEXT"]), ACTOR)
        .unwrap();
    let e = s
        .set_human_coding(ID, 1, &hc(U3, &["TECH"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");
}

#[test]
fn a_phase2_code_cannot_take_a_configured_category_name() {
    let (s, _d) = ready();
    s.set_study_config(&custom(&["ALPHA", "BETA"]), ACTOR)
        .unwrap();
    let e = s.create_code(&new_code("ALPHA", None), ACTOR).unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");
    assert!(e.to_string().contains("phase 1"), "{e}");
}

// ------------------------------------------------- change after coding

#[test]
fn categories_can_change_freely_before_any_phase1_coding() {
    let (s, _d) = ready(); // segmented, nothing coded
    s.set_study_config(&custom(&["ALPHA"]), ACTOR).unwrap();
    s.set_study_config(&custom(&["GAMMA", "DELTA"]), ACTOR)
        .unwrap();
    assert_eq!(names(&s.phase1_codebook().unwrap()), ["GAMMA", "DELTA"]);
}

#[test]
fn changing_categories_after_phase1_coding_is_refused_unless_forced() {
    let (s, _d) = ready();
    phase1(&s); // U1 NA, U2 TECH, U3 ORG
    let before = s.study_config().unwrap();
    let n_events = s.audit_events(None).unwrap().len();

    let e = s
        .set_study_config(&custom(&["ALPHA", "BETA"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard", "{e}");
    assert!(e.to_string().contains("force"), "{e}");
    assert_eq!(s.study_config().unwrap(), before);
    assert_eq!(s.setting(KEY).unwrap(), None);
    assert_eq!(s.audit_events(None).unwrap().len(), n_events);
    assert!(config_events(&s).is_empty());

    // Forced: applied and audited as such; the existing codings are kept.
    let cfg = custom(&["ALPHA", "BETA"]);
    let stored = s.set_study_config_with(&cfg, true, "alice").unwrap();
    assert_eq!(names(&s.phase1_codebook().unwrap()), ["ALPHA", "BETA"]);
    let events = config_events(&s);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor, "alice");
    assert_eq!(events[0].detail["force"], json!(true));
    assert_eq!(
        events[0].detail["after"],
        serde_json::to_value(&stored).unwrap()
    );
    assert_eq!(
        events[0].detail["before"],
        serde_json::to_value(&before).unwrap()
    );
    assert_eq!(s.human_codings(ID, 1).unwrap().len(), 3);
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn other_settings_stay_editable_after_coding() {
    let (s, _d) = ready();
    coded(&s);
    let mut cfg = s.study_config().unwrap();
    cfg.study_name = "Renamed".into();
    s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(s.study_config().unwrap().study_name, "Renamed");
    // Same categories, other template version: still not a category change.
    cfg.prompt_template.version = "CS-P04-EN v3.2".into();
    s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(
        s.study_config().unwrap().prompt_template.version,
        "CS-P04-EN v3.2"
    );
    assert_eq!(config_events(&s).len(), 2);
}

// ------------------------------------------------- neutral defaults (F-15)

#[test]
fn an_unset_configuration_is_neutral_for_a_new_database() {
    let s = Store::open_in_memory().unwrap();
    let cfg = s.study_config().unwrap();
    assert_eq!(cfg.id_prefixes, vec!["INT".to_string()]);
    assert!(!cfg.interviewers.iter().any(|n| n.contains("Dos Santos")));
}

#[test]
fn a_first_study_database_without_configuration_keeps_its_prefixes_and_interviewers() {
    let s = Store::open_in_memory().unwrap();
    s.connection()
        .execute(
            "INSERT INTO interviews VALUES ('ECO-16','f','/tmp/f',NULL,'[]','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    let cfg = s.study_config().unwrap();
    for p in ["INT", "ECO", "RET", "ADJ"] {
        assert!(cfg.id_prefixes.iter().any(|x| x == p), "{p}");
    }
    assert!(cfg.interviewers.iter().any(|n| n == "Dos Santos Alexandre"));
}
