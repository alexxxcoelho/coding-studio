//! Import of the register from a CSV: dry run, apply, atomicity.

use super::*;
use cs_model::person::PersonPatch;
use cs_model::project::*;

fn named(name: &str) -> PersonPatch {
    PersonPatch {
        name: Some(name.into()),
        ..Default::default()
    }
}

fn second_interview(s: &Store) {
    let mut e = default_entry();
    e.interview_id = "ECO-17".into();
    e.folder_name = "ECO-17_Lib".into();
    s.upsert_interview(&e, ACTOR).unwrap();
}

const CSV: &str = "name,organisation,role,groups,anonymise,interviews\n\
Anna Muller,Banque Alpha,Directrice,Détaillants/Suisse romande,oui,ECO-16\n\
Luc Favre,,Acheteur,\"Détaillants/Suisse romande;Fabricants\",non,ECO-16:co_respondent;ECO-17:mentioned\n\
Eva Roth,Cave Beta,,,,\n";

#[test]
fn import_dry_run_reports_and_writes_nothing() {
    let (s, _d) = ready();
    second_interview(&s);
    let before = s.audit_events(None).unwrap().len();
    let r = s.import_persons(CSV, true, ACTOR).unwrap();
    assert!(r.ok && r.dry_run && !r.applied, "{:?}", r.problems);
    assert_eq!(
        (r.n_rows, r.n_create, r.n_update, r.n_unchanged),
        (3, 3, 0, 0)
    );
    assert_eq!(r.n_links, 3);
    assert_eq!(
        r.groups_to_create,
        vec!["Détaillants", "Détaillants/Suisse romande", "Fabricants"]
    );
    assert_eq!(r.rows[0].person_id, "P-0001");
    assert!(s.all_persons().unwrap().is_empty());
    assert!(s.person_groups().unwrap().is_empty());
    assert_eq!(s.audit_events(None).unwrap().len(), before);
}

#[test]
fn import_applies_in_one_transaction_with_one_audit_event() {
    let (s, _d) = ready();
    second_interview(&s);
    let dry = s.import_persons(CSV, true, ACTOR).unwrap();
    let before = s.audit_events(None).unwrap().len();
    let r = s.import_persons(CSV, false, ACTOR).unwrap();
    assert!(r.applied && r.ok);
    // The dry run and the apply say the same thing.
    assert_eq!(r.rows, dry.rows);
    assert_eq!(s.audit_events(None).unwrap().len(), before + 1);
    let all = s.all_persons().unwrap();
    assert_eq!(all.len(), 3);
    let anna = all.iter().find(|p| p.name == "Anna Muller").unwrap();
    let luc = all.iter().find(|p| p.name == "Luc Favre").unwrap();
    assert!(anna.anonymise && !luc.anonymise);
    assert_eq!(anna.organisation, "Banque Alpha");
    assert_eq!(anna.function, "Directrice");
    assert_eq!(luc.group_ids.len(), 2);
    assert_eq!(s.person_groups().unwrap().len(), 3);
    let links = s.person_links().unwrap();
    assert_eq!(links.len(), 3);
    assert!(links.iter().any(|l| l.person_id == luc.person_id
        && l.interview_id == "ECO-17"
        && l.role == "mentioned"));
    // Applying the same file again changes nothing.
    let again = s.import_persons(CSV, false, ACTOR).unwrap();
    assert_eq!(
        (again.n_create, again.n_update, again.n_unchanged),
        (0, 0, 3)
    );
    assert_eq!(s.all_persons().unwrap().len(), 3);
    // The audit event holds counts and the file digest, never a name.
    let events = serde_json::to_string(&s.audit_events(None).unwrap()).unwrap();
    assert!(events.contains(&dry.csv_sha256));
    assert!(!events.contains("Banque Alpha") && !events.contains("Anna Muller"));
    // Counters moved on: the next record continues the numbering.
    assert_eq!(
        s.create_person(&named("Zoé Blanc"), ACTOR)
            .unwrap()
            .person_id,
        "P-0004"
    );
}

#[test]
fn a_file_with_one_bad_row_changes_nothing_and_lists_every_problem() {
    let (s, _d) = ready();
    let bad = "name,anonymise,interviews,consent_date\n\
Anna Muller,oui,ECO-16,2026-13-01\n\
Luc Favre,peut-être,ECO-16,\n\
Eva Roth,oui,NOPE-9,\n\
Anna Muller,oui,,\n\
,oui,,\n\
Zoé Blanc,non,ECO-16,\n";
    let r = s.import_persons(bad, false, ACTOR).unwrap();
    assert!(!r.ok && !r.applied);
    let rows: Vec<usize> = r.problems.iter().map(|p| p.row).collect();
    assert_eq!(rows, vec![2, 3, 4, 5, 6]);
    assert!(s.all_persons().unwrap().is_empty());
    assert!(s.person_groups().unwrap().is_empty());
    // No name column.
    let r = s.import_persons("foo,bar\n1,2\n", false, ACTOR).unwrap();
    assert!(!r.ok);
    assert_eq!(r.problems[0].row, 1);
    assert!(!s.import_persons("", false, ACTOR).unwrap().ok);
}

#[test]
fn import_updates_by_name_or_id_keeping_empty_cells_and_adding_groups() {
    let (s, _d) = ready();
    let anna = s
        .create_person(
            &PersonPatch {
                name: Some("Anna Muller".into()),
                function: Some("Directrice".into()),
                email: Some("a@b.ch".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    let csv = "person_id;nom;fonction;groupes;aliases;anonymiser\n\
P-0001;Anna Muller;;Fabricants;Anna|A. M.;non\n";
    let r = s.import_persons(csv, false, ACTOR).unwrap();
    assert!(r.ok, "{:?}", r.problems);
    assert_eq!((r.n_create, r.n_update), (0, 1));
    let p = s.person(&anna.person_id).unwrap();
    assert_eq!(p.function, "Directrice", "an empty cell leaves the field");
    assert_eq!(p.email, "a@b.ch");
    assert_eq!(p.aliases, vec!["Anna", "A. M."]);
    assert!(!p.anonymise);
    assert_eq!(p.group_ids.len(), 1);
    assert!(s.pseudonym_config().unwrap().entries.is_empty());
    // An unknown id is a problem.
    let r = s
        .import_persons("person_id,name\nP-0042,Luc\n", true, ACTOR)
        .unwrap();
    assert!(!r.ok);
}

#[test]
fn import_enforces_the_register_rules_per_row() {
    let (s, _d) = ready();
    // Two flagged rows that would share a name through an alias.
    let csv = "name,aliases\nAnna Muller,Anna\nAnna Favre,Anna\n";
    let r = s.import_persons(csv, true, ACTOR).unwrap();
    assert!(!r.ok);
    assert_eq!(r.problems.len(), 1);
    assert_eq!(r.problems[0].row, 3);
    // Person attributes through attr: columns.
    let mut cfg = s.project_config().unwrap();
    cfg.attributes = vec![AttributeDef {
        id: "seniority".into(),
        label: "Ancienneté".into(),
        kind: AttributeKind::Number,
        choices: vec![],
        shared_with_model: false,
        scope: AttributeScope::Person,
    }];
    s.set_project_config(&cfg, ACTOR).unwrap();
    let r = s
        .import_persons("name,attr:seniority\nAnna Muller,12\n", false, ACTOR)
        .unwrap();
    assert!(r.applied, "{:?}", r.problems);
    assert_eq!(s.person("P-0001").unwrap().attributes["seniority"], "12");
    let r = s
        .import_persons("name,attr:seniority\nLuc Favre,long\n", true, ACTOR)
        .unwrap();
    assert!(!r.ok);
}
