use cs_model::project::*;

fn attr(id: &str, kind: AttributeKind, choices: &[&str]) -> AttributeDef {
    AttributeDef {
        id: id.into(),
        label: id.into(),
        kind,
        choices: choices.iter().map(|s| s.to_string()).collect(),
        shared_with_model: false,
    }
}

#[test]
fn the_defaults_are_valid_and_have_the_announced_sizes() {
    let c = ProjectConfig::default();
    c.validate().unwrap();
    assert_eq!(c.checklist.len(), 4);
    assert_eq!(c.theme_checks.len(), 7);
    assert_eq!(c.evidence_natures.len(), 5);
    let links: Vec<&str> = c.link_types.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(
        links,
        [
            "soutient",
            "nuance",
            "conditionne",
            "mécanisme",
            "exception",
            "contredit"
        ]
    );
    assert!(c.interpretive_framework.is_none());
    assert!(c.matrix_dimensions.is_empty());
    assert_eq!(c.default_template().unwrap().id, "default");
}

#[test]
fn no_default_text_has_a_long_dash_or_an_emoji() {
    let json = serde_json::to_string(&ProjectConfig::default()).unwrap();
    assert!(!json.contains('\u{2014}'));
    assert!(json.chars().all(|c| (c as u32) < 0x1F000));
}

#[test]
fn attribute_values_are_validated_by_type() {
    let n = attr("size", AttributeKind::Number, &[]);
    assert_eq!(n.check_value(" 12,5 ").unwrap().as_deref(), Some("12.5"));
    assert!(n.check_value("beaucoup").is_err());
    assert_eq!(n.check_value("  ").unwrap(), None);
    let d = attr("when", AttributeKind::Date, &[]);
    assert!(d.check_value("2026-02-29").is_err());
    assert!(d.check_value("2024-02-29").unwrap().is_some());
    assert!(d.check_value("12/03/2026").is_err());
    let c = attr("sector", AttributeKind::Choice, &["public", "privé"]);
    assert!(c.check_value("privé").unwrap().is_some());
    assert!(c.check_value("Privé").is_err());
    assert!(attr("note", AttributeKind::Text, &[])
        .check_value("libre")
        .is_ok());
}

#[test]
fn problems_are_all_reported_with_the_element_named() {
    let mut c = ProjectConfig::default();
    c.attributes = vec![
        attr("sector", AttributeKind::Choice, &[]),
        attr("sector", AttributeKind::Text, &[]),
        attr("Bad Id", AttributeKind::Text, &[]),
    ];
    c.checklist.push(ListItem {
        id: "turns_seen".into(),
        label: "x".into(),
    });
    let p = c.problems();
    assert!(p
        .iter()
        .any(|m| m.contains("sector") && m.contains("needs values")));
    assert!(p.iter().any(|m| m.contains("duplicate id sector")));
    assert!(p.iter().any(|m| m.contains("Bad Id")));
    assert!(p
        .iter()
        .any(|m| m.contains("checklist") && m.contains("duplicate id")));
    assert!(p.len() >= 4);
}

#[test]
fn exactly_one_default_template_and_questions_are_required() {
    let mut c = ProjectConfig::default();
    c.memo_templates[0].default = false;
    assert!(c.validate().is_err());
    let mut c = ProjectConfig::default();
    c.memo_templates[0].questions.clear();
    assert!(c.validate().is_err());
}

#[test]
fn framework_presets_and_custom_dimensions() {
    let toe = framework_preset("toe").unwrap();
    assert_eq!(toe.dimensions.len(), 3);
    assert_eq!(framework_preset("tam_utaut").unwrap().dimensions.len(), 6);
    assert!(framework_preset("nope").is_none());
    let mut c = ProjectConfig::default();
    c.interpretive_framework = Some(toe);
    c.validate().unwrap();
    c.interpretive_framework = Some(Framework {
        preset: "custom".into(),
        dimensions: vec![],
    });
    assert!(c.validate().is_err());
    c.interpretive_framework = Some(Framework {
        preset: "weird".into(),
        dimensions: framework_preset("toe").unwrap().dimensions,
    });
    assert!(c.validate().is_err());
}

#[test]
fn a_changed_template_gets_the_next_version_and_keeps_the_old_one() {
    let before = ProjectConfig::default();
    let mut edited = before.clone();
    edited.memo_templates[0].questions.push(MemoQuestion {
        id: "q5".into(),
        text: "Qui décide de quoi ?".into(),
    });
    let next = edited.versioned_after(&before);
    assert_eq!(next.memo_templates[0].version, 2);
    let old = next.template_at("default", 1).unwrap();
    assert_eq!(old.questions.len(), 4);
    assert_eq!(next.template_at("default", 2).unwrap().questions.len(), 5);
    // Saving the same content again does not bump.
    let again = next.clone().versioned_after(&next);
    assert_eq!(again.memo_templates[0].version, 2);
    assert_eq!(again.memo_history.len(), 1);
}
