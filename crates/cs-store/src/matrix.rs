//! V1 lot 5: the analysis matrix.
//!
//! A matrix is a view, never a stored truth. Rows and columns are chosen among
//! interviews, values of an interview attribute, themes, codes and the custom
//! dimensions of the project. The passage count of a cell is computed from the
//! codings at each request. What the researcher decides about a cell (one of
//! four closed values, a one-line summary, passages linked by hand) is stored
//! under the stable keys of its row and column, so it survives a change of
//! configuration, a rename or a reorder.
//!
//! Keys: `interview:INT-01`, `attr:sector=Public`, `theme:T-0001`,
//! `code:C-0003`, `dim:who_pays`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::analysis::{cell_value_label, CELL_VALUES};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::analysis_index::Index;
use crate::{append_event, from_json, not_found, now, to_json, validation, Store};

const MAX_SUMMARY: usize = 300;

/// What a row or a column is made of.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixAxis {
    /// `interviews`, `cases`, `attribute`, `themes`, `codes`, `focused_codes` or `dimensions`.
    pub kind: String,
    /// Ids to keep, in this order. Empty: all of them (all interviews, all
    /// values of the attribute, all themes in document order, all active
    /// codes, all project dimensions).
    #[serde(default)]
    pub ids: Vec<String>,
    /// For `attribute`: the attribute id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// For `themes`: keep only this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Keep the interviews whose attribute has one of the values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixFilter {
    pub attribute: String,
    pub values: Vec<String>,
}

/// A matrix configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixConfig {
    pub rows: Vec<MatrixAxis>,
    pub cols: Vec<MatrixAxis>,
    #[serde(default)]
    pub filters: Vec<MatrixFilter>,
}

/// One row or one column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixEntry {
    pub key: String,
    pub kind: String,
    pub id: String,
    pub label: String,
    /// False for an interview with no coded passage yet: its cells read
    /// "not coded yet". True for every other entry.
    pub coded: bool,
}

/// One cell: the computed count and what the researcher decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixCell {
    pub row_key: String,
    pub col_key: String,
    /// Passages in the cell (computed from the codings; for a custom
    /// dimension, the passages linked by hand).
    pub passages: usize,
    /// Distinct interviews of those passages.
    pub interviews: usize,
    pub passage_ids: Vec<String>,
    pub value: Option<String>,
    pub value_label: Option<String>,
    pub summary: String,
    /// True when the passages are those linked by hand (a custom dimension).
    pub manual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Matrix {
    pub config: MatrixConfig,
    pub rows: Vec<MatrixEntry>,
    pub cols: Vec<MatrixEntry>,
    /// Row-major: `cells[i][j]` is `rows[i]` by `cols[j]`.
    pub cells: Vec<Vec<MatrixCell>>,
    /// Interviews kept by the filters (all when there are none).
    pub interviews_in_scope: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixViewRecord {
    pub view_id: String,
    pub name: String,
    pub config: MatrixConfig,
    pub created_at: String,
    pub updated_at: String,
}

/// What can be chosen for rows and columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixOptions {
    pub interviews: Vec<String>,
    pub attributes: Vec<MatrixAttributeOption>,
    pub themes: Vec<MatrixThemeOption>,
    pub codes: Vec<MatrixCodeOption>,
    pub dimensions: Vec<MatrixEntry>,
    pub cell_values: Vec<MatrixValueOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixAttributeOption {
    pub id: String,
    pub label: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixThemeOption {
    pub theme_id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixCodeOption {
    pub code_id: String,
    pub name: String,
    /// Has sub-codes: a "focused" code that gathers others.
    pub focused: bool,
    pub passages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixValueOption {
    pub value: String,
    pub label: String,
}

/// Edit of what the researcher said about a cell. Omitted fields unchanged;
/// an explicit null clears the value.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CellPatch {
    pub row_key: String,
    pub col_key: String,
    #[serde(default, deserialize_with = "crate::codebook::present")]
    pub value: Option<Option<String>>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Which stored side of a key is meant.
enum Key {
    Interview(String),
    AttrValue(String),
    Theme(String),
    Code(String),
    Dim(String),
    Case(String),
}

fn parse_key(key: &str) -> Result<Key> {
    let bad = || validation(format!("unknown matrix key {key}"));
    let (kind, rest) = key.split_once(':').ok_or_else(bad)?;
    match kind {
        "interview" if !rest.is_empty() => Ok(Key::Interview(rest.into())),
        "theme" if !rest.is_empty() => Ok(Key::Theme(rest.into())),
        "code" if !rest.is_empty() => Ok(Key::Code(rest.into())),
        "dim" if !rest.is_empty() => Ok(Key::Dim(rest.into())),
        "case" if !rest.is_empty() => Ok(Key::Case(rest.into())),
        "attr" => {
            let (a, v) = rest.split_once('=').ok_or_else(bad)?;
            if a.is_empty() || v.is_empty() {
                return Err(bad());
            }
            Ok(Key::AttrValue(a.into()))
        }
        _ => Err(bad()),
    }
}

/// The passages (positions in the index) behind an entry, or `None` for a
/// custom dimension, which has no code to compute from.
struct Resolved {
    entry: MatrixEntry,
    set: Option<BTreeSet<usize>>,
}

impl Store {
    fn attribute_values_of(&self, attribute: &str) -> Result<Vec<String>> {
        let cfg = self.project_config()?;
        let def = cfg
            .attribute(attribute)
            .ok_or_else(|| validation(format!("unknown attribute {attribute}")))?;
        let mut values: Vec<String> = def.choices.clone();
        let mut seen: BTreeSet<String> = values.iter().cloned().collect();
        for (_, m) in self.all_attribute_values()? {
            if let Some(v) = m.get(attribute) {
                if seen.insert(v.clone()) {
                    values.push(v.clone());
                }
            }
        }
        if def.choices.is_empty() {
            values.sort();
        }
        Ok(values)
    }

    /// Interviews kept by the filters.
    fn matrix_scope(&self, filters: &[MatrixFilter]) -> Result<Vec<String>> {
        let mut scope: Vec<String> = self
            .summaries()?
            .into_iter()
            .map(|s| s.interview_id)
            .collect();
        for f in filters {
            let mut keep: BTreeSet<String> = BTreeSet::new();
            for v in &f.values {
                keep.extend(self.interviews_with_attribute(&f.attribute, v)?);
            }
            scope.retain(|i| keep.contains(i));
        }
        Ok(scope)
    }

    fn resolve_axes(
        &self,
        axes: &[MatrixAxis],
        idx: &Index,
        scope: &[String],
    ) -> Result<Vec<Resolved>> {
        let cfg = self.project_config()?;
        let mut out: Vec<Resolved> = Vec::new();
        let by_interview = |ids: &BTreeSet<String>| -> BTreeSet<usize> {
            idx.passages
                .iter()
                .enumerate()
                .filter(|(_, p)| ids.contains(&p.interview_id))
                .map(|(i, _)| i)
                .collect()
        };
        for axis in axes {
            match axis.kind.as_str() {
                "interviews" => {
                    let all = self.summaries()?;
                    let known: BTreeSet<&str> =
                        all.iter().map(|s| s.interview_id.as_str()).collect();
                    for id in &axis.ids {
                        if !known.contains(id.as_str()) {
                            return Err(validation(format!("unknown interview {id}")));
                        }
                    }
                    let ids: Vec<String> = if axis.ids.is_empty() {
                        scope.to_vec()
                    } else {
                        axis.ids
                            .iter()
                            .filter(|i| scope.contains(i))
                            .cloned()
                            .collect()
                    };
                    for id in ids {
                        let set = by_interview(&BTreeSet::from([id.clone()]));
                        let coded = set
                            .iter()
                            .any(|i| idx.passages[*i].codings.iter().any(|c| !c.system));
                        out.push(Resolved {
                            set: Some(set),
                            entry: MatrixEntry {
                                key: format!("interview:{id}"),
                                kind: "interview".into(),
                                label: id.clone(),
                                id,
                                coded,
                            },
                        });
                    }
                }
                "attribute" => {
                    let attr = axis
                        .attribute
                        .as_deref()
                        .ok_or_else(|| validation("an attribute axis needs `attribute`"))?;
                    let def_label = cfg
                        .attribute(attr)
                        .map(|d| d.label.clone())
                        .ok_or_else(|| validation(format!("unknown attribute {attr}")))?;
                    let values = if axis.ids.is_empty() {
                        self.attribute_values_of(attr)?
                    } else {
                        axis.ids.clone()
                    };
                    for v in values {
                        let of: BTreeSet<String> = self
                            .interviews_with_attribute(attr, &v)?
                            .into_iter()
                            .filter(|i| scope.contains(i))
                            .collect();
                        out.push(Resolved {
                            set: Some(by_interview(&of)),
                            entry: MatrixEntry {
                                key: format!("attr:{attr}={v}"),
                                kind: "attribute_value".into(),
                                id: v.clone(),
                                coded: true,
                                label: format!("{def_label} : {v}"),
                            },
                        });
                    }
                }
                "themes" => {
                    let all = self.themes(axis.status.as_deref())?;
                    let chosen: Vec<&crate::ThemeView> = if axis.ids.is_empty() {
                        all.iter().collect()
                    } else {
                        axis.ids
                            .iter()
                            .map(|id| {
                                all.iter()
                                    .find(|t| t.theme_id == *id)
                                    .ok_or_else(|| validation(format!("unknown theme {id}")))
                            })
                            .collect::<Result<_>>()?
                    };
                    for t in chosen {
                        let set = self.theme_passage_positions(idx, &t.theme_id)?;
                        out.push(Resolved {
                            set: Some(restrict(idx, set, scope)),
                            entry: MatrixEntry {
                                key: format!("theme:{}", t.theme_id),
                                kind: "theme".into(),
                                id: t.theme_id.clone(),
                                coded: true,
                                label: t.title.clone(),
                            },
                        });
                    }
                }
                "codes" | "focused_codes" => {
                    let focused_only = axis.kind == "focused_codes";
                    let nodes: Vec<_> = self
                        .code_nodes()?
                        .into_iter()
                        .filter(|n| !n.system && n.active && n.merged_into.is_none())
                        .collect();
                    // Focused codes gather sub-codes; a codebook without any sub-code
                    // has no gathering level, so all its codes are offered.
                    let any_focused = nodes.iter().any(|n| idx.is_focused(&n.code_id));
                    let chosen = if axis.ids.is_empty() {
                        nodes
                            .iter()
                            .filter(|n| {
                                !focused_only || !any_focused || idx.is_focused(&n.code_id)
                            })
                            .collect::<Vec<_>>()
                    } else {
                        axis.ids
                            .iter()
                            .map(|id| {
                                let r = self.resolve_code(id)?;
                                nodes
                                    .iter()
                                    .find(|n| n.code_id == r.code_id)
                                    .ok_or_else(|| validation(format!("unknown code {id}")))
                            })
                            .collect::<Result<Vec<_>>>()?
                    };
                    for n in chosen {
                        let set = idx.with_codes(&idx.closure([n.code_id.clone()]));
                        out.push(Resolved {
                            set: Some(restrict(idx, set, scope)),
                            entry: MatrixEntry {
                                key: format!("code:{}", n.code_id),
                                kind: "code".into(),
                                id: n.code_id.clone(),
                                coded: true,
                                label: n.name.clone(),
                            },
                        });
                    }
                }
                "dimensions" => {
                    let dims = &cfg.matrix_dimensions;
                    let chosen: Vec<&cs_model::project::ListItem> = if axis.ids.is_empty() {
                        dims.iter().collect()
                    } else {
                        axis.ids
                            .iter()
                            .map(|id| {
                                dims.iter()
                                    .find(|d| d.id == *id)
                                    .ok_or_else(|| validation(format!("unknown dimension {id}")))
                            })
                            .collect::<Result<_>>()?
                    };
                    for d in chosen {
                        out.push(Resolved {
                            set: None,
                            entry: MatrixEntry {
                                key: format!("dim:{}", d.id),
                                kind: "dimension".into(),
                                id: d.id.clone(),
                                coded: true,
                                label: d.label.clone(),
                            },
                        });
                    }
                }
                "cases" => {
                    let all = self.cases(None)?;
                    let chosen: Vec<&cs_model::Case> = if axis.ids.is_empty() {
                        all.iter().collect()
                    } else {
                        axis.ids
                            .iter()
                            .map(|id| {
                                all.iter()
                                    .find(|c| c.case_id == *id)
                                    .ok_or_else(|| validation(format!("unknown case {id}")))
                            })
                            .collect::<Result<_>>()?
                    };
                    for c in chosen {
                        let units: BTreeSet<&String> = c.units.iter().collect();
                        let set: BTreeSet<usize> = idx
                            .passages
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| units.contains(&p.unit_id))
                            .map(|(i, _)| i)
                            .collect();
                        out.push(Resolved {
                            set: Some(restrict(idx, set, scope)),
                            entry: MatrixEntry {
                                key: format!("case:{}", c.case_id),
                                kind: "case".into(),
                                id: c.case_id.clone(),
                                coded: !c.units.is_empty(),
                                label: c.label.clone(),
                            },
                        });
                    }
                }
                other => {
                    return Err(validation(format!(
                        "unknown axis kind {other}; use interviews, attribute, themes, codes or dimensions"
                    )))
                }
            }
        }
        // Only one entry per key on an axis.
        let mut seen = BTreeSet::new();
        out.retain(|r| seen.insert(r.entry.key.clone()));
        Ok(out)
    }

    pub(crate) fn stored_cells(
        &self,
    ) -> Result<BTreeMap<(String, String), (Option<String>, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT row_key, col_key, value, summary FROM matrix_cells")?;
        let out = stmt
            .query_map([], |r| {
                Ok((
                    (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
                    (r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?),
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    fn manual_cell_passages(&self) -> Result<BTreeMap<(String, String), Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT row_key, col_key, passage_id FROM matrix_cell_passages
             WHERE removed_at IS NULL ORDER BY added_at, passage_id",
        )?;
        let mut out: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (r, c, p) = row?;
            out.entry((r, c)).or_default().push(p);
        }
        Ok(out)
    }

    /// Computes a matrix for a configuration. Nothing is stored.
    pub fn compute_matrix(&self, config: &MatrixConfig) -> Result<Matrix> {
        if config.rows.is_empty() || config.cols.is_empty() {
            return Err(validation(
                "a matrix needs at least one row axis and one column axis",
            ));
        }
        let scope = self.matrix_scope(&config.filters)?;
        let idx = Index::load(self)?;
        let rows = self.resolve_axes(&config.rows, &idx, &scope)?;
        let cols = self.resolve_axes(&config.cols, &idx, &scope)?;
        let stored = self.stored_cells()?;
        let manual = self.manual_cell_passages()?;
        let in_scope: BTreeSet<&String> = scope.iter().collect();
        let mut cells = Vec::with_capacity(rows.len());
        for r in &rows {
            let mut line = Vec::with_capacity(cols.len());
            for c in &cols {
                let (rk, ck) = (r.entry.key.clone(), c.entry.key.clone());
                let keyed = |m: &BTreeMap<(String, String), Vec<String>>| {
                    m.get(&(rk.clone(), ck.clone()))
                        .or_else(|| m.get(&(ck.clone(), rk.clone())))
                        .cloned()
                };
                let (ids, interviews, is_manual): (Vec<String>, usize, bool) =
                    match (&r.set, &c.set) {
                        (Some(a), Some(b)) => {
                            let both: Vec<&cs_model::passage::Passage> =
                                a.intersection(b).map(|i| &idx.passages[*i]).collect();
                            let iv: BTreeSet<&str> =
                                both.iter().map(|p| p.interview_id.as_str()).collect();
                            (
                                both.iter().map(|p| p.passage_id.clone()).collect(),
                                iv.len(),
                                false,
                            )
                        }
                        _ => {
                            let linked = keyed(&manual).unwrap_or_default();
                            let mut ids = Vec::new();
                            let mut iv: BTreeSet<String> = BTreeSet::new();
                            for pid in linked {
                                if let Some(p) = idx.passages.iter().find(|p| p.passage_id == pid) {
                                    if in_scope.contains(&p.interview_id) {
                                        iv.insert(p.interview_id.clone());
                                        ids.push(pid);
                                    }
                                }
                            }
                            (ids, iv.len(), true)
                        }
                    };
                let saved = stored
                    .get(&(rk.clone(), ck.clone()))
                    .or_else(|| stored.get(&(ck.clone(), rk.clone())));
                let value = saved.and_then(|s| s.0.clone());
                line.push(MatrixCell {
                    row_key: rk,
                    col_key: ck,
                    passages: ids.len(),
                    interviews,
                    passage_ids: ids,
                    value_label: value.as_deref().map(|v| cell_value_label(v).to_string()),
                    value,
                    summary: saved.map(|s| s.1.clone()).unwrap_or_default(),
                    manual: is_manual,
                });
            }
            cells.push(line);
        }
        Ok(Matrix {
            config: config.clone(),
            rows: rows.into_iter().map(|r| r.entry).collect(),
            cols: cols.into_iter().map(|r| r.entry).collect(),
            cells,
            interviews_in_scope: scope,
        })
    }

    /// What can be chosen for rows and columns, with the closed cell values.
    pub fn matrix_options(&self) -> Result<MatrixOptions> {
        let cfg = self.project_config()?;
        let idx = Index::load(self)?;
        let mut attributes = Vec::new();
        for a in &cfg.attributes {
            attributes.push(MatrixAttributeOption {
                id: a.id.clone(),
                label: a.label.clone(),
                values: self.attribute_values_of(&a.id)?,
            });
        }
        let codes = self
            .code_nodes()?
            .into_iter()
            .filter(|n| !n.system && n.active && n.merged_into.is_none())
            .map(|n| MatrixCodeOption {
                focused: idx.is_focused(&n.code_id),
                passages: n.counts.passages_rollup,
                code_id: n.code_id,
                name: n.name,
            })
            .collect();
        Ok(MatrixOptions {
            interviews: self
                .summaries()?
                .into_iter()
                .map(|s| s.interview_id)
                .collect(),
            attributes,
            themes: self
                .themes(None)?
                .into_iter()
                .map(|t| MatrixThemeOption {
                    theme_id: t.theme_id,
                    title: t.title,
                    status: t.status,
                })
                .collect(),
            codes,
            dimensions: cfg
                .matrix_dimensions
                .iter()
                .map(|d| MatrixEntry {
                    key: format!("dim:{}", d.id),
                    kind: "dimension".into(),
                    id: d.id.clone(),
                    coded: true,
                    label: d.label.clone(),
                })
                .collect(),
            cell_values: CELL_VALUES
                .iter()
                .map(|v| MatrixValueOption {
                    value: (*v).into(),
                    label: cell_value_label(v).into(),
                })
                .collect(),
        })
    }

    // ------------------------------------------------------- stored cells

    fn check_key_exists(&self, key: &str) -> Result<()> {
        match parse_key(key)? {
            Key::Interview(id) => self.require_interview(&id),
            Key::AttrValue(a) => {
                if self.project_config()?.attribute(&a).is_none() {
                    return Err(validation(format!("unknown attribute {a}")));
                }
                Ok(())
            }
            Key::Theme(id) => self.theme(&id).map(|_| ()),
            Key::Code(id) => self.resolve_code(&id).map(|_| ()),
            Key::Case(id) => self.case(&id).map(|_| ()),
            Key::Dim(id) => {
                if self
                    .project_config()?
                    .matrix_dimensions
                    .iter()
                    .any(|d| d.id == id)
                {
                    Ok(())
                } else {
                    Err(validation(format!("unknown dimension {id}")))
                }
            }
        }
    }

    /// Records what the researcher says about a cell: its value (one of four),
    /// its one-line summary. A cell with neither is forgotten.
    pub fn set_matrix_cell(&self, patch: &CellPatch, actor: &str) -> Result<()> {
        self.check_key_exists(&patch.row_key)?;
        self.check_key_exists(&patch.col_key)?;
        let old: Option<(Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT value, summary FROM matrix_cells WHERE row_key = ?1 AND col_key = ?2",
                params![patch.row_key, patch.col_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (old_value, old_summary) = old.clone().unwrap_or((None, String::new()));
        let value = match &patch.value {
            None => old_value.clone(),
            Some(None) => None,
            Some(Some(v)) => {
                let v = v.trim();
                if v.is_empty() {
                    None
                } else if CELL_VALUES.contains(&v) {
                    Some(v.to_string())
                } else {
                    return Err(validation(format!(
                        "a cell value is one of {}, not {v}",
                        CELL_VALUES.join(", ")
                    )));
                }
            }
        };
        let summary = match &patch.summary {
            None => old_summary.clone(),
            Some(s) => {
                let s = s.trim();
                if s.chars().count() > MAX_SUMMARY || s.contains('\n') {
                    return Err(validation(format!(
                        "a cell summary is one line of at most {MAX_SUMMARY} characters"
                    )));
                }
                s.to_string()
            }
        };
        if value == old_value && summary == old_summary {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        if value.is_none() && summary.is_empty() {
            tx.execute(
                "DELETE FROM matrix_cells WHERE row_key = ?1 AND col_key = ?2",
                params![patch.row_key, patch.col_key],
            )?;
        } else {
            tx.execute(
                "INSERT INTO matrix_cells (row_key, col_key, value, summary, updated_at, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(row_key, col_key) DO UPDATE SET value = excluded.value,
                   summary = excluded.summary, updated_at = excluded.updated_at,
                   updated_by = excluded.updated_by",
                params![patch.row_key, patch.col_key, value, summary, now(), actor],
            )?;
        }
        append_event(
            &tx,
            actor,
            "matrix.cell",
            None,
            Some(&format!("{}|{}", patch.row_key, patch.col_key)),
            json!({
                "row_key": patch.row_key, "col_key": patch.col_key,
                "before": {"value": old_value, "summary": old_summary},
                "after": {"value": value, "summary": summary},
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Links a passage to a cell by hand (the way to fill a custom dimension).
    pub fn link_cell_passage(
        &self,
        row_key: &str,
        col_key: &str,
        passage_id: &str,
        actor: &str,
    ) -> Result<()> {
        self.check_key_exists(row_key)?;
        self.check_key_exists(col_key)?;
        self.passage(passage_id)?;
        let present: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT removed_at FROM matrix_cell_passages
                 WHERE row_key = ?1 AND col_key = ?2 AND passage_id = ?3",
                params![row_key, col_key, passage_id],
                |r| r.get(0),
            )
            .optional()?;
        if matches!(present, Some(None)) {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO matrix_cell_passages (row_key, col_key, passage_id, added_at, added_by, removed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)
             ON CONFLICT(row_key, col_key, passage_id) DO UPDATE SET removed_at = NULL,
               added_at = excluded.added_at, added_by = excluded.added_by",
            params![row_key, col_key, passage_id, now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            "matrix.cell.link",
            None,
            Some(&format!("{row_key}|{col_key}")),
            json!({"row_key": row_key, "col_key": col_key, "passage_id": passage_id}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Removes a hand-made link between a passage and a cell.
    pub fn unlink_cell_passage(
        &self,
        row_key: &str,
        col_key: &str,
        passage_id: &str,
        actor: &str,
    ) -> Result<()> {
        let n = self.conn.query_row(
            "SELECT COUNT(*) FROM matrix_cell_passages
             WHERE row_key = ?1 AND col_key = ?2 AND passage_id = ?3 AND removed_at IS NULL",
            params![row_key, col_key, passage_id],
            |r| r.get::<_, i64>(0),
        )?;
        if n == 0 {
            return Err(not_found(format!(
                "no link between {passage_id} and the cell {row_key} x {col_key}"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE matrix_cell_passages SET removed_at = ?4
             WHERE row_key = ?1 AND col_key = ?2 AND passage_id = ?3",
            params![row_key, col_key, passage_id, now()],
        )?;
        append_event(
            &tx,
            actor,
            "matrix.cell.unlink",
            None,
            Some(&format!("{row_key}|{col_key}")),
            json!({"row_key": row_key, "col_key": col_key, "passage_id": passage_id}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every stored cell (value or summary), for export.
    pub fn matrix_cells_stored(&self) -> Result<Vec<(String, String, Option<String>, String)>> {
        Ok(self
            .stored_cells()?
            .into_iter()
            .map(|((r, c), (v, s))| (r, c, v, s))
            .collect())
    }

    /// Every hand-made cell link, for export.
    pub fn matrix_cell_links(&self) -> Result<Vec<(String, String, String)>> {
        Ok(self
            .manual_cell_passages()?
            .into_iter()
            .flat_map(|((r, c), ps)| ps.into_iter().map(move |p| (r.clone(), c.clone(), p)))
            .collect())
    }

    // ------------------------------------------------------ saved views

    fn check_config(&self, config: &MatrixConfig) -> Result<()> {
        // Computing it is the validation: unknown ids and kinds are refused.
        self.compute_matrix(config).map(|_| ())
    }

    fn next_view_id(&self) -> Result<String> {
        let max: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(CAST(substr(view_id, 3) AS INTEGER)), 0) FROM matrix_views",
            [],
            |r| r.get(0),
        )?;
        Ok(format!("M-{:04}", max + 1))
    }

    fn view_record(&self, view_id: &str) -> Result<MatrixViewRecord> {
        let row = self
            .conn
            .query_row(
                "SELECT view_id, name, config, created_at, updated_at FROM matrix_views
                 WHERE view_id = ?1 AND deleted_at IS NULL",
                [view_id.trim()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| not_found(format!("matrix view {view_id}")))?;
        Ok(MatrixViewRecord {
            view_id: row.0,
            name: row.1,
            config: from_json(&row.2)?,
            created_at: row.3,
            updated_at: row.4,
        })
    }

    /// Saved matrix views, oldest first.
    pub fn matrix_views(&self) -> Result<Vec<MatrixViewRecord>> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT view_id FROM matrix_views WHERE deleted_at IS NULL ORDER BY view_id",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        ids.iter().map(|id| self.view_record(id)).collect()
    }

    /// One saved view with its matrix computed now.
    pub fn matrix_view(&self, view_id: &str) -> Result<(MatrixViewRecord, Matrix)> {
        let rec = self.view_record(view_id)?;
        let m = self.compute_matrix(&rec.config)?;
        Ok((rec, m))
    }

    /// Saves a configuration under a name.
    pub fn create_matrix_view(
        &self,
        name: &str,
        config: &MatrixConfig,
        actor: &str,
    ) -> Result<MatrixViewRecord> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 200 || name.contains('\n') {
            return Err(validation("a matrix view needs a one-line name"));
        }
        self.check_config(config)?;
        let id = self.next_view_id()?;
        let ts = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO matrix_views (view_id, name, config, created_at, created_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?4)",
            params![id, name, to_json(config), ts, actor],
        )?;
        append_event(
            &tx,
            actor,
            "matrix.view.create",
            None,
            Some(&id),
            json!({"view_id": id, "name": name, "config": config}),
        )?;
        tx.commit()?;
        self.view_record(&id)
    }

    /// Renames a view and/or replaces its configuration.
    pub fn update_matrix_view(
        &self,
        view_id: &str,
        name: Option<&str>,
        config: Option<&MatrixConfig>,
        actor: &str,
    ) -> Result<MatrixViewRecord> {
        let before = self.view_record(view_id)?;
        let name = match name.map(str::trim) {
            None => before.name.clone(),
            Some(n) if n.is_empty() || n.chars().count() > 200 || n.contains('\n') => {
                return Err(validation("a matrix view needs a one-line name"))
            }
            Some(n) => n.to_string(),
        };
        let config = match config {
            None => before.config.clone(),
            Some(c) => {
                self.check_config(c)?;
                c.clone()
            }
        };
        if name == before.name && config == before.config {
            return Ok(before);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE matrix_views SET name = ?2, config = ?3, updated_at = ?4 WHERE view_id = ?1",
            params![before.view_id, name, to_json(&config), now()],
        )?;
        append_event(
            &tx,
            actor,
            "matrix.view.update",
            None,
            Some(&before.view_id),
            json!({
                "view_id": before.view_id,
                "before": {"name": before.name, "config": before.config},
                "after": {"name": name, "config": config},
            }),
        )?;
        tx.commit()?;
        self.view_record(&before.view_id)
    }

    /// Deletes a saved view (the cells the researcher filled stay).
    pub fn delete_matrix_view(&self, view_id: &str, actor: &str) -> Result<()> {
        let rec = self.view_record(view_id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE matrix_views SET deleted_at = ?2 WHERE view_id = ?1",
            params![rec.view_id, now()],
        )?;
        append_event(
            &tx,
            actor,
            "matrix.view.delete",
            None,
            Some(&rec.view_id),
            json!({"view_id": rec.view_id, "name": rec.name}),
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Keeps the passages of the interviews in scope.
fn restrict(idx: &Index, set: BTreeSet<usize>, scope: &[String]) -> BTreeSet<usize> {
    set.into_iter()
        .filter(|i| scope.contains(&idx.passages[*i].interview_id))
        .collect()
}

/// A passage of a cell, with who spoke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CellPassage {
    #[serde(flatten)]
    pub brief: crate::links::PassageBrief,
    pub speaker: String,
}

/// One cell opened: its passages and what the researcher said of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CellDetail {
    pub row_key: String,
    pub col_key: String,
    pub row_label: String,
    pub col_label: String,
    pub value: Option<String>,
    pub value_label: Option<String>,
    pub summary: String,
    pub passages: Vec<CellPassage>,
    pub n_interviews: usize,
    pub manual: bool,
}

/// The axis that lists exactly the entry of a key.
fn axis_of_key(key: &str) -> Result<MatrixAxis> {
    let axis = |kind: &str, ids: Vec<String>, attribute: Option<String>| MatrixAxis {
        kind: kind.into(),
        ids,
        attribute,
        status: None,
    };
    Ok(match parse_key(key)? {
        Key::Interview(id) => axis("interviews", vec![id], None),
        Key::AttrValue(a) => {
            let value = key
                .split_once('=')
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            axis("attribute", vec![value], Some(a))
        }
        Key::Theme(id) => axis("themes", vec![id], None),
        Key::Code(id) => axis("codes", vec![id], None),
        Key::Dim(id) => axis("dimensions", vec![id], None),
        Key::Case(id) => axis("cases", vec![id], None),
    })
}

impl Store {
    /// Opens one cell: the passages behind it, who spoke, the value and summary.
    pub fn matrix_cell_detail(
        &self,
        row_key: &str,
        col_key: &str,
        filters: &[MatrixFilter],
    ) -> Result<CellDetail> {
        let config = MatrixConfig {
            rows: vec![axis_of_key(row_key)?],
            cols: vec![axis_of_key(col_key)?],
            filters: filters.to_vec(),
        };
        let m = self.compute_matrix(&config)?;
        let (Some(r), Some(c)) = (m.rows.first(), m.cols.first()) else {
            return Err(not_found(format!(
                "cell {row_key} x {col_key} (outside the filters)"
            )));
        };
        let cell = &m.cells[0][0];
        let words = self.verification_words()?;
        let mut passages = Vec::new();
        for pid in &cell.passage_ids {
            let p = self.passage(pid)?;
            let speaker = self
                .current_unit(&p.interview_id, &p.unit_id)
                .map(|u| u.speaker_raw)
                .unwrap_or_default();
            passages.push(CellPassage {
                brief: self.brief_of(&p, &words),
                speaker,
            });
        }
        Ok(CellDetail {
            row_key: r.key.clone(),
            col_key: c.key.clone(),
            row_label: r.label.clone(),
            col_label: c.label.clone(),
            value: cell.value.clone(),
            value_label: cell.value_label.clone(),
            summary: cell.summary.clone(),
            passages,
            n_interviews: cell.interviews,
            manual: cell.manual,
        })
    }
}
