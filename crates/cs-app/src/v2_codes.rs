//! V2 routes for codes and search: statistics per code (F-45), code search
//! (F-41), split and undo (F-39), full-text search (F-55), co-occurrence
//! (F-56). Thin adapters over `cs-store`; the rules live there. Documented in
//! `docs/API.md`, "V2 codes and search".

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use cs_model::CsError;
use cs_store::{CoRequest, CodeSearchOptions, Relation, SearchMode, SearchRequest};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/codes/search", get(search_codes))
        .route("/api/search/text", get(search_text))
        .route("/api/codes/cooccurrence", get(cooccurrence))
        .route("/api/codes/cooccurrence/passages", get(cooccurrence_passages))
        .route("/api/codes/{id}/stats", get(code_stats))
}

fn invalid(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CsError::Validation(msg.into()))
}

#[derive(Deserialize, Default)]
struct StatsQuery {
    #[serde(default)]
    attribute: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

/// Passages, interviews, cases and attribute values of one code, own and
/// rolled up. `attribute` and `value` go together.
async fn code_stats(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<StatsQuery>,
) -> ApiResult {
    let filter = match (q.attribute.as_deref(), q.value.as_deref()) {
        (Some(a), Some(v)) => Some((a, v)),
        (None, None) => None,
        _ => return Err(invalid("attribute and value go together").into()),
    };
    Ok(Json(json!(st.store().code_stats(&id, filter)?)))
}

#[derive(Deserialize, Default)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    include_inactive: bool,
    #[serde(default)]
    include_system: bool,
}

/// Code picker search (F-41): names, aliases, label, token and definitions,
/// prefix and accent-insensitive.
async fn search_codes(State(st): State<AppState>, Query(q): Query<SearchQuery>) -> ApiResult {
    let opts = CodeSearchOptions {
        include_inactive: q.include_inactive,
        include_system: q.include_system,
        limit: q.limit,
    };
    Ok(Json(json!(st.store().search_codes(&q.q, &opts)?)))
}

#[derive(Deserialize, Default)]
struct TextQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    prefix: bool,
    #[serde(default)]
    include_interviewer: bool,
    /// Comma-separated interview ids.
    #[serde(default)]
    interviews: Option<String>,
    #[serde(default)]
    attribute: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    coded: Option<bool>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    descendants: Option<bool>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    context: Option<usize>,
}

/// Full-text search across the interviews (F-55).
async fn search_text(State(st): State<AppState>, Query(q): Query<TextQuery>) -> ApiResult {
    let mode = match q.mode.as_deref() {
        None | Some("") | Some("all_words") => SearchMode::AllWords,
        Some("phrase") => SearchMode::Phrase,
        Some(other) => return Err(invalid(format!("mode is all_words or phrase, not {other}")).into()),
    };
    let req = SearchRequest {
        q: q.q,
        mode,
        prefix: q.prefix,
        include_interviewer: q.include_interviewer,
        interviews: q
            .interviews
            .iter()
            .flat_map(|s| s.split(','))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        attribute: q.attribute,
        value: q.value,
        coded: q.coded,
        code: q.code,
        descendants: q.descendants.unwrap_or(true),
        limit: q.limit,
        offset: q.offset.unwrap_or(0),
        context: q.context,
    };
    Ok(Json(json!(st.store().search_text(&req)?)))
}

#[derive(Deserialize, Default)]
struct CoQuery {
    #[serde(default)]
    relation: Option<String>,
    #[serde(default)]
    group_by: Option<String>,
    #[serde(default)]
    attribute: Option<String>,
    /// Comma-separated codes.
    #[serde(default)]
    codes: Option<String>,
    #[serde(default)]
    rollup: bool,
    #[serde(default)]
    min_units: Option<usize>,
    /// Comma-separated interview ids.
    #[serde(default)]
    interviews: Option<String>,
    /// Drill-down: the two codes of the cell.
    #[serde(default)]
    a: Option<String>,
    #[serde(default)]
    b: Option<String>,
}

fn list(raw: &Option<String>) -> Vec<String> {
    raw.iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn co_request(q: &CoQuery) -> Result<CoRequest, anyhow::Error> {
    let relation = match q.relation.as_deref() {
        None | Some("") | Some("overlap") => Relation::Overlap,
        Some("same_passage") => Relation::SamePassage,
        Some("same_unit") => Relation::SameUnit,
        Some(other) => {
            return Err(invalid(format!(
                "relation is same_passage, overlap or same_unit, not {other}"
            )))
        }
    };
    Ok(CoRequest {
        relation,
        group_by: q.group_by.clone(),
        attribute: q.attribute.clone(),
        codes: list(&q.codes),
        rollup: q.rollup,
        min_units: q.min_units.unwrap_or(1),
        interviews: list(&q.interviews),
    })
}

/// Code co-occurrence matrix (F-56), as sparse cells in codebook order.
async fn cooccurrence(State(st): State<AppState>, Query(q): Query<CoQuery>) -> ApiResult {
    Ok(Json(json!(st.store().cooccurrence(&co_request(&q)?)?)))
}

/// Drill-down to the passage pairs behind a cell: `a` and `b` are codes.
async fn cooccurrence_passages(
    State(st): State<AppState>,
    Query(q): Query<CoQuery>,
) -> ApiResult {
    let (Some(a), Some(b)) = (q.a.as_deref(), q.b.as_deref()) else {
        return Err(invalid("give the two codes of the cell: a and b").into());
    };
    let req = co_request(&q)?;
    let items = st.store().cooccurrence_passages(a, b, &req)?;
    Ok(Json(json!({
        "a": a, "b": b, "relation": req.relation, "rollup": req.rollup,
        "n_units": items.len(),
        "items": items,
    })))
}
