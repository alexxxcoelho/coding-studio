#![allow(clippy::field_reassign_with_default, clippy::type_complexity)]
//! Behaviour of `StudyConfig` (generalisation, v5). Red until implemented.

use cs_model::phase::phase1_codebook;
use cs_model::{Category, CsError, FrameworkPreset, StudyConfig, MAX_WORDS};

fn cat(name: &str) -> Category {
    Category {
        name: name.into(),
        label: format!("{name} label"),
        definition: format!("{name} definition"),
        attachment: None,
    }
}

fn names(cats: &[Category]) -> Vec<&str> {
    cats.iter().map(|c| c.name.as_str()).collect()
}

fn custom(list: &[&str]) -> StudyConfig {
    StudyConfig {
        framework: FrameworkPreset::Custom,
        phase1_categories: list.iter().map(|n| cat(n)).collect(),
        ..StudyConfig::default()
    }
}

fn refused(cfg: &StudyConfig) -> String {
    match cfg.validate() {
        Err(CsError::Validation(m)) => m,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

// ------------------------------------------------------------ defaults

#[test]
fn default_config_is_neutral() {
    let cfg = StudyConfig::default();
    assert_eq!(cfg.framework, FrameworkPreset::Toe);
    assert_eq!(cfg.id_prefixes, ["INT"]);
    assert_eq!(cfg.max_words, MAX_WORDS);
    assert_eq!(cfg.max_words, 150);
    assert_eq!(
        cfg.interviewers,
        ["Interviewer", "Intervieweur", "Intervieweuse"]
    );
    assert_eq!(cfg.prompt_template.version, "CS-P04-EN v3.1");

    // The categories are today's phase 1 codebook.
    let book = phase1_codebook();
    assert_eq!(cfg.phase1_categories.len(), book.codes.len());
    for (c, code) in cfg.phase1_categories.iter().zip(&book.codes) {
        assert_eq!(c.name, code.code);
        assert_eq!(c.label, code.label);
        assert_eq!(c.definition, code.definition);
        assert_eq!(c.attachment, code.toe);
    }
    assert_eq!(
        names(&cfg.phase1_categories),
        ["TECH", "ORG", "ENV", "CONTEXT"]
    );
    assert!(cfg.validate().is_ok());
}

#[test]
fn default_template_parts_are_filled_and_free_of_long_dashes() {
    let t = StudyConfig::default().prompt_template;
    for part in [&t.header, &t.codebook_intro, &t.units_intro, &t.footer] {
        assert!(!part.trim().is_empty());
        assert!(!part.contains('\u{2014}') && !part.contains('\u{2013}'));
    }
}

#[test]
fn serde_round_trip_and_wire_names() {
    let cfg = StudyConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let back: StudyConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back, cfg);

    let v = serde_json::to_value(&cfg).unwrap();
    assert_eq!(v["framework"], "toe");
    assert_eq!(v["max_words"], 150);
    assert_eq!(v["prompt_template"]["version"], "CS-P04-EN v3.1");
    for key in [
        "phase1_categories",
        "id_prefixes",
        "interviewers",
        "study_name",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert!(v["phase1_categories"][0].get("attachment").is_some());

    for (preset, wire) in [
        (FrameworkPreset::Toe, "toe"),
        (FrameworkPreset::TamUtaut, "tam_utaut"),
        (FrameworkPreset::None, "none"),
        (FrameworkPreset::Custom, "custom"),
    ] {
        assert_eq!(serde_json::to_value(preset).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<FrameworkPreset>(wire.into()).unwrap(),
            preset
        );
    }
}

// ------------------------------------------------------------- presets

#[test]
fn toe_preset_is_the_four_toe_categories() {
    let cats = FrameworkPreset::Toe.categories(&[]);
    assert_eq!(names(&cats), ["TECH", "ORG", "ENV", "CONTEXT"]);
    let attach: Vec<Option<&str>> = cats.iter().map(|c| c.attachment.as_deref()).collect();
    assert_eq!(attach, [Some("T"), Some("O"), Some("E"), None]);
}

#[test]
fn tam_utaut_preset_has_the_constructs_and_context() {
    let cats = FrameworkPreset::TamUtaut.categories(&[]);
    assert_eq!(
        names(&cats),
        [
            "PERFORMANCE_EXPECTANCY",
            "EFFORT_EXPECTANCY",
            "SOCIAL_INFLUENCE",
            "FACILITATING_CONDITIONS",
            "CONTEXT"
        ]
    );
    assert!(cats
        .iter()
        .all(|c| !c.label.is_empty() && !c.definition.is_empty()));
}

#[test]
fn none_preset_is_context_only_and_ignores_the_custom_list() {
    let cats = FrameworkPreset::None.categories(&[cat("ALPHA")]);
    assert_eq!(names(&cats), ["CONTEXT"]);
}

#[test]
fn custom_preset_keeps_the_users_list() {
    let mine = vec![cat("ALPHA"), cat("BETA")];
    assert_eq!(FrameworkPreset::Custom.categories(&mine), mine);
    assert!(FrameworkPreset::Custom.categories(&[]).is_empty());
}

#[test]
fn effective_categories_and_normalisation_follow_the_preset() {
    // A preset other than Custom wins over a stale stored list.
    let stale = StudyConfig {
        framework: FrameworkPreset::TamUtaut,
        phase1_categories: vec![cat("ALPHA")],
        ..StudyConfig::default()
    };
    assert_eq!(names(&stale.effective_categories()).len(), 5);
    let n = stale.normalised();
    assert_eq!(n.phase1_categories, stale.effective_categories());
    assert_eq!(n.framework, FrameworkPreset::TamUtaut);
    assert_eq!(n.normalised(), n);

    let mine = custom(&["ALPHA", "BETA"]);
    assert_eq!(names(&mine.effective_categories()), ["ALPHA", "BETA"]);
    assert_eq!(mine.normalised(), mine);
    assert_eq!(StudyConfig::default().normalised(), StudyConfig::default());
}

// ---------------------------------------------------------- validation

#[test]
fn validation_refuses_empty_category_names() {
    assert!(refused(&custom(&["ALPHA", ""])).contains("empty"));
    assert!(refused(&custom(&["  "])).contains("empty"));
}

#[test]
fn validation_refuses_duplicate_category_names() {
    assert!(refused(&custom(&["ALPHA", "BETA", "ALPHA"])).contains("duplicate"));
}

#[test]
fn validation_refuses_a_category_named_na() {
    assert!(refused(&custom(&["ALPHA", "NA"])).contains("NA"));
}

#[test]
fn validation_refuses_max_words_below_twenty() {
    let mut cfg = StudyConfig::default();
    cfg.max_words = 19;
    assert!(refused(&cfg).contains("max_words"));
    cfg.max_words = 0;
    assert!(refused(&cfg).contains("max_words"));
    cfg.max_words = 20;
    assert!(cfg.validate().is_ok());
    cfg.max_words = 500;
    assert!(cfg.validate().is_ok());
}

#[test]
fn validation_refuses_bad_prefix_lists() {
    let mut cfg = StudyConfig::default();
    cfg.id_prefixes = vec![];
    assert!(refused(&cfg).contains("id_prefixes"));
    cfg.id_prefixes = vec!["ECO".into(), "".into()];
    assert!(refused(&cfg).contains("id_prefixes"));
    cfg.id_prefixes = vec!["E.O".into()];
    assert!(refused(&cfg).contains("id_prefixes"));
    cfg.id_prefixes = vec!["A|B".into()];
    assert!(refused(&cfg).contains("id_prefixes"));
}

#[test]
fn validation_accepts_the_presets_and_a_custom_list() {
    for preset in [
        FrameworkPreset::Toe,
        FrameworkPreset::TamUtaut,
        FrameworkPreset::None,
    ] {
        let cfg = StudyConfig {
            framework: preset,
            ..StudyConfig::default()
        }
        .normalised();
        assert!(cfg.validate().is_ok(), "{preset:?}");
    }
    assert!(custom(&["ALPHA", "BETA"]).validate().is_ok());
}

// ------------------------------------------------------------ id regex

#[test]
fn id_regex_matches_the_default_prefixes() {
    let re = StudyConfig::default().id_regex();
    let caps = re.captures("ECO33_Roman").expect("ECO33_Roman matches");
    assert_eq!(&caps[1], "ECO");
    assert_eq!(&caps[2], "33");
    let caps = re.captures("RET-22_von-Grone").unwrap();
    assert_eq!((&caps[1], &caps[2]), ("RET", "22"));
    let caps = re.captures("ECO35-Schwarz").unwrap();
    assert_eq!((&caps[1], &caps[2]), ("ECO", "35"));
}

#[test]
fn id_regex_refuses_unknown_prefixes_and_other_cases() {
    let re = StudyConfig::default().id_regex();
    assert!(!re.is_match("XYZ12_Someone"));
    assert!(!re.is_match("eco-05"));
    assert!(!re.is_match("Pasted clipping"));
    assert!(!re.is_match("_ECO33"), "anchored at the start");
}

#[test]
fn id_regex_follows_the_configured_prefixes() {
    let cfg = StudyConfig {
        id_prefixes: vec!["XYZ".into(), "AB".into()],
        ..StudyConfig::default()
    };
    let re = cfg.id_regex();
    let caps = re.captures("AB-7_Jane").unwrap();
    assert_eq!((&caps[1], &caps[2]), ("AB", "7"));
    assert!(re.is_match("XYZ01"));
    assert!(!re.is_match("ECO33_Roman"));
}
