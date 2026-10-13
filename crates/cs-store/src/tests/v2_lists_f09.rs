//! V2, F-09: the origin and role lists of the qualification of a passage
//! live in the project configuration, with the earlier fixed values as
//! defaults. Data already stored stays valid.

use super::*;
use crate::library::PassageQualify;
use crate::passages::NewPassage;
use cs_model::passage::OffsetUnit;
use cs_model::project::*;

fn passage(s: &Store) -> String {
    s.create_passage(
        &NewPassage {
            interview_id: ID.into(),
            unit_id: U2.into(),
            start: None,
            end: None,
            offset_unit: OffsetUnit::Scalar,
            text: None,
        },
        ACTOR,
    )
    .unwrap()
    .0
    .passage_id
}

fn q(origin: Option<&str>, role: Option<&str>) -> PassageQualify {
    PassageQualify {
        origin: Some(origin.map(str::to_string)),
        role: Some(role.map(str::to_string)),
    }
}

fn item(id: &str, label: &str) -> ListItem {
    ListItem {
        id: id.into(),
        label: label.into(),
    }
}

#[test]
fn defaults_are_the_earlier_fixed_values() {
    let cfg = ProjectConfig::default();
    let origins: Vec<&str> = cfg.passage_origins.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        origins,
        ["lived_experience", "project", "opinion", "document"]
    );
    assert_eq!(cfg.passage_origins[0].label, "expérience vécue");
    let roles: Vec<&str> = cfg.passage_roles.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(roles, cs_model::CLAIM_ROLES);
    assert_eq!(cfg.passage_roles[0].label, "déclencheur");
}

#[test]
fn a_configuration_stored_before_the_lists_gets_the_defaults() {
    let s = Store::open_in_memory().unwrap();
    s.set_setting(
        "project_config",
        r#"{"attributes":[],"checklist":[],"theme_checks":[]}"#,
        ACTOR,
    )
    .unwrap();
    let cfg = s.project_config().unwrap();
    assert_eq!(cfg.passage_origins.len(), 4);
    assert_eq!(cfg.passage_roles.len(), 5);
}

#[test]
fn qualification_uses_the_configured_lists() {
    let (s, _d) = ready();
    let pid = passage(&s);
    let err = s.qualify_passage(&pid, &q(Some("hearsay"), None), ACTOR);
    assert!(err.is_err());
    let mut cfg = s.project_config().unwrap();
    cfg.passage_origins.push(item("hearsay", "ouï-dire"));
    cfg.passage_roles.push(item("amplifier", "amplificateur"));
    s.set_project_config(&cfg, ACTOR).unwrap();
    let p = s
        .qualify_passage(&pid, &q(Some("hearsay"), Some("amplifier")), ACTOR)
        .unwrap();
    assert_eq!(p.origin.as_deref(), Some("hearsay"));
    assert_eq!(p.role.as_deref(), Some("amplifier"));
    // The earlier values still work.
    s.qualify_passage(&pid, &q(Some("opinion"), Some("trigger")), ACTOR)
        .unwrap();
}

#[test]
fn a_value_in_use_cannot_be_dropped_but_an_unused_one_can() {
    let (s, _d) = ready();
    let pid = passage(&s);
    s.qualify_passage(&pid, &q(Some("project"), Some("constraint")), ACTOR)
        .unwrap();
    let mut cfg = s.project_config().unwrap();
    cfg.passage_origins.retain(|i| i.id != "project");
    let err = s.set_project_config(&cfg, ACTOR).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<CsError>(),
        Some(CsError::Guard(_))
    ));
    let mut cfg = s.project_config().unwrap();
    cfg.passage_roles.retain(|i| i.id != "constraint");
    assert!(s.set_project_config(&cfg, ACTOR).is_err());
    let mut cfg = s.project_config().unwrap();
    cfg.passage_origins.retain(|i| i.id != "document");
    cfg.passage_roles.retain(|i| i.id != "trigger");
    s.set_project_config(&cfg, ACTOR).unwrap();
}

#[test]
fn the_lists_are_validated_like_the_other_lists() {
    let mut cfg = ProjectConfig::default();
    cfg.passage_origins.push(item("project", "autre"));
    assert!(cfg.validate().is_err());
    let mut cfg = ProjectConfig::default();
    cfg.passage_roles.push(item("Bad Id", "x"));
    assert!(cfg.validate().is_err());
    let mut cfg = ProjectConfig::default();
    cfg.passage_origins.clear();
    cfg.validate().unwrap();
}

#[test]
fn the_template_carries_the_lists() {
    let (s, _d) = ready();
    let mut cfg = s.project_config().unwrap();
    cfg.passage_origins.push(item("hearsay", "ouï-dire"));
    s.set_project_config(&cfg, ACTOR).unwrap();
    let t = s.export_template().unwrap();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("hearsay"));
}
