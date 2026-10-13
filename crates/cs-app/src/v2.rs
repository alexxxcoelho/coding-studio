//! V2 routes: write-up in docx, anonymised export, whole-project bundle,
//! example project. Every route is a thin adapter over the functions of the
//! `export_v2`, `bundle` and `example` modules, which the command line uses
//! too. Documented in `docs/API.md`, section "V2 exports and CLI".

use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{header, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::export_v2::{AnonymiseOptions, Scope};
use crate::{invalid, ApiError, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/writing/export.docx", get(writeup_docx))
        .route("/api/export/anonymised", post(export_anonymised))
        .route("/api/project/bundle", get(export_bundle))
        .route(
            "/api/project/bundle/import",
            post(import_bundle).layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
}

/// Largest bundle accepted on import, in bytes.
const MAX_UPLOAD: usize = 1024 * 1024 * 1024;

/// `GET /api/project/bundle`: the whole project as one `.csproj.zip`.
async fn export_bundle(State(st): State<AppState>) -> Result<Response, ApiError> {
    let out = crate::bundle::export_bundle(&st.store(), &st.actor)?;
    let mut res = attachment(out.bytes, "application/zip", &out.file_name);
    if let Ok(v) = HeaderValue::from_str(&out.sha256) {
        res.headers_mut()
            .insert(HeaderName::from_static("x-export-sha256"), v);
    }
    Ok(res)
}

#[derive(Deserialize)]
struct BundleQuery {
    /// False (default): dry run. True: write.
    #[serde(default)]
    apply: bool,
    /// Path of the new database. Absent: the current database, which must be
    /// empty and is replaced by the project of the bundle.
    #[serde(default)]
    target: Option<String>,
}

/// `POST /api/project/bundle/import?apply=&target=`: the body is the bundle.
async fn import_bundle(
    State(st): State<AppState>,
    Query(q): Query<BundleQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let report = match q.target.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(target) => crate::bundle::import_bundle(&body, Path::new(target), q.apply, &st.actor)?,
        None => {
            let dry = crate::bundle::import_bundle(&body, &st.db_path, false, &st.actor)?;
            if !q.apply || !dry.ok {
                dry
            } else {
                // The running store lets go of the file, the bundle is
                // written over it, and the store is opened again on it.
                let mut guard = st.store.lock().unwrap_or_else(|e| e.into_inner());
                *guard = cs_store::Store::open_in_memory()?;
                let result = crate::bundle::import_bundle(&body, &st.db_path, true, &st.actor);
                *guard = cs_store::Store::open(&st.db_path)?;
                result?
            }
        }
    };
    Ok(Json(json!(report)))
}

/// `<db folder>/exports`, where every package is written.
pub(crate) fn exports_base(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("exports")
}

#[derive(Deserialize)]
struct AnonymisedBody {
    /// `interview`, `cross-case` or `writeup`.
    scope: String,
    #[serde(default)]
    interview_id: Option<String>,
    #[serde(flatten)]
    options: AnonymiseOptions,
}

/// `POST /api/export/anonymised`: a pseudonymised copy of a package.
async fn export_anonymised(
    State(st): State<AppState>,
    Json(body): Json<AnonymisedBody>,
) -> Result<Json<Value>, ApiError> {
    let scope = match body.scope.as_str() {
        "interview" => Scope::Interview(
            body.interview_id
                .filter(|i| !i.trim().is_empty())
                .ok_or_else(|| invalid("scope interview needs an interview_id"))?,
        ),
        "cross-case" => Scope::CrossCase,
        "writeup" => Scope::Writeup,
        other => {
            return Err(invalid(format!(
                "unknown scope {other}; use interview, cross-case or writeup"
            )))
        }
    };
    let store = st.store();
    let r = crate::export_v2::write_anonymised(
        &store,
        &scope,
        &exports_base(&st.db_path),
        &st.db_path,
        &st.actor,
        &body.options,
    )?;
    Ok(Json(json!(r)))
}

fn attachment(bytes: Vec<u8>, content_type: &'static str, file_name: &str) -> Response {
    let mut res = (
        [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
        bytes,
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\"")) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    res
}

pub(crate) const DOCX_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// `GET /api/writing/export.docx`: the write-up as a Word document.
async fn writeup_docx(State(st): State<AppState>) -> Result<Response, ApiError> {
    let doc = st.store().writeup_doc()?;
    let bytes = crate::docx::render(&doc)?;
    Ok(attachment(bytes, DOCX_TYPE, "writeup.docx"))
}
