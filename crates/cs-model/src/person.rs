//! Persons, organisations and groups (the people interviewed).
//!
//! A [`Person`] is a record of the register of the project: a person or an
//! organisation (`kind`), with an `anonymise` flag, a stable pseudonym and
//! aliases. The flagged records feed the project pseudonymisation list
//! ([`derive_entries`]); the unflagged ones are never replaced. Persons are
//! organised in a tree of [`PersonGroup`]s and linked to interviews with a
//! role ([`INTERVIEW_ROLES`]).
//!
//! Decision on organisations: an organisation is a record of the same kind of
//! entity (`kind = "organisation"`), with its own anonymise flag, pseudonym
//! and aliases, so that groups, links to interviews, import and exports work
//! for both without a second code path. The `organisation` field of a person
//! is plain text.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::project::valid_date;
use crate::pseudonym::{PseudonymEntry, Pseudonymiser};
use crate::CsError;

/// Kinds of record.
pub const PERSON_KINDS: [&str; 2] = ["person", "organisation"];

/// Roles of a person in an interview.
pub const INTERVIEW_ROLES: [&str; 4] = ["respondent", "co_respondent", "mentioned", "interviewer"];

/// Roles whose attributes may go to the model as the description of the
/// respondent side of an interview.
pub const RESPONDENT_ROLES: [&str; 2] = ["respondent", "co_respondent"];

/// Consent states (empty: not recorded).
pub const CONSENT_STATUSES: [&str; 4] = ["pending", "given", "refused", "withdrawn"];

/// Shortest alias accepted: shorter ones would match inside ordinary words.
pub const MIN_ALIAS_CHARS: usize = 2;

/// The stable id of the n-th person, `P-0001`.
pub fn person_id(n: i64) -> String {
    format!("P-{n:04}")
}

/// The stable id of the n-th group, `G-0001`.
pub fn group_id(n: i64) -> String {
    format!("G-{n:04}")
}

/// A person or an organisation of the register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub person_id: String,
    /// `person` or `organisation`.
    pub kind: String,
    pub name: String,
    /// Organisation of a person, as text.
    #[serde(default)]
    pub organisation: String,
    /// Role or function in the organisation.
    #[serde(default)]
    pub function: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub notes: String,
    /// Empty, or one of [`CONSENT_STATUSES`].
    #[serde(default)]
    pub consent_status: String,
    /// `YYYY-MM-DD` or empty.
    #[serde(default)]
    pub consent_date: String,
    /// Path of the consent document, as the researcher keeps it.
    #[serde(default)]
    pub consent_document: String,
    /// Whether the name is replaced by the pseudonym (default true).
    pub anonymise: bool,
    pub pseudonym: String,
    /// Other spellings of the name, initials, first name alone.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Person-scoped attribute id to value.
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub group_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Fields of a person to create or change. A missing field is left as it is
/// (on creation: the default). `name` is required on creation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonPatch {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub organisation: Option<String>,
    #[serde(default)]
    pub function: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub consent_status: Option<String>,
    #[serde(default)]
    pub consent_date: Option<String>,
    #[serde(default)]
    pub consent_document: Option<String>,
    #[serde(default)]
    pub anonymise: Option<bool>,
    /// Empty text: back to the generated pseudonym.
    #[serde(default)]
    pub pseudonym: Option<String>,
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    /// Attribute id to value; `null` or an empty text clears.
    #[serde(default)]
    pub attributes: Option<BTreeMap<String, Option<String>>>,
    /// The whole list of groups of the person (replaces it).
    #[serde(default)]
    pub group_ids: Option<Vec<String>>,
}

/// A group of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonGroup {
    pub group_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A group with its roll-up counts and its children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupNode {
    #[serde(flatten)]
    pub group: PersonGroup,
    /// Slash-separated path of names from the root.
    pub path: String,
    pub depth: usize,
    /// Members of this group itself.
    pub n_persons: usize,
    /// Distinct members of this group and of every group below it.
    pub n_persons_total: usize,
    /// Distinct interviews linked to those members.
    pub n_interviews_total: usize,
    pub children: Vec<GroupNode>,
}

/// A link between a person and an interview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonInterviewLink {
    pub person_id: String,
    pub interview_id: String,
    pub role: String,
}

fn invalid<T>(m: impl Into<String>) -> Result<T, CsError> {
    Err(CsError::Validation(m.into()))
}

/// Checks a kind (lower-cased).
pub fn check_kind(kind: &str) -> Result<String, CsError> {
    let k = kind.trim().to_lowercase();
    if PERSON_KINDS.contains(&k.as_str()) {
        Ok(k)
    } else {
        invalid(format!(
            "kind must be person or organisation, not {kind:?}"
        ))
    }
}

/// Checks the role of a link (lower-cased).
pub fn check_role(role: &str) -> Result<String, CsError> {
    let r = role.trim().to_lowercase();
    if INTERVIEW_ROLES.contains(&r.as_str()) {
        Ok(r)
    } else {
        invalid(format!(
            "role must be one of {INTERVIEW_ROLES:?}, not {role:?}"
        ))
    }
}

/// Checks a consent status; empty is allowed.
pub fn check_consent(status: &str) -> Result<String, CsError> {
    let s = status.trim().to_lowercase();
    if s.is_empty() || CONSENT_STATUSES.contains(&s.as_str()) {
        Ok(s)
    } else {
        invalid(format!(
            "consent_status must be empty or one of {CONSENT_STATUSES:?}, not {status:?}"
        ))
    }
}

/// Checks a consent date; empty is allowed.
pub fn check_consent_date(date: &str) -> Result<String, CsError> {
    let d = date.trim();
    if d.is_empty() || valid_date(d) {
        Ok(d.to_string())
    } else {
        invalid(format!("consent_date must be YYYY-MM-DD, not {date:?}"))
    }
}

/// Trims the aliases, drops empty ones and duplicates (case ignored), refuses
/// one that is too short, and refuses an alias equal to the name itself.
pub fn normalise_aliases(name: &str, aliases: &[String]) -> Result<Vec<String>, CsError> {
    let mut out: Vec<String> = Vec::new();
    for a in aliases {
        let a = a.trim().to_string();
        if a.is_empty() {
            continue;
        }
        if a.chars().count() < MIN_ALIAS_CHARS {
            return invalid(format!(
                "alias {a:?} is shorter than {MIN_ALIAS_CHARS} characters: it would match inside ordinary words"
            ));
        }
        if a.to_lowercase() == name.trim().to_lowercase()
            || out.iter().any(|o| o.to_lowercase() == a.to_lowercase())
        {
            continue;
        }
        out.push(a);
    }
    Ok(out)
}

/// The generated pseudonym of the n-th record of a kind, in a language
/// (`fr`, `en`, `de`): "Répondant 03", "Participant 03", "Teilnehmer 03";
/// organisations: "Organisation 03", "Organisation 03", "Organisation 03".
pub fn default_pseudonym(kind: &str, language: &str, n: i64) -> String {
    let lang = language.trim().to_lowercase();
    let word = if kind == "organisation" {
        "Organisation"
    } else if lang.starts_with("en") {
        "Participant"
    } else if lang.starts_with("de") {
        "Teilnehmer"
    } else {
        "Répondant"
    };
    format!("{word} {n:02}")
}

/// The pseudonymisation entries that the flagged records contribute: the name
/// and each alias map to the pseudonym of the record. Unflagged records
/// contribute nothing. The name comes first, so that [`Pseudonymiser::unmask`]
/// turns a pseudonym back into the name and not into an alias.
pub fn derive_entries(persons: &[Person]) -> Vec<PseudonymEntry> {
    let mut out = Vec::new();
    for p in persons.iter().filter(|p| p.anonymise) {
        for original in std::iter::once(&p.name).chain(p.aliases.iter()) {
            out.push(PseudonymEntry {
                kind: p.kind.clone(),
                original: original.clone(),
                pseudonym: p.pseudonym.clone(),
            });
        }
    }
    out
}

/// Problems of a set of entries that comes from several sources: a name used
/// for two pseudonyms or by two records, a pseudonym that is itself a name or
/// that contains one (it would put a name back). `owner` gives, for each
/// entry, the label used in the message.
pub fn check_entries(entries: &[(PseudonymEntry, String)]) -> Result<(), CsError> {
    for (i, (a, owner_a)) in entries.iter().enumerate() {
        for (b, owner_b) in &entries[i + 1..] {
            if a.original.to_lowercase() == b.original.to_lowercase() {
                return invalid(format!(
                    "the name {:?} is used by {owner_a} and by {owner_b}",
                    a.original
                ));
            }
            if a.pseudonym.to_lowercase() == b.pseudonym.to_lowercase() && owner_a != owner_b {
                return invalid(format!(
                    "the pseudonym {:?} is used by {owner_a} and by {owner_b}",
                    a.pseudonym
                ));
            }
        }
    }
    let rules = Pseudonymiser::new(
        &entries
            .iter()
            .map(|(e, _)| e.clone())
            .collect::<Vec<PseudonymEntry>>(),
    );
    for (e, owner) in entries {
        if rules.contains_original(&e.pseudonym) {
            return invalid(format!(
                "the pseudonym {:?} of {owner} contains a listed name or alias",
                e.pseudonym
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str, name: &str, anonymise: bool, pseudo: &str, aliases: &[&str]) -> Person {
        Person {
            person_id: id.into(),
            kind: "person".into(),
            name: name.into(),
            organisation: String::new(),
            function: String::new(),
            email: String::new(),
            phone: String::new(),
            notes: String::new(),
            consent_status: String::new(),
            consent_date: String::new(),
            consent_document: String::new(),
            anonymise,
            pseudonym: pseudo.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            attributes: BTreeMap::new(),
            group_ids: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn ids_and_pseudonyms_are_stable_and_follow_the_language() {
        assert_eq!(person_id(3), "P-0003");
        assert_eq!(group_id(12), "G-0012");
        assert_eq!(default_pseudonym("person", "fr", 3), "Répondant 03");
        assert_eq!(default_pseudonym("person", "en", 3), "Participant 03");
        assert_eq!(default_pseudonym("person", "de-CH", 12), "Teilnehmer 12");
        assert_eq!(default_pseudonym("organisation", "en", 1), "Organisation 01");
    }

    #[test]
    fn only_flagged_records_feed_the_list_and_the_name_comes_first() {
        let ps = vec![
            person("P-0001", "Anna Muller", true, "Répondant 01", &["Anna", "A. M."]),
            person("P-0002", "Luc Favre", false, "Répondant 02", &["Luc"]),
        ];
        let e = derive_entries(&ps);
        let originals: Vec<&str> = e.iter().map(|x| x.original.as_str()).collect();
        assert_eq!(originals, vec!["Anna Muller", "Anna", "A. M."]);
        assert!(e.iter().all(|x| x.pseudonym == "Répondant 01"));
        let p = Pseudonymiser::new(&e);
        assert_eq!(p.mask("Anna Muller et Luc Favre"), "Répondant 01 et Luc Favre");
        assert_eq!(p.unmask("Répondant 01"), "Anna Muller");
    }

    #[test]
    fn aliases_are_cleaned_and_short_ones_refused() {
        let a = normalise_aliases(
            "Anna Muller",
            &[" Anna ".into(), "anna".into(), "".into(), "anna muller".into()],
        )
        .unwrap();
        assert_eq!(a, vec!["Anna"]);
        assert!(normalise_aliases("Anna", &["A".into()]).is_err());
    }

    #[test]
    fn clashes_between_records_are_refused() {
        let mk = |o: &str, p: &str, owner: &str| {
            (
                PseudonymEntry {
                    kind: "person".into(),
                    original: o.into(),
                    pseudonym: p.into(),
                },
                owner.to_string(),
            )
        };
        assert!(check_entries(&[mk("Anna", "R1", "P-1"), mk("anna", "R2", "P-2")]).is_err());
        assert!(check_entries(&[mk("Anna", "R1", "P-1"), mk("Luc", "r1", "P-2")]).is_err());
        assert!(check_entries(&[mk("Anna", "R1", "P-1"), mk("Anna Muller", "R1", "P-1")]).is_ok());
        assert!(check_entries(&[mk("Anna", "R1", "P-1"), mk("Luc", "Luc 2", "P-2")]).is_err());
        assert!(check_entries(&[mk("Anna", "R1", "P-1"), mk("Luc", "R2", "P-2")]).is_ok());
    }

    #[test]
    fn checks_on_the_small_fields() {
        assert_eq!(check_kind(" Organisation ").unwrap(), "organisation");
        assert!(check_kind("animal").is_err());
        assert!(check_role("boss").is_err());
        assert_eq!(check_role("Co_Respondent").unwrap(), "co_respondent");
        assert_eq!(check_consent("").unwrap(), "");
        assert!(check_consent("maybe").is_err());
        assert!(check_consent_date("2026-02-30").is_err());
        assert_eq!(check_consent_date("2026-02-28").unwrap(), "2026-02-28");
    }
}
