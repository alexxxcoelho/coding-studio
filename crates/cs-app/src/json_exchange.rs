//! Local JSON exchange: preview first, then atomic commit against the exact
//! reviewed state. Does not call a model or modify frozen review evidence.
use crate::{ApiResult, AppState};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use cs_model::{Arbitration, ArbitrationChoice, CsError, NA, PHASE_2};
use cs_store::{CodebookJson, Store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/writing/json", get(export_writing))
        .route("/api/writing/json/import/preview", post(preview_writing))
        .route("/api/writing/json/import", post(import_writing))
        .route("/api/codebook/json", get(export_codebook))
        .route("/api/codebook/json/import/preview", post(preview_codebook))
        .route("/api/codebook/json/import", post(import_codebook))
        .route(
            "/api/interviews/{id}/review/import/preview",
            post(preview_decisions),
        )
        .route("/api/interviews/{id}/review/import", post(import_decisions))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
}
fn invalid(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Validation(message.into()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportBody {
    raw: String,
    expected_digest: Option<String>,
    #[serde(default)]
    replace_existing: bool,
}
fn parse<T: serde::de::DeserializeOwned>(raw: &str) -> anyhow::Result<T> {
    let raw = raw.trim();
    let raw = if raw.starts_with("```json\n") || raw.starts_with("```\n") {
        raw.split_once('\n')
            .unwrap()
            .1
            .strip_suffix("```")
            .ok_or_else(|| invalid("unclosed JSON fence"))?
            .trim()
    } else {
        raw
    };
    serde_json::from_str(raw).map_err(|e| invalid(format!("Invalid JSON: {e}")))
}
fn hash(v: &Value) -> String {
    cs_model::sha256_hex(serde_json::to_string(v).unwrap().as_bytes())
}
fn check_digest(expected: Option<&str>, actual: &str) -> anyhow::Result<()> {
    if expected != Some(actual) {
        return Err(invalid(
            "The data changed or the preview is missing. Preview the JSON again before importing.",
        ));
    }
    Ok(())
}
async fn export_codebook(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().export_codebook_json()?)))
}
fn codebook_preview(store: &Store, body: &ImportBody) -> anyhow::Result<(CodebookJson, Value)> {
    let doc: CodebookJson = parse(&body.raw)?;
    let changes = store.preview_codebook_json(&doc)?;
    let current = store.export_codebook_json()?;
    let digest =
        hash(&json!({"current":current,"input":doc,"questions":store.research_questions()?}));
    Ok((
        doc,
        json!({"digest":digest,"count":changes.len(),"replacements":0,"changes":changes}),
    ))
}
async fn preview_codebook(State(st): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    Ok(Json(codebook_preview(&st.store(), &body)?.1))
}
async fn import_codebook(State(st): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    let store = st.store();
    let (doc, preview) = codebook_preview(&store, &body)?;
    check_digest(
        body.expected_digest.as_deref(),
        preview["digest"].as_str().unwrap(),
    )?;
    let changes = store.import_codebook_tree_json(&doc, &st.actor)?;
    Ok(Json(json!({"count":changes.len()})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionsJson {
    interview_id: String,
    decisions: Vec<DecisionJson>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionJson {
    unit_id: String,
    decision: String,
    final_codes: Vec<String>,
    comment: String,
    #[serde(default)]
    run_label: Option<String>,
}
fn decisions_preview(
    store: &Store,
    id: &str,
    body: &ImportBody,
    actor: &str,
) -> anyhow::Result<(Vec<Arbitration>, Value)> {
    let input: DecisionsJson = parse(&body.raw)?;
    if input.interview_id != id {
        return Err(invalid(format!("interview_id must be {id}")));
    }
    let review = store.review(id)?;
    let nodes = store.code_nodes()?;
    let segmentation = store
        .segmentation(id)?
        .ok_or_else(|| invalid("missing segmentation"))?;
    let mut result = Vec::new();
    let mut changes = Vec::new();
    let mut replacements = 0;
    for row in input.decisions {
        let item = review
            .items
            .iter()
            .find(|i| i.item.unit_id == row.unit_id)
            .ok_or_else(|| invalid(format!("{} is not in the review", row.unit_id)))?;
        let mut codes = BTreeSet::new();
        for key in &row.final_codes {
            if key == NA {
                codes.insert(NA.to_string());
                continue;
            }
            let code = nodes
                .iter()
                .find(|n| n.code_id == *key || n.token == *key)
                .ok_or_else(|| invalid(format!("unknown code {key}")))?;
            if !code.active || code.system || code.merged_into.is_some() {
                return Err(invalid(format!("inactive, system or merged code: {key}")));
            }
            codes.insert(code.token.clone());
        }
        let final_codes = codes;
        let same = |other: &BTreeSet<String>| other == &final_codes;
        let choice = match row.decision.as_str() {
            "keep_mine" => {
                if row.run_label.is_some() {
                    return Err(invalid("run_label is only allowed for accept_reviewer"));
                }
                if same(&item.item.human) {
                    ArbitrationChoice::Human
                } else {
                    ArbitrationChoice::Custom
                }
            }
            "accept_reviewer" => {
                let matching = if let Some(label) = &row.run_label {
                    let values = item
                        .item
                        .ai
                        .get(label)
                        .ok_or_else(|| invalid(format!("unknown run {label}")))?;
                    if !same(values) {
                        return Err(invalid(format!(
                            "final_codes do not match run {label} for {}",
                            row.unit_id
                        )));
                    }
                    Some(label.clone())
                } else {
                    item.item
                        .ai
                        .iter()
                        .find(|(_, v)| same(v))
                        .map(|(l, _)| l.clone())
                };
                matching
                    .map(|run_label| ArbitrationChoice::Ai { run_label })
                    .unwrap_or(ArbitrationChoice::Custom)
            }
            "merge" | "custom" => {
                if row.run_label.is_some() {
                    return Err(invalid("run_label is only allowed for accept_reviewer"));
                }
                ArbitrationChoice::Custom
            }
            other => {
                return Err(invalid(format!(
                    "unknown decision {other}; use keep_mine, accept_reviewer, merge or custom"
                )))
            }
        };
        let a = Arbitration {
            unit_id: row.unit_id.clone(),
            phase: PHASE_2,
            choice,
            final_codes,
            rationale: row.comment,
            decided_by: actor.into(),
            decided_at: String::new(),
        };
        if item.item.decision.is_some() {
            replacements += 1;
        }
        let excerpt = segmentation
            .units
            .iter()
            .find(|u| u.unit_id == row.unit_id)
            .map(|u| u.text.as_str());
        changes.push(json!({"unit_id":row.unit_id,"excerpt":excerpt,"requested_decision":row.decision,"before":item.item.decision,"after":a}));
        result.push(a);
    }
    store.validate_decisions(id, &result, actor)?;
    let digest = hash(
        &json!({"review":review,"codebook":nodes,"segmentation_hash":segmentation.hash,"raw":body.raw}),
    );
    Ok((
        result,
        json!({"digest":digest,"count":changes.len(),"replacements":replacements,"changes":changes}),
    ))
}
async fn preview_decisions(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ImportBody>,
) -> ApiResult {
    Ok(Json(
        decisions_preview(&st.store(), &id, &body, &st.actor)?.1,
    ))
}
async fn import_decisions(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ImportBody>,
) -> ApiResult {
    let store = st.store();
    let (rows, preview) = decisions_preview(&store, &id, &body, &st.actor)?;
    check_digest(
        body.expected_digest.as_deref(),
        preview["digest"].as_str().unwrap(),
    )?;
    if preview["replacements"].as_u64().unwrap_or(0) > 0 && !body.replace_existing {
        return Err(invalid(
            "Explicitly allow replacement of existing decisions before importing.",
        )
        .into());
    }
    let result = store.record_decisions(&id, &rows, &st.actor)?;
    Ok(Json(json!({"count":result.len()})))
}

async fn export_writing(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().export_writing_json()?)))
}
async fn preview_writing(State(st): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    let doc: cs_store::WritingJson = parse(&body.raw)?;
    Ok(Json(st.store().preview_writing_json(&doc, &st.actor)?))
}
async fn import_writing(State(st): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    let doc: cs_store::WritingJson = parse(&body.raw)?;
    let n = st.store().import_writing_json(
        &doc,
        body.expected_digest.as_deref().unwrap_or(""),
        body.replace_existing,
        &st.actor,
    )?;
    Ok(Json(json!({"count":n})))
}
