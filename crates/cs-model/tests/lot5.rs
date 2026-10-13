//! V1 lot 5, model side: description and coding rules of the study
//! configuration, closed lists of the analysis.

use cs_model::analysis::*;
use cs_model::{PromptTemplate, StudyConfig};

#[test]
fn the_new_study_fields_default_to_empty_and_old_json_still_reads() {
    let cfg = StudyConfig::default();
    assert_eq!(cfg.description, "");
    assert_eq!(cfg.coding_rules, "");
    let mut v = serde_json::to_value(&cfg).unwrap();
    v.as_object_mut().unwrap().remove("description");
    v.as_object_mut().unwrap().remove("coding_rules");
    let back: StudyConfig = serde_json::from_value(v).unwrap();
    assert_eq!(back, cfg);
}

#[test]
fn a_customised_header_and_footer_move_to_the_empty_fields_only() {
    let mut cfg = StudyConfig::default();
    cfg.prompt_template.header = "Etude sur les petites bibliotheques".into();
    cfg.prompt_template.footer = "1. Un code par idee.".into();
    cfg.migrate_legacy_text();
    assert_eq!(cfg.description, "Etude sur les petites bibliotheques");
    assert_eq!(cfg.coding_rules, "1. Un code par idee.");
    // the template itself is kept
    assert_eq!(
        cfg.prompt_template.header,
        "Etude sur les petites bibliotheques"
    );

    let mut kept = StudyConfig {
        description: "Deja ecrit".into(),
        ..StudyConfig::default()
    };
    kept.prompt_template.header = "Autre texte".into();
    kept.migrate_legacy_text();
    assert_eq!(kept.description, "Deja ecrit");
}

#[test]
fn the_shipped_prompt_is_not_copied_into_the_description() {
    let mut cfg = StudyConfig::default();
    cfg.migrate_legacy_text();
    assert_eq!(cfg.description, "");
    assert_eq!(cfg.coding_rules, "");
    assert_eq!(cfg.prompt_template, PromptTemplate::default());
}

#[test]
fn overlong_description_is_refused() {
    let cfg = StudyConfig {
        description: "x".repeat(20_001),
        ..StudyConfig::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn the_closed_lists_match_the_guide() {
    assert_eq!(THEME_STATUSES, ["candidate", "revised", "retained"]);
    assert_eq!(CELL_VALUES.len(), 4);
    assert_eq!(THEME_PARTS.len(), 5);
    assert_eq!(audit_verdict_label("supported"), "étayée");
    assert_eq!(audit_verdict_label("to_nuance"), "à nuancer");
    assert_eq!(audit_verdict_label("to_withdraw"), "à retirer");
    assert_eq!(passage_origin_label("lived_experience"), "expérience vécue");
    assert_eq!(decision_outcome_label("defer"), "différer");
    assert_eq!(cell_value_label("not_mentioned"), "non évoqué");
}
