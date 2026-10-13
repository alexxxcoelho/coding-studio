//! Gap 3: `description` and `coding_rules` in the study configuration.

use super::*;
use cs_model::config::STUDY_CONFIG_KEY;
use cs_model::StudyConfig;

#[test]
fn description_and_coding_rules_are_stored_and_audited() {
    let (s, _d) = ready();
    let cfg = StudyConfig {
        study_name: "Petites bibliotheques".into(),
        description: "Pourquoi une petite bibliotheque change de systeme de pret.".into(),
        coding_rules: "Un code par idee distincte.".into(),
        ..StudyConfig::default()
    };
    let stored = s.set_study_config(&cfg, ACTOR).unwrap();
    assert_eq!(stored.description, cfg.description);
    assert_eq!(stored.coding_rules, cfg.coding_rules);
    let again = s.study_config().unwrap();
    assert_eq!(again.description, cfg.description);
    // the prompt template is untouched
    assert_eq!(
        again.prompt_template,
        StudyConfig::default().prompt_template
    );
    let ev = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .rev()
        .find(|e| e.action == "config.update")
        .unwrap();
    assert_eq!(ev.detail["before"]["description"], "");
    assert_eq!(ev.detail["after"]["description"], cfg.description);
}

#[test]
fn a_configuration_saved_before_the_fields_existed_keeps_its_text() {
    let (s, _d) = ready();
    // What an earlier version stored: no description, no coding_rules, a
    // customised prompt header and footer.
    let mut old = serde_json::to_value(StudyConfig::default()).unwrap();
    let map = old.as_object_mut().unwrap();
    map.remove("description");
    map.remove("coding_rules");
    map["prompt_template"]["header"] = "Etude sur le pret en bibliotheque".into();
    map["prompt_template"]["footer"] = "1. Un code par idee.".into();
    s.connection()
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, 't')",
            rusqlite::params![STUDY_CONFIG_KEY, old.to_string()],
        )
        .unwrap();
    let cfg = s.study_config().unwrap();
    assert_eq!(cfg.description, "Etude sur le pret en bibliotheque");
    assert_eq!(cfg.coding_rules, "1. Un code par idee.");
    assert_eq!(
        cfg.prompt_template.header, "Etude sur le pret en bibliotheque",
        "prompt_template is kept as it was"
    );
    // Once saved, the text is the researcher's: clearing it sticks.
    let cleared = StudyConfig {
        description: String::new(),
        coding_rules: String::new(),
        ..cfg
    };
    s.set_study_config(&cleared, ACTOR).unwrap();
    assert_eq!(s.study_config().unwrap().description, "");
}

#[test]
fn a_default_configuration_has_empty_texts() {
    let (s, _d) = ready();
    let cfg = s.study_config().unwrap();
    assert_eq!(cfg.description, "");
    assert_eq!(cfg.coding_rules, "");
}
