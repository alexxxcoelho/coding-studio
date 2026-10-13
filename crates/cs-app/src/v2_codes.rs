//! V2 routes for codes and search: statistics per code (F-45), code search
//! (F-41), split and undo (F-39), full-text search (F-55), co-occurrence
//! (F-56). Thin adapters over `cs-store`; the rules live there. Documented in
//! `docs/API.md`, "V2 codes and search".

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use cs_model::CsError;
use serde::Deserialize;
use serde_json::json;

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/codes/{id}/stats", get(code_stats))
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
