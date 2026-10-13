//! V2, F-56: code co-occurrence, with the three relations, groups and
//! drill-down.

use super::v2_common::*;
use super::*;
use crate::cooccurrence::{CoRequest, Relation};

struct Fx {
    s: Store,
    _d: TempDir,
    a: String,
    b: String,
    c: String,
    parent: String,
    p: Vec<String>,
}

/// ECO-16/U2 (P1 whole unit: A+B); ECO-16/U3 (P2 0..10 A, P3 5..20 B,
/// P4 25..37 C); RET-22/U2 (P5 0..5 A, P6 10..20 B). A and B sit under
/// `parent`.
fn fixture() -> Fx {
    let (s, d) = ready();
    add_interview(&s, R, &["Why did you stop?", "The partner pulled out."]);
    with_attributes(&s);
    set_attr(&s, ID, "sector", "public");
    set_attr(&s, R, "sector", "privé");
    let parent = code(&s, "Parent", None);
    let a = code(&s, "Alpha", Some(&parent));
    let b = code(&s, "Beta", Some(&parent));
    let c = code(&s, "Gamma", None);
    let p = vec![
        pcode(&s, ID, U2, None, &[&a, &b]),
        pcode(&s, ID, U3, Some((0, 10)), &[&a]),
        pcode(&s, ID, U3, Some((5, 20)), &[&b]),
        pcode(&s, ID, U3, Some((25, 36)), &[&c]),
        pcode(&s, R, R2, Some((0, 5)), &[&a]),
        pcode(&s, R, R2, Some((10, 20)), &[&b]),
    ];
    Fx {
        s,
        _d: d,
        a,
        b,
        c,
        parent,
        p,
    }
}

fn req(rel: Relation) -> CoRequest {
    CoRequest {
        relation: rel,
        ..Default::default()
    }
}

fn cell<'a>(
    g: &'a crate::cooccurrence::CoGroup,
    x: &str,
    y: &str,
) -> Option<&'a crate::cooccurrence::CoCell> {
    g.cells
        .iter()
        .find(|c| (c.a == x && c.b == y) || (c.a == y && c.b == x))
}

#[test]
fn same_passage_counts_codes_on_the_same_passage_only() {
    let f = fixture();
    let m = f.s.cooccurrence(&req(Relation::SamePassage)).unwrap();
    let g = &m.groups[0];
    assert_eq!(g.key, "all");
    assert_eq!(g.cells.len(), 1);
    let c = cell(g, &f.a, &f.b).unwrap();
    assert_eq!((c.units, c.interviews, c.passage_pairs), (1, 1, 1));
}

#[test]
fn overlap_counts_passages_that_share_characters() {
    let f = fixture();
    let m = f.s.cooccurrence(&req(Relation::Overlap)).unwrap();
    let g = &m.groups[0];
    let c = cell(g, &f.a, &f.b).unwrap();
    assert_eq!((c.units, c.interviews, c.passage_pairs), (2, 1, 2));
    assert!(cell(g, &f.a, &f.c).is_none());
    assert!(cell(g, &f.b, &f.c).is_none());
}

#[test]
fn same_unit_counts_the_codes_of_one_turn_and_reports_a_jaccard_index() {
    let f = fixture();
    let m = f.s.cooccurrence(&req(Relation::SameUnit)).unwrap();
    let g = &m.groups[0];
    let ab = cell(g, &f.a, &f.b).unwrap();
    assert_eq!((ab.units, ab.interviews, ab.passage_pairs), (3, 2, 3));
    assert!((ab.jaccard - 1.0).abs() < 1e-9);
    let ac = cell(g, &f.a, &f.c).unwrap();
    assert_eq!((ac.units, ac.interviews), (1, 1));
    assert!((ac.jaccard - 1.0 / 3.0).abs() < 1e-9);
    // The diagonal: how many passages, turns and interviews carry the code.
    let da = g.diagonal.iter().find(|d| d.code_id == f.a).unwrap();
    assert_eq!((da.passages, da.units, da.interviews), (3, 3, 2));
    // Cells are listed once, in codebook order, never as (b, a).
    let order: Vec<&str> = m.codes.iter().map(|c| c.code_id.as_str()).collect();
    let pos = |id: &str| order.iter().position(|x| *x == id).unwrap();
    assert!(g.cells.iter().all(|c| pos(&c.a) < pos(&c.b)));
    assert!(m.note.contains("not causation"));
}

#[test]
fn groups_by_interview_and_by_attribute_value() {
    let f = fixture();
    let mut r = req(Relation::SameUnit);
    r.group_by = Some("interview".into());
    let m = f.s.cooccurrence(&r).unwrap();
    let keys: Vec<&str> = m.groups.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(keys, [ID, R]);
    assert_eq!(cell(&m.groups[0], &f.a, &f.b).unwrap().units, 2);
    assert_eq!(cell(&m.groups[1], &f.a, &f.b).unwrap().units, 1);

    let mut r = req(Relation::SameUnit);
    r.group_by = Some("attribute".into());
    r.attribute = Some("sector".into());
    let m = f.s.cooccurrence(&r).unwrap();
    let keys: Vec<&str> = m.groups.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(
        keys,
        ["public", "privé"],
        "values in the order of the choices"
    );
    assert_eq!(m.groups[0].interviews, 1);

    r.attribute = None;
    assert!(f.s.cooccurrence(&r).is_err());
    r.group_by = Some("nonsense".into());
    assert!(f.s.cooccurrence(&r).is_err());
}

#[test]
fn rollup_reads_the_descendants_into_their_ancestors_and_skips_structure() {
    let f = fixture();
    let mut r = req(Relation::SameUnit);
    r.rollup = true;
    let m = f.s.cooccurrence(&r).unwrap();
    let g = &m.groups[0];
    // Parent and Gamma meet in ECO-16/U3 (through Alpha and Gamma).
    let pc = cell(g, &f.parent, &f.c).unwrap();
    assert_eq!((pc.units, pc.interviews), (1, 1));
    // An ancestor and its own descendant are not a co-occurrence.
    assert!(cell(g, &f.parent, &f.a).is_none());
    let dp = g.diagonal.iter().find(|d| d.code_id == f.parent).unwrap();
    assert_eq!(dp.passages, 5, "A and B passages, the same passage once");
    // Without rollup the parent has no passage of its own.
    let m = f.s.cooccurrence(&req(Relation::SameUnit)).unwrap();
    assert!(m.codes.iter().all(|c| c.code_id != f.parent));
}

#[test]
fn code_selection_and_minimum() {
    let f = fixture();
    let mut r = req(Relation::SameUnit);
    r.codes = vec![f.a.clone(), f.c.clone()];
    let m = f.s.cooccurrence(&r).unwrap();
    assert_eq!(m.codes.len(), 2);
    assert_eq!(m.groups[0].cells.len(), 1);
    let mut r = req(Relation::SameUnit);
    r.min_units = 2;
    let m = f.s.cooccurrence(&r).unwrap();
    assert_eq!(m.groups[0].cells.len(), 1, "only A with B reaches 2 turns");
    let mut r = req(Relation::SameUnit);
    r.codes = vec!["NOPE".into()];
    assert!(f.s.cooccurrence(&r).is_err());
}

#[test]
fn the_reserved_code_and_deleted_passages_are_left_out() {
    let f = fixture();
    // A LATER marker on P4 does not create a co-occurrence.
    f.s.code_passage(&f.p[3], "LATER", "later", ACTOR).unwrap();
    f.s.delete_passage(&f.p[5], "mistake", ACTOR).unwrap();
    let m = f.s.cooccurrence(&req(Relation::SameUnit)).unwrap();
    let g = &m.groups[0];
    assert_eq!(cell(g, &f.a, &f.b).unwrap().units, 2);
    assert!(m.codes.iter().all(|c| c.code_id != "LATER"));
}

#[test]
fn drill_down_lists_the_passage_pairs() {
    let f = fixture();
    let items =
        f.s.cooccurrence_passages(&f.a, &f.b, &req(Relation::Overlap))
            .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].unit_id, U2);
    assert_eq!(items[0].pairs[0].a.passage_id, f.p[0]);
    assert_eq!(items[0].pairs[0].b.passage_id, f.p[0]);
    let u3 = &items[1];
    assert_eq!(u3.unit_id, U3);
    assert_eq!(u3.pairs.len(), 1);
    assert_eq!(u3.pairs[0].a.passage_id, f.p[1]);
    assert_eq!(u3.pairs[0].b.passage_id, f.p[2]);
    let o = u3.pairs[0].overlap.as_ref().unwrap();
    assert_eq!((o.start, o.end), (5, 10));
    assert_eq!(u3.pairs[0].b.text.chars().count(), 15);
    assert!(u3.text.starts_with("Our staff"));
    // The same drill-down in the other direction gives the same pairs, with
    // the sides swapped.
    let rev =
        f.s.cooccurrence_passages(&f.b, &f.a, &req(Relation::Overlap))
            .unwrap();
    assert_eq!(rev[1].pairs[0].a.passage_id, f.p[2]);
    // Restricted to one interview.
    let mut r = req(Relation::SameUnit);
    r.interviews = vec![R.into()];
    let items = f.s.cooccurrence_passages(&f.a, &f.b, &r).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].interview_id, R);
}
