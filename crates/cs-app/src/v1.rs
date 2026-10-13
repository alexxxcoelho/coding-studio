//! V1 lot 1 routes: stable code ids and the code tree, passages and their
//! codings, highlights, the LATER marker. Thin adapters over `cs-store`; the
//! rules live there. Documented in `docs/API.md`, section "V1 lots 1 and 2".

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use cs_model::passage::OffsetUnit;
use cs_store::{CodeUpdate, NewCodeFromPassage, NewPassage, PassageFilter};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/codes", get(list_codes))
        .route("/api/codes/from-passage", post(create_from_passage))
        .route("/api/codes/{id}", get(get_code).put(update_code))
        .route("/api/codes/{id}/move", post(move_code))
        .route("/api/codes/{id}/aliases", post(add_alias))
        .route("/api/codes/{id}/aliases/{alias}", delete(remove_alias))
        .route("/api/codes/{id}/merge", post(merge_code))
        .route("/api/codes/{id}/active", post(set_active))
        .route("/api/codes/{id}/passages", get(code_passages))
        .route("/api/codebook/revisions", get(revisions))
        .route(
            "/api/interviews/{id}/passages",
            get(list_passages).post(add_passage),
        )
        .route("/api/interviews/{id}/highlights", get(highlights))
        .route("/api/interviews/{id}/later", get(interview_later))
        .route("/api/later", get(later))
        .route(
            "/api/passages/{pid}",
            get(get_passage).delete(delete_passage),
        )
        .route("/api/passages/{pid}/codings", post(add_coding))
        .route("/api/passages/{pid}/codings/{code}", delete(remove_coding))
}

// ------------------------------------------------------------------ codes

#[derive(Deserialize, Default)]
struct CodesQuery {
    /// Count only the interviews whose attribute has this value.
    #[serde(default)]
    attribute: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

/// Every code with aliases and counts, then the reserved code "Plus tard".
/// With `?attribute=&value=` the counts cover only the interviews whose
/// attribute has that value ("codes by attribute").
async fn list_codes(State(st): State<AppState>, Query(q): Query<CodesQuery>) -> ApiResult {
    let store = st.store();
    let only = match (q.attribute.as_deref(), q.value.as_deref()) {
        (Some(a), Some(v)) => Some(store.interviews_with_attribute(a, v)?),
        (None, None) => None,
        _ => {
            return Err(anyhow::Error::new(cs_model::CsError::Validation(
                "attribute and value go together".into(),
            ))
            .into())
        }
    };
    Ok(Json(json!({
        "codebook_version": store.codebook_version()?,
        "filter": only.as_ref().map(|s| json!({
            "attribute": q.attribute, "value": q.value, "interviews": s,
        })),
        "codes": store.code_nodes_filtered(only.as_ref())?,
    })))
}

async fn get_code(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().code_node(&id)?)))
}

async fn update_code(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(patch): Json<CodeUpdate>,
) -> ApiResult {
    let node = st.store().update_code(&id, &patch, &st.actor)?;
    Ok(Json(json!(node)))
}

/// Creates a code from a passage (an existing `passage_id`, or a selection
/// in `passage`) and codes the passage with it. Body: [`NewCodeFromPassage`].
async fn create_from_passage(
    State(st): State<AppState>,
    Json(body): Json<NewCodeFromPassage>,
) -> ApiResult {
    let store = st.store();
    let (code, passage) = store.create_code_from_passage(&body, &st.actor)?;
    Ok(Json(json!({
        "code": code,
        "passage": passage,
        "codebook_version": store.codebook_version()?,
    })))
}

#[derive(Deserialize)]
struct MoveBody {
    #[serde(default)]
    parent_id: Option<String>,
    rationale: String,
}

async fn move_code(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<MoveBody>,
) -> ApiResult {
    let node = st
        .store()
        .move_code(&id, body.parent_id.as_deref(), &body.rationale, &st.actor)?;
    Ok(Json(json!(node)))
}

#[derive(Deserialize)]
struct AliasBody {
    alias: String,
}

async fn add_alias(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<AliasBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().add_alias(
        &id,
        &body.alias,
        &st.actor
    )?)))
}

async fn remove_alias(
    State(st): State<AppState>,
    UrlPath((id, alias)): UrlPath<(String, String)>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .remove_alias(&id, &alias, &st.actor)?)))
}

#[derive(Deserialize)]
struct MergeBody {
    into: String,
    rationale: String,
}

async fn merge_code(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<MergeBody>,
) -> ApiResult {
    let store = st.store();
    let from = store.code_node(&id)?;
    let into = store.code_node(&body.into)?;
    let outcome = store.merge_code(&from.token, &into.token, &body.rationale, &st.actor)?;
    Ok(Json(json!({
        "outcome": outcome,
        "merged": store.code_node(&from.code_id)?,
        "code": store.code_node(&into.code_id)?,
    })))
}

#[derive(Deserialize)]
struct ActiveBody {
    active: bool,
}

async fn set_active(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<ActiveBody>,
) -> ApiResult {
    let store = st.store();
    let node = store.code_node(&id)?;
    store.set_code_active(&node.token, body.active, &st.actor)?;
    Ok(Json(json!(store.code_node(&node.code_id)?)))
}

#[derive(Deserialize, Default)]
struct DescendantsQuery {
    #[serde(default)]
    descendants: bool,
}

/// Passages of a code with the previous and next turn and the other codes.
async fn code_passages(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<DescendantsQuery>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .passages_of_code(&id, q.descendants)?)))
}

/// The codebook version and its revisions (every change, with its reason).
async fn revisions(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    Ok(Json(json!({
        "codebook_version": store.codebook_version()?,
        "revisions": store.codebook_revisions()?,
    })))
}

// --------------------------------------------------------------- passages

#[derive(Deserialize, Default)]
struct PassageQuery {
    #[serde(default)]
    unit_id: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    kind: Option<String>,
}

async fn list_passages(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<PassageQuery>,
) -> ApiResult {
    let list = st.store().passages(&PassageFilter {
        interview_id: Some(id),
        unit_id: q.unit_id,
        code: q.code,
        kind: q.kind,
    })?;
    Ok(Json(json!(list)))
}

/// Body of `POST /api/interviews/{id}/passages`. Without `start` and `end` the
/// whole unit is selected. Without `code` the passage is a highlight.
#[derive(Deserialize)]
struct PassageBody {
    unit_id: String,
    #[serde(default)]
    start: Option<usize>,
    #[serde(default)]
    end: Option<usize>,
    #[serde(default)]
    offset_unit: OffsetUnit,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

async fn add_passage(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<PassageBody>,
) -> ApiResult {
    let req = NewPassage {
        interview_id: id,
        unit_id: body.unit_id,
        start: body.start,
        end: body.end,
        offset_unit: body.offset_unit,
        text: body.text,
    };
    let store = st.store();
    let (passage, created) = match body.code.as_deref().filter(|c| !c.trim().is_empty()) {
        Some(code) => store.code_span(&req, code, body.note.as_deref().unwrap_or(""), &st.actor)?,
        None => store.create_passage(&req, &st.actor)?,
    };
    Ok(Json(json!({ "passage": passage, "created": created })))
}

async fn highlights(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().highlights(Some(&id))?)))
}

async fn interview_later(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    store.passages(&PassageFilter {
        interview_id: Some(id.clone()),
        ..Default::default()
    })?;
    Ok(Json(json!(store.later_items(Some(&id))?)))
}

/// Parked passages of the whole corpus with the count per interview.
async fn later(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let summary: Vec<Value> = store
        .later_summary()?
        .into_iter()
        .map(|(interview_id, n)| json!({ "interview_id": interview_id, "n": n }))
        .collect();
    Ok(Json(
        json!({ "summary": summary, "items": store.later_items(None)? }),
    ))
}

async fn get_passage(State(st): State<AppState>, UrlPath(pid): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().passage(&pid)?)))
}

#[derive(Deserialize, Default)]
struct ReasonQuery {
    #[serde(default)]
    reason: String,
}

async fn delete_passage(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Query(q): Query<ReasonQuery>,
) -> ApiResult {
    st.store().delete_passage(&pid, &q.reason, &st.actor)?;
    Ok(Json(json!({ "deleted": pid })))
}

#[derive(Deserialize)]
struct CodingBody {
    code: String,
    #[serde(default)]
    note: Option<String>,
}

async fn add_coding(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Json(body): Json<CodingBody>,
) -> ApiResult {
    let p = st.store().code_passage(
        &pid,
        &body.code,
        body.note.as_deref().unwrap_or(""),
        &st.actor,
    )?;
    Ok(Json(json!(p)))
}

async fn remove_coding(
    State(st): State<AppState>,
    UrlPath((pid, code)): UrlPath<(String, String)>,
    Query(q): Query<ReasonQuery>,
) -> ApiResult {
    let p = st
        .store()
        .uncode_passage(&pid, &code, &q.reason, &st.actor)?;
    Ok(Json(json!(p)))
}
