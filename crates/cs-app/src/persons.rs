//! Routes of the register of the people interviewed: persons and
//! organisations, groups, links to interviews, CSV import, verification
//! package. Thin adapters over `cs-store`. Documented in `docs/API.md`,
//! section "Personnes et groupes".

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use cs_model::person::PersonPatch;
use cs_store::{GroupPatch, PersonFilter};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/persons", get(list_persons).post(create_person))
        .route("/api/persons/import", post(import_persons))
        .route(
            "/api/persons/import-table",
            post(crate::persons_table::preview)
                .layer(axum::extract::DefaultBodyLimit::max(24 * 1024 * 1024)),
        )
        .route(
            "/api/persons/{pid}",
            get(get_person).put(update_person).delete(delete_person),
        )
        .route("/api/persons/{pid}/interviews", post(link_from_person))
        .route(
            "/api/persons/{pid}/interviews/{iv}",
            axum::routing::delete(unlink_from_person),
        )
        .route(
            "/api/interviews/{id}/persons",
            get(interview_persons)
                .put(set_interview_persons)
                .post(link_to_interview),
        )
        .route(
            "/api/interviews/{id}/persons/{pid}",
            axum::routing::delete(unlink_from_interview),
        )
        .route("/api/person-groups", get(group_tree).post(create_group))
        .route(
            "/api/person-groups/{gid}",
            get(get_group).put(update_group).delete(delete_group),
        )
        .route(
            "/api/export/verification",
            post(crate::export_verification::export_verification_route),
        )
}

// ----------------------------------------------------------------- persons

async fn list_persons(State(st): State<AppState>, Query(filter): Query<PersonFilter>) -> ApiResult {
    let rows = st.store().persons(&filter)?;
    Ok(Json(json!({"n": rows.len(), "persons": rows})))
}

async fn create_person(State(st): State<AppState>, Json(body): Json<PersonPatch>) -> ApiResult {
    Ok(Json(json!(st.store().create_person(&body, &st.actor)?)))
}

async fn get_person(State(st): State<AppState>, UrlPath(pid): UrlPath<String>) -> ApiResult {
    Ok(Json(json!(st.store().person_detail(&pid)?)))
}

async fn update_person(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Json(body): Json<PersonPatch>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .update_person(&pid, &body, &st.actor)?)))
}

#[derive(Deserialize)]
struct DeleteQuery {
    #[serde(default)]
    with_links: bool,
}

async fn delete_person(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Query(q): Query<DeleteQuery>,
) -> ApiResult {
    st.store().delete_person(&pid, q.with_links, &st.actor)?;
    Ok(Json(json!({"deleted": pid})))
}

#[derive(Deserialize)]
struct ImportBody {
    /// The CSV text.
    csv: String,
    /// True (the default) reports without writing.
    #[serde(default = "yes")]
    dry_run: bool,
}

fn yes() -> bool {
    true
}

async fn import_persons(State(st): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    let report = st
        .store()
        .import_persons(&body.csv, body.dry_run, &st.actor)?;
    Ok(Json(json!(report)))
}

// ------------------------------------------------------------------- links

#[derive(Deserialize)]
struct LinkFromPerson {
    interview_id: String,
    #[serde(default = "respondent")]
    role: String,
}

#[derive(Deserialize)]
struct LinkToInterview {
    person_id: String,
    #[serde(default = "respondent")]
    role: String,
}

fn respondent() -> String {
    "respondent".into()
}

async fn link_from_person(
    State(st): State<AppState>,
    UrlPath(pid): UrlPath<String>,
    Json(body): Json<LinkFromPerson>,
) -> ApiResult {
    let store = st.store();
    store.link_person(&pid, &body.interview_id, &body.role, &st.actor)?;
    Ok(Json(json!(store.person_detail(&pid)?)))
}

async fn unlink_from_person(
    State(st): State<AppState>,
    UrlPath((pid, iv)): UrlPath<(String, String)>,
) -> ApiResult {
    let store = st.store();
    store.unlink_person(&pid, &iv, &st.actor)?;
    Ok(Json(json!(store.person_detail(&pid)?)))
}

fn persons_of(id: &str, list: Vec<cs_store::InterviewPerson>) -> serde_json::Value {
    json!({"interview_id": id, "persons": list})
}

async fn interview_persons(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let list = st.store().interview_persons(&id)?;
    Ok(Json(persons_of(&id, list)))
}

#[derive(Deserialize)]
struct SetPersons {
    persons: Vec<LinkToInterview>,
}

async fn set_interview_persons(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<SetPersons>,
) -> ApiResult {
    let links: Vec<(String, String)> = body
        .persons
        .into_iter()
        .map(|l| (l.person_id, l.role))
        .collect();
    let list = st.store().set_interview_persons(&id, &links, &st.actor)?;
    Ok(Json(persons_of(&id, list)))
}

async fn link_to_interview(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<LinkToInterview>,
) -> ApiResult {
    let list = st
        .store()
        .link_person(&body.person_id, &id, &body.role, &st.actor)?;
    Ok(Json(persons_of(&id, list)))
}

async fn unlink_from_interview(
    State(st): State<AppState>,
    UrlPath((id, pid)): UrlPath<(String, String)>,
) -> ApiResult {
    let list = st.store().unlink_person(&pid, &id, &st.actor)?;
    Ok(Json(persons_of(&id, list)))
}

// ------------------------------------------------------------------ groups

async fn group_tree(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().group_tree()?)))
}

#[derive(Deserialize)]
struct NewGroup {
    name: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    description: String,
}

async fn create_group(State(st): State<AppState>, Json(body): Json<NewGroup>) -> ApiResult {
    let g = st.store().create_group(
        &body.name,
        body.parent_id.as_deref(),
        &body.description,
        &st.actor,
    )?;
    Ok(Json(json!(g)))
}

async fn get_group(State(st): State<AppState>, UrlPath(gid): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let g = store.person_group(&gid)?;
    let tree = store.group_tree()?;
    fn find<'a>(
        nodes: &'a [cs_model::person::GroupNode],
        id: &str,
    ) -> Option<&'a cs_model::person::GroupNode> {
        for n in nodes {
            if n.group.group_id == id {
                return Some(n);
            }
            if let Some(f) = find(&n.children, id) {
                return Some(f);
            }
        }
        None
    }
    let node = find(&tree.groups, &gid).cloned();
    let persons = store.persons(&PersonFilter {
        group_id: Some(gid.clone()),
        ..Default::default()
    })?;
    Ok(Json(json!({"group": g, "node": node, "persons": persons})))
}

async fn update_group(
    State(st): State<AppState>,
    UrlPath(gid): UrlPath<String>,
    Json(body): Json<GroupPatch>,
) -> ApiResult {
    Ok(Json(json!(st
        .store()
        .update_group(&gid, &body, &st.actor)?)))
}

#[derive(Deserialize)]
struct DeleteGroupQuery {
    #[serde(default)]
    reassign_to: Option<String>,
}

async fn delete_group(
    State(st): State<AppState>,
    UrlPath(gid): UrlPath<String>,
    Query(q): Query<DeleteGroupQuery>,
) -> ApiResult {
    st.store()
        .delete_group(&gid, q.reassign_to.as_deref(), &st.actor)?;
    Ok(Json(json!({"deleted": gid})))
}
