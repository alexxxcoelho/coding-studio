//! V1 lot 2 routes: project configuration, interview attributes, memos,
//! end-of-interview checklist, project template. Thin adapters over
//! `cs-store`. Documented in `docs/API.md`, section "V1 lots 1 and 2".

use std::collections::BTreeMap;

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cs_model::project::{framework_preset, ProjectConfig, FRAMEWORK_PRESETS};
use cs_store::MemoOverrides;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/project-config", get(get_config).put(put_config))
        .route("/api/project-config/presets", get(presets))
        .route("/api/project/template", get(export_template))
        .route("/api/project/template/import", post(import_template))
        .route(
            "/api/interviews/{id}/attributes",
            get(get_attributes).put(put_attributes),
        )
        .route("/api/interviews/{id}/memo", get(get_memo))
        .route("/api/interviews/{id}/memo/template", put(put_memo_template))
        .route(
            "/api/interviews/{id}/memo/overrides",
            put(put_memo_overrides),
        )
        .route("/api/interviews/{id}/memo/answers", put(put_memo_answers))
        .route(
            "/api/interviews/{id}/checklist",
            get(get_checklist).put(put_checklist),
        )
}

async fn get_config(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().project_config()?)))
}

async fn put_config(State(st): State<AppState>, Json(cfg): Json<ProjectConfig>) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .set_project_config(&cfg, &st.actor)?)))
}

/// The interpretive framework presets, to fill the Settings form.
async fn presets() -> ApiResult {
    let frameworks: BTreeMap<&str, _> = FRAMEWORK_PRESETS
        .iter()
        .filter_map(|p| framework_preset(p).map(|f| (*p, f)))
        .collect();
    Ok(Json(json!({
        "frameworks": frameworks,
        "defaults": ProjectConfig::default(),
    })))
}

async fn export_template(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().export_template()?)))
}

#[derive(Deserialize)]
struct ImportBody {
    template: Value,
    #[serde(default)]
    dry_run: bool,
}

/// 200 with the report whether or not the template could be applied
/// (`applied`, `problems`). 400 when the body is not a template at all.
async fn import_template(State(st): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    let report = st
        .store()
        .import_template(&body.template, body.dry_run, &st.actor)?;
    Ok(Json(json!(report)))
}

async fn get_attributes(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().interview_attributes(&id)?)))
}

#[derive(Deserialize)]
struct AttributesBody {
    values: BTreeMap<String, Option<String>>,
}

async fn put_attributes(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<AttributesBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().set_interview_attributes(
        &id,
        &body.values,
        &st.actor
    )?)))
}

async fn get_memo(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().interview_memo(&id)?)))
}

#[derive(Deserialize)]
struct TemplateBody {
    template_id: String,
}

async fn put_memo_template(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<TemplateBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().set_memo_template(
        &id,
        &body.template_id,
        &st.actor
    )?)))
}

async fn put_memo_overrides(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<MemoOverrides>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .set_memo_overrides(&id, &body, &st.actor)?)))
}

#[derive(Deserialize)]
struct AnswersBody {
    answers: BTreeMap<String, String>,
}

async fn put_memo_answers(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<AnswersBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().answer_memo(
        &id,
        &body.answers,
        &st.actor
    )?)))
}

async fn get_checklist(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().interview_checklist(&id)?)))
}

#[derive(Deserialize)]
struct ChecklistBody {
    #[serde(default)]
    checked: BTreeMap<String, bool>,
    #[serde(default)]
    note: Option<String>,
}

async fn put_checklist(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<ChecklistBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().set_interview_checklist(
        &id,
        &body.checked,
        body.note.as_deref(),
        &st.actor
    )?)))
}
