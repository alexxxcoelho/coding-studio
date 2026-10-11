//! Coding Studio local server: JSON API over the store, the ingest and the AI
//! layers, plus the embedded web UI. Every route is a thin adapter; protocol
//! rules live in `cs-store` and `cs-ai`, never here.

mod error;
mod export;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cs_model::{
    AiRunMeta, Arbitration, ArbitrationChoice, Code, CodeSet, CodingMap, CsError, HumanCoding,
    Segmentation,
};
use cs_store::Store;
use serde::Deserialize;
use serde_json::{json, Value};

pub use error::ApiError;

/// Version reported by `/api/state` and written in every export manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

type ApiResult = Result<Json<Value>, ApiError>;

/// Shared state of the server. The SQLite connection is not `Sync`, so the
/// store sits behind a mutex; each request holds it only for its own work.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub db_path: PathBuf,
    pub coding_db: Option<PathBuf>,
    pub actor: String,
    pub interviewer_names: Vec<String>,
    /// Native file dialog, present in the desktop app only. The browser mode
    /// has none and the UI falls back to typed paths.
    pub picker: Option<Picker>,
}

/// What the native dialog should let the user choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickKind {
    Folder,
    File,
}

/// Opens a native dialog and returns the chosen path, `None` when cancelled.
/// Called from a blocking thread, never from the async runtime.
pub type Picker = Arc<dyn Fn(PickKind, &str) -> Option<PathBuf> + Send + Sync>;

/// Start-up settings shared by the browser binary and the desktop app.
#[derive(Debug, Clone)]
pub struct Startup {
    pub db: PathBuf,
    pub library: Option<PathBuf>,
    pub coding_db: Option<PathBuf>,
    pub actor: String,
    /// Empty: `cs_ingest::DEFAULT_INTERVIEWER_NAMES`.
    pub interviewer_names: Vec<String>,
}

/// Opens (or creates) the database, records the library folder and imports
/// the codebook when the local one is still empty.
pub fn prepare(startup: Startup, picker: Option<Picker>) -> anyhow::Result<AppState> {
    use anyhow::Context;
    if let Some(parent) = startup.db.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let store =
        Store::open(&startup.db).with_context(|| format!("opening {}", startup.db.display()))?;
    if let Some(lib) = &startup.library {
        let lib = lib.display().to_string();
        if store.setting("library_root")?.as_deref() != Some(lib.as_str()) {
            store.set_setting("library_root", &lib, &startup.actor)?;
        }
    }
    if let Some(cdb) = startup.coding_db.as_ref().filter(|p| p.is_file()) {
        if store.codebook()?.codes.is_empty() {
            store
                .import_codebook(cdb, &startup.actor)
                .with_context(|| format!("importing the codebook from {}", cdb.display()))?;
        }
    }
    let interviewer_names = if startup.interviewer_names.is_empty() {
        cs_ingest::DEFAULT_INTERVIEWER_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        startup.interviewer_names
    };
    Ok(AppState {
        store: Arc::new(Mutex::new(store)),
        db_path: startup.db,
        coding_db: startup.coding_db,
        actor: startup.actor,
        interviewer_names,
        picker,
    })
}

impl AppState {
    fn store(&self) -> MutexGuard<'_, Store> {
        // A poisoned lock only means an earlier request panicked; the
        // connection itself is still consistent (every write is transactional).
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Builds the router: API under `/api`, the web UI at `/`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/style.css", get(style_css))
        .route("/api/state", get(api_state))
        .route("/api/pick", post(pick))
        .route("/api/library/scan", post(scan_library))
        .route("/api/interviews", get(list_interviews))
        .route("/api/interviews/{id}", get(interview_detail))
        .route("/api/interviews/{id}/file", get(interview_file))
        .route("/api/interviews/{id}/primary", post(set_primary))
        .route("/api/interviews/{id}/segment", post(segment))
        .route("/api/interviews/{id}/units", get(units))
        .route("/api/interviews/{id}/coding/{unit_id}", put(set_coding))
        .route(
            "/api/interviews/{id}/prompts",
            post(generate_prompts).get(list_prompts),
        )
        .route("/api/interviews/{id}/runs", post(create_run))
        .route(
            "/api/interviews/{id}/runs/{run_id}/outputs",
            post(import_output),
        )
        .route("/api/interviews/{id}/agreement", get(agreement))
        .route("/api/interviews/{id}/divergences", get(divergences))
        .route("/api/interviews/{id}/arbitration/{unit_id}", put(arbitrate))
        .route("/api/interviews/{id}/export", post(export_package))
        .route("/api/codebook", get(codebook))
        .route("/api/codebook/import", post(import_codebook))
        .route("/api/codebook/codes", post(add_code))
        .route("/api/codebook/codes/{code}/in_prompt", put(set_in_prompt))
        .route("/api/audit", get(audit))
        .route("/api/audit/verify", get(audit_verify))
        .with_state(state)
}

/// Current time, RFC 3339 UTC to the second, as stored everywhere else.
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn not_found(what: impl Into<String>) -> ApiError {
    ApiError::from(anyhow::Error::new(CsError::NotFound(what.into())))
}

fn invalid(msg: impl Into<String>) -> ApiError {
    ApiError::from(anyhow::Error::new(CsError::Validation(msg.into())))
}

fn require_segmentation(store: &Store, id: &str) -> Result<Segmentation, ApiError> {
    store.segmentation(id)?.ok_or_else(|| {
        invalid(format!(
            "{id} has no segmentation yet; segment a transcript first"
        ))
    })
}

/// Complete AI runs of an interview as (label, codings), in creation order.
fn complete_runs(store: &Store, id: &str) -> Result<Vec<(String, CodingMap)>, ApiError> {
    Ok(store
        .ai_runs(id)?
        .into_iter()
        .filter(|r| r.complete)
        .map(|r| (r.meta.run_label, r.codings))
        .collect())
}

// ------------------------------------------------------------------ static

/// Embedded assets are served uncached, so a rebuilt binary is always picked up.
fn asset(content_type: &'static str, body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

async fn index() -> Response {
    asset(
        "text/html; charset=utf-8",
        include_str!("../web/index.html"),
    )
}

async fn app_js() -> Response {
    asset(
        "text/javascript; charset=utf-8",
        include_str!("../web/app.js"),
    )
}

async fn style_css() -> Response {
    asset("text/css; charset=utf-8", include_str!("../web/style.css"))
}

// ------------------------------------------------------------------ state

async fn api_state(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    Ok(Json(json!({
        "version": VERSION,
        "db_path": st.db_path,
        "library_root": store.setting("library_root")?,
        "coding_db": st.coding_db,
        "actor": st.actor,
        "codebook_size": store.codebook()?.codes.len(),
        "chain": store.verify_chain()?,
        "native_dialogs": st.picker.is_some(),
    })))
}

#[derive(Deserialize)]
struct PickBody {
    kind: PickKind,
    title: Option<String>,
}

/// Native folder or file dialog (desktop app only).
async fn pick(State(st): State<AppState>, Json(body): Json<PickBody>) -> ApiResult {
    let picker = st.picker.clone().ok_or_else(|| ApiError {
        status: axum::http::StatusCode::NOT_IMPLEMENTED,
        kind: "validation",
        message: "native dialogs exist in the desktop app only; type the path".into(),
        issues: None,
    })?;
    let title = body.title.unwrap_or_default();
    let chosen = tokio::task::spawn_blocking(move || picker(body.kind, &title))
        .await
        .map_err(|e| anyhow::anyhow!("dialog thread failed: {e}"))?;
    Ok(Json(
        json!({ "path": chosen.map(|p| p.display().to_string()) }),
    ))
}

// ---------------------------------------------------------------- library

#[derive(Deserialize, Default)]
struct ScanBody {
    root: Option<String>,
}

async fn scan_library(State(st): State<AppState>, body: Option<Json<ScanBody>>) -> ApiResult {
    let body = body.map(|b| b.0).unwrap_or_default();
    let store = st.store();
    let root = match body
        .root
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
    {
        Some(r) => {
            if store.setting("library_root")?.as_deref() != Some(r.as_str()) {
                store.set_setting("library_root", &r, &st.actor)?;
            }
            r
        }
        None => store
            .setting("library_root")?
            .ok_or_else(|| invalid("no library folder given; enter the folder to scan"))?,
    };
    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        return Err(invalid(format!("{root} is not a folder")));
    }
    let entries = cs_ingest::scan_library(root_path)?;
    for entry in &entries {
        store.upsert_interview(entry, &st.actor)?;
    }
    Ok(Json(
        json!({ "root": root, "interviews": store.summaries()? }),
    ))
}

async fn list_interviews(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().summaries()?)))
}

async fn interview_detail(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let entry = store
        .interview(&id)?
        .ok_or_else(|| not_found(format!("interview {id}")))?;
    let segmentation = store.segmentation(&id)?;
    let guard = match segmentation {
        Some(_) => Some(store.guard(&id)?),
        None => None,
    };
    let runs: Vec<Value> = store
        .ai_runs(&id)?
        .into_iter()
        .map(|r| {
            json!({
                "run_id": r.run_id,
                "meta": r.meta,
                "parts_received": r.parts_received,
                "parts_expected": r.parts_expected,
                "complete": r.complete,
                "n_coded": r.codings.len(),
            })
        })
        .collect();
    let prompts: Vec<Value> = store
        .prompts(&id)?
        .into_iter()
        .map(|p| {
            json!({
                "part": p.part, "of": p.of, "sha256": p.sha256,
                "n_units": p.unit_ids.len(), "prompt_version": p.prompt_version,
                "segmentation_hash": p.segmentation_hash, "codebook_hash": p.codebook_hash,
            })
        })
        .collect();
    let locked = runs.iter().any(|r| {
        r["parts_received"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    });
    Ok(Json(json!({
        "entry": entry,
        "segmentation": segmentation,
        "guard": guard,
        "runs": runs,
        "prompts": prompts,
        "human_locked": locked,
    })))
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

async fn interview_file(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<FileQuery>,
) -> ApiResult {
    let entry = st
        .store()
        .interview(&id)?
        .ok_or_else(|| not_found(format!("interview {id}")))?;
    if q.path.split(['/', '\\']).any(|seg| seg == "..") {
        return Err(invalid("path must stay inside the interview folder"));
    }
    let source = entry
        .sources
        .iter()
        .find(|s| s.rel_path == q.path)
        .ok_or_else(|| not_found(format!("{} is not in the inventory of {id}", q.path)))?;
    let bytes = std::fs::read(Path::new(&entry.folder_path).join(&source.rel_path))
        .map_err(|e| invalid(format!("cannot read {}: {e}", source.rel_path)))?;
    Ok(Json(json!({
        "path": source.rel_path,
        "kind": source.kind,
        "sha256_recorded": source.sha256,
        "sha256_now": cs_model::sha256_hex(&bytes),
        "text": String::from_utf8_lossy(&bytes),
    })))
}

#[derive(Deserialize)]
struct PrimaryBody {
    rel_path: String,
}

async fn set_primary(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<PrimaryBody>,
) -> ApiResult {
    st.store()
        .set_primary_source(&id, &body.rel_path, &st.actor)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
struct SegmentBody {
    rel_path: Option<String>,
}

async fn segment(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    body: Option<Json<SegmentBody>>,
) -> ApiResult {
    let body = body.map(|b| b.0).unwrap_or_default();
    let store = st.store();
    let entry = store
        .interview(&id)?
        .ok_or_else(|| not_found(format!("interview {id}")))?;
    let rel = body
        .rel_path
        .filter(|r| !r.trim().is_empty())
        .or_else(|| entry.primary_source.clone())
        .ok_or_else(|| {
            invalid(format!(
                "{id} has no transcript to segment; choose a source"
            ))
        })?;
    let seg = cs_ingest::build_segmentation(&entry, &rel, &st.interviewer_names)?;
    store.save_segmentation(&seg, &st.actor)?;
    let current = store.segmentation(&id)?;
    Ok(Json(json!(current)))
}

async fn units(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let codings = store.human_codings(&id)?;
    let rows: Vec<Value> = seg
        .units
        .iter()
        .map(|u| json!({ "unit": u, "coding": codings.get(&u.unit_id) }))
        .collect();
    Ok(Json(json!(rows)))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CodingBody {
    codes: CodeSet,
    subcode: Option<String>,
    modifier: Option<String>,
    ev_level: Option<String>,
    provenance: Option<String>,
    memo: Option<String>,
    excerpt: Option<String>,
}

async fn set_coding(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<CodingBody>,
) -> ApiResult {
    let coding = HumanCoding {
        unit_id: unit_id.clone(),
        codes: body
            .codes
            .into_iter()
            .map(|c| c.trim().to_string())
            .collect(),
        subcode: body.subcode,
        modifier: body.modifier,
        ev_level: body.ev_level,
        provenance: body.provenance,
        memo: body.memo,
        excerpt: body.excerpt,
    };
    let store = st.store();
    store.set_human_coding(&id, &coding, &st.actor)?;
    let saved = store.human_codings(&id)?.remove(&unit_id);
    Ok(Json(json!({ "coding": saved, "guard": store.guard(&id)? })))
}

// ---------------------------------------------------------------- codebook

async fn codebook(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().codebook()?)))
}

#[derive(Deserialize, Default)]
struct ImportCodebookBody {
    coding_db: Option<String>,
}

async fn import_codebook(
    State(st): State<AppState>,
    body: Option<Json<ImportCodebookBody>>,
) -> ApiResult {
    let body = body.map(|b| b.0).unwrap_or_default();
    let path = body
        .coding_db
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| st.coding_db.clone())
        .ok_or_else(|| invalid("no coding.db given; enter its path"))?;
    if !path.is_file() {
        return Err(invalid(format!("{} is not a file", path.display())));
    }
    Ok(Json(json!(st.store().import_codebook(&path, &st.actor)?)))
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
    label: String,
    #[serde(default)]
    definition: String,
    #[serde(default)]
    family: Option<String>,
}

async fn add_code(State(st): State<AppState>, Json(body): Json<CodeBody>) -> ApiResult {
    let code = body.code.trim().to_string();
    if code.is_empty() || body.label.trim().is_empty() {
        return Err(invalid("an inductive code needs a code and a label"));
    }
    if code.eq_ignore_ascii_case(cs_model::NA) {
        return Err(invalid("NA is reserved"));
    }
    let new = Code {
        code,
        family: body
            .family
            .filter(|f| !f.trim().is_empty())
            .unwrap_or_else(|| "INDUCTIVE".to_string()),
        label: body.label.trim().to_string(),
        definition: body.definition.trim().to_string(),
        origin: "inductive".to_string(),
        active: true,
        in_prompt: true,
    };
    let store = st.store();
    store.add_code(&new, &st.actor)?;
    Ok(Json(json!(store.codebook()?)))
}

#[derive(Deserialize)]
struct InPromptBody {
    in_prompt: bool,
}

async fn set_in_prompt(
    State(st): State<AppState>,
    UrlPath(code): UrlPath<String>,
    Json(body): Json<InPromptBody>,
) -> ApiResult {
    let store = st.store();
    store.set_code_in_prompt(&code, body.in_prompt, &st.actor)?;
    Ok(Json(json!(store.codebook()?)))
}

// ------------------------------------------------------------------ AI side

#[derive(Deserialize, Default)]
struct PromptBody {
    #[serde(default)]
    batch_size: usize,
}

async fn generate_prompts(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    body: Option<Json<PromptBody>>,
) -> ApiResult {
    let batch = body.map(|b| b.0.batch_size).unwrap_or(0);
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let book = store.codebook()?;
    if book.prompt_codes().is_empty() {
        return Err(invalid(
            "the codebook offers no code to the AI; import the codebook first",
        ));
    }
    let parts = cs_ai::build_prompts(&seg, &book, batch);
    for part in &parts {
        store.record_prompt(part, &st.actor)?;
    }
    Ok(Json(json!(parts)))
}

async fn list_prompts(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().prompts(&id)?)))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RunBody {
    run_label: String,
    model_name: String,
    model_version: String,
    temperature: Option<f64>,
    seed: Option<String>,
    context_length: Option<u32>,
    batch_size: Option<u32>,
    prompt_id_version: String,
    executed_at: String,
    notes: Option<String>,
}

async fn create_run(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<RunBody>,
) -> ApiResult {
    let label = body.run_label.trim().to_string();
    if label.is_empty() || body.model_name.trim().is_empty() {
        return Err(invalid(
            "a run needs a label (R1, R2...) and the name of the model",
        ));
    }
    let meta = AiRunMeta {
        run_label: label,
        model_name: body.model_name.trim().to_string(),
        model_version: body.model_version.trim().to_string(),
        temperature: body.temperature,
        seed: body.seed.filter(|s| !s.trim().is_empty()),
        context_length: body.context_length,
        batch_size: body.batch_size,
        prompt_id_version: if body.prompt_id_version.trim().is_empty() {
            cs_ai::PROMPT_VERSION.to_string()
        } else {
            body.prompt_id_version.trim().to_string()
        },
        executed_at: if body.executed_at.trim().is_empty() {
            now()
        } else {
            body.executed_at.trim().to_string()
        },
        notes: body.notes.filter(|s| !s.trim().is_empty()),
    };
    let store = st.store();
    if store.interview(&id)?.is_none() {
        return Err(not_found(format!("interview {id}")));
    }
    let run_id = store.create_ai_run(&id, &meta, &st.actor)?;
    Ok(Json(json!({ "run_id": run_id, "meta": meta })))
}

#[derive(Deserialize)]
struct OutputBody {
    raw: String,
    part: Option<u32>,
    of: Option<u32>,
}

async fn import_output(
    State(st): State<AppState>,
    UrlPath((id, run_id)): UrlPath<(String, i64)>,
    Json(body): Json<OutputBody>,
) -> ApiResult {
    let store = st.store();
    if !store.ai_runs(&id)?.iter().any(|r| r.run_id == run_id) {
        return Err(not_found(format!("run {run_id} of {id}")));
    }
    let seg = require_segmentation(&store, &id)?;
    let book = store.codebook()?;
    let expected = body.part.zip(body.of);
    let parsed =
        cs_ai::parse_ai_output(&body.raw, &seg, &book, expected).map_err(ApiError::with_issues)?;
    store.record_ai_output(run_id, &body.raw, &parsed, &st.actor)?;
    let run = store
        .ai_runs(&id)?
        .into_iter()
        .find(|r| r.run_id == run_id)
        .ok_or_else(|| not_found(format!("run {run_id}")))?;
    Ok(Json(json!(run)))
}

async fn agreement(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let human = store.human_coding_map(&id)?;
    let runs = complete_runs(&store, &id)?;
    let order: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
    let mut reports = Vec::new();
    for (label, codings) in &runs {
        reports.push(cs_ai::agreement(
            "Human",
            &human,
            &format!("AI {label}"),
            codings,
            &order,
        ));
    }
    for (i, (la, a)) in runs.iter().enumerate() {
        for (lb, b) in runs.iter().skip(i + 1) {
            reports.push(cs_ai::agreement(
                &format!("AI {la}"),
                a,
                &format!("AI {lb}"),
                b,
                &order,
            ));
        }
    }
    let markdown = if reports.is_empty() {
        String::new()
    } else {
        cs_ai::render_agreement_md(&seg, &reports)
    };
    Ok(Json(json!({ "reports": reports, "markdown": markdown })))
}

async fn divergences(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let human = store.human_coding_map(&id)?;
    let runs = complete_runs(&store, &id)?;
    let current = store.arbitrations(&id)?;
    let history = store.arbitration_history(&id)?;
    let units: BTreeMap<&str, &cs_model::Unit> =
        seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let divs = if runs.is_empty() {
        Vec::new()
    } else {
        cs_ai::divergences(&seg, &human, &runs)
    };
    let n_decided = divs
        .iter()
        .filter(|d| current.contains_key(&d.unit_id))
        .count();
    let items: Vec<Value> = divs
        .iter()
        .map(|d| {
            let hist: Vec<&Arbitration> =
                history.iter().filter(|a| a.unit_id == d.unit_id).collect();
            json!({
                "divergence": d,
                "unit": units.get(d.unit_id.as_str()),
                "arbitration": current.get(&d.unit_id),
                "history": hist,
            })
        })
        .collect();
    Ok(Json(json!({
        "runs": runs.iter().map(|(l, _)| l).collect::<Vec<_>>(),
        "n_divergences": divs.len(),
        "n_decided": n_decided,
        "items": items,
    })))
}

#[derive(Deserialize)]
struct ArbitrationBody {
    choice: ArbitrationChoice,
    final_codes: CodeSet,
    #[serde(default)]
    rationale: String,
}

async fn arbitrate(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<ArbitrationBody>,
) -> ApiResult {
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let human = store.human_coding_map(&id)?;
    let runs = complete_runs(&store, &id)?;
    let divs = cs_ai::divergences(&seg, &human, &runs);
    if !divs.iter().any(|d| d.unit_id == unit_id) {
        return Err(invalid(format!(
            "{unit_id} is not a divergence; only divergent units are arbitrated"
        )));
    }
    let arb = Arbitration {
        unit_id,
        choice: body.choice,
        final_codes: body.final_codes,
        rationale: body.rationale.trim().to_string(),
        decided_by: st.actor.clone(),
        decided_at: now(),
    };
    store.record_arbitration(&id, &arb, &st.actor)?;
    Ok(Json(json!(arb)))
}

async fn export_package(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let base = st
        .db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("exports");
    let result = export::write_package(&store, &id, &base, &st.db_path, &st.actor)?;
    Ok(Json(json!(result)))
}

// -------------------------------------------------------------------- audit

#[derive(Deserialize)]
struct AuditQuery {
    interview_id: Option<String>,
}

async fn audit(State(st): State<AppState>, Query(q): Query<AuditQuery>) -> ApiResult {
    let id = q.interview_id.filter(|s| !s.is_empty());
    Ok(Json(json!(st.store().audit_events(id.as_deref())?)))
}

async fn audit_verify(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().verify_chain()?)))
}
