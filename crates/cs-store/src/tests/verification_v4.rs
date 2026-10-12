use super::*;
use cs_model::SpeakerRole;

fn flags(
    s: &Store,
    unit: &str,
    f: &[&str],
    kind: Option<&str>,
) -> anyhow::Result<cs_model::UnitFlags> {
    let f: Vec<String> = f.iter().map(|x| x.to_string()).collect();
    s.set_unit_flags(ID, unit, &f, kind, ACTOR)
}

fn target<'a>(
    t: &'a [cs_model::VerificationTarget],
    unit: &str,
) -> &'a cs_model::VerificationTarget {
    t.iter()
        .find(|x| x.unit_id == unit)
        .unwrap_or_else(|| panic!("{unit} is not a verification target"))
}

fn has(t: &cs_model::VerificationTarget, reason: &str) -> bool {
    t.reasons.iter().any(|r| r == reason)
}

fn status(s: &Store, unit: &str) -> cs_model::VerificationStatus {
    s.verification_status(ID)
        .unwrap()
        .into_iter()
        .find(|v| v.unit_id == unit)
        .unwrap()
}

/// Challenged review (U3 disagrees), quoted U2, VERIFY flag on U1, case on U2.
/// Every unit of the fixture is then a target, with no sample left to draw.
fn targeted(s: &Store) {
    challenged(s);
    let mut c = hc(U2, &["TECH-ADV"]);
    c.excerpt = Some("paid back within a year".into());
    s.set_human_coding(ID, 2, &c, ACTOR).unwrap();
    flags(s, U1, &["VERIFY"], None).unwrap();
    let mut k = case("Pilot");
    k.units = vec![U2.into()];
    for f in [
        &mut k.trigger,
        &mut k.alternatives,
        &mut k.constraints,
        &mut k.choice,
        &mut k.result,
    ] {
        f.evidence.clear();
    }
    s.create_case(ID, &k, ACTOR).unwrap();
}

/// Store with `n` units, nothing coded, flagged, quoted or in a case.
fn many(n: usize) -> (Store, tempfile::TempDir) {
    let (s, d) = ready();
    let texts: Vec<String> = (1..=n).map(|i| format!("Statement number {i}.")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    s.save_segmentation(&segmentation(&refs), ACTOR).unwrap();
    (s, d)
}

#[test]
fn flags_closed_list_and_co_constructed_kind() {
    let (s, _d) = ready();
    assert_eq!(
        kind(&flags(&s, U2, &["FOO"], None).unwrap_err()),
        "validation"
    );
    let err = flags(&s, U2, &["CO-CONSTRUCTED"], None).unwrap_err();
    assert_eq!(kind(&err), "validation");
    assert!(err.to_string().contains("kind"), "{err}");
    let ok = flags(&s, U2, &["CO-CONSTRUCTED"], Some("assent")).unwrap();
    assert_eq!(ok.co_constructed_kind.as_deref(), Some("assent"));
    let err = flags(&s, U3, &["VERIFY"], Some("assent")).unwrap_err();
    assert_eq!(kind(&err), "validation");

    let got = flags(&s, U3, &["VERIFY", "UNFINISHED", "VERIFY"], None).unwrap();
    assert_eq!(got.flags, ["UNFINISHED", "VERIFY"]);
    let listed = s.unit_flags(ID).unwrap();
    let ids: Vec<&str> = listed.iter().map(|f| f.unit_id.as_str()).collect();
    assert_eq!(ids, [U2, U3], "U1 has no flags");

    let ev = s
        .audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .rfind(|e| e.action == "unit.flags")
        .expect("unit.flags event");
    assert!(ev.detail.get("before").is_some(), "{}", ev.detail);
    assert!(ev.detail.get("after").is_some(), "{}", ev.detail);
}

#[test]
fn flags_stay_editable_after_ai_import() {
    let (s, _d) = ready();
    challenged(&s);
    let got = flags(&s, U3, &["SECOND-HAND"], None).unwrap();
    assert_eq!(got.flags, ["SECOND-HAND"]);
}

#[test]
fn correction_overlays_the_unit_without_rewriting_the_segmentation() {
    let (s, _d) = ready();
    let before = s.segmentation(ID).unwrap().unwrap();
    let text = "The tags paid back within two years.";
    s.correct_unit(
        ID,
        U2,
        text,
        SpeakerRole::Respondent,
        "re-listened at 03:12",
        ACTOR,
    )
    .unwrap();
    let cur = s.current_unit(ID, U2).unwrap();
    assert_eq!(cur.text, text);
    assert!(cur.corrected);

    let after = s.segmentation(ID).unwrap().unwrap();
    let orig = after.units.iter().find(|u| u.unit_id == U2).unwrap();
    assert_eq!(orig.text, "The tags paid back within a year.");
    assert!(!orig.corrected);
    assert_eq!(after.hash, before.hash);

    let rows = s.unit_corrections(ID, U2).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason, "re-listened at 03:12");
    assert_eq!(rows[0].text, text);

    let err = s
        .correct_unit(ID, U2, text, SpeakerRole::Respondent, "  ", ACTOR)
        .unwrap_err();
    assert_eq!(kind(&err), "validation");

    let c = s.connection();
    for sql in [
        "UPDATE unit_corrections SET text = 'x'",
        "DELETE FROM unit_corrections",
    ] {
        let e = c.execute(sql, []).unwrap_err();
        assert!(e.to_string().contains("append-only"), "{sql}: {e}");
    }
}

#[test]
fn verification_state_is_computed_and_goes_stale_on_correction() {
    let (s, _d) = ready();
    let all = s.verification_status(ID).unwrap();
    assert_eq!(all.len(), 3);
    for v in &all {
        assert_eq!(v.state, "unverified", "{}", v.unit_id);
        assert!(!v.stale);
    }

    let v = s
        .verify_unit(ID, U2, "verified", Some("ok"), ACTOR)
        .unwrap();
    assert_eq!(v.state, "verified");
    assert!(!v.stale);
    assert!(v.at.is_some() && v.by.is_some());
    assert_eq!(status(&s, U2).state, "verified");

    let err = s.verify_unit(ID, U2, "bogus", None, ACTOR).unwrap_err();
    assert_eq!(kind(&err), "validation");

    s.correct_unit(
        ID,
        U2,
        "The tags paid back within two years.",
        SpeakerRole::Respondent,
        "re-listened",
        ACTOR,
    )
    .unwrap();
    let st = status(&s, U2);
    assert_eq!(st.state, "unverified");
    assert!(st.stale);

    let v = s.verify_unit(ID, U2, "verified", None, ACTOR).unwrap();
    assert_eq!(v.state, "verified");
    assert!(!v.stale);

    let v = s
        .verify_unit(ID, U3, "unusable", Some("inaudible"), ACTOR)
        .unwrap();
    assert_eq!(v.state, "unusable");

    let c = s.connection();
    for sql in [
        "UPDATE unit_verifications SET state = 'verified'",
        "DELETE FROM unit_verifications",
    ] {
        let e = c.execute(sql, []).unwrap_err();
        assert!(e.to_string().contains("append-only"), "{sql}: {e}");
    }
}

#[test]
fn verification_targets_cover_quotes_flags_disagreements_cases_and_a_sample() {
    let (s, _d) = ready();
    targeted(&s);
    let t = s.verification_targets(ID).unwrap();
    let u2 = target(&t, U2);
    assert!(has(u2, "quoted"), "{:?}", u2.reasons);
    assert!(has(u2, "case"), "{:?}", u2.reasons);
    assert!(has(target(&t, U3), "disagreement"));
    assert!(has(target(&t, U1), "flagged"));
    for x in &t {
        let mut sorted = x.reasons.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(x.reasons, sorted, "{}: sorted and deduplicated", x.unit_id);
    }

    let again = s.verification_targets(ID).unwrap();
    assert_eq!(t, again, "deterministic");
    let n = s
        .audit_events(Some(ID))
        .unwrap()
        .iter()
        .filter(|e| e.action == "verification.sample")
        .count();
    assert_eq!(n, 1, "the draw is audited once");
    let rows: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'verification_sample_seed:ECO-16'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn verification_sample_is_at_least_five_or_ten_percent() {
    for (n, want) in [(30, 5), (60, 6)] {
        let (s, _d) = many(n);
        let t = s.verification_targets(ID).unwrap();
        assert_eq!(t.len(), want, "{n} units");
        for x in &t {
            assert_eq!(x.reasons, ["sample"], "{}", x.unit_id);
        }
    }
}

#[test]
fn verification_coverage_counts() {
    let (s, _d) = ready();
    targeted(&s);
    s.verify_unit(ID, U2, "verified", None, ACTOR).unwrap();
    s.verify_unit(ID, U3, "unusable", Some("inaudible"), ACTOR)
        .unwrap();
    let c = s.verification_coverage(ID).unwrap();
    assert_eq!(c.n_units, 3);
    assert_eq!(c.n_verified, 1);
    assert_eq!(c.n_unusable, 1);
    assert_eq!(c.n_stale, 0);
    assert_eq!(c.n_targets, 3);
    // Assumption: a target counts as handled when it is verified or unusable.
    assert_eq!(c.n_targets_verified, 2);
}
