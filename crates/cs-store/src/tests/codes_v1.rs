//! V1 lot 1: stable code ids, names, aliases, the code tree and the codebook
//! version. Acceptance tests A4 (rename) and A5 (hierarchy, store side).

use super::*;
use crate::codetree::CodeUpdate;

const OLD_SCHEMA: &str = include_str!("fixtures/schema_v3.sql");

fn node(s: &Store, key: &str) -> cs_model::passage::CodeNode {
    s.code_node(key).unwrap()
}

fn id_of(s: &Store, token: &str) -> String {
    node(s, token).code_id
}

fn rename(s: &Store, key: &str, name: &str) -> Result<cs_model::passage::CodeNode> {
    s.update_code(
        key,
        &CodeUpdate {
            name: Some(name.into()),
            ..Default::default()
        },
        ACTOR,
    )
}

#[test]
fn every_code_gets_a_stable_id_name_and_alias_on_import() {
    let (s, _d) = ready();
    let nodes = s.code_nodes().unwrap();
    let real: Vec<_> = nodes.iter().filter(|n| !n.system).collect();
    assert_eq!(real.len(), s.codebook().unwrap().codes.len());
    let ids: Vec<&str> = real.iter().map(|n| n.code_id.as_str()).collect();
    assert_eq!(ids[0], "C-0001");
    assert_eq!(ids[1], "C-0002");
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "ids are unique");
    let adv = real.iter().find(|n| n.token == "TECH-ADV").unwrap();
    assert_eq!(adv.name, "TECH-ADV");
    assert!(adv
        .aliases
        .iter()
        .any(|a| a.alias == "TECH-ADV" && a.reason == "legacy_token"));
}

#[test]
fn the_later_code_is_always_listed_and_is_not_in_the_codes_table() {
    let (s, _d) = ready();
    let nodes = s.code_nodes().unwrap();
    let later = nodes.iter().find(|n| n.system).unwrap();
    assert_eq!(later.code_id, "LATER");
    assert_eq!(later.name, "Plus tard");
    assert!(!s
        .codebook()
        .unwrap()
        .codes
        .iter()
        .any(|c| c.code == "LATER"));
    // The system code cannot be edited, moved or merged.
    assert!(rename(&s, "LATER", "Autre").is_err());
    assert!(s.move_code("LATER", None, "x", ACTOR).is_err());
}

#[test]
fn a_version_3_database_is_migrated_in_place_and_stays_readable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("old.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(OLD_SCHEMA).unwrap();
        c.execute_batch("PRAGMA user_version = 3;").unwrap();
        c.execute(
            "INSERT INTO codes (code, family, label, definition, origin, toe, ord) VALUES ('ZED','OTHER','Zed','def z','field',NULL,2)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO codes (code, family, label, definition, origin, toe, ord) VALUES ('ALPHA','TECH','Alpha','def a','literature','T',1)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO interviews VALUES ('ECO-16','f','/tmp/f',NULL,'[]','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO human_codings (interview_id, phase, unit_id, codes, updated_at) VALUES ('ECO-16',2,'ECO-16-U002','[\"ALPHA\"]','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let book = s.codebook().unwrap();
    assert_eq!(book.codes.len(), 2);
    assert_eq!(s.human_codings("ECO-16", 2).unwrap().len(), 1);
    // Ids follow the codebook order (ord, then token).
    assert_eq!(id_of(&s, "ALPHA"), "C-0001");
    assert_eq!(id_of(&s, "ZED"), "C-0002");
    assert_eq!(node(&s, "C-0001").name, "ALPHA");
    let v: i64 = s
        .connection()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, crate::SCHEMA_VERSION);
    let events = s.audit_events(None).unwrap();
    assert!(events.iter().any(|e| e.action == "schema.migrate"));
    assert!(s.verify_chain().unwrap().ok);
    // Opening again changes nothing.
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(id_of(&s, "ALPHA"), "C-0001");
    assert_eq!(
        s.audit_events(None)
            .unwrap()
            .iter()
            .filter(|e| e.action == "schema.migrate")
            .count(),
        1
    );
}

#[test]
fn a_code_created_after_the_migration_gets_the_next_id() {
    let (s, _d) = ready();
    let n = s.codebook().unwrap().codes.len();
    let made = s
        .create_code(
            &Code {
                code: "NEW-ONE".into(),
                family: String::new(),
                label: String::new(),
                definition: "d".into(),
                origin: "field".into(),
                toe: None,
                active: true,
                in_prompt: true,
                research_questions: vec![],
                merged_into: None,
                version: 1,
                example: None,
                reason: None,
                created_at: None,
            },
            ACTOR,
        )
        .unwrap();
    let id = id_of(&s, &made.code);
    assert_eq!(id, format!("C-{:04}", n + 1));
    assert_eq!(node(&s, &id).name, "NEW-ONE");
}

#[test]
fn rename_keeps_the_id_and_the_codings_and_keeps_the_old_name_as_alias() {
    let (s, _d) = ready();
    coded(&s);
    let id = id_of(&s, "TECH-ADV");
    let before = s.human_codings(ID, 2).unwrap();
    let renamed = rename(&s, &id, "Avantage relatif perçu").unwrap();
    assert_eq!(renamed.code_id, id);
    assert_eq!(renamed.name, "Avantage relatif perçu");
    assert_eq!(renamed.version, 2);
    assert!(renamed
        .aliases
        .iter()
        .any(|a| a.alias == "TECH-ADV" && a.reason == "legacy_token"));
    assert!(renamed
        .aliases
        .iter()
        .any(|a| a.alias == "TECH-ADV" || a.reason == "former_name"));
    assert_eq!(s.human_codings(ID, 2).unwrap(), before);
    // The old name and the new one both resolve to the same code.
    assert_eq!(s.resolve_code("tech-adv").unwrap().code_id, id);
    assert_eq!(
        s.resolve_code("avantage RELATIF perçu").unwrap().code_id,
        id
    );
    assert_eq!(s.resolve_code(&id).unwrap().code_id, id);
}

#[test]
fn renaming_twice_keeps_every_former_name() {
    let (s, _d) = ready();
    let id = id_of(&s, "TECH-ADV");
    rename(&s, &id, "Premier nom").unwrap();
    rename(&s, &id, "Second nom").unwrap();
    let n = node(&s, &id);
    let former: Vec<&str> = n
        .aliases
        .iter()
        .filter(|a| a.reason == "former_name")
        .map(|a| a.alias.as_str())
        .collect();
    assert_eq!(former, vec!["Premier nom"]);
    assert_eq!(s.resolve_code("Premier nom").unwrap().code_id, id);
    assert_eq!(s.resolve_code("TECH-ADV").unwrap().code_id, id);
}

#[test]
fn a_name_already_taken_is_refused_whatever_the_case() {
    let (s, _d) = ready();
    let a = id_of(&s, "TECH-ADV");
    for taken in [
        "org-skills",
        "ORG-SKILLS",
        "Plus tard",
        "LATER",
        "NA",
        "  ",
        "",
    ] {
        let err = rename(&s, &a, taken).unwrap_err();
        assert!(
            matches!(err.downcast_ref::<CsError>(), Some(CsError::Validation(_))),
            "{taken:?}"
        );
    }
    // A former name of another code is taken too.
    let b = id_of(&s, "ORG-SKILLS");
    rename(&s, &b, "Compétences").unwrap();
    assert!(rename(&s, &a, "compétences").is_err());
    assert!(rename(&s, &a, "ORG-SKILLS").is_err());
}

#[test]
fn synonyms_are_added_and_removed_but_former_names_stay() {
    let (s, _d) = ready();
    let id = id_of(&s, "TECH-ADV");
    s.add_alias(&id, "bénéfice perçu", ACTOR).unwrap();
    assert_eq!(s.resolve_code("Bénéfice perçu").unwrap().code_id, id);
    assert!(s.add_alias(&id, "bénéfice perçu", ACTOR).is_err());
    let other = id_of(&s, "ORG-SKILLS");
    assert!(s.add_alias(&other, "Bénéfice perçu", ACTOR).is_err());
    s.remove_alias(&id, "bénéfice perçu", ACTOR).unwrap();
    assert!(s.resolve_code("bénéfice perçu").is_err());
    assert!(s.remove_alias(&id, "TECH-ADV", ACTOR).is_err());
}

#[test]
fn every_change_advances_the_codebook_version_and_is_audited() {
    let (s, _d) = ready();
    let v0 = s.codebook_version().unwrap();
    let id = id_of(&s, "TECH-ADV");
    rename(&s, &id, "Nom un").unwrap();
    assert_eq!(s.codebook_version().unwrap(), v0 + 1);
    let other = id_of(&s, "ORG-SKILLS");
    s.move_code(&other, Some(&id), "regroupement", ACTOR)
        .unwrap();
    assert_eq!(s.codebook_version().unwrap(), v0 + 2);
    s.set_code_active("ENV-PARTNER", false, ACTOR).unwrap();
    assert_eq!(s.codebook_version().unwrap(), v0 + 3);
    let revs = s.codebook_revisions().unwrap();
    assert_eq!(revs.len() as i64, s.codebook_version().unwrap());
    assert!(revs.last().unwrap().reason.contains("active"));
    let actions: Vec<String> = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert!(actions.contains(&"codebook.code.rename".to_string()));
    assert!(actions.contains(&"codebook.code.move".to_string()));
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn moving_a_code_changes_its_parent_and_no_coding() {
    let (s, _d) = ready();
    coded(&s);
    let parent = id_of(&s, "TECH-ADV");
    let child = id_of(&s, "ORG-SKILLS");
    let before = s.human_codings(ID, 2).unwrap();
    let moved = s
        .move_code(&child, Some(&parent), "regroupement", ACTOR)
        .unwrap();
    assert_eq!(moved.parent_id.as_deref(), Some(parent.as_str()));
    assert_eq!(s.human_codings(ID, 2).unwrap(), before);
    // Back to the root.
    let root = s.move_code(&child, None, "retour", ACTOR).unwrap();
    assert_eq!(root.parent_id, None);
}

#[test]
fn a_move_that_would_make_a_cycle_is_refused() {
    let (s, _d) = ready();
    let a = id_of(&s, "TECH-ADV");
    let b = id_of(&s, "ORG-SKILLS");
    let c = id_of(&s, "ENV-PARTNER");
    s.move_code(&b, Some(&a), "r", ACTOR).unwrap();
    s.move_code(&c, Some(&b), "r", ACTOR).unwrap();
    for (child, parent) in [(&a, &a), (&a, &b), (&a, &c)] {
        let err = s.move_code(child, Some(parent), "r", ACTOR).unwrap_err();
        assert!(
            matches!(err.downcast_ref::<CsError>(), Some(CsError::Validation(_))),
            "{child} under {parent}"
        );
    }
    assert!(s.move_code(&a, Some("C-9999"), "r", ACTOR).is_err());
    assert!(s.move_code(&a, Some("LATER"), "r", ACTOR).is_err());
    assert_eq!(node(&s, &a).parent_id, None);
}

#[test]
fn a_move_needs_a_rationale() {
    let (s, _d) = ready();
    let a = id_of(&s, "TECH-ADV");
    let b = id_of(&s, "ORG-SKILLS");
    assert!(s.move_code(&b, Some(&a), "  ", ACTOR).is_err());
}

#[test]
fn merging_makes_the_merged_name_and_aliases_resolve_to_the_target() {
    let (s, _d) = ready();
    let from = id_of(&s, "ORG-SKILLS");
    let into = id_of(&s, "TECH-ADV");
    rename(&s, &from, "Compétences internes").unwrap();
    s.merge_code("ORG-SKILLS", "TECH-ADV", "synonymes", ACTOR)
        .unwrap();
    let target = node(&s, &into);
    assert_eq!(
        s.resolve_code("Compétences internes").unwrap().code_id,
        into
    );
    assert_eq!(s.resolve_code("ORG-SKILLS").unwrap().code_id, into);
    assert!(target
        .aliases
        .iter()
        .any(|a| a.reason == "merge" && a.alias == "Compétences internes"));
    let merged = node(&s, &from);
    assert_eq!(merged.merged_into.as_deref(), Some(into.as_str()));
    assert!(!merged.active);
    // The name of a merged code stays reserved.
    assert!(rename(&s, &into, "Compétences internes").is_err());
}

#[test]
fn merging_reparents_the_children_of_the_merged_code() {
    let (s, _d) = ready();
    let from = id_of(&s, "ORG-SKILLS");
    let into = id_of(&s, "TECH-ADV");
    let kid = id_of(&s, "ENV-PARTNER");
    s.move_code(&kid, Some(&from), "r", ACTOR).unwrap();
    s.merge_code("ORG-SKILLS", "TECH-ADV", "r", ACTOR).unwrap();
    assert_eq!(node(&s, &kid).parent_id.as_deref(), Some(into.as_str()));
}

#[test]
fn codes_imported_from_csv_get_ids_names_and_a_revision() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("book.csv");
    std::fs::write(
        &path,
        "code,definition\nALPHA,First code\nBETA-ONE,Second code\n",
    )
    .unwrap();
    let s = Store::open_in_memory().unwrap();
    let v0 = s.codebook_version().unwrap();
    s.import_codebook_csv(&path, ACTOR).unwrap();
    let nodes: Vec<_> = s
        .code_nodes()
        .unwrap()
        .into_iter()
        .filter(|n| !n.system)
        .collect();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].code_id, "C-0001");
    assert_eq!(nodes[1].name, "BETA-ONE");
    assert_eq!(s.codebook_version().unwrap(), v0 + 1);
}
