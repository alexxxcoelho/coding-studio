//! V1 lot 1: a code created from a passage in one call, and the passages of a
//! code with their context (F-32, F-44).

use super::*;
use crate::codetree::NewCodeFromPassage;
use crate::passages::{NewPassage, PassageFilter};
use cs_model::passage::OffsetUnit;

fn id_of(s: &Store, token: &str) -> String {
    s.code_node(token).unwrap().code_id
}

fn new_code(name: &str) -> NewCodeFromPassage {
    NewCodeFromPassage {
        name: name.into(),
        definition: "Ce que le répondant dit du retour sur investissement.".into(),
        includes: "Délai de retour annoncé".into(),
        excludes: "Coût d'achat seul".into(),
        parent_id: None,
        passage_id: None,
        passage: None,
        research_questions: Vec::new(),
        reason: None,
    }
}

fn selection(unit: &str, start: usize, end: usize) -> NewPassage {
    NewPassage {
        interview_id: ID.into(),
        unit_id: unit.into(),
        start: Some(start),
        end: Some(end),
        offset_unit: OffsetUnit::Scalar,
        text: None,
    }
}

#[test]
fn a_code_is_created_from_a_selection_and_codes_it_in_one_call() {
    let (s, _d) = ready();
    let v0 = s.codebook_version().unwrap();
    let mut req = new_code("Retour sur investissement rapide");
    req.passage = Some(selection(U2, 4, 33));
    let (code, passage) = s.create_code_from_passage(&req, ACTOR).unwrap();
    assert!(code.code_id.starts_with("C-"));
    assert_eq!(code.name, "Retour sur investissement rapide");
    assert_eq!(code.includes, "Délai de retour annoncé");
    assert_eq!(code.excludes, "Coût d'achat seul");
    assert_eq!(code.origin, "field");
    assert_eq!(code.version, 1);
    assert_eq!(
        code.origin_passage_id.as_deref(),
        Some(passage.passage_id.as_str())
    );
    assert_eq!(code.example_passage_id, code.origin_passage_id);
    assert_eq!(code.example.as_deref(), Some(passage.text.as_str()));
    assert_eq!(passage.codings.len(), 1);
    assert_eq!(passage.codings[0].code_id, code.code_id);
    assert_eq!(s.codebook_version().unwrap(), v0 + 1);
    assert_eq!(s.code_node(&code.code_id).unwrap().counts.passages, 1);
    let actions: Vec<String> = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert!(actions.contains(&"codebook.code.create_from_passage".to_string()));
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn a_code_is_created_from_an_existing_highlight_and_converts_it() {
    let (s, _d) = ready();
    let (h, _) = s.create_passage(&selection(U2, 4, 8), ACTOR).unwrap();
    let mut req = new_code("Balises");
    req.passage_id = Some(h.passage_id.clone());
    let (code, p) = s.create_code_from_passage(&req, ACTOR).unwrap();
    assert_eq!(p.passage_id, h.passage_id);
    assert_eq!(p.kind, "coded");
    assert!(s.highlights(Some(ID)).unwrap().is_empty());
    assert_eq!(
        code.origin_passage_id.as_deref(),
        Some(h.passage_id.as_str())
    );
}

#[test]
fn a_new_code_can_be_placed_under_a_parent_at_creation() {
    let (s, _d) = ready();
    let parent = id_of(&s, "TECH-ADV");
    let mut req = new_code("Sous-code");
    req.parent_id = Some(parent.clone());
    req.passage = Some(selection(U2, 4, 8));
    let (code, _) = s.create_code_from_passage(&req, ACTOR).unwrap();
    assert_eq!(code.parent_id.as_deref(), Some(parent.as_str()));
    assert_eq!(s.code_node(&parent).unwrap().counts.passages_rollup, 1);
}

#[test]
fn a_refused_creation_leaves_nothing_behind() {
    let (s, _d) = ready();
    let v0 = s.codebook_version().unwrap();
    let n0 = s.code_nodes().unwrap().len();
    let mut taken = new_code("ORG-SKILLS");
    taken.passage = Some(selection(U2, 4, 8));
    assert!(s.create_code_from_passage(&taken, ACTOR).is_err());
    let mut no_def = new_code("Autre");
    no_def.definition = "  ".into();
    no_def.passage = Some(selection(U2, 4, 8));
    assert!(s.create_code_from_passage(&no_def, ACTOR).is_err());
    let mut bad_parent = new_code("Autre");
    bad_parent.parent_id = Some("C-9999".into());
    bad_parent.passage = Some(selection(U2, 4, 8));
    assert!(s.create_code_from_passage(&bad_parent, ACTOR).is_err());
    let mut bad_span = new_code("Autre");
    bad_span.passage = Some(selection(U2, 4, 400));
    assert!(s.create_code_from_passage(&bad_span, ACTOR).is_err());
    let none = new_code("Autre");
    assert!(
        s.create_code_from_passage(&none, ACTOR).is_err(),
        "a passage is required"
    );
    assert_eq!(s.codebook_version().unwrap(), v0);
    assert_eq!(s.code_nodes().unwrap().len(), n0);
    assert!(s.passages(&PassageFilter::default()).unwrap().is_empty());
}

fn interviewer_seg() -> Segmentation {
    let mut seg = segmentation(&[
        "So tell me about the pilot.",
        "The tags paid back within a year.",
        "Our staff had no RFID skills at all.",
    ]);
    seg.units[0].role = SpeakerRole::Interviewer;
    seg.hash = Segmentation::units_hash(&seg.units);
    seg
}

#[test]
fn a_code_created_from_a_passage_is_a_normal_phase_2_code() {
    let (s, _d) = ready();
    let mut req = new_code("Retour rapide");
    req.passage = Some(selection(U2, 4, 8));
    let (code, _) = s.create_code_from_passage(&req, ACTOR).unwrap();
    let legacy = s
        .codebook()
        .unwrap()
        .codes
        .into_iter()
        .find(|c| c.code == code.token)
        .unwrap();
    assert!(legacy.active && legacy.in_prompt);
    assert_eq!(legacy.origin, "field");
    assert!(cs_model::phase::is_phase2_code(&legacy));
    // The interviewer cannot be the origin.
    s.save_segmentation(&interviewer_seg(), ACTOR).unwrap();
    let mut bad = new_code("Autre");
    bad.passage = Some(NewPassage {
        unit_id: format!("{ID}-U001"),
        ..selection(U2, 0, 3)
    });
    assert!(s.create_code_from_passage(&bad, ACTOR).is_err());
}

#[test]
fn passages_of_a_code_come_with_their_neighbours_and_other_codes() {
    let (s, _d) = ready();
    s.code_span(&selection(U2, 4, 8), "TECH-ADV", "", ACTOR)
        .unwrap();
    s.code_span(&selection(U2, 4, 8), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    let rows = s.passages_of_code("TECH-ADV", false).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.passage.text, "tags");
    assert_eq!(r.speaker, "RESP");
    assert_eq!(
        r.previous.as_ref().unwrap().text,
        "So tell me about the pilot."
    );
    assert_eq!(
        r.next.as_ref().unwrap().text,
        "Our staff had no RFID skills at all."
    );
    assert_eq!(r.other_codes, vec!["ORG-SKILLS".to_string()]);
    // With descendants.
    let a = id_of(&s, "TECH-ADV");
    let c = id_of(&s, "ENV-PARTNER");
    s.move_code(&c, Some(&a), "r", ACTOR).unwrap();
    s.code_span(&selection(U3, 0, 3), "ENV-PARTNER", "", ACTOR)
        .unwrap();
    assert_eq!(s.passages_of_code("TECH-ADV", false).unwrap().len(), 1);
    assert_eq!(s.passages_of_code("TECH-ADV", true).unwrap().len(), 2);
}
