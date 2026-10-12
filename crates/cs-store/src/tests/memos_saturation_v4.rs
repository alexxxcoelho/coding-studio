use super::*;
use cs_model::RqMemo;

const R: &str = "RET-22";
const R1: &str = "RET-22-U001";
const R2: &str = "RET-22-U002";

/// Registers a second interview (RET-22) with two units.
fn second_interview(s: &Store) {
    let mut e = default_entry();
    e.interview_id = R.into();
    e.folder_name = "RET-22_X".into();
    e.folder_path = "/tmp/RET-22_X".into();
    s.upsert_interview(&e, ACTOR).unwrap();
    let mut seg = segmentation(&["Why did you stop?", "The partner pulled out."]);
    seg.interview_id = R.into();
    for (i, u) in seg.units.iter_mut().enumerate() {
        u.unit_id = format!("{R}-U{:03}", i + 1);
    }
    seg.hash = Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
}

#[test]
fn rq_memo_round_trip_validates_units_and_is_audited() {
    let (s, _d) = ready();
    // SQ1 is imported with the codebook (TECH-ADV maps to it).
    assert!(s
        .research_questions()
        .unwrap()
        .iter()
        .any(|q| q.id == "SQ1"));
    assert!(s.rq_memo("SQ1").unwrap().is_none());
    let memo = RqMemo {
        rq_id: "SQ1".into(),
        documented: "Payback stated in U002.".into(),
        interpretation: "Technology advantage is perceived as realised.".into(),
        rivals: "Selection bias of respondents.".into(),
        limits: "One interview.".into(),
        units_for: vec![U2.into()],
        units_against: vec![U3.into()],
        ..Default::default()
    };
    let saved = s.set_rq_memo(&memo, ACTOR).unwrap();
    assert_eq!(saved.rq_id, "SQ1");
    assert_eq!(saved.units_for, [U2]);
    assert_eq!(saved.units_against, [U3]);
    let read = s.rq_memo("SQ1").unwrap().unwrap();
    assert_eq!(read.documented, memo.documented);
    assert_eq!(read.interpretation, memo.interpretation);
    assert_eq!(read.rivals, memo.rivals);
    assert_eq!(read.limits, memo.limits);
    assert_eq!(read.units_for, [U2]);
    assert_eq!(read.units_against, [U3]);

    let mut bad = memo.clone();
    bad.units_for = vec!["ECO-16-U999".into()];
    let err = s.set_rq_memo(&bad, ACTOR).unwrap_err();
    assert_eq!(kind(&err), "validation");
    assert!(err.to_string().contains("ECO-16-U999"));
    let mut bad = memo.clone();
    bad.units_against = vec!["ECO-16-U999".into()];
    assert_eq!(kind(&s.set_rq_memo(&bad, ACTOR).unwrap_err()), "validation");

    let mut unknown = memo.clone();
    unknown.rq_id = "NOPE".into();
    assert_eq!(
        kind(&s.set_rq_memo(&unknown, ACTOR).unwrap_err()),
        "not_found"
    );

    assert!(actions(&s).contains(&"rq_memo.update".to_string()));
    let all = s.rq_memos().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].rq_id, "SQ1");
}

#[test]
fn saturation_orders_interviews_by_first_coding_and_attributes_first_uses() {
    let (s, _d) = ready();
    second_interview(&s);
    // ECO-16 first: U002 TECH-ADV, U003 ORG-SKILLS.
    coded(&s);
    // RET-22: phase 1 then phase 2.
    s.set_human_coding(R, 1, &hc(R1, &["ENV"]), ACTOR).unwrap();
    s.set_human_coding(R, 1, &hc(R2, &["TECH"]), ACTOR).unwrap();
    // A label on R1, resolved to a new field code FIELD-X.
    let t = s
        .create_to_clarify(R, R1, "partner exit", "", ACTOR)
        .unwrap();
    s.set_human_coding(R, 2, &hc(R1, &[]), ACTOR).unwrap();
    let req = ResolveRequest {
        kind: ResolutionKind::NewCode,
        code: "FIELD-X".into(),
        rationale: None,
        all_with_label: false,
        new_code: Some(new_code("FIELD-X", Some("E"))),
    };
    s.resolve_to_clarify(t.id, &req, ACTOR).unwrap();
    // Final current coding of RET-22 (FIELD-X is replaced on R1).
    s.set_human_coding(R, 2, &hc(R1, &["ENV-PARTNER"]), ACTOR)
        .unwrap();
    s.set_human_coding(R, 2, &hc(R2, &["TECH-ADV"]), ACTOR)
        .unwrap();

    let rows = s.saturation().unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    let (a, b) = (&rows[0], &rows[1]);
    assert_eq!((a.order, a.interview_id.as_str()), (1, ID));
    assert_eq!((b.order, b.interview_id.as_str()), (2, R));

    let mut first = a.codes_first_used.clone();
    first.sort();
    assert_eq!(first, ["ORG-SKILLS", "TECH-ADV"]);
    assert!(a.field_codes_created.is_empty());
    assert_eq!((a.labels_opened, a.labels_resolved), (0, 0));

    assert_eq!(b.codes_first_used, ["ENV-PARTNER"]);
    assert_eq!(b.field_codes_created, ["FIELD-X"]);
    assert_eq!((b.labels_opened, b.labels_resolved), (1, 1));

    assert!(!a.first_coded_at.is_empty());
    assert!(!b.first_coded_at.is_empty());
    assert!(a.first_coded_at <= b.first_coded_at);
}

#[test]
fn saturation_is_empty_without_coding() {
    let (s, _d) = ready();
    assert!(s.saturation().unwrap().is_empty());
}
