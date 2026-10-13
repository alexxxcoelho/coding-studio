//! A read-only index over the passages and the code tree, loaded once and
//! shared by the themes, the matrix, the writing and the final audit, so that
//! a view of many themes or a matrix of many cells does not reload the
//! corpus for each of them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::passage::Passage;

use crate::passages::PassageFilter;
use crate::Store;

pub(crate) struct Index {
    /// Every passage that is not deleted, highlights included, in interview,
    /// unit and position order.
    pub passages: Vec<Passage>,
    /// `code_id` to its parent's `code_id`.
    pub parent: BTreeMap<String, String>,
    /// `code_id` to the stable ids of the codes merged into it (their own
    /// passages already carry the target id, this is for display).
    pub name: BTreeMap<String, String>,
}

impl Index {
    pub fn load(store: &Store) -> Result<Index> {
        let passages = store.passages(&PassageFilter::default())?;
        let mut parent = BTreeMap::new();
        let mut name = BTreeMap::new();
        let mut stmt = store.conn.prepare(
            "SELECT code_id, parent_id, COALESCE(name, code) FROM codes WHERE code_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, p, n) = row?;
            if let Some(p) = p {
                parent.insert(id.clone(), p);
            }
            name.insert(id, n);
        }
        Ok(Index {
            passages,
            parent,
            name,
        })
    }

    /// The given codes and all their descendants.
    pub fn closure<I: IntoIterator<Item = String>>(&self, ids: I) -> BTreeSet<String> {
        let mut wanted: BTreeSet<String> = ids.into_iter().collect();
        loop {
            let before = wanted.len();
            for (child, parent) in &self.parent {
                if wanted.contains(parent) {
                    wanted.insert(child.clone());
                }
            }
            if wanted.len() == before {
                return wanted;
            }
        }
    }

    /// Whether a code has at least one child.
    pub fn is_focused(&self, code_id: &str) -> bool {
        self.parent.values().any(|p| p == code_id)
    }

    /// Positions in `passages` of the passages that carry one of the codes.
    /// The reserved code "Plus tard" is never among `codes`, so a parked
    /// passage is not found through it.
    pub fn with_codes(&self, codes: &BTreeSet<String>) -> BTreeSet<usize> {
        self.passages
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.codings
                    .iter()
                    .any(|c| !c.system && codes.contains(&c.code_id))
            })
            .map(|(i, _)| i)
            .collect()
    }
}
