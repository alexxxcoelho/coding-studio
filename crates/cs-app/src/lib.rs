//! Coding Studio local server: JSON API over the store, the ingest and the AI
//! layers, plus the embedded React build. Every route is a thin adapter;
//! protocol rules live in `cs-store`, `cs-ai` and `cs-model`, never here.
//! The contract is documented in `docs/API.md`.

mod error;
mod export;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cs_model::phase::{phase1_codebook, phase2_allowed, PHASE_1, PHASE_2};
use cs_model::{
    AiRunMeta, Arbitration, ArbitrationChoice, Code, CodeSet, Codebook, CodingMap, CsError,
    HumanCoding, Segmentation,
};
use cs_store::Store;
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use serde_json::{json, Value};

pub use error::ApiError;

/// Version reported by `/api/state` and written in every export manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The React build (`make web` copies `web/dist` here).
static WEB: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/web-dist");

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

/// Builds the router: API under `/api`, the React build everywhere else.
pub fn router(state: AppState) -> Router {
    const P: &str = "/api/interviews/{id}/phases/{phase}";
    Router::new()
        .route("/api/state", get(api_state))
        .route("/api/pick", post(pick))
        .route("/api/library/scan", post(scan_library))
        .route("/api/interviews", get(list_interviews))
        .route("/api/interviews/{id}", get(interview_detail))
        .route("/api/interviews/{id}/file", get(interview_file))
        .route("/api/interviews/{id}/primary", post(set_primary))
        .route("/api/interviews/{id}/segment", post(segment))
        .route("/api/interviews/{id}/export", post(export_package))
        .route(P, get(phase_state))
        .route(&format!("{P}/units"), get(units))
        .route(&format!("{P}/coding/{{unit_id}}"), put(set_coding))
        .route(
            &format!("{P}/prompts"),
            post(generate_prompts).get(list_prompts),
        )
        .route(&format!("{P}/runs"), post(create_run).get(list_runs))
        .route(&format!("{P}/runs/{{run_id}}/outputs"), post(import_output))
        .route(&format!("{P}/agreement"), get(agreement))
        .route(&format!("{P}/divergences"), get(divergences))
        .route(&format!("{P}/arbitration/{{unit_id}}"), put(arbitrate))
        .route(&format!("{P}/final"), get(final_coding))
        .route("/api/codebook", get(codebook))
        .route("/api/codebook/phase1", get(codebook_phase1))
        .route("/api/codebook/import", post(import_codebook))
        .route("/api/codebook/codes", post(add_code))
        .route("/api/codebook/codes/{code}/in_prompt", put(set_in_prompt))
        .route("/api/audit", get(audit))
        .route("/api/audit/verify", get(audit_verify))
        .fallback(static_file)
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

fn guard(msg: impl Into<String>) -> ApiError {
    ApiError::from(anyhow::Error::new(CsError::Guard(msg.into())))
}

fn parse_phase(raw: &str) -> Result<u8, ApiError> {
    match raw {
        "1" => Ok(PHASE_1),
        "2" => Ok(PHASE_2),
        _ => Err(invalid(format!("unknown phase {raw}; use 1 or 2"))),
    }
}

fn require_interview(store: &Store, id: &str) -> Result<cs_model::InterviewEntry, ApiError> {
    store
        .interview(id)?
        .ok_or_else(|| not_found(format!("interview {id}")))
}

fn require_segmentation(store: &Store, id: &str) -> Result<Segmentation, ApiError> {
    require_interview(store, id)?;
    store.segmentation(id)?.ok_or_else(|| {
        invalid(format!(
            "{id} has no segmentation yet; segment a transcript first"
        ))
    })
}

/// Complete AI runs of a phase as (label, codings), in creation order.
fn complete_runs(store: &Store, id: &str, phase: u8) -> Result<Vec<(String, CodingMap)>, ApiError> {
    Ok(store
        .ai_runs(id, phase)?
        .into_iter()
        .filter(|r| r.complete)
        .map(|r| (r.meta.run_label, r.codings))
        .collect())
}

/// Reconciled phase 1 codes per unit; empty while phase 1 is not reconciled.
fn phase1_finals(store: &Store, id: &str) -> Result<BTreeMap<String, CodeSet>, ApiError> {
    Ok(match store.final_codings(id, PHASE_1)? {
        Ok(finals) => finals.into_iter().map(|f| (f.unit_id, f.codes)).collect(),
        Err(_) => BTreeMap::new(),
    })
}

/// Codes the candidate may use on each unit of a phase (NA aside).
fn human_allowed(
    store: &Store,
    id: &str,
    phase: u8,
    unit_ids: &[String],
) -> Result<BTreeMap<String, Vec<Code>>, ApiError> {
    if phase == PHASE_1 {
        let codes = phase1_codebook().codes;
        return Ok(unit_ids
            .iter()
            .map(|u| (u.clone(), codes.clone()))
            .collect());
    }
    let book = store.codebook()?;
    let p1 = phase1_finals(store, id)?;
    Ok(unit_ids
        .iter()
        .map(|u| {
            let set = p1.get(u).cloned().unwrap_or_default();
            let codes = phase2_allowed(&book, &set, true)
                .into_iter()
                .cloned()
                .collect();
            (u.clone(), codes)
        })
        .collect())
}

/// Everything a phase's prompts are built from. The same scope is rebuilt to
/// check an answer, so the parser always sees the prompt's own inputs.
struct AiScope {
    seg: Segmentation,
    unit_ids: Vec<String>,
    codebook: Codebook,
    allowed: Option<BTreeMap<String, Vec<String>>>,
}

impl AiScope {
    fn input(&self, phase: u8) -> cs_ai::PromptInput<'_> {
        cs_ai::PromptInput {
            seg: &self.seg,
            phase,
            unit_ids: &self.unit_ids,
            codebook: &self.codebook,
            allowed: self.allowed.as_ref(),
        }
    }
}

/// Phase 1: the four TOE categories over every unit. Phase 2: the relevant
/// units, each with the study-specific codes of its reconciled contexts plus
/// the cross-cutting families, minus the codes withheld from the AI.
fn ai_scope(store: &Store, id: &str, phase: u8) -> Result<AiScope, ApiError> {
    let seg = require_segmentation(store, id)?;
    let unit_ids = store.phase_units(id, phase)?;
    if phase == PHASE_1 {
        return Ok(AiScope {
            seg,
            unit_ids,
            codebook: phase1_codebook(),
            allowed: None,
        });
    }
    if unit_ids.is_empty() {
        return Err(guard(
            "phase 2 has no unit in scope yet: reconcile phase 1 first (a complete AI run and every divergence arbitrated)",
        ));
    }
    let book = store.codebook()?;
    let p1 = phase1_finals(store, id)?;
    let mut allowed = BTreeMap::new();
    let mut union = BTreeSet::new();
    for u in &unit_ids {
        let set = p1.get(u).cloned().unwrap_or_default();
        let codes: Vec<String> = phase2_allowed(&book, &set, false)
            .into_iter()
            .filter(|c| c.in_prompt)
            .map(|c| c.code.clone())
            .collect();
        union.extend(codes.iter().cloned());
        allowed.insert(u.clone(), codes);
    }
    let codebook = Codebook {
        source: book.source.clone(),
        codes: book
            .codes
            .iter()
            .filter(|c| union.contains(&c.code))
            .cloned()
            .collect(),
    };
    Ok(AiScope {
        seg,
        unit_ids,
        codebook,
        allowed: Some(allowed),
    })
}

// ------------------------------------------------------------------ static

/// Serves the embedded React build. Hashed assets are immutable; any other
/// path that is not a file gets `index.html` (client-side routing).
async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return not_found(format!("no API route {}", uri.path())).into_response();
    }
    if let Some(file) = WEB.get_file(path).filter(|_| !path.is_empty()) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [
                (header::CONTENT_TYPE, mime.essence_str().to_string()),
                (header::CACHE_CONTROL, cache.to_string()),
            ],
            file.contents(),
        )
            .into_response();
    }
    match WEB.get_file("index.html") {
        Some(index) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            index.contents(),
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "The web interface is not built; run `make web`.",
        )
            .into_response(),
    }
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
        status: StatusCode::NOT_IMPLEMENTED,
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
    let entry = require_interview(&store, &id)?;
    Ok(Json(json!({
        "entry": entry,
        "segmentation": store.segmentation(&id)?,
        "phase1": store.phase_summary(&id, PHASE_1)?,
        "phase2": store.phase_summary(&id, PHASE_2)?,
        "phase2_started": store.phase2_started(&id)?,
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
    let entry = require_interview(&st.store(), &id)?;
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
        "sha256": source.sha256,
        "changed_since_scan": cs_model::sha256_hex(&bytes) != source.sha256,
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
    let store = st.store();
    store.set_primary_source(&id, &body.rel_path, &st.actor)?;
    Ok(Json(json!(require_interview(&store, &id)?)))
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
    let entry = require_interview(&store, &id)?;
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
    Ok(Json(json!(store.segmentation(&id)?)))
}

// ------------------------------------------------------------ phase coding

async fn phase_state(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    require_interview(&store, &id)?;
    let locked = store
        .ai_runs(&id, phase)?
        .iter()
        .any(|r| !r.parts_received.is_empty());
    let frozen = phase == PHASE_1 && store.phase2_started(&id)?;
    Ok(Json(json!({
        "phase": phase,
        "summary": store.phase_summary(&id, phase)?,
        "guard": store.guard(&id, phase)?,
        "reconciled": store.is_reconciled(&id, phase)?,
        "frozen": frozen,
        "locked": locked,
    })))
}

async fn units(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let scope = store.phase_units(&id, phase)?;
    let codings = store.human_codings(&id, phase)?;
    let allowed = human_allowed(&store, &id, phase, &scope)?;
    let p1 = if phase == PHASE_2 {
        phase1_finals(&store, &id)?
    } else {
        BTreeMap::new()
    };
    let by_id: BTreeMap<&str, &cs_model::Unit> =
        seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let rows: Vec<Value> = scope
        .iter()
        .filter_map(|u| by_id.get(u.as_str()).map(|unit| (u, unit)))
        .map(|(u, unit)| {
            json!({
                "unit": unit,
                "coding": codings.get(u),
                "allowed": allowed.get(u).cloned().unwrap_or_default(),
                "phase1_final": if phase == PHASE_2 { p1.get(u).map(|s| json!(s)) } else { None },
            })
        })
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
    UrlPath((id, phase, unit_id)): UrlPath<(String, String, String)>,
    Json(body): Json<CodingBody>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
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
    require_interview(&store, &id)?;
    store.set_human_coding(&id, phase, &coding, &st.actor)?;
    let saved = store.human_codings(&id, phase)?.remove(&unit_id);
    Ok(Json(json!(saved)))
}

// ---------------------------------------------------------------- codebook

async fn codebook(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().codebook()?)))
}

async fn codebook_phase1() -> ApiResult {
    Ok(Json(json!(phase1_codebook())))
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
    #[serde(default)]
    label: String,
    #[serde(default)]
    definition: String,
    #[serde(default)]
    example: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// New inductive code. The store requires a definition and the reason it
/// was created (inductive log); inductive codes are never offered to the AI.
async fn add_code(State(st): State<AppState>, Json(body): Json<CodeBody>) -> ApiResult {
    let code = body.code.trim().to_string();
    let label = match body.label.trim() {
        "" => code.clone(),
        l => l.to_string(),
    };
    let new = Code {
        code,
        family: "INDUCTIVE".to_string(),
        label,
        definition: body.definition.trim().to_string(),
        origin: "inductive".to_string(),
        active: true,
        in_prompt: false,
        example: body.example,
        reason: body.reason,
        created_at: None,
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
    UrlPath((id, phase)): UrlPath<(String, String)>,
    body: Option<Json<PromptBody>>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let batch = body.map(|b| b.0.batch_size).unwrap_or(0);
    let store = st.store();
    let scope = ai_scope(&store, &id, phase)?;
    if scope.unit_ids.is_empty() {
        return Err(invalid(format!("{id} has no unit in phase {phase}")));
    }
    if scope.codebook.codes.is_empty() {
        return Err(invalid(
            "no code can be offered to the AI; import the codebook first",
        ));
    }
    let parts = cs_ai::build_prompts(&scope.input(phase), batch);
    for part in &parts {
        store.record_prompt(part, &st.actor)?;
    }
    Ok(Json(json!(parts)))
}

async fn list_prompts(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    Ok(Json(json!(st.store().prompts(&id, phase)?)))
}

async fn list_runs(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    Ok(Json(json!(st.store().ai_runs(&id, phase)?)))
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
    UrlPath((id, phase)): UrlPath<(String, String)>,
    Json(body): Json<RunBody>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
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
    require_interview(&store, &id)?;
    let run_id = store.create_ai_run(&id, phase, &meta, &st.actor)?;
    let run = store
        .ai_runs(&id, phase)?
        .into_iter()
        .find(|r| r.run_id == run_id)
        .ok_or_else(|| not_found(format!("run {run_id}")))?;
    Ok(Json(json!(run)))
}

#[derive(Deserialize)]
struct OutputBody {
    raw: String,
    part: Option<u32>,
    of: Option<u32>,
}

async fn import_output(
    State(st): State<AppState>,
    UrlPath((id, phase, run_id)): UrlPath<(String, String, i64)>,
    Json(body): Json<OutputBody>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    require_interview(&store, &id)?;
    if !store
        .ai_runs(&id, phase)?
        .iter()
        .any(|r| r.run_id == run_id)
    {
        return Err(not_found(format!("run {run_id} of {id}, phase {phase}")));
    }
    let scope = ai_scope(&store, &id, phase)?;
    let expected = body.part.zip(body.of);
    let parsed = cs_ai::parse_ai_output(&body.raw, &scope.input(phase), expected)
        .map_err(ApiError::with_issues)?;
    store.record_ai_output(run_id, &body.raw, &parsed, &st.actor)?;
    let run = store
        .ai_runs(&id, phase)?
        .into_iter()
        .find(|r| r.run_id == run_id)
        .ok_or_else(|| not_found(format!("run {run_id}")))?;
    Ok(Json(json!(run)))
}

// ----------------------------------------------------------- reconciliation

/// Human against each complete run, then runs pairwise, over `order`.
pub(crate) fn agreement_reports(
    human: &CodingMap,
    runs: &[(String, CodingMap)],
    order: &[String],
) -> Vec<cs_model::AgreementReport> {
    let mut reports = Vec::new();
    for (label, codings) in runs {
        reports.push(cs_ai::agreement(
            "Human",
            human,
            &format!("AI {label}"),
            codings,
            order,
        ));
    }
    for (i, (la, a)) in runs.iter().enumerate() {
        for (lb, b) in runs.iter().skip(i + 1) {
            reports.push(cs_ai::agreement(
                &format!("AI {la}"),
                a,
                &format!("AI {lb}"),
                b,
                order,
            ));
        }
    }
    reports
}

async fn agreement(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let order = store.phase_units(&id, phase)?;
    let human = store.human_coding_map(&id, phase)?;
    let runs = complete_runs(&store, &id, phase)?;
    let reports = agreement_reports(&human, &runs, &order);
    let markdown = if reports.is_empty() {
        String::new()
    } else {
        cs_ai::render_agreement_md(&seg, phase, &order, &reports)
    };
    Ok(Json(json!({ "reports": reports, "markdown": markdown })))
}

async fn divergences(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let order = store.phase_units(&id, phase)?;
    let human = store.human_coding_map(&id, phase)?;
    let runs = complete_runs(&store, &id, phase)?;
    let current = store.arbitrations(&id, phase)?;
    let history = store.arbitration_history(&id, phase)?;
    let units: BTreeMap<&str, &cs_model::Unit> =
        seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let divs = cs_ai::divergences(&order, &human, &runs);
    let div_ids: Vec<String> = divs.iter().map(|d| d.unit_id.clone()).collect();
    let allowed = human_allowed(&store, &id, phase, &div_ids)?;
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
                "allowed": allowed.get(&d.unit_id).cloned().unwrap_or_default(),
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
    UrlPath((id, phase, unit_id)): UrlPath<(String, String, String)>,
    Json(body): Json<ArbitrationBody>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    require_segmentation(&store, &id)?;
    let order = store.phase_units(&id, phase)?;
    let human = store.human_coding_map(&id, phase)?;
    let runs = complete_runs(&store, &id, phase)?;
    let divs = cs_ai::divergences(&order, &human, &runs);
    if !divs.iter().any(|d| d.unit_id == unit_id) {
        return Err(invalid(format!(
            "{unit_id} is not a divergence of phase {phase}; only divergent units are arbitrated"
        )));
    }
    let arb = Arbitration {
        unit_id,
        phase,
        choice: body.choice,
        final_codes: body.final_codes,
        rationale: body.rationale.trim().to_string(),
        decided_by: st.actor.clone(),
        decided_at: now(),
    };
    store.record_arbitration(&id, &arb, &st.actor)?;
    Ok(Json(json!(arb)))
}

async fn final_coding(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(match store.final_codings(&id, phase)? {
        Ok(finals) => json!({ "reconciled": true, "finals": finals, "pending": [] }),
        Err(pending) => json!({ "reconciled": false, "finals": [], "pending": pending }),
    }))
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
