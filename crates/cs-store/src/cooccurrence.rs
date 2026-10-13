//! V2, F-56: code co-occurrence. Which codes appear together, counted in
//! three ways, optionally per interview or per attribute value, with a
//! drill-down to the passages that make a cell.
//!
//! Relations (a co-occurrence is a description of the coding, never a claim
//! about cause):
//! - `same_passage`: both codes are on the same passage;
//! - `overlap`: the code of one passage and the code of another passage that
//!   shares at least one character with it, in the same turn (the same
//!   passage counts);
//! - `same_unit`: both codes are on passages of the same turn, wherever they
//!   lie.
//!
//! A cell counts distinct turns, distinct interviews and distinct passage
//! pairs, apart. The diagonal gives the passages, turns and interviews of a
//! code alone, and `jaccard` compares the turns of the pair with the turns of
//! either code. With `rollup`, a passage also counts for the ancestors of its
//! codes; an ancestor and its own descendant are never paired.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{not_found, validation, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    SamePassage,
    #[default]
    Overlap,
    SameUnit,
}

impl Relation {
    fn holds(self, p: &P, q: &P) -> bool {
        match self {
            Relation::SamePassage => p.passage_id == q.passage_id,
            Relation::Overlap => {
                p.passage_id == q.passage_id || (p.start < q.end && q.start < p.end)
            }
            Relation::SameUnit => true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoRequest {
    #[serde(default)]
    pub relation: Relation,
    /// `interview` or `attribute` (with `attribute`); none for one group.
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub attribute: Option<String>,
    /// Codes to cross (id, name, alias or token); all coded codes when empty.
    #[serde(default)]
    pub codes: Vec<String>,
    /// Count a passage for the ancestors of its codes too.
    #[serde(default)]
    pub rollup: bool,
    /// Leave out the cells with fewer turns.
    #[serde(default = "one")]
    pub min_units: usize,
    /// Restrict to these interviews.
    #[serde(default)]
    pub interviews: Vec<String>,
}

fn one() -> usize {
    1
}

impl Default for CoRequest {
    fn default() -> Self {
        CoRequest {
            relation: Relation::Overlap,
            group_by: None,
            attribute: None,
            codes: vec![],
            rollup: false,
            min_units: 1,
            interviews: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoCode {
    pub code_id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoCell {
    /// The code that comes first in the codebook order.
    pub a: String,
    pub b: String,
    pub units: usize,
    pub interviews: usize,
    pub passage_pairs: usize,
    /// Turns with both codes over turns with either code.
    pub jaccard: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoDiagonal {
    pub code_id: String,
    pub passages: usize,
    pub units: usize,
    pub interviews: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoGroup {
    pub key: String,
    pub label: String,
    /// Interviews in the group that have a coded passage.
    pub interviews: usize,
    pub cells: Vec<CoCell>,
    pub diagonal: Vec<CoDiagonal>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoMatrix {
    pub relation: Relation,
    pub group_by: Option<String>,
    pub attribute: Option<String>,
    pub rollup: bool,
    pub codes: Vec<CoCode>,
    pub groups: Vec<CoGroup>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoPassage {
    pub passage_id: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoOverlap {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoPair {
    /// The passage that carries the first code asked for.
    pub a: CoPassage,
    pub b: CoPassage,
    /// The characters the two passages share, none when they do not touch.
    pub overlap: Option<CoOverlap>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoItem {
    pub interview_id: String,
    pub unit_id: String,
    pub seq: u32,
    pub speaker: String,
    pub text: String,
    pub pairs: Vec<CoPair>,
}

/// A coded passage as the computation sees it.
#[derive(Clone)]
struct P {
    passage_id: String,
    interview_id: String,
    unit_id: String,
    start: usize,
    end: usize,
    text: String,
    /// Code ids, ancestors included under `rollup`.
    codes: BTreeSet<String>,
}

const NOTE: &str = "Co-occurrence describes how the coding falls together in the transcripts; \
it is not causation and not prevalence. Turns, interviews and passage pairs are counted apart.";

/// Is `anc` an ancestor of `node` in the code tree?
fn is_ancestor(parent: &BTreeMap<&str, Option<&str>>, anc: &str, node: &str) -> bool {
    let mut cur = parent.get(node).copied().flatten();
    let mut guard = 0;
    while let Some(p) = cur {
        if p == anc {
            return true;
        }
        guard += 1;
        if guard > 64 {
            return false;
        }
        cur = parent.get(p).copied().flatten();
    }
    false
}

impl Store {
    /// Coded passages (active non-reserved codes) with their codes expanded.
    fn coded_passages(
        &self,
        rollup: bool,
        only: &BTreeSet<String>,
    ) -> Result<(Vec<P>, Vec<cs_model::passage::CodeNode>)> {
        let nodes = self.raw_nodes()?;
        let parent: BTreeMap<&str, Option<&str>> = nodes
            .iter()
            .map(|n| (n.code_id.as_str(), n.parent_id.as_deref()))
            .collect();
        let mut out = Vec::new();
        for p in self.passages(&Default::default())? {
            if !only.is_empty() && !only.contains(&p.interview_id) {
                continue;
            }
            let mut codes: BTreeSet<String> = p
                .codings
                .iter()
                .filter(|c| !c.system)
                .map(|c| c.code_id.clone())
                .collect();
            if codes.is_empty() {
                continue;
            }
            if rollup {
                let own: Vec<String> = codes.iter().cloned().collect();
                for c in own {
                    let mut cur = parent.get(c.as_str()).copied().flatten();
                    let mut guard = 0;
                    while let Some(id) = cur {
                        guard += 1;
                        if guard > 64 {
                            break;
                        }
                        codes.insert(id.to_string());
                        cur = parent.get(id).copied().flatten();
                    }
                }
            }
            out.push(P {
                passage_id: p.passage_id,
                interview_id: p.interview_id,
                unit_id: p.unit_id,
                start: p.start,
                end: p.end,
                text: p.text,
                codes,
            });
        }
        Ok((out, nodes))
    }

    fn resolve_code_list(&self, keys: &[String]) -> Result<Vec<String>> {
        keys.iter()
            .map(|k| Ok(self.resolve_code(k)?.code_id))
            .collect()
    }

    /// The co-occurrence matrix. See the module documentation.
    pub fn cooccurrence(&self, req: &CoRequest) -> Result<CoMatrix> {
        for id in &req.interviews {
            self.require_interview(id)?;
        }
        let only: BTreeSet<String> = req.interviews.iter().cloned().collect();
        let selected = self.resolve_code_list(&req.codes)?;
        // Groups of interviews.
        enum Grouping {
            One,
            Interview,
            Attribute(String),
        }
        let grouping = match (req.group_by.as_deref(), req.attribute.as_deref()) {
            (None, _) | (Some(""), _) => Grouping::One,
            (Some("interview"), _) => Grouping::Interview,
            (Some("attribute"), Some(a)) => {
                if self.project_config()?.attribute(a).is_none() {
                    return Err(validation(format!("unknown attribute {a}")));
                }
                Grouping::Attribute(a.to_string())
            }
            (Some("attribute"), None) => {
                return Err(validation("group_by=attribute needs an attribute"))
            }
            (Some(other), _) => {
                return Err(validation(format!(
                    "group_by is interview or attribute, not {other}"
                )))
            }
        };
        let (passages, nodes) = self.coded_passages(req.rollup, &only)?;
        let order: BTreeMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.code_id.as_str(), i))
            .collect();
        let parent: BTreeMap<&str, Option<&str>> = nodes
            .iter()
            .map(|n| (n.code_id.as_str(), n.parent_id.as_deref()))
            .collect();
        // Codes of the matrix.
        let used: BTreeSet<&str> = passages
            .iter()
            .flat_map(|p| p.codes.iter().map(String::as_str))
            .collect();
        let mut codes: Vec<&cs_model::passage::CodeNode> = nodes
            .iter()
            .filter(|n| {
                if selected.is_empty() {
                    used.contains(n.code_id.as_str())
                } else {
                    selected.contains(&n.code_id)
                }
            })
            .collect();
        codes.sort_by_key(|n| order[n.code_id.as_str()]);
        let keep: BTreeSet<&str> = codes.iter().map(|n| n.code_id.as_str()).collect();

        // Group the passages.
        let values = self.all_attribute_values()?;
        let mut groups: Vec<(String, String, Vec<&P>)> = Vec::new();
        match &grouping {
            Grouping::One => groups.push(("all".into(), "all".into(), passages.iter().collect())),
            Grouping::Interview => {
                let mut by: BTreeMap<&str, Vec<&P>> = BTreeMap::new();
                for p in &passages {
                    by.entry(p.interview_id.as_str()).or_default().push(p);
                }
                for (k, v) in by {
                    groups.push((k.into(), k.into(), v));
                }
            }
            Grouping::Attribute(a) => {
                let def = self
                    .project_config()?
                    .attribute(a)
                    .cloned()
                    .ok_or_else(|| not_found(format!("attribute {a}")))?;
                let mut by: BTreeMap<String, Vec<&P>> = BTreeMap::new();
                for p in &passages {
                    let v = values
                        .get(&p.interview_id)
                        .and_then(|m| m.get(a))
                        .cloned()
                        .unwrap_or_default();
                    by.entry(v).or_default().push(p);
                }
                // Values in the order of the choices, others alphabetically,
                // interviews without a value last.
                let mut keys: Vec<String> = by.keys().cloned().collect();
                keys.sort_by_key(|k| {
                    (
                        k.is_empty(),
                        def.choices
                            .iter()
                            .position(|c| c == k)
                            .unwrap_or(usize::MAX),
                        k.clone(),
                    )
                });
                for k in keys {
                    let label = if k.is_empty() {
                        "(no value)".into()
                    } else {
                        k.clone()
                    };
                    let v = by.remove(&k).unwrap_or_default();
                    groups.push((k, label, v));
                }
            }
        }

        let is_structural = |x: &str, y: &str| -> bool {
            req.rollup && (is_ancestor(&parent, x, y) || is_ancestor(&parent, y, x))
        };

        let mut out_groups = Vec::new();
        for (key, label, ps) in groups {
            // Diagonal.
            #[allow(clippy::type_complexity)]
            let mut diag: BTreeMap<
                &str,
                (BTreeSet<&str>, BTreeSet<(&str, &str)>, BTreeSet<&str>),
            > = BTreeMap::new();
            for p in &ps {
                for c in p.codes.iter().filter(|c| keep.contains(c.as_str())) {
                    let e = diag.entry(c.as_str()).or_default();
                    e.0.insert(p.passage_id.as_str());
                    e.1.insert((p.interview_id.as_str(), p.unit_id.as_str()));
                    e.2.insert(p.interview_id.as_str());
                }
            }
            // Pairs, per turn.
            let mut by_unit: BTreeMap<(&str, &str), Vec<&P>> = BTreeMap::new();
            for p in &ps {
                by_unit
                    .entry((p.interview_id.as_str(), p.unit_id.as_str()))
                    .or_default()
                    .push(p);
            }
            type Key<'a> = (&'a str, &'a str);
            #[allow(clippy::type_complexity)]
            let mut cells: BTreeMap<
                Key,
                (
                    BTreeSet<(&str, &str)>,
                    BTreeSet<&str>,
                    BTreeSet<(&str, &str)>,
                ),
            > = BTreeMap::new();
            for (unit, list) in &by_unit {
                for p in list {
                    for q in list {
                        if !req.relation.holds(p, q) {
                            continue;
                        }
                        for a in p.codes.iter().filter(|c| keep.contains(c.as_str())) {
                            for b in q.codes.iter().filter(|c| keep.contains(c.as_str())) {
                                if order[a.as_str()] >= order[b.as_str()] || is_structural(a, b) {
                                    continue;
                                }
                                let e = cells.entry((a.as_str(), b.as_str())).or_default();
                                e.0.insert(*unit);
                                e.1.insert(unit.0);
                                e.2.insert((p.passage_id.as_str(), q.passage_id.as_str()));
                            }
                        }
                    }
                }
            }
            let mut cell_list: Vec<CoCell> = cells
                .into_iter()
                .filter(|(_, (u, ..))| u.len() >= req.min_units.max(1))
                .map(|((a, b), (u, i, pairs))| {
                    let ua = diag.get(a).map_or(0, |d| d.1.len());
                    let ub = diag.get(b).map_or(0, |d| d.1.len());
                    let either = ua + ub - u.len().min(ua + ub);
                    CoCell {
                        a: a.into(),
                        b: b.into(),
                        units: u.len(),
                        interviews: i.len(),
                        passage_pairs: pairs.len(),
                        jaccard: if either == 0 {
                            0.0
                        } else {
                            u.len() as f64 / either as f64
                        },
                    }
                })
                .collect();
            cell_list.sort_by_key(|c| (order[c.a.as_str()], order[c.b.as_str()]));
            let diagonal = codes
                .iter()
                .filter_map(|n| {
                    diag.get(n.code_id.as_str()).map(|(p, u, i)| CoDiagonal {
                        code_id: n.code_id.clone(),
                        passages: p.len(),
                        units: u.len(),
                        interviews: i.len(),
                    })
                })
                .collect();
            let interviews = ps
                .iter()
                .map(|p| p.interview_id.as_str())
                .collect::<BTreeSet<_>>()
                .len();
            out_groups.push(CoGroup {
                key,
                label,
                interviews,
                cells: cell_list,
                diagonal,
            });
        }
        if out_groups.is_empty() {
            out_groups.push(CoGroup {
                key: "all".into(),
                label: "all".into(),
                interviews: 0,
                cells: vec![],
                diagonal: vec![],
            });
        }
        Ok(CoMatrix {
            relation: req.relation,
            group_by: req.group_by.clone().filter(|g| !g.is_empty()),
            attribute: req.attribute.clone(),
            rollup: req.rollup,
            codes: codes
                .iter()
                .map(|n| CoCode {
                    code_id: n.code_id.clone(),
                    name: n.name.clone(),
                    active: n.active && n.merged_into.is_none(),
                })
                .collect(),
            groups: out_groups,
            note: NOTE.into(),
        })
    }

    /// The passages that make the cell `(a, b)`: for each turn, the pairs of
    /// passages (the first carries `a`, the second `b`) that stand in the
    /// relation. `req.group_by` is ignored; `req.interviews` restricts.
    pub fn cooccurrence_passages(&self, a: &str, b: &str, req: &CoRequest) -> Result<Vec<CoItem>> {
        let a = self.resolve_code(a)?;
        let b = self.resolve_code(b)?;
        if a.system || b.system {
            return Err(validation("the reserved code is counted in no table"));
        }
        if a.code_id == b.code_id {
            return Err(validation("give two different codes"));
        }
        for id in &req.interviews {
            self.require_interview(id)?;
        }
        let only: BTreeSet<String> = req.interviews.iter().cloned().collect();
        let (passages, nodes) = self.coded_passages(req.rollup, &only)?;
        if req.rollup {
            let parent: BTreeMap<&str, Option<&str>> = nodes
                .iter()
                .map(|n| (n.code_id.as_str(), n.parent_id.as_deref()))
                .collect();
            if is_ancestor(&parent, &a.code_id, &b.code_id)
                || is_ancestor(&parent, &b.code_id, &a.code_id)
            {
                return Ok(vec![]);
            }
        }
        let mut by_unit: BTreeMap<(String, String), Vec<&P>> = BTreeMap::new();
        for p in &passages {
            by_unit
                .entry((p.interview_id.clone(), p.unit_id.clone()))
                .or_default()
                .push(p);
        }
        let brief = |p: &P| CoPassage {
            passage_id: p.passage_id.clone(),
            start: p.start,
            end: p.end,
            text: p.text.clone(),
        };
        let mut items = Vec::new();
        for ((iv, unit_id), list) in by_unit {
            let mut pairs = Vec::new();
            for p in &list {
                if !p.codes.contains(&a.code_id) {
                    continue;
                }
                for q in &list {
                    if !q.codes.contains(&b.code_id) || !req.relation.holds(p, q) {
                        continue;
                    }
                    let (s, e) = (p.start.max(q.start), p.end.min(q.end));
                    pairs.push(CoPair {
                        a: brief(p),
                        b: brief(q),
                        overlap: (s < e).then_some(CoOverlap { start: s, end: e }),
                    });
                }
            }
            if pairs.is_empty() {
                continue;
            }
            pairs.sort_by(|x, y| {
                (
                    x.a.start,
                    x.a.end,
                    &x.a.passage_id,
                    x.b.start,
                    &x.b.passage_id,
                )
                    .cmp(&(
                        y.a.start,
                        y.a.end,
                        &y.a.passage_id,
                        y.b.start,
                        &y.b.passage_id,
                    ))
            });
            let unit = self.current_unit(&iv, &unit_id)?;
            items.push(CoItem {
                interview_id: iv,
                unit_id,
                seq: unit.seq,
                speaker: unit.speaker_raw,
                text: unit.text,
                pairs,
            });
        }
        items.sort_by(|x, y| (&x.interview_id, x.seq).cmp(&(&y.interview_id, y.seq)));
        Ok(items)
    }
}
