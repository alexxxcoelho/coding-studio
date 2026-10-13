//! V1 lot 5 routes: themes, matrix, typed links, write-up, final audit,
//! documented decisions, and the API gaps reported by the front-end. Thin
//! adapters over `cs-store`; the rules live there. JSON shapes follow the
//! client of the Analyser and Rédiger screens (`web/src/lib/analysis-api.ts`).
//! Documented in `docs/API.md`, section "V1 lot 5 and gaps".

use std::path::PathBuf;

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cs_model::analysis::LinkTarget;
use cs_model::Case;
use cs_store::{
    CellPatch, FrameworkReading, MatrixAxis, MatrixConfig, MatrixFilter, NewCode, NewTheme,
    PartEdit, PassageQualify, ThemePatch, ThemeView,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{export, ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        // ---- gaps
        .route("/api/codes", post(create_code))
        .route("/api/codes/{id}/memo", get(code_memo).put(set_code_memo))
        .route("/api/codes/{id}/undo-create", post(undo_create))
        .route("/api/passages/{pid}/qualification", put(qualify_passage))
        .route(
            "/api/interviews/{id}/units/{unit}/mark",
            put(mark_unit).delete(unmark_unit),
        )
        .route("/api/interviews/{id}/marks", get(unit_marks))
        .route("/api/library/progress", get(library_progress))
        // ---- themes
        .route("/api/themes", get(list_themes).post(create_theme))
        .route("/api/themes/order", put(reorder_themes))
        .route(
            "/api/themes/{id}",
            get(get_theme).put(update_theme).delete(delete_theme),
        )
        .route("/api/themes/{id}/status", post(set_status))
        .route("/api/themes/{id}/checks/{check}", put(set_check))
        .route("/api/themes/{id}/codes", post(add_theme_code))
        .route("/api/themes/{id}/codes/{code}", delete(remove_theme_code))
        .route("/api/themes/{id}/passages", get(theme_passages))
        .route("/api/themes/{id}/history", get(theme_history))
        .route(
            "/api/themes/{id}/framework",
            get(get_framework).put(put_framework),
        )
        // ---- matrix
        .route("/api/matrix", post(matrix))
        .route("/api/matrix/compute", post(matrix_native))
        .route("/api/matrix/options", get(matrix_options))
        .route("/api/matrix/views", get(matrix_views).post(save_view))
        .route("/api/matrix/views/{id}", delete(delete_view))
        .route("/api/matrix/cell", get(get_cell).put(put_cell))
        .route("/api/matrix/cell/passages", post(attach_passage))
        .route("/api/matrix/cell/passages/{pid}", delete(detach_passage))
        // ---- links
        .route("/api/links", get(list_links).post(create_link))
        .route("/api/links/{id}", put(update_link).delete(remove_link))
        // ---- writing
        .route("/api/writing/themes/{id}", get(theme_writing))
        .route("/api/writing/themes/{id}/parts/{part}", put(put_part))
        .route(
            "/api/writing/questions/{rq}",
            get(get_question).put(put_question),
        )
        .route("/api/writing/framework", get(framework_table))
        .route("/api/writing/export", post(export_writeup))
        // ---- final audit
        .route("/api/final-audit", get(list_audits).post(draw_audit))
        .route("/api/final-audit/latest", get(latest_audit))
        .route("/api/final-audit/{id}", get(get_audit))
        .route("/api/final-audit/{id}/verdicts", post(post_verdict))
        // ---- decisions (the documented decisions, in the words of the guide)
        .route("/api/decisions", get(list_decisions))
        .route("/api/decisions/{id}", get(get_decision).put(put_decision))
        .route(
            "/api/interviews/{id}/decisions",
            get(interview_decisions).post(add_decision),
        )
}

// ------------------------------------------------------------------- gaps

async fn create_code(State(st): State<AppState>, Json(req): Json<NewCode>) -> ApiResult {
    Ok(Json(json!(st.store().create_code_v1(&req, &st.actor)?)))
}

async fn code_memo(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let code = store.code_node(&id)?;
    Ok(Json(json!({
        "code_id": code.code_id,
        "memo": code.memo,
        "history": store.code_memo_history(&id)?,
    })))
}

#[derive(Deserialize)]
struct MemoBody {
    text: String,
}

async fn set_code_memo(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<MemoBody>,
) -> ApiResult {
    let store = st.store();
    let code = store.set_code_memo(&id, &body.text, &st.actor)?;
    Ok(Json(json!({
        "code_id": code.code_id,
        "memo": code.memo,
        "history": store.code_memo_history(&id)?,
    })))
}

async fn undo_create(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .undo_create_code_from_passage(&id, &st.actor)?)))
}

async fn qualify_passage(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Json(body): Json<PassageQualify>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .qualify_passage(&pid, &body, &st.actor)?)))
}

#[derive(Deserialize, Default)]
struct MarkBody {
    #[serde(default)]
    note: String,
}

async fn mark_unit(
    State(st): State<AppState>,
    UrlPath((id, unit)): UrlPath<(String, String)>,
    body: Option<Json<MarkBody>>,
) -> ApiResult {
    let note = body.map(|b| b.0.note).unwrap_or_default();
    Ok(Json(json!(st
        .store()
        .set_unit_mark(&id, &unit, &note, &st.actor)?)))
}

async fn unmark_unit(
    State(st): State<AppState>,
    UrlPath((id, unit)): UrlPath<(String, String)>,
) -> ApiResult {
    let cleared = st.store().clear_unit_mark(&id, &unit, &st.actor)?;
    Ok(Json(json!({"unit_id": unit, "cleared": cleared})))
}

async fn unit_marks(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().unit_marks(&id)?)))
}

async fn library_progress(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().library_progress()?)))
}

// ----------------------------------------------------------------- themes

/// A theme as the screens read it: the store's view with `ord`, the counts
/// the list needs and the checks keyed by `check_id`.
fn theme_json(t: &ThemeView) -> Value {
    let mut v = serde_json::to_value(t).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("ord".into(), json!(t.order));
        o.insert("codes_count".into(), json!(t.codes.len()));
        o.insert("interviews_count".into(), json!(t.counts.interviews));
        o.insert(
            "checks".into(),
            json!(t
                .checks
                .iter()
                .map(|c| json!({
                    "check_id": c.id, "id": c.id, "label": c.label,
                    "done": c.done, "note": c.note, "at": c.at, "by": c.by,
                }))
                .collect::<Vec<_>>()),
        );
    }
    v
}

fn summary_json(t: &ThemeView) -> Value {
    json!({
        "theme_id": t.theme_id, "title": t.title, "status": t.status,
        "status_label": t.status_label,
        "ord": t.order, "codes_count": t.codes.len(),
        "interviews_count": t.counts.interviews, "passages_count": t.counts.passages,
        "research_questions": t.research_questions,
        "checks_done": t.checks_done, "checks_total": t.checks_total,
    })
}

#[derive(Deserialize, Default)]
struct ThemesQuery {
    #[serde(default)]
    status: Option<String>,
}

async fn list_themes(State(st): State<AppState>, Query(q): Query<ThemesQuery>) -> ApiResult {
    let themes = st.store().themes(q.status.as_deref())?;
    Ok(Json(json!(themes
        .iter()
        .map(summary_json)
        .collect::<Vec<_>>())))
}

async fn create_theme(State(st): State<AppState>, Json(req): Json<NewTheme>) -> ApiResult {
    Ok(Json(theme_json(&st.store().create_theme(&req, &st.actor)?)))
}

async fn get_theme(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(theme_json(&st.store().theme(&id)?)))
}

#[derive(Deserialize, Default)]
struct ThemeEditBody {
    #[serde(flatten)]
    patch: ThemePatch,
    #[serde(default)]
    research_questions: Option<Vec<String>>,
}

async fn update_theme(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<ThemeEditBody>,
) -> ApiResult {
    let store = st.store();
    let mut t = store.update_theme(&id, &body.patch, &st.actor)?;
    if let Some(q) = &body.research_questions {
        t = store.set_theme_questions(&id, q, &st.actor)?;
    }
    Ok(Json(theme_json(&t)))
}

#[derive(Deserialize, Default)]
struct ReasonQuery {
    #[serde(default)]
    reason: String,
}

async fn delete_theme(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<ReasonQuery>,
) -> ApiResult {
    st.store().delete_theme(&id, &q.reason, &st.actor)?;
    Ok(Json(json!({"deleted": id})))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    override_reason: Option<String>,
}

/// 409 when the target is "retained", a check is not done and there is no
/// `override_reason`.
async fn set_status(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(b): Json<StatusBody>,
) -> ApiResult {
    Ok(Json(theme_json(&st.store().set_theme_status(
        &id,
        &b.status,
        &b.reason,
        b.override_reason.as_deref(),
        &st.actor,
    )?)))
}

#[derive(Deserialize)]
struct CheckBody {
    done: bool,
    #[serde(default)]
    note: String,
}

async fn set_check(
    State(st): State<AppState>,
    UrlPath((id, check)): UrlPath<(String, String)>,
    Json(b): Json<CheckBody>,
) -> ApiResult {
    Ok(Json(theme_json(&st.store().set_theme_check(
        &id, &check, b.done, &b.note, &st.actor,
    )?)))
}

#[derive(Deserialize)]
struct CodeBody {
    code_id: String,
}

async fn add_theme_code(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(b): Json<CodeBody>,
) -> ApiResult {
    Ok(Json(theme_json(
        &st.store().add_theme_code(&id, &b.code_id, &st.actor)?,
    )))
}

async fn remove_theme_code(
    State(st): State<AppState>,
    UrlPath((id, code)): UrlPath<(String, String)>,
) -> ApiResult {
    Ok(Json(theme_json(
        &st.store().remove_theme_code(&id, &code, &st.actor)?,
    )))
}

async fn theme_passages(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().theme_passages(&id)?)))
}

async fn theme_history(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().theme_history(&id)?)))
}

#[derive(Deserialize)]
struct OrderBody {
    ids: Vec<String>,
}

async fn reorder_themes(State(st): State<AppState>, Json(b): Json<OrderBody>) -> ApiResult {
    let themes = st.store().reorder_themes(&b.ids, &st.actor)?;
    Ok(Json(json!(themes
        .iter()
        .map(summary_json)
        .collect::<Vec<_>>())))
}

async fn get_framework(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().framework_reading(&id)?)))
}

async fn put_framework(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(b): Json<FrameworkReading>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .set_framework_reading(&id, &b, &st.actor)?)))
}

async fn framework_table(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().framework_table()?)))
}

// ----------------------------------------------------------------- matrix

/// One side of the matrix as the screens send it.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct AxisSource {
    kind: String,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ClientFilter {
    attribute: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ClientConfig {
    rows: AxisSource,
    cols: AxisSource,
    #[serde(default)]
    filter: Option<ClientFilter>,
}

fn native_axis(a: &AxisSource) -> MatrixAxis {
    match a.kind.as_str() {
        "attribute" => MatrixAxis {
            kind: "attribute".into(),
            attribute: a.id.clone(),
            ..MatrixAxis::default()
        },
        "dimension" | "dimensions" => MatrixAxis {
            kind: "dimensions".into(),
            ids: a.id.iter().cloned().collect(),
            ..MatrixAxis::default()
        },
        other => MatrixAxis {
            kind: other.to_string(),
            ids: a.id.iter().cloned().collect(),
            ..MatrixAxis::default()
        },
    }
}

fn client_axis(a: Option<&MatrixAxis>) -> AxisSource {
    match a {
        None => AxisSource {
            kind: "interviews".into(),
            id: None,
        },
        Some(a) if a.kind == "attribute" => AxisSource {
            kind: "attribute".into(),
            id: a.attribute.clone(),
        },
        Some(a) if a.kind == "dimensions" => AxisSource {
            kind: "dimension".into(),
            id: a.ids.first().cloned(),
        },
        Some(a) => AxisSource {
            kind: a.kind.clone(),
            id: None,
        },
    }
}

fn native_config(c: &ClientConfig) -> MatrixConfig {
    MatrixConfig {
        rows: vec![native_axis(&c.rows)],
        cols: vec![native_axis(&c.cols)],
        filters: c
            .filter
            .iter()
            .map(|f| MatrixFilter {
                attribute: f.attribute.clone(),
                values: vec![f.value.clone()],
            })
            .collect(),
    }
}

fn client_config(c: &MatrixConfig) -> ClientConfig {
    ClientConfig {
        rows: client_axis(c.rows.first()),
        cols: client_axis(c.cols.first()),
        filter: c.filters.first().and_then(|f| {
            f.values.first().map(|v| ClientFilter {
                attribute: f.attribute.clone(),
                value: v.clone(),
            })
        }),
    }
}

/// The body is `{config}` or the config itself.
fn config_of(body: Value) -> Result<ClientConfig, crate::error::ApiError> {
    let v = body.get("config").cloned().unwrap_or(body);
    serde_json::from_value(v).map_err(|e| {
        crate::error::ApiError::from(anyhow::Error::new(cs_model::CsError::Validation(format!(
            "matrix configuration: {e}"
        ))))
    })
}

fn matrix_json(m: &cs_store::Matrix) -> Value {
    let cells: Vec<Value> = m
        .cells
        .iter()
        .flatten()
        .map(|c| {
            json!({
                "row_key": c.row_key, "col_key": c.col_key, "value": c.value,
                "value_label": c.value_label, "summary": c.summary,
                "n_passages": c.passages, "n_interviews": c.interviews,
                "manual": c.manual,
            })
        })
        .collect();
    json!({
        "rows": m.rows, "cols": m.cols, "cells": cells,
        "interviews_in_scope": m.interviews_in_scope,
    })
}

async fn matrix(State(st): State<AppState>, Json(body): Json<Value>) -> ApiResult {
    let cfg = native_config(&config_of(body)?);
    Ok(Json(matrix_json(&st.store().compute_matrix(&cfg)?)))
}

/// The matrix of any native configuration (several axes, several filters).
async fn matrix_native(State(st): State<AppState>, Json(cfg): Json<MatrixConfig>) -> ApiResult {
    Ok(Json(json!(st.store().compute_matrix(&cfg)?)))
}

async fn matrix_options(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().matrix_options()?)))
}

fn view_json(v: &cs_store::MatrixViewRecord) -> Value {
    json!({
        "view_id": v.view_id, "name": v.name,
        "config": client_config(&v.config), "native_config": v.config,
        "created_at": v.created_at, "updated_at": v.updated_at,
    })
}

async fn matrix_views(State(st): State<AppState>) -> ApiResult {
    let views = st.store().matrix_views()?;
    Ok(Json(json!(views.iter().map(view_json).collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct SaveViewBody {
    name: String,
    config: Value,
}

async fn save_view(State(st): State<AppState>, Json(b): Json<SaveViewBody>) -> ApiResult {
    // The client sends its single-axis form; a native configuration (with
    // `rows` as a list) is accepted too.
    let native = if b.config.get("rows").is_some_and(Value::is_array) {
        serde_json::from_value::<MatrixConfig>(b.config).map_err(|e| {
            crate::error::ApiError::from(anyhow::Error::new(cs_model::CsError::Validation(
                format!("matrix configuration: {e}"),
            )))
        })?
    } else {
        native_config(&config_of(b.config)?)
    };
    let v = st.store().create_matrix_view(&b.name, &native, &st.actor)?;
    Ok(Json(view_json(&v)))
}

async fn delete_view(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    st.store().delete_matrix_view(&id, &st.actor)?;
    Ok(Json(json!({"deleted": id})))
}

#[derive(Deserialize)]
struct CellQuery {
    row: String,
    col: String,
    #[serde(default)]
    attribute: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

fn filters_of(q: &CellQuery) -> Vec<MatrixFilter> {
    match (&q.attribute, &q.value) {
        (Some(a), Some(v)) => vec![MatrixFilter {
            attribute: a.clone(),
            values: vec![v.clone()],
        }],
        _ => Vec::new(),
    }
}

async fn get_cell(State(st): State<AppState>, Query(q): Query<CellQuery>) -> ApiResult {
    Ok(Json(json!(st.store().matrix_cell_detail(
        &q.row,
        &q.col,
        &filters_of(&q)
    )?)))
}

async fn put_cell(State(st): State<AppState>, Json(b): Json<CellPatch>) -> ApiResult {
    let store = st.store();
    store.set_matrix_cell(&b, &st.actor)?;
    Ok(Json(json!(store.matrix_cell_detail(
        &b.row_key,
        &b.col_key,
        &[]
    )?)))
}

#[derive(Deserialize)]
struct AttachBody {
    row_key: String,
    col_key: String,
    passage_id: String,
}

async fn attach_passage(State(st): State<AppState>, Json(b): Json<AttachBody>) -> ApiResult {
    let store = st.store();
    store.link_cell_passage(&b.row_key, &b.col_key, &b.passage_id, &st.actor)?;
    Ok(Json(json!(store.matrix_cell_detail(
        &b.row_key,
        &b.col_key,
        &[]
    )?)))
}

#[derive(Deserialize)]
struct DetachQuery {
    row: String,
    col: String,
}

async fn detach_passage(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Query(q): Query<DetachQuery>,
) -> ApiResult {
    let store = st.store();
    store.unlink_cell_passage(&q.row, &q.col, &pid, &st.actor)?;
    Ok(Json(json!(store.matrix_cell_detail(
        &q.row,
        &q.col,
        &[]
    )?)))
}

// ------------------------------------------------------------------ links

async fn list_links(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().links()?)))
}

#[derive(Deserialize)]
struct LinkBody {
    passage_id: String,
    target: LinkTarget,
    link_type: String,
    #[serde(default)]
    note: String,
}

async fn create_link(State(st): State<AppState>, Json(b): Json<LinkBody>) -> ApiResult {
    Ok(Json(json!(st.store().create_link(
        &b.passage_id,
        &b.target,
        &b.link_type,
        &b.note,
        &st.actor
    )?)))
}

#[derive(Deserialize)]
struct LinkEdit {
    #[serde(default)]
    link_type: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

async fn update_link(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(b): Json<LinkEdit>,
) -> ApiResult {
    Ok(Json(json!(st.store().update_link(
        &id,
        b.link_type.as_deref(),
        b.note.as_deref(),
        &st.actor
    )?)))
}

async fn remove_link(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<ReasonQuery>,
) -> ApiResult {
    st.store().remove_link(&id, &q.reason, &st.actor)?;
    Ok(Json(json!({"removed": id})))
}

// ---------------------------------------------------------------- writing

async fn theme_writing(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().theme_writing(&id)?)))
}

async fn put_part(
    State(st): State<AppState>,
    UrlPath((id, part)): UrlPath<(String, String)>,
    Json(b): Json<PartEdit>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .set_theme_part(&id, &part, &b, &st.actor)?)))
}

async fn get_question(State(st): State<AppState>, UrlPath(rq): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().question_synthesis(&rq)?)))
}

#[derive(Deserialize)]
struct SynthesisBody {
    #[serde(default)]
    answer: String,
    #[serde(default)]
    limits: String,
}

async fn put_question(
    State(st): State<AppState>,
    UrlPath(rq): UrlPath<String>,
    Json(b): Json<SynthesisBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().set_question_synthesis(
        &rq, &b.answer, &b.limits, &st.actor
    )?)))
}

/// Writes the write-up package and returns its folder, files and the Markdown.
async fn export_writeup(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let base = st
        .db_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("exports");
    let r = export::write_writeup(&store, &base, &st.db_path, &st.actor)?;
    Ok(Json(json!(r)))
}

// ------------------------------------------------------------ final audit

async fn latest_audit(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().latest_final_audit()?)))
}

async fn list_audits(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let mut out = Vec::new();
    for id in store.final_audit_ids()? {
        let a = store.final_audit(id)?;
        out.push(json!({
            "audit_id": a.audit_id, "seed": a.seed, "n": a.n,
            "created_at": a.created_at, "created_by": a.created_by,
        }));
    }
    Ok(Json(json!(out)))
}

#[derive(Deserialize, Default)]
struct DrawBody {
    #[serde(default)]
    n: Option<usize>,
    #[serde(default)]
    seed: Option<String>,
}

async fn draw_audit(State(st): State<AppState>, body: Option<Json<DrawBody>>) -> ApiResult {
    let b = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(json!(st.store().draw_final_audit(
        b.n.unwrap_or(3),
        b.seed.as_deref(),
        &st.actor
    )?)))
}

async fn get_audit(State(st): State<AppState>, UrlPath(id): UrlPath<i64>) -> ApiResult {
    Ok(Json(json!(st.store().final_audit(id)?)))
}

#[derive(Deserialize)]
struct VerdictBody {
    claim_key: String,
    verdict: String,
    #[serde(default)]
    note: String,
}

async fn post_verdict(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    Json(b): Json<VerdictBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().set_audit_verdict(
        id,
        &b.claim_key,
        &b.verdict,
        &b.note,
        &st.actor
    )?)))
}

// -------------------------------------------------------------- decisions

/// A decision as the guide words it: `name` beside `label`.
fn decision_json(c: &Case) -> Value {
    let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("decision_id".into(), json!(c.case_id));
        o.insert("name".into(), json!(c.label));
        o.insert(
            "outcome_label".into(),
            json!(c
                .outcome
                .as_deref()
                .map(cs_model::analysis::decision_outcome_label)),
        );
    }
    v
}

async fn list_decisions(State(st): State<AppState>) -> ApiResult {
    let cases = st.store().cases(None)?;
    Ok(Json(json!(cases
        .iter()
        .map(decision_json)
        .collect::<Vec<_>>())))
}

async fn interview_decisions(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult {
    let store = st.store();
    if store.interview(&id)?.is_none() {
        return Err(crate::error::ApiError::from(anyhow::Error::new(
            cs_model::CsError::NotFound(format!("interview {id}")),
        )));
    }
    let cases = store.cases(Some(&id))?;
    Ok(Json(json!(cases
        .iter()
        .map(decision_json)
        .collect::<Vec<_>>())))
}

async fn get_decision(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(decision_json(&st.store().case(&id)?)))
}

/// The body of a decision. A decision read from this API carries `name` and
/// `label` together; when both are there `name` is dropped, so that a read
/// can be written back (`name` is an alias of `label` for serde).
fn case_of(mut body: Value) -> Result<Case, crate::error::ApiError> {
    if let Some(o) = body.as_object_mut() {
        if o.contains_key("label") {
            o.remove("name");
        }
    }
    serde_json::from_value(body).map_err(|e| {
        crate::error::ApiError::from(anyhow::Error::new(cs_model::CsError::Validation(format!(
            "decision: {e}"
        ))))
    })
}

async fn add_decision(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    let case = case_of(body)?;
    Ok(Json(decision_json(
        &st.store().create_case(&id, &case, &st.actor)?,
    )))
}

async fn put_decision(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    let case = case_of(body)?;
    Ok(Json(decision_json(
        &st.store().update_case(&id, &case, &st.actor)?,
    )))
}
