//! REFI-QDA routes (SPEC F-103): export of the codebook (`.qdc`) and of the
//! project (`.qdpx`), import of a codebook and of the codes and text codings of
//! a project. Thin adapters over `cs-qdpx`; the rules live there and are
//! documented in `docs/CONTRACTS.md` (section 11) and `docs/API.md`.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{header, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cs_model::ValidationIssue;
use cs_qdpx::{ExportOptions, Mode};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiError, ApiResult, AppState};

/// Largest file accepted on import, in bytes.
const MAX_UPLOAD: usize = 256 * 1024 * 1024;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/export/qdpx", get(export_qdpx))
        .route("/api/export/qdc", get(export_qdc))
        .route(
            "/api/import/qdc",
            post(import_qdc).layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
        .route(
            "/api/import/qdpx",
            post(import_qdpx).layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
}

/// Library errors to the usual API errors: a rejected file lists every
/// problem found, in `issues`.
fn api_error(e: cs_qdpx::Error) -> ApiError {
    match e {
        cs_qdpx::Error::Rejected(issues) => ApiError {
            status: axum::http::StatusCode::BAD_REQUEST,
            kind: "validation",
            message: format!("file rejected: {} problem(s) found", issues.len()),
            issues: Some(
                issues
                    .into_iter()
                    .map(|i| ValidationIssue {
                        location: i.location,
                        message: i.message,
                    })
                    .collect(),
            ),
            rejected: None,
        },
        cs_qdpx::Error::Store(e) => ApiError::from(e),
    }
}

fn file_response(out: cs_qdpx::Export, content_type: &'static str) -> Response {
    let disposition = format!("attachment; filename=\"{}\"", out.file_name);
    let mut res = (
        [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
        out.bytes,
    )
        .into_response();
    let h = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Ok(v) = HeaderValue::from_str(&out.summary.sha256) {
        h.insert(HeaderName::from_static("x-export-sha256"), v);
    }
    if let Ok(v) = HeaderValue::from_str(&json!(out.summary).to_string()) {
        h.insert(HeaderName::from_static("x-export-summary"), v);
    }
    res
}

#[derive(Deserialize, Default)]
struct ExportQuery {
    /// Export highlights under the code "Surlignages".
    #[serde(default)]
    highlights: bool,
    /// `name` of the REFI project.
    #[serde(default)]
    name: Option<String>,
}

async fn export_qdpx(
    State(st): State<AppState>,
    Query(q): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let mut opts = ExportOptions {
        include_highlights: q.highlights,
        ..Default::default()
    };
    if let Some(n) = q.name.filter(|n| !n.trim().is_empty()) {
        opts.project_name = n.trim().to_string();
    }
    let out = cs_qdpx::export_qdpx(&st.store(), &opts, &st.actor).map_err(api_error)?;
    Ok(file_response(out, "application/zip"))
}

async fn export_qdc(State(st): State<AppState>) -> Result<Response, ApiError> {
    let out = cs_qdpx::export_qdc(&st.store(), &st.actor).map_err(api_error)?;
    Ok(file_response(out, "application/xml"))
}

#[derive(Deserialize, Default)]
struct ImportQuery {
    /// False (default): dry run, nothing is written. True: apply.
    #[serde(default)]
    apply: bool,
}

fn mode(q: &ImportQuery) -> Mode {
    if q.apply {
        Mode::Apply
    } else {
        Mode::DryRun
    }
}

/// The body is the file itself (`application/xml`, `.qdc`).
async fn import_qdc(
    State(st): State<AppState>,
    Query(q): Query<ImportQuery>,
    body: Bytes,
) -> ApiResult {
    let report = cs_qdpx::import_qdc(&st.store(), &body, mode(&q), &st.actor).map_err(api_error)?;
    Ok(Json(json!(report)))
}

/// The body is the file itself (`application/zip`, `.qdpx`).
async fn import_qdpx(
    State(st): State<AppState>,
    Query(q): Query<ImportQuery>,
    body: Bytes,
) -> ApiResult {
    let report =
        cs_qdpx::import_qdpx(&st.store(), &body, mode(&q), &st.actor).map_err(api_error)?;
    Ok(Json(json!(report)))
}
