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
        .route(
            "/api/export/reviewer/open",
            post(crate::reviewer_export::open_package),
        )
        .route(
            "/api/export/reviewer/catalogue",
            get(crate::reviewer_export::get_catalogue),
        )
        .route(
            "/api/export/reviewer",
            post(crate::reviewer_export::generate).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/api/writing/export.docx", get(writeup_docx))
        .route("/api/analysis/export.docx", get(analysis_docx))
        .route("/api/analysis/export", post(analysis_package))
        .route("/api/export/anonymised", post(export_anonymised))
        .route("/api/example/load", post(load_example))
        .route("/api/project/bundle", get(export_bundle))
        .route(
            "/api/project/bundle/import",
            post(import_bundle).layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
}

#[derive(Deserialize, Default)]
struct ExampleBody {
    /// Folder the example transcript is copied to; absent, a temporary one.
    #[serde(default)]
    library: Option<String>,
}

/// `POST /api/example/load`: the example project into an empty database.
async fn load_example(
    State(st): State<AppState>,
    body: Option<Json<ExampleBody>>,
) -> Result<Json<Value>, ApiError> {
    let library = body
        .and_then(|b| b.0.library)
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .map(PathBuf::from);
    let report = crate::example::load(&st, library.as_deref())?;
    Ok(Json(json!(report)))
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
    let store = st.store();
    crate::anonymisation::require_all(&store)?;
    let doc = crate::export_v2::mask_document(&store, &store.writeup_doc()?, &st.actor)?;
    let bytes = crate::docx::render(&doc)?;
    Ok(attachment(bytes, DOCX_TYPE, "writeup.docx"))
}

/// Saved analysis, comparison, codebook and source evidence, without runtime AI.
async fn analysis_docx(State(st): State<AppState>) -> Result<Response, ApiError> {
    crate::anonymisation::require_all(&st.store())?;
    let doc = crate::analysis_report::document(&st.store(), &st.actor)?;
    let bytes = crate::docx::render_report(&doc)?;
    Ok(attachment(bytes, DOCX_TYPE, "analysis-report.docx"))
}

/// Persist a versioned report locally, including a plain-text companion and hashes.
async fn analysis_package(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    crate::anonymisation::require_all(&st.store())?;
    let doc = crate::analysis_report::document(&st.store(), &st.actor)?;
    let bytes = crate::docx::render_report(&doc)?;
    let base = exports_base(&st.db_path);
    std::fs::create_dir_all(&base).map_err(anyhow::Error::from)?;
    let dir = crate::export::target_dir(&base, "analysis-report");
    std::fs::create_dir(&dir).map_err(anyhow::Error::from)?;
    let mut writer = crate::export::Writer {
        dir: dir.clone(),
        files: vec![],
    };
    let evidence = crate::analysis_report::evidence_value(&st.store(), &st.actor)?;
    writer.write(
        "analysis-evidence.json",
        &serde_json::to_vec_pretty(&evidence).map_err(anyhow::Error::from)?,
    )?;
    writer.write("analysis-report.docx", &bytes)?;
    writer.write("analysis-report.md", doc.to_markdown().as_bytes())?;
    let manifest = json!({"kind": "analysis_report", "generator": env!("CARGO_PKG_VERSION"), "runtime_ai": false, "files": writer.files});
    writer.write(
        "manifest.json",
        &serde_json::to_vec_pretty(&manifest).map_err(anyhow::Error::from)?,
    )?;
    st.store().log(
        &st.actor,
        "export.analysis",
        None,
        Some("analysis-report"),
        json!({"dir": dir.display().to_string(), "files": writer.files, "runtime_ai": false}),
    )?;
    Ok(Json(
        json!({"dir": dir.display().to_string(), "files": writer.files}),
    ))
}
