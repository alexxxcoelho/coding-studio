//! V2, F-45: counts per code of passages, distinct interviews, distinct
//! cases and distinct attribute values, own and rolled up.

use super::v2_common::*;
use super::*;

/// ECO-16 (public), RET-22 (privé), RET-23 (no value).
/// A has children A1 and A2.
/// P1 ECO-16/U2 A1; P2 ECO-16/U3 A1+A2; P3 RET-22/U2 A2; P4 RET-23/U1 A.
/// Case X attributes ECO-16/U2 and RET-22/U2, case Y ECO-16/U3.
fn fixture() -> (Store, TempDir, String, String, String) {
    let (s, d) = ready();
    add_interview(&s, R, &["Why did you stop?", "The partner pulled out."]);
    add_interview(&s, T, &["We never started."]);
    with_attributes(&s);
    set_attr(&s, ID, "sector", "public");
    set_attr(&s, R, "sector", "privé");
    let a = code(&s, "Alpha", None);
    let a1 = code(&s, "Alpha one", Some(&a));
    let a2 = code(&s, "Alpha two", Some(&a));
    pcode(&s, ID, U2, None, &[&a1]);
    pcode(&s, ID, U3, None, &[&a1, &a2]);
    pcode(&s, R, R2, None, &[&a2]);
    pcode(&s, T, T1, None, &[&a]);
    make_case(&s, ID, "X", &[U2, R2]);
    make_case(&s, ID, "Y", &[U3]);
    (s, d, a, a1, a2)
}

#[test]
fn counts_of_a_leaf_code_cover_passages_interviews_cases_and_values() {
    let (s, _d, _a, a1, a2) = fixture();
    let st = s.code_stats(&a1, None).unwrap();
    assert_eq!(st.own.passages, 2);
    assert_eq!(st.own.interviews, 1);
    assert_eq!(st.own.cases, 2);
    assert_eq!(st.own.case_ids.len(), 2);
    let sector = &st.own.attributes[0];
    assert_eq!(sector.attribute_id, "sector");
    assert_eq!(sector.distinct_values, 1);
    assert_eq!(sector.values[0].value, "public");
    assert_eq!(sector.values[0].interviews, 1);
    assert_eq!(sector.interviews_without_value, 0);

    let st = s.code_stats(&a2, None).unwrap();
    assert_eq!(st.own.passages, 2);
    assert_eq!(st.own.interviews, 2);
    assert_eq!(st.own.cases, 2);
    assert_eq!(st.own.attributes[0].distinct_values, 2);
}

#[test]
fn the_rollup_counts_a_passage_once_and_reports_interviews_without_value() {
    let (s, _d, a, ..) = fixture();
    let st = s.code_stats(&a, None).unwrap();
    // The code itself: only P4.
    assert_eq!(st.own.passages, 1);
    assert_eq!(st.own.interviews, 1);
    assert_eq!(st.own.cases, 0);
    // With descendants: P1, P2 (coded twice below, counted once), P3, P4.
    assert_eq!(st.rollup.passages, 4);
    assert_eq!(st.rollup.interviews, 3);
    assert_eq!(st.rollup.cases, 2);
    let sector = &st.rollup.attributes[0];
    assert_eq!(sector.distinct_values, 2);
    assert_eq!(sector.interviews_without_value, 1);
    // An attribute nobody filled: no value, all interviews without.
    let size = &st.rollup.attributes[1];
    assert_eq!(size.distinct_values, 0);
    assert_eq!(size.interviews_without_value, 3);
    assert!(st.note.contains("never added"));
}

#[test]
fn the_figures_are_in_the_code_nodes() {
    let (s, _d, a, a1, _a2) = fixture();
    let nodes = s.code_nodes().unwrap();
    let node = |id: &str| nodes.iter().find(|n| n.code_id == id).unwrap();
    let c = &node(&a1).counts;
    assert_eq!((c.passages, c.interviews, c.cases), (2, 1, 2));
    assert_eq!(c.attribute_values["sector"], 1);
    let c = &node(&a).counts;
    assert_eq!(
        (c.passages_rollup, c.interviews_rollup, c.cases_rollup),
        (4, 3, 2)
    );
    assert_eq!(c.attribute_values_rollup["sector"], 2);
    // The reserved code is counted nowhere.
    let later = nodes.iter().find(|n| n.system).unwrap();
    assert_eq!(later.counts.passages, 0);
}

#[test]
fn a_filter_restricts_all_counts_to_the_matching_interviews() {
    let (s, _d, a, ..) = fixture();
    let st = s.code_stats(&a, Some(("sector", "privé"))).unwrap();
    assert_eq!(st.rollup.interviews, 1);
    assert_eq!(st.rollup.passages, 1);
    assert_eq!(st.rollup.cases, 1);
    assert_eq!(st.filter.unwrap().value, "privé");
    assert!(s.code_stats(&a, Some(("nope", "x"))).is_err());
}

#[test]
fn deleted_passages_and_removed_codings_do_not_count() {
    let (s, _d, _a, a1, _a2) = fixture();
    let p = s
        .passages(&Default::default())
        .unwrap()
        .into_iter()
        .find(|p| p.unit_id == U2)
        .unwrap();
    s.delete_passage(&p.passage_id, "test", ACTOR).unwrap();
    let st = s.code_stats(&a1, None).unwrap();
    assert_eq!(st.own.passages, 1);
    assert_eq!(st.own.cases, 1);
}

#[test]
fn the_reserved_code_has_no_statistics() {
    let (s, ..) = fixture();
    assert!(s.code_stats("LATER", None).is_err());
}
