//! User-selected model preparation. Raw input goes only to the configured endpoint,
//! redirects or environment proxies. Models propose; only the researcher commits.
use crate::{ApiResult, AppState};
use anyhow::{ensure, Result};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use cs_model::{
    pseudonym::{PseudonymEntry, Pseudonymiser},
    sha256_hex, CsError, Segmentation,
};
use cs_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::Duration,
};
macro_rules! ensure_api {
    ($condition:expr,$error:expr $(,)?) => {
        if !$condition {
            return Err(anyhow::Error::new($error).into());
        }
    };
}
const SETTINGS: &str = "local_preparation_config";
const PREFIX: &str = "local_preparation:";
const VERSION: &str = "local-transcript-preparation-1";
#[derive(Default, Serialize)]
struct Progress {
    stage: String,
    done: usize,
    total: usize,
    cancel_requested: bool,
}
#[derive(Default)]
struct Job {
    progress: Mutex<Progress>,
    cancel: AtomicBool,
}
static JOBS: OnceLock<Mutex<BTreeMap<String, Weak<Job>>>> = OnceLock::new();
fn jobs() -> &'static Mutex<BTreeMap<String, Weak<Job>>> {
    JOBS.get_or_init(Mutex::default)
}
pub(crate) fn cancel_project(db: &std::path::Path) {
    let prefix = format!("{}::", db.display());
    if let Ok(jobs) = jobs().lock() {
        for (key, job) in jobs.iter() {
            if key.starts_with(&prefix) {
                if let Some(job) = job.upgrade() {
                    job.cancel.store(true, Ordering::SeqCst);
                }
            }
        }
    }
}
fn job_key(st: &AppState, id: &str) -> String {
    format!("{}::{id}", st.db_path.display())
}
fn register_job(key: String) -> Result<Arc<Job>> {
    let mut jobs = jobs()
        .lock()
        .map_err(|_| invalid("État de préparation indisponible"))?;
    jobs.retain(|_, j| j.strong_count() > 0);
    ensure!(
        jobs.get(&key).and_then(Weak::upgrade).is_none(),
        CsError::Guard("Une préparation est déjà en cours pour cet entretien".into())
    );
    let job = Arc::new(Job::default());
    jobs.insert(key, Arc::downgrade(&job));
    Ok(job)
}
impl Job {
    fn update(&self, stage: &str, done: usize, total: usize) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::SeqCst),
            CsError::Guard(
                "Préparation arrêtée. Les sources et le codage restent inchangés.".into()
            )
        );
        if let Ok(mut p) = self.progress.lock() {
            p.stage = stage.into();
            p.done = done;
            p.total = total;
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Feedback {
    unit_id: String,
    instruction: String,
}
#[derive(Deserialize)]
struct PrepareBody {
    #[serde(flatten)]
    options: Options,
    #[serde(default = "all_stages")]
    stage: String,
    #[serde(default)]
    feedback: Vec<Feedback>,
    #[serde(default)]
    draft_digest: Option<String>,
    #[serde(default)]
    batch_entries: Vec<PseudonymEntry>,
    #[serde(default)]
    prompt_only: bool,
    #[serde(default)]
    pasted_response: Option<String>,
    #[serde(default)]
    prompt_digest: Option<String>,
}
fn all_stages() -> String {
    "all".into()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Options {
    pub url: String,
    pub model: String,
    #[serde(default, skip_serializing)]
    pub bearer_token: Option<String>,
    #[serde(default = "yes")]
    pub anonymise: bool,
    #[serde(default)]
    pub cleanup: bool,
    #[serde(default)]
    pub target_language: String,
    #[serde(default = "default_context")]
    pub context_tokens: usize,
    #[serde(default = "default_output_tokens")]
    pub output_tokens: usize,
    #[serde(default = "default_reasoning")]
    pub reasoning: String,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
}
// Secrets stay in process memory; never in drafts, SQLite settings or audit payloads.
static TOKENS: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
fn token_key(st: &AppState, url: &str) -> Result<String> {
    Ok(job_key(st, &endpoint(url)?))
}
fn session_token(st: &AppState, url: &str, supplied: Option<&str>) -> Result<Option<String>> {
    if let Some(token) = supplied {
        return validate_token(token);
    }
    let key = token_key(st, url)?;
    Ok(TOKENS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| invalid("Jeton local indisponible"))?
        .get(&key)
        .cloned())
}
fn validate_token(token: &str) -> Result<Option<String>> {
    let token = token.trim();
    ensure!(
        token.len() <= 8192 && token.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        CsError::Validation("Jeton Bearer invalide".into())
    );
    let token = token.strip_prefix("Bearer ").unwrap_or(token).trim();
    Ok((!token.is_empty()).then(|| token.to_owned()))
}
fn default_output_tokens() -> usize {
    32768
}
fn output_budget(o: &Options) -> usize {
    o.output_tokens.min(o.context_tokens / 2)
}
fn default_context() -> usize {
    131_072
}
fn default_reasoning_effort() -> String {
    "medium".into()
}
fn default_reasoning() -> String {
    "server".into()
}
fn yes() -> bool {
    true
}
fn invalid(msg: &str) -> anyhow::Error {
    CsError::Validation(msg.into()).into()
}
pub fn endpoint(base: &str) -> Result<String> {
    let mut u =
        url::Url::parse(base.trim()).map_err(|_| invalid("Adresse du modèle local invalide"))?;
    ensure!(
        matches!(u.scheme(), "http" | "https")
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none(),
        CsError::Validation("URL locale sans identifiants ni paramètres requise".into())
    );
    ensure!(
        u.host_str().is_some(),
        CsError::Validation("Hôte du serveur requis".into())
    );
    if u.path() == "/" {
        u.set_path("/v1");
    }
    Ok(u.as_str()
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .into())
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .new_agent()
}
fn completion_request(
    opts: &Options,
    system: &str,
    input: &Value,
    output_tokens: usize,
) -> Result<(String, Value)> {
    let base = endpoint(&opts.url)?;
    let input = serde_json::to_string(input)?;
    let mut body = json!({"model":opts.model,"temperature":0,"max_completion_tokens":output_tokens,"stream":false,"store":false,"messages":[{"role":"system","content":system},{"role":"user","content":input}]});
    match opts.reasoning.as_str() {
        "off" => body["reasoning_effort"] = json!("none"),
        "on" => body["reasoning_effort"] = json!(opts.reasoning_effort),
        _ => {}
    }
    Ok((format!("{base}/chat/completions"), body))
}
#[derive(Debug)]
struct OutputTruncated;
impl std::fmt::Display for OutputTruncated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Model output truncated")
    }
}
impl std::error::Error for OutputTruncated {}
fn complete(
    opts: &Options,
    system: &str,
    input: &Value,
    mut check: impl FnMut() -> Result<()>,
) -> Result<Value> {
    let input_tokens = (serde_json::to_vec(input)?.len() + system.len()).div_ceil(3) + 512;
    let available = (opts.context_tokens * 9 / 10).saturating_sub(input_tokens);
    let mut budget = output_budget(opts).min(available);
    ensure!(
        budget >= 256,
        CsError::Validation("Contexte insuffisant pour une réponse complète".into())
    );
    for attempt in 0..3 {
        check()?;
        match complete_with_budget(opts, system, input, budget) {
            Ok(value) => return Ok(value),
            Err(e) if e.downcast_ref::<OutputTruncated>().is_some() => {
                let larger = (budget * 2).min(available);
                if larger == budget || attempt == 2 {
                    return Err(invalid(&format!("Le serveur a tronqué la réponse malgré un budget de sortie de {budget} tokens. Augmentez sa limite de génération ou désactivez son thinking, puis reprenez ce résultat. Le mode prompt à copier reste disponible. Aucune modification appliquée.")));
                }
                budget = larger;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
fn complete_with_budget(
    opts: &Options,
    system: &str,
    input: &Value,
    output_tokens: usize,
) -> Result<Value> {
    let (endpoint, body) = completion_request(opts, system, input, output_tokens)?;
    let mut call = agent().post(endpoint);
    if let Some(token) = opts.bearer_token.as_deref() {
        if let Some(token) = validate_token(token)? {
            call = call.header("Authorization", format!("Bearer {token}"));
        }
    }
    let mut r=call.send_json(&body).map_err(|e|match e {ureq::Error::StatusCode(400|422)=>invalid("Le serveur refuse les paramètres OpenAI : vérifiez le modèle, max_completion_tokens et reasoning_effort (none ou l’effort choisi)."),_=>invalid("Le modèle ne répond pas : vérifiez l’URL, le Bearer, le modèle et sa capacité. Aucune bascule vers un service externe.")})?;
    ensure!(
        r.status().as_u16() == 200,
        CsError::Validation("Réponse locale refusée (redirection ou statut inattendu)".into())
    );
    let v: Value = r
        .body_mut()
        .with_config()
        .limit(4 * 1024 * 1024)
        .read_json()
        .map_err(|_| invalid("Réponse du serveur local illisible"))?;
    if v["choices"][0]["finish_reason"] == "length" {
        return Err(OutputTruncated.into());
    }
    let message = &v["choices"][0]["message"];
    ensure!(
        message["tool_calls"]
            .as_array()
            .is_none_or(|calls| calls.is_empty()),
        CsError::Validation("Les appels d’outils sont interdits pour la préparation".into())
    );
    let content=message["content"].as_str().ok_or_else(||invalid("Réponse Chat Completions vide : vérifiez la compatibilité OpenAI et les réglages de thinking du serveur"))?.trim();
    let content = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("``` "))
        .or_else(|| content.strip_prefix("```\n"))
        .unwrap_or(content)
        .trim()
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(content).map_err(|_| {
        invalid("Le modèle doit répondre en JSON complet. Aucune modification appliquée.")
    })
}
const DETECT: &str = include_str!("../../../docs/prompts/local-detect-identities.txt");
const TRANSFORM: &str = include_str!("../../../docs/prompts/local-transform-transcript.txt");
#[derive(Clone, Serialize, Deserialize)]
struct Draft {
    version: String,
    source_digest: String,
    #[serde(default)]
    source_entries: Option<Vec<PseudonymEntry>>,
    options: Options,
    original: Segmentation,
    anonymised: Segmentation,
    prepared: Segmentation,
    entries: Vec<PseudonymEntry>,
    reviewed: bool,
    #[serde(default)]
    accepted_variant: Option<String>,
    created_at: String,
    #[serde(default)]
    coding_will_be_preserved: bool,
    #[serde(default)]
    applied_to_coding: Option<bool>,
    #[serde(default = "all_stages")]
    stage: String,
    #[serde(default)]
    anonymisation_approved: bool,
    #[serde(default)]
    transformation_approved: bool,
    #[serde(default)]
    feedback: Vec<Feedback>,
    #[serde(default)]
    manual_prompt: Option<String>,
    #[serde(default)]
    manual_exchanges: Vec<Value>,
    #[serde(default)]
    pasted_response: Option<String>,
}
fn digest(seg: &Segmentation, entries: &[PseudonymEntry]) -> Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(&(VERSION, seg, entries))?))
}
// Accept only compatible additions from other reviewed interviews in a batch.
// Edits/removals of inherited rules, source edits and conflicting new rules stay stale.
fn source_matches(seg: &Segmentation, current: &[PseudonymEntry], draft: &Draft) -> Result<bool> {
    let Some(snapshot) = &draft.source_entries else {
        return Ok(digest(seg, current)? == draft.source_digest);
    };
    if digest(seg, snapshot)? != draft.source_digest
        || snapshot.iter().any(|e| !current.contains(e))
    {
        return Ok(false);
    }
    for e in current.iter().filter(|e| !snapshot.contains(e)) {
        let name = e.original.to_lowercase();
        let occurs = seg.units.iter().any(|u| {
            u.text.to_lowercase().contains(&name) || u.speaker_raw.to_lowercase().contains(&name)
        });
        if occurs && !draft.entries.contains(e) {
            return Ok(false);
        }
        if draft.entries.iter().any(|other| {
            other.original.eq_ignore_ascii_case(&e.original) && other.pseudonym != e.pseudonym
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn raw_seg(st: &AppState, id: &str) -> Result<Segmentation> {
    let store = st.store();
    if let Some(s) = store.corrected_segmentation(id)? {
        return Ok(s);
    }
    let entry = store
        .interview(id)?
        .ok_or_else(|| anyhow::Error::new(CsError::NotFound("Entretien introuvable".into())))?;
    let rel = entry
        .primary_source
        .as_deref()
        .ok_or_else(|| invalid("Choisissez une transcription source"))?;
    cs_ingest::build_segmentation_with(&crate::segmentation_config(&store, st)?, &entry, rel)
}
fn has_coding(store: &Store, id: &str) -> Result<bool> {
    let count:i64=store.connection().query_row("SELECT (SELECT COUNT(*) FROM human_codings WHERE interview_id=?1 AND (phase=2 OR COALESCE(modifier,'')!='auto'))+(SELECT COUNT(*) FROM passages WHERE interview_id=?1 AND deleted_at IS NULL)+(SELECT COUNT(*) FROM prompts WHERE interview_id=?1)",[id],|r|r.get(0))?;
    Ok(count > 0)
}
fn valid_options(o: &Options) -> Result<()> {
    ensure!(
        (256..=262144).contains(&o.output_tokens),
        CsError::Validation("Budget de sortie attendu entre 256 et 262144 tokens".into())
    );
    endpoint(&o.url)?;
    ensure!(
        matches!(
            o.reasoning_effort.as_str(),
            "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        ),
        CsError::Validation("Effort attendu : minimal, low, medium, high, xhigh ou max".into())
    );
    ensure!(
        (8192..=1_048_576).contains(&o.context_tokens),
        CsError::Validation("Contexte attendu entre 8192 et 1048576 tokens".into())
    );
    ensure!(
        matches!(o.reasoning.as_str(), "server" | "on" | "off"),
        CsError::Validation("Thinking attendu : server, on ou off".into())
    );
    ensure!(
        !o.model.trim().is_empty() && o.model.len() < 256,
        CsError::Validation("Nom du modèle requis".into())
    );
    ensure!(
        o.anonymise || o.cleanup || !o.target_language.trim().is_empty(),
        CsError::Validation("Choisissez une opération".into())
    );
    ensure!(
        o.target_language.chars().count() <= 80,
        CsError::Validation("Langue cible trop longue".into())
    );
    Ok(())
}
/// Conservative estimate, not a tokenizer: UTF-8 bytes / 3, with 10% context
/// margin and a separate output reserve. Whole interviews are preferred.
fn unit_batches(
    units: &[cs_model::Unit],
    context: usize,
    output_tokens: usize,
    overhead_bytes: usize,
    transform: bool,
) -> Result<Vec<std::ops::Range<usize>>> {
    let available =
        (context * 9 / 10).saturating_sub(output_tokens + overhead_bytes.div_ceil(3) + 512);
    // A derivative repeats the text: leave room for translation expansion and JSON.
    let budget = if transform {
        available.min(output_tokens / 2)
    } else {
        available
    };
    let mut batches = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (i, unit) in units.iter().enumerate() {
        let cost = serde_json::to_vec(unit)?.len().div_ceil(3) + 1;
        ensure!(cost <= budget, CsError::Validation("Un tour dépasse le budget de la requête. Augmentez le contexte ou vérifiez sa segmentation.".into()));
        if used + cost > budget && i > start {
            batches.push(start..i);
            start = i;
            used = 0;
        }
        used += cost;
    }
    if start < units.len() {
        batches.push(start..units.len());
    }
    Ok(batches)
}
enum PreparationMode {
    Model,
    Prompt,
    Pasted(Value),
}
#[derive(Debug)]
struct PromptReady(String);
impl std::fmt::Display for PromptReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Prompt ready")
    }
}
impl std::error::Error for PromptReady {}
fn proposal(
    mode: &PreparationMode,
    opts: &Options,
    system: &str,
    input: &Value,
    check: impl FnMut() -> Result<()>,
) -> Result<Value> {
    match mode {
        PreparationMode::Model=>complete(opts,system,input,check),
        PreparationMode::Pasted(v)=>Ok(v.clone()),
        PreparationMode::Prompt=>Err(PromptReady(format!("{}\n\nProcess the COMPLETE interview below in a single response. Return only the complete JSON object; do not truncate it or omit any required item.\n\n<research_data>\n{}\n</research_data>",system.replace("This request runs only on the researcher's explicitly selected local server. ",""),serde_json::to_string_pretty(input)?)).into()),
    }
}
fn pasted_json(raw: &str) -> Result<Value> {
    ensure!(
        raw.len() <= 8 * 1024 * 1024,
        CsError::Validation("Réponse collée trop volumineuse".into())
    );
    let raw = raw
        .trim()
        .strip_prefix("```json")
        .or_else(|| raw.trim().strip_prefix("```"))
        .unwrap_or(raw.trim())
        .trim()
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(raw).map_err(|_| {
        invalid("Collez une réponse JSON complète. Aucun résultat partiel n’a été appliqué.")
    })
}
fn prepare_model(
    original: Segmentation,
    existing: Vec<PseudonymEntry>,
    opts: Options,
    feedback: Vec<Feedback>,
    reserved_labels: Vec<String>,
    mode: PreparationMode,
    mut progress: impl FnMut(&str, usize, usize) -> Result<()>,
) -> Result<Draft> {
    if matches!(mode, PreparationMode::Model) {
        valid_options(&opts)?;
    }
    let source_digest = digest(&original, &existing)?;
    let mut completed_groups = 0;
    let source_entries = Some(existing.clone());
    let mut entries = existing;
    let mut used: BTreeSet<String> = entries.iter().map(|e| e.pseudonym.clone()).collect();
    used.extend(reserved_labels);
    let mut n = 1;
    if opts.anonymise {
        let mut offset = 0;
        let mut index = 0;
        while offset < original.units.len() {
            let ranges = if !matches!(mode, PreparationMode::Model) {
                std::iter::once(0..original.units.len() - offset).collect()
            } else {
                unit_batches(
                    &original.units[offset..],
                    opts.context_tokens,
                    output_budget(&opts),
                    DETECT.len()
                        + serde_json::to_vec(&entries)?.len()
                        + serde_json::to_vec(&feedback)?.len(),
                    false,
                )?
            };
            let end = offset + ranges[0].end;
            let chunk = &original.units[offset..end];
            progress("anonymisation", index, index + ranges.len())?;
            let v = proposal(
                &mode,
                &opts,
                DETECT,
                &json!({"units":chunk,"existing_replacements":entries,"researcher_feedback":feedback,"task":"Detect personal names only; preserve every other character by returning spans, not rewritten text."}),
                || progress("anonymisation", index, index + ranges.len()),
            )?;
            let list = v["entities"]
                .as_array()
                .ok_or_else(|| invalid("Le modèle doit retourner entities: []"))?;
            for e in list {
                let name = e["original"]
                    .as_str()
                    .ok_or_else(|| invalid("Entité sans forme originale"))?
                    .trim();
                ensure!(
                    !name.is_empty()
                        && name.chars().count() <= 200
                        && chunk
                            .iter()
                            .any(|u| u.text.contains(name) || u.speaker_raw.contains(name)),
                    CsError::Validation(
                        "Entité inventée ou absente de son extrait : proposition refusée".into()
                    )
                );
                if entries
                    .iter()
                    .any(|x| x.original.eq_ignore_ascii_case(name))
                {
                    continue;
                }
                let same = e["same_as"].as_str().filter(|s| !s.is_empty());
                let label = if let Some(s) = same {
                    entries
                        .iter()
                        .find(|x| x.original == s)
                        .map(|x| x.pseudonym.clone())
                        .ok_or_else(|| invalid("Identité de référence inconnue"))?
                } else {
                    while used.contains(&format!("[Person {n}]")) {
                        n += 1;
                    }
                    let label = format!("[Person {n}]");
                    used.insert(label.clone());
                    n += 1;
                    label
                };
                entries.push(PseudonymEntry {
                    kind: "person".into(),
                    original: name.into(),
                    pseudonym: label,
                });
            }
            offset = end;
            index += 1;
            completed_groups += 1;
        }
    }
    let rules = Pseudonymiser::new(&entries);
    ensure!(
        entries
            .iter()
            .all(|e| !rules.contains_original(&e.pseudonym)),
        CsError::Validation("Un label contient une identité : proposition refusée".into())
    );
    let mut anonymised = original.clone();
    if opts.anonymise {
        for u in &mut anonymised.units {
            u.text = crate::anonymisation::mask_with(&rules, &u.text);
            u.speaker_raw = crate::anonymisation::mask_with(&rules, &u.speaker_raw);
        }
    }
    anonymised.hash = Segmentation::units_hash(&anonymised.units);
    let mut prepared = anonymised.clone();
    if opts.cleanup || !opts.target_language.trim().is_empty() {
        let ranges = if !matches!(mode, PreparationMode::Model) {
            std::iter::once(0..prepared.units.len()).collect()
        } else {
            unit_batches(
                &prepared.units,
                opts.context_tokens,
                output_budget(&opts),
                TRANSFORM.len() + opts.target_language.len() + serde_json::to_vec(&feedback)?.len(),
                true,
            )?
        };
        for (index, range) in ranges.iter().enumerate() {
            let chunk = &mut prepared.units[range.clone()];
            progress("transformation", index, ranges.len())?;
            let v = proposal(
                &mode,
                &opts,
                TRANSFORM,
                &json!({"cleanup":opts.cleanup,"target_language":opts.target_language,"units":chunk,"researcher_feedback":feedback}),
                || progress("transformation", index, ranges.len()),
            )?;
            let returned = v["units"]
                .as_array()
                .ok_or_else(|| invalid("Le modèle doit retourner units: []"))?;
            ensure!(
                returned.len() == chunk.len(),
                CsError::Validation(
                    "Le modèle a supprimé ou ajouté des tours : proposition refusée".into()
                )
            );
            for (u, item) in chunk.iter_mut().zip(returned) {
                ensure!(
                    item["unit_id"] == u.unit_id,
                    CsError::Validation("Ordre ou identifiant de tour modifié".into())
                );
                let text = item["text"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| invalid("Un tour ne peut pas être supprimé"))?;
                check_fidelity(&u.text, text)?;
                u.text = text.into();
            }
            completed_groups += 1;
        }
    }
    // A model transform can invent an unmasked known identity; never accept it.
    if opts.anonymise {
        for u in &mut prepared.units {
            u.text = crate::anonymisation::mask_with(&rules, &u.text);
        }
    }
    progress("complete", completed_groups, completed_groups)?;
    prepared.hash = Segmentation::units_hash(&prepared.units);
    prepared.rule = format!(
        "{VERSION}; local model {}; cleanup={}; language={}",
        opts.model, opts.cleanup, opts.target_language
    );
    Ok(Draft {
        version: VERSION.into(),
        source_digest,
        source_entries,
        options: opts,
        original,
        anonymised,
        prepared,
        entries,
        reviewed: false,
        accepted_variant: None,
        created_at: crate::now(),
        coding_will_be_preserved: false,
        applied_to_coding: None,
        stage: all_stages(),
        anonymisation_approved: false,
        transformation_approved: false,
        feedback,
        manual_prompt: None,
        manual_exchanges: vec![],
        pasted_response: None,
    })
}
fn check_fidelity(before: &str, after: &str) -> Result<()> {
    let labels = regex::Regex::new(r"\[[^\]\r\n]+\]").unwrap();
    let collect = |s: &str| {
        let mut v: Vec<_> = labels
            .find_iter(s)
            .map(|x| x.as_str().to_string())
            .collect();
        v.sort();
        v
    };
    ensure!(
        collect(before) == collect(after),
        CsError::Validation("Les labels anonymes ont été modifiés ou supprimés".into())
    );
    let numbers = regex::Regex::new(r"\d+(?:[.,]\d+)*").unwrap();
    let num = |s: &str| {
        let mut v: Vec<_> = numbers
            .find_iter(s)
            .map(|x| x.as_str().replace(',', "."))
            .collect();
        v.sort();
        v
    };
    ensure!(
        num(before) == num(after),
        CsError::Validation(
            "Des nombres ont changé : proposition refusée pour préserver les faits".into()
        )
    );
    Ok(())
}
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/interviews/{id}/local-preparation/progress",
            get(progress_view),
        )
        .route(
            "/api/interviews/{id}/local-preparation/cancel",
            post(cancel),
        )
        .route(
            "/api/local-preparation/config",
            get(get_options).put(save_options),
        )
        .route("/api/local-preparation/models", post(models))
        .route(
            "/api/interviews/{id}/local-preparation",
            get(get_draft).post(prepare),
        )
        .route("/api/interviews/{id}/local-preparation/apply", post(apply))
        .route(
            "/api/interviews/{id}/local-preparation/review",
            post(accept_stage),
        )
}
async fn progress_view(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    let job = jobs()
        .lock()
        .ok()
        .and_then(|m| m.get(&job_key(&st, &id)).and_then(Weak::upgrade));
    Ok(Json(
        job.and_then(|j| j.progress.lock().ok().map(|p| json!(&*p)))
            .unwrap_or(Value::Null),
    ))
}
async fn cancel(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    if let Some(job) = jobs()
        .lock()
        .ok()
        .and_then(|m| m.get(&job_key(&st, &id)).and_then(Weak::upgrade))
    {
        job.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut p) = job.progress.lock() {
            p.cancel_requested = true;
        }
    }
    Ok(Json(json!({"cancel_requested":true})))
}
async fn get_options(State(st): State<AppState>) -> ApiResult {
    let value = st
        .store()
        .setting(SETTINGS)?
        .map(|s| serde_json::from_str::<Options>(&s))
        .transpose()
        .map_err(anyhow::Error::from)?;
    let mut result=value.map(|o| json!(o)).unwrap_or(json!({"url":"http://127.0.0.1:1234","model":"","anonymise":true,"cleanup":false,"target_language":"","context_tokens":default_context(),"output_tokens":default_output_tokens(),"reasoning":default_reasoning(),"reasoning_effort":default_reasoning_effort()}));
    result["has_bearer_token"] = json!(session_token(
        &st,
        result["url"].as_str().unwrap_or("http://127.0.0.1:1234"),
        None
    )?
    .is_some());
    Ok(Json(result))
}
async fn save_options(State(st): State<AppState>, Json(o): Json<Options>) -> ApiResult {
    valid_options(&o)?;
    if let Some(token) = &o.bearer_token {
        let key = token_key(&st, &o.url)?;
        let mut tokens = TOKENS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| invalid("Jeton local indisponible"))?;
        if let Some(token) = validate_token(token)? {
            tokens.insert(key, token);
        } else {
            tokens.remove(&key);
        }
    }
    let value = serde_json::to_string(&o).map_err(anyhow::Error::from)?;
    st.store().set_setting(SETTINGS, &value, &st.actor)?;
    Ok(Json(json!(o)))
}
#[derive(Deserialize)]
struct Models {
    url: String,
    #[serde(default)]
    bearer_token: Option<String>,
}
async fn models(State(st): State<AppState>, Json(body): Json<Models>) -> ApiResult {
    let endpoint = format!("{}/models", endpoint(&body.url)?);
    let token = session_token(&st, &body.url, body.bearer_token.as_deref())?;
    let list=tokio::task::spawn_blocking(move||->Result<Value>{let mut call=agent().get(endpoint);if let Some(token)=token {call=call.header("Authorization",format!("Bearer {token}"));}let mut r=call.call().map_err(|_|invalid("Serveur local inaccessible"))?;ensure!(r.status().as_u16()==200,CsError::Validation("Réponse locale inattendue".into()));let v:Value=r.body_mut().with_config().limit(1024*1024).read_json()?;Ok(json!({"models":v["data"].as_array().ok_or_else(||invalid("Liste de modèles invalide"))?.iter().filter_map(|m|m["id"].as_str()).collect::<Vec<_>>()}))}).await.map_err(|_|invalid("Le test local a échoué"))??;
    Ok(Json(list))
}
fn draft_view(d: &Draft) -> Result<Value> {
    let mut v = json!(d);
    v["draft_digest"] = json!(sha256_hex(serde_json::to_string(d)?.as_bytes()));
    Ok(v)
}
async fn get_draft(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    let d = st
        .store()
        .setting(&format!("{PREFIX}{id}"))?
        .map(|s| serde_json::from_str::<Draft>(&s))
        .transpose()
        .map_err(anyhow::Error::from)?;
    Ok(Json(match d {
        Some(d) => draft_view(&d)?,
        None => Value::Null,
    }))
}
async fn prepare(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PrepareBody>,
) -> ApiResult {
    let mut opts = body.options;
    let manual = body.prompt_only || body.pasted_response.is_some();
    if manual {
        opts.bearer_token = None;
        opts.model = "manual-prompt".into();
        ensure_api!(
            matches!(body.stage.as_str(), "anonymisation" | "transformation"),
            CsError::Validation("Choisissez une étape pour le prompt manuel".into())
        );
        ensure_api!(
            opts.anonymise || opts.cleanup || !opts.target_language.trim().is_empty(),
            CsError::Validation("Choisissez une opération".into())
        );
        ensure_api!(
            opts.target_language.chars().count() <= 80,
            CsError::Validation("Langue cible trop longue".into())
        );
    } else {
        opts.bearer_token = session_token(&st, &opts.url, opts.bearer_token.as_deref())?;
        valid_options(&opts)?;
    }
    ensure_api!(
        matches!(
            body.stage.as_str(),
            "all" | "anonymisation" | "transformation"
        ),
        CsError::Validation("Étape de préparation invalide".into())
    );
    ensure_api!(
        body.batch_entries.len() <= 20000,
        CsError::Validation("Lot de correspondances trop volumineux".into())
    );
    ensure_api!(
        body.batch_entries
            .iter()
            .all(|e| !e.original.trim().is_empty()
                && e.original.chars().count() <= 200
                && !e.pseudonym.trim().is_empty()
                && e.pseudonym.len() <= 200),
        CsError::Validation("Correspondance du lot invalide".into())
    );
    let original = raw_seg(&st, &id)?;
    ensure_api!(
        body.feedback.len() <= 100
            && body
                .feedback
                .iter()
                .all(|f| f.instruction.chars().count() <= 2000
                    && original.units.iter().any(|u| u.unit_id == f.unit_id)),
        CsError::Validation("Commentaires limités aux segments de cet entretien".into())
    );
    let (existing, prior) = {
        let store = st.store();
        (
            crate::anonymisation::entries(&store)?,
            store
                .setting(&format!("{PREFIX}{id}"))?
                .map(|s| serde_json::from_str::<Draft>(&s))
                .transpose()
                .map_err(anyhow::Error::from)?,
        )
    };
    if !body.feedback.is_empty() || (body.stage == "transformation" && opts.anonymise) {
        let d = prior
            .as_ref()
            .ok_or_else(|| invalid("Relisez d’abord la proposition d’anonymisation"))?;
        ensure_api!(
            !d.reviewed
                && body.draft_digest.as_deref()
                    == Some(&sha256_hex(
                        serde_json::to_string(d)
                            .map_err(anyhow::Error::from)?
                            .as_bytes()
                    )),
            CsError::Guard("La proposition a changé : rechargez la comparaison".into())
        );
        ensure_api!(
            source_matches(&original, &existing, d)?,
            CsError::Guard("Source ou règles modifiées : recommencez la préparation".into())
        );
        if body.stage == "transformation" && opts.anonymise {
            ensure_api!(
                d.anonymisation_approved,
                CsError::Guard(
                    "Validez les différences d’anonymisation avant le nettoyage ou la traduction"
                        .into()
                )
            );
        }
    }
    let prompt_digest = sha256_hex(
        &serde_json::to_vec(&(
            &original,
            &existing,
            &opts,
            &body.stage,
            &body.feedback,
            &body.draft_digest,
        ))
        .map_err(anyhow::Error::from)?,
    );
    let prompt_key = format!("{PREFIX}prompt:{id}");
    let mut manual_prompt = None;
    if body.pasted_response.is_some() {
        let stored: Value = serde_json::from_str(
            &st.store()
                .setting(&prompt_key)?
                .ok_or_else(|| invalid("Générez d’abord le prompt"))?,
        )
        .map_err(anyhow::Error::from)?;
        ensure_api!(
            body.prompt_digest.as_deref() == Some(&prompt_digest)
                && stored["prompt_digest"] == prompt_digest,
            CsError::Guard("Source, réglages ou proposition modifiés : régénérez le prompt".into())
        );
        manual_prompt = stored["prompt"].as_str().map(str::to_owned);
    }
    let history = if body.draft_digest.is_some() {
        prior
            .as_ref()
            .map(|d| d.manual_exchanges.clone())
            .unwrap_or_default()
    } else {
        vec![]
    };
    let raw_response = body.pasted_response.clone();
    let mode = if body.prompt_only {
        PreparationMode::Prompt
    } else if let Some(raw) = &body.pasted_response {
        PreparationMode::Pasted(pasted_json(raw)?)
    } else {
        PreparationMode::Model
    };
    let stage = body.stage;
    let feedback = body.feedback;
    let job = register_job(job_key(&st, &id))?;
    let worker = job.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut operation = opts.clone();
        let source = if stage == "transformation" && opts.anonymise {
            prior.as_ref().unwrap().anonymised.clone()
        } else {
            original.clone()
        };
        if stage == "anonymisation" {
            operation.cleanup = false;
            operation.target_language.clear();
            operation.anonymise = true;
        }
        if stage == "transformation" {
            operation.anonymise = false;
        }
        let mut entries = if stage == "transformation" && opts.anonymise {
            prior.as_ref().unwrap().entries.clone()
        } else {
            existing.clone()
        };
        let reserved_labels = body
            .batch_entries
            .iter()
            .map(|e| e.pseudonym.clone())
            .collect();
        if stage == "anonymisation" {
            for e in &body.batch_entries {
                let name = e.original.to_lowercase();
                if !entries
                    .iter()
                    .any(|old| old.original.eq_ignore_ascii_case(&e.original))
                    && original.units.iter().any(|u| {
                        u.text.to_lowercase().contains(&name)
                            || u.speaker_raw.to_lowercase().contains(&name)
                    })
                {
                    entries.push(e.clone());
                }
            }
        }
        let mut d = prepare_model(
            source,
            entries,
            operation,
            feedback,
            reserved_labels,
            mode,
            |stage, done, total| worker.update(stage, done, total),
        )?;
        d.options = opts;
        d.manual_prompt = manual_prompt;
        d.pasted_response = raw_response;
        d.manual_exchanges = history;
        if let (Some(prompt), Some(response)) = (&d.manual_prompt, &d.pasted_response) {
            d.manual_exchanges
                .push(json!({"stage":stage,"prompt":prompt,"response":response}));
        }
        if manual {
            d.prepared.rule = "manual prompt response; researcher review required".into();
        }
        d.stage = stage;
        d.source_digest = digest(&original, &existing)?;
        d.source_entries = Some(existing.clone());
        if d.stage == "transformation" {
            d.original = original;
            if let Some(previous) = prior.filter(|_| d.options.anonymise) {
                d.anonymised = previous.anonymised;
                d.entries = previous.entries;
                d.anonymisation_approved = previous.anonymisation_approved;
            }
        }
        Ok::<_, anyhow::Error>(d)
    })
    .await
    .map_err(|_| invalid("Préparation locale interrompue"))?;
    let mut d = match result {
        Ok(d) => d,
        Err(e) => {
            if let Some(prompt) = e.downcast_ref::<PromptReady>() {
                let package = json!({"prompt":prompt.0,"prompt_digest":prompt_digest});
                st.store()
                    .set_setting(&prompt_key, &package.to_string(), &st.actor)?;
                return Ok(Json(package));
            }
            return Err(e.into());
        }
    };
    ensure_api!(
        !job.cancel.load(Ordering::SeqCst),
        CsError::Guard("Préparation arrêtée sans modification des sources".into())
    );
    let current = raw_seg(&st, &id)?;
    let store = st.store();
    d.coding_will_be_preserved = has_coding(&store, &id)?;
    ensure_api!(
        source_matches(&current, &crate::anonymisation::entries(&store)?, &d)?,
        CsError::Guard(
            "La transcription ou les règles ont changé pendant la préparation : relancez-la".into()
        )
    );
    store.set_setting(
        &format!("{PREFIX}{id}"),
        &serde_json::to_string(&d).map_err(anyhow::Error::from)?,
        &st.actor,
    )?;
    Ok(Json(draft_view(&d)?))
}
#[derive(Deserialize)]
struct ReviewedUnit {
    unit_id: String,
    text: String,
    speaker_raw: String,
}
#[derive(Deserialize)]
struct StageReview {
    digest: String,
    reviewed: bool,
    pending_diffs: usize,
    units: Vec<ReviewedUnit>,
}
async fn accept_stage(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StageReview>,
) -> ApiResult {
    ensure_api!(
        body.reviewed && body.pending_diffs == 0,
        CsError::Validation("Résolvez chaque différence avant de valider cette étape".into())
    );
    let current = raw_seg(&st, &id)?;
    let store = st.store();
    let key = format!("{PREFIX}{id}");
    let mut d: Draft = serde_json::from_str(
        &store
            .setting(&key)?
            .ok_or_else(|| invalid("Aucune proposition à relire"))?,
    )
    .map_err(anyhow::Error::from)?;
    ensure_api!(
        !d.reviewed
            && body.digest
                == sha256_hex(
                    serde_json::to_string(&d)
                        .map_err(anyhow::Error::from)?
                        .as_bytes()
                ),
        CsError::Guard("Proposition modifiée : rechargez les différences".into())
    );
    ensure_api!(
        source_matches(&current, &crate::anonymisation::entries(&store)?, &d)?,
        CsError::Guard("Source ou règles modifiées : relancez la préparation".into())
    );
    ensure_api!(
        matches!(d.stage.as_str(), "anonymisation" | "transformation"),
        CsError::Validation("Cette proposition ne relève pas du parcours par étapes".into())
    );
    let target = if d.stage == "anonymisation" {
        &mut d.anonymised
    } else {
        &mut d.prepared
    };
    ensure_api!(
        target.units.len() == body.units.len(),
        CsError::Validation("Tous les segments doivent être conservés".into())
    );
    for (u, reviewed) in target.units.iter_mut().zip(body.units) {
        ensure_api!(
            u.unit_id == reviewed.unit_id
                && !reviewed.text.trim().is_empty()
                && reviewed.text.len() <= 128000
                && reviewed.speaker_raw.len() <= 1000,
            CsError::Validation("Segment supprimé, déplacé ou trop volumineux".into())
        );
        if d.stage == "transformation" {
            ensure_api!(
                u.speaker_raw == reviewed.speaker_raw,
                CsError::Validation("Les locuteurs restent ceux de l’anonymisation validée".into())
            );
        }
        u.text = reviewed.text;
        u.speaker_raw = reviewed.speaker_raw;
    }
    target.hash = Segmentation::units_hash(&target.units);
    if d.stage == "anonymisation" {
        d.anonymisation_approved = true;
        d.transformation_approved = false;
        d.prepared = d.anonymised.clone();
        let inherited = crate::anonymisation::entries(&store)?;
        d.entries.retain(|e| {
            inherited.contains(e)
                || d.anonymised
                    .units
                    .iter()
                    .any(|u| u.text.contains(&e.pseudonym) || u.speaker_raw.contains(&e.pseudonym))
        });
    } else {
        d.transformation_approved = true;
    }
    store.set_setting(
        &key,
        &serde_json::to_string(&d).map_err(anyhow::Error::from)?,
        &st.actor,
    )?;
    Ok(Json(draft_view(&d)?))
}
#[derive(Deserialize)]
struct Apply {
    digest: String,
    reviewed: bool,
    #[serde(default)]
    use_prepared: bool,
}
async fn apply(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Apply>,
) -> ApiResult {
    ensure_api!(
        b.reviewed,
        CsError::Validation("Confirmez la relecture complète".into())
    );
    let current = raw_seg(&st, &id)?;
    let store = st.store();
    let preserve_coding = has_coding(&store, &id)?;
    let key = format!("{PREFIX}{id}");
    let mut d: Draft = serde_json::from_str(
        &store
            .setting(&key)?
            .ok_or_else(|| invalid("Aucune proposition à valider"))?,
    )
    .map_err(anyhow::Error::from)?;
    ensure_api!(
        !d.reviewed
            && b.digest
                == sha256_hex(
                    serde_json::to_string(&d)
                        .map_err(anyhow::Error::from)?
                        .as_bytes()
                ),
        CsError::Guard("Proposition modifiée ou déjà appliquée : rechargez la comparaison".into())
    );
    ensure_api!(
        source_matches(&current, &crate::anonymisation::entries(&store)?, &d)?,
        CsError::Guard("Source ou règles modifiées : relancez la préparation".into())
    );
    if d.stage != "all" {
        ensure_api!(
            !d.options.anonymise || d.anonymisation_approved,
            CsError::Guard("L’anonymisation doit être validée d’abord".into())
        );
        ensure_api!(
            d.transformation_approved
                || (!d.options.cleanup && d.options.target_language.trim().is_empty()),
            CsError::Guard("Validez les différences du nettoyage et de la traduction".into())
        );
    }
    let mut cfg: Value = serde_json::from_str(
        &store
            .setting("anonymisation_v1")?
            .unwrap_or_else(|| "{}".into()),
    )
    .map_err(anyhow::Error::from)?;
    if d.options.anonymise {
        let mut map: BTreeMap<String, PseudonymEntry> =
            serde_json::from_value::<Vec<PseudonymEntry>>(
                cfg.get("entries").cloned().unwrap_or(json!([])),
            )
            .map_err(anyhow::Error::from)?
            .into_iter()
            .map(|e| (e.original.to_lowercase(), e))
            .collect();
        let inherited = crate::anonymisation::entries(&store)?;
        for e in &d.entries {
            if !inherited.iter().any(|old| old == e) {
                map.insert(e.original.to_lowercase(), e.clone());
            }
        }
        cfg["entries"] = json!(map.into_values().collect::<Vec<_>>());
        cfg["reviewed"] = json!({});
        cfg["batches"] = json!({});
    }
    d.reviewed = true;
    d.applied_to_coding = Some(!preserve_coding);
    d.accepted_variant = Some(
        if b.use_prepared {
            "prepared"
        } else {
            "verbatim"
        }
        .into(),
    );
    let seg = if b.use_prepared {
        &d.prepared
    } else {
        &d.anonymised
    };
    let settings = [
        (
            "anonymisation_v1".into(),
            serde_json::to_string(&cfg).map_err(anyhow::Error::from)?,
        ),
        (key, serde_json::to_string(&d).map_err(anyhow::Error::from)?),
    ];
    if preserve_coding {
        store.save_preparation_copy(&id, &st.actor, &settings)?;
    } else {
        store.save_segmentation_with_settings(seg, &st.actor, &settings)?;
    }
    Ok(Json(
        json!({"applied":true,"coding_preserved":preserve_coding,"applied_to_coding":!preserve_coding,"anonymised":d.options.anonymise,"cleanup":b.use_prepared&&d.options.cleanup,"target_language":if b.use_prepared{&d.options.target_language}else{""},"original_retained":true,"ai_review_still_requires_approval":true}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_model::{SpeakerRole, Unit};
    fn synthetic() -> Segmentation {
        let units=vec![Unit{unit_id:"SYN-01-U001".into(),seq:1,speaker_raw:"INT".into(),role:SpeakerRole::Interviewer,timestamp:"00:01".into(),text:"Sophie Martin vous a aidé ?".into(),corrected:false},Unit{unit_id:"SYN-01-U002".into(),seq:2,speaker_raw:"RESP".into(),role:SpeakerRole::Respondent,timestamp:"00:05".into(),text:"Euh, oui, Sophie Martin m'a aidé. Nous avons 3 magasins, mais nous n'avons pas réduit les coûts.".into(),corrected:false}];
        Segmentation {
            interview_id: "SYN-01".into(),
            source_rel_path: "synthetic.txt".into(),
            source_sha256: "fictional".into(),
            rule: "synthetic".into(),
            hash: Segmentation::units_hash(&units),
            units,
            warnings: vec![],
        }
    }
    #[test]
    fn context_batches_prefer_whole_interviews_not_six_turns() {
        let mut units = synthetic().units;
        units = (0..100)
            .map(|i| {
                let mut u = units[i % 2].clone();
                u.unit_id = format!("SYN-U{i}");
                u
            })
            .collect();
        assert_eq!(
            unit_batches(&units, 131_072, 32768, 2000, false).unwrap(),
            vec![0..100]
        );
        for u in &mut units {
            u.text = "fictional text ".repeat(100);
        }
        let transform = unit_batches(&units, 131_072, 32768, 2000, true).unwrap();
        assert!(
            transform.len() > 1,
            "Output budget, not number of turns, bounds a derivative"
        );
        let small = unit_batches(&units, 8192, 4096, 2000, false).unwrap();
        let large = unit_batches(&units, 131_072, 32768, 2000, false).unwrap();
        assert!(small.len() > large.len());
        assert_eq!(large, vec![0..100]);
        assert_eq!(
            small.iter().flat_map(|r| r.clone()).collect::<Vec<_>>(),
            (0..100).collect::<Vec<_>>()
        );
    }
    #[test]
    fn explicit_thinking_uses_openai_reasoning_effort_and_completion_budget() {
        let mut opts: Options = serde_json::from_value(
            json!({"url":"https://cluster.example.org/v1","model":"test","target_language":""}),
        )
        .unwrap();
        assert_eq!(opts.output_tokens, 32768);
        let (url, body) =
            completion_request(&opts, "system", &json!({}), output_budget(&opts)).unwrap();
        assert!(url.ends_with("/v1/chat/completions"));
        assert!(body.get("reasoning_effort").is_none());
        assert_eq!(body["max_completion_tokens"], 32768);
        assert_eq!(body["store"], false);
        opts.reasoning = "off".into();
        let (_, body) =
            completion_request(&opts, "system", &json!({}), output_budget(&opts)).unwrap();
        assert_eq!(body["reasoning_effort"], "none");
        opts.reasoning = "on".into();
        for effort in ["minimal", "low", "medium", "high", "xhigh", "max"] {
            opts.reasoning_effort = effort.into();
            let (url, body) =
                completion_request(&opts, "system", &json!({}), output_budget(&opts)).unwrap();
            assert!(url.ends_with("/v1/chat/completions"));
            assert_eq!(body["reasoning_effort"], effort);
            assert!(body.get("reasoning").is_none());
        }
    }
    #[test]
    fn configured_dns_names_and_all_ip_addresses_are_accepted() {
        for u in [
            "http://localhost:1234",
            "http://127.0.0.1:1234/v1",
            "http://192.168.1.2/v1",
            "https://ai.cluster.example.org/v1",
            "http://cluster-ia:8080",
            "https://8.8.8.8/v1",
            "http://169.254.169.254/v1",
            "http://[::1]/v1",
            "http://[2606:4700:4700::1111]/v1",
        ] {
            assert!(endpoint(u).is_ok(), "{u}");
        }
        assert_eq!(
            endpoint("https://ai.cluster.example.org/custom/v1/chat/completions").unwrap(),
            "https://ai.cluster.example.org/custom/v1"
        );
        for u in [
            "http://user:secret@localhost/v1",
            "http://localhost/v1?token=x",
            "ftp://localhost/v1",
            "file:///tmp/model",
        ] {
            assert!(endpoint(u).is_err(), "{u}");
        }
    }
    #[test]
    fn transforms_cannot_drop_pseudonyms_or_change_numbers() {
        assert!(check_fidelity("[Person 1] has 3 stores.", "[Person 1] a 3 magasins.").is_ok());
        assert!(check_fidelity("[Person 1] has 3 stores.", "Someone has 3 stores.").is_err());
        assert!(check_fidelity("[Person 1] has 3 stores.", "[Person 1] has 4 stores.").is_err());
    }
    #[test]
    #[ignore = "Explicit live test only: fictional data sent to user-selected LM Studio server"]
    fn live_local_model_preserves_original_and_proposes_english_copy() {
        let d = prepare_model(
            synthetic(),
            vec![],
            Options {
                url: "http://127.0.0.1:1234".into(),
                model: "google/gemma-4-12b-qat".into(),
                bearer_token: None,
                anonymise: true,
                cleanup: true,
                target_language: "English".into(),
                context_tokens: default_context(),
                output_tokens: default_output_tokens(),
                reasoning: default_reasoning(),
                reasoning_effort: default_reasoning_effort(),
            },
            vec![],
            vec![],
            PreparationMode::Model,
            |_, _, _| Ok(()),
        )
        .unwrap();
        assert_eq!(d.original.units[1].text, synthetic().units[1].text);
        assert!(!d.anonymised.units[1].text.contains("Sophie"));
        assert!(d.prepared.units[1].text.contains('3'));
        assert!(d.prepared.units[1].text.contains("[Person"));
        assert!(!d.reviewed);
        println!("{}",serde_json::to_string_pretty(&json!({"original":d.original.units,"anonymised":d.anonymised.units,"prepared":d.prepared.units,"mapping":d.entries,"reviewed":d.reviewed})).unwrap());
    }
}
