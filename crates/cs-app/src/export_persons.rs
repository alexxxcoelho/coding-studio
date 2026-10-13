//! `persons.csv` and `groups.csv`: the register of the people interviewed in
//! the exported packages.
//!
//! Two shapes of `persons.csv`:
//! - the full one, with names, aliases, contact details, notes and the path of
//!   the consent document. It is in the normal packages and can be read back
//!   by the import (`POST /api/persons/import`);
//! - the public one, for the anonymised export and the verification package:
//!   the name column holds the pseudonym of a record that is anonymised and
//!   the real name of one that is not; aliases, e-mail, phone, notes and the
//!   consent document are left out; the organisation text of an anonymised
//!   person is replaced by the pseudonym of the matching organisation record,
//!   or hidden when no such record exists.

use std::collections::BTreeMap;

use anyhow::Result;
use cs_model::person::{Person, PersonGroup};
use cs_store::{group_path, Store};

use crate::export::csv_field;

/// Shown where the organisation of an anonymised person cannot be named.
pub(crate) const HIDDEN: &str = "[masquée]";

fn line(cells: &[String]) -> String {
    let mut l = cells
        .iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",");
    l.push('\n');
    l
}

fn yes_no(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

/// The records an export covers: those linked to one of `interview_ids`, or
/// all of them when `None`.
fn covered(store: &Store, interview_ids: Option<&[String]>) -> Result<Vec<Person>> {
    let persons = store.all_persons()?;
    let Some(ids) = interview_ids else {
        return Ok(persons);
    };
    let linked: std::collections::BTreeSet<String> = store
        .person_links()?
        .into_iter()
        .filter(|l| ids.contains(&l.interview_id))
        .map(|l| l.person_id)
        .collect();
    Ok(persons
        .into_iter()
        .filter(|p| linked.contains(&p.person_id))
        .collect())
}

fn paths_of(p: &Person, groups: &[PersonGroup]) -> String {
    p.group_ids
        .iter()
        .map(|g| group_path(groups, g))
        .collect::<Vec<_>>()
        .join(";")
}

fn interviews_of(
    p: &Person,
    links: &BTreeMap<String, Vec<(String, String)>>,
    ids: Option<&[String]>,
) -> String {
    links
        .get(&p.person_id)
        .into_iter()
        .flatten()
        .filter(|(iv, _)| ids.is_none_or(|ids| ids.contains(iv)))
        .map(|(iv, role)| format!("{iv}:{role}"))
        .collect::<Vec<_>>()
        .join(";")
}

fn link_map(store: &Store) -> Result<BTreeMap<String, Vec<(String, String)>>> {
    let mut m: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for l in store.person_links()? {
        m.entry(l.person_id)
            .or_default()
            .push((l.interview_id, l.role));
    }
    Ok(m)
}

/// The full `persons.csv`, readable by the import.
pub(crate) fn persons_csv_full(store: &Store, interview_ids: Option<&[String]>) -> Result<String> {
    let cfg = store.project_config()?;
    let defs: Vec<_> = cfg.person_attributes().collect();
    let groups = store.person_groups()?;
    let links = link_map(store)?;
    let mut head = vec![
        "person_id",
        "kind",
        "name",
        "aliases",
        "organisation",
        "function",
        "email",
        "phone",
        "notes",
        "consent_status",
        "consent_date",
        "consent_document",
        "anonymise",
        "pseudonym",
        "groups",
        "interviews",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    head.extend(defs.iter().map(|d| format!("attr:{}", d.id)));
    let mut out = line(&head);
    for p in covered(store, interview_ids)? {
        let mut cells = vec![
            p.person_id.clone(),
            p.kind.clone(),
            p.name.clone(),
            p.aliases.join(";"),
            p.organisation.clone(),
            p.function.clone(),
            p.email.clone(),
            p.phone.clone(),
            p.notes.clone(),
            p.consent_status.clone(),
            p.consent_date.clone(),
            p.consent_document.clone(),
            yes_no(p.anonymise),
            p.pseudonym.clone(),
            paths_of(&p, &groups),
            interviews_of(&p, &links, interview_ids),
        ];
        cells.extend(
            defs.iter()
                .map(|d| p.attributes.get(&d.id).cloned().unwrap_or_default()),
        );
        out.push_str(&line(&cells));
    }
    Ok(out)
}

/// The organisation text of a record as the public register shows it.
fn public_organisation(p: &Person, all: &[Person]) -> String {
    if p.organisation.is_empty() || !p.anonymise {
        return p.organisation.clone();
    }
    let key = p.organisation.to_lowercase();
    let org = all.iter().find(|o| {
        o.kind == "organisation"
            && std::iter::once(&o.name)
                .chain(o.aliases.iter())
                .any(|n| n.to_lowercase() == key)
    });
    match org {
        Some(o) if o.anonymise => o.pseudonym.clone(),
        Some(o) => o.name.clone(),
        None => HIDDEN.to_string(),
    }
}

/// The public `persons.csv`: a pseudonym for a record that is anonymised, the
/// real name for one that is not, no contact detail.
pub(crate) fn persons_csv_public(
    store: &Store,
    interview_ids: Option<&[String]>,
) -> Result<String> {
    let cfg = store.project_config()?;
    let defs: Vec<_> = cfg.person_attributes().collect();
    let groups = store.person_groups()?;
    let links = link_map(store)?;
    let all = store.all_persons()?;
    let mut head = vec![
        "person_id",
        "kind",
        "name",
        "anonymise",
        "pseudonym",
        "organisation",
        "function",
        "groups",
        "interviews",
        "consent_status",
        "consent_date",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    head.extend(defs.iter().map(|d| format!("attr:{}", d.id)));
    let mut out = line(&head);
    for p in covered(store, interview_ids)? {
        let mut cells = vec![
            p.person_id.clone(),
            p.kind.clone(),
            if p.anonymise {
                p.pseudonym.clone()
            } else {
                p.name.clone()
            },
            yes_no(p.anonymise),
            p.pseudonym.clone(),
            public_organisation(&p, &all),
            p.function.clone(),
            paths_of(&p, &groups),
            interviews_of(&p, &links, interview_ids),
            p.consent_status.clone(),
            p.consent_date.clone(),
        ];
        cells.extend(
            defs.iter()
                .map(|d| p.attributes.get(&d.id).cloned().unwrap_or_default()),
        );
        out.push_str(&line(&cells));
    }
    Ok(out)
}

/// `groups.csv`: the tree, depth first, with the roll-up counts.
pub(crate) fn groups_csv(store: &Store) -> Result<String> {
    fn walk(out: &mut String, nodes: &[cs_model::person::GroupNode]) {
        for n in nodes {
            out.push_str(&line(&[
                n.group.group_id.clone(),
                n.group.parent_id.clone().unwrap_or_default(),
                n.group.name.clone(),
                n.path.clone(),
                n.group.description.clone(),
                n.n_persons.to_string(),
                n.n_persons_total.to_string(),
                n.n_interviews_total.to_string(),
            ]));
            walk(out, &n.children);
        }
    }
    let mut out = line(
        &[
            "group_id",
            "parent_id",
            "name",
            "path",
            "description",
            "n_persons",
            "n_persons_total",
            "n_interviews_total",
        ]
        .map(String::from),
    );
    walk(&mut out, &store.group_tree()?.groups);
    Ok(out)
}

/// Descriptions of the two files for the README of a package.
pub(crate) const PERSONS_README: &str = "The register of the people interviewed, `person_id,kind,name,aliases,organisation,function,email,phone,notes,consent_status,consent_date,consent_document,anonymise,pseudonym,groups,interviews`, then one `attr:<id>` column per person attribute. Names are included: this file is in the normal package only. `groups` are slash-separated paths, several separated by `;`; `interviews` are `ID:role` separated by `;`. The file can be read back by the persons import.";
pub(crate) const GROUPS_README: &str = "The tree of groups of persons, `group_id,parent_id,name,path,description,n_persons,n_persons_total,n_interviews_total`, depth first. `n_persons_total` and `n_interviews_total` count distinct persons and interviews in the group and below it.";
pub(crate) const PERSONS_PUBLIC_README: &str = "The register of the people interviewed, in its public form, `person_id,kind,name,anonymise,pseudonym,organisation,function,groups,interviews,consent_status,consent_date`, then one `attr:<id>` column per person attribute. `name` is the pseudonym of a record that is anonymised and the real name of one that is not; aliases, e-mail, phone, notes and the consent document are left out; the organisation of an anonymised person is its pseudonym, or `[masquée]` when it is not in the register. `groups` are slash-separated paths; `interviews` are `ID:role` separated by `;`.";
