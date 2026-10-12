use super::*;

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

/// A case with no attribution and no evidence.
fn bare(label: &str) -> Case {
    let mut c = case(label);
    c.units = vec![];
    for f in [
        &mut c.trigger,
        &mut c.alternatives,
        &mut c.constraints,
        &mut c.choice,
        &mut c.result,
    ] {
        f.evidence.clear();
    }
    c
}

fn cell(cc: &cs_model::CrossCase, rq: &str, case_id: &str) -> cs_model::CrossCaseCell {
    cc.cells
        .iter()
        .find(|c| c.rq == rq && c.case_id == case_id)
        .unwrap_or_else(|| panic!("no cell {rq} / {case_id}"))
        .clone()
}

#[test]
fn case_units_are_sorted_deduplicated_and_derive_interviews() {
    let (s, _d) = ready();
    let mut c = bare("Pilot");
    c.units = vec![U3.into(), U2.into(), U2.into()];
    let c = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(c.units, [U2, U3]);
    assert_eq!(c.interviews, [ID]);
    assert_eq!(c.interview_id, ID);
    let read = s.case(&c.case_id).unwrap();
    assert_eq!(read.units, [U2, U3]);
    assert_eq!(read.interviews, [ID]);
}

#[test]
fn case_evidence_must_be_attributed_first() {
    let (s, _d) = ready();
    let mut c = bare("Pilot");
    c.trigger.evidence = vec![U2.into()];
    let err = s.create_case(ID, &c, ACTOR).unwrap_err();
    assert_eq!(kind(&err), "validation");
    let msg = err.to_string();
    assert!(msg.contains("trigger"), "{msg}");
    assert!(msg.contains(U2), "{msg}");
}

#[test]
fn case_units_must_exist_in_their_interview() {
    let (s, _d) = ready();
    let mut c = bare("Pilot");
    c.units = vec!["ECO-16-U999".into()];
    let err = s.create_case(ID, &c, ACTOR).unwrap_err();
    assert_eq!(kind(&err), "validation");
    assert!(err.to_string().contains("ECO-16-U999"));
}

#[test]
fn case_can_attribute_units_of_another_interview() {
    let (s, _d) = ready();
    second_interview(&s);
    let mut c = bare("Cross-interview project");
    c.units = vec![U2.into(), R1.into()];
    let c = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(c.interview_id, ID);
    assert_eq!(c.interviews, [ID, R]);
    let ids = |v: Vec<Case>| v.into_iter().map(|c| c.case_id).collect::<Vec<_>>();
    assert_eq!(ids(s.cases(Some(R)).unwrap()), [c.case_id.as_str()]);
    assert_eq!(ids(s.cases(Some(ID)).unwrap()), [c.case_id.as_str()]);
    assert_eq!(ids(s.cases(None).unwrap()), [c.case_id.as_str()]);
    let mut unknown = bare("Bad");
    unknown.units = vec![R2.into(), "RET-22-U009".into()];
    assert_eq!(
        kind(&s.create_case(ID, &unknown, ACTOR).unwrap_err()),
        "validation"
    );
}

#[test]
fn case_new_fields_round_trip_and_are_trimmed() {
    let (s, _d) = ready();
    let mut c = bare("Pilot");
    c.decision = Some(" adopted ".into());
    c.granularity = Some("item".into());
    c.carrier_chosen = Some("RAIN RFID".into());
    c.carriers_rejected = vec!["QR".into(), " NFC ".into()];
    c.account_type = Some("first-hand".into());
    c.swiss_link = Some("Swiss retailer".into());
    c.trigger.contradictions = " none seen ".into();
    c.trigger.missing = "cost figures".into();
    let made = s.create_case(ID, &c, ACTOR).unwrap();
    let c = s.case(&made.case_id).unwrap();
    assert_eq!(c.decision.as_deref(), Some("adopted"));
    assert_eq!(c.granularity.as_deref(), Some("item"));
    assert_eq!(c.carrier_chosen.as_deref(), Some("RAIN RFID"));
    assert_eq!(c.carriers_rejected, ["QR", "NFC"]);
    assert_eq!(c.account_type.as_deref(), Some("first-hand"));
    assert_eq!(c.swiss_link.as_deref(), Some("Swiss retailer"));
    assert_eq!(c.trigger.contradictions, "none seen");
    assert_eq!(c.trigger.missing, "cost figures");

    let mut blank = bare("Blank");
    blank.decision = Some("  ".into());
    blank.swiss_link = Some(String::new());
    let blank = s.create_case(ID, &blank, ACTOR).unwrap();
    assert_eq!(blank.decision, None);
    assert_eq!(blank.swiss_link, None);
}

#[test]
fn case_update_rewrites_units_and_is_audited() {
    let (s, _d) = ready();
    let mut c = bare("Pilot");
    c.units = vec![U2.into()];
    let made = s.create_case(ID, &c, ACTOR).unwrap();
    let mut edit = made.clone();
    edit.units = vec![U3.into()];
    s.update_case(&made.case_id, &edit, ACTOR).unwrap();
    assert_eq!(s.case(&made.case_id).unwrap().units, [U3]);
    let acts = actions(&s);
    let create = acts.iter().position(|a| a == "case.create").unwrap();
    let update = acts.iter().rposition(|a| a == "case.update").unwrap();
    assert!(create < update);
    let ev = s
        .audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .rfind(|e| e.action == "case.update")
        .unwrap();
    assert_eq!(ev.detail["before"]["units"], json!([U2]));
    assert_eq!(ev.detail["after"]["units"], json!([U3]));
}

#[test]
fn cross_case_undocumented_case_has_empty_cells() {
    let (s, _d) = ready();
    coded(&s);
    let made = s.create_case(ID, &bare("No units"), ACTOR).unwrap();
    let cc = s.cross_case().unwrap();
    assert!(!cc.questions.is_empty());
    for q in &cc.questions {
        let c = cell(&cc, &q.id, &made.case_id);
        assert!(!c.documented, "{}", q.id);
        assert!(c.codes.is_empty(), "{}", q.id);
    }
}

#[test]
fn cross_case_uses_only_attributed_units() {
    let (s, _d) = ready();
    coded(&s);
    let mut c = bare("Pilot");
    c.units = vec![U2.into()];
    let made = s.create_case(ID, &c, ACTOR).unwrap();
    let cc = s.cross_case().unwrap();
    let sq1 = cell(&cc, "SQ1", &made.case_id);
    assert!(sq1.documented);
    assert_eq!(sq1.codes.len(), 1);
    assert_eq!(sq1.codes[0].code, "TECH-ADV");
    assert_eq!(sq1.codes[0].units, [U2]);
    let sq2 = cell(&cc, "SQ2", &made.case_id);
    assert!(sq2.documented);
    assert!(sq2.codes.is_empty(), "U003 is not attributed");
}

#[test]
fn cross_case_spans_interviews() {
    let (s, _d) = ready();
    coded(&s);
    second_interview(&s);
    s.set_human_coding(R, 1, &hc(R1, &["ENV"]), ACTOR).unwrap();
    s.set_human_coding(R, 2, &hc(R1, &["ENV-PARTNER"]), ACTOR)
        .unwrap();
    let mut c = bare("Cross-interview project");
    c.units = vec![U2.into(), R1.into()];
    let made = s.create_case(ID, &c, ACTOR).unwrap();
    let cc = s.cross_case().unwrap();
    let sq3 = cell(&cc, "SQ3", &made.case_id);
    assert!(sq3.documented);
    assert_eq!(sq3.codes.len(), 1);
    assert_eq!(sq3.codes[0].code, "ENV-PARTNER");
    assert_eq!(sq3.codes[0].units, [R1]);
    let sq1 = cell(&cc, "SQ1", &made.case_id);
    assert_eq!(sq1.codes[0].units, [U2]);
}
