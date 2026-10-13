//! V1 lot 5 exports: themes, theme sheets, matrix, typed links, the write-up,
//! the final audit and the documented decisions. The files are written by
//! `write_files` into any export folder (the cross-case package and the
//! write-up package) and are covered by that folder's manifest. The same data,
//! as canonical JSON, goes into `state.json` through `state_parts`.

use std::collections::BTreeMap;

use anyhow::Result;
use cs_model::analysis::{cell_value_label, decision_outcome_label};
use cs_store::Store;
use serde_json::{json, Value};

use crate::export::{csv_field, Writer};

fn row(cells: &[String]) -> String {
    let line: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
    format!("{}\n", line.join(","))
}

fn s(v: &str) -> String {
    v.to_string()
}

pub(crate) fn themes_csv(store: &Store) -> Result<String> {
    let mut out = row(&[
        s("theme_id"),
        s("order"),
        s("title"),
        s("status"),
        s("proposition"),
        s("code_ids"),
        s("code_names"),
        s("research_questions"),
        s("passages"),
        s("interviews"),
        s("checks_done"),
        s("checks_total"),
        s("created_at"),
        s("updated_at"),
    ]);
    for t in store.themes(None)? {
        out.push_str(&row(&[
            t.theme_id.clone(),
            t.order.to_string(),
            t.title.clone(),
            t.status.clone(),
            t.proposition.clone(),
            t.codes
                .iter()
                .map(|c| c.code_id.clone())
                .collect::<Vec<_>>()
                .join(";"),
            t.codes
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(";"),
            t.research_questions.join(";"),
            t.counts.passages.to_string(),
            t.counts.interviews.to_string(),
            t.checks_done.to_string(),
            t.checks_total.to_string(),
            t.created_at.clone(),
            t.updated_at.clone(),
        ]));
    }
    Ok(out)
}

pub(crate) fn theme_sheets_value(store: &Store) -> Result<Value> {
    let mut out = Vec::new();
    for t in store.themes(None)? {
        let history = store.theme_history(&t.theme_id)?;
        let writing = store.theme_writing(&t.theme_id)?;
        let parts: BTreeMap<String, Value> = writing
            .parts
            .iter()
            .map(|(k, p)| {
                (
                    k.clone(),
                    json!({
                        "text": p.text, "nature": p.nature,
                        "links": p.links.iter().map(|l| &l.link_id).collect::<Vec<_>>(),
                    }),
                )
            })
            .collect();
        out.push(json!({"sheet": t, "history": history, "parts": parts}));
    }
    Ok(json!(out))
}

pub(crate) fn matrix_csv(store: &Store) -> Result<String> {
    let mut out = row(&[
        s("view_id"),
        s("view_name"),
        s("row_key"),
        s("row_label"),
        s("col_key"),
        s("col_label"),
        s("passages"),
        s("interviews"),
        s("value"),
        s("value_label"),
        s("summary"),
        s("passages_linked_by_hand"),
    ]);
    let mut seen: BTreeMap<(String, String), ()> = BTreeMap::new();
    for rec in store.matrix_views()? {
        let Ok(m) = store.compute_matrix(&rec.config) else {
            continue;
        };
        for (i, r) in m.rows.iter().enumerate() {
            for (j, c) in m.cols.iter().enumerate() {
                let cell = &m.cells[i][j];
                seen.insert((r.key.clone(), c.key.clone()), ());
                out.push_str(&row(&[
                    rec.view_id.clone(),
                    rec.name.clone(),
                    r.key.clone(),
                    r.label.clone(),
                    c.key.clone(),
                    c.label.clone(),
                    cell.passages.to_string(),
                    cell.interviews.to_string(),
                    cell.value.clone().unwrap_or_default(),
                    cell.value_label.clone().unwrap_or_default(),
                    cell.summary.clone(),
                    if cell.manual { s("1") } else { s("0") },
                ]));
            }
        }
    }
    // What the researcher said about cells that no saved view shows.
    for (r, c, value, summary) in store.matrix_cells_stored()? {
        if seen.contains_key(&(r.clone(), c.clone())) {
            continue;
        }
        out.push_str(&row(&[
            String::new(),
            String::new(),
            r,
            String::new(),
            c,
            String::new(),
            String::new(),
            String::new(),
            value.clone().unwrap_or_default(),
            value
                .as_deref()
                .map(|v| cell_value_label(v).to_string())
                .unwrap_or_default(),
            summary,
            String::new(),
        ]));
    }
    Ok(out)
}

pub(crate) fn links_csv(store: &Store) -> Result<String> {
    let mut out = row(&[
        s("link_id"),
        s("passage_id"),
        s("interview_id"),
        s("unit_id"),
        s("target_kind"),
        s("target_id"),
        s("part"),
        s("link_type"),
        s("note"),
        s("quote"),
        s("verification"),
        s("created_at"),
        s("created_by"),
    ]);
    for l in store.links()? {
        let (kind, id, part) = match &l.target {
            cs_model::analysis::LinkTarget::ThemePart { theme_id, part } => {
                ("theme_part", theme_id.clone(), part.clone())
            }
            cs_model::analysis::LinkTarget::RqPart { rq_id } => {
                ("rq_part", rq_id.clone(), String::new())
            }
        };
        out.push_str(&row(&[
            l.link_id.clone(),
            l.passage.passage_id.clone(),
            l.passage.interview_id.clone(),
            l.passage.unit_id.clone(),
            s(kind),
            id,
            part,
            l.link_type.clone(),
            l.note.clone(),
            l.passage.text.clone(),
            l.passage.verification.clone().unwrap_or_default(),
            l.created_at.clone(),
            l.created_by.clone(),
        ]));
    }
    // Passages attached by hand to a matrix cell.
    for (r, c, p) in store.matrix_cell_links()? {
        out.push_str(&row(&[
            String::new(),
            p,
            String::new(),
            String::new(),
            s("matrix_cell"),
            r,
            c,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]));
    }
    Ok(out)
}

pub(crate) fn final_audit_value(store: &Store) -> Result<Value> {
    let mut audits = Vec::new();
    for id in store.final_audit_ids()?.into_iter().rev() {
        audits.push(store.final_audit(id)?);
    }
    Ok(json!({
        "note": "Each audit records its seed: the same seed over the same write-up gives the same draw.",
        "audits": audits,
        "verdict_log": store.audit_verdict_log()?,
    }))
}

pub(crate) fn decisions_csv(store: &Store) -> Result<String> {
    let mut out = row(&[
        s("decision_id"),
        s("interview_id"),
        s("name"),
        s("outcome"),
        s("outcome_label"),
        s("decision_as_written"),
        s("characteristics"),
        s("units"),
        s("passages"),
    ]);
    for c in store.cases(None)? {
        out.push_str(&row(&[
            c.case_id.clone(),
            c.interview_id.clone(),
            c.label.clone(),
            c.outcome.clone().unwrap_or_default(),
            c.outcome
                .as_deref()
                .map(|o| decision_outcome_label(o).to_string())
                .unwrap_or_default(),
            c.decision.clone().unwrap_or_default(),
            c.characteristics
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(";"),
            c.units.join(";"),
            c.passages.join(";"),
        ]));
    }
    Ok(out)
}

/// Writes the lot 5 files into an export folder.
pub(crate) fn write_files(w: &mut Writer, store: &Store) -> Result<()> {
    w.write("themes.csv", themes_csv(store)?.as_bytes())?;
    w.write(
        "theme_sheets.json",
        &serde_json::to_vec_pretty(&theme_sheets_value(store)?)?,
    )?;
    w.write("matrix.csv", matrix_csv(store)?.as_bytes())?;
    w.write("links.csv", links_csv(store)?.as_bytes())?;
    w.write("writeup.md", store.writeup_markdown()?.as_bytes())?;
    w.write(
        "final_audit.json",
        &serde_json::to_vec_pretty(&final_audit_value(store)?)?,
    )?;
    w.write("decisions.csv", decisions_csv(store)?.as_bytes())?;
    w.write(
        "decisions.json",
        &serde_json::to_vec_pretty(&json!(store.cases(None)?))?,
    )?;
    Ok(())
}

/// Lines of the README of an export folder that carries the lot 5 files.
pub(crate) const README_LINES: [(&str, &str); 8] = [
    ("themes.csv", "One row per theme in the document order: `theme_id,order,title,status,proposition,code_ids,code_names,research_questions,passages,interviews,checks_done,checks_total,created_at,updated_at`. The title is a sentence. `status` is candidate, revised or retained. `passages` and `interviews` count the passages carrying the theme's codes (descendants included) and their distinct interviews."),
    ("theme_sheets.json", "Each theme with its full sheet (proposition, conditions, alternative interpretation, limits, decision memo, supporting and divergent cases, checks with their answers and notes), its history (creation, title changes, status changes with the reason and whether checks were overridden) and its five written parts with the ids of their links."),
    ("matrix.csv", "Every cell of every saved matrix view, `view_id,view_name,row_key,row_label,col_key,col_label,passages,interviews,value,value_label,summary,passages_linked_by_hand`, then the cells the researcher filled that no saved view shows. `passages` is computed from the codings at export time; `value` is one of present, not_mentioned, contradicted, uncertain, and is empty until the researcher sets it."),
    ("links.csv", "The typed links between passages and the parts of the write-up, `link_id,passage_id,interview_id,unit_id,target_kind,target_id,part,link_type,note,quote,verification,created_at,created_by`, with the exact quotation; then the passages attached by hand to a matrix cell (`target_kind` = matrix_cell)."),
    ("writeup.md", "The write-up in Markdown: retained themes in the document order with their five parts, the synthesis per research question, the framework reading table when the project has a framework, the latest final audit. Each linked passage is a footnote: interview, unit, exact quotation."),
    ("final_audit.json", "Every draw of the final audit with its seed, the claims drawn, the chain of each (part, theme, codes, passages, quotation, interview, verification), the gaps found, and the verdicts given (supported, to_nuance, to_withdraw) with their notes, plus the complete log of verdicts."),
    ("decisions.csv", "The documented decisions in the words of the guide, `decision_id,interview_id,name,outcome,outcome_label,decision_as_written,characteristics,units,passages`. `outcome` is adopt, defer or reject; `decision_as_written` keeps what earlier versions stored."),
    ("decisions.json", "The documented decisions in full, with their five synthesis fields, characteristics and passages."),
];

/// What the lot 5 data add to the canonical `state.json` parts.
pub(crate) fn state_parts(store: &Store) -> Result<BTreeMap<&'static str, Value>> {
    let mut parts = BTreeMap::new();
    parts.insert("themes", theme_sheets_value(store)?);
    let mut versions: BTreeMap<String, Value> = BTreeMap::new();
    let part_keys: Vec<&str> = cs_model::analysis::THEME_PARTS
        .iter()
        .map(|(k, _)| *k)
        .collect();
    for t in store.themes(None)? {
        for k in &part_keys {
            let v = store.writing_versions("theme_part", &t.theme_id, k)?;
            if !v.is_empty() {
                versions.insert(format!("theme_part:{}:{k}", t.theme_id), json!(v));
            }
        }
        if store.framework_reading(&t.theme_id).is_ok() {
            let v = store.writing_versions("framework_theme", &t.theme_id, "")?;
            if !v.is_empty() {
                versions.insert(format!("framework_theme:{}", t.theme_id), json!(v));
            }
        }
    }
    for q in store.research_questions()? {
        let v = store.writing_versions("rq_synthesis", &q.id, "")?;
        if !v.is_empty() {
            versions.insert(format!("rq_synthesis:{}", q.id), json!(v));
        }
    }
    parts.insert("writing_versions", json!(versions));
    parts.insert("links", json!(store.links()?));
    parts.insert(
        "matrix",
        json!({
            "views": store.matrix_views()?,
            "cells": store.matrix_cells_stored()?,
            "cell_passages": store.matrix_cell_links()?,
        }),
    );
    parts.insert("final_audit", final_audit_value(store)?);
    Ok(parts)
}
