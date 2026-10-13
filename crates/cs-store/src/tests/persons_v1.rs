//! The register of persons, organisations and groups: records, groups,
//! links to interviews, the pseudonymisation entries they contribute.

use super::*;
use crate::{GroupPatch, PersonFilter};
use cs_model::person::PersonPatch;
use cs_model::project::*;
use cs_model::pseudonym::{PseudonymConfig, PseudonymEntry, Pseudonymiser};
use std::collections::BTreeMap;

fn named(name: &str) -> PersonPatch {
    PersonPatch {
        name: Some(name.into()),
        ..Default::default()
    }
}

fn unflagged(name: &str) -> PersonPatch {
    PersonPatch {
        name: Some(name.into()),
        anonymise: Some(false),
        ..Default::default()
    }
}

fn is_kind(err: &anyhow::Error, want: &str) -> bool {
    match err.downcast_ref::<CsError>() {
        Some(CsError::Validation(_)) => want == "validation",
        Some(CsError::Guard(_)) => want == "guard",
        Some(CsError::NotFound(_)) => want == "not_found",
        None => false,
    }
}

fn second_interview(s: &Store) {
    let mut e = default_entry();
    e.interview_id = "ECO-17".into();
    e.folder_name = "ECO-17_Lib".into();
    s.upsert_interview(&e, ACTOR).unwrap();
}

#[test]
fn a_record_gets_a_stable_id_a_default_pseudonym_and_the_flag_on() {
    let (s, _d) = ready();
    let a = s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    let b = s.create_person(&named("Luc Favre"), ACTOR).unwrap();
    assert_eq!(a.person_id, "P-0001");
    assert_eq!(b.person_id, "P-0002");
    assert!(a.anonymise);
    assert_eq!(a.pseudonym, "Répondant 01");
    assert_eq!(b.pseudonym, "Répondant 02");
    // Ids and pseudonym numbers are not reused after a delete.
    s.delete_person("P-0002", false, ACTOR).unwrap();
    let c = s.create_person(&named("Eva Roth"), ACTOR).unwrap();
    assert_eq!(c.person_id, "P-0003");
    assert_eq!(c.pseudonym, "Répondant 03");
    // The pseudonym follows the project language.
    let mut cfg = s.lexicon_config().unwrap();
    cfg.language = "en".into();
    s.set_lexicon_config(&cfg, ACTOR).unwrap();
    let d = s.create_person(&named("John Doe"), ACTOR).unwrap();
    assert_eq!(d.pseudonym, "Participant 04");
    // An organisation has its own numbering and word.
    let o = s
        .create_person(
            &PersonPatch {
                kind: Some("organisation".into()),
                name: Some("Banque Alpha".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(o.pseudonym, "Organisation 01");
}

#[test]
fn the_pseudonym_is_editable_and_stays_unique() {
    let (s, _d) = ready();
    s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    let b = s.create_person(&named("Luc Favre"), ACTOR).unwrap();
    let e = s
        .update_person(
            &b.person_id,
            &PersonPatch {
                pseudonym: Some("répondant 01".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
    let ok = s
        .update_person(
            &b.person_id,
            &PersonPatch {
                pseudonym: Some("Détaillant A".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(ok.pseudonym, "Détaillant A");
    // An empty pseudonym asks for a new generated one.
    let again = s
        .update_person(
            &b.person_id,
            &PersonPatch {
                pseudonym: Some("".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(again.pseudonym, "Répondant 03");
}

#[test]
fn only_flagged_records_feed_the_list_and_unflagged_names_are_never_replaced() {
    let (s, _d) = ready();
    let a = s
        .create_person(
            &PersonPatch {
                name: Some("Anna Muller".into()),
                aliases: Some(vec!["Anna".into(), "A. Muller".into()]),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    s.create_person(&unflagged("Luc Favre"), ACTOR).unwrap();
    let cfg = s.pseudonym_config().unwrap();
    assert!(cfg.enabled, "a flagged record puts the list in force");
    let rules = cfg.pseudonymiser();
    let text = "Anna Muller et Luc Favre ; Anna dit que A. Muller part. Luc reste.";
    let masked = rules.mask(text);
    assert_eq!(
        masked,
        format!(
            "{p} et Luc Favre ; {p} dit que {p} part. Luc reste.",
            p = a.pseudonym
        )
    );
    assert!(!rules.contains_original(&masked));
    // Unflagging removes the entries; flagging again brings them back.
    s.update_person(
        &a.person_id,
        &PersonPatch {
            anonymise: Some(false),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    assert!(s.pseudonym_config().unwrap().entries.is_empty());
    assert!(!s.pseudonym_config().unwrap().enabled);
}

#[test]
fn manual_entries_are_kept_and_merged_but_never_shadow_the_register() {
    let (s, _d) = ready();
    let manual = PseudonymConfig {
        enabled: true,
        entries: vec![
            PseudonymEntry {
                kind: "organisation".into(),
                original: "Banque Beta".into(),
                pseudonym: "ORG-B".into(),
            },
            PseudonymEntry {
                kind: "person".into(),
                original: "Anna Muller".into(),
                pseudonym: "MAN-1".into(),
            },
        ],
    };
    s.set_pseudonym_config(&manual, ACTOR).unwrap();
    let a = s.create_person(&named("anna muller"), ACTOR).unwrap();
    let eff = s.effective_pseudonyms().unwrap();
    // The record wins over the manual entry of the same name.
    let originals: Vec<&str> = eff
        .config
        .entries
        .iter()
        .map(|e| e.original.as_str())
        .collect();
    assert_eq!(originals, vec!["anna muller", "Banque Beta"]);
    assert_eq!(eff.person_of, vec![Some(a.person_id.clone()), None]);
    assert_eq!(eff.shadowed.len(), 1);
    assert!(eff.shadowed[0].1.contains("P-0001"));
    // The stored manual list is untouched, and a PUT of the manual list does
    // not take the register's entries in.
    assert_eq!(s.manual_pseudonym_config().unwrap().entries.len(), 2);
    let next = PseudonymConfig {
        enabled: true,
        entries: eff.config.entries.clone(),
    };
    let stored = s.set_pseudonym_config(&next, ACTOR).unwrap();
    assert_eq!(
        stored.entries.len(),
        1,
        "the record's names are not kept by hand"
    );
    assert_eq!(stored.entries[0].original, "Banque Beta");
    // A manual entry for a record that is not anonymised is not applied.
    s.create_person(&unflagged("Luc Favre"), ACTOR).unwrap();
    let with_luc = PseudonymConfig {
        enabled: true,
        entries: vec![PseudonymEntry {
            kind: "person".into(),
            original: "Luc Favre".into(),
            pseudonym: "X1".into(),
        }],
    };
    let stored = s.set_pseudonym_config(&with_luc, ACTOR).unwrap();
    assert!(stored.entries.is_empty());
    let rules = s.pseudonym_config().unwrap().pseudonymiser();
    assert_eq!(rules.mask("Luc Favre"), "Luc Favre");
}

#[test]
fn the_flag_alone_puts_the_list_in_force_even_when_the_switch_is_off() {
    let (s, _d) = ready();
    assert!(!s.pseudonym_config().unwrap().enabled);
    s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    let cfg = s.pseudonym_config().unwrap();
    assert!(cfg.enabled);
    assert!(!s.manual_pseudonym_config().unwrap().enabled);
    assert_ne!(cfg.digest(), PseudonymConfig::default().digest());
}

#[test]
fn clashes_in_the_register_are_refused() {
    let (s, _d) = ready();
    s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    // The same name for a second flagged record.
    let e = s.create_person(&named("anna muller"), ACTOR).unwrap_err();
    assert!(is_kind(&e, "validation"));
    // An alias that is another record's name.
    let e = s
        .create_person(
            &PersonPatch {
                name: Some("Eva Roth".into()),
                aliases: Some(vec!["Anna Muller".into()]),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
    // A record that is not anonymised cannot carry a name a flagged one replaces.
    let e = s
        .create_person(&unflagged("Anna Muller"), ACTOR)
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
    // A pseudonym that contains a listed name would put it back.
    let e = s
        .create_person(
            &PersonPatch {
                name: Some("Eva Roth".into()),
                pseudonym: Some("Cousine de Anna Muller".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
    // An alias too short to be safe.
    let e = s
        .create_person(
            &PersonPatch {
                name: Some("Eva Roth".into()),
                aliases: Some(vec!["E".into()]),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
    // Nothing was written by the refused calls, not even a counter-consuming record.
    assert_eq!(s.all_persons().unwrap().len(), 1);
}

#[test]
fn a_first_name_alias_cannot_replace_part_of_an_unflagged_name() {
    let (s, _d) = ready();
    s.create_person(&unflagged("Anna Favre"), ACTOR).unwrap();
    let e = s
        .create_person(
            &PersonPatch {
                name: Some("Anna Muller".into()),
                aliases: Some(vec!["Anna".into()]),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
    // Without the first name alias it is fine.
    s.create_person(&named("Anna Muller"), ACTOR).unwrap();
}

#[test]
fn optional_fields_consent_and_validation() {
    let (s, _d) = ready();
    let p = s
        .create_person(
            &PersonPatch {
                name: Some(" Anna Muller ".into()),
                organisation: Some("Banque Alpha".into()),
                function: Some("Directrice".into()),
                email: Some("a@b.ch".into()),
                phone: Some("+41 79 000".into()),
                notes: Some("rencontrée au salon".into()),
                consent_status: Some("given".into()),
                consent_date: Some("2026-09-01".into()),
                consent_document: Some("consent/anna.pdf".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(p.name, "Anna Muller");
    assert_eq!(p.consent_status, "given");
    for bad in [
        PersonPatch {
            consent_status: Some("peut-être".into()),
            ..Default::default()
        },
        PersonPatch {
            consent_date: Some("01.09.2026".into()),
            ..Default::default()
        },
        PersonPatch {
            email: Some("pas-un-mail".into()),
            ..Default::default()
        },
        PersonPatch {
            kind: Some("organisation".into()),
            ..Default::default()
        },
    ] {
        let e = s.update_person(&p.person_id, &bad, ACTOR).unwrap_err();
        assert!(is_kind(&e, "validation"));
    }
    assert!(is_kind(
        &s.create_person(&PersonPatch::default(), ACTOR).unwrap_err(),
        "validation"
    ));
    assert!(is_kind(&s.person("P-9999").unwrap_err(), "not_found"));
}

#[test]
fn person_attributes_reuse_the_attribute_definitions_with_a_scope() {
    let (s, _d) = ready();
    let mut cfg = s.project_config().unwrap();
    cfg.attributes = vec![
        AttributeDef {
            id: "sector".into(),
            label: "Secteur".into(),
            kind: AttributeKind::Choice,
            choices: vec!["public".into(), "privé".into()],
            shared_with_model: false,
            scope: AttributeScope::Interview,
        },
        AttributeDef {
            id: "seniority".into(),
            label: "Ancienneté".into(),
            kind: AttributeKind::Number,
            choices: vec![],
            shared_with_model: true,
            scope: AttributeScope::Person,
        },
    ];
    s.set_project_config(&cfg, ACTOR).unwrap();
    // The interview sheet only knows the interview-scoped attribute.
    let view = s.interview_attributes(ID).unwrap();
    assert_eq!(view.definitions.len(), 1);
    assert_eq!(view.definitions[0].id, "sector");
    let mut by = BTreeMap::new();
    by.insert("seniority".to_string(), Some("12".to_string()));
    assert!(s.set_interview_attributes(ID, &by, ACTOR).is_err());
    // The person takes the person-scoped one, validated by type.
    let p = s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    let bad = PersonPatch {
        attributes: Some(BTreeMap::from([(
            "seniority".to_string(),
            Some("long".to_string()),
        )])),
        ..Default::default()
    };
    assert!(is_kind(
        &s.update_person(&p.person_id, &bad, ACTOR).unwrap_err(),
        "validation"
    ));
    let unknown = PersonPatch {
        attributes: Some(BTreeMap::from([(
            "sector".to_string(),
            Some("public".to_string()),
        )])),
        ..Default::default()
    };
    assert!(is_kind(
        &s.update_person(&p.person_id, &unknown, ACTOR).unwrap_err(),
        "validation"
    ));
    let ok = PersonPatch {
        attributes: Some(BTreeMap::from([(
            "seniority".to_string(),
            Some("12".to_string()),
        )])),
        ..Default::default()
    };
    let p = s.update_person(&p.person_id, &ok, ACTOR).unwrap();
    assert_eq!(p.attributes["seniority"], "12");
    // A value in use protects the definition (type, scope, removal).
    let mut retyped = s.project_config().unwrap();
    retyped.attributes[1].kind = AttributeKind::Text;
    assert!(is_kind(
        &s.set_project_config(&retyped, ACTOR).unwrap_err(),
        "guard"
    ));
    let mut rescoped = s.project_config().unwrap();
    rescoped.attributes[1].scope = AttributeScope::Interview;
    assert!(is_kind(
        &s.set_project_config(&rescoped, ACTOR).unwrap_err(),
        "guard"
    ));
    let mut removed = s.project_config().unwrap();
    removed.attributes.pop();
    assert!(is_kind(
        &s.set_project_config(&removed, ACTOR).unwrap_err(),
        "guard"
    ));
    // Only respondents' shared attributes are offered to the prompt.
    s.link_person(&p.person_id, ID, "respondent", ACTOR)
        .unwrap();
    assert_eq!(
        s.shared_person_attributes(ID).unwrap(),
        vec![("Ancienneté".to_string(), "12".to_string())]
    );
    s.link_person(&p.person_id, ID, "mentioned", ACTOR).unwrap();
    assert!(s.shared_person_attributes(ID).unwrap().is_empty());
}

#[test]
fn groups_form_a_tree_with_unique_sibling_names_and_no_cycle() {
    let (s, _d) = ready();
    let root = s.create_group("Détaillants", None, "", ACTOR).unwrap();
    let ch = s
        .create_group("Suisse romande", Some(&root.group_id), "", ACTOR)
        .unwrap();
    let vd = s
        .create_group("Vaud", Some(&ch.group_id), "", ACTOR)
        .unwrap();
    assert_eq!(
        [
            root.group_id.as_str(),
            ch.group_id.as_str(),
            vd.group_id.as_str()
        ],
        ["G-0001", "G-0002", "G-0003"]
    );
    // Same name at the same level, any case: refused. Same name elsewhere: fine.
    assert!(is_kind(
        &s.create_group("détaillants", None, "", ACTOR).unwrap_err(),
        "validation"
    ));
    s.create_group("Vaud", None, "", ACTOR).unwrap();
    assert!(is_kind(
        &s.create_group("", None, "", ACTOR).unwrap_err(),
        "validation"
    ));
    assert!(is_kind(
        &s.create_group("a/b", None, "", ACTOR).unwrap_err(),
        "validation"
    ));
    assert!(is_kind(
        &s.create_group("X", Some("G-0099"), "", ACTOR).unwrap_err(),
        "not_found"
    ));
    // Cycles: under itself, under a descendant.
    for target in [&root.group_id, &ch.group_id, &vd.group_id] {
        let e = s
            .update_group(
                &root.group_id,
                &GroupPatch {
                    parent_id: Some(Some(target.clone())),
                    ..Default::default()
                },
                ACTOR,
            )
            .unwrap_err();
        assert!(is_kind(&e, "validation"), "{target}");
        assert!(e.to_string().contains("cycle"));
    }
    // Rename and move.
    let renamed = s
        .update_group(
            &vd.group_id,
            &GroupPatch {
                name: Some("Vaud et Fribourg".into()),
                description: Some("cantons".into()),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(renamed.name, "Vaud et Fribourg");
    let moved = s
        .update_group(
            &vd.group_id,
            &GroupPatch {
                parent_id: Some(None),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(moved.parent_id, None);
    // A move onto a sibling name is refused.
    let e = s
        .update_group(
            &ch.group_id,
            &GroupPatch {
                name: Some("Détaillants".into()),
                parent_id: Some(None),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(is_kind(&e, "validation"));
}

#[test]
fn deleting_a_group_needs_it_empty_or_a_reassignment() {
    let (s, _d) = ready();
    let a = s.create_group("A", None, "", ACTOR).unwrap();
    let b = s.create_group("B", Some(&a.group_id), "", ACTOR).unwrap();
    let c = s.create_group("C", None, "", ACTOR).unwrap();
    let p = s
        .create_person(
            &PersonPatch {
                name: Some("Anna Muller".into()),
                group_ids: Some(vec![b.group_id.clone()]),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    // A has a sub-group, B has a member: both refused.
    assert!(is_kind(
        &s.delete_group(&a.group_id, None, ACTOR).unwrap_err(),
        "guard"
    ));
    assert!(is_kind(
        &s.delete_group(&b.group_id, None, ACTOR).unwrap_err(),
        "guard"
    ));
    // Reassigning to itself or a descendant is refused.
    assert!(is_kind(
        &s.delete_group(&a.group_id, Some(&b.group_id), ACTOR)
            .unwrap_err(),
        "validation"
    ));
    assert!(is_kind(
        &s.delete_group(&a.group_id, Some(&a.group_id), ACTOR)
            .unwrap_err(),
        "validation"
    ));
    // With a target: sub-groups and members move there.
    s.delete_group(&b.group_id, Some(&c.group_id), ACTOR)
        .unwrap();
    assert_eq!(
        s.person(&p.person_id).unwrap().group_ids,
        vec![c.group_id.clone()]
    );
    s.delete_group(&a.group_id, None, ACTOR).unwrap();
    assert_eq!(s.person_groups().unwrap().len(), 1);
    // An empty group goes without a target.
    s.delete_group(&c.group_id, Some(&c.group_id), ACTOR)
        .unwrap_err();
    let d = s.create_group("D", None, "", ACTOR).unwrap();
    s.delete_group(&d.group_id, None, ACTOR).unwrap();
    // Ids are not reused.
    let e = s.create_group("E", None, "", ACTOR).unwrap();
    assert_eq!(e.group_id, "G-0005");
}

#[test]
fn a_person_can_belong_to_several_groups_and_the_counts_roll_up() {
    let (s, _d) = ready();
    second_interview(&s);
    let ret = s.create_group("Détaillants", None, "", ACTOR).unwrap();
    let ch = s
        .create_group("Suisse romande", Some(&ret.group_id), "", ACTOR)
        .unwrap();
    let be = s
        .create_group("Belgique", Some(&ret.group_id), "", ACTOR)
        .unwrap();
    let other = s.create_group("Fabricants", None, "", ACTOR).unwrap();
    let mk = |name: &str, groups: &[&str]| PersonPatch {
        name: Some(name.into()),
        group_ids: Some(groups.iter().map(|g| g.to_string()).collect()),
        ..Default::default()
    };
    let a = s
        .create_person(&mk("Anna Muller", &[&ch.group_id, &other.group_id]), ACTOR)
        .unwrap();
    let b = s
        .create_person(&mk("Luc Favre", &[&ch.group_id]), ACTOR)
        .unwrap();
    let c = s
        .create_person(&mk("Eva Roth", &[&be.group_id]), ACTOR)
        .unwrap();
    s.create_person(&mk("Sans Groupe", &[]), ACTOR).unwrap();
    s.link_person(&a.person_id, ID, "respondent", ACTOR)
        .unwrap();
    s.link_person(&b.person_id, ID, "co_respondent", ACTOR)
        .unwrap();
    s.link_person(&c.person_id, "ECO-17", "respondent", ACTOR)
        .unwrap();
    let t = s.group_tree().unwrap();
    assert_eq!((t.n_groups, t.n_persons, t.n_ungrouped_persons), (4, 4, 1));
    let top = t
        .groups
        .iter()
        .find(|g| g.group.name == "Détaillants")
        .unwrap();
    assert_eq!(top.n_persons, 0);
    assert_eq!(top.n_persons_total, 3);
    assert_eq!(top.n_interviews_total, 2);
    assert_eq!(top.children.len(), 2);
    assert_eq!(top.children[0].path, "Détaillants/Belgique");
    let rom = &top.children[1];
    assert_eq!(
        (rom.n_persons, rom.n_persons_total, rom.n_interviews_total),
        (2, 2, 1)
    );
    // Filters follow the subtree.
    let in_top = s
        .persons(&PersonFilter {
            group_id: Some(ret.group_id.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(in_top.len(), 3);
    let in_iv = s
        .persons(&PersonFilter {
            interview_id: Some("ECO-17".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(in_iv.len(), 1);
    assert_eq!(in_iv[0].person.name, "Eva Roth");
    let q = s
        .persons(&PersonFilter {
            q: Some("luc".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(q.len(), 1);
    // The path helper and ensure_group_path agree and reuse what exists.
    let id = s
        .ensure_group_path("Détaillants/Suisse romande", ACTOR)
        .unwrap();
    assert_eq!(id, ch.group_id);
    let new = s
        .ensure_group_path("Détaillants/Tessin/Lugano", ACTOR)
        .unwrap();
    assert_eq!(
        crate::group_path(&s.person_groups().unwrap(), &new),
        "Détaillants/Tessin/Lugano"
    );
}

#[test]
fn links_to_interviews_carry_a_role_and_an_interview_has_several_persons() {
    let (s, _d) = ready();
    let a = s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    let b = s.create_person(&named("Luc Favre"), ACTOR).unwrap();
    let c = s.create_person(&unflagged("Eva Roth"), ACTOR).unwrap();
    s.link_person(&b.person_id, ID, "interviewer", ACTOR)
        .unwrap();
    s.link_person(&c.person_id, ID, "mentioned", ACTOR).unwrap();
    let v = s
        .link_person(&a.person_id, ID, "respondent", ACTOR)
        .unwrap();
    let roles: Vec<(&str, &str)> = v
        .iter()
        .map(|ip| (ip.role.as_str(), ip.person.person_id.as_str()))
        .collect();
    assert_eq!(
        roles,
        vec![
            ("respondent", "P-0001"),
            ("mentioned", "P-0003"),
            ("interviewer", "P-0002")
        ]
    );
    // A role change is an update, not a second link.
    let v = s
        .link_person(&a.person_id, ID, "co_respondent", ACTOR)
        .unwrap();
    assert_eq!(v.len(), 3);
    assert!(is_kind(
        &s.link_person(&a.person_id, ID, "boss", ACTOR).unwrap_err(),
        "validation"
    ));
    assert!(is_kind(
        &s.link_person("P-0099", ID, "respondent", ACTOR)
            .unwrap_err(),
        "not_found"
    ));
    assert!(is_kind(
        &s.link_person(&a.person_id, "NOPE-01", "respondent", ACTOR)
            .unwrap_err(),
        "not_found"
    ));
    // The detail has the interviews, groups and attributes.
    let d = s.person_detail(&a.person_id).unwrap();
    assert_eq!(d.interviews.len(), 1);
    assert_eq!(d.interviews[0].role, "co_respondent");
    // Replace the whole set.
    let v = s
        .set_interview_persons(ID, &[(a.person_id.clone(), "respondent".into())], ACTOR)
        .unwrap();
    assert_eq!(v.len(), 1);
    assert!(is_kind(
        &s.set_interview_persons(
            ID,
            &[
                (a.person_id.clone(), "respondent".into()),
                (a.person_id.clone(), "mentioned".into())
            ],
            ACTOR
        )
        .unwrap_err(),
        "validation"
    ));
    // Unlink; unlinking twice is a 404.
    s.unlink_person(&a.person_id, ID, ACTOR).unwrap();
    assert!(is_kind(
        &s.unlink_person(&a.person_id, ID, ACTOR).unwrap_err(),
        "not_found"
    ));
}

#[test]
fn deleting_a_linked_person_is_refused_unless_the_links_go_too() {
    let (s, _d) = ready();
    let a = s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    s.link_person(&a.person_id, ID, "respondent", ACTOR)
        .unwrap();
    assert!(is_kind(
        &s.delete_person(&a.person_id, false, ACTOR).unwrap_err(),
        "guard"
    ));
    s.delete_person(&a.person_id, true, ACTOR).unwrap();
    assert!(s.all_persons().unwrap().is_empty());
    assert!(s.person_links().unwrap().is_empty());
    assert!(s.pseudonym_config().unwrap().entries.is_empty());
}

#[test]
fn every_write_is_audited_without_names() {
    let (s, _d) = ready();
    let before = s.audit_events(None).unwrap().len();
    let g = s.create_group("Détaillants", None, "", ACTOR).unwrap();
    let a = s
        .create_person(
            &PersonPatch {
                name: Some("Anna Muller".into()),
                email: Some("anna@secret.ch".into()),
                notes: Some("note privée".into()),
                aliases: Some(vec!["Anna".into()]),
                group_ids: Some(vec![g.group_id.clone()]),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    s.update_person(
        &a.person_id,
        &PersonPatch {
            function: Some("Directrice".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    // No change: no event.
    s.update_person(
        &a.person_id,
        &PersonPatch {
            function: Some("Directrice".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    s.link_person(&a.person_id, ID, "respondent", ACTOR)
        .unwrap();
    s.update_group(
        &g.group_id,
        &GroupPatch {
            name: Some("Magasins".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    s.unlink_person(&a.person_id, ID, ACTOR).unwrap();
    s.delete_person(&a.person_id, false, ACTOR).unwrap();
    s.delete_group(&g.group_id, None, ACTOR).unwrap();
    let events = s.audit_events(None).unwrap();
    let actions: Vec<&str> = events[before..].iter().map(|e| e.action.as_str()).collect();
    assert_eq!(
        actions,
        vec![
            "person_group.create",
            "person.create",
            "person.update",
            "person.link",
            "person_group.update",
            "person.unlink",
            "person.delete",
            "person_group.delete"
        ]
    );
    let all = serde_json::to_string(&events).unwrap();
    for secret in ["anna@secret.ch", "note privée", "Anna Muller", "Directrice"] {
        assert!(!all.contains(secret), "{secret} leaked into the audit log");
    }
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn the_migration_is_idempotent_and_keeps_the_register() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("p.db");
    {
        let s = Store::open(&path).unwrap();
        s.create_person(&named("Anna Muller"), ACTOR).unwrap();
        s.create_group("G", None, "", ACTOR).unwrap();
    }
    {
        let s = Store::open(&path).unwrap();
        crate::persons::migrate(s.connection()).unwrap();
        crate::persons::migrate(s.connection()).unwrap();
        assert_eq!(s.all_persons().unwrap().len(), 1);
        assert_eq!(s.person_groups().unwrap().len(), 1);
        let v: i64 = s
            .connection()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        assert!(SCHEMA_VERSION >= 8);
        // The counters survive: the next id continues.
        assert_eq!(
            s.create_person(&named("Luc Favre"), ACTOR)
                .unwrap()
                .person_id,
            "P-0002"
        );
    }
    // A database made before the register (version 7, no persons table).
    let old = dir.path().join("old.db");
    {
        let s = Store::open(&old).unwrap();
        let c = s.connection();
        c.execute_batch(
            "DROP TABLE person_group_members; DROP TABLE person_interviews; DROP TABLE persons;
             DROP TABLE person_groups; DROP TABLE person_counters; PRAGMA user_version = 7;",
        )
        .unwrap();
    }
    let s = Store::open(&old).unwrap();
    assert!(s.all_persons().unwrap().is_empty());
    s.create_person(&named("Anna Muller"), ACTOR).unwrap();
}

#[test]
fn effective_digest_changes_with_the_register_and_is_order_free() {
    let (s, _d) = ready();
    let d0 = s.pseudonym_config().unwrap().digest();
    let a = s.create_person(&named("Anna Muller"), ACTOR).unwrap();
    let d1 = s.pseudonym_config().unwrap().digest();
    assert_ne!(d0, d1);
    s.update_person(
        &a.person_id,
        &PersonPatch {
            notes: Some("x".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    assert_eq!(s.pseudonym_config().unwrap().digest(), d1);
    let rules: Pseudonymiser = s.pseudonym_config().unwrap().pseudonymiser();
    assert!(rules.contains_original("Anna Muller"));
}
