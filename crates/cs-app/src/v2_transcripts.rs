//! V2 routes: transcript versions and the review of passages after a change of
//! the text (F-24), audio linked to the transcript (F-62). Thin adapters over
//! `cs-store`; the rules live there. Documented in `docs/API.md`, section
//! "V2 transcripts and audio".

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use cs_model::passage::OffsetUnit;
use cs_store::{Resolution, ReviewFilter};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/interviews/{id}/transcript/versions",
            get(list_versions),
        )
        .route(
            "/api/interviews/{id}/transcript/versions/{version}",
            get(version_units),
        )
        .route("/api/interviews/{id}/transcript/review", get(interview_review))
        .route("/api/transcript/review", get(review))
        .route("/api/transcript/review/{pid}", get(review_item))
        .route("/api/transcript/review/{pid}/accept", post(accept))
        .route("/api/transcript/review/{pid}/adjust", post(adjust))
        .route("/api/transcript/review/{pid}/detach", post(detach))
}

async fn list_versions(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    Ok(Json(json!({
        "interview_id": id,
        "current": store.current_transcript_version(&id)?,
        "versions": store.transcript_versions(&id)?,
    })))
}

async fn version_units(
    State(st): State<AppState>,
    UrlPath((id, version)): UrlPath<(String, i64)>,
) -> ApiResult {
    let store = st.store();
    Ok(Json(json!({
        "interview_id": id,
        "version": version,
        "units": store.transcript_units_at(&id, version)?,
    })))
}

#[derive(Deserialize, Default)]
struct ReviewQuery {
    #[serde(default)]
    interview_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

fn review_body(st: &AppState, filter: ReviewFilter) -> ApiResult {
    let store = st.store();
    let items = store.transcript_review(&filter)?;
    let open = items.iter().filter(|i| i.status == "needs_review").count();
    Ok(Json(json!({
        "interview_id": filter.interview_id,
        "status": filter.status.unwrap_or_else(|| "needs_review".into()),
        "open": open,
        "items": items,
    })))
}

/// Passages flagged after a change of the text, across the library.
async fn review(State(st): State<AppState>, Query(q): Query<ReviewQuery>) -> ApiResult {
    review_body(
        &st,
        ReviewFilter {
            interview_id: q.interview_id,
            status: q.status,
        },
    )
}

async fn interview_review(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<ReviewQuery>,
) -> ApiResult {
    review_body(
        &st,
        ReviewFilter {
            interview_id: Some(id),
            status: q.status,
        },
    )
}

async fn review_item(State(st): State<AppState>, UrlPath(pid): UrlPath<i64>) -> ApiResult {
    Ok(Json(json!(st.store().transcript_review_item(pid)?)))
}

#[derive(Deserialize, Default)]
struct NoteBody {
    #[serde(default)]
    note: String,
}

async fn accept(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<i64>,
    body: Option<Json<NoteBody>>,
) -> ApiResult {
    let note = body.map(|b| b.0.note).unwrap_or_default();
    let item = st
        .store()
        .resolve_projection(pid, &Resolution::Accept, &note, &st.actor)?;
    Ok(Json(json!(item)))
}

#[derive(Deserialize)]
struct AdjustBody {
    #[serde(default)]
    unit_id: Option<String>,
    start: usize,
    end: usize,
    #[serde(default)]
    offset_unit: OffsetUnit,
    #[serde(default)]
    note: String,
}

async fn adjust(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<i64>,
    Json(body): Json<AdjustBody>,
) -> ApiResult {
    let item = st.store().resolve_projection(
        pid,
        &Resolution::Adjust {
            unit_id: body.unit_id,
            start: body.start,
            end: body.end,
            offset_unit: body.offset_unit,
        },
        &body.note,
        &st.actor,
    )?;
    Ok(Json(json!(item)))
}

async fn detach(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<i64>,
    body: Option<Json<NoteBody>>,
) -> ApiResult {
    let note = body.map(|b| b.0.note).unwrap_or_default();
    let item = st
        .store()
        .resolve_projection(pid, &Resolution::Detach, &note, &st.actor)?;
    Ok(Json(json!(item)))
}
