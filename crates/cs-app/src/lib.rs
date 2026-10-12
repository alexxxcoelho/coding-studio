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
use cs_model::phase::{phase1_codebook, phase2_codes, phase2_prompt_codes, PHASE_1, PHASE_2};
use cs_model::{
    AiRunMeta, Arbitration, ArbitrationChoice, Case, Claim, Code, CodeSet, CodingMap, CsError,
    HumanCoding, ResolutionKind, RqMemo, Segmentation, SpeakerRole, Unit,
};
use cs_store::{CodePatch, ResolveRequest, Store};
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
    const I: &str = "/api/interviews/{id}";
    const P: &str = "/api/interviews/{id}/phases/{phase}";
    Router::new()
        .route("/api/state", get(api_state))
        .route("/api/pick", post(pick))
        .route("/api/library/scan", post(scan_library))
        .route("/api/interviews", get(list_interviews))
        .route(I, get(interview_detail))
        .route(&format!("{I}/file"), get(interview_file))
        .route(&format!("{I}/primary"), post(set_primary))
        .route(&format!("{I}/segment"), post(segment))
        .route(&format!("{I}/export"), post(export_package))
        .route(P, get(phase_state))
        .route(&format!("{P}/units"), get(units))
        .route(&format!("{P}/coding/{{unit_id}}"), put(set_coding))
        .route(
            &format!("{P}/prompts"),
            post(generate_prompts).get(list_prompts),
        )
        .route(&format!("{P}/runs"), post(create_run).get(list_runs))
        .route(&format!("{P}/runs/{{run_id}}/outputs"), post(import_output))
        .route(&format!("{I}/review"), get(review))
        .route(&format!("{I}/snapshots"), get(human_snapshots))
        .route(&format!("{I}/review/{{unit_id}}"), put(decide))
        .route(&format!("{I}/final"), get(final_coding))
        .route(&format!("{I}/agreement"), get(agreement))
        .route(
            &format!("{I}/units/{{unit_id}}/to-clarify"),
            post(add_to_clarify),
        )
        .route("/api/to-clarify", get(list_to_clarify))
        .route("/api/to-clarify/{tid}", put(edit_to_clarify))
        .route("/api/to-clarify/{tid}/resolve", post(resolve_to_clarify))
        .route("/api/codebook", get(codebook))
        .route("/api/codebook/phase1", get(codebook_phase1))
        .route("/api/codebook/import", post(import_codebook))
        .route("/api/codebook/export", post(export_codebook))
        .route("/api/codebook/codes", post(add_code))
        .route("/api/codebook/codes/{code}", put(edit_code))
        .route("/api/codebook/codes/{code}/in_prompt", put(set_in_prompt))
        .route("/api/codebook/codes/{code}/active", post(set_active))
        .route("/api/codebook/codes/{code}/merge", post(merge_code))
        .route("/api/codebook/codes/{code}/units", get(code_units))
        .route(
            "/api/research-questions",
            get(list_questions).post(add_question),
        )
        .route(
            "/api/research-questions/{rq}",
            put(edit_question).delete(delete_question),
        )
        .route("/api/cases", get(all_cases))
        .route("/api/cases/{case_id}", put(edit_case))
        .route(&format!("{I}/cases"), get(interview_cases).post(add_case))
        .route("/api/cross-case", get(cross_case))
        .route(&format!("{I}/flags"), get(list_flags))
        .route(&format!("{I}/units/{{unit_id}}/flags"), put(set_flags))
        .route(
            &format!("{I}/units/{{unit_id}}/corrections"),
            get(list_corrections).post(add_correction),
        )
        .route(&format!("{I}/verification"), get(verification_state))
        .route(
            &format!("{I}/units/{{unit_id}}/verification"),
            put(set_verification),
        )
        .route(&format!("{I}/claims"), get(list_claims).post(add_claim))
        .route(
            &format!("{I}/units/{{unit_id}}/claims"),
            get(list_unit_claims),
        )
        .route(
            "/api/claims/{claim_id}",
            put(edit_claim).delete(remove_claim),
        )
        .route(&format!("{I}/claims/review"), get(claims_review))
        .route(
            "/api/research-questions/{rq}/memo",
            get(get_rq_memo).put(put_rq_memo),
        )
        .route("/api/saturation", get(saturation_table))
        .route("/api/export/cross-case", post(export_cross_case))
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

fn parse_phase(raw: &str) -> Result<u8, ApiError> {
    match raw {
        "1" => Ok(PHASE_1),
        "2" => Ok(PHASE_2),
        _ => Err(invalid(format!("unknown phase {raw}; use 1 or 2"))),
    }
}

/// The AI pass exists in phase 2 only.
fn only_phase2(raw: &str) -> Result<(), ApiError> {
    match parse_phase(raw)? {
        PHASE_2 => Ok(()),
        _ => Err(invalid(
            "phase 1 is a provisional pre-classification: it has no AI pass",
        )),
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

/// Units compared with the AI: those of the complete runs' batches, in
/// segmentation order.
pub(crate) fn compared_units(store: &Store, id: &str) -> anyhow::Result<Vec<String>> {
    let in_runs: BTreeSet<String> = store
        .ai_runs(id)?
        .into_iter()
        .filter(|r| r.complete)
        .flat_map(|r| r.unit_ids)
        .collect();
    Ok(store
        .segmentation(id)?
        .map(|s| {
            s.units
                .into_iter()
                .map(|u| u.unit_id)
                .filter(|u| in_runs.contains(u))
                .collect()
        })
        .unwrap_or_default())
}

/// A complete run: (run_label, batch_id, codings).
pub(crate) type CandidateRun = (String, i64, CodingMap);

/// The complete runs, and the frozen human reference (snapshot codings) of
/// each of their batches, keyed by batch_id.
pub(crate) fn candidate_runs(
    store: &Store,
    id: &str,
) -> anyhow::Result<(BTreeMap<i64, CodingMap>, Vec<CandidateRun>)> {
    let runs: Vec<CandidateRun> = store
        .ai_runs(id)?
        .into_iter()
        .filter(|r| r.complete)
        .map(|r| (r.meta.run_label, r.batch_id, r.codings))
        .collect();
    let mut snapshots = BTreeMap::new();
    for (_, batch_id, _) in &runs {
        if !snapshots.contains_key(batch_id) {
            snapshots.insert(*batch_id, store.human_snapshot(id, *batch_id)?.codings);
        }
    }
    Ok((snapshots, runs))
}

/// Candidate (the snapshot of each run's batch) against each complete run,
/// then runs pairwise, over `order`.
pub(crate) fn agreement_reports(
    snapshots: &BTreeMap<i64, CodingMap>,
    runs: &[CandidateRun],
    order: &[String],
) -> anyhow::Result<Vec<cs_model::AgreementReport>> {
    let mut reports = Vec::new();
    for (label, batch_id, codings) in runs {
        let human = snapshots
            .get(batch_id)
            .ok_or_else(|| anyhow::anyhow!("no human snapshot of batch {batch_id}"))?;
        reports.push(cs_ai::agreement(
            "Candidate",
            human,
            &format!("AI {label}"),
            codings,
            order,
        ));
    }
    for (i, (la, _, a)) in runs.iter().enumerate() {
        for (lb, _, b) in runs.iter().skip(i + 1) {
            reports.push(cs_ai::agreement(
                &format!("AI {la}"),
                a,
                &format!("AI {lb}"),
                b,
                order,
            ));
        }
    }
    Ok(reports)
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
    Ok(Json(json!({
        "phase": phase,
        "summary": store.phase_summary(&id, phase)?,
        "guard": store.guard(&id, phase)?,
    })))
}

async fn units(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    let phase = parse_phase(&phase)?;
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let units = current_units(&store, &id, &seg)?;
    let codings = store.human_codings(&id, phase)?;
    if phase == PHASE_1 {
        let allowed = phase1_codebook().codes;
        let rows: Vec<Value> = units
            .iter()
            .map(|u| {
                json!({
                    "unit": u,
                    "coding": codings.get(&u.unit_id),
                    "allowed": allowed,
                })
            })
            .collect();
        return Ok(Json(json!(rows)));
    }
    let scope: BTreeSet<String> = store.phase_units(&id, PHASE_2)?.into_iter().collect();
    let p1 = store.human_coding_map(&id, PHASE_1)?;
    let book = store.codebook()?;
    let labels = store.to_clarify_items(Some(&id), false)?;
    let mut claims: BTreeMap<String, Vec<Claim>> = BTreeMap::new();
    for claim in store.claims(&id)? {
        claims.entry(claim.unit_id.clone()).or_default().push(claim);
    }
    let flags: BTreeMap<String, Vec<String>> = store
        .unit_flags(&id)?
        .into_iter()
        .map(|f| (f.unit_id, f.flags))
        .collect();
    let verification: BTreeMap<String, Value> = store
        .verification_status(&id)?
        .into_iter()
        .map(|s| (s.unit_id, json!({ "state": s.state, "stale": s.stale })))
        .collect();
    let rows: Vec<Value> = units
        .iter()
        .filter(|u| scope.contains(&u.unit_id) || codings.contains_key(&u.unit_id))
        .map(|u| {
            let ctx = p1.get(&u.unit_id).cloned().unwrap_or_default();
            let allowed: Vec<&Code> = phase2_codes(&book, &ctx);
            json!({
                "unit": u,
                "coding": codings.get(&u.unit_id),
                "allowed": allowed,
                "phase1": p1.get(&u.unit_id),
                "in_scope": scope.contains(&u.unit_id),
                "to_clarify": labels.iter().filter(|t| t.unit_id == u.unit_id).collect::<Vec<_>>(),
                "claims": claims.get(&u.unit_id).cloned().unwrap_or_default(),
                "flags": flags.get(&u.unit_id).cloned().unwrap_or_default(),
                "verification": verification
                    .get(&u.unit_id)
                    .cloned()
                    .unwrap_or_else(|| json!({ "state": "unverified", "stale": false })),
            })
        })
        .collect();
    Ok(Json(json!(rows)))
}

/// The units as they stand now: the segmentation with the latest correction of
/// each unit overlaid. The segmentation itself is never rewritten.
fn current_units(store: &Store, id: &str, seg: &Segmentation) -> Result<Vec<Unit>, ApiError> {
    seg.units
        .iter()
        .map(|u| -> Result<Unit, ApiError> {
            let mut unit = u.clone();
            if let Some(c) = store.unit_corrections(id, &u.unit_id)?.pop() {
                unit.text = c.text;
                unit.role = c.role;
                unit.corrected = true;
            }
            Ok(unit)
        })
        .collect()
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
        unit_id,
        codes: body.codes,
        subcode: body.subcode,
        modifier: body.modifier,
        ev_level: body.ev_level,
        provenance: body.provenance,
        memo: body.memo,
        excerpt: body.excerpt,
        revised_after_ai: false,
    };
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(
        store.set_human_coding(&id, phase, &coding, &st.actor)?
    )))
}

// -------------------------------------------------------------- to clarify

#[derive(Deserialize)]
struct LabelBody {
    label: String,
    #[serde(default)]
    note: String,
}

async fn add_to_clarify(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<LabelBody>,
) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(store.create_to_clarify(
        &id,
        &unit_id,
        &body.label,
        &body.note,
        &st.actor
    )?)))
}

#[derive(Deserialize, Default)]
struct ToClarifyQuery {
    interview_id: Option<String>,
    open: Option<bool>,
}

/// Items with the text of their unit.
async fn list_to_clarify(State(st): State<AppState>, Query(q): Query<ToClarifyQuery>) -> ApiResult {
    let store = st.store();
    let id = q.interview_id.filter(|s| !s.is_empty());
    let items = store.to_clarify_items(id.as_deref(), q.open.unwrap_or(false))?;
    let mut texts: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut out = Vec::new();
    for t in items {
        if !texts.contains_key(&t.interview_id) {
            let map = store
                .segmentation(&t.interview_id)?
                .map(|s| s.units.into_iter().map(|u| (u.unit_id, u.text)).collect())
                .unwrap_or_default();
            texts.insert(t.interview_id.clone(), map);
        }
        let mut v = json!(t);
        v["unit_text"] = json!(texts[&t.interview_id].get(&t.unit_id));
        out.push(v);
    }
    Ok(Json(json!(out)))
}

async fn edit_to_clarify(
    State(st): State<AppState>,
    UrlPath(tid): UrlPath<i64>,
    Json(body): Json<LabelBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().update_to_clarify(
        tid,
        &body.label,
        &body.note,
        &st.actor
    )?)))
}

#[derive(Deserialize)]
struct ResolveBody {
    kind: ResolutionKind,
    code: String,
    #[serde(default)]
    rationale: Option<String>,
    #[serde(default)]
    all_with_label: bool,
    #[serde(default)]
    new_code: Option<CodeBody>,
}

async fn resolve_to_clarify(
    State(st): State<AppState>,
    UrlPath(tid): UrlPath<i64>,
    Json(body): Json<ResolveBody>,
) -> ApiResult {
    let req = ResolveRequest {
        kind: body.kind,
        code: body.code,
        rationale: body.rationale,
        all_with_label: body.all_with_label,
        new_code: body.new_code.map(CodeBody::into_code),
    };
    let store = st.store();
    let resolved = store.resolve_to_clarify(tid, &req, &st.actor)?;
    Ok(Json(
        json!({ "resolved": resolved, "codebook": store.codebook()? }),
    ))
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
    origin: String,
    #[serde(default)]
    toe: Option<String>,
    #[serde(default)]
    research_questions: Vec<String>,
    #[serde(default)]
    example: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default = "yes")]
    in_prompt: bool,
}

fn yes() -> bool {
    true
}

impl CodeBody {
    fn into_code(self) -> Code {
        let code = self.code.trim().to_string();
        let label = match self.label.trim() {
            "" => code.clone(),
            l => l.to_string(),
        };
        Code {
            code,
            family: String::new(),
            label,
            definition: self.definition,
            origin: self.origin,
            toe: self.toe,
            active: true,
            in_prompt: self.in_prompt,
            research_questions: self.research_questions,
            merged_into: None,
            version: 1,
            example: self.example,
            reason: self.reason,
            created_at: None,
        }
    }
}

/// New phase 2 code (origin literature or field, default field).
async fn add_code(State(st): State<AppState>, Json(body): Json<CodeBody>) -> ApiResult {
    let store = st.store();
    store.create_code(&body.into_code(), &st.actor)?;
    Ok(Json(json!(store.codebook()?)))
}

async fn edit_code(
    State(st): State<AppState>,
    UrlPath(code): UrlPath<String>,
    Json(patch): Json<CodePatch>,
) -> ApiResult {
    let store = st.store();
    store.edit_code(&code, &patch, &st.actor)?;
    Ok(Json(json!(store.codebook()?)))
}

#[derive(Deserialize)]
struct ActiveBody {
    active: bool,
}

async fn set_active(
    State(st): State<AppState>,
    UrlPath(code): UrlPath<String>,
    Json(body): Json<ActiveBody>,
) -> ApiResult {
    let store = st.store();
    store.set_code_active(&code, body.active, &st.actor)?;
    Ok(Json(json!(store.codebook()?)))
}

#[derive(Deserialize)]
struct MergeBody {
    into: String,
    #[serde(default)]
    rationale: String,
}

async fn merge_code(
    State(st): State<AppState>,
    UrlPath(code): UrlPath<String>,
    Json(body): Json<MergeBody>,
) -> ApiResult {
    let store = st.store();
    let out = store.merge_code(&code, &body.into, &body.rationale, &st.actor)?;
    Ok(Json(json!({
        "codebook": store.codebook()?,
        "rewritten_codings": out.rewritten_codings,
        "rewritten_decisions": out.rewritten_decisions,
    })))
}

async fn code_units(State(st): State<AppState>, UrlPath(code): UrlPath<String>) -> ApiResult {
    let rows: Vec<Value> = st
        .store()
        .code_units(&code)?
        .into_iter()
        .map(|(iid, unit, coding)| json!({"interview_id": iid, "unit": unit, "coding": coding}))
        .collect();
    Ok(Json(json!(rows)))
}

/// Writes the codebook as CSV and SQL under `<db dir>/exports/codebook/`.
async fn export_codebook(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let (csv, sql) = store.codebook_export()?;
    let base = st
        .db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("exports")
        .join("codebook");
    std::fs::create_dir_all(&base)
        .map_err(|e| invalid(format!("creating {}: {e}", base.display())))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let mut files = Vec::new();
    for (ext, content) in [("csv", csv), ("sql", sql)] {
        let mut path = base.join(format!("{stamp}_codebook.{ext}"));
        let mut n = 2;
        while path.exists() {
            path = base.join(format!("{stamp}_{n}_codebook.{ext}"));
            n += 1;
        }
        std::fs::write(&path, &content)
            .map_err(|e| invalid(format!("writing {}: {e}", path.display())))?;
        files.push(json!({
            "path": path.display().to_string(),
            "sha256": cs_model::sha256_hex(content.as_bytes()),
            "size": content.len(),
        }));
    }
    store.log(
        &st.actor,
        "codebook.export",
        None,
        Some(&base.display().to_string()),
        json!({ "files": files, "n_codes": store.codebook()?.codes.len() }),
    )?;
    Ok(Json(json!({ "files": files })))
}

// ------------------------------------------------------ research questions

async fn list_questions(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().research_questions()?)))
}

#[derive(Deserialize)]
struct QuestionBody {
    #[serde(default)]
    id: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    ord: Option<u32>,
}

async fn add_question(State(st): State<AppState>, Json(body): Json<QuestionBody>) -> ApiResult {
    let store = st.store();
    store.create_research_question(&body.id, &body.text, &st.actor)?;
    Ok(Json(json!(store.research_questions()?)))
}

async fn edit_question(
    State(st): State<AppState>,
    UrlPath(rq): UrlPath<String>,
    Json(body): Json<QuestionBody>,
) -> ApiResult {
    let store = st.store();
    store.update_research_question(&rq, &body.text, body.ord, &st.actor)?;
    Ok(Json(json!(store.research_questions()?)))
}

async fn delete_question(State(st): State<AppState>, UrlPath(rq): UrlPath<String>) -> ApiResult {
    let store = st.store();
    store.delete_research_question(&rq, &st.actor)?;
    Ok(Json(json!(store.research_questions()?)))
}

// ------------------------------------------------------------------- cases

async fn all_cases(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().cases(None)?)))
}

async fn interview_cases(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(store.cases(Some(&id))?)))
}

async fn add_case(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(case): Json<Case>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .create_case(&id, &case, &st.actor)?)))
}

async fn edit_case(
    State(st): State<AppState>,
    UrlPath(case_id): UrlPath<String>,
    Json(case): Json<Case>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .update_case(&case_id, &case, &st.actor)?)))
}

async fn cross_case(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().cross_case()?)))
}

// ------------------------------------------- flags, corrections, verification

async fn list_flags(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(store.unit_flags(&id)?)))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FlagsBody {
    flags: Vec<String>,
    co_constructed_kind: Option<String>,
}

async fn set_flags(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<FlagsBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().set_unit_flags(
        &id,
        &unit_id,
        &body.flags,
        body.co_constructed_kind.as_deref(),
        &st.actor
    )?)))
}

async fn list_corrections(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
) -> ApiResult {
    Ok(Json(json!(st.store().unit_corrections(&id, &unit_id)?)))
}

#[derive(Deserialize)]
struct CorrectionBody {
    text: String,
    role: SpeakerRole,
    reason: String,
}

async fn add_correction(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<CorrectionBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().correct_unit(
        &id,
        &unit_id,
        &body.text,
        body.role,
        &body.reason,
        &st.actor
    )?)))
}

async fn verification_state(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!({
        "status": store.verification_status(&id)?,
        "targets": store.verification_targets(&id)?,
        "coverage": store.verification_coverage(&id)?,
    })))
}

#[derive(Deserialize)]
struct VerificationBody {
    state: String,
    #[serde(default)]
    note: Option<String>,
}

async fn set_verification(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<VerificationBody>,
) -> ApiResult {
    Ok(Json(json!(st.store().verify_unit(
        &id,
        &unit_id,
        &body.state,
        body.note.as_deref(),
        &st.actor
    )?)))
}

// ------------------------------------------------------- claims, memos, saturation

async fn list_claims(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(store.claims(&id)?)))
}

async fn list_unit_claims(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(store.unit_claims(&id, &unit_id)?)))
}

async fn add_claim(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(claim): Json<Claim>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .create_claim(&id, &claim, &st.actor)?)))
}

async fn edit_claim(
    State(st): State<AppState>,
    UrlPath(claim_id): UrlPath<i64>,
    Json(claim): Json<Claim>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .update_claim(claim_id, &claim, &st.actor)?)))
}

async fn remove_claim(State(st): State<AppState>, UrlPath(claim_id): UrlPath<i64>) -> ApiResult {
    st.store().delete_claim(claim_id, &st.actor)?;
    Ok(Json(json!({ "deleted": claim_id })))
}

async fn claims_review(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    let rows: Vec<Value> = store
        .claims_to_review(&id)?
        .into_iter()
        .map(|(claim_id, reason)| json!({ "claim_id": claim_id, "reason": reason }))
        .collect();
    Ok(Json(json!(rows)))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MemoBody {
    documented: String,
    interpretation: String,
    rivals: String,
    limits: String,
    units_for: Vec<String>,
    units_against: Vec<String>,
}

async fn get_rq_memo(State(st): State<AppState>, UrlPath(rq): UrlPath<String>) -> ApiResult {
    let memo = st
        .store()
        .rq_memo(&rq)?
        .ok_or_else(|| not_found(format!("no memo for research question {rq}")))?;
    Ok(Json(json!(memo)))
}

async fn put_rq_memo(
    State(st): State<AppState>,
    UrlPath(rq): UrlPath<String>,
    Json(body): Json<MemoBody>,
) -> ApiResult {
    let memo = RqMemo {
        rq_id: rq,
        documented: body.documented,
        interpretation: body.interpretation,
        rivals: body.rivals,
        limits: body.limits,
        units_for: body.units_for,
        units_against: body.units_against,
        updated_at: String::new(),
    };
    Ok(Json(json!(st.store().set_rq_memo(&memo, &st.actor)?)))
}

async fn saturation_table(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().saturation()?)))
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
    only_phase2(&phase)?;
    let batch = body.map(|b| b.0.batch_size).unwrap_or(0);
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let unit_ids = store.phase_units(&id, PHASE_2)?;
    if unit_ids.is_empty() {
        return Err(invalid(format!(
            "{id} has no unit in phase 2: give units a phase 1 category other than NA first"
        )));
    }
    let book = store.codebook()?;
    let codes: Vec<Code> = phase2_prompt_codes(&book).into_iter().cloned().collect();
    if codes.is_empty() {
        return Err(invalid(
            "no code can be offered to the AI; import or create codes first",
        ));
    }
    let parts = cs_ai::build_prompts(
        &cs_ai::PromptInput {
            seg: &seg,
            unit_ids: &unit_ids,
            codes: &codes,
        },
        batch,
    );
    Ok(Json(json!(store.record_prompts(&parts, &st.actor)?)))
}

async fn list_prompts(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    only_phase2(&phase)?;
    Ok(Json(json!(st.store().prompts(&id)?)))
}

async fn list_runs(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
) -> ApiResult {
    only_phase2(&phase)?;
    Ok(Json(json!(st.store().ai_runs(&id)?)))
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
    batch_id: Option<i64>,
}

async fn create_run(
    State(st): State<AppState>,
    UrlPath((id, phase)): UrlPath<(String, String)>,
    Json(body): Json<RunBody>,
) -> ApiResult {
    only_phase2(&phase)?;
    let meta = AiRunMeta {
        run_label: body.run_label,
        model_name: body.model_name,
        model_version: body.model_version,
        temperature: body.temperature,
        seed: body.seed,
        context_length: body.context_length,
        batch_size: body.batch_size,
        prompt_id_version: if body.prompt_id_version.trim().is_empty() {
            cs_ai::PROMPT_VERSION.to_string()
        } else {
            body.prompt_id_version
        },
        executed_at: if body.executed_at.trim().is_empty() {
            now()
        } else {
            body.executed_at
        },
        notes: body.notes,
        batch_id: body.batch_id,
    };
    let store = st.store();
    require_interview(&store, &id)?;
    let run_id = store.create_ai_run(&id, &meta, &st.actor)?;
    Ok(Json(json!(store.ai_run(run_id)?)))
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
    only_phase2(&phase)?;
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let run = store.ai_run(run_id)?;
    if run.interview_id != id {
        return Err(not_found(format!("run {run_id} of {id}")));
    }
    let parts: Vec<cs_model::PromptPart> = store
        .prompts(&id)?
        .into_iter()
        .filter(|p| p.batch_id == run.batch_id)
        .collect();
    let (unit_ids, codes) = cs_ai::batch_scope(&parts);
    let scope = cs_ai::AnswerScope {
        seg: &seg,
        unit_ids: &unit_ids,
        codes: &codes,
    };
    let parsed = cs_ai::parse_ai_output(&body.raw, &scope, body.part.zip(body.of))
        .map_err(ApiError::with_issues)?;
    store.record_ai_output(run_id, &body.raw, &parsed, &st.actor)?;
    Ok(Json(json!(store.ai_run(run_id)?)))
}

// ------------------------------------------------------------------ review

async fn review(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let r = store.review(&id)?;
    let units: BTreeMap<&str, &cs_model::Unit> =
        seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let p1 = store.human_coding_map(&id, PHASE_1)?;
    let book = store.codebook()?;
    let items: Vec<Value> = r
        .items
        .iter()
        .map(|e| {
            let mut v = json!(e);
            v["unit"] = json!(units.get(e.item.unit_id.as_str()));
            let ctx = p1.get(&e.item.unit_id).cloned().unwrap_or_default();
            v["allowed"] = json!(phase2_codes(&book, &ctx));
            v
        })
        .collect();
    Ok(Json(json!({
        "runs": r.runs,
        "items": items,
        "n_disagreements": r.n_disagreements,
        "n_sampled": r.n_sampled,
        "n_decided": r.n_decided,
        "seeds": r.seeds.iter().map(|(l, s)| json!({"run_label": l, "seed": s})).collect::<Vec<_>>(),
        "snapshots": r.snapshots,
    })))
}

async fn human_snapshots(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    Ok(Json(json!(store.human_snapshots(&id)?)))
}

#[derive(Deserialize)]
struct DecisionBody {
    choice: ArbitrationChoice,
    final_codes: CodeSet,
    #[serde(default)]
    rationale: String,
}

async fn decide(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Json(body): Json<DecisionBody>,
) -> ApiResult {
    let arb = Arbitration {
        unit_id,
        phase: PHASE_2,
        choice: body.choice,
        final_codes: body.final_codes,
        rationale: body.rationale,
        decided_by: st.actor.clone(),
        decided_at: String::new(),
    };
    Ok(Json(json!(st
        .store()
        .record_decision(&id, &arb, &st.actor)?)))
}

async fn final_coding(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    require_interview(&store, &id)?;
    let (finals, uncoded) = store.final_codings(&id)?;
    let r = store.review(&id)?;
    let open = r
        .items
        .iter()
        .filter(|e| e.item.kind == cs_model::ReviewKind::Disagreement && e.item.decision.is_none())
        .count();
    Ok(Json(json!({
        "finals": finals,
        "uncoded": uncoded,
        "n_decided": r.n_decided,
        "n_open_disagreements": open,
    })))
}

async fn agreement(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let seg = require_segmentation(&store, &id)?;
    let order = compared_units(&store, &id)?;
    let (snapshots, runs) = candidate_runs(&store, &id)?;
    let reports = agreement_reports(&snapshots, &runs, &order)?;
    let markdown = if reports.is_empty() {
        String::new()
    } else {
        cs_ai::render_agreement_md(&seg, &order, &reports)
    };
    Ok(Json(json!({ "reports": reports, "markdown": markdown })))
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

// ------------------------------------------------------------------ export

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

async fn export_cross_case(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let base = st
        .db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("exports");
    let result = export::write_cross_case(&store, &base, &st.db_path, &st.actor)?;
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
