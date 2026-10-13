use super::*;
use cs_model::Claim;

fn claim(unit: &str, excerpt: &str, codes: &[&str]) -> Claim {
    Claim {
        nature: None,
        unit_id: unit.into(),
        excerpt: excerpt.into(),
        codes: set(codes),
        ev_level: Some("EV2".into()),
        provenance: Some("SPONTANEOUS".into()),
        role: Some("trigger".into()),
        ..Default::default()
    }
}

fn validation_msg(r: anyhow::Result<Claim>) -> String {
    let err = r.unwrap_err();
    assert_eq!(kind(&err), "validation", "{err}");
    err.to_string()
}

#[test]
fn claim_requires_exact_excerpt_and_closed_lists() {
    let (s, _d) = ready();
    coded(&s);
    let ok = claim(U2, "paid back within a year", &["TECH-ADV"]);
    let made = s.create_claim(ID, &ok, ACTOR).unwrap();
    assert!(made.claim_id > 0);
    assert_eq!(made.interview_id, ID);
    assert_eq!(made.unit_id, U2);

    let mut bad = ok.clone();
    bad.excerpt = "paid back in a year".into();
    assert!(validation_msg(s.create_claim(ID, &bad, ACTOR)).contains("excerpt"));

    let mut bad = ok.clone();
    bad.ev_level = Some("EV9".into());
    assert!(validation_msg(s.create_claim(ID, &bad, ACTOR)).contains("ev_level"));

    let mut bad = ok.clone();
    bad.provenance = Some("GUESS".into());
    assert!(validation_msg(s.create_claim(ID, &bad, ACTOR)).contains("provenance"));

    let mut bad = ok.clone();
    bad.role = Some("cause".into());
    assert!(validation_msg(s.create_claim(ID, &bad, ACTOR)).contains("role"));

    let mut bad = ok.clone();
    bad.codes = set(&["ENV-PARTNER"]);
    assert!(validation_msg(s.create_claim(ID, &bad, ACTOR)).contains("ENV-PARTNER"));

    let mut empty = ok.clone();
    empty.codes = set(&[]);
    let made = s.create_claim(ID, &empty, ACTOR).unwrap();
    assert!(made.codes.is_empty());
}

#[test]
fn claims_crud_and_audit() {
    let (s, _d) = ready();
    coded(&s);
    let made = s
        .create_claim(
            ID,
            &claim(U2, "paid back within a year", &["TECH-ADV"]),
            ACTOR,
        )
        .unwrap();
    let mut edit = made.clone();
    edit.ev_level = Some("EV3".into());
    edit.memo = Some("stated with figures".into());
    let upd = s.update_claim(made.claim_id, &edit, ACTOR).unwrap();
    assert_eq!(upd.claim_id, made.claim_id);
    assert_eq!(upd.ev_level.as_deref(), Some("EV3"));

    let listed = s.unit_claims(ID, U2).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].ev_level.as_deref(), Some("EV3"));
    assert_eq!(listed[0].memo.as_deref(), Some("stated with figures"));

    s.delete_claim(made.claim_id, ACTOR).unwrap();
    assert!(s.claims(ID).unwrap().is_empty());

    let acts = actions(&s);
    for a in ["claim.create", "claim.update", "claim.delete"] {
        assert!(acts.contains(&a.to_string()), "{a} missing in {acts:?}");
    }
    let ev = s
        .audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .rfind(|e| e.action == "claim.update")
        .unwrap();
    assert!(
        ev.detail["before"].to_string().contains("EV2"),
        "{}",
        ev.detail
    );
    assert!(
        ev.detail["after"].to_string().contains("EV3"),
        "{}",
        ev.detail
    );
}

#[test]
fn claims_to_review_flags_code_removal_and_text_drift() {
    let (s, _d) = ready();
    coded(&s);
    let made = s
        .create_claim(
            ID,
            &claim(U2, "paid back within a year", &["TECH-ADV"]),
            ACTOR,
        )
        .unwrap();
    assert!(s.claims_to_review(ID).unwrap().is_empty());
    s.set_human_coding(ID, 2, &hc(U2, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    let review = s.claims_to_review(ID).unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(review[0].0, made.claim_id);
    assert!(review[0].1.contains("TECH-ADV"), "{}", review[0].1);
}

// Text drift depends on `correct_unit` (C3, T15); it compiles once T15 lands.
#[test]
fn claims_to_review_flags_text_drift() {
    let (s, _d) = ready();
    coded(&s);
    let made = s
        .create_claim(
            ID,
            &claim(U2, "paid back within a year", &["TECH-ADV"]),
            ACTOR,
        )
        .unwrap();
    s.correct_unit(
        ID,
        U2,
        "The tags paid back within two years.",
        SpeakerRole::Respondent,
        "re-listened",
        ACTOR,
    )
    .unwrap();
    let review = s.claims_to_review(ID).unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(review[0].0, made.claim_id);
    assert!(review[0].1.contains("excerpt"), "{}", review[0].1);
}

#[test]
fn claims_are_per_statement_not_per_code() {
    let (s, _d) = ready();
    coded(&s);
    let mut a = claim(U2, "paid back within a year", &["TECH-ADV"]);
    a.role = Some("trigger".into());
    let mut b = claim(U2, "The tags paid back", &["TECH-ADV"]);
    b.role = Some("constraint".into());
    let a = s.create_claim(ID, &a, ACTOR).unwrap();
    let b = s.create_claim(ID, &b, ACTOR).unwrap();
    assert!(a.claim_id < b.claim_id);
    let got = s.unit_claims(ID, U2).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].claim_id, a.claim_id);
    assert_eq!(got[1].claim_id, b.claim_id);
    assert_eq!(got[0].role.as_deref(), Some("trigger"));
    assert_eq!(got[1].role.as_deref(), Some("constraint"));
    assert_eq!(s.claims(ID).unwrap().len(), 2);
    assert!(s.unit_claims(ID, U3).unwrap().is_empty());
}
