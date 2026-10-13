//! Files of the V1 lots 1 and 2 in the exported packages: passages,
//! highlights, interview attributes, memos and checklists, the study
//! configuration. Each is registered in the manifest by the writer, so its
//! SHA-256 is covered like every other file. `docs/CONTRACTS.md` lists them.

use std::collections::BTreeMap;

use anyhow::Result;
use cs_model::passage::Passage;
use cs_store::{PassageFilter, Store};
use serde_json::{json, Value};

use crate::export::csv_field;

fn row(cells: &[String]) -> String {
    let mut line = cells
        .iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",");
    line.push('\n');
    line
}

/// Every passage of the given interviews, highlights included, in
/// interview and position order.
pub(crate) fn all_passages(store: &Store, interview_ids: &[String]) -> Result<Vec<Passage>> {
    let mut out = Vec::new();
    for id in interview_ids {
        out.extend(store.passages(&PassageFilter {
            interview_id: Some(id.clone()),
            ..Default::default()
        })?);
    }
    Ok(out)
}

/// `passage_id,interview_id,unit_id,start,end,text,kind,code_ids,code_names,later,later_note,stale,anchor_digest,created_at,created_by`.
/// Positions are Unicode scalar values, end exclusive. The reserved code
/// "Plus tard" is not in `code_ids`: it is the `later` flag.
pub(crate) fn passages_csv(passages: &[Passage]) -> String {
    let mut out = String::from(
        "passage_id,interview_id,unit_id,start,end,text,kind,code_ids,code_names,later,later_note,stale,anchor_digest,created_at,created_by\n",
    );
    for p in passages {
        let real: Vec<_> = p.codings.iter().filter(|c| !c.system).collect();
        let later = p.codings.iter().find(|c| c.system);
        out.push_str(&row(&[
            p.passage_id.clone(),
            p.interview_id.clone(),
            p.unit_id.clone(),
            p.start.to_string(),
            p.end.to_string(),
            p.text.clone(),
            p.kind.clone(),
            real.iter()
                .map(|c| c.code_id.as_str())
                .collect::<Vec<_>>()
                .join(";"),
            real.iter()
                .map(|c| c.code_name.as_str())
                .collect::<Vec<_>>()
                .join(";"),
            (later.is_some() as u8).to_string(),
            later.map(|c| c.note.clone()).unwrap_or_default(),
            (p.stale as u8).to_string(),
            p.anchor_digest.clone(),
            p.created_at.clone(),
            p.created_by.clone(),
        ]));
    }
    out
}

/// The passages without an active code: `passage_id,interview_id,unit_id,start,end,text,stale,created_at,created_by`.
pub(crate) fn highlights_csv(passages: &[Passage]) -> String {
    let mut out = String::from(
        "passage_id,interview_id,unit_id,start,end,text,stale,created_at,created_by\n",
    );
    for p in passages.iter().filter(|p| p.codings.is_empty()) {
        out.push_str(&row(&[
            p.passage_id.clone(),
            p.interview_id.clone(),
            p.unit_id.clone(),
            p.start.to_string(),
            p.end.to_string(),
            p.text.clone(),
            (p.stale as u8).to_string(),
            p.created_at.clone(),
            p.created_by.clone(),
        ]));
    }
    out
}

/// One line per interview and attribute that has a value:
/// `interview_id,attribute_id,label,kind,value`.
pub(crate) fn attributes_csv(store: &Store, interview_ids: &[String]) -> Result<String> {
    let mut out = String::from("interview_id,attribute_id,label,kind,value\n");
    for id in interview_ids {
        let view = store.interview_attributes(id)?;
        for def in &view.definitions {
            if let Some(v) = view.values.get(&def.id) {
                out.push_str(&row(&[
                    id.clone(),
                    def.id.clone(),
                    def.label.clone(),
                    format!("{:?}", def.kind).to_lowercase(),
                    v.clone(),
                ]));
            }
        }
    }
    Ok(out)
}

/// Memos (template, version, the interview's own edits, answers with their
/// times) and checklists of the given interviews.
pub(crate) fn memos_value(store: &Store, interview_ids: &[String]) -> Result<Value> {
    let mut items = Vec::new();
    for id in interview_ids {
        items.push(json!({
            "interview_id": id,
            "memo": store.interview_memo(id)?,
            "checklist": store.interview_checklist(id)?,
        }));
    }
    Ok(json!({ "interviews": items }))
}

pub(crate) fn memos_json(store: &Store, interview_ids: &[String]) -> Result<Vec<u8>> {
    Ok(serde_json::to_string_pretty(&memos_value(store, interview_ids)?)?.into_bytes())
}

/// The study configuration as a project template (the same JSON the template
/// export gives): study description and settings, research questions,
/// attributes, memo templates, lists, framework. No interview data.
pub(crate) fn study_config_json(store: &Store) -> Result<Vec<u8>> {
    Ok(serde_json::to_string_pretty(&store.export_template()?)?.into_bytes())
}

/// Lot 4 (Review): the candidate concepts and their decisions, the
/// pseudonymisation list digest and categories (never the names), the
/// harmonisation worklist and the review state of the given interviews.
/// Candidate text is kept as the model saw it (pseudonymised).
pub(crate) fn review_value(store: &Store, interview_ids: &[String]) -> Result<Value> {
    let cfg = store.pseudonym_config()?;
    let mut items = Vec::new();
    for id in interview_ids {
        items.push(json!({
            "interview_id": id,
            "concept_candidates": store.concept_candidates(id)?,
            "harmonisation": store.harmonisation(Some(id))?,
            "review_state": store.review_state(id)?,
        }));
    }
    Ok(json!({
        "pseudonymisation": {
            "enabled": cfg.enabled,
            "digest": cfg.digest(),
            "categories": cfg.categories(),
            "n_entries": cfg.entries.len(),
        },
        "interviews": items,
    }))
}

pub(crate) fn review_json(store: &Store, interview_ids: &[String]) -> Result<Vec<u8>> {
    Ok(serde_json::to_string_pretty(&review_value(store, interview_ids)?)?.into_bytes())
}

/// What the V1 lots add to the canonical `state.json` parts.
pub(crate) fn state_parts(
    store: &Store,
    interview_ids: &[String],
) -> Result<BTreeMap<&'static str, Value>> {
    let mut parts = BTreeMap::new();
    parts.insert("passages", json!(all_passages(store, interview_ids)?));
    let mut attrs = BTreeMap::new();
    for id in interview_ids {
        attrs.insert(id.clone(), store.interview_attributes(id)?.values);
    }
    parts.insert("attribute_values", json!(attrs));
    parts.insert("memos_and_checklists", memos_value(store, interview_ids)?);
    parts.insert("project_config", json!(store.export_template()?));
    parts.insert("review", review_value(store, interview_ids)?);
    let tree: Vec<Value> = store
        .code_nodes()?
        .into_iter()
        .filter(|n| !n.system)
        .map(|n| {
            json!({
                "code_id": n.code_id, "token": n.token, "name": n.name,
                "parent_id": n.parent_id, "aliases": n.aliases,
                "includes": n.includes, "excludes": n.excludes, "memo": n.memo,
            })
        })
        .collect();
    parts.insert("code_tree", json!(tree));
    let mut marks = BTreeMap::new();
    for id in interview_ids {
        marks.insert(id.clone(), store.unit_marks(id)?);
    }
    parts.insert("unit_marks", json!(marks));
    for (k, v) in crate::export_analysis::state_parts(store)? {
        parts.insert(k, v);
    }
    Ok(parts)
}
