//! V1 lot 5: the documented decisions use the words of the guide.

use super::*;

#[test]
fn an_earlier_free_text_decision_reads_as_an_outcome() {
    let (s, _d) = ready();
    let mut c = case("Pilote en magasin");
    c.decision = Some("adopted".into());
    let made = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(made.outcome.as_deref(), Some("adopt"));
    assert_eq!(made.decision.as_deref(), Some("adopted"), "kept as written");
    assert_eq!(
        s.case(&made.case_id).unwrap().outcome.as_deref(),
        Some("adopt")
    );
}

#[test]
fn outcome_characteristics_and_passages_are_stored_and_validated() {
    let (s, _d) = ready();
    let (p, _) = s
        .create_passage(
            &crate::NewPassage {
                interview_id: ID.into(),
                unit_id: U2.into(),
                start: None,
                end: None,
                offset_unit: Default::default(),
                text: None,
            },
            ACTOR,
        )
        .unwrap();
    let mut c = case("Pilote en magasin");
    c.outcome = Some("refuser".into());
    c.characteristics
        .insert(" taille ".into(), " petite ".into());
    c.passages = vec![p.passage_id.clone(), p.passage_id.clone()];
    let made = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(made.outcome.as_deref(), Some("reject"));
    assert_eq!(made.characteristics["taille"], "petite");
    assert_eq!(made.passages, vec![p.passage_id.clone()]);
    let again = s.case(&made.case_id).unwrap();
    assert_eq!(again.characteristics, made.characteristics);
    // refused
    let mut bad = case("Autre");
    bad.outcome = Some("peut-être".into());
    assert_eq!(
        kind(&s.create_case(ID, &bad, ACTOR).unwrap_err()),
        "validation"
    );
    let mut bad = case("Autre");
    bad.passages = vec!["P-99999".into()];
    assert_eq!(
        kind(&s.create_case(ID, &bad, ACTOR).unwrap_err()),
        "validation"
    );
    let mut bad = case("Autre");
    bad.characteristics.insert("  ".into(), "x".into());
    assert_eq!(
        kind(&s.create_case(ID, &bad, ACTOR).unwrap_err()),
        "validation"
    );
}

#[test]
fn name_is_accepted_for_label_on_input() {
    let c: cs_model::Case = serde_json::from_str(r#"{"name": "Pilote"}"#).unwrap();
    assert_eq!(c.label, "Pilote");
}
