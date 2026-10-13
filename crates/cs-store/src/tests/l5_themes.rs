//! V1 lot 5: themes, their sheet, checks, status and history.

use super::*;
use crate::codetree::NewCodeFromPassage;
use crate::passages::NewPassage;
use crate::{NewTheme, ThemePatch};
use cs_model::passage::OffsetUnit;

fn code(s: &Store, name: &str, unit: &str, start: usize, end: usize) -> String {
    let req = NewCodeFromPassage {
        name: name.into(),
        definition: "Une définition.".into(),
        includes: String::new(),
        excludes: String::new(),
        parent_id: None,
        passage_id: None,
        passage: Some(NewPassage {
            interview_id: ID.into(),
            unit_id: unit.into(),
            start: Some(start),
            end: Some(end),
            offset_unit: OffsetUnit::Scalar,
            text: None,
        }),
        research_questions: Vec::new(),
        reason: None,
    };
    s.create_code_from_passage(&req, ACTOR).unwrap().0.code_id
}

fn theme(s: &Store, title: &str, codes: &[String]) -> crate::ThemeView {
    s.create_theme(
        &NewTheme {
            title: title.into(),
            codes: codes.to_vec(),
            ..NewTheme::default()
        },
        ACTOR,
    )
    .unwrap()
}

#[test]
fn a_theme_is_created_as_a_candidate_with_its_codes_and_counts() {
    let (s, _d) = ready();
    let a = code(&s, "Retour rapide", U2, 0, 10);
    let b = code(&s, "Peur du coût", U2, 12, 20);
    s.create_research_question("RQ1", "Pourquoi ?", ACTOR)
        .unwrap();
    let t = s
        .create_theme(
            &NewTheme {
                title: "Le retour rapide rassure les directions".into(),
                proposition: "Deux phrases.".into(),
                codes: vec![a.clone(), "Peur du coût".into(), a.clone()],
                research_questions: vec!["RQ1".into()],
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(t.theme_id, "T-0001");
    assert_eq!(t.status, "candidate");
    assert_eq!(t.status_label, "Candidat");
    assert_eq!(
        t.codes
            .iter()
            .map(|c| c.code_id.clone())
            .collect::<Vec<_>>(),
        vec![a.clone(), b.clone()]
    );
    assert_eq!(t.counts.passages, 2);
    assert_eq!(t.counts.interviews, 1);
    assert_eq!(t.research_questions, vec!["RQ1".to_string()]);
    assert_eq!(t.checks_total, t.checks.len());
    assert!(!t.checks_complete);
    // a second theme may reuse a code
    let t2 = theme(&s, "Autre proposition", &[a]);
    assert_eq!(t2.theme_id, "T-0002");
    assert_eq!(t2.order, 2);
    let ev: Vec<String> = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert_eq!(ev.iter().filter(|a| *a == "theme.create").count(), 2);
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn title_and_codes_are_validated() {
    let (s, _d) = ready();
    for bad in ["", "   ", "a\nb"] {
        let e = s
            .create_theme(
                &NewTheme {
                    title: bad.into(),
                    ..NewTheme::default()
                },
                ACTOR,
            )
            .unwrap_err();
        assert_eq!(kind(&e), "validation", "{bad:?}");
    }
    assert!(s
        .create_theme(
            &NewTheme {
                title: "ok".into(),
                codes: vec!["inconnu".into()],
                ..NewTheme::default()
            },
            ACTOR,
        )
        .is_err());
    let e = s
        .create_theme(
            &NewTheme {
                title: "ok".into(),
                research_questions: vec!["NOPE".into()],
                ..NewTheme::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    assert!(s.themes(None).unwrap().is_empty());
}

#[test]
fn the_sheet_is_edited_and_a_title_change_is_kept_in_the_history() {
    let (s, _d) = ready();
    let t = theme(&s, "Premier titre", &[]);
    let n = s.audit_events(None).unwrap().len();
    let t = s
        .update_theme(
            &t.theme_id,
            &ThemePatch {
                title: Some("Second titre".into()),
                reason: Some("plus affirmatif".into()),
                limits: Some("Un seul site.".into()),
                ..ThemePatch::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(t.title, "Second titre");
    assert_eq!(t.limits, "Un seul site.");
    let h = s.theme_history(&t.theme_id).unwrap();
    assert_eq!(
        h.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        ["created", "title"]
    );
    assert_eq!(h[1].before.as_deref(), Some("Premier titre"));
    assert_eq!(h[1].reason, "plus affirmatif");
    // same values again: nothing written
    let n2 = s.audit_events(None).unwrap().len();
    assert_eq!(n2, n + 1);
    s.update_theme(
        &t.theme_id,
        &ThemePatch {
            limits: Some("Un seul site.".into()),
            ..ThemePatch::default()
        },
        ACTOR,
    )
    .unwrap();
    assert_eq!(s.audit_events(None).unwrap().len(), n2);
    let e = s
        .update_theme(
            &t.theme_id,
            &ThemePatch {
                supporting_cases: Some(vec!["CASE-9".into()]),
                ..ThemePatch::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    let e = s
        .update_theme("T-0099", &ThemePatch::default(), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "not_found");
}

#[test]
fn retained_needs_every_check_or_an_explicit_override() {
    let (s, _d) = ready();
    let t = theme(&s, "Une proposition", &[]);
    let id = t.theme_id.clone();
    let e = s
        .set_theme_status(&id, "retained", "", None, ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard");
    let e = s
        .set_theme_status(&id, "retained", "", Some("  "), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard", "a blank override is not a reason");
    assert_eq!(s.theme(&id).unwrap().status, "candidate");
    let e = s
        .set_theme_status(&id, "perfect", "", None, ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    // revised needs nothing
    assert_eq!(
        s.set_theme_status(&id, "revised", "relu", None, ACTOR)
            .unwrap()
            .status,
        "revised"
    );
    // override with a reason
    let t = s
        .set_theme_status(&id, "retained", "", Some("Décision du comité"), ACTOR)
        .unwrap();
    assert_eq!(t.status, "retained");
    let h = s.theme_history(&id).unwrap();
    let last = h.last().unwrap();
    assert!(last.override_checks);
    assert_eq!(last.reason, "Décision du comité");
    assert_eq!(last.before.as_deref(), Some("revised"));
    // with every check done, no override is needed
    let t2 = theme(&s, "Deuxième", &[]);
    let checks: Vec<String> = t2.checks.iter().map(|c| c.id.clone()).collect();
    assert!(checks.len() >= 2);
    for c in &checks {
        s.set_theme_check(&t2.theme_id, c, true, "fait", ACTOR)
            .unwrap();
    }
    let t2 = s.theme(&t2.theme_id).unwrap();
    assert!(t2.checks_complete);
    let t2 = s
        .set_theme_status(&t2.theme_id, "retained", "", None, ACTOR)
        .unwrap();
    assert_eq!(t2.status, "retained");
    assert!(
        !s.theme_history(&t2.theme_id)
            .unwrap()
            .last()
            .unwrap()
            .override_checks
    );
    let ev = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .filter(|e| e.action == "theme.status")
        .collect::<Vec<_>>();
    assert_eq!(ev.len(), 3);
    assert_eq!(ev[1].detail["override"], true);
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn checks_are_answered_from_the_project_list_only() {
    let (s, _d) = ready();
    let t = theme(&s, "Une proposition", &[]);
    let first = t.checks[0].id.clone();
    let t = s
        .set_theme_check(&t.theme_id, &first, true, "relu", ACTOR)
        .unwrap();
    assert!(t.checks[0].done);
    assert_eq!(t.checks[0].note, "relu");
    assert_eq!(t.checks_done, 1);
    let e = s
        .set_theme_check(&t.theme_id, "inventé", true, "", ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    let t = s
        .set_theme_check(&t.theme_id, &first, false, "", ACTOR)
        .unwrap();
    assert_eq!(t.checks_done, 0);
}

#[test]
fn codes_questions_and_order_can_be_changed() {
    let (s, _d) = ready();
    let a = code(&s, "Un code", U2, 0, 10);
    let b = code(&s, "Autre code", U2, 12, 20);
    s.create_research_question("RQ1", "Pourquoi ?", ACTOR)
        .unwrap();
    let t1 = theme(&s, "Premier", std::slice::from_ref(&a));
    let t2 = theme(&s, "Second", &[]);
    let t = s.add_theme_code(&t1.theme_id, &b, ACTOR).unwrap();
    assert_eq!(t.codes.len(), 2);
    let t = s.remove_theme_code(&t1.theme_id, &a, ACTOR).unwrap();
    assert_eq!(t.codes.len(), 1);
    assert_eq!(t.codes[0].code_id, b);
    let t = s
        .set_theme_questions(&t2.theme_id, &["RQ1".into()], ACTOR)
        .unwrap();
    assert_eq!(t.research_questions, vec!["RQ1".to_string()]);
    let list = s
        .reorder_themes(std::slice::from_ref(&t2.theme_id), ACTOR)
        .unwrap();
    assert_eq!(
        list.iter().map(|t| t.theme_id.clone()).collect::<Vec<_>>(),
        vec![t2.theme_id.clone(), t1.theme_id.clone()]
    );
    let e = s.reorder_themes(&["T-0099".into()], ACTOR).unwrap_err();
    assert_eq!(kind(&e), "validation");
    assert_eq!(s.themes(Some("retained")).unwrap().len(), 0);
    assert_eq!(s.themes(Some("candidate")).unwrap().len(), 2);
    assert_eq!(kind(&s.themes(Some("x")).unwrap_err()), "validation");
}

#[test]
fn a_theme_can_be_deleted_and_keeps_its_history() {
    let (s, _d) = ready();
    let a = code(&s, "Premier code", U2, 0, 10);
    let t = theme(&s, "Proposition", &[a]);
    s.delete_theme(&t.theme_id, "doublon", ACTOR).unwrap();
    assert!(s.themes(None).unwrap().is_empty());
    assert_eq!(kind(&s.theme(&t.theme_id).unwrap_err()), "not_found");
    assert!(
        s.theme_history(&t.theme_id).is_ok(),
        "the history stays readable"
    );
}

#[test]
fn gap_1_a_passage_starting_at_zero_is_not_the_whole_unit() {
    let (s, _d) = ready();
    let (p, _) = s
        .create_passage(
            &NewPassage {
                interview_id: ID.into(),
                unit_id: U2.into(),
                start: Some(0),
                end: Some(8),
                offset_unit: OffsetUnit::Scalar,
                text: None,
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(p.text, "The tags");
    assert!(!p.whole_unit, "starts at 0 but stops before the end");
    let (whole, _) = s
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
    assert!(whole.whole_unit);
}
