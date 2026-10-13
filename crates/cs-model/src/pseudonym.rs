//! Pseudonymisation of what is sent to a model (V1 lot 4, F-12).
//!
//! The project keeps one editable list: a real name (person or organisation)
//! and the pseudonym that replaces it. [`Pseudonymiser::mask`] is applied to
//! every text that leaves the machine; [`Pseudonymiser::unmask`] reverses it
//! for display. The match ignores case and respects word boundaries: "Anna"
//! does not match inside "Annabelle". The list does not catch a form of a name
//! that it does not list, and it does not cover free text such as memos; the
//! interface says so ([`FREE_TEXT_NOTE`]).

use serde::{Deserialize, Serialize};

use crate::{sha256_hex, CsError};

/// Key of the `settings` table holding the list.
pub const PSEUDONYMISATION_KEY: &str = "pseudonymisation";

/// Kinds of names the list accepts.
pub const KINDS: [&str; 2] = ["person", "organisation"];

/// What the interface and the run records say about the limits of the method.
pub const FREE_TEXT_NOTE: &str = "La liste remplace les noms qu'elle contient, sous la forme exacte où ils sont listés, dans tout ce qui part vers un modèle. Elle ne couvre ni une autre forme d'un nom ni le texte libre (mémos, notes) : relisez le prompt avant de l'envoyer à un fournisseur hébergé.";

/// What a run records of the pseudonymisation applied to its prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PseudonymisationRecord {
    /// Whether the option was on when the prompt was built.
    pub enabled: bool,
    /// Digest of the list the prompt was built with.
    pub digest: String,
    /// Kinds of names that were masked (`person`, `organisation`).
    pub categories: Vec<String>,
    pub n_entries: usize,
    /// The list does not cover free text; the researcher reads the prompt.
    pub free_text_covered: bool,
    pub note: String,
}

/// One real name and its replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PseudonymEntry {
    /// `person` or `organisation`.
    pub kind: String,
    pub original: String,
    pub pseudonym: String,
}

/// The project list and its switch.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PseudonymConfig {
    /// When false nothing is replaced.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub entries: Vec<PseudonymEntry>,
}

fn invalid(m: String) -> CsError {
    CsError::Validation(m)
}

impl PseudonymConfig {
    /// Trims the entries and refuses an empty name, an unknown kind, a name
    /// listed twice, a pseudonym used twice, and a pseudonym that is itself a
    /// listed name (it would put a name back).
    pub fn normalised(&self) -> Result<PseudonymConfig, CsError> {
        let mut entries: Vec<PseudonymEntry> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            let n = i + 1;
            let original = e.original.trim().to_string();
            let pseudonym = e.pseudonym.trim().to_string();
            let kind = e.kind.trim().to_lowercase();
            if original.is_empty() {
                return Err(invalid(format!("entry {n}: the name is empty")));
            }
            if pseudonym.is_empty() {
                return Err(invalid(format!(
                    "entry {n}: the pseudonym of {original} is empty"
                )));
            }
            if !KINDS.contains(&kind.as_str()) {
                return Err(invalid(format!(
                    "entry {n}: kind must be person or organisation, not {:?}",
                    e.kind
                )));
            }
            if original.to_lowercase() == pseudonym.to_lowercase() {
                return Err(invalid(format!(
                    "entry {n}: the pseudonym of {original} is the name itself"
                )));
            }
            entries.push(PseudonymEntry {
                kind,
                original,
                pseudonym,
            });
        }
        for (i, a) in entries.iter().enumerate() {
            for b in &entries[i + 1..] {
                if a.original.to_lowercase() == b.original.to_lowercase() {
                    return Err(invalid(format!("{} is listed twice", a.original)));
                }
                if a.pseudonym.to_lowercase() == b.pseudonym.to_lowercase() {
                    return Err(invalid(format!(
                        "the pseudonym {} is used for two names",
                        a.pseudonym
                    )));
                }
            }
        }
        let cfg = PseudonymConfig {
            enabled: self.enabled,
            entries,
        };
        let rules = Pseudonymiser::new(&cfg.entries);
        for e in &cfg.entries {
            if rules.contains_original(&e.pseudonym) {
                return Err(invalid(format!(
                    "the pseudonym {} contains a listed name",
                    e.pseudonym
                )));
            }
        }
        Ok(cfg)
    }

    /// SHA-256 of the list as applied: empty text when the option is off, so
    /// that a disabled list and an empty list have the same digest. Entries
    /// are sorted, so the order of the list does not change it.
    pub fn digest(&self) -> String {
        let mut rows: Vec<(String, String, String)> = if self.enabled {
            self.entries
                .iter()
                .map(|e| (e.kind.clone(), e.original.clone(), e.pseudonym.clone()))
                .collect()
        } else {
            Vec::new()
        };
        rows.sort();
        sha256_hex(serde_json::to_string(&rows).unwrap_or_default().as_bytes())
    }

    /// The masking rules in force (none when the option is off).
    pub fn pseudonymiser(&self) -> Pseudonymiser {
        if self.enabled {
            Pseudonymiser::new(&self.entries)
        } else {
            Pseudonymiser::new(&[])
        }
    }

    /// Kinds present in the list in force, sorted.
    pub fn categories(&self) -> Vec<String> {
        let mut k: Vec<String> = if self.enabled {
            self.entries.iter().map(|e| e.kind.clone()).collect()
        } else {
            Vec::new()
        };
        k.sort();
        k.dedup();
        k
    }
}

/// Replacement rules, longest name first so that "Banque Alpha" wins over "Alpha".
#[derive(Debug, Clone, Default)]
pub struct Pseudonymiser {
    rules: Vec<(Vec<char>, String, String)>,
    reverse: Vec<(Vec<char>, String)>,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn same(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Position where `needle` matches `hay` at `i`, ignoring case, on word boundaries.
fn matches_at(hay: &[char], i: usize, needle: &[char]) -> bool {
    if needle.is_empty() || i + needle.len() > hay.len() {
        return false;
    }
    if !hay[i..i + needle.len()]
        .iter()
        .zip(needle)
        .all(|(a, b)| same(*a, *b))
    {
        return false;
    }
    let before_ok = i == 0 || !is_word(hay[i - 1]) || !is_word(needle[0]);
    let end = i + needle.len();
    let after_ok = end == hay.len() || !is_word(hay[end]) || !is_word(needle[needle.len() - 1]);
    before_ok && after_ok
}

impl Pseudonymiser {
    pub fn new(entries: &[PseudonymEntry]) -> Pseudonymiser {
        let mut rules: Vec<(Vec<char>, String, String)> = entries
            .iter()
            .filter(|e| !e.original.is_empty())
            .map(|e| {
                (
                    e.original.chars().collect(),
                    e.original.clone(),
                    e.pseudonym.clone(),
                )
            })
            .collect();
        rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.1.cmp(&b.1)));
        let mut reverse: Vec<(Vec<char>, String)> = entries
            .iter()
            .filter(|e| !e.pseudonym.is_empty())
            .map(|e| (e.pseudonym.chars().collect(), e.original.clone()))
            .collect();
        // Stable: several names may share a pseudonym (a person and their
        // aliases); the one listed first is the name given back.
        reverse.sort_by_key(|b| std::cmp::Reverse(b.0.len()));
        Pseudonymiser { rules, reverse }
    }

    /// True when no rule is in force.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Replaces every listed name by its pseudonym.
    pub fn mask(&self, text: &str) -> String {
        self.mask_counted(text, &mut Vec::new())
    }

    /// Like [`Self::mask`], adding the replacements made to `counts` (indexed
    /// by the position of the original in `originals()`).
    pub fn mask_counted(&self, text: &str, counts: &mut Vec<usize>) -> String {
        if self.rules.is_empty() {
            return text.to_string();
        }
        if counts.len() < self.rules.len() {
            counts.resize(self.rules.len(), 0);
        }
        let hay: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        'outer: while i < hay.len() {
            for (k, (needle, _, pseudo)) in self.rules.iter().enumerate() {
                if matches_at(&hay, i, needle) {
                    out.push_str(pseudo);
                    counts[k] += 1;
                    i += needle.len();
                    continue 'outer;
                }
            }
            out.push(hay[i]);
            i += 1;
        }
        out
    }

    /// The listed names, in the order `mask_counted` counts them.
    pub fn originals(&self) -> Vec<&str> {
        self.rules.iter().map(|r| r.1.as_str()).collect()
    }

    /// True when a listed name still appears in `text`.
    pub fn contains_original(&self, text: &str) -> bool {
        if self.rules.is_empty() {
            return false;
        }
        let hay: Vec<char> = text.chars().collect();
        (0..hay.len()).any(|i| self.rules.iter().any(|(n, _, _)| matches_at(&hay, i, n)))
    }

    /// Replaces every pseudonym by its name (display only; the match is exact
    /// in case, so that "P1" is not read inside other words).
    pub fn unmask(&self, text: &str) -> String {
        if self.reverse.is_empty() {
            return text.to_string();
        }
        let hay: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        'outer: while i < hay.len() {
            for (needle, original) in &self.reverse {
                if i + needle.len() <= hay.len()
                    && hay[i..i + needle.len()] == needle[..]
                    && matches_at(&hay, i, needle)
                {
                    out.push_str(original);
                    i += needle.len();
                    continue 'outer;
                }
            }
            out.push(hay[i]);
            i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, original: &str, pseudonym: &str) -> PseudonymEntry {
        PseudonymEntry {
            kind: kind.into(),
            original: original.into(),
            pseudonym: pseudonym.into(),
        }
    }

    #[test]
    fn masks_whole_words_ignoring_case_and_longest_first() {
        let p = Pseudonymiser::new(&[
            entry("person", "Anna", "P1"),
            entry("organisation", "Banque Alpha", "ORG1"),
            entry("organisation", "Alpha", "ORG2"),
        ]);
        assert_eq!(
            p.mask("anna a vu Annabelle chez la Banque Alpha, pas chez Alpha."),
            "P1 a vu Annabelle chez la ORG1, pas chez ORG2."
        );
        assert_eq!(p.mask("l'Anna, Anna-Lou"), "l'P1, P1-Lou");
        assert!(p.contains_original("chez ALPHA"));
        assert!(!p.contains_original("Annabelle"));
    }

    #[test]
    fn unmask_reverses_a_masked_text() {
        let p = Pseudonymiser::new(&[
            entry("person", "Anna Muller", "P1"),
            entry("organisation", "Banque Alpha", "ORG1"),
        ]);
        let masked = p.mask("Anna Muller travaille à la Banque Alpha.");
        assert_eq!(masked, "P1 travaille à la ORG1.");
        assert_eq!(
            p.unmask(&masked),
            "Anna Muller travaille à la Banque Alpha."
        );
        assert_eq!(p.unmask("P12"), "P12");
    }

    #[test]
    fn counts_the_replacements() {
        let p = Pseudonymiser::new(&[entry("person", "Anna", "P1"), entry("person", "Luc", "P2")]);
        let mut counts = Vec::new();
        p.mask_counted("Anna et Anna et Luc", &mut counts);
        let by: Vec<(&str, usize)> = p.originals().into_iter().zip(counts).collect();
        assert!(by.contains(&("Anna", 2)) && by.contains(&("Luc", 1)));
    }

    #[test]
    fn the_list_is_validated_and_its_digest_ignores_order_and_switch() {
        let ok = PseudonymConfig {
            enabled: true,
            entries: vec![
                entry("Person", " Anna ", "P1"),
                entry("organisation", "Alpha", "ORG1"),
            ],
        };
        let n = ok.normalised().unwrap();
        assert_eq!(n.entries[0].original, "Anna");
        assert_eq!(n.entries[0].kind, "person");
        let mut reversed = n.clone();
        reversed.entries.reverse();
        assert_eq!(n.digest(), reversed.digest());
        let off = PseudonymConfig {
            enabled: false,
            ..n.clone()
        };
        assert_eq!(off.digest(), PseudonymConfig::default().digest());
        assert!(off.pseudonymiser().is_empty());
        for bad in [
            vec![entry("person", "", "P1")],
            vec![entry("person", "Anna", "")],
            vec![entry("animal", "Anna", "P1")],
            vec![entry("person", "Anna", "anna")],
            vec![entry("person", "Anna", "P1"), entry("person", "anna", "P2")],
            vec![entry("person", "Anna", "P1"), entry("person", "Luc", "p1")],
            vec![entry("person", "Anna", "Luc"), entry("person", "Luc", "P2")],
        ] {
            assert!(PseudonymConfig {
                enabled: true,
                entries: bad
            }
            .normalised()
            .is_err());
        }
    }
}
