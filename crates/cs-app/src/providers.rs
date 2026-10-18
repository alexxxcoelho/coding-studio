//! Model providers over HTTP: the registry and its stored configuration, the
//! isolation probe, the run of a prompt batch through a provider, and the
//! ChatGPT sign-in. The protocol rules live in `cs-ai` and `cs-store`; this
//! module only adapts them to the routes registered in `lib.rs`.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::anyhow;
use axum::extract::{Path as UrlPath, State};
use axum::Json;
use cs_ai::providers::oauth_flow::{ChatGptAuth, FileTokenStore, LoopbackCallback, UreqTransport};
use cs_ai::providers::{
    providers_from_config, Execution, IsolationReport, OAuthChatGptConfig,
    OpenAiCompatibleProvider, ProviderConfig, ProviderDescription, ProviderKind, ProviderRequest,
    ProviderResponse, ProvidersConfig, COPY_PASTE_ID,
};
use cs_model::{
    AiRunMeta, CsError, PromptPart, ProviderExecution, ProviderPartRecord, ProviderRecord,
    Segmentation, PHASE_2,
};
use cs_store::Store;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    invalid, not_found, now, require_interview, require_segmentation, ApiError, ApiResult, AppState,
};

/// Settings key of the stored configuration: `{"providers": [entry, ...]}`,
/// the entries exactly as the candidate wrote them. Never holds a secret: an
/// API key is named by its environment variable.
const PROVIDERS_KEY: &str = "providers_config";

/// How long a sign-in waits for the browser callback.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

// ------------------------------------------------------------ configuration

/// The stored configuration as written, `{"providers": [...]}`; an empty
/// list when nothing was stored yet.
fn stored_document(store: &Store) -> anyhow::Result<Value> {
    match store.setting(PROVIDERS_KEY)? {
        Some(text) => {
            serde_json::from_str(&text).map_err(|e| anyhow!("corrupt providers configuration: {e}"))
        }
        None => Ok(json!({ "providers": [] })),
    }
}

fn load_config(store: &Store) -> Result<ProvidersConfig, ApiError> {
    let doc = stored_document(store)?;
    serde_json::from_value(doc)
        .map_err(|e| ApiError::from(anyhow!("stored providers configuration is unreadable: {e}")))
}

fn entry_id(entry: &ProviderConfig) -> &str {
    match entry {
        ProviderConfig::Cli(c) => &c.id,
        ProviderConfig::OpenAiCompatible(c) => &c.id,
        ProviderConfig::OAuthChatGpt(c) => &c.id,
    }
}

fn kind_of(entry: &ProviderConfig) -> ProviderKind {
    match entry {
        ProviderConfig::Cli(_) => ProviderKind::Cli,
        ProviderConfig::OpenAiCompatible(_) => ProviderKind::OpenAiCompatible,
        ProviderConfig::OAuthChatGpt(_) => ProviderKind::OAuthChatGpt,
    }
}

/// Kind of the provider `id` (the reserved manual workflow included).
fn kind_by_id(cfg: &ProvidersConfig, id: &str) -> Result<ProviderKind, ApiError> {
    if id == COPY_PASTE_ID {
        return Ok(ProviderKind::CopyPaste);
    }
    cfg.providers
        .iter()
        .find(|e| entry_id(e) == id)
        .map(kind_of)
        .ok_or_else(|| not_found(format!("provider {id}")))
}

fn execution_of(e: &Execution) -> ProviderExecution {
    match e {
        Execution::Local => ProviderExecution::Local,
        Execution::Hosted { vendor, endpoint } => ProviderExecution::Hosted {
            vendor: vendor.clone(),
            endpoint: endpoint.clone(),
        },
    }
}

fn join_err(e: tokio::task::JoinError) -> ApiError {
    ApiError::from(anyhow!("provider thread failed: {e}"))
}

// ----------------------------------------------------------------- registry

/// Whether a command can be started: a path that exists, or a name found on
/// the PATH.
fn command_available(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(command).is_file()))
}

/// Whether a provider can be used now. The manual workflow always can. A
/// command must exist, an API key variable must be set, a ChatGPT login must
/// have a connected profile.
fn configured(entry: Option<&ProviderConfig>) -> bool {
    match entry {
        None => true,
        Some(ProviderConfig::Cli(c)) => command_available(&c.command),
        Some(ProviderConfig::OpenAiCompatible(c)) => {
            std::env::var(&c.api_key_env).is_ok_and(|v| !v.trim().is_empty())
        }
        Some(ProviderConfig::OAuthChatGpt(c)) => token_dir_opt(c).is_some_and(|dir| {
            oauth_session(dir)
                .auth
                .profiles()
                .is_ok_and(|ps| ps.iter().any(|p| p.connected))
        }),
    }
}

fn token_dir_opt(c: &OAuthChatGptConfig) -> Option<&str> {
    c.token_dir
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
}

/// `[{id, kind, description, configured, config}]`, the manual workflow
/// first, then the stored entries in the order given.
fn registry_listing(store: &Store) -> ApiResult {
    let doc = stored_document(store)?;
    let cfg = load_config(store)?;
    let rows: Vec<Value> = providers_from_config(&cfg)
        .iter()
        .map(|p| {
            let desc: ProviderDescription = p.describe();
            let entry = cfg.providers.iter().find(|e| entry_id(e) == p.id());
            let config = doc["providers"]
                .as_array()
                .and_then(|raw| raw.iter().find(|e| e["id"] == p.id()).cloned())
                .unwrap_or(Value::Null);
            json!({
                "id": desc.id,
                "kind": desc.kind,
                "description": desc,
                "configured": configured(entry),
                "config": config,
            })
        })
        .collect();
    Ok(Json(json!(rows)))
}

pub async fn list(State(st): State<AppState>) -> ApiResult {
    registry_listing(&st.store())
}

#[derive(Deserialize)]
pub struct PutBody {
    providers: Vec<Value>,
}

/// Validates the whole list, then stores it. Nothing is stored and nothing is
/// audited when one entry is refused.
pub async fn put(State(st): State<AppState>, Json(body): Json<PutBody>) -> ApiResult {
    let store = st.store();
    let before = stored_document(&store)?;
    let mut seen = BTreeSet::new();
    for (i, raw) in body.providers.iter().enumerate() {
        let n = i + 1;
        let entry: ProviderConfig = serde_json::from_value(raw.clone())
            .map_err(|e| invalid(format!("provider {n}: {e}")))?;
        let id = entry_id(&entry).to_string();
        if id.trim().is_empty() {
            return Err(invalid(format!(
                "provider {n}: the id is empty; give every provider an id"
            )));
        }
        if id != id.trim() {
            return Err(invalid(format!(
                "provider {n}: the id has spaces around it: {id:?}"
            )));
        }
        if id == COPY_PASTE_ID {
            return Err(invalid(format!(
                "provider {n}: {COPY_PASTE_ID} is reserved for the manual workflow"
            )));
        }
        if !seen.insert(id.clone()) {
            return Err(invalid(format!("duplicate provider id {id}")));
        }
        if let ProviderConfig::OpenAiCompatible(c) = &entry {
            OpenAiCompatibleProvider::new(c.clone())
                .map_err(|e| invalid(format!("provider {id}: {e}")))?;
        }
    }
    let after = json!({ "providers": body.providers });
    store.set_setting(PROVIDERS_KEY, &after.to_string(), &st.actor)?;
    store.log(
        &st.actor,
        "providers.update",
        None,
        None,
        json!({ "before": before, "after": after }),
    )?;
    registry_listing(&store)
}

// --------------------------------------------------------------------- probe

/// Runs the isolation probe of a command-line provider.
pub async fn probe(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let cfg = load_config(&st.store())?;
    let kind = kind_by_id(&cfg, &id)?;
    if kind != ProviderKind::Cli {
        return Err(invalid(format!(
            "the isolation probe does not apply to {id}: it checks command-line providers only"
        )));
    }
    let report: IsolationReport = tokio::task::spawn_blocking(move || {
        let registry = providers_from_config(&cfg);
        let provider = registry
            .iter()
            .find(|p| p.id() == id)
            .ok_or_else(|| anyhow!("provider {id} is not configured"))?;
        provider.probe_isolation()
    })
    .await
    .map_err(join_err)??;
    Ok(Json(json!(report)))
}

// ----------------------------------------------------------------- run batch

#[derive(Deserialize)]
pub struct RunBody {
    provider_id: String,
    run_label: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

/// What a run needs, read under the store lock and owned afterwards.
struct RunPlan {
    interview_id: String,
    cfg: ProvidersConfig,
    provider_id: String,
    label: String,
    batch_id: i64,
    parts: Vec<PromptPart>,
    seg: Segmentation,
}

/// Every check that needs no provider call: the interview, the provider, the
/// label, the prompt parts of the batch, the candidate's blind coding.
fn plan_run(st: &AppState, id: &str, batch_id: i64, body: &RunBody) -> Result<RunPlan, ApiError> {
    let store = st.store();
    let mut parts: Vec<PromptPart> = store
        .prompts(id)?
        .into_iter()
        .filter(|p| p.batch_id == batch_id)
        .collect();
    if parts.is_empty() {
        return Err(ApiError::from(anyhow::Error::new(CsError::Guard(format!(
            "batch {batch_id} has no prompt for {id}; generate the prompts first"
        )))));
    }
    require_interview(&store, id)?;
    let cfg = load_config(&store)?;
    let provider_id = body.provider_id.clone();
    if provider_id == COPY_PASTE_ID {
        return Err(invalid(format!(
            "{COPY_PASTE_ID} is the manual workflow: it runs no model, paste the answer instead"
        )));
    }
    kind_by_id(&cfg, &provider_id)?;
    let label = body.run_label.trim().to_string();
    if label.is_empty() {
        return Err(invalid("a run label is required (R1, R2, R3...)"));
    }
    parts.sort_by_key(|p| p.part);
    let seg = require_segmentation(&store, id)?;
    let codings = store.human_codings(id, PHASE_2)?;
    let uncoded = parts
        .iter()
        .flat_map(|p| p.unit_ids.iter())
        .filter(|u| !codings.contains_key(*u))
        .count();
    if uncoded > 0 {
        return Err(ApiError::from(anyhow::Error::new(CsError::Guard(format!(
            "blind coding guard: {uncoded} units of this batch are not yet coded by the candidate"
        )))));
    }
    if store.ai_runs(id)?.iter().any(|r| r.meta.run_label == label) {
        return Err(invalid(format!(
            "run label {label} is already used for {id}"
        )));
    }
    parts = crate::anonymisation::require_batch(&store, id, batch_id)?;
    Ok(RunPlan {
        interview_id: id.to_string(),
        cfg,
        provider_id,
        label,
        batch_id,
        parts,
        seg,
    })
}

/// Sends every part of the batch to the provider, then validates every answer
/// before anything is written: one refused answer refuses the run.
pub async fn run_batch(
    State(st): State<AppState>,
    UrlPath((id, batch_id)): UrlPath<(String, i64)>,
    Json(body): Json<RunBody>,
) -> ApiResult {
    let plan = plan_run(&st, &id, batch_id, &body)?;
    crate::v1_review::refuse_unmasked(&st.store(), &plan.parts)?;
    let requests: Vec<ProviderRequest> = plan
        .parts
        .iter()
        .map(|p| ProviderRequest {
            prompt: p.text.clone(),
            model: body.model.clone(),
            temperature: body.temperature,
            seed: body.seed,
            max_tokens: body.max_tokens,
        })
        .collect();
    let (cfg, provider_id) = (plan.cfg.clone(), plan.provider_id.clone());
    let (description, responses) = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(ProviderDescription, Vec<ProviderResponse>)> {
            let registry = providers_from_config(&cfg);
            let provider = registry
                .iter()
                .find(|p| p.id() == provider_id)
                .ok_or_else(|| anyhow!("provider {provider_id} is not configured"))?;
            let mut out = Vec::with_capacity(requests.len());
            for req in &requests {
                out.push(provider.complete(req)?);
            }
            Ok((provider.describe(), out))
        },
    )
    .await
    .map_err(join_err)??;
    finish_run(&st, &plan, &body, description, responses)
}

fn finish_run(
    st: &AppState,
    plan: &RunPlan,
    body: &RunBody,
    description: ProviderDescription,
    responses: Vec<ProviderResponse>,
) -> ApiResult {
    let (unit_ids, codes) = cs_ai::batch_scope(&plan.parts);
    let scope = cs_ai::AnswerScope {
        seg: &plan.seg,
        unit_ids: &unit_ids,
        codes: &codes,
    };
    let mut parsed: Vec<(String, cs_model::AiOutput)> = Vec::new();
    let mut issues = Vec::new();
    let mut rejected = Vec::new();
    for (part, resp) in plan.parts.iter().zip(&responses) {
        let checked = cs_ai::parse_ai_output(&resp.raw, &scope, Some((part.part, part.of)))
            .and_then(|out| {
                crate::v1_review::check_answer(&st.store(), &plan.interview_id, &out)
                    .map(|_| out)
                    .map_err(|e| e.issues.unwrap_or_default())
            });
        match checked {
            Ok(out) => parsed.push((resp.raw.clone(), out)),
            Err(found) => {
                issues.extend(found);
                rejected.push(json!({ "part": part.part, "raw": resp.raw }));
            }
        }
    }
    if !rejected.is_empty() {
        let mut err = ApiError::with_issues(issues);
        err.rejected = Some(rejected);
        return Err(err);
    }

    let first = &responses[0];
    let exposed = |name: &str| first.parameters_exposed.iter().any(|p| p == name);
    let record = ProviderRecord {
        id: plan.provider_id.clone(),
        execution: execution_of(&first.execution),
        parameters_exposed: first.parameters_exposed.clone(),
        parameters_not_exposed: first.parameters_not_exposed.clone(),
        parts: plan
            .parts
            .iter()
            .zip(&responses)
            .map(|(part, r)| ProviderPartRecord {
                part: part.part,
                request_sha256: r.request_sha256.clone(),
                started_at: r.started_at.clone(),
                finished_at: r.finished_at.clone(),
            })
            .collect(),
    };
    let meta = AiRunMeta {
        run_label: plan.label.clone(),
        model_name: first.model.clone(),
        model_version: String::new(),
        // Only what the provider could set is journalised as a run parameter.
        temperature: body
            .temperature
            .filter(|_| exposed("temperature"))
            .map(f64::from),
        seed: body.seed.filter(|_| exposed("seed")).map(|s| s.to_string()),
        context_length: None,
        batch_size: None,
        prompt_id_version: plan.parts[0].prompt_version.clone(),
        executed_at: now(),
        notes: None,
        batch_id: Some(plan.batch_id),
        provider: Some(record),
        pseudonymisation: crate::v1_review::run_record(&st.store(), &plan.parts[0].text)?,
    };

    let store = st.store();
    let run_id = store.create_ai_run(&plan.interview_id, &meta, &st.actor)?;
    for (raw, out) in &parsed {
        store.record_ai_output(run_id, raw, out, &st.actor)?;
    }
    let journal_line = description.journal_line();
    store.log(
        &st.actor,
        "ai_run.provider",
        Some(&plan.interview_id),
        Some(&plan.label),
        json!({
            "run_id": run_id,
            "provider_id": plan.provider_id,
            "journal_line": journal_line,
            "batch_id": plan.batch_id,
            "parts": plan.parts.len(),
        }),
    )?;
    Ok(Json(json!({
        "run": store.ai_run(run_id)?,
        "journal_line": journal_line,
    })))
}

// ------------------------------------------------------------------- sign-in

/// The sign-in state of one ChatGPT token directory.
#[derive(Default, Clone)]
struct SignIn {
    pending: bool,
    error: Option<String>,
}

/// One auth service per token directory, kept for the life of the process so
/// that a sign-in begun by one request is finished by the callback thread and
/// reported by the status route. Keyed by directory, so two providers over
/// the same tokens share one service.
struct OAuthSession {
    auth: ChatGptAuth,
    sign_in: Mutex<SignIn>,
}

fn oauth_session(token_dir: &str) -> Arc<OAuthSession> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<OAuthSession>>>> = OnceLock::new();
    let sessions = SESSIONS.get_or_init(Default::default);
    let mut sessions = sessions.lock().unwrap_or_else(|e| e.into_inner());
    sessions
        .entry(token_dir.to_string())
        .or_insert_with(|| {
            Arc::new(OAuthSession {
                auth: ChatGptAuth::new(
                    Box::new(UreqTransport::new()),
                    Box::new(FileTokenStore::new(token_dir)),
                ),
                sign_in: Mutex::default(),
            })
        })
        .clone()
}

/// The ChatGPT configuration of `id`; the sign-in applies to that provider only.
fn oauth_config(cfg: &ProvidersConfig, id: &str) -> Result<OAuthChatGptConfig, ApiError> {
    if id == COPY_PASTE_ID {
        return Err(invalid(format!(
            "the sign-in does not apply to {COPY_PASTE_ID}: it has no account"
        )));
    }
    match cfg.providers.iter().find(|e| entry_id(e) == id) {
        None => Err(not_found(format!("provider {id}"))),
        Some(ProviderConfig::OAuthChatGpt(c)) => Ok(c.clone()),
        Some(_) => Err(invalid(format!(
            "the sign-in does not apply to {id}: only the ChatGPT provider signs in"
        ))),
    }
}

fn token_dir_of(c: &OAuthChatGptConfig) -> Result<String, ApiError> {
    token_dir_opt(c).map(str::to_string).ok_or_else(|| {
        invalid(format!(
            "no token directory for {}: set token_dir in its configuration",
            c.id
        ))
    })
}

/// Starts a sign-in: a loopback listener on a free port, the authorization URL
/// for the browser, and a thread that waits for the callback and completes it.
pub async fn oauth_begin(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let config = oauth_config(&load_config(&st.store())?, &id)?;
    let session = oauth_session(&token_dir_of(&config)?);
    let (redirect, callback) = LoopbackCallback::listen(None)?;
    let authorization = session
        .auth
        .begin(&redirect, config.profile_id.as_deref())?;
    let state = authorization.state.clone();
    *session.sign_in.lock().unwrap_or_else(|e| e.into_inner()) = SignIn {
        pending: true,
        error: None,
    };
    let worker = session.clone();
    let expected = state.clone();
    std::thread::spawn(move || {
        let outcome = callback
            .wait_for_state(&expected, SIGN_IN_TIMEOUT)
            .and_then(|url| worker.auth.complete(&expected, &url));
        let mut sign_in = worker.sign_in.lock().unwrap_or_else(|e| e.into_inner());
        *sign_in = match outcome {
            Ok(_) => SignIn::default(),
            Err(e) => {
                worker.auth.cancel(&expected);
                SignIn {
                    pending: false,
                    error: Some(format!("{e:#}")),
                }
            }
        };
    });
    Ok(Json(json!({ "url": authorization.url, "state": state })))
}

/// Connection state of a ChatGPT provider: the connected profile, a sign-in
/// under way, or the error of the last one.
pub async fn oauth_status(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let config = oauth_config(&load_config(&st.store())?, &id)?;
    let session = oauth_session(&token_dir_of(&config)?);
    let profile = session.auth.profiles()?.into_iter().find(|p| p.connected);
    let sign_in = session
        .sign_in
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Ok(Json(json!({
        "connected": profile.is_some(),
        "pending": sign_in.pending,
        "profile": profile,
        "error": sign_in.error,
    })))
}
