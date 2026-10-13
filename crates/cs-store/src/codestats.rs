//! V2, F-45: counts that never mislead. For each code: passages, distinct
//! interviews, distinct cases (through the units attributed to a case) and
//! distinct values of each interview attribute, on the code alone and rolled
//! up over its descendants (a passage counted once).
//!
//! Passages, interviews and cases are three different units and are never
//! added. An interview with no value for an attribute counts in no value; it
//! is reported apart as `interviews_without_value`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::passage::{
    AttributeStat, CodeCounts, CodeNode, CodeStats, StatBlock, StatFilter, ValueCount, LATER_ID,
};
use rusqlite::Connection;

use crate::{validation, Store};

/// A passage as the counts need it.
pub(crate) type PassageRef = (String, String, String); // passage, interview, unit

/// Cases and attribute values, loaded once for a whole computation.
pub(crate) struct StatCtx {
    /// (interview, unit) to the cases that attribute the unit.
    case_of: BTreeMap<(String, String), BTreeSet<String>>,
    /// Attribute ids and labels, in configuration order.
    attributes: Vec<(String, String)>,
    /// interview to attribute to value.
    values: BTreeMap<String, BTreeMap<String, String>>,
}

impl StatCtx {
    pub(crate) fn load(store: &Store) -> Result<StatCtx> {
        let mut case_of: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        let mut stmt = store
            .conn
            .prepare("SELECT case_id, interview_id, unit_id FROM case_units")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (case, interview, unit) = row?;
            case_of.entry((interview, unit)).or_default().insert(case);
        }
        let attributes = store
            .project_config()?
            .attributes
            .into_iter()
            .map(|a| (a.id, a.label))
            .collect();
        Ok(StatCtx {
            case_of,
            attributes,
            values: store.all_attribute_values()?,
        })
    }

    fn cases(&self, refs: &BTreeSet<&PassageRef>) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (_, interview, unit) in refs {
            if let Some(c) = self.case_of.get(&(interview.clone(), unit.clone())) {
                out.extend(c.iter().cloned());
            }
        }
        out
    }

    /// The full block for a set of passages.
    pub(crate) fn block(&self, refs: &BTreeSet<&PassageRef>) -> StatBlock {
        let interviews: BTreeSet<&String> = refs.iter().map(|(_, i, _)| i).collect();
        let cases = self.cases(refs);
        let attributes = self
            .attributes
            .iter()
            .map(|(id, label)| {
                let mut counts: BTreeMap<&String, usize> = BTreeMap::new();
                let mut without = 0;
                for iv in &interviews {
                    match self.values.get(*iv).and_then(|m| m.get(id)) {
                        Some(v) => *counts.entry(v).or_default() += 1,
                        None => without += 1,
                    }
                }
                AttributeStat {
                    attribute_id: id.clone(),
                    label: label.clone(),
                    distinct_values: counts.len(),
                    values: counts
                        .into_iter()
                        .map(|(v, n)| ValueCount {
                            value: v.clone(),
                            interviews: n,
                        })
                        .collect(),
                    interviews_without_value: without,
                }
            })
            .collect();
        StatBlock {
            passages: refs.len(),
            interviews: interviews.len(),
            cases: cases.len(),
            interview_ids: interviews.into_iter().cloned().collect(),
            case_ids: cases.into_iter().collect(),
            attributes,
        }
    }

    /// Only the figures of [`CodeCounts`] for one scope.
    fn figures(
        &self,
        refs: &BTreeSet<&PassageRef>,
    ) -> (usize, usize, usize, BTreeMap<String, usize>) {
        let b = self.block(refs);
        (
            b.passages,
            b.interviews,
            b.cases,
            b.attributes
                .into_iter()
                .map(|a| (a.attribute_id, a.distinct_values))
                .collect(),
        )
    }
}

/// Active passage references of every code, by stable code id. Deleted
/// passages and removed codings do not count; the reserved code is left out.
pub(crate) fn own_refs(
    conn: &Connection,
    interviews: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, BTreeSet<PassageRef>>> {
    let mut stmt = conn.prepare(
        "SELECT pc.code_id, p.passage_id, p.interview_id, p.unit_id
         FROM passage_codings pc JOIN passages p USING (passage_id)
         WHERE pc.removed_at IS NULL AND p.deleted_at IS NULL AND pc.code_id <> ?1",
    )?;
    let mut own: BTreeMap<String, BTreeSet<PassageRef>> = BTreeMap::new();
    for row in stmt.query_map([LATER_ID], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (code, passage, interview, unit) = row?;
        if interviews.is_some_and(|set| !set.contains(&interview)) {
            continue;
        }
        own.entry(code)
            .or_default()
            .insert((passage, interview, unit));
    }
    Ok(own)
}

/// Ids of a code and of all its descendants.
fn subtree<'a>(root: &'a str, children: &BTreeMap<&'a str, Vec<&'a str>>) -> Vec<&'a str> {
    let mut stack = vec![root];
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        out.push(id);
        if let Some(kids) = children.get(id) {
            stack.extend(kids.iter().copied());
        }
    }
    out
}

fn children_of(nodes: &[CodeNode]) -> BTreeMap<&str, Vec<&str>> {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in nodes {
        if let Some(p) = &n.parent_id {
            children.entry(p.as_str()).or_default().push(&n.code_id);
        }
    }
    children
}

impl Store {
    /// Counts of every code (own and rolled up), for `/api/codes`.
    pub(crate) fn code_counts_v2(
        &self,
        nodes: &[CodeNode],
        interviews: Option<&BTreeSet<String>>,
    ) -> Result<BTreeMap<String, CodeCounts>> {
        let own = own_refs(&self.conn, interviews)?;
        let ctx = StatCtx::load(self)?;
        let children = children_of(nodes);
        let empty = BTreeSet::new();
        let mut out = BTreeMap::new();
        for n in nodes {
            let direct: BTreeSet<&PassageRef> =
                own.get(&n.code_id).unwrap_or(&empty).iter().collect();
            let mut all: BTreeSet<&PassageRef> = BTreeSet::new();
            for id in subtree(&n.code_id, &children) {
                if let Some(set) = own.get(id) {
                    all.extend(set.iter());
                }
            }
            let (passages, interviews, cases, attribute_values) = ctx.figures(&direct);
            let (passages_rollup, interviews_rollup, cases_rollup, attribute_values_rollup) =
                ctx.figures(&all);
            out.insert(
                n.code_id.clone(),
                CodeCounts {
                    passages,
                    interviews,
                    passages_rollup,
                    interviews_rollup,
                    cases,
                    cases_rollup,
                    attribute_values,
                    attribute_values_rollup,
                },
            );
        }
        Ok(out)
    }

    /// Detailed statistics of one code (`key` is an id, name, alias or
    /// token). With `filter`, only the interviews whose attribute has that
    /// value are counted.
    pub fn code_stats(&self, key: &str, filter: Option<(&str, &str)>) -> Result<CodeStats> {
        let only = match filter {
            Some((a, v)) => Some(self.interviews_with_attribute(a, v)?),
            None => None,
        };
        let nodes = self.raw_nodes()?;
        let row = self.resolve_code(key)?;
        if row.system {
            return Err(validation("the reserved code is counted in no table"));
        }
        let node = nodes
            .iter()
            .find(|n| n.code_id == row.code_id)
            .ok_or_else(|| crate::not_found(format!("code {key}")))?;
        let own = own_refs(&self.conn, only.as_ref())?;
        let ctx = StatCtx::load(self)?;
        let children = children_of(&nodes);
        let empty = BTreeSet::new();
        let direct: BTreeSet<&PassageRef> =
            own.get(&node.code_id).unwrap_or(&empty).iter().collect();
        let mut all: BTreeSet<&PassageRef> = BTreeSet::new();
        for id in subtree(&node.code_id, &children) {
            if let Some(set) = own.get(id) {
                all.extend(set.iter());
            }
        }
        Ok(CodeStats {
            code_id: node.code_id.clone(),
            name: node.name.clone(),
            own: ctx.block(&direct),
            rollup: ctx.block(&all),
            filter: filter.map(|(a, v)| StatFilter {
                attribute: a.into(),
                value: v.into(),
            }),
            note: "Passages, interviews and cases are counted apart and never added. \
                   An interview without a value for an attribute is counted in no value. \
                   Counts describe coverage of the coded corpus, not prevalence."
                .into(),
        })
    }
}
