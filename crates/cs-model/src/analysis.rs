//! V1 lot 5: Analyse and Write. Closed lists and the pure parts (seeded draw)
//! of themes, matrix cells, the five parts of a theme, links and the final
//! audit. Storage lives in `cs-store`; this module has no I/O.
//!
//! Closed lists here are the ones SPEC 3.2 and the guide fix on purpose (theme
//! statuses, matrix cell values, the five parts, audit verdicts, passage
//! origins). The lists the project can edit (theme checks, evidence natures,
//! link types) are in [`crate::project::ProjectConfig`].

use serde::{Deserialize, Serialize};

use crate::sha256_hex;

/// Statuses of a theme, in the order of the work.
pub const THEME_STATUSES: [&str; 3] = ["candidate", "revised", "retained"];

/// French labels of [`THEME_STATUSES`].
pub fn theme_status_label(status: &str) -> &'static str {
    match status {
        "candidate" => "Candidat",
        "revised" => "Révisé",
        "retained" => "Retenu",
        _ => "",
    }
}

/// Values of a matrix cell. Not configurable, so that matrices of different
/// projects read the same way (SPEC 3.2).
pub const CELL_VALUES: [&str; 4] = ["present", "not_mentioned", "contradicted", "uncertain"];

/// French labels of [`CELL_VALUES`].
pub fn cell_value_label(value: &str) -> &'static str {
    match value {
        "present" => "présent",
        "not_mentioned" => "non évoqué",
        "contradicted" => "contredit",
        "uncertain" => "incertain",
        _ => "",
    }
}

/// The five written parts of a retained theme, in reading order: key and
/// French title.
pub const THEME_PARTS: [(&str, &str); 5] = [
    ("analytical_statement", "Énoncé analytique"),
    ("mechanism_evidence", "Mécanisme et preuves"),
    ("variations_divergent", "Variations et cas divergents"),
    ("competing_limits", "Explications concurrentes et limites"),
    ("mini_synthesis", "Mini-synthèse"),
];

/// Whether `key` is one of the five parts.
pub fn is_theme_part(key: &str) -> bool {
    THEME_PARTS.iter().any(|(k, _)| *k == key)
}

/// Title of a part, empty for an unknown key.
pub fn theme_part_title(key: &str) -> &'static str {
    THEME_PARTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, t)| *t)
        .unwrap_or("")
}

/// Verdicts of the final audit: supported, to nuance, to withdraw.
pub const AUDIT_VERDICTS: [&str; 3] = ["supported", "to_nuance", "to_withdraw"];

/// French labels of [`AUDIT_VERDICTS`].
pub fn audit_verdict_label(verdict: &str) -> &'static str {
    match verdict {
        "supported" => "étayée",
        "to_nuance" => "à nuancer",
        "to_withdraw" => "à retirer",
        _ => "",
    }
}

/// How a passage came to be said (guide, "Qualifier une citation"): a lived
/// experience, a project, an opinion, a document.
pub const PASSAGE_ORIGINS: [&str; 4] = ["lived_experience", "project", "opinion", "document"];

/// French labels of [`PASSAGE_ORIGINS`].
pub fn passage_origin_label(origin: &str) -> &'static str {
    match origin {
        "lived_experience" => "expérience vécue",
        "project" => "projet",
        "opinion" => "opinion",
        "document" => "document",
        _ => "",
    }
}

/// Role of a passage in a decision, per the guide: trigger, after-the-fact
/// justification, enabling condition, constraint, undetermined. The same list
/// as the roles of a claim.
pub const PASSAGE_ROLES: [&str; 5] = crate::CLAIM_ROLES;

/// French labels of [`PASSAGE_ROLES`].
pub fn passage_role_label(role: &str) -> &'static str {
    match role {
        "trigger" => "déclencheur",
        "retrospective-justification" => "justification après coup",
        "enabling-condition" => "condition facilitatrice",
        "constraint" => "contrainte",
        "undetermined" => "indéterminé",
        _ => "",
    }
}

/// Outcomes of a documented decision, per the guide: adopt, defer, reject.
pub const DECISION_OUTCOMES: [&str; 3] = ["adopt", "defer", "reject"];

/// French labels of [`DECISION_OUTCOMES`].
pub fn decision_outcome_label(outcome: &str) -> &'static str {
    match outcome {
        "adopt" => "adopter",
        "defer" => "différer",
        "reject" => "refuser",
        _ => "",
    }
}

/// Normalises a stored or typed outcome to one of [`DECISION_OUTCOMES`]. The
/// earlier free-text values (`adopted`, `deferred`, `rejected`) and the French
/// verbs are understood; anything else gives `None`.
pub fn normalise_outcome(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().as_str() {
        "adopt" | "adopted" | "adopter" | "adopté" | "adoptée" => Some("adopt"),
        "defer" | "deferred" | "différer" | "differer" | "différé" | "différée" => {
            Some("defer")
        }
        "reject" | "rejected" | "refuser" | "refusé" | "refusée" => Some("reject"),
        _ => None,
    }
}

/// Seed of an audit draw made without one: sixteen hex characters taken from
/// the SHA-256 of the time and of a salt. Recorded with the draw, so that it
/// can be replayed.
pub fn fresh_seed(time: &str, salt: &str) -> String {
    sha256_hex(format!("{time}|{salt}").as_bytes())[..16].to_string()
}

/// Deterministic draw of `n` keys among `keys`: ordered by SHA-256 of the seed
/// followed by the key, the first `n` kept. The same seed and the same set
/// always give the same draw, whatever the order `keys` is given in.
pub fn seeded_draw(seed: &str, keys: &[String], n: usize) -> Vec<String> {
    let mut keyed: Vec<(String, &String)> = keys
        .iter()
        .map(|k| (sha256_hex(format!("{seed}|{k}").as_bytes()), k))
        .collect();
    keyed.sort();
    keyed.dedup();
    keyed.into_iter().take(n).map(|(_, k)| k.clone()).collect()
}

/// A target of a typed link: a part of a theme, or the synthesis of a
/// research question.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkTarget {
    /// One of the five parts of a theme.
    ThemePart { theme_id: String, part: String },
    /// The synthesis of a research question.
    RqPart { rq_id: String },
}

impl LinkTarget {
    /// Stable key of the claim a target stands for in the final audit.
    pub fn claim_key(&self) -> String {
        match self {
            LinkTarget::ThemePart { theme_id, part } => format!("theme_part:{theme_id}:{part}"),
            LinkTarget::RqPart { rq_id } => format!("rq_part:{rq_id}"),
        }
    }

    /// Reads a key written by [`Self::claim_key`].
    pub fn from_claim_key(key: &str) -> Option<LinkTarget> {
        let mut it = key.splitn(3, ':');
        match (it.next()?, it.next()?, it.next()) {
            ("theme_part", theme_id, Some(part)) => Some(LinkTarget::ThemePart {
                theme_id: theme_id.into(),
                part: part.into(),
            }),
            ("rq_part", rq_id, None) => Some(LinkTarget::RqPart {
                rq_id: rq_id.into(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draw_is_reproducible_and_independent_of_input_order() {
        let keys: Vec<String> = (1..=9).map(|i| format!("k{i}")).collect();
        let mut rev = keys.clone();
        rev.reverse();
        let a = seeded_draw("seed-1", &keys, 3);
        assert_eq!(a.len(), 3);
        assert_eq!(a, seeded_draw("seed-1", &rev, 3));
        assert_ne!(a, seeded_draw("seed-2", &keys, 3));
        assert_eq!(seeded_draw("seed-1", &keys, 99).len(), 9);
    }

    #[test]
    fn claim_keys_round_trip() {
        let t = LinkTarget::ThemePart {
            theme_id: "T-0001".into(),
            part: "mechanism_evidence".into(),
        };
        assert_eq!(LinkTarget::from_claim_key(&t.claim_key()), Some(t));
        let r = LinkTarget::RqPart { rq_id: "Q1".into() };
        assert_eq!(LinkTarget::from_claim_key(&r.claim_key()), Some(r));
        assert_eq!(LinkTarget::from_claim_key("nonsense"), None);
    }

    #[test]
    fn outcomes_accept_the_earlier_values() {
        assert_eq!(normalise_outcome("adopted"), Some("adopt"));
        assert_eq!(normalise_outcome(" Différer "), Some("defer"));
        assert_eq!(normalise_outcome("rejected"), Some("reject"));
        assert_eq!(normalise_outcome("maybe"), None);
    }

    #[test]
    fn labels_have_no_long_dash() {
        for s in THEME_STATUSES {
            assert!(!theme_status_label(s).contains('\u{2014}'));
        }
        for (_, t) in THEME_PARTS {
            assert!(!t.contains('\u{2014}'));
        }
    }
}
