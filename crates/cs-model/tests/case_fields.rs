use cs_model::project::*;

fn field(id: &str, kind: AttributeKind, choices: &[&str]) -> CaseFieldDef {
    CaseFieldDef {
        id: id.into(),
        label: id.into(),
        kind,
        choices: choices.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn case_fields_are_validated_like_attributes() {
    let mut c = ProjectConfig::default();
    assert!(c.case_fields.is_empty(), "no field of any study by default");
    c.case_fields = vec![
        field("budget", AttributeKind::Number, &[]),
        field("scope", AttributeKind::Choice, &["local", "global"]),
    ];
    c.validate().unwrap();
    let f = c.case_field("scope").unwrap();
    assert_eq!(f.check_value(" local ").unwrap().as_deref(), Some("local"));
    assert!(f
        .check_value("galactic")
        .unwrap_err()
        .to_string()
        .contains("case field scope"));
    assert!(c.case_field("budget").unwrap().check_value("x").is_err());

    c.case_fields
        .push(field("budget", AttributeKind::Text, &[]));
    c.case_fields
        .push(field("Bad Id", AttributeKind::Text, &[]));
    c.case_fields
        .push(field("empty", AttributeKind::Choice, &[]));
    c.case_fields
        .push(field("odd", AttributeKind::Text, &["a"]));
    let problems = c.problems();
    assert!(problems.iter().any(|p| p.contains("duplicate id budget")));
    assert!(problems.iter().any(|p| p.contains("Bad Id")));
    assert!(problems.iter().any(|p| p.contains("a choice needs values")));
    assert!(problems.iter().any(|p| p.contains("only a choice field")));
}

#[test]
fn a_config_without_case_fields_still_reads() {
    let c: ProjectConfig = serde_json::from_str("{}").unwrap();
    assert!(c.case_fields.is_empty());
}
