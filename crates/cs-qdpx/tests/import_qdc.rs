//! Import of a REFI-QDA codebook (`.qdc`).

mod common;

use common::*;
use cs_qdpx::import::CodeOutcome;
use cs_qdpx::{import_qdc, Error, Mode};
use cs_store::Store;

const NESTED: &[u8] = include_bytes!("fixtures/nested.qdc");

fn names(s: &Store) -> Vec<String> {
    tree(s).into_iter().map(|t| t.0).collect()
}

#[test]
fn dry_run_reports_and_writes_nothing() {
    let s = store_with_interviews();
    let before = audit_actions(&s).len();
    let r = import_qdc(&s, NESTED, Mode::DryRun, ACTOR).unwrap();
    assert!(!r.applied);
    assert_eq!(r.kind, "qdc");
    assert_eq!(r.codes_created, 5);
    assert_eq!(r.codes_matched, 0);
    assert_eq!(r.codes_skipped, 0);
    assert!(r.codes.iter().all(|c| c.code_id.is_none()));
    let licences = r.codes.iter().find(|c| c.name == "Licences").unwrap();
    assert_eq!(licences.path, "Obstacles > Coûts > Licences");
    assert_eq!(licences.parent.as_deref(), Some("Obstacles > Coûts"));
    assert!(tree(&s).is_empty(), "nothing written");
    assert_eq!(audit_actions(&s).len(), before, "nothing audited");
}

#[test]
fn apply_creates_the_tree_with_definitions() {
    let s = store_with_interviews();
    let r = import_qdc(&s, NESTED, Mode::Apply, ACTOR).unwrap();
    assert!(r.applied);
    assert_eq!(r.codes_created, 5);
    assert!(r.codes.iter().all(|c| c.code_id.is_some()));
    let t = tree(&s);
    let get = |n: &str| t.iter().find(|x| x.0 == n).unwrap().clone();
    assert_eq!(get("Obstacles").2, None);
    assert_eq!(get("Obstacles").1, "Ce qui freine l'adoption.");
    assert_eq!(get("Coûts").2.as_deref(), Some("Obstacles"));
    assert_eq!(get("Coûts").1, "Coûts directs & indirects.");
    assert_eq!(get("Licences").2.as_deref(), Some("Coûts"));
    assert_eq!(get("Compétences").2.as_deref(), Some("Obstacles"));
    assert_eq!(get("Facilitateurs").2, None);
    // Without description: a placeholder, since a code needs a definition.
    assert!(get("Facilitateurs").1.contains("à compléter"));
    // Origin and the creation reason are recorded.
    let node = s.code_node(&r.codes[0].code_id.clone().unwrap()).unwrap();
    assert_eq!(node.origin, "literature");
    assert_eq!(node.reason.as_deref(), Some("Import REFI-QDA (.qdc)"));
}

#[test]
fn siblings_keep_the_file_order() {
    let s = store_with_interviews();
    import_qdc(&s, NESTED, Mode::Apply, ACTOR).unwrap();
    let nodes = s.code_nodes().unwrap();
    let order: Vec<&str> = nodes
        .iter()
        .filter(|n| !n.system)
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(
        order,
        vec![
            "Obstacles",
            "Coûts",
            "Licences",
            "Compétences",
            "Facilitateurs"
        ]
    );
}

#[test]
fn importing_twice_changes_nothing() {
    let s = store_with_interviews();
    import_qdc(&s, NESTED, Mode::Apply, ACTOR).unwrap();
    let before = (tree(&s), s.codebook_version().unwrap());
    let r = import_qdc(&s, NESTED, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 0);
    assert_eq!(r.codes_matched, 5);
    assert_eq!((tree(&s), s.codebook_version().unwrap()), before);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
}

#[test]
fn codes_match_by_name_alias_token_and_guid_without_case() {
    let s = store_with_interviews();
    let cost = add_code(&s, "COUT", "Coûts", "Mes coûts.", None);
    s.add_alias(&cost, "Charges", ACTOR).unwrap();
    add_code(&s, "COMP", "Savoir-faire", "Compétences, ma version.", None);
    let renamed = add_code(&s, "OLDNAME", "Autre nom", "Renommé depuis.", None);
    let xml = format!(
        r#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
          <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="COÛTS" isCodable="true"/>
          <Code guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="charges" isCodable="true"/>
          <Code guid="cccccccc-cccc-4ccc-8ccc-cccccccccccc" name="comp" isCodable="true"/>
          <Code guid="{}" name="Un tout autre libellé" isCodable="true"/>
        </Codes></CodeBook>"#,
        cs_qdpx::guid::code(&renamed)
    );
    let r = import_qdc(&s, xml.as_bytes(), Mode::DryRun, ACTOR).unwrap();
    assert_eq!(r.codes_created, 0, "{:?}", r.codes);
    assert_eq!(r.codes_matched, 4);
    // "COÛTS" (name) and "charges" (alias) are the same code.
    assert_eq!(r.codes[0].code_id.as_deref(), Some(cost.as_str()));
    assert_eq!(r.codes[1].code_id.as_deref(), Some(cost.as_str()));
    assert_eq!(r.codes[1].matched_by.as_deref(), Some("name"));
    assert_eq!(r.codes[3].matched_by.as_deref(), Some("guid"));
    assert_eq!(r.codes[3].code_id.as_deref(), Some(renamed.as_str()));
}

#[test]
fn a_matched_code_is_not_moved_or_edited() {
    let s = store_with_interviews();
    let top = add_code(&s, "TOP", "Racine", "La racine.", None);
    add_code(&s, "COUT", "Coûts", "Ma définition.", Some(&top));
    // The file has "Coûts" at the root and a different description.
    let xml = r#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
        <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="Autre" isCodable="true">
          <Code guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="Coûts" isCodable="true">
            <Description>Définition de l'autre outil.</Description></Code></Code>
        </Codes></CodeBook>"#;
    let version = s.codebook_version().unwrap();
    let r = import_qdc(&s, xml.as_bytes(), Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 1);
    assert_eq!(r.codes_matched, 1);
    assert!(
        r.issues
            .iter()
            .any(|i| i.message.contains("stays where it is")),
        "{:?}",
        r.issues
    );
    let t = tree(&s);
    let cost = t.iter().find(|x| x.0 == "Coûts").unwrap();
    assert_eq!(cost.2.as_deref(), Some("Racine"));
    assert_eq!(cost.1, "Ma définition.");
    assert!(s.codebook_version().unwrap() > version);
}

#[test]
fn reserved_names_are_skipped_and_children_move_up() {
    let s = store_with_interviews();
    let xml = r#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
        <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="Plus tard" isCodable="true">
          <Code guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="Enfant" isCodable="true"/></Code>
        <Code guid="cccccccc-cccc-4ccc-8ccc-cccccccccccc" name="NA" isCodable="true"/>
        </Codes></CodeBook>"#;
    let r = import_qdc(&s, xml.as_bytes(), Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_skipped, 2);
    assert_eq!(r.codes_created, 1);
    assert_eq!(names(&s), vec!["Enfant"]);
    assert_eq!(tree(&s)[0].2, None, "moved up to the root");
    assert_eq!(r.codes[1].parent, None);
    assert!(r.codes[0].reason.as_deref().unwrap().contains("Plus tard"));
    assert!(r.codes[2].reason.as_deref().unwrap().contains("reserved"));
}

#[test]
fn names_that_give_the_same_token_still_create_two_codes() {
    let s = store_with_interviews();
    let xml = r#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
        <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="Coût / bénéfice" isCodable="true"/>
        <Code guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="Coût - bénéfice" isCodable="true"/>
        </Codes></CodeBook>"#;
    let r = import_qdc(&s, xml.as_bytes(), Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 2);
    assert_eq!(names(&s), vec!["Coût - bénéfice", "Coût / bénéfice"]);
}

#[test]
fn same_name_under_two_parents_is_one_code() {
    let s = store_with_interviews();
    let xml = r#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
        <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="A" isCodable="true">
          <Code guid="a1a1a1a1-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="Autre" isCodable="true"/></Code>
        <Code guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="B" isCodable="true">
          <Code guid="b1b1b1b1-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="autre" isCodable="true"/></Code>
        </Codes></CodeBook>"#;
    let r = import_qdc(&s, xml.as_bytes(), Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 3);
    assert_eq!(r.codes_matched, 1);
    let t = tree(&s);
    assert_eq!(
        t.iter().find(|x| x.0 == "Autre").unwrap().2.as_deref(),
        Some("A")
    );
}

#[test]
fn every_write_is_audited() {
    let s = store_with_interviews();
    import_qdc(&s, NESTED, Mode::Apply, ACTOR).unwrap();
    let acts = audit_actions(&s);
    assert_eq!(
        acts.iter().filter(|a| *a == "codebook.code.create").count(),
        5
    );
    assert!(acts.contains(&"codebook.code.move".to_string()));
    assert_eq!(acts.last().map(String::as_str), Some("refi.import.qdc"));
    assert!(s.verify_chain().unwrap().ok);
    let ev = s.audit_events(None).unwrap().pop().unwrap();
    assert_eq!(ev.detail["codes_created"], 5);
    assert_eq!(ev.detail["sha256"].as_str().unwrap().len(), 64);
}

fn rejected(bytes: &[u8]) -> Vec<String> {
    let s = store_with_interviews();
    match import_qdc(&s, bytes, Mode::Apply, ACTOR) {
        Err(Error::Rejected(issues)) => {
            assert!(tree(&s).is_empty(), "a rejected file writes nothing");
            issues.into_iter().map(|i| i.message).collect()
        }
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[test]
fn malformed_files_get_an_error_report_and_write_nothing() {
    let msgs = rejected(b"<CodeBook><Codes><Code name=\"A\"></Codes></CodeBook>");
    assert!(msgs[0].contains("malformed XML"), "{msgs:?}");
    // The position is in the message.
    assert!(msgs[0].contains("at "), "{msgs:?}");
    assert!(rejected(b"").iter().any(|m| m.contains("malformed XML")));
    assert!(rejected(b"PK\x03\x04binary")
        .iter()
        .any(|m| m.contains("not valid UTF-8") || m.contains("malformed")));
    assert!(rejected(b"<html><body/></html>")
        .iter()
        .any(|m| m.contains("not a REFI-QDA codebook")));
    assert!(rejected(b"<Project xmlns=\"urn:QDA-XML:project:1.0\"/>")
        .iter()
        .any(|m| m.contains(".qdpx")));
    // DTD with entities is refused, not expanded.
    let bomb = br#"<?xml version="1.0"?><!DOCTYPE CodeBook [<!ENTITY a "aaaa">]><CodeBook><Codes/></CodeBook>"#;
    assert!(rejected(bomb).iter().any(|m| m.contains("malformed XML")));
}

#[test]
fn all_code_errors_are_listed_at_once() {
    let msgs = rejected(
        br#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
          <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="A" isCodable="true"/>
          <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="B" isCodable="true"/>
          <Code guid="cccccccc-cccc-4ccc-8ccc-cccccccccccc" isCodable="true"/>
          <Code guid="dddddddd-dddd-4ddd-8ddd-dddddddddddd" name="  " isCodable="true"/>
        </Codes></CodeBook>"#,
    );
    assert_eq!(msgs.len(), 3, "{msgs:?}");
}

#[test]
fn an_empty_codebook_is_accepted_and_does_nothing() {
    let s = store_with_interviews();
    let r = import_qdc(
        &s,
        br#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes/></CodeBook>"#,
        Mode::Apply,
        ACTOR,
    )
    .unwrap();
    assert_eq!(r.codes_created + r.codes_matched + r.codes_skipped, 0);
}

#[test]
fn outcome_is_serialised_in_snake_case() {
    let s = store_with_interviews();
    let r = import_qdc(&s, NESTED, Mode::DryRun, ACTOR).unwrap();
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["codes"][0]["outcome"], "create");
    assert_eq!(v["applied"], false);
    assert_eq!(r.codes[0].outcome, CodeOutcome::Create);
}
