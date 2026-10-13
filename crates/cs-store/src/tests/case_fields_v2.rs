use super::*;
use cs_model::project::{AttributeKind, CaseFieldDef};

fn def(id: &str, kind: AttributeKind, choices: &[&str]) -> CaseFieldDef {
    CaseFieldDef {
        id: id.into(),
        label: id.into(),
        kind,
        choices: choices.iter().map(|s| s.to_string()).collect(),
    }
}

fn with_fields(s: &Store, defs: Vec<CaseFieldDef>) {
    let mut cfg = s.project_config().unwrap();
    cfg.case_fields = defs;
    s.set_project_config(&cfg, ACTOR).unwrap();
}

#[test]
fn characteristics_follow_the_fields_the_project_defines() {
    let (s, _d) = ready();
    with_fields(
        &s,
        vec![
            def("budget", AttributeKind::Number, &[]),
            def("scope", AttributeKind::Choice, &["local", "global"]),
        ],
    );
    let mut c = case("Pilot");
    c.stage = None;
    c.characteristics.insert("budget".into(), "12,5".into());
    c.characteristics.insert("scope".into(), "local".into());
    let saved = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(
        saved.characteristics["budget"], "12.5",
        "normalised by type"
    );

    let mut bad = case("Bad value");
    bad.stage = None;
    bad.characteristics
        .insert("scope".into(), "galactic".into());
    let err = s.create_case(ID, &bad, ACTOR).unwrap_err().to_string();
    assert!(err.contains("case field scope"), "{err}");

    let mut unknown = case("Unknown key");
    unknown.stage = None;
    unknown.characteristics.insert("mood".into(), "x".into());
    let err = s.create_case(ID, &unknown, ACTOR).unwrap_err().to_string();
    assert!(err.contains("mood"), "{err}");
}

#[test]
fn without_defined_fields_characteristics_stay_free() {
    let (s, _d) = ready();
    let mut c = case("Free");
    c.stage = None;
    c.characteristics
        .insert("Anything goes".into(), "yes".into());
    let saved = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(saved.characteristics["Anything goes"], "yes");
}

#[test]
fn legacy_fields_are_stored_as_characteristics_and_stay_readable() {
    let (s, _d) = ready();
    let mut c = case("Legacy client");
    c.decision = Some("adopted".into());
    c.carriers_rejected = vec!["QR".into(), "NFC".into()];
    let saved = s.create_case(ID, &c, ACTOR).unwrap();
    assert_eq!(saved.characteristics["decision"], "adopted");
    assert_eq!(saved.characteristics["carriers_rejected"], "QR; NFC");
    assert_eq!(saved.characteristics["stage"], "STAGE-PILOT");
    let read = s.case(&saved.case_id).unwrap();
    assert_eq!(read.decision.as_deref(), Some("adopted"));
    assert_eq!(read.carriers_rejected, ["QR", "NFC"]);

    // A client that edits a legacy field wins over the stored characteristic.
    let mut edit = read.clone();
    edit.decision = Some("deferred".into());
    let after = s.update_case(&saved.case_id, &edit, ACTOR).unwrap();
    assert_eq!(after.characteristics["decision"], "deferred");
    // A client that edits the characteristic and leaves the legacy field wins too.
    let mut edit = after.clone();
    edit.characteristics
        .insert("decision".into(), "rejected".into());
    let after = s.update_case(&saved.case_id, &edit, ACTOR).unwrap();
    assert_eq!(after.decision.as_deref(), Some("rejected"));
    // Clearing the legacy field removes the characteristic.
    let mut edit = after.clone();
    edit.granularity = None;
    edit.decision = None;
    let after = s.update_case(&saved.case_id, &edit, ACTOR).unwrap();
    assert!(!after.characteristics.contains_key("decision"));
}

#[test]
fn a_field_in_use_cannot_be_removed_or_retyped_and_unlisted_keys_block_the_list() {
    let (s, _d) = ready();
    with_fields(&s, vec![def("budget", AttributeKind::Number, &[])]);
    let mut c = case("Costed");
    c.stage = None;
    c.characteristics.insert("budget".into(), "10".into());
    s.create_case(ID, &c, ACTOR).unwrap();

    let mut cfg = s.project_config().unwrap();
    cfg.case_fields = vec![];
    // Back to free mode is allowed; the data stay.
    s.set_project_config(&cfg, ACTOR).unwrap();
    cfg.case_fields = vec![def("other", AttributeKind::Text, &[])];
    let err = s.set_project_config(&cfg, ACTOR).unwrap_err().to_string();
    assert!(err.contains("budget"), "{err}");
    cfg.case_fields = vec![def("budget", AttributeKind::Text, &[])];
    // A number stored as text is still a valid text.
    s.set_project_config(&cfg, ACTOR).unwrap();
    cfg.case_fields = vec![def("budget", AttributeKind::Choice, &["low"])];
    let err = s.set_project_config(&cfg, ACTOR).unwrap_err().to_string();
    assert!(err.contains("budget"), "{err}");
}

#[test]
fn old_rows_are_migrated_to_characteristics_on_open() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("old.db");
    let case_id;
    {
        let s = Store::open(&path).unwrap();
        let e = default_entry();
        s.upsert_interview(&e, ACTOR).unwrap();
        s.save_segmentation(&segmentation(&["Q?", "We chose item level tags."]), ACTOR)
            .unwrap();
        let mut c = case("Old");
        c.stage = Some("STAGE-X".into());
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
        case_id = s.create_case(ID, &c, ACTOR).unwrap().case_id;
        // Rewrite the row as an earlier version stored it: legacy keys in the
        // JSON blob, no characteristics.
        s.connection()
            .execute(
                "UPDATE cases SET fields = json_set(json_remove(fields, '$.characteristics'), '$.decision', 'adopted', '$.swiss_link', 'Migros', '$.carriers_rejected', json('[\"QR\"]')), stage = 'STAGE-X', outcome_status = 'STATUS-Y'",
                [],
            )
            .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let raw: String = s
        .connection()
        .query_row(
            "SELECT fields FROM cases WHERE case_id = ?1",
            [&case_id],
            |r| r.get(0),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["characteristics"]["decision"], "adopted");
    assert_eq!(v["characteristics"]["swiss_link"], "Migros");
    assert_eq!(v["characteristics"]["carriers_rejected"], "QR");
    assert_eq!(v["characteristics"]["stage"], "STAGE-X");
    assert_eq!(v["characteristics"]["outcome_status"], "STATUS-Y");
    let c = s.case(&case_id).unwrap();
    assert_eq!(c.characteristics["decision"], "adopted");
    assert_eq!(c.decision.as_deref(), Some("adopted"));
    // Idempotent: a second open changes nothing and logs nothing more.
    let n_events: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'case.fields_migrated'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_events, 1);
    drop(s);
    let s = Store::open(&path).unwrap();
    let n_events2: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'case.fields_migrated'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_events2, 1);
    assert!(s.verify_chain().unwrap().ok);
}
