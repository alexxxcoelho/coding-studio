//! V1 lot 2: attributes and their values, memo templates and per-interview
//! memos, the checklist, the project template. Acceptance tests A21 to A26
//! (store side).

use super::*;
use crate::{MemoOverrides, TemplateReport};
use cs_model::project::*;
use std::collections::BTreeMap;

fn attr(id: &str, kind: AttributeKind, choices: &[&str]) -> AttributeDef {
    AttributeDef {
        id: id.into(),
        label: id.into(),
        kind,
        choices: choices.iter().map(|s| s.to_string()).collect(),
        shared_with_model: false,
        scope: Default::default(),
    }
}

fn with_attrs(s: &Store) -> ProjectConfig {
    let mut cfg = s.project_config().unwrap();
    cfg.attributes = vec![
        attr("sector", AttributeKind::Choice, &["public", "privé"]),
        attr("size", AttributeKind::Number, &[]),
        attr("visit", AttributeKind::Date, &[]),
        attr("note", AttributeKind::Text, &[]),
    ];
    s.set_project_config(&cfg, ACTOR).unwrap()
}

fn set(s: &Store, pairs: &[(&str, Option<&str>)]) -> Result<crate::AttributeView> {
    let m: BTreeMap<String, Option<String>> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
        .collect();
    s.set_interview_attributes(ID, &m, ACTOR)
}

fn second_interview(s: &Store) {
    let mut e = default_entry();
    e.interview_id = "ECO-17".into();
    e.folder_name = "ECO-17_Lib".into();
    s.upsert_interview(&e, ACTOR).unwrap();
}

fn audit_actions(s: &Store) -> Vec<String> {
    s.audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect()
}

#[test]
fn unset_configuration_gives_the_defaults_and_a_save_of_them_is_silent() {
    let (s, _d) = ready();
    let cfg = s.project_config().unwrap();
    assert_eq!(cfg, ProjectConfig::default());
    let n = s.audit_events(None).unwrap().len();
    s.set_project_config(&cfg, ACTOR).unwrap();
    assert_eq!(
        s.audit_events(None).unwrap().len(),
        n,
        "no change, no event"
    );
}

#[test]
fn a_configuration_change_is_audited_with_before_and_after() {
    let (s, _d) = ready();
    with_attrs(&s);
    let ev = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .rev()
        .find(|e| e.action == "project_config.update")
        .unwrap();
    assert!(ev.detail["before"]["attributes"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        ev.detail["after"]["attributes"].as_array().unwrap().len(),
        4
    );
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn an_invalid_configuration_is_refused_and_nothing_is_written() {
    let (s, _d) = ready();
    let mut cfg = ProjectConfig::default();
    cfg.attributes = vec![attr("sector", AttributeKind::Choice, &[])];
    let err = s.set_project_config(&cfg, ACTOR).unwrap_err();
    assert!(err.to_string().contains("sector"));
    assert_eq!(s.project_config().unwrap(), ProjectConfig::default());
}

#[test]
fn a22_attribute_values_are_validated_stored_and_audited() {
    let (s, _d) = ready();
    with_attrs(&s);
    let v = set(
        &s,
        &[
            ("sector", Some("privé")),
            ("size", Some("12,5")),
            ("visit", Some("2026-03-04")),
        ],
    )
    .unwrap();
    assert_eq!(v.values["sector"], "privé");
    assert_eq!(v.values["size"], "12.5");
    assert!(set(&s, &[("sector", Some("autre"))]).is_err());
    assert!(set(&s, &[("size", Some("beaucoup"))]).is_err());
    assert!(set(&s, &[("visit", Some("2026-13-01"))]).is_err());
    assert!(set(&s, &[("nope", Some("x"))]).is_err());
    // A refused batch writes nothing, even its valid part.
    assert!(set(&s, &[("note", Some("ok")), ("size", Some("x"))]).is_err());
    assert!(!s
        .interview_attributes(ID)
        .unwrap()
        .values
        .contains_key("note"));
    // Clearing.
    let v = set(&s, &[("size", None)]).unwrap();
    assert!(!v.values.contains_key("size"));
    assert!(audit_actions(&s).contains(&"interview.attributes".to_string()));
    assert!(set(&s, &[("note", Some("x"))]).is_ok());
}

#[test]
fn attributes_of_an_unknown_interview_are_not_found() {
    let (s, _d) = ready();
    assert!(s.interview_attributes("NOPE").is_err());
}

#[test]
fn an_attribute_with_values_cannot_be_removed_retyped_or_lose_a_value_in_use() {
    let (s, _d) = ready();
    let cfg = with_attrs(&s);
    set(&s, &[("sector", Some("privé")), ("size", Some("3"))]).unwrap();
    let mut no_sector = cfg.clone();
    no_sector.attributes.retain(|a| a.id != "sector");
    let e = s.set_project_config(&no_sector, ACTOR).unwrap_err();
    assert!(e.to_string().contains("cannot be removed"), "{e}");
    let mut retyped = cfg.clone();
    retyped.attributes[1].kind = AttributeKind::Text;
    assert!(s.set_project_config(&retyped, ACTOR).is_err());
    let mut fewer = cfg.clone();
    fewer.attributes[0].choices = vec!["public".into()];
    assert!(s.set_project_config(&fewer, ACTOR).is_err());
    // An attribute without values can go, a choice not in use can go.
    let mut ok = cfg.clone();
    ok.attributes.retain(|a| a.id != "note");
    ok.attributes[0].choices = vec!["privé".into(), "mixte".into()];
    s.set_project_config(&ok, ACTOR).unwrap();
}

#[test]
fn interviews_are_found_by_attribute_value() {
    let (s, _d) = ready();
    second_interview(&s);
    with_attrs(&s);
    set(&s, &[("sector", Some("privé"))]).unwrap();
    s.set_interview_attributes(
        "ECO-17",
        &BTreeMap::from([("sector".to_string(), Some("public".to_string()))]),
        ACTOR,
    )
    .unwrap();
    let p = s.interviews_with_attribute("sector", "privé").unwrap();
    assert_eq!(p.into_iter().collect::<Vec<_>>(), vec![ID.to_string()]);
    assert!(s
        .interviews_with_attribute("sector", "mixte")
        .unwrap()
        .is_empty());
    assert!(s.interviews_with_attribute("nope", "x").is_err());
}

#[test]
fn codes_can_be_counted_for_the_interviews_of_an_attribute_value() {
    let (s, _d) = ready();
    second_interview(&s);
    with_attrs(&s);
    set(&s, &[("sector", Some("privé"))]).unwrap();
    let only = s.interviews_with_attribute("sector", "public").unwrap();
    let nodes = s.code_nodes_filtered(Some(&only)).unwrap();
    assert!(nodes.iter().all(|n| n.counts.passages == 0));
}

#[test]
fn a23_memo_defaults_then_overrides_then_answers_with_timestamps() {
    let (s, _d) = ready();
    let m = s.interview_memo(ID).unwrap();
    assert_eq!(m.template_id, "default");
    assert_eq!(m.template_version, 1);
    assert_eq!(m.questions.len(), 4);
    assert!(!m.saved);
    let ov = MemoOverrides {
        added: vec![MemoQuestion {
            id: String::new(),
            text: "Qui décide de quoi ?".into(),
        }],
        removed: vec!["q3".into()],
        reworded: BTreeMap::from([("q1".to_string(), "Que raconte-t-elle ?".to_string())]),
    };
    let m = s.set_memo_overrides(ID, &ov, ACTOR).unwrap();
    let texts: Vec<&str> = m.questions.iter().map(|q| q.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Que raconte-t-elle ?",
            "Qu'est-ce qui m'a surpris ?",
            "Que demander aux entretiens suivants ?",
            "Qui décide de quoi ?"
        ]
    );
    assert_eq!(m.questions[3].id, "x1");
    assert_eq!(m.removed.len(), 1);
    assert!(m.questions[0].reworded);
    // Answers.
    let m = s
        .answer_memo(
            ID,
            &BTreeMap::from([("q1".to_string(), " Une histoire. ".to_string())]),
            ACTOR,
        )
        .unwrap();
    let a = m.questions[0].answer.clone().unwrap();
    assert_eq!(a.text, "Une histoire.");
    assert!(!a.at.is_empty());
    // A removed question cannot be answered, an unknown one neither.
    assert!(s
        .answer_memo(
            ID,
            &BTreeMap::from([("q3".to_string(), "x".to_string())]),
            ACTOR
        )
        .is_err());
    assert!(s
        .answer_memo(
            ID,
            &BTreeMap::from([("zz".to_string(), "x".to_string())]),
            ACTOR
        )
        .is_err());
    // The template itself is untouched by the interview's edits.
    assert_eq!(
        s.project_config().unwrap().memo_templates[0]
            .questions
            .len(),
        4
    );
    // Clearing an answer.
    let m = s
        .answer_memo(
            ID,
            &BTreeMap::from([("q1".to_string(), String::new())]),
            ACTOR,
        )
        .unwrap();
    assert!(m.questions[0].answer.is_none());
    let acts = audit_actions(&s);
    assert!(acts.contains(&"memo.overrides".to_string()));
    assert!(acts.contains(&"memo.answer".to_string()));
}

#[test]
fn override_mistakes_are_refused() {
    let (s, _d) = ready();
    let bad = |ov: MemoOverrides| s.set_memo_overrides(ID, &ov, ACTOR).is_err();
    assert!(bad(MemoOverrides {
        removed: vec!["zz".into()],
        ..Default::default()
    }));
    assert!(bad(MemoOverrides {
        reworded: BTreeMap::from([("zz".into(), "x".into())]),
        ..Default::default()
    }));
    assert!(bad(MemoOverrides {
        reworded: BTreeMap::from([("q1".into(), "  ".into())]),
        ..Default::default()
    }));
    assert!(bad(MemoOverrides {
        added: vec![MemoQuestion {
            id: "q1".into(),
            text: "x".into()
        }],
        ..Default::default()
    }));
    assert!(bad(MemoOverrides {
        added: vec![MemoQuestion {
            id: String::new(),
            text: String::new()
        }],
        ..Default::default()
    }));
}

#[test]
fn a23_a_memo_keeps_the_version_it_was_answered_with() {
    let (s, _d) = ready();
    s.answer_memo(
        ID,
        &BTreeMap::from([("q2".to_string(), "Le prix.".to_string())]),
        ACTOR,
    )
    .unwrap();
    let mut cfg = s.project_config().unwrap();
    cfg.memo_templates[0].questions[1].text = "Qu'est-ce qui a étonné ?".into();
    let stored = s.set_project_config(&cfg, ACTOR).unwrap();
    assert_eq!(stored.memo_templates[0].version, 2);
    let m = s.interview_memo(ID).unwrap();
    assert_eq!(m.template_version, 1);
    assert_eq!(m.current_version, 2);
    assert!(m.outdated);
    assert_eq!(m.questions[1].text, "Qu'est-ce qui m'a surpris ?");
    assert_eq!(m.questions[1].answer.as_ref().unwrap().text, "Le prix.");
    // Moving to the new version is explicit, and keeps the answer.
    let m = s.set_memo_template(ID, "default", ACTOR).unwrap();
    assert_eq!(m.template_version, 2);
    assert!(!m.outdated);
    assert_eq!(m.questions[1].text, "Qu'est-ce qui a étonné ?");
    assert_eq!(m.questions[1].answer.as_ref().unwrap().text, "Le prix.");
}

#[test]
fn several_templates_one_default_and_switching_keeps_answers_aside() {
    let (s, _d) = ready();
    let mut cfg = s.project_config().unwrap();
    cfg.memo_templates.push(MemoTemplate {
        id: "retail".into(),
        name: "Commerce".into(),
        default: false,
        version: 1,
        questions: vec![MemoQuestion {
            id: "r1".into(),
            text: "Quel flux de marchandises ?".into(),
        }],
    });
    s.set_project_config(&cfg, ACTOR).unwrap();
    s.answer_memo(
        ID,
        &BTreeMap::from([("q1".to_string(), "Réponse".to_string())]),
        ACTOR,
    )
    .unwrap();
    let m = s.set_memo_template(ID, "retail", ACTOR).unwrap();
    assert_eq!(m.template_name, "Commerce");
    assert_eq!(m.questions.len(), 1);
    assert_eq!(m.orphan_answers["q1"].text, "Réponse");
    assert!(s.set_memo_template(ID, "nope", ACTOR).is_err());
    // A template used by an interview cannot be removed.
    let mut gone = s.project_config().unwrap();
    gone.memo_templates.retain(|t| t.id != "retail");
    assert!(s.set_project_config(&gone, ACTOR).is_err());
    // Two defaults are refused.
    let mut two = s.project_config().unwrap();
    two.memo_templates[1].default = true;
    assert!(s.set_project_config(&two, ACTOR).is_err());
}

#[test]
fn a24_the_checklist_comes_from_the_configuration_and_never_locks() {
    let (s, _d) = ready();
    let c = s.interview_checklist(ID).unwrap();
    assert_eq!(c.items.len(), 4);
    assert!(c.items.iter().all(|i| !i.checked));
    assert!(!c.complete);
    let c = s
        .set_interview_checklist(
            ID,
            &BTreeMap::from([("turns_seen".to_string(), true)]),
            Some("Position claire."),
            ACTOR,
        )
        .unwrap();
    assert!(c.items[0].checked && c.items[0].checked_at.is_some());
    assert_eq!(c.note, "Position claire.");
    assert!(s
        .set_interview_checklist(ID, &BTreeMap::from([("zz".to_string(), true)]), None, ACTOR)
        .is_err());
    // Checking the same item again keeps its time and writes no event.
    let n = s.audit_events(None).unwrap().len();
    let again = s
        .set_interview_checklist(
            ID,
            &BTreeMap::from([("turns_seen".to_string(), true)]),
            None,
            ACTOR,
        )
        .unwrap();
    assert_eq!(again.items[0].checked_at, c.items[0].checked_at);
    assert_eq!(s.audit_events(None).unwrap().len(), n);
    // An edited list shows at once; the old state of a removed item is ignored.
    let mut cfg = s.project_config().unwrap();
    cfg.checklist = vec![ListItem {
        id: "turns_seen".into(),
        label: "Tours vus".into(),
    }];
    s.set_project_config(&cfg, ACTOR).unwrap();
    let c = s.interview_checklist(ID).unwrap();
    assert_eq!(c.items.len(), 1);
    assert!(c.complete);
}

#[test]
fn a21_a_template_round_trips_between_two_projects_with_no_interview_data() {
    let (a, _d) = ready();
    let mut cfg = with_attrs(&a);
    cfg.interpretive_framework = framework_preset("toe");
    cfg.matrix_dimensions = vec![ListItem {
        id: "who_pays".into(),
        label: "Qui paie".into(),
    }];
    cfg.theme_checks.truncate(5);
    a.set_project_config(&cfg, ACTOR).unwrap();
    a.create_research_question("Q1", "Pourquoi changer ?", ACTOR)
        .unwrap();
    set(&a, &[("sector", Some("privé"))]).unwrap();
    a.answer_memo(
        ID,
        &BTreeMap::from([("q1".to_string(), "secret".to_string())]),
        ACTOR,
    )
    .unwrap();

    let t1 = a.export_template().unwrap();
    let json = serde_json::to_string(&t1).unwrap();
    assert!(!json.contains("secret"), "no interview data");
    assert!(!json.contains(ID));

    let b = Store::open_in_memory().unwrap();
    let report = b
        .import_template(&serde_json::to_value(&t1).unwrap(), false, ACTOR)
        .unwrap();
    assert!(report.applied, "{:?}", report.problems);
    let t2 = b.export_template().unwrap();
    assert_eq!(
        serde_json::to_string(&t2).unwrap(),
        json,
        "A26 round trip, same bytes"
    );
    assert!(audit_actions(&b).contains(&"project.template.import".to_string()));
    assert!(b.verify_chain().unwrap().ok);
}

#[test]
fn a_dry_run_reports_and_writes_nothing() {
    let (a, _d) = ready();
    with_attrs(&a);
    let t = serde_json::to_value(a.export_template().unwrap()).unwrap();
    let b = Store::open_in_memory().unwrap();
    let n = b.audit_events(None).unwrap().len();
    let r: TemplateReport = b.import_template(&t, true, ACTOR).unwrap();
    assert!(!r.applied && r.problems.is_empty());
    assert!(r
        .changes
        .iter()
        .any(|c| c.part == "attributes" && c.action == "added"));
    assert_eq!(b.audit_events(None).unwrap().len(), n);
    assert!(b.project_config().unwrap().attributes.is_empty());
}

#[test]
fn a_bad_template_reports_every_problem_and_applies_nothing() {
    let (a, _d) = ready();
    let mut t = serde_json::to_value(a.export_template().unwrap()).unwrap();
    t["version"] = 99.into();
    t["project_config"]["attributes"] = serde_json::json!([
        {"id": "sector", "label": "Secteur", "kind": "choice", "choices": []},
        {"id": "sector", "label": "Autre", "kind": "text"}
    ]);
    t["project_config"]["link_types"] = serde_json::json!([]);
    let b = Store::open_in_memory().unwrap();
    let r = b.import_template(&t, false, ACTOR).unwrap();
    assert!(!r.applied);
    assert!(r.problems.len() >= 3, "{:?}", r.problems);
    assert!(b.project_config().unwrap() == ProjectConfig::default());
    // Not a template at all.
    assert!(b
        .import_template(&serde_json::json!({"hello": 1}), false, ACTOR)
        .is_err());
}

#[test]
fn a_template_never_removes_an_attribute_that_has_values() {
    let (s, _d) = ready();
    with_attrs(&s);
    set(&s, &[("sector", Some("privé"))]).unwrap();
    let mut t =
        serde_json::to_value(Store::open_in_memory().unwrap().export_template().unwrap()).unwrap();
    t["project_config"]["attributes"] = serde_json::json!([
        {"id": "role", "label": "Rôle", "kind": "text"}
    ]);
    let r = s.import_template(&t, false, ACTOR).unwrap();
    assert!(r.applied, "{:?}", r.problems);
    let cfg = s.project_config().unwrap();
    let ids: Vec<&str> = cfg.attributes.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&"role") && ids.contains(&"sector"));
    assert!(
        !ids.contains(&"size"),
        "an attribute without values is replaced"
    );
    assert!(r
        .changes
        .iter()
        .any(|c| c.action == "kept" && c.detail.contains("sector")));
    assert_eq!(
        s.interview_attributes(ID).unwrap().values["sector"],
        "privé"
    );
}

#[test]
fn a_template_that_retypes_an_attribute_with_values_is_refused() {
    let (s, _d) = ready();
    with_attrs(&s);
    set(&s, &[("size", Some("3"))]).unwrap();
    let mut t =
        serde_json::to_value(Store::open_in_memory().unwrap().export_template().unwrap()).unwrap();
    t["project_config"]["attributes"] =
        serde_json::json!([{"id": "size", "label": "Taille", "kind": "text"}]);
    let r = s.import_template(&t, false, ACTOR).unwrap();
    assert!(!r.applied);
    assert!(r.problems.iter().any(|p| p.contains("size")));
}

#[test]
fn the_framework_is_configuration_only_and_never_reaches_the_codebook() {
    let (s, _d) = ready();
    let before = s.codebook().unwrap();
    let mut cfg = s.project_config().unwrap();
    cfg.interpretive_framework = framework_preset("tam_utaut");
    s.set_project_config(&cfg, ACTOR).unwrap();
    assert_eq!(s.codebook().unwrap(), before);
    assert!(s
        .code_nodes()
        .unwrap()
        .iter()
        .all(|n| n.toe.is_none() || !n.name.contains("Utilité")));
    // No framework again.
    cfg.interpretive_framework = None;
    s.set_project_config(&cfg, ACTOR).unwrap();
    assert!(s.project_config().unwrap().interpretive_framework.is_none());
}
