//! V2, F-39: split a code with a reassignment worklist, undo of a split and
//! of a merge from the journal (append-only, audited, ids never reused).

use super::v2_common::*;
use super::*;
use crate::codeops::{SplitDecision, SplitPart, SplitRequest};

fn part(name: &str) -> SplitPart {
    SplitPart {
        name: name.into(),
        definition: format!("definition of {name}"),
        includes: String::new(),
        excludes: String::new(),
        parent_id: None,
    }
}

fn split_req(names: &[&str]) -> SplitRequest {
    SplitRequest {
        parts: names.iter().map(|n| part(n)).collect(),
        rationale: "two ideas in one code".into(),
    }
}

fn dec(passage: &str, action: &str, targets: &[&str]) -> SplitDecision {
    SplitDecision {
        passage_id: passage.into(),
        action: action.into(),
        targets: targets.iter().map(|s| s.to_string()).collect(),
        note: String::new(),
    }
}

fn codes_of(s: &Store, passage: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .passage(passage)
        .unwrap()
        .codings
        .iter()
        .map(|c| c.code_id.clone())
        .collect();
    v.sort();
    v
}

/// Code "Big" with three passages: ECO-16/U1, U2, U3.
fn fixture() -> (Store, TempDir, String, Vec<String>) {
    let (s, d) = ready();
    let big = code(&s, "Big", None);
    let ps = vec![
        pcode(&s, ID, U1, None, &[&big]),
        pcode(&s, ID, U2, None, &[&big]),
        pcode(&s, ID, U3, None, &[&big]),
    ];
    (s, d, big, ps)
}

#[test]
fn a_split_creates_the_new_codes_and_a_worklist_of_every_passage() {
    let (s, _d, big, ps) = fixture();
    let out = s
        .split_code(&big, &split_req(&["Left", "Right"]), ACTOR)
        .unwrap();
    assert_eq!(out.new_codes.len(), 2);
    assert_eq!(out.worklist.total, 3);
    assert_eq!(out.worklist.pending, 3);
    // Nothing moved yet.
    assert_eq!(codes_of(&s, &ps[0]), [big.clone()]);
    let wl = s.split_worklist(out.op_id, None).unwrap();
    assert_eq!(wl.items.len(), 3);
    assert!(wl.items.iter().all(|i| i.status == "pending"));
    assert_eq!(wl.original.code_id, big);
    assert!(wl.items[0].passage.text.len() > 3);
    // New codes are children of the original's parent (none here) and active.
    assert!(out
        .new_codes
        .iter()
        .all(|c| c.active && c.parent_id.is_none()));
}

#[test]
fn a_split_needs_two_distinct_parts_and_a_rationale() {
    let (s, _d, big, _) = fixture();
    assert!(s.split_code(&big, &split_req(&["Only"]), ACTOR).is_err());
    assert!(s
        .split_code(&big, &split_req(&["Same", "same"]), ACTOR)
        .is_err());
    assert!(s
        .split_code(&big, &split_req(&["Big", "Other"]), ACTOR)
        .is_err());
    let mut r = split_req(&["A1", "A2"]);
    r.rationale = " ".into();
    assert!(s.split_code(&big, &r, ACTOR).is_err());
    assert!(s
        .split_code("LATER", &split_req(&["A1", "A2"]), ACTOR)
        .is_err());
    // Nothing was created by the refusals.
    assert_eq!(
        s.code_nodes().unwrap().iter().filter(|n| !n.system).count() >= 1,
        true
    );
    assert!(s.code_nodes().unwrap().iter().all(|n| n.name != "A1"));
}

#[test]
fn decisions_assign_one_or_several_new_codes_or_leave_the_passage() {
    let (s, _d, big, ps) = fixture();
    let out = s
        .split_code(&big, &split_req(&["Left", "Right"]), ACTOR)
        .unwrap();
    let (l, r) = (
        out.new_codes[0].code_id.clone(),
        out.new_codes[1].code_id.clone(),
    );
    let wl = s
        .split_decide(
            out.op_id,
            &[
                dec(&ps[0], "assign", &[&l]),
                dec(&ps[1], "assign", &["Left", &r]),
                dec(&ps[2], "leave", &[]),
            ],
            ACTOR,
        )
        .unwrap();
    assert_eq!(wl.pending, 0);
    assert_eq!((wl.assigned, wl.left), (2, 1));
    let mut want = [l.clone(), r.clone()];
    want.sort();
    assert_eq!(codes_of(&s, &ps[0]), [l.clone()]);
    assert_eq!(codes_of(&s, &ps[1]), want);
    assert_eq!(codes_of(&s, &ps[2]), [big.clone()]);
    // Counts follow.
    let nodes = s.code_nodes().unwrap();
    let n = |id: &str| {
        nodes
            .iter()
            .find(|x| x.code_id == id)
            .unwrap()
            .counts
            .passages
    };
    assert_eq!((n(&big), n(&l), n(&r)), (1, 2, 1));

    // A decision can be revised; the earlier one stays on record.
    s.split_decide(out.op_id, &[dec(&ps[0], "assign", &[&r])], ACTOR)
        .unwrap();
    assert_eq!(codes_of(&s, &ps[0]), [r.clone()]);
    s.split_decide(out.op_id, &[dec(&ps[0], "leave", &[])], ACTOR)
        .unwrap();
    assert_eq!(codes_of(&s, &ps[0]), [big.clone()]);
    let rows: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM split_decisions", [], |x| x.get(0))
        .unwrap();
    assert_eq!(rows, 3 + 1 + 1);
    let wl = s.split_worklist(out.op_id, Some("left")).unwrap();
    assert_eq!(wl.items.len(), 2);
}

#[test]
fn bad_decisions_are_refused_as_a_whole() {
    let (s, _d, big, ps) = fixture();
    let out = s
        .split_code(&big, &split_req(&["Left", "Right"]), ACTOR)
        .unwrap();
    let l = out.new_codes[0].code_id.clone();
    let other = code(&s, "Elsewhere", None);
    for bad in [
        vec![
            dec(&ps[0], "assign", &[&l]),
            dec(&ps[1], "assign", &[&other]),
        ],
        vec![dec(&ps[0], "assign", &[])],
        vec![dec(&ps[0], "leave", &[&l])],
        vec![dec(&ps[0], "move", &[&l])],
        vec![dec("P-NOPE", "leave", &[])],
    ] {
        assert!(s.split_decide(out.op_id, &bad, ACTOR).is_err());
        assert_eq!(codes_of(&s, &ps[0]), [big.clone()], "nothing applied");
    }
    assert_eq!(s.split_worklist(out.op_id, None).unwrap().pending, 3);
}

#[test]
fn undo_of_a_split_restores_the_original_coding_and_retires_the_new_codes() {
    let (s, _d, big, ps) = fixture();
    let out = s
        .split_code(&big, &split_req(&["Left", "Right"]), ACTOR)
        .unwrap();
    let (l, r) = (
        out.new_codes[0].code_id.clone(),
        out.new_codes[1].code_id.clone(),
    );
    s.split_decide(
        out.op_id,
        &[
            dec(&ps[0], "assign", &[&l]),
            dec(&ps[1], "assign", &[&l, &r]),
        ],
        ACTOR,
    )
    .unwrap();
    // A coding made on a new code after the split goes back with the rest.
    let late = pcode(&s, ID, U1, Some((0, 5)), &[&r]);
    let rep = s.undo_code_op(out.op_id, "wrong cut", ACTOR).unwrap();
    assert_eq!(rep.kind, "split");
    assert_eq!(codes_of(&s, &ps[0]), [big.clone()]);
    assert_eq!(codes_of(&s, &ps[1]), [big.clone()]);
    assert_eq!(codes_of(&s, &late), [big.clone()]);
    let nodes = s.code_nodes().unwrap();
    for id in [&l, &r] {
        let n = nodes.iter().find(|x| &x.code_id == id).unwrap();
        assert!(!n.active);
        assert_eq!(n.merged_into.as_deref(), Some(big.as_str()));
        assert_eq!(n.counts.passages, 0);
    }
    // The names are free again, ids are never reused.
    let again = s
        .split_code(&big, &split_req(&["Left", "Right"]), ACTOR)
        .unwrap();
    assert!(again
        .new_codes
        .iter()
        .all(|c| c.code_id != l && c.code_id != r));
    // Undone once only.
    assert!(s.undo_code_op(out.op_id, "again", ACTOR).is_err());
    // The journal keeps both operations and says which one was undone.
    let ops = s.code_ops(None).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].undone.as_ref().unwrap().rationale, "wrong cut");
    assert!(ops[1].undone.is_none());
    assert!(s.verify_chain().unwrap().ok);
    let actions: Vec<String> = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert!(actions.contains(&"codebook.code.split".to_string()));
    assert!(actions.contains(&"codebook.code.undo_split".to_string()));
}

#[test]
fn undo_of_a_merge_gives_back_codings_aliases_children_and_the_code() {
    let (s, _d) = ready();
    let a = code(&s, "Alpha", None);
    let b = code(&s, "Beta", None);
    let child = code(&s, "Alpha child", Some(&a));
    s.add_alias(&a, "Alfa", ACTOR).unwrap();
    let p1 = pcode(&s, ID, U1, None, &[&a]);
    let p2 = pcode(&s, ID, U2, None, &[&a, &b]);
    let p3 = pcode(&s, ID, U3, None, &[&b]);
    s.merge_code(&a, &b, "same idea", ACTOR).unwrap();
    assert_eq!(codes_of(&s, &p1), [b.clone()]);
    assert_eq!(codes_of(&s, &p2), [b.clone()]);
    assert_eq!(s.resolve_code("Alfa").unwrap().code_id, b);
    assert_eq!(
        s.code_node(&child).unwrap().parent_id.as_deref(),
        Some(b.as_str())
    );

    let ops = s.code_ops(Some(&a)).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, "merge");
    let rep = s.undo_code_op(ops[0].op_id, "they differ", ACTOR).unwrap();
    assert_eq!(rep.kind, "merge");
    let mut ab = [a.clone(), b.clone()];
    ab.sort();
    assert_eq!(codes_of(&s, &p1), [a.clone()]);
    assert_eq!(codes_of(&s, &p2), ab);
    assert_eq!(codes_of(&s, &p3), [b.clone()]);
    let node = s.code_node(&a).unwrap();
    assert!(node.active);
    assert!(node.merged_into.is_none());
    assert_eq!(
        s.code_node(&child).unwrap().parent_id.as_deref(),
        Some(a.as_str())
    );
    assert_eq!(s.resolve_code("Alfa").unwrap().code_id, a);
    assert_eq!(s.resolve_code("Alpha").unwrap().code_id, a);
    assert!(s.undo_code_op(ops[0].op_id, "twice", ACTOR).is_err());
    // The code can be merged again.
    s.merge_code(&a, &b, "after all", ACTOR).unwrap();
    assert_eq!(s.code_ops(Some(&a)).unwrap().len(), 2);
}

#[test]
fn undo_of_a_merge_restores_the_unit_codings_of_the_candidate() {
    let (s, _d) = ready();
    let (from, into) = ("TECH-ADV".to_string(), "ORG-SKILLS".to_string());
    s.set_human_coding(
        ID,
        2,
        &cs_model::HumanCoding {
            unit_id: U2.into(),
            codes: set(&[&from]),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    s.merge_code(&from, &into, "same", ACTOR).unwrap();
    let after = s.human_codings(ID, 2).unwrap();
    assert!(after[U2].codes.contains(&into));
    let op = s.code_ops(None).unwrap()[0].op_id;
    s.undo_code_op(op, "no", ACTOR).unwrap();
    let back = s.human_codings(ID, 2).unwrap();
    assert!(back[U2].codes.contains(&from) && !back[U2].codes.contains(&into));
}

#[test]
fn an_undo_must_come_after_the_later_operations_on_the_same_codes() {
    let (s, _d) = ready();
    let a = code(&s, "Alpha", None);
    let b = code(&s, "Beta", None);
    let c = code(&s, "Gamma", None);
    pcode(&s, ID, U1, None, &[&a]);
    s.merge_code(&a, &b, "one", ACTOR).unwrap();
    s.merge_code(&b, &c, "two", ACTOR).unwrap();
    let ops = s.code_ops(None).unwrap();
    let err = s.undo_code_op(ops[0].op_id, "first", ACTOR).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<CsError>(),
        Some(CsError::Guard(_))
    ));
    s.undo_code_op(ops[1].op_id, "second", ACTOR).unwrap();
    s.undo_code_op(ops[0].op_id, "first", ACTOR).unwrap();
    assert!(s.code_node(&a).unwrap().active);
    assert!(s.code_node(&b).unwrap().active);
}

#[test]
fn a_merge_made_before_the_journal_cannot_be_undone() {
    let (s, _d) = ready();
    assert!(s.undo_code_op(999, "x", ACTOR).is_err());
    assert!(s.code_ops(None).unwrap().is_empty());
}

#[test]
fn the_journal_is_append_only() {
    let (s, _d, big, _) = fixture();
    let out = s
        .split_code(&big, &split_req(&["Left", "Right"]), ACTOR)
        .unwrap();
    s.split_decide(
        out.op_id,
        &[dec(&out_first(&s, out.op_id), "leave", &[])],
        ACTOR,
    )
    .unwrap();
    s.undo_code_op(out.op_id, "x", ACTOR).unwrap();
    for sql in [
        "UPDATE code_ops SET rationale = 'y'",
        "DELETE FROM code_ops",
        "UPDATE code_op_undos SET rationale = 'y'",
        "DELETE FROM code_op_undos",
        "UPDATE split_decisions SET note = 'y'",
        "DELETE FROM split_decisions",
    ] {
        assert!(s.connection().execute(sql, []).is_err(), "{sql}");
    }
}

fn out_first(s: &Store, op: i64) -> String {
    s.split_worklist(op, None).unwrap().items[0]
        .passage
        .passage_id
        .clone()
}
