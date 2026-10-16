//! V2, F-41: code search for the picker. A query is split in words; every
//! word must be the beginning of a word of the name, the legacy
//! token, the label or the definition of the code (prefix match, accents and
//! case ignored). Results are ranked: a name beats the
//! definition; a whole-query prefix of the name beats a word prefix.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::textfold::{fold_text, folded_words};
use crate::Store;

/// Default and maximum number of results.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodeSearchOptions {
    /// Also list codes that are inactive or merged away.
    #[serde(default)]
    pub include_inactive: bool,
    /// Also list the reserved code "Plus tard".
    #[serde(default)]
    pub include_system: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Where the query matched, best field first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeMatch {
    /// `name`, `alias`, `token`, `label` or `definition`.
    pub field: String,
    /// The text of the field that matched (the alias itself for an alias).
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CodeHit {
    pub code_id: String,
    pub name: String,
    pub token: String,
    pub definition: String,
    pub parent_id: Option<String>,
    /// Names of the ancestors, root first.
    pub path: Vec<String>,
    pub active: bool,
    pub system: bool,
    pub passages: usize,
    pub interviews: usize,
    pub score: u32,
    pub matched: CodeMatch,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CodeSearch {
    pub query: String,
    /// Number of codes that match, before the limit.
    pub total: usize,
    pub items: Vec<CodeHit>,
}

/// Weight of a field, and of the way the word matched in it.
fn weight(field: &str, whole_query_prefix: bool) -> u32 {
    match (field, whole_query_prefix) {
        ("name", true) => 100,
        ("name", false) => 80,
        ("alias", true) => 90,
        ("alias", false) => 70,
        ("token", _) => 60,
        ("label", _) => 55,
        _ => 40,
    }
}

/// Does every query word begin a word of `field_words`? Returns true when so.
fn covers(query: &[String], field_words: &[String]) -> bool {
    query
        .iter()
        .all(|q| field_words.iter().any(|w| w.starts_with(q.as_str())))
}

impl Store {
    /// Codes matching `q` for the picker. An empty query lists the active
    /// codes in codebook order.
    pub fn search_codes(&self, q: &str, opts: &CodeSearchOptions) -> Result<CodeSearch> {
        let query = folded_words(q);
        let limit = opts.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let nodes = self.code_nodes()?;
        let by_id: std::collections::BTreeMap<&str, &cs_model::passage::CodeNode> =
            nodes.iter().map(|n| (n.code_id.as_str(), n)).collect();
        let path_of = |n: &cs_model::passage::CodeNode| -> Vec<String> {
            let mut out = Vec::new();
            let mut cur = n.parent_id.as_deref();
            let mut guard = 0;
            while let Some(id) = cur {
                guard += 1;
                match by_id.get(id) {
                    Some(p) if guard < 64 => {
                        out.push(p.name.clone());
                        cur = p.parent_id.as_deref();
                    }
                    _ => break,
                }
            }
            out.reverse();
            out
        };
        let mut hits: Vec<(u32, usize, CodeHit)> = Vec::new();
        for (order, n) in nodes.iter().enumerate() {
            if n.system && !opts.include_system {
                continue;
            }
            let usable = n.active && n.merged_into.is_none();
            if !usable && !opts.include_inactive {
                continue;
            }
            let best: Option<(u32, CodeMatch)> = if query.is_empty() {
                Some((
                    0,
                    CodeMatch {
                        field: "name".into(),
                        text: n.name.clone(),
                    },
                ))
            } else {
                let mut best: Option<(u32, CodeMatch)> = None;
                let consider = |field: &str, text: &str, best: &mut Option<(u32, CodeMatch)>| {
                    let words = folded_words(text);
                    if !covers(&query, &words) {
                        return;
                    }
                    // Whole query is the beginning of the field text.
                    let joined = folded_words(text).join(" ");
                    let whole = joined.starts_with(&query.join(" "));
                    let w = weight(field, whole);
                    if best.as_ref().is_none_or(|(b, _)| w > *b) {
                        *best = Some((
                            w,
                            CodeMatch {
                                field: field.into(),
                                text: text.into(),
                            },
                        ));
                    }
                };
                consider("name", &n.name, &mut best);
                consider("token", &n.token, &mut best);
                consider("label", &n.label, &mut best);
                consider("definition", &n.definition, &mut best);
                // A query spread over several fields (a name word and a
                // definition word) still finds the code, at the lowest weight.
                if best.is_none() {
                    let mut all = String::new();
                    for part in [
                        n.name.as_str(),
                        n.token.as_str(),
                        n.label.as_str(),
                        n.definition.as_str(),
                    ] {
                        all.push(' ');
                        all.push_str(part);
                    }
                    if covers(&query, &folded_words(&all)) {
                        best = Some((
                            30,
                            CodeMatch {
                                field: "definition".into(),
                                text: n.definition.clone(),
                            },
                        ));
                    }
                }
                best
            };
            let Some((score, matched)) = best else {
                continue;
            };
            hits.push((
                score,
                order,
                CodeHit {
                    code_id: n.code_id.clone(),
                    name: n.name.clone(),
                    token: n.token.clone(),
                    definition: n.definition.clone(),
                    parent_id: n.parent_id.clone(),
                    path: path_of(n),
                    active: usable,
                    system: n.system,
                    passages: n.counts.passages,
                    interviews: n.counts.interviews,
                    score,
                    matched,
                },
            ));
        }
        // Best score first; equal scores by folded name, then codebook order.
        hits.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| {
                    if query.is_empty() {
                        std::cmp::Ordering::Equal
                    } else {
                        fold_key(&a.2.name).cmp(&fold_key(&b.2.name))
                    }
                })
                .then(a.1.cmp(&b.1))
        });
        let total = hits.len();
        Ok(CodeSearch {
            query: q.trim().to_string(),
            total,
            items: hits.into_iter().take(limit).map(|h| h.2).collect(),
        })
    }
}

fn fold_key(s: &str) -> String {
    fold_text(s).chars.into_iter().collect()
}
