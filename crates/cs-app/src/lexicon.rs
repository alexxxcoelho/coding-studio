//! V1 lot 3 routes: the lexical workbench (language, keyphrases of a passage,
//! similar passages, keyword table, keyword in context, term frequency per
//! attribute value). Thin adapters over `cs-store` and `cs-lex`; nothing here
//! calls a model. Documented in `docs/API.md`, "V1 lots 3 and 4".

use std::collections::BTreeSet;

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use cs_lex::{KeywordOptions, KwicQuery, Speakers};
use cs_model::CsError;
use cs_store::{LexiconConfig, SimilarOptions};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/lexicon/config", get(get_config).put(put_config))
        .route("/api/lexicon/status", get(status))
        .route("/api/lexicon/kwic", get(kwic))
        .route("/api/lexicon/keywords", get(keywords))
        .route("/api/lexicon/term-frequency", get(term_frequency))
        .route("/api/passages/{pid}/lexical", get(passage_panel))
        .route("/api/passages/{pid}/similar", get(passage_similar))
        .route(
            "/api/interviews/{id}/units/{unit_id}/similar",
            get(unit_similar),
        )
}

fn invalid(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Validation(msg.into()))
}

fn speakers(raw: &Option<String>) -> Result<Speakers, anyhow::Error> {
    match raw.as_deref() {
        None | Some("") => Ok(Speakers::Respondent),
        Some(s) => Speakers::parse(s)
            .ok_or_else(|| invalid("speakers must be respondent, interviewer or all")),
    }
}

/// The interviews a request is limited to: an interview, an attribute value,
/// or both (intersection). `None` for the whole corpus.
fn limit_to(
    store: &cs_store::Store,
    interview_id: &Option<String>,
    attribute: &Option<String>,
    value: &Option<String>,
) -> Result<Option<BTreeSet<String>>, anyhow::Error> {
    let by_attr = store.interview_filter(attribute.as_deref(), value.as_deref())?;
    let by_id: Option<BTreeSet<String>> = match interview_id.as_deref() {
        None | Some("") => None,
        Some(id) => {
            let known = store.interview(id)?.is_some();
            if !known {
                return Err(anyhow::Error::new(CsError::NotFound(format!(
                    "interview {id}"
                ))));
            }
            Some([id.to_string()].into())
        }
    };
    Ok(match (by_attr, by_id) {
        (Some(a), Some(b)) => Some(a.intersection(&b).cloned().collect()),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    })
}

// ----------------------------------------------------------- configuration

async fn get_config(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().lexicon_config()?)))
}

async fn put_config(State(st): State<AppState>, Json(body): Json<LexiconConfig>) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .set_lexicon_config(&body, &st.actor)?)))
}

/// Language, size of the index and the limits of the method.
async fn status(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let idx = store.lexical_index()?;
    Ok(Json(json!({
        "config": store.lexicon_config()?,
        "languages": ["fr", "en", "de"],
        "index": idx.stats(),
        "fingerprint": store.lexical_fingerprint()?,
        "method": {
            "tokens": "mots, élisions françaises séparées, tirets séparés",
            "stems": "Snowball (fr, en, de)",
            "keyphrases": "YAKE, statistique et déterministe",
            "similarity": "TF-IDF sur les racines des tours de l'enquêté, cosinus",
            "ai": false,
        },
        "note": cs_lex::SIMILARITY_NOTE,
    })))
}

// -------------------------------------------------------------------- kwic

#[derive(Deserialize, Default)]
#[serde(default)]
struct KwicParams {
    q: String,
    stem: bool,
    width: Option<usize>,
    interview_id: Option<String>,
    attribute: Option<String>,
    value: Option<String>,
    speakers: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn kwic(State(st): State<AppState>, Query(p): Query<KwicParams>) -> ApiResult {
    if p.q.trim().is_empty() {
        return Err(invalid("q is required: the word or phrase to look for").into());
    }
    let store = st.store();
    let interviews = limit_to(&store, &p.interview_id, &p.attribute, &p.value)?;
    let result = store.lexical_kwic(&KwicQuery {
        query: p.q.clone(),
        stem: p.stem,
        width: p.width.unwrap_or(60).clamp(10, 400),
        interviews,
        speakers: speakers(&p.speakers)?,
        limit: p.limit.unwrap_or(200).clamp(1, 1000),
        offset: p.offset.unwrap_or(0),
    })?;
    Ok(Json(json!({
        "query": p.q,
        "stem": result.stem,
        "query_tokens": result.query_tokens,
        "total": result.total,
        "n_interviews": result.by_interview.len(),
        "by_interview": result.by_interview.iter()
            .map(|(i, n)| json!({"interview_id": i, "n": n})).collect::<Vec<_>>(),
        "hits": result.hits,
        "offset": p.offset.unwrap_or(0),
    })))
}

// ---------------------------------------------------------------- keywords

#[derive(Deserialize, Default)]
#[serde(default)]
struct KeywordParams {
    scope: Option<String>,
    min_passages: Option<usize>,
    limit: Option<usize>,
    interview_id: Option<String>,
    attribute: Option<String>,
    value: Option<String>,
}

/// The keyword table: stems of the highlights with passages, interviews and
/// how over-represented they are against the respondent text. A table, never
/// a word cloud.
async fn keywords(State(st): State<AppState>, Query(p): Query<KeywordParams>) -> ApiResult {
    let store = st.store();
    let interviews = limit_to(&store, &p.interview_id, &p.attribute, &p.value)?;
    let scope = p.scope.clone().unwrap_or_else(|| "highlights".into());
    let table = store.lexical_keywords(
        &scope,
        interviews.as_ref(),
        &KeywordOptions {
            min_passages: p.min_passages.unwrap_or(1),
            limit: p.limit.unwrap_or(100).clamp(1, 1000),
        },
    )?;
    Ok(Json(json!({
        "scope": scope,
        "n_passages": table.n_passages,
        "n_terms_in_passages": table.n_terms_in_passages,
        "n_rows": table.n_rows,
        "rows": table.rows,
        "note": "Les racines les plus fréquentes dans ces passages par rapport au texte des enquêtés. Un comptage de mots, jamais une mesure d'importance.",
    })))
}

// --------------------------------------------------------- term frequency

#[derive(Deserialize, Default)]
#[serde(default)]
struct FreqParams {
    attribute: String,
    /// Comma-separated words. Empty: the most frequent terms of each value.
    terms: String,
    stem: bool,
    speakers: Option<String>,
    top: Option<usize>,
}

async fn term_frequency(State(st): State<AppState>, Query(p): Query<FreqParams>) -> ApiResult {
    if p.attribute.trim().is_empty() {
        return Err(invalid("attribute is required").into());
    }
    let store = st.store();
    let sp = speakers(&p.speakers)?;
    let terms: Vec<String> = p
        .terms
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if terms.is_empty() {
        let values =
            store.lexical_top_terms(&p.attribute, sp, p.top.unwrap_or(10).clamp(1, 100))?;
        return Ok(Json(json!({
            "attribute": p.attribute,
            "mode": "top_terms",
            "values": values,
        })));
    }
    let rows = store.lexical_term_frequency(&p.attribute, &terms, p.stem, sp)?;
    Ok(Json(json!({
        "attribute": p.attribute,
        "mode": "terms",
        "stem": p.stem,
        "terms": rows,
        "note": "Occurrences et occurrences pour 1000 mots par valeur de l'attribut. Un comptage exploratoire, pas une prévalence.",
    })))
}

// ----------------------------------------------------- passage and similar

async fn passage_panel(State(st): State<AppState>, UrlPath(pid): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().passage_lexical(&pid)?)))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SimilarParams {
    k: Option<usize>,
    min_score: Option<f64>,
    interview_id: Option<String>,
    attribute: Option<String>,
    value: Option<String>,
    include_own_unit: bool,
    without_code: Option<String>,
}

fn similar_options(
    store: &cs_store::Store,
    p: &SimilarParams,
) -> Result<SimilarOptions, anyhow::Error> {
    Ok(SimilarOptions {
        k: p.k.unwrap_or(10).clamp(1, 100),
        min_score: p.min_score.unwrap_or(0.0).clamp(0.0, 1.0),
        interviews: limit_to(store, &p.interview_id, &p.attribute, &p.value)?,
        include_own_unit: p.include_own_unit,
        without_code: p.without_code.clone().filter(|c| !c.trim().is_empty()),
    })
}

/// Units close to a passage, with the codes they already carry.
async fn passage_similar(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Query(p): Query<SimilarParams>,
) -> ApiResult {
    let store = st.store();
    let opts = similar_options(&store, &p)?;
    let items = store.similar_to_passage(&pid, &opts)?;
    Ok(Json(json!({
        "passage_id": pid,
        "items": items,
        "note": cs_lex::SIMILARITY_NOTE,
    })))
}

async fn unit_similar(
    State(st): State<AppState>,
    UrlPath((id, unit_id)): UrlPath<(String, String)>,
    Query(p): Query<SimilarParams>,
) -> ApiResult {
    let store = st.store();
    let opts = similar_options(&store, &p)?;
    let items = store.similar_to_unit(&id, &unit_id, &opts)?;
    Ok(Json(json!({
        "interview_id": id,
        "unit_id": unit_id,
        "items": items,
        "note": cs_lex::SIMILARITY_NOTE,
    })))
}
