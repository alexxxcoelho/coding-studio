//! The register of the people interviewed: persons and organisations, the
//! tree of groups, the links to interviews, and the pseudonymisation entries
//! the register contributes.
//!
//! Rules kept here:
//! - Ids are stable and never reused (`P-0001`, `G-0001`): they come from
//!   counters, not from `MAX(id) + 1`.
//! - Every write is one transaction with its audit event. The event holds the
//!   ids, the names of the fields changed and the non-identifying values (the
//!   flag, the pseudonym, the digest of the list); it never holds a name, an
//!   alias, an e-mail address, a phone number or a note, because the audit log
//!   cannot be edited and must not become a second copy of personal data.
//! - A flagged record feeds the pseudonymisation list; an unflagged record is
//!   never replaced. A change that would break either rule is refused as a
//!   whole (see [`Store::check_register`]).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::person::{
    check_consent, check_consent_date, check_entries, check_kind, check_role, default_pseudonym,
    derive_entries, group_id, normalise_aliases, person_id, GroupNode, Person, PersonGroup,
    PersonInterviewLink, PersonPatch, RESPONDENT_ROLES,
};
use cs_model::project::{AttributeDef, ProjectConfig};
use cs_model::pseudonym::{PseudonymConfig, PseudonymEntry, Pseudonymiser};
use rusqlite::{params, Connection};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

use crate::{append_event, from_json, guard_err, not_found, now, to_json, validation, Store};

const SCHEMA: &str = include_str!("persons_schema.sql");

/// Applies the schema of the register. Safe to run any number of times.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

// ---------------------------------------------------------------- views

/// Filters of the list of persons. All are optional and combine with AND.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PersonFilter {
    /// `person` or `organisation`.
    #[serde(default)]
    pub kind: Option<String>,
    /// A group; its sub-groups are included.
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub interview_id: Option<String>,
    #[serde(default)]
    pub anonymise: Option<bool>,
    /// Text found in the name, an alias, the organisation or the function
    /// (case ignored).
    #[serde(default)]
    pub q: Option<String>,
}

/// A person with the number of interviews it is linked to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonRow {
    #[serde(flatten)]
    pub person: Person,
    pub n_interviews: usize,
}

/// A group of a person, with its path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupRef {
    pub group_id: String,
    pub name: String,
    pub path: String,
}

/// An interview of a person, with what makes its context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonInterview {
    pub interview_id: String,
    pub role: String,
    pub folder_name: String,
    /// Values of the interview attributes.
    pub attributes: BTreeMap<String, String>,
}

/// A person with their interviews, groups and attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonDetail {
    #[serde(flatten)]
    pub person: Person,
    pub groups: Vec<GroupRef>,
    pub interviews: Vec<PersonInterview>,
    /// Definitions of the person attributes of the project.
    pub attribute_definitions: Vec<AttributeDef>,
    /// Definitions of the interview attributes (to read `interviews[].attributes`).
    pub interview_attribute_definitions: Vec<AttributeDef>,
}

/// A person of an interview, with the role in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterviewPerson {
    pub role: String,
    #[serde(flatten)]
    pub person: Person,
}

/// The tree of groups with the totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupTree {
    pub groups: Vec<GroupNode>,
    pub n_groups: usize,
    pub n_persons: usize,
    pub n_ungrouped_persons: usize,
}

fn double_option<'de, D>(d: D) -> std::result::Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(d)?))
}

/// Fields of a group to change. `parent_id` absent: unchanged; `null`: moved
/// to the root.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GroupPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id: Option<Option<String>>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Where the entries of the effective list come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePseudonyms {
    /// The list in force: the register's entries, then the manual ones that
    /// they do not shadow.
    pub config: PseudonymConfig,
    /// For each entry of `config.entries`, the record it comes from (`None`:
    /// entered by hand in the list).
    pub person_of: Vec<Option<String>>,
    /// Manual entries left out, with the reason.
    pub shadowed: Vec<(PseudonymEntry, String)>,
}

// ----------------------------------------------------------------- rows

const PERSON_COLS: &str = "person_id, kind, name, organisation, person_function, email, phone, notes, consent_status, consent_date, consent_document, anonymise, pseudonym, aliases, attributes, created_at, updated_at";

fn person_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Person> {
    let aliases: String = r.get(13)?;
    let attributes: String = r.get(14)?;
    Ok(Person {
        person_id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        organisation: r.get(3)?,
        function: r.get(4)?,
        email: r.get(5)?,
        phone: r.get(6)?,
        notes: r.get(7)?,
        consent_status: r.get(8)?,
        consent_date: r.get(9)?,
        consent_document: r.get(10)?,
        anonymise: r.get::<_, i64>(11)? != 0,
        pseudonym: r.get(12)?,
        aliases: serde_json::from_str(&aliases).unwrap_or_default(),
        attributes: serde_json::from_str(&attributes).unwrap_or_default(),
        group_ids: Vec::new(),
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

fn clean(field: &str, value: &str, max: usize) -> Result<String> {
    let v = value.trim();
    if v.chars().count() > max {
        return Err(validation(format!(
            "{field} is longer than {max} characters"
        )));
    }
    Ok(v.to_string())
}

fn group_name(raw: &str) -> Result<String> {
    let n = raw.trim();
    if n.is_empty() {
        return Err(validation("the name of a group is empty"));
    }
    if n.chars().count() > 200 || n.contains(['/', '\n', '\r']) {
        return Err(validation(
            "the name of a group has at most 200 characters, without '/' or line break",
        ));
    }
    Ok(n.to_string())
}

impl Store {
    fn next_counter(&self, conn: &Connection, name: &str) -> Result<i64> {
        conn.execute(
            "INSERT OR IGNORE INTO person_counters (name, value) VALUES (?1, 0)",
            [name],
        )?;
        conn.execute(
            "UPDATE person_counters SET value = value + 1 WHERE name = ?1",
            [name],
        )?;
        Ok(conn.query_row(
            "SELECT value FROM person_counters WHERE name = ?1",
            [name],
            |r| r.get(0),
        )?)
    }

    fn language(&self) -> String {
        self.lexicon_config()
            .map(|c| c.language)
            .unwrap_or_else(|_| "fr".into())
    }

    /// A pseudonym of the kind that no record uses yet.
    fn fresh_pseudonym(&self, conn: &Connection, kind: &str) -> Result<String> {
        let language = self.language();
        let used: BTreeSet<String> = self
            .all_persons_on(conn)?
            .iter()
            .map(|p| p.pseudonym.to_lowercase())
            .collect();
        loop {
            let n = self.next_counter(conn, &format!("pseudonym.{kind}"))?;
            let candidate = default_pseudonym(kind, &language, n);
            if !used.contains(&candidate.to_lowercase()) {
                return Ok(candidate);
            }
        }
    }

    fn all_persons_on(&self, conn: &Connection) -> Result<Vec<Person>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT {PERSON_COLS} FROM persons ORDER BY person_id"
        ))?;
        let mut persons: Vec<Person> = stmt
            .query_map([], person_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut stmt = conn
            .prepare("SELECT person_id, group_id FROM person_group_members ORDER BY group_id")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (p, g) = row?;
            members.entry(p).or_default().push(g);
        }
        for p in &mut persons {
            p.group_ids = members.remove(&p.person_id).unwrap_or_default();
        }
        Ok(persons)
    }

    /// Every record, by id.
    pub fn all_persons(&self) -> Result<Vec<Person>> {
        self.all_persons_on(&self.conn)
    }

    /// One record. Not found: 404.
    pub fn person(&self, id: &str) -> Result<Person> {
        self.all_persons()?
            .into_iter()
            .find(|p| p.person_id == id)
            .ok_or_else(|| not_found(format!("person {id}")))
    }

    fn person_interview_map(&self) -> Result<BTreeMap<String, Vec<(String, String)>>> {
        let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT person_id, interview_id, role FROM person_interviews ORDER BY interview_id",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (p, i, role) = row?;
            out.entry(p).or_default().push((i, role));
        }
        Ok(out)
    }

    /// The register, filtered, by id.
    pub fn persons(&self, filter: &PersonFilter) -> Result<Vec<PersonRow>> {
        let links = self.person_interview_map()?;
        let groups = self.person_groups()?;
        let subtree: Option<BTreeSet<String>> = match &filter.group_id {
            Some(g) => {
                self.require_group(&groups, g)?;
                Some(subtree_ids(&groups, g))
            }
            None => None,
        };
        let kind = filter.kind.as_deref().map(check_kind).transpose()?;
        if let Some(i) = &filter.interview_id {
            self.require_interview(i)?;
        }
        let q = filter
            .q
            .as_deref()
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let mut out = Vec::new();
        for p in self.all_persons()? {
            if kind.as_ref().is_some_and(|k| *k != p.kind)
                || filter.anonymise.is_some_and(|a| a != p.anonymise)
                || subtree
                    .as_ref()
                    .is_some_and(|s| !p.group_ids.iter().any(|g| s.contains(g)))
            {
                continue;
            }
            let mine = links.get(&p.person_id).cloned().unwrap_or_default();
            if let Some(i) = &filter.interview_id {
                if !mine.iter().any(|(iv, _)| iv == i) {
                    continue;
                }
            }
            if let Some(q) = &q {
                let hay = [&p.name, &p.organisation, &p.function]
                    .iter()
                    .map(|s| s.to_lowercase())
                    .chain(p.aliases.iter().map(|a| a.to_lowercase()))
                    .collect::<Vec<_>>();
                if !hay.iter().any(|h| h.contains(q.as_str())) {
                    continue;
                }
            }
            out.push(PersonRow {
                n_interviews: mine.len(),
                person: p,
            });
        }
        Ok(out)
    }

    // -------------------------------------------------------- validation

    /// The rules of the register applied to a whole set of records and the
    /// manual list: pseudonyms are distinct, no name is used by two records or
    /// by a pseudonym, and a record that is not anonymised does not carry a
    /// name that a flagged record would replace.
    pub(crate) fn check_register(
        &self,
        persons: &[Person],
        manual: &PseudonymConfig,
    ) -> Result<()> {
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        for p in persons {
            if p.pseudonym.trim().is_empty() {
                return Err(validation(format!("{} has no pseudonym", p.person_id)));
            }
            if let Some(other) = seen.insert(p.pseudonym.to_lowercase(), &p.person_id) {
                return Err(validation(format!(
                    "the pseudonym {:?} is used by {other} and by {}",
                    p.pseudonym, p.person_id
                )));
            }
        }
        let eff = effective_from(persons, manual);
        let mut owned: Vec<(PseudonymEntry, String)> = Vec::new();
        for (e, owner) in eff.config.entries.iter().zip(&eff.person_of) {
            owned.push((
                e.clone(),
                owner.clone().unwrap_or_else(|| "the manual list".into()),
            ));
        }
        check_entries(&owned)?;
        let derived = derive_entries(persons);
        let rules = Pseudonymiser::new(&derived);
        for p in persons.iter().filter(|p| !p.anonymise) {
            for n in std::iter::once(&p.name).chain(p.aliases.iter()) {
                if rules.contains_original(n) {
                    return Err(validation(format!(
                        "{} is not anonymised but its name {:?} contains a name or alias that another record replaces",
                        p.person_id, n
                    )));
                }
            }
        }
        Ok(())
    }

    fn check_patch(
        &self,
        base: &Person,
        patch: &PersonPatch,
        cfg: &ProjectConfig,
        creating: bool,
    ) -> Result<Person> {
        let mut p = base.clone();
        if let Some(k) = &patch.kind {
            let k = check_kind(k)?;
            if !creating && k != p.kind {
                return Err(validation("the kind of a record cannot change"));
            }
            p.kind = k;
        }
        if let Some(n) = &patch.name {
            let n = clean("name", n, 200)?;
            if n.contains(['\n', '\r']) {
                return Err(validation("the name has a line break"));
            }
            p.name = n;
        }
        if p.name.is_empty() {
            return Err(validation("a record needs a name"));
        }
        for (field, value, target, max) in [
            ("organisation", &patch.organisation, &mut p.organisation, 200),
            ("function", &patch.function, &mut p.function, 200),
            ("email", &patch.email, &mut p.email, 200),
            ("phone", &patch.phone, &mut p.phone, 60),
            ("notes", &patch.notes, &mut p.notes, 10_000),
            (
                "consent_document",
                &patch.consent_document,
                &mut p.consent_document,
                1000,
            ),
        ] {
            if let Some(v) = value {
                *target = clean(field, v, max)?;
            }
        }
        if !p.email.is_empty() && !p.email.contains('@') {
            return Err(validation(format!("email {:?} has no @", p.email)));
        }
        if let Some(s) = &patch.consent_status {
            p.consent_status = check_consent(s)?;
        }
        if let Some(d) = &patch.consent_date {
            p.consent_date = check_consent_date(d)?;
        }
        if let Some(a) = patch.anonymise {
            p.anonymise = a;
        }
        p.aliases = match &patch.aliases {
            Some(a) => normalise_aliases(&p.name, a)?,
            None => normalise_aliases(&p.name, &p.aliases)?,
        };
        if let Some(changes) = &patch.attributes {
            for (id, raw) in changes {
                let def = cfg
                    .person_attribute(id)
                    .ok_or_else(|| validation(format!("unknown person attribute {id}")))?;
                match raw
                    .as_deref()
                    .map(|r| def.check_value(r))
                    .transpose()?
                    .flatten()
                {
                    Some(v) => {
                        p.attributes.insert(id.clone(), v);
                    }
                    None => {
                        p.attributes.remove(id);
                    }
                }
            }
        }
        Ok(p)
    }

    fn require_group(&self, groups: &[PersonGroup], id: &str) -> Result<()> {
        if groups.iter().any(|g| g.group_id == id) {
            Ok(())
        } else {
            Err(not_found(format!("group {id}")))
        }
    }

    // ------------------------------------------------------ persons: writes

    /// Creates a record. `name` is required; the pseudonym is generated when
    /// not given; `anonymise` is true unless said otherwise.
    pub fn create_person(&self, patch: &PersonPatch, actor: &str) -> Result<Person> {
        let cfg = self.project_config()?;
        let manual = self.manual_pseudonym_config()?;
        let groups = self.person_groups()?;
        let digest_before = self.pseudonym_config()?.digest();
        let kind = check_kind(patch.kind.as_deref().unwrap_or("person"))?;
        let tx = self.conn.unchecked_transaction()?;
        let n = self.next_counter(&tx, "person")?;
        let stamp = now();
        let blank = Person {
            person_id: person_id(n),
            kind: kind.clone(),
            name: String::new(),
            organisation: String::new(),
            function: String::new(),
            email: String::new(),
            phone: String::new(),
            notes: String::new(),
            consent_status: String::new(),
            consent_date: String::new(),
            consent_document: String::new(),
            anonymise: true,
            pseudonym: String::new(),
            aliases: Vec::new(),
            attributes: BTreeMap::new(),
            group_ids: Vec::new(),
            created_at: stamp.clone(),
            updated_at: stamp,
        };
        let mut p = self.check_patch(&blank, patch, &cfg, true)?;
        p.pseudonym = match patch.pseudonym.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => clean("pseudonym", s, 100)?,
            _ => self.fresh_pseudonym(&tx, &kind)?,
        };
        if let Some(gs) = &patch.group_ids {
            for g in gs {
                self.require_group(&groups, g)?;
            }
            p.group_ids = dedup(gs);
        }
        let mut all = self.all_persons_on(&tx)?;
        all.push(p.clone());
        self.check_register(&all, &manual)?;
        tx.execute(
            &format!(
                "INSERT INTO persons ({PERSON_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)"
            ),
            params![
                p.person_id,
                p.kind,
                p.name,
                p.organisation,
                p.function,
                p.email,
                p.phone,
                p.notes,
                p.consent_status,
                p.consent_date,
                p.consent_document,
                i64::from(p.anonymise),
                p.pseudonym,
                to_json(&p.aliases),
                to_json(&p.attributes),
                p.created_at,
                p.updated_at,
            ],
        )?;
        for g in &p.group_ids {
            tx.execute(
                "INSERT INTO person_group_members (person_id, group_id) VALUES (?1, ?2)",
                params![p.person_id, g],
            )?;
        }
        let digest_after = effective_from(&all, &manual).config.digest();
        append_event(
            &tx,
            actor,
            "person.create",
            None,
            Some(&p.person_id),
            json!({
                "person_id": p.person_id,
                "kind": p.kind,
                "anonymise": p.anonymise,
                "pseudonym": p.pseudonym,
                "n_aliases": p.aliases.len(),
                "group_ids": p.group_ids,
                "pseudonymisation_digest_before": digest_before,
                "pseudonymisation_digest_after": digest_after,
            }),
        )?;
        tx.commit()?;
        Ok(p)
    }

    /// Changes a record. Nothing is written (and nothing audited) when the
    /// patch changes nothing.
    pub fn update_person(&self, id: &str, patch: &PersonPatch, actor: &str) -> Result<Person> {
        let cfg = self.project_config()?;
        let manual = self.manual_pseudonym_config()?;
        let groups = self.person_groups()?;
        let tx = self.conn.unchecked_transaction()?;
        let mut all = self.all_persons_on(&tx)?;
        let idx = all
            .iter()
            .position(|p| p.person_id == id)
            .ok_or_else(|| not_found(format!("person {id}")))?;
        let before = all[idx].clone();
        let mut p = self.check_patch(&before, patch, &cfg, false)?;
        if let Some(ps) = patch.pseudonym.as_deref() {
            let ps = ps.trim();
            p.pseudonym = if ps.is_empty() {
                self.fresh_pseudonym(&tx, &p.kind)?
            } else {
                clean("pseudonym", ps, 100)?
            };
        }
        if let Some(gs) = &patch.group_ids {
            for g in gs {
                self.require_group(&groups, g)?;
            }
            p.group_ids = dedup(gs);
        }
        if p == before {
            return Ok(before);
        }
        p.updated_at = now();
        let digest_before = effective_from(&all, &manual).config.digest();
        all[idx] = p.clone();
        self.check_register(&all, &manual)?;
        tx.execute(
            "UPDATE persons SET name=?2, organisation=?3, person_function=?4, email=?5, phone=?6,
               notes=?7, consent_status=?8, consent_date=?9, consent_document=?10, anonymise=?11,
               pseudonym=?12, aliases=?13, attributes=?14, updated_at=?15 WHERE person_id=?1",
            params![
                p.person_id,
                p.name,
                p.organisation,
                p.function,
                p.email,
                p.phone,
                p.notes,
                p.consent_status,
                p.consent_date,
                p.consent_document,
                i64::from(p.anonymise),
                p.pseudonym,
                to_json(&p.aliases),
                to_json(&p.attributes),
                p.updated_at,
            ],
        )?;
        if p.group_ids != before.group_ids {
            tx.execute("DELETE FROM person_group_members WHERE person_id = ?1", [id])?;
            for g in &p.group_ids {
                tx.execute(
                    "INSERT INTO person_group_members (person_id, group_id) VALUES (?1, ?2)",
                    params![id, g],
                )?;
            }
        }
        let mut changed: Vec<&str> = Vec::new();
        macro_rules! diff {
            ($($f:ident),*) => { $( if p.$f != before.$f { changed.push(stringify!($f)); } )* };
        }
        diff!(
            name,
            organisation,
            function,
            email,
            phone,
            notes,
            consent_status,
            consent_date,
            consent_document,
            anonymise,
            pseudonym,
            aliases,
            attributes,
            group_ids
        );
        let digest_after = effective_from(&all, &manual).config.digest();
        append_event(
            &tx,
            actor,
            "person.update",
            None,
            Some(id),
            json!({
                "person_id": id,
                "changed": changed,
                "anonymise_before": before.anonymise,
                "anonymise_after": p.anonymise,
                "pseudonym_before": before.pseudonym,
                "pseudonym_after": p.pseudonym,
                "pseudonymisation_digest_before": digest_before,
                "pseudonymisation_digest_after": digest_after,
            }),
        )?;
        tx.commit()?;
        Ok(p)
    }

    /// Deletes a record. Refused (409) while it is linked to an interview,
    /// unless `with_links` is true, which removes the links too.
    pub fn delete_person(&self, id: &str, with_links: bool, actor: &str) -> Result<()> {
        let p = self.person(id)?;
        let links = self.person_interview_map()?.remove(id).unwrap_or_default();
        if !links.is_empty() && !with_links {
            return Err(guard_err(format!(
                "{id} is linked to {} interview(s); unlink it first or delete with its links",
                links.len()
            )));
        }
        let manual = self.manual_pseudonym_config()?;
        let digest_before = self.pseudonym_config()?.digest();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM person_interviews WHERE person_id = ?1", [id])?;
        tx.execute("DELETE FROM person_group_members WHERE person_id = ?1", [id])?;
        tx.execute("DELETE FROM persons WHERE person_id = ?1", [id])?;
        let digest_after = effective_from(&self.all_persons_on(&tx)?, &manual)
            .config
            .digest();
        append_event(
            &tx,
            actor,
            "person.delete",
            None,
            Some(id),
            json!({
                "person_id": id,
                "kind": p.kind,
                "anonymise": p.anonymise,
                "n_links_removed": links.len(),
                "n_groups": p.group_ids.len(),
                "pseudonymisation_digest_before": digest_before,
                "pseudonymisation_digest_after": digest_after,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    // ----------------------------------------------------- persons: reading

    /// A person with their interviews, groups and attributes.
    pub fn person_detail(&self, id: &str) -> Result<PersonDetail> {
        let person = self.person(id)?;
        let groups = self.person_groups()?;
        let refs = person
            .group_ids
            .iter()
            .filter_map(|g| groups.iter().find(|x| x.group_id == *g))
            .map(|g| GroupRef {
                group_id: g.group_id.clone(),
                name: g.name.clone(),
                path: group_path(&groups, &g.group_id),
            })
            .collect();
        let mut interviews = Vec::new();
        for (iv, role) in self.person_interview_map()?.remove(id).unwrap_or_default() {
            let folder: String = self.conn.query_row(
                "SELECT folder_name FROM interviews WHERE interview_id = ?1",
                [&iv],
                |r| r.get(0),
            )?;
            interviews.push(PersonInterview {
                attributes: self.interview_attributes(&iv)?.values,
                interview_id: iv,
                role,
                folder_name: folder,
            });
        }
        let cfg = self.project_config()?;
        Ok(PersonDetail {
            person,
            groups: refs,
            interviews,
            attribute_definitions: cfg.person_attributes().cloned().collect(),
            interview_attribute_definitions: cfg.interview_attributes().cloned().collect(),
        })
    }

    /// The persons of an interview with their role, respondents first.
    pub fn interview_persons(&self, interview_id: &str) -> Result<Vec<InterviewPerson>> {
        self.require_interview(interview_id)?;
        let mut stmt = self
            .conn
            .prepare("SELECT person_id, role FROM person_interviews WHERE interview_id = ?1")?;
        let links: BTreeMap<String, String> = stmt
            .query_map([interview_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out: Vec<InterviewPerson> = self
            .all_persons()?
            .into_iter()
            .filter_map(|p| {
                links.get(&p.person_id).map(|role| InterviewPerson {
                    role: role.clone(),
                    person: p,
                })
            })
            .collect();
        let rank = |r: &str| {
            cs_model::person::INTERVIEW_ROLES
                .iter()
                .position(|x| *x == r)
                .unwrap_or(9)
        };
        out.sort_by(|a, b| {
            rank(&a.role)
                .cmp(&rank(&b.role))
                .then_with(|| a.person.person_id.cmp(&b.person.person_id))
        });
        Ok(out)
    }

    /// Every link of the register.
    pub fn person_links(&self) -> Result<Vec<PersonInterviewLink>> {
        let mut stmt = self.conn.prepare(
            "SELECT person_id, interview_id, role FROM person_interviews ORDER BY person_id, interview_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PersonInterviewLink {
                    person_id: r.get(0)?,
                    interview_id: r.get(1)?,
                    role: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    // --------------------------------------------------------------- links

    /// Links a person to an interview with a role, or changes the role of an
    /// existing link.
    pub fn link_person(
        &self,
        person_id: &str,
        interview_id: &str,
        role: &str,
        actor: &str,
    ) -> Result<Vec<InterviewPerson>> {
        let role = check_role(role)?;
        self.person(person_id)?;
        self.require_interview(interview_id)?;
        let before: Option<String> = self
            .conn
            .query_row(
                "SELECT role FROM person_interviews WHERE person_id = ?1 AND interview_id = ?2",
                [person_id, interview_id],
                |r| r.get(0),
            )
            .ok();
        if before.as_deref() == Some(role.as_str()) {
            return self.interview_persons(interview_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO person_interviews (person_id, interview_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(person_id, interview_id) DO UPDATE SET role = excluded.role",
            params![person_id, interview_id, role, now()],
        )?;
        append_event(
            &tx,
            actor,
            "person.link",
            Some(interview_id),
            Some(person_id),
            json!({"person_id": person_id, "interview_id": interview_id,
                   "role_before": before, "role_after": role}),
        )?;
        tx.commit()?;
        self.interview_persons(interview_id)
    }

    /// Removes the link of a person to an interview. Not found when absent.
    pub fn unlink_person(
        &self,
        person_id: &str,
        interview_id: &str,
        actor: &str,
    ) -> Result<Vec<InterviewPerson>> {
        self.require_interview(interview_id)?;
        let tx = self.conn.unchecked_transaction()?;
        let n = tx.execute(
            "DELETE FROM person_interviews WHERE person_id = ?1 AND interview_id = ?2",
            [person_id, interview_id],
        )?;
        if n == 0 {
            return Err(not_found(format!(
                "{person_id} is not linked to {interview_id}"
            )));
        }
        append_event(
            &tx,
            actor,
            "person.unlink",
            Some(interview_id),
            Some(person_id),
            json!({"person_id": person_id, "interview_id": interview_id}),
        )?;
        tx.commit()?;
        self.interview_persons(interview_id)
    }

    /// Replaces the persons of an interview by the given (person, role) list.
    pub fn set_interview_persons(
        &self,
        interview_id: &str,
        links: &[(String, String)],
        actor: &str,
    ) -> Result<Vec<InterviewPerson>> {
        self.require_interview(interview_id)?;
        let known: BTreeSet<String> = self
            .all_persons()?
            .into_iter()
            .map(|p| p.person_id)
            .collect();
        let mut wanted: BTreeMap<String, String> = BTreeMap::new();
        for (p, r) in links {
            if !known.contains(p) {
                return Err(not_found(format!("person {p}")));
            }
            if wanted.insert(p.clone(), check_role(r)?).is_some() {
                return Err(validation(format!("{p} is listed twice")));
            }
        }
        let current: BTreeMap<String, String> = self
            .interview_persons(interview_id)?
            .into_iter()
            .map(|ip| (ip.person.person_id, ip.role))
            .collect();
        if current == wanted {
            return self.interview_persons(interview_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM person_interviews WHERE interview_id = ?1",
            [interview_id],
        )?;
        for (p, r) in &wanted {
            tx.execute(
                "INSERT INTO person_interviews (person_id, interview_id, role, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![p, interview_id, r, now()],
            )?;
        }
        append_event(
            &tx,
            actor,
            "interview.persons",
            Some(interview_id),
            None,
            json!({"before": current, "after": wanted}),
        )?;
        tx.commit()?;
        self.interview_persons(interview_id)
    }

    // -------------------------------------------------------------- groups

    /// Every group, by id.
    pub fn person_groups(&self) -> Result<Vec<PersonGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT group_id, name, parent_id, description, created_at, updated_at
             FROM person_groups ORDER BY group_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PersonGroup {
                    group_id: r.get(0)?,
                    name: r.get(1)?,
                    parent_id: r.get(2)?,
                    description: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// One group. Not found: 404.
    pub fn person_group(&self, id: &str) -> Result<PersonGroup> {
        self.person_groups()?
            .into_iter()
            .find(|g| g.group_id == id)
            .ok_or_else(|| not_found(format!("group {id}")))
    }

    /// The tree with the roll-up counts, children sorted by name.
    pub fn group_tree(&self) -> Result<GroupTree> {
        let groups = self.person_groups()?;
        let persons = self.all_persons()?;
        let links = self.person_interview_map()?;
        let mut roots: Vec<&PersonGroup> =
            groups.iter().filter(|g| g.parent_id.is_none()).collect();
        roots.sort_by_key(|g| g.name.to_lowercase());
        let tree = roots
            .into_iter()
            .map(|r| build_node(&groups, &persons, &links, r, 0))
            .collect();
        Ok(GroupTree {
            groups: tree,
            n_groups: groups.len(),
            n_persons: persons.len(),
            n_ungrouped_persons: persons.iter().filter(|p| p.group_ids.is_empty()).count(),
        })
    }

    fn sibling_clash(
        &self,
        groups: &[PersonGroup],
        parent: Option<&str>,
        name: &str,
        except: Option<&str>,
    ) -> Result<()> {
        if groups.iter().any(|g| {
            g.parent_id.as_deref() == parent
                && g.name.to_lowercase() == name.to_lowercase()
                && Some(g.group_id.as_str()) != except
        }) {
            return Err(validation(format!(
                "a group named {name:?} already exists at this level"
            )));
        }
        Ok(())
    }

    /// Creates a group under `parent_id` (the root when `None`).
    pub fn create_group(
        &self,
        name: &str,
        parent_id: Option<&str>,
        description: &str,
        actor: &str,
    ) -> Result<PersonGroup> {
        let name = group_name(name)?;
        let description = clean("description", description, 2000)?;
        let groups = self.person_groups()?;
        if let Some(p) = parent_id {
            self.require_group(&groups, p)?;
        }
        self.sibling_clash(&groups, parent_id, &name, None)?;
        let tx = self.conn.unchecked_transaction()?;
        let id = group_id(self.next_counter(&tx, "group")?);
        let stamp = now();
        tx.execute(
            "INSERT INTO person_groups (group_id, name, parent_id, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, name, parent_id, description, stamp],
        )?;
        append_event(
            &tx,
            actor,
            "person_group.create",
            None,
            Some(&id),
            json!({"group_id": id, "name": name, "parent_id": parent_id}),
        )?;
        tx.commit()?;
        self.person_group(&id)
    }

    /// Renames, moves or describes a group. A move that would put a group
    /// under itself or under one of its descendants is refused (400).
    pub fn update_group(&self, id: &str, patch: &GroupPatch, actor: &str) -> Result<PersonGroup> {
        let groups = self.person_groups()?;
        let before = groups
            .iter()
            .find(|g| g.group_id == id)
            .cloned()
            .ok_or_else(|| not_found(format!("group {id}")))?;
        let mut g = before.clone();
        if let Some(n) = &patch.name {
            g.name = group_name(n)?;
        }
        if let Some(d) = &patch.description {
            g.description = clean("description", d, 2000)?;
        }
        if let Some(parent) = &patch.parent_id {
            if let Some(p) = parent {
                self.require_group(&groups, p)?;
                if subtree_ids(&groups, id).contains(p) {
                    return Err(validation(format!(
                        "{id} cannot be moved under {p}: it would create a cycle"
                    )));
                }
            }
            g.parent_id = parent.clone();
        }
        if g == before {
            return Ok(before);
        }
        self.sibling_clash(&groups, g.parent_id.as_deref(), &g.name, Some(id))?;
        g.updated_at = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE person_groups SET name=?2, parent_id=?3, description=?4, updated_at=?5 WHERE group_id=?1",
            params![id, g.name, g.parent_id, g.description, g.updated_at],
        )?;
        append_event(
            &tx,
            actor,
            "person_group.update",
            None,
            Some(id),
            json!({
                "group_id": id,
                "name_before": before.name, "name_after": g.name,
                "parent_before": before.parent_id, "parent_after": g.parent_id,
                "description_changed": before.description != g.description,
            }),
        )?;
        tx.commit()?;
        Ok(g)
    }

    /// Deletes a group. Refused (409) while it has sub-groups or members,
    /// unless `reassign_to` names another group (not the group itself nor one
    /// of its descendants) that receives them.
    pub fn delete_group(&self, id: &str, reassign_to: Option<&str>, actor: &str) -> Result<()> {
        let groups = self.person_groups()?;
        let g = groups
            .iter()
            .find(|g| g.group_id == id)
            .cloned()
            .ok_or_else(|| not_found(format!("group {id}")))?;
        let children: Vec<&PersonGroup> = groups
            .iter()
            .filter(|c| c.parent_id.as_deref() == Some(id))
            .collect();
        let members: Vec<String> = self
            .all_persons()?
            .into_iter()
            .filter(|p| p.group_ids.iter().any(|x| x == id))
            .map(|p| p.person_id)
            .collect();
        if (!children.is_empty() || !members.is_empty()) && reassign_to.is_none() {
            return Err(guard_err(format!(
                "group {id} is not empty ({} sub-group(s), {} person(s)); move them or give a group to reassign them to",
                children.len(),
                members.len()
            )));
        }
        if let Some(t) = reassign_to {
            self.require_group(&groups, t)?;
            if subtree_ids(&groups, id).contains(t) {
                return Err(validation(format!(
                    "{t} is {id} or below it: choose another group to reassign to"
                )));
            }
            for c in &children {
                self.sibling_clash(&groups, Some(t), &c.name, Some(&c.group_id))?;
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        if let Some(t) = reassign_to {
            tx.execute(
                "UPDATE person_groups SET parent_id = ?2, updated_at = ?3 WHERE parent_id = ?1",
                params![id, t, now()],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO person_group_members (person_id, group_id)
                 SELECT person_id, ?2 FROM person_group_members WHERE group_id = ?1",
                params![id, t],
            )?;
        }
        tx.execute("DELETE FROM person_group_members WHERE group_id = ?1", [id])?;
        tx.execute("DELETE FROM person_groups WHERE group_id = ?1", [id])?;
        append_event(
            &tx,
            actor,
            "person_group.delete",
            None,
            Some(id),
            json!({
                "group_id": id,
                "name": g.name,
                "reassigned_to": reassign_to,
                "n_children": children.len(),
                "n_persons": members.len(),
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Finds or creates the groups of a path such as `Détaillants/Suisse
    /// romande` and returns the id of the last one.
    pub fn ensure_group_path(&self, path: &str, actor: &str) -> Result<String> {
        let mut parent: Option<String> = None;
        for part in path.split('/') {
            let name = group_name(part)?;
            let groups = self.person_groups()?;
            let found = groups
                .iter()
                .find(|g| g.parent_id == parent && g.name.to_lowercase() == name.to_lowercase());
            let id = match found {
                Some(g) => g.group_id.clone(),
                None => {
                    self.create_group(&name, parent.as_deref(), "", actor)?
                        .group_id
                }
            };
            parent = Some(id);
        }
        parent.ok_or_else(|| validation("the group path is empty"))
    }

    // ------------------------------------------------- pseudonymisation

    /// The list as stored: the entries typed by hand and the switch.
    pub fn manual_pseudonym_config(&self) -> Result<PseudonymConfig> {
        match self.setting(cs_model::pseudonym::PSEUDONYMISATION_KEY)? {
            Some(raw) => from_json(&raw),
            None => Ok(PseudonymConfig::default()),
        }
    }

    /// The list in force, with the origin of each entry.
    pub fn effective_pseudonyms(&self) -> Result<EffectivePseudonyms> {
        Ok(effective_from(
            &self.all_persons()?,
            &self.manual_pseudonym_config()?,
        ))
    }

    /// Attributes of the persons of an interview that the project shares with
    /// the model: (label, value) pairs. Only respondents and co-respondents
    /// count. The label carries the id of the record when there are several,
    /// never a name. The prompt masks the values like any other text.
    pub fn shared_person_attributes(&self, interview_id: &str) -> Result<Vec<(String, String)>> {
        let cfg = self.project_config()?;
        let defs: Vec<&AttributeDef> = cfg
            .person_attributes()
            .filter(|d| d.shared_with_model)
            .collect();
        if defs.is_empty() {
            return Ok(Vec::new());
        }
        let people: Vec<InterviewPerson> = self
            .interview_persons(interview_id)?
            .into_iter()
            .filter(|ip| RESPONDENT_ROLES.contains(&ip.role.as_str()))
            .collect();
        let several = people.len() > 1;
        let mut out = Vec::new();
        for ip in &people {
            for d in &defs {
                if let Some(v) = ip.person.attributes.get(&d.id) {
                    let label = if several {
                        format!("{} ({})", d.label, ip.person.person_id)
                    } else {
                        d.label.clone()
                    };
                    out.push((label, v.clone()));
                }
            }
        }
        Ok(out)
    }

    /// Values of the person attributes across the register (for the checks of
    /// a change of configuration).
    pub(crate) fn person_attribute_values(
        &self,
    ) -> Result<Vec<(String, BTreeMap<String, String>)>> {
        Ok(self
            .all_persons()?
            .into_iter()
            .map(|p| (p.person_id, p.attributes))
            .collect())
    }
}

fn dedup(ids: &[String]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for i in ids {
        if !v.contains(i) {
            v.push(i.clone());
        }
    }
    v.sort();
    v
}

/// The list in force for a register and a manual list: the flagged records
/// first, then the manual entries that no record shadows. A manual entry is
/// left out when a flagged record already lists the same name, and when a
/// record that is not anonymised carries the same name (it must never be
/// replaced). The list is on when the manual switch is on or when a record is
/// flagged: flagging a record is the decision to replace it.
pub(crate) fn effective_from(persons: &[Person], manual: &PseudonymConfig) -> EffectivePseudonyms {
    let mut entries: Vec<PseudonymEntry> = Vec::new();
    let mut person_of: Vec<Option<String>> = Vec::new();
    for p in persons.iter().filter(|p| p.anonymise) {
        for e in derive_entries(std::slice::from_ref(p)) {
            entries.push(e);
            person_of.push(Some(p.person_id.clone()));
        }
    }
    let n_derived = entries.len();
    let mut shadowed = Vec::new();
    for m in &manual.entries {
        let key = m.original.to_lowercase();
        if let Some(i) = entries[..n_derived]
            .iter()
            .position(|e| e.original.to_lowercase() == key)
        {
            shadowed.push((
                m.clone(),
                format!("managed by {}", person_of[i].clone().unwrap_or_default()),
            ));
            continue;
        }
        if let Some(p) = persons.iter().find(|p| {
            !p.anonymise
                && std::iter::once(&p.name)
                    .chain(p.aliases.iter())
                    .any(|n| n.to_lowercase() == key)
        }) {
            shadowed.push((m.clone(), format!("{} is not anonymised", p.person_id)));
            continue;
        }
        entries.push(m.clone());
        person_of.push(None);
    }
    EffectivePseudonyms {
        config: PseudonymConfig {
            enabled: manual.enabled || n_derived > 0,
            entries,
        },
        person_of,
        shadowed,
    }
}

fn subtree_ids(groups: &[PersonGroup], root: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(g) = stack.pop() {
        if out.insert(g.clone()) {
            for c in groups.iter().filter(|c| c.parent_id.as_deref() == Some(&g)) {
                stack.push(c.group_id.clone());
            }
        }
    }
    out
}

/// `Détaillants/Suisse romande`.
pub fn group_path(groups: &[PersonGroup], id: &str) -> String {
    let mut parts = Vec::new();
    let mut cur = groups.iter().find(|g| g.group_id == id);
    let mut guard = 0;
    while let Some(g) = cur {
        parts.push(g.name.clone());
        guard += 1;
        if guard > 1000 {
            break;
        }
        cur = g
            .parent_id
            .as_ref()
            .and_then(|p| groups.iter().find(|x| x.group_id == *p));
    }
    parts.reverse();
    parts.join("/")
}

fn build_node(
    groups: &[PersonGroup],
    persons: &[Person],
    links: &BTreeMap<String, Vec<(String, String)>>,
    g: &PersonGroup,
    depth: usize,
) -> GroupNode {
    let sub = subtree_ids(groups, &g.group_id);
    let members_total: BTreeSet<&str> = persons
        .iter()
        .filter(|p| p.group_ids.iter().any(|x| sub.contains(x)))
        .map(|p| p.person_id.as_str())
        .collect();
    let interviews: BTreeSet<&str> = members_total
        .iter()
        .flat_map(|p| links.get(*p).into_iter().flatten())
        .map(|(i, _)| i.as_str())
        .collect();
    let mut kids: Vec<&PersonGroup> = groups
        .iter()
        .filter(|c| c.parent_id.as_deref() == Some(&g.group_id))
        .collect();
    kids.sort_by_key(|c| c.name.to_lowercase());
    GroupNode {
        path: group_path(groups, &g.group_id),
        depth,
        n_persons: persons
            .iter()
            .filter(|p| p.group_ids.iter().any(|x| *x == g.group_id))
            .count(),
        n_persons_total: members_total.len(),
        n_interviews_total: interviews.len(),
        children: kids
            .into_iter()
            .map(|c| build_node(groups, persons, links, c, depth + 1))
            .collect(),
        group: g.clone(),
    }
}
