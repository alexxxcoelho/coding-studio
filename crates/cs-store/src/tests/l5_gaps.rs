//! V1 lot 5, API gaps 2, 4, 5, 6, 7 and 8 at the store level.

use super::*;
use crate::codetree::{NewCode, NewCodeFromPassage};
use crate::passages::NewPassage;
use crate::PassageQualify;
use cs_model::passage::OffsetUnit;

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

fn from_passage(name: &str, sel: NewPassage) -> NewCodeFromPassage {
    NewCodeFromPassage {
        name: name.into(),
        definition: "Une définition.".into(),
        includes: String::new(),
        excludes: String::new(),
        parent_id: None,
        passage_id: None,
        passage: Some(sel),
        research_questions: Vec::new(),
        reason: None,
    }
}

fn new_code(name: &str) -> NewCode {
    NewCode {
        name: name.into(),
        definition: "Ce que dit le répondant.".into(),
        ..NewCode::default()
    }
}

// ------------------------------------------------------------------ gap 2

#[test]
fn gap_2_a_code_is_created_without_a_passage_and_keeps_its_given_name() {
    let (s, _d) = ready();
    let v0 = s.codebook_version().unwrap();
    let parent = s.create_code_v1(&new_code("Freins"), ACTOR).unwrap();
    assert!(parent.code_id.starts_with("C-"));
    assert_eq!(parent.name, "Freins");
    assert_ne!(parent.token, "Freins", "the token is not the name");
    assert_eq!(parent.counts.passages, 0);
    let child = s
        .create_code_v1(
            &NewCode {
                parent_id: Some(parent.code_id.clone()),
                includes: "coût".into(),
                excludes: "temps".into(),
                memo: "à relire".into(),
                ..new_code("Coût perçu")
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(child.parent_id.as_deref(), Some(parent.code_id.as_str()));
    assert_eq!(child.includes, "coût");
    assert_eq!(child.memo, "à relire");
    assert_eq!(s.codebook_version().unwrap(), v0 + 2);
    // name and definition are required, names are unique
    for bad in [
        NewCode {
            name: " ".into(),
            ..new_code("x")
        },
        NewCode {
            definition: "".into(),
            ..new_code("Autre")
        },
        new_code("freins"),
        NewCode {
            parent_id: Some("C-9999".into()),
            ..new_code("Orphelin")
        },
    ] {
        assert_eq!(
            kind(&s.create_code_v1(&bad, ACTOR).unwrap_err()),
            "validation"
        );
    }
    assert!(s
        .audit_events(None)
        .unwrap()
        .iter()
        .any(|e| e.action == "codebook.code.create"));
    assert!(s.verify_chain().unwrap().ok);
}

// ------------------------------------------------------------------ gap 4

#[test]
fn gap_4_a_unit_is_marked_to_review_and_the_mark_is_cleared() {
    let (s, _d) = ready();
    let m = s.set_unit_mark(ID, U2, "ambigu", ACTOR).unwrap();
    assert_eq!((m.unit_id.as_str(), m.mark.as_str()), (U2, "to_review"));
    assert_eq!(m.note, "ambigu");
    assert_eq!(s.unit_marks(ID).unwrap().len(), 1);
    assert_eq!(s.simple_progress(ID).unwrap().n_to_review, 1);
    // idempotent, and a new note replaces the old one
    let n = s.audit_events(None).unwrap().len();
    s.set_unit_mark(ID, U2, "ambigu", ACTOR).unwrap();
    assert_eq!(s.audit_events(None).unwrap().len(), n);
    s.set_unit_mark(ID, U2, "toujours ambigu", ACTOR).unwrap();
    assert_eq!(s.unit_marks(ID).unwrap()[0].note, "toujours ambigu");
    assert!(s.clear_unit_mark(ID, U2, ACTOR).unwrap());
    assert!(!s.clear_unit_mark(ID, U2, ACTOR).unwrap());
    assert_eq!(s.simple_progress(ID).unwrap().n_to_review, 0);
    assert_eq!(
        kind(&s.set_unit_mark(ID, "ECO-16-U099", "", ACTOR).unwrap_err()),
        "validation"
    );
    let actions: Vec<String> = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert!(actions.contains(&"unit.mark".to_string()));
    assert!(actions.contains(&"unit.unmark".to_string()));
}

#[test]
fn gap_4_an_interviewer_turn_cannot_be_marked() {
    let (s, _d) = ready();
    let mut seg = segmentation(&["Une question ?", "Une réponse assez longue."]);
    seg.units[0].role = SpeakerRole::Interviewer;
    seg.hash = cs_model::Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
    assert_eq!(
        kind(&s.set_unit_mark(ID, U1, "", ACTOR).unwrap_err()),
        "validation"
    );
    s.set_unit_mark(ID, U2, "", ACTOR).unwrap();
}

// ------------------------------------------------------------------ gap 5

#[test]
fn gap_5_the_library_progress_is_one_call() {
    let (s, _d) = ready();
    s.set_unit_mark(ID, U3, "", ACTOR).unwrap();
    let lib = s.library_progress().unwrap();
    assert_eq!(lib.interviews.len(), 1);
    let row = &lib.interviews[0];
    assert_eq!(row.interview_id, ID);
    assert_eq!(row.units, 3);
    assert_eq!(row.respondent_units, 3);
    assert_eq!(row.relevant + row.not_relevant, 3);
    assert_eq!(row.coded, 0);
    assert_eq!(row.to_review, 1);
    assert_eq!(row.review.state, "none");
    assert!(row.memo_total > 0);
    assert_eq!(row.memo_answered, 0);
    assert_eq!(lib.totals.interviews, 1);
    assert_eq!(lib.totals.to_review, 1);
    assert_eq!(lib.totals.units, 3);
}

// ------------------------------------------------------------------ gap 6

#[test]
fn gap_6_the_code_memo_has_its_own_field_and_history() {
    let (s, _d) = ready();
    let code = s.create_code_v1(&new_code("Freins"), ACTOR).unwrap();
    assert_eq!(code.memo, "");
    let v = s.codebook_version().unwrap();
    let code = s
        .set_code_memo(&code.code_id, "Premier jet.", ACTOR)
        .unwrap();
    assert_eq!(code.memo, "Premier jet.");
    assert_eq!(code.reason, None, "reason is not touched");
    assert_eq!(code.version, 1, "a memo is not a new version of the code");
    assert_eq!(s.codebook_version().unwrap(), v);
    s.set_code_memo(&code.code_id, "Premier jet.", ACTOR)
        .unwrap();
    s.set_code_memo(&code.code_id, "Second jet.", ACTOR)
        .unwrap();
    let h = s.code_memo_history(&code.code_id).unwrap();
    assert_eq!(
        h.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        ["Premier jet.", "Second jet."]
    );
    assert_eq!(s.code_node(&code.code_id).unwrap().memo, "Second jet.");
    let cleared = s.set_code_memo(&code.code_id, "", ACTOR).unwrap();
    assert_eq!(cleared.memo, "");
    assert_eq!(s.code_memo_history(&code.code_id).unwrap().len(), 3);
    assert_eq!(
        s.audit_events(None)
            .unwrap()
            .iter()
            .filter(|e| e.action == "codebook.code.memo")
            .count(),
        3
    );
    assert_eq!(
        kind(&s.set_code_memo("C-9999", "x", ACTOR).unwrap_err()),
        "not_found"
    );
}

// ------------------------------------------------------------------ gap 7

#[test]
fn gap_7_undo_removes_the_coding_the_code_and_the_selection() {
    let (s, _d) = ready();
    let (code, passage) = s
        .create_code_from_passage(&from_passage("Retour rapide", selection(U2, 4, 8)), ACTOR)
        .unwrap();
    let v = s.codebook_version().unwrap();
    let out = s
        .undo_create_code_from_passage(&code.code_id, ACTOR)
        .unwrap();
    assert!(out.coding_removed);
    assert!(out.code_deleted);
    assert!(out.passage_deleted);
    assert!(out.code_kept_because.is_empty());
    assert_eq!(kind(&s.code_node(&code.code_id).unwrap_err()), "not_found");
    assert_eq!(
        kind(&s.passage(&passage.passage_id).unwrap_err()),
        "not_found"
    );
    assert_eq!(s.codebook_version().unwrap(), v + 1);
    let ev = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .find(|e| e.action == "codebook.code.undo_create_from_passage")
        .unwrap();
    assert_eq!(ev.detail["code_deleted"], true);
    assert!(s.verify_chain().unwrap().ok);
    // the name is free again
    s.create_code_v1(&new_code("Retour rapide"), ACTOR).unwrap();
}

#[test]
fn gap_7_the_code_stays_when_something_else_uses_it() {
    let (s, _d) = ready();
    let (code, passage) = s
        .create_code_from_passage(&from_passage("Retour rapide", selection(U2, 4, 8)), ACTOR)
        .unwrap();
    // a second passage carries the code
    let (second, _) = s
        .code_span(&selection(U3, 0, 3), &code.code_id, "", ACTOR)
        .unwrap();
    let out = s
        .undo_create_code_from_passage(&code.code_id, ACTOR)
        .unwrap();
    assert!(out.coding_removed);
    assert!(!out.code_deleted);
    assert_eq!(out.code_kept_because, vec!["coded_elsewhere".to_string()]);
    assert!(s.code_node(&code.code_id).is_ok());
    assert!(s.passage(&second.passage_id).unwrap().codings.len() == 1);
    assert!(
        out.passage_deleted,
        "the selection made by the call goes with its coding"
    );
    assert!(s.passage(&passage.passage_id).is_err());
}

#[test]
fn gap_7_a_code_not_made_from_a_passage_cannot_be_undone() {
    let (s, _d) = ready();
    let code = s.create_code_v1(&new_code("Libre"), ACTOR).unwrap();
    assert_eq!(
        kind(
            &s.undo_create_code_from_passage(&code.code_id, ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
}

// ------------------------------------------------------------------ gap 8

#[test]
fn gap_8_origin_and_role_come_from_fixed_lists() {
    let (s, _d) = ready();
    let (p, _) = s
        .create_passage(
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
        .unwrap();
    assert_eq!((p.origin.clone(), p.role.clone()), (None, None));
    let q = s
        .qualify_passage(
            &p.passage_id,
            &PassageQualify {
                origin: Some(Some("lived_experience".into())),
                role: Some(Some("trigger".into())),
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(q.origin.as_deref(), Some("lived_experience"));
    assert_eq!(q.role.as_deref(), Some("trigger"));
    // only role changes; explicit null clears origin
    let q = s
        .qualify_passage(
            &p.passage_id,
            &PassageQualify {
                origin: Some(None),
                role: None,
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!((q.origin, q.role.as_deref()), (None, Some("trigger")));
    for bad in [
        PassageQualify {
            origin: Some(Some("rumeur".into())),
            role: None,
        },
        PassageQualify {
            origin: None,
            role: Some(Some("chef".into())),
        },
    ] {
        assert_eq!(
            kind(&s.qualify_passage(&p.passage_id, &bad, ACTOR).unwrap_err()),
            "validation"
        );
    }
    let n = s.audit_events(None).unwrap().len();
    s.qualify_passage(&p.passage_id, &PassageQualify::default(), ACTOR)
        .unwrap();
    assert_eq!(s.audit_events(None).unwrap().len(), n);
}

#[test]
fn gap_8_a_claim_nature_is_taken_from_the_project_list_and_old_values_stay_readable() {
    let (s, _d) = ready();
    let mk = |nature: Option<&str>| cs_model::Claim {
        unit_id: U2.into(),
        excerpt: "tags".into(),
        nature: nature.map(String::from),
        ..cs_model::Claim::default()
    };
    let made = s.create_claim(ID, &mk(Some("first_hand")), ACTOR).unwrap();
    assert_eq!(made.nature.as_deref(), Some("first_hand"));
    assert_eq!(
        kind(&s.create_claim(ID, &mk(Some("rumeur")), ACTOR).unwrap_err()),
        "validation"
    );
    // a claim without a nature (saved before) is still fine
    let old = s.create_claim(ID, &mk(None), ACTOR).unwrap();
    assert_eq!(old.nature, None);
    assert_eq!(s.claims(ID).unwrap().len(), 2);
}
