//! Import of the register from a CSV file: a dry run that reports every
//! problem and every change, then the same run applied in one transaction.
//!
//! The run is simulated in memory on copies of the records, groups and links,
//! with the same rules as the single-record calls (`check_patch`,
//! `check_register`). Nothing is written unless the whole file is accepted, so
//! a file with one wrong row changes nothing.
//!
//! Columns (header row required; French or English names, case ignored):
//! `name`/`nom` (required), `kind`/`type`, `organisation`, `function`/
//! `fonction`/`role`, `email`, `phone`/`telephone`, `notes`, `groups`/`groupes`
//! (paths such as `Détaillants/Suisse romande`, several separated by `;`),
//! `anonymise`/`anonymiser` (yes/no/oui/non/true/false/1/0), `pseudonym`,
//! `aliases`/`alias` (separated by `;` or `|`), `interviews`/`entretiens` (ids
//! separated by `;` or `|`, each `ID` or `ID:role`, default role respondent),
//! `consent_status`, `consent_date`, `consent_document`, `person_id`, and
//! `attr:<id>` for a person attribute. An empty cell leaves a field as it is.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::person::{
    check_kind, check_role, default_pseudonym, group_id, person_id, Person, PersonGroup,
    PersonPatch,
};
use cs_model::sha256_hex;
use rusqlite::params;
use serde::Serialize;
use serde_json::json;

use crate::persons::{dedup, insert_person_row, update_person_row};
use crate::{append_event, now, validation, Store};

/// A problem found in the file; `row` 0 is the file itself, 1 the header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonsImportProblem {
    pub row: usize,
    pub message: String,
}

/// What one row does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonsImportRow {
    pub row: usize,
    /// `create`, `update` or `unchanged`.
    pub action: String,
    pub person_id: String,
    pub name: String,
}

/// The report of a dry run or of an applied import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonsImportReport {
    pub dry_run: bool,
    /// True when the file was written (no dry run, no problem).
    pub applied: bool,
    /// True when the file has no problem.
    pub ok: bool,
    pub n_rows: usize,
    pub n_create: usize,
    pub n_update: usize,
    pub n_unchanged: usize,
    pub n_links: usize,
    /// Paths of the groups the import creates.
    pub groups_to_create: Vec<String>,
    pub rows: Vec<PersonsImportRow>,
    pub problems: Vec<PersonsImportProblem>,
    pub warnings: Vec<String>,
    pub csv_sha256: String,
}

/// Splits CSV text into records (quotes, doubled quotes, line breaks inside
/// quotes). The delimiter is `,` or `;`.
fn parse_csv(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut any = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == delim => row.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            c => field.push(c),
        }
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn column_key(header: &str) -> String {
    let h = header.trim().trim_start_matches('\u{feff}').to_lowercase();
    if let Some(id) = h
        .strip_prefix("attr:")
        .or_else(|| h.strip_prefix("attribut:"))
    {
        return format!("attr:{}", id.trim());
    }
    match h.as_str() {
        "name" | "nom" => "name",
        "kind" | "type" => "kind",
        "organisation" | "organization" | "entreprise" => "organisation",
        "function" | "fonction" | "role" | "rôle" => "function",
        "email" | "e-mail" | "courriel" | "mail" => "email",
        "phone" | "telephone" | "téléphone" | "tel" | "tél" => "phone",
        "notes" | "note" | "remarques" => "notes",
        "groups" | "group" | "groupes" | "groupe" => "groups",
        "anonymise" | "anonymiser" | "anonymize" | "anonymiser?" => "anonymise",
        "pseudonym" | "pseudonyme" => "pseudonym",
        "aliases" | "alias" => "aliases",
        "interviews" | "interview" | "entretiens" | "entretien" => "interviews",
        "consent_status" | "consentement" => "consent_status",
        "consent_date" | "date_consentement" => "consent_date",
        "consent_document" | "document_consentement" => "consent_document",
        "person_id" | "id" => "person_id",
        other => return format!("?{other}"),
    }
    .to_string()
}

fn yes_no(raw: &str) -> Option<bool> {
    match raw.trim().to_lowercase().as_str() {
        "yes" | "y" | "oui" | "o" | "true" | "vrai" | "1" | "x" => Some(true),
        "no" | "n" | "non" | "false" | "faux" | "0" => Some(false),
        _ => None,
    }
}

fn split(raw: &str, seps: &[char]) -> Vec<String> {
    raw.split(|c| seps.contains(&c))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The counters and the new groups of a simulated run.
struct Sim {
    persons: Vec<Person>,
    groups: Vec<PersonGroup>,
    new_groups: Vec<String>,
    counters: BTreeMap<String, i64>,
    links: BTreeMap<(String, String), String>,
}

impl Sim {
    fn next(&mut self, name: &str) -> i64 {
        let v = self.counters.entry(name.to_string()).or_insert(0);
        *v += 1;
        *v
    }

    fn group_path(&mut self, path: &str) -> Result<String> {
        let mut parent: Option<String> = None;
        for part in path.split('/') {
            let name = part.trim();
            if name.is_empty() || name.chars().count() > 200 {
                return Err(validation(format!(
                    "the group path {path:?} has an empty or too long part"
                )));
            }
            let found = self
                .groups
                .iter()
                .find(|g| g.parent_id == parent && g.name.to_lowercase() == name.to_lowercase())
                .map(|g| g.group_id.clone());
            let id = match found {
                Some(id) => id,
                None => {
                    let id = group_id(self.next("group"));
                    let stamp = now();
                    self.groups.push(PersonGroup {
                        group_id: id.clone(),
                        name: name.to_string(),
                        parent_id: parent.clone(),
                        description: String::new(),
                        created_at: stamp.clone(),
                        updated_at: stamp,
                    });
                    self.new_groups.push(id.clone());
                    id
                }
            };
            parent = Some(id);
        }
        parent.ok_or_else(|| validation("the group path is empty"))
    }
}

impl Store {
    /// Reads a CSV of persons. With `dry_run` nothing is written; otherwise
    /// the file is applied in one transaction when it has no problem.
    pub fn import_persons(
        &self,
        csv: &str,
        dry_run: bool,
        actor: &str,
    ) -> Result<PersonsImportReport> {
        let mut report = PersonsImportReport {
            dry_run,
            applied: false,
            ok: false,
            n_rows: 0,
            n_create: 0,
            n_update: 0,
            n_unchanged: 0,
            n_links: 0,
            groups_to_create: Vec::new(),
            rows: Vec::new(),
            problems: Vec::new(),
            warnings: Vec::new(),
            csv_sha256: sha256_hex(csv.as_bytes()),
        };
        let first_line = csv.lines().next().unwrap_or("");
        let delim = if first_line.matches(';').count() > first_line.matches(',').count() {
            ';'
        } else {
            ','
        };
        let mut records = parse_csv(csv, delim);
        if records.is_empty() {
            report.problems.push(PersonsImportProblem {
                row: 0,
                message: "the file is empty".into(),
            });
            return Ok(report);
        }
        let header: Vec<String> = records.remove(0).iter().map(|h| column_key(h)).collect();
        for h in header.iter().filter(|h| h.starts_with('?')) {
            report
                .warnings
                .push(format!("column {:?} is ignored", &h[1..]));
        }
        let col = |key: &str| header.iter().position(|h| h == key);
        let Some(name_col) = col("name") else {
            report.problems.push(PersonsImportProblem {
                row: 1,
                message: "the header has no name column (name or nom)".into(),
            });
            return Ok(report);
        };
        let cfg = self.project_config()?;
        let manual = self.manual_pseudonym_config()?;
        let language = self.language();
        let interviews: BTreeSet<String> = {
            let mut stmt = self.conn.prepare("SELECT interview_id FROM interviews")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut sim = Sim {
            persons: self.all_persons()?,
            groups: self.person_groups()?,
            new_groups: Vec::new(),
            counters: self
                .conn
                .prepare("SELECT name, value FROM person_counters")?
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<_>>()?,
            links: self
                .person_links()?
                .into_iter()
                .map(|l| ((l.person_id, l.interview_id), l.role))
                .collect(),
        };
        let initial: BTreeMap<String, Person> = sim
            .persons
            .iter()
            .map(|p| (p.person_id.clone(), p.clone()))
            .collect();
        let initial_links = sim.links.clone();
        let mut seen_names: BTreeMap<String, usize> = BTreeMap::new();

        for (i, rec) in records.iter().enumerate() {
            let row_no = i + 2;
            if rec.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            report.n_rows += 1;
            let cell = |key: &str| -> String {
                col(key)
                    .and_then(|c| rec.get(c))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            };
            let name = rec
                .get(name_col)
                .map(|s| s.trim())
                .unwrap_or("")
                .to_string();
            let mut problem = |msg: String| {
                report.problems.push(PersonsImportProblem {
                    row: row_no,
                    message: msg,
                })
            };
            if name.is_empty() {
                problem("the name is empty".into());
                continue;
            }
            if let Some(first) = seen_names.insert(name.to_lowercase(), row_no) {
                problem(format!("{name:?} already appears at row {first}"));
                continue;
            }
            // Which record is it?
            let pid = cell("person_id");
            let existing = if !pid.is_empty() {
                match sim.persons.iter().position(|p| p.person_id == pid) {
                    Some(ix) => Some(ix),
                    None => {
                        problem(format!("unknown person_id {pid}"));
                        continue;
                    }
                }
            } else {
                sim.persons
                    .iter()
                    .position(|p| p.name.to_lowercase() == name.to_lowercase())
            };
            // Cells to a patch.
            let mut patch = PersonPatch {
                name: Some(name.clone()),
                ..Default::default()
            };
            let some = |s: String| if s.is_empty() { None } else { Some(s) };
            patch.organisation = some(cell("organisation"));
            patch.function = some(cell("function"));
            patch.email = some(cell("email"));
            patch.phone = some(cell("phone"));
            patch.notes = some(cell("notes"));
            patch.consent_status = some(cell("consent_status"));
            patch.consent_date = some(cell("consent_date"));
            patch.consent_document = some(cell("consent_document"));
            patch.pseudonym = some(cell("pseudonym"));
            let aliases = split(&cell("aliases"), &[';', '|']);
            if !aliases.is_empty() {
                patch.aliases = Some(aliases);
            }
            let an = cell("anonymise");
            if !an.is_empty() {
                match yes_no(&an) {
                    Some(b) => patch.anonymise = Some(b),
                    None => {
                        problem(format!("anonymise must be yes or no, not {an:?}"));
                        continue;
                    }
                }
            }
            let kind_cell = cell("kind");
            if !kind_cell.is_empty() {
                match check_kind(&kind_cell) {
                    Ok(k) => patch.kind = Some(k),
                    Err(e) => {
                        problem(e.to_string());
                        continue;
                    }
                }
            }
            let mut attrs: BTreeMap<String, Option<String>> = BTreeMap::new();
            for (ci, h) in header.iter().enumerate() {
                if let Some(id) = h.strip_prefix("attr:") {
                    let v = rec.get(ci).map(|s| s.trim()).unwrap_or("");
                    if !v.is_empty() {
                        attrs.insert(id.to_string(), Some(v.to_string()));
                    }
                }
            }
            if !attrs.is_empty() {
                patch.attributes = Some(attrs);
            }
            // Interviews and groups of the row.
            let mut row_links: Vec<(String, String)> = Vec::new();
            let mut bad = false;
            for tok in split(&cell("interviews"), &[';', '|']) {
                let (iv, role) = match tok.split_once(':') {
                    Some((iv, r)) => (iv.trim().to_string(), r.trim().to_string()),
                    None => (tok.clone(), "respondent".to_string()),
                };
                match check_role(&role) {
                    Ok(role) if interviews.contains(&iv) => row_links.push((iv, role)),
                    Ok(_) => {
                        problem(format!("unknown interview {iv}"));
                        bad = true;
                    }
                    Err(e) => {
                        problem(e.to_string());
                        bad = true;
                    }
                }
            }
            if bad {
                continue;
            }
            // Apply to a copy; keep it only when the register stays valid.
            let before_sim_persons = sim.persons.clone();
            let before_groups = sim.groups.clone();
            let before_new = sim.new_groups.clone();
            let before_counters = sim.counters.clone();
            let outcome = (|| -> Result<(Person, bool)> {
                let mut group_ids: Vec<String> = Vec::new();
                for path in split(&cell("groups"), &[';']) {
                    group_ids.push(sim.group_path(&path)?);
                }
                let (base, creating) = match existing {
                    Some(ix) => (sim.persons[ix].clone(), false),
                    None => {
                        let kind = patch.kind.clone().unwrap_or_else(|| "person".into());
                        let n = sim.next("person");
                        let stamp = now();
                        (
                            Person {
                                person_id: person_id(n),
                                kind,
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
                            },
                            true,
                        )
                    }
                };
                let mut p = self.check_patch(&base, &patch, &cfg, creating)?;
                if creating {
                    p.pseudonym = match &patch.pseudonym {
                        Some(s) => s.trim().to_string(),
                        None => loop {
                            let n = sim.next(&format!("pseudonym.{}", p.kind));
                            let c = default_pseudonym(&p.kind, &language, n);
                            if !sim
                                .persons
                                .iter()
                                .any(|o| o.pseudonym.to_lowercase() == c.to_lowercase())
                            {
                                break c;
                            }
                        },
                    };
                } else if let Some(s) = &patch.pseudonym {
                    p.pseudonym = s.trim().to_string();
                }
                p.group_ids = dedup(
                    &base
                        .group_ids
                        .iter()
                        .cloned()
                        .chain(group_ids)
                        .collect::<Vec<_>>(),
                );
                if p != base {
                    p.updated_at = now();
                }
                match existing {
                    Some(ix) => sim.persons[ix] = p.clone(),
                    None => sim.persons.push(p.clone()),
                }
                self.check_register(&sim.persons, &manual)?;
                Ok((p, creating))
            })();
            match outcome {
                Err(e) => {
                    sim.persons = before_sim_persons;
                    sim.groups = before_groups;
                    sim.new_groups = before_new;
                    sim.counters = before_counters;
                    report.problems.push(PersonsImportProblem {
                        row: row_no,
                        message: e.to_string(),
                    });
                }
                Ok((p, creating)) => {
                    let unchanged = !creating
                        && initial.get(&p.person_id) == Some(&p)
                        && row_links.iter().all(|(iv, r)| {
                            initial_links.get(&(p.person_id.clone(), iv.clone())) == Some(r)
                        });
                    for (iv, r) in &row_links {
                        sim.links
                            .insert((p.person_id.clone(), iv.clone()), r.clone());
                    }
                    let action = if creating {
                        report.n_create += 1;
                        "create"
                    } else if unchanged {
                        report.n_unchanged += 1;
                        "unchanged"
                    } else {
                        report.n_update += 1;
                        "update"
                    };
                    report.rows.push(PersonsImportRow {
                        row: row_no,
                        action: action.into(),
                        person_id: p.person_id.clone(),
                        name: p.name.clone(),
                    });
                }
            }
        }
        report.n_links = sim
            .links
            .iter()
            .filter(|(k, v)| initial_links.get(*k) != Some(*v))
            .count();
        report.groups_to_create = sim
            .new_groups
            .iter()
            .map(|g| crate::persons::group_path(&sim.groups, g))
            .collect();
        report.ok = report.problems.is_empty();
        if dry_run || !report.ok {
            return Ok(report);
        }

        // Apply in one transaction.
        let digest_before = self.pseudonym_config()?.digest();
        let tx = self.conn.unchecked_transaction()?;
        for gid in &sim.new_groups {
            if let Some(g) = sim.groups.iter().find(|g| g.group_id == *gid) {
                tx.execute(
                    "INSERT INTO person_groups (group_id, name, parent_id, description, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![g.group_id, g.name, g.parent_id, g.description, g.created_at, g.updated_at],
                )?;
            }
        }
        for p in &sim.persons {
            match initial.get(&p.person_id) {
                None => insert_person_row(&tx, p)?,
                Some(before) if before != p => update_person_row(&tx, p, before)?,
                Some(_) => {}
            }
        }
        for ((pid, iv), role) in &sim.links {
            if initial_links.get(&(pid.clone(), iv.clone())) != Some(role) {
                tx.execute(
                    "INSERT INTO person_interviews (person_id, interview_id, role, created_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(person_id, interview_id) DO UPDATE SET role = excluded.role",
                    params![pid, iv, role, now()],
                )?;
            }
        }
        for (name, value) in &sim.counters {
            tx.execute(
                "INSERT INTO person_counters (name, value) VALUES (?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET value = excluded.value",
                params![name, value],
            )?;
        }
        let digest_after = crate::persons::effective_from(&sim.persons, &manual)
            .config
            .digest();
        append_event(
            &tx,
            actor,
            "persons.import",
            None,
            None,
            json!({
                "csv_sha256": report.csv_sha256,
                "n_rows": report.n_rows,
                "n_create": report.n_create,
                "n_update": report.n_update,
                "n_unchanged": report.n_unchanged,
                "n_links": report.n_links,
                "n_groups_created": sim.new_groups.len(),
                "pseudonymisation_digest_before": digest_before,
                "pseudonymisation_digest_after": digest_after,
            }),
        )?;
        tx.commit()?;
        report.applied = true;
        Ok(report)
    }
}
