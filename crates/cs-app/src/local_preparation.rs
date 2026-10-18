//! User-selected local model preparation. Raw input never goes to a public host,
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
    net::IpAddr,
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
pub struct Options {
    pub url: String,
    pub model: String,
    #[serde(default = "yes")]
    pub anonymise: bool,
    #[serde(default)]
    pub cleanup: bool,
    #[serde(default)]
    pub target_language: String,
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
    let host = u
        .host_str()
        .ok_or_else(|| invalid("Hôte local requis"))?
        .trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost") {
        u.set_host(Some("127.0.0.1"))?;
    } else {
        let ip: IpAddr = host
            .parse()
            .map_err(|_| invalid("Utilisez localhost ou une adresse IP locale explicite"))?;
        let local = match ip {
            IpAddr::V4(a) => a.is_loopback() || a.is_private(),
            IpAddr::V6(a) => a.is_loopback() || a.is_unique_local(),
        };
        ensure!(
            local,
            CsError::Validation(
                "Les services hébergés publics sont interdits pour les transcriptions originales"
                    .into()
            )
        );
    }
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
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .new_agent()
}
fn complete(opts: &Options, system: &str, input: &Value) -> Result<Value> {
    let endpoint = format!("{}/chat/completions", endpoint(&opts.url)?);
    let body = json!({"model":opts.model,"temperature":0,"max_tokens":4096,"stream":false,"messages":[{"role":"system","content":system},{"role":"user","content":serde_json::to_string(input)?}]});
    let mut r=agent().post(endpoint).send_json(&body).map_err(|_|invalid("Le modèle local ne répond pas : vérifiez l’URL, le modèle et sa capacité. Aucune bascule vers un service externe."))?;
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
    ensure!(
        v["choices"][0]["finish_reason"] != "length",
        CsError::Validation(
            "Réponse locale tronquée : utilisez un modèle avec une fenêtre de contexte plus grande"
                .into()
        )
    );
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| invalid("Réponse du modèle local vide"))?
        .trim();
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
    options: Options,
    original: Segmentation,
    anonymised: Segmentation,
    prepared: Segmentation,
    entries: Vec<PseudonymEntry>,
    reviewed: bool,
    #[serde(default)]
    accepted_variant: Option<String>,
    created_at: String,
}
fn digest(seg: &Segmentation, entries: &[PseudonymEntry]) -> Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(&(VERSION, seg, entries))?))
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
fn require_uncoded(store: &Store, id: &str) -> Result<()> {
    let c = store.connection();
    let count:i64=c.query_row("SELECT (SELECT COUNT(*) FROM human_codings WHERE interview_id=?1 AND (phase=2 OR COALESCE(modifier,'')!='auto'))+(SELECT COUNT(*) FROM passages WHERE interview_id=?1 AND deleted_at IS NULL)+(SELECT COUNT(*) FROM prompts WHERE interview_id=?1)",[id],|r|r.get(0))?;
    ensure!(count==0,CsError::Guard("Cet entretien a déjà été codé ou revu. Préparez une copie dans un nouveau projet pour préserver ses ancrages.".into()));
    Ok(())
}
fn valid_options(o: &Options) -> Result<()> {
    endpoint(&o.url)?;
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
fn prepare_model(
    original: Segmentation,
    existing: Vec<PseudonymEntry>,
    opts: Options,
    mut progress: impl FnMut(&str, usize, usize) -> Result<()>,
) -> Result<Draft> {
    valid_options(&opts)?;
    let source_digest = digest(&original, &existing)?;
    let total = original.units.len().div_ceil(6);
    let mut entries = existing;
    let mut used: BTreeSet<String> = entries.iter().map(|e| e.pseudonym.clone()).collect();
    let mut n = 1;
    if opts.anonymise {
        for (index, chunk) in original.units.chunks(6).enumerate() {
            progress("anonymisation", index, total)?;
            let v = complete(
                &opts,
                DETECT,
                &json!({"units":chunk,"existing_replacements":entries,"task":"Detect personal names only; preserve every other character by returning spans, not rewritten text."}),
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
        for (index, chunk) in prepared.units.chunks_mut(6).enumerate() {
            progress("transformation", index, total)?;
            let v = complete(
                &opts,
                TRANSFORM,
                &json!({"cleanup":opts.cleanup,"target_language":opts.target_language,"units":chunk}),
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
        }
    }
    // A model transform can invent an unmasked known identity; never accept it.
    if opts.anonymise {
        for u in &mut prepared.units {
            u.text = crate::anonymisation::mask_with(&rules, &u.text);
        }
    }
    progress("complete", total, total)?;
    prepared.hash = Segmentation::units_hash(&prepared.units);
    prepared.rule = format!(
        "{VERSION}; local model {}; cleanup={}; language={}",
        opts.model, opts.cleanup, opts.target_language
    );
    Ok(Draft {
        version: VERSION.into(),
        source_digest,
        options: opts,
        original,
        anonymised,
        prepared,
        entries,
        reviewed: false,
        accepted_variant: None,
        created_at: crate::now(),
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
        .map(|s| serde_json::from_str::<Value>(&s))
        .transpose()
        .map_err(anyhow::Error::from)?;
    Ok(Json(value.unwrap_or(json!({"url":"http://127.0.0.1:1234","model":"","anonymise":true,"cleanup":false,"target_language":""}))))
}
async fn save_options(State(st): State<AppState>, Json(o): Json<Options>) -> ApiResult {
    valid_options(&o)?;
    let value = serde_json::to_string(&o).map_err(anyhow::Error::from)?;
    st.store().set_setting(SETTINGS, &value, &st.actor)?;
    Ok(Json(json!(o)))
}
#[derive(Deserialize)]
struct Models {
    url: String,
}
async fn models(Json(body): Json<Models>) -> ApiResult {
    let endpoint = format!("{}/models", endpoint(&body.url)?);
    let list=tokio::task::spawn_blocking(move||->Result<Value>{let mut r=agent().get(endpoint).call().map_err(|_|invalid("Serveur local inaccessible"))?;ensure!(r.status().as_u16()==200,CsError::Validation("Réponse locale inattendue".into()));let v:Value=r.body_mut().with_config().limit(1024*1024).read_json()?;Ok(json!({"models":v["data"].as_array().ok_or_else(||invalid("Liste de modèles invalide"))?.iter().filter_map(|m|m["id"].as_str()).collect::<Vec<_>>()}))}).await.map_err(|_|invalid("Le test local a échoué"))??;
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
    Json(opts): Json<Options>,
) -> ApiResult {
    valid_options(&opts)?;
    let original = raw_seg(&st, &id)?;
    let existing = {
        let store = st.store();
        require_uncoded(&store, &id)?;
        crate::anonymisation::entries(&store)?
    };
    let job = register_job(job_key(&st, &id))?;
    let worker = job.clone();
    let d = tokio::task::spawn_blocking(move || {
        prepare_model(original, existing, opts, |stage, done, total| {
            worker.update(stage, done, total)
        })
    })
    .await
    .map_err(|_| invalid("Préparation locale interrompue"))??;
    ensure_api!(
        !job.cancel.load(Ordering::SeqCst),
        CsError::Guard("Préparation arrêtée sans modification des sources".into())
    );
    let current = raw_seg(&st, &id)?;
    let store = st.store();
    require_uncoded(&store, &id)?;
    ensure_api!(
        digest(&current, &crate::anonymisation::entries(&store)?)? == d.source_digest,
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
    require_uncoded(&store, &id)?;
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
        digest(&current, &crate::anonymisation::entries(&store)?)? == d.source_digest,
        CsError::Guard("Source ou règles modifiées : relancez la préparation".into())
    );
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
    store.save_segmentation_with_settings(
        seg,
        &st.actor,
        &[
            (
                "anonymisation_v1".into(),
                serde_json::to_string(&cfg).map_err(anyhow::Error::from)?,
            ),
            (key, serde_json::to_string(&d).map_err(anyhow::Error::from)?),
        ],
    )?;
    Ok(Json(
        json!({"applied":true,"anonymised":d.options.anonymise,"cleanup":b.use_prepared&&d.options.cleanup,"target_language":if b.use_prepared{&d.options.target_language}else{""},"original_retained":true,"ai_review_still_requires_approval":true}),
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
    fn only_explicit_local_destinations_are_accepted() {
        assert_eq!(
            endpoint("http://127.0.0.1:1234").unwrap(),
            "http://127.0.0.1:1234/v1"
        );
        assert_eq!(
            endpoint("http://localhost:1234/v1/").unwrap(),
            "http://127.0.0.1:1234/v1"
        );
        assert!(endpoint("http://[::1]:1234/v1").is_ok());
        assert!(endpoint("http://192.168.1.2:1234/v1").is_ok());
        for u in [
            "https://api.openai.com/v1",
            "http://127.0.0.1.evil.test/v1",
            "http://user:secret@localhost:1234/v1",
            "http://localhost:1234/v1?token=x",
            "http://169.254.169.254/v1",
            "ftp://127.0.0.1/v1",
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
                anonymise: true,
                cleanup: true,
                target_language: "English".into(),
            },
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
