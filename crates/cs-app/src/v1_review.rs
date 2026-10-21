//! V1 lot 4 routes: candidate concepts, the pseudonymisation list, the
//! harmonisation worklist and the review state, and the builders the AI side
//! of `lib.rs` and `providers.rs` use (prompt context, quotation check, run
//! record). Thin adapters over `cs-store` and `cs-ai`. Documented in
//! `docs/API.md`, section "V1 lots 3 and 4".

use std::collections::{BTreeMap, BTreeSet};

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use cs_ai::{PromptContext, TreeCode};
use cs_model::passage::CodeNode;
use cs_model::pseudonym::{
    PseudonymConfig, PseudonymEntry, PseudonymisationRecord, FREE_TEXT_NOTE,
};
use cs_model::{AiOutput, Code, CsError, Segmentation, SpeakerRole};
use cs_store::{ConceptCandidateView, ConceptDecisionRequest, Store};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ApiError, ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/interviews/{id}/concept-candidates",
            get(list_candidates),
        )
        .route(
            "/api/interviews/{id}/concept-candidates/{cid}",
            put(decide_candidate),
        )
        .route(
            "/api/interviews/{id}/pseudonymisation",
            get(interview_pseudonymisation).put(put_interview_pseudonymisation),
        )
        .route(
            "/api/pseudonymisation",
            get(project_pseudonymisation).put(put_project_pseudonymisation),
        )
        .route(
            "/api/interviews/{id}/harmonisation",
            get(list_harmonisation),
        )
        .route(
            "/api/interviews/{id}/harmonisation/confirm",
            put(confirm_harmonisation_batch),
        )
        .route(
            "/api/interviews/{id}/harmonisation/{item}",
            put(resolve_harmonisation),
        )
        .route("/api/harmonisation", get(all_harmonisation))
        .route(
            "/api/interviews/{id}/review-state",
            get(interview_review_state),
        )
        .route("/api/review-state", get(corpus_review_state))
}

fn invalid(msg: impl Into<String>) -> ApiError {
    ApiError::from(anyhow::Error::new(CsError::Validation(msg.into())))
}

// ---------------------------------------------------------------- builders

/// The tree of the codes offered to the model, parent first. A code whose
/// parent is not offered sits under its nearest offered ancestor.
fn offered_tree(nodes: &[CodeNode], codes: &[Code]) -> Vec<TreeCode> {
    let by_token: BTreeMap<&str, &CodeNode> = nodes.iter().map(|n| (n.token.as_str(), n)).collect();
    let by_id: BTreeMap<&str, &CodeNode> = nodes.iter().map(|n| (n.code_id.as_str(), n)).collect();
    let offered: BTreeSet<&str> = codes.iter().map(|c| c.code.as_str()).collect();
    let parent_of = |node: &CodeNode| -> Option<String> {
        let mut cur = node.parent_id.as_deref();
        let mut guard = 0;
        while let Some(id) = cur {
            guard += 1;
            if guard > 64 {
                return None;
            }
            let p = by_id.get(id)?;
            if offered.contains(p.token.as_str()) {
                return Some(p.token.clone());
            }
            cur = p.parent_id.as_deref();
        }
        None
    };
    let flat: Vec<TreeCode> = codes
        .iter()
        .map(|c| match by_token.get(c.code.as_str()) {
            Some(n) => TreeCode {
                code: c.code.clone(),
                name: n.name.clone(),
                parent: parent_of(n),
                definition: n.definition.clone(),
                includes: n.includes.clone(),
                excludes: n.excludes.clone(),
                example: n.example.clone(),
            },
            None => TreeCode {
                code: c.code.clone(),
                name: c.label.clone(),
                parent: None,
                definition: c.definition.clone(),
                includes: String::new(),
                excludes: String::new(),
                example: c.example.clone(),
            },
        })
        .collect();
    // Depth first, in codebook order.
    let mut out = Vec::with_capacity(flat.len());
    fn walk(parent: Option<&str>, flat: &[TreeCode], out: &mut Vec<TreeCode>, depth: usize) {
        if depth > 64 {
            return;
        }
        for c in flat.iter().filter(|c| c.parent.as_deref() == parent) {
            out.push(c.clone());
            walk(Some(c.code.as_str()), flat, out, depth + 1);
        }
    }
    walk(None, &flat, &mut out, 0);
    out
}

/// What the configuration of the study adds to the prompt of an interview:
/// description and coding rules, research questions, the offered code tree,
/// the attributes the project shares with the model, the interviewer turn
/// before each unit, the pseudonymisation of the project. Reads nothing of the
/// researcher's coding.
pub(crate) fn prompt_context(
    store: &Store,
    seg: &Segmentation,
    unit_ids: &[String],
    codes: &[Code],
) -> anyhow::Result<PromptContext> {
    let study = store.study_config()?;
    let nodes = store.code_nodes()?;
    let attrs = store.interview_attributes(&seg.interview_id)?;
    let mut attributes: Vec<(String, String)> = attrs
        .definitions
        .iter()
        .filter(|d| d.shared_with_model)
        .filter_map(|d| {
            attrs
                .values
                .get(&d.id)
                .map(|v| (d.label.clone(), v.clone()))
        })
        .collect();
    // Attributes of the respondents of the register that the project shares;
    // the prompt masks them with the list like every other text.
    attributes.extend(store.shared_person_attributes(&seg.interview_id)?);
    let in_scope: BTreeSet<&str> = unit_ids.iter().map(String::as_str).collect();
    let mut preceding = BTreeMap::new();
    for (i, unit) in seg.units.iter().enumerate() {
        if i == 0 || !in_scope.contains(unit.unit_id.as_str()) {
            continue;
        }
        let before = &seg.units[i - 1];
        if before.role == SpeakerRole::Interviewer && !before.text.trim().is_empty() {
            preceding.insert(unit.unit_id.clone(), before.text.clone());
        }
    }
    let privacy_entries = crate::anonymisation::entries(store)?;
    let list = PseudonymConfig {
        enabled: true,
        entries: privacy_entries,
    };
    Ok(PromptContext {
        description: study.description.clone(),
        coding_rules: study.coding_rules.clone(),
        research_questions: store
            .research_questions()?
            .into_iter()
            .map(|q| (q.id, q.text))
            .collect(),
        tree: offered_tree(&nodes, codes),
        attributes,
        preceding,
        pseudonyms: crate::anonymisation::rules(store)?,
        pseudonym_digest: list.digest(),
    })
}

/// A9: the quotations of the candidate concepts of an answer must be found in
/// the units they cite (as the model saw them). All problems are returned.
pub(crate) fn check_answer(store: &Store, id: &str, out: &AiOutput) -> Result<(), ApiError> {
    if out.candidate_concepts.is_empty() {
        return Ok(());
    }
    let seg = store
        .corrected_segmentation(id)?
        .ok_or_else(|| invalid(format!("{id} has no segmentation yet")))?;
    let texts: BTreeMap<String, String> = seg
        .units
        .iter()
        .map(|u| (u.unit_id.clone(), u.text.clone()))
        .collect();
    let rules = crate::anonymisation::rules(store)?;
    let issues = cs_ai::check_candidate_quotes(out, &texts, &rules);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::with_issues(issues))
    }
}

/// A name of the list in a prompt that is about to leave the machine: refused.
pub(crate) fn refuse_unmasked(
    store: &Store,
    parts: &[cs_model::PromptPart],
) -> Result<(), ApiError> {
    let rules = crate::anonymisation::rules(store)?;
    if rules.is_empty() {
        return Ok(());
    }
    if let Some(p) = parts.iter().find(|p| rules.contains_original(&p.text)) {
        return Err(ApiError::from(anyhow::Error::new(CsError::Guard(format!(
            "part {} of the prompt contains a name of the pseudonymisation list: generate the prompt again before sending it",
            p.part
        )))));
    }
    Ok(())
}

fn meta_line(text: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}: ");
    text.lines()
        .take_while(|l| !l.starts_with("## "))
        .find_map(|l| l.strip_prefix(&prefix))
        .map(|v| v.trim().to_string())
}

/// What a run records of the pseudonymisation of the prompt it answers, read
/// from the digest the prompt carries. `None` for a prompt built without the
/// configuration.
pub(crate) fn run_record(
    store: &Store,
    prompt_text: &str,
) -> anyhow::Result<Option<PseudonymisationRecord>> {
    let Some(digest) = meta_line(prompt_text, "pseudonymisation_digest") else {
        return Ok(None);
    };
    let cfg = PseudonymConfig {
        enabled: true,
        entries: crate::anonymisation::entries(store)?,
    };
    let enabled = digest != PseudonymConfig::default().digest();
    let same = cfg.digest() == digest;
    Ok(Some(PseudonymisationRecord {
        enabled,
        categories: if same { cfg.categories() } else { Vec::new() },
        n_entries: if same && cfg.enabled {
            cfg.entries.len()
        } else {
            0
        },
        digest,
        free_text_covered: false,
        note: FREE_TEXT_NOTE.to_string(),
    }))
}

/// [`run_record`] for the prompt batch a run answers (the latest when none is given).
pub(crate) fn batch_run_record(
    store: &Store,
    id: &str,
    batch_id: Option<i64>,
) -> anyhow::Result<Option<PseudonymisationRecord>> {
    let parts = store.prompts(id)?;
    let batch = batch_id.or_else(|| parts.iter().map(|p| p.batch_id).max());
    match parts.iter().find(|p| Some(p.batch_id) == batch) {
        Some(p) => run_record(store, &p.text),
        None => Ok(None),
    }
}

// --------------------------------------------------------------- candidates

/// The label, description and quotations of a candidate as the researcher
/// reads them: the pseudonyms of the model are turned back into the names.
fn displayed(store: &Store, mut c: ConceptCandidateView) -> anyhow::Result<ConceptCandidateView> {
    let rules = crate::anonymisation::rules(store)?;
    c.label = rules.unmask(&c.label);
    c.description = c.description.map(|d| rules.unmask(&d));
    for e in &mut c.evidence {
        e.quote = rules.unmask(&e.quote);
    }
    Ok(c)
}

async fn list_candidates(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let store = st.store();
    let list: Vec<ConceptCandidateView> = store
        .concept_candidates(&id)?
        .into_iter()
        .map(|c| displayed(&store, c))
        .collect::<anyhow::Result<_>>()?;
    Ok(Json(json!(list)))
}

async fn decide_candidate(
    State(st): State<AppState>,
    UrlPath((id, cid)): UrlPath<(String, String)>,
    Json(body): Json<ConceptDecisionRequest>,
) -> ApiResult {
    let store = st.store();
    let v = store.decide_concept_candidate(&id, &cid, &body, &st.actor)?;
    Ok(Json(json!(displayed(&store, v)?)))
}

// ------------------------------------------------------- pseudonymisation

fn pseudonym_view(store: &Store, interview: Option<&str>) -> anyhow::Result<Value> {
    let eff = store.effective_pseudonyms()?;
    let manual = store.manual_pseudonym_config()?;
    let cfg = eff.config.clone();
    let entries: Vec<Value> = match interview {
        Some(id) => store
            .pseudonym_usage(id)?
            .into_iter()
            .map(|u| {
                json!({"kind": u.kind, "original": u.original, "pseudonym": u.pseudonym, "count": u.count})
            })
            .collect(),
        None => cfg
            .entries
            .iter()
            .zip(&eff.person_of)
            .map(|(e, person)| {
                json!({
                    "kind": e.kind, "original": e.original, "pseudonym": e.pseudonym,
                    "source": if person.is_some() { "person" } else { "manual" },
                    "person_id": person,
                })
            })
            .collect(),
    };
    let shadowed: Vec<Value> = eff
        .shadowed
        .iter()
        .map(|(e, why)| json!({"kind": e.kind, "original": e.original, "pseudonym": e.pseudonym, "reason": why}))
        .collect();
    Ok(json!({
        "enabled": cfg.enabled,
        "enabled_manual": manual.enabled,
        "n_manual_entries": manual.entries.len(),
        "n_person_entries": eff.person_of.iter().filter(|p| p.is_some()).count(),
        "shadowed_manual": shadowed,
        "digest": cfg.digest(),
        "categories": cfg.categories(),
        "n_list_entries": cfg.entries.len(),
        "entries": entries,
        "free_text_covered": false,
        "note": FREE_TEXT_NOTE,
    }))
}

async fn interview_pseudonymisation(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult {
    Ok(Json(pseudonym_view(&st.store(), Some(&id))?))
}

async fn project_pseudonymisation(State(st): State<AppState>) -> ApiResult {
    Ok(Json(pseudonym_view(&st.store(), None)?))
}

/// An entry of the list as the interface sends it back. Entries that come
/// from a person (`source: "person"`) are not kept by hand: they are edited
/// through the person.
#[derive(Deserialize)]
struct PseudonymBodyEntry {
    kind: String,
    original: String,
    pseudonym: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Deserialize)]
struct PseudonymBody {
    /// The switch; unchanged when omitted.
    #[serde(default)]
    enabled: Option<bool>,
    /// The whole list of entries kept by hand, replacing the stored one;
    /// unchanged when omitted.
    #[serde(default)]
    entries: Option<Vec<PseudonymBodyEntry>>,
}

fn apply_pseudonym_body(st: &AppState, body: PseudonymBody) -> anyhow::Result<()> {
    let store = st.store();
    let current = store.manual_pseudonym_config()?;
    let next = PseudonymConfig {
        enabled: body.enabled.unwrap_or(current.enabled),
        entries: match body.entries {
            Some(list) => list
                .into_iter()
                .filter(|e| e.source.as_deref() != Some("person"))
                .map(|e| PseudonymEntry {
                    kind: e.kind,
                    original: e.original,
                    pseudonym: e.pseudonym,
                })
                .collect(),
            None => current.entries,
        },
    };
    store.set_pseudonym_config(&next, &st.actor)?;
    Ok(())
}

/// Edits the list of the project (the same list for every interview).
async fn put_interview_pseudonymisation(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<PseudonymBody>,
) -> ApiResult {
    st.store().interview(&id)?.ok_or_else(|| {
        ApiError::from(anyhow::Error::new(CsError::NotFound(format!(
            "interview {id}"
        ))))
    })?;
    apply_pseudonym_body(&st, body)?;
    Ok(Json(pseudonym_view(&st.store(), Some(&id))?))
}

async fn put_project_pseudonymisation(
    State(st): State<AppState>,
    Json(body): Json<PseudonymBody>,
) -> ApiResult {
    apply_pseudonym_body(&st, body)?;
    Ok(Json(pseudonym_view(&st.store(), None)?))
}

// ----------------------------------------------------------- harmonisation

fn worklist(items: Vec<cs_store::HarmonisationView>) -> Value {
    let n_open = items.iter().filter(|i| i.status == "open").count();
    json!({
        "n_open": n_open,
        "n_done": items.len() - n_open,
        "items": items,
    })
}

async fn list_harmonisation(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    Ok(Json(worklist(st.store().harmonisation(Some(&id))?)))
}

async fn all_harmonisation(State(st): State<AppState>) -> ApiResult {
    Ok(Json(worklist(st.store().harmonisation(None)?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmItem {
    item_id: i64,
    #[serde(default)]
    note: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmBody {
    items: Vec<ConfirmItem>,
}
async fn confirm_harmonisation_batch(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<ConfirmBody>,
) -> ApiResult {
    let items = body
        .items
        .into_iter()
        .map(|r| (r.item_id, r.note))
        .collect::<Vec<_>>();
    let n = st
        .store()
        .confirm_harmonisation_batch(&id, &items, &st.actor)?;
    Ok(Json(json!({"confirmed":n})))
}

#[derive(Deserialize)]
struct ResolveBody {
    resolution: String,
    #[serde(default)]
    note: Option<String>,
}

async fn resolve_harmonisation(
    State(st): State<AppState>,
    UrlPath((id, item)): UrlPath<(String, i64)>,
    Json(body): Json<ResolveBody>,
) -> ApiResult {
    let v = st.store().resolve_harmonisation(
        &id,
        item,
        &body.resolution,
        body.note.as_deref(),
        &st.actor,
    )?;
    Ok(Json(json!(v)))
}

// ------------------------------------------------------------ review state

async fn interview_review_state(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult {
    let store = st.store();
    store.interview(&id)?.ok_or_else(|| {
        ApiError::from(anyhow::Error::new(CsError::NotFound(format!(
            "interview {id}"
        ))))
    })?;
    Ok(Json(json!(store.review_state(&id)?)))
}

async fn corpus_review_state(State(st): State<AppState>) -> ApiResult {
    Ok(Json(json!(st.store().corpus_review_state()?)))
}
