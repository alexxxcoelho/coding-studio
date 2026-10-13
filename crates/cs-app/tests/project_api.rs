//! V1 lot 2 through the HTTP API: project configuration, interview
//! attributes, memos, checklist. Acceptance tests A22, A23, A24.

mod common;

use axum::http::StatusCode;
use common::{call, code_id, ok, setup, unit_ids};
use serde_json::{json, Value};

async fn config(app: &axum::Router) -> Value {
    ok(app, "GET", "/api/project-config", None).await
}

async fn put_config(app: &axum::Router, cfg: Value) -> (StatusCode, Value) {
    call(app, "PUT", "/api/project-config", Some(cfg)).await
}

fn attrs() -> Value {
    json!([
        {"id": "kind", "label": "Type de bibliothèque", "kind": "choice", "choices": ["communale", "scolaire"]},
        {"id": "volunteers", "label": "Bénévoles", "kind": "number"},
        {"id": "visit", "label": "Date de visite", "kind": "date"},
    ])
}

#[tokio::test]
async fn the_defaults_are_served_and_complete() {
    let ctx = setup().await;
    let c = config(&ctx.app).await;
    assert_eq!(c["checklist"].as_array().unwrap().len(), 4);
    assert_eq!(c["theme_checks"].as_array().unwrap().len(), 7);
    assert_eq!(c["evidence_natures"].as_array().unwrap().len(), 5);
    assert_eq!(c["link_types"].as_array().unwrap().len(), 6);
    assert!(c["interpretive_framework"].is_null());
    assert_eq!(c["memo_templates"][0]["default"], true);
    let p = ok(&ctx.app, "GET", "/api/project-config/presets", None).await;
    assert_eq!(
        p["frameworks"]["toe"]["dimensions"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        p["frameworks"]["tam_utaut"]["dimensions"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[tokio::test]
async fn an_invalid_configuration_is_a_400_naming_the_element() {
    let ctx = setup().await;
    let mut c = config(&ctx.app).await;
    c["attributes"] = json!([{"id": "kind", "label": "Type", "kind": "choice", "choices": []}]);
    let (st, v) = put_config(&ctx.app, c).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    assert!(v["error"].as_str().unwrap().contains("kind"));
    assert!(config(&ctx.app).await["attributes"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a22_attributes_are_defined_filled_validated_and_used_as_a_code_filter() {
    let ctx = setup().await;
    let mut c = config(&ctx.app).await;
    c["attributes"] = attrs();
    let (st, v) = put_config(&ctx.app, c).await;
    assert_eq!(st, StatusCode::OK, "{v}");

    let put = |id: &'static str, values: Value| {
        let app = ctx.app.clone();
        async move {
            call(
                &app,
                "PUT",
                &format!("/api/interviews/{id}/attributes"),
                Some(json!({ "values": values })),
            )
            .await
        }
    };
    let (st, v) = put(
        "INT-01",
        json!({"kind": "communale", "volunteers": "12", "visit": "2026-05-04"}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["values"]["kind"], "communale");
    for bad in [
        json!({"kind": "privée"}),
        json!({"volunteers": "douze"}),
        json!({"visit": "04/05/2026"}),
        json!({"unknown": "x"}),
    ] {
        let (st, v) = put("INT-01", bad.clone()).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    put("INT-02", json!({"kind": "scolaire"})).await;
    let (st, _) = call(&ctx.app, "GET", "/api/interviews/NOPE/attributes", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // The values are a filter of the codes.
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": unit, "code": adv})),
    )
    .await;
    let unit2 = unit_ids(&ctx.app, "INT-02").await[1].clone();
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-02/passages",
        Some(json!({"unit_id": unit2, "code": adv})),
    )
    .await;
    let count = |v: &Value| {
        v["codes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["code_id"] == json!(adv))
            .unwrap()["counts"]["passages"]
            .as_i64()
            .unwrap()
    };
    assert_eq!(count(&ok(&ctx.app, "GET", "/api/codes", None).await), 2);
    let f = ok(
        &ctx.app,
        "GET",
        "/api/codes?attribute=kind&value=scolaire",
        None,
    )
    .await;
    assert_eq!(count(&f), 1);
    assert_eq!(f["filter"]["interviews"], json!(["INT-02"]));
    let (st, _) = call(&ctx.app, "GET", "/api/codes?attribute=kind", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(&ctx.app, "GET", "/api/codes?attribute=zz&value=a", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // An attribute with values cannot go; the refusal is a 409.
    let mut c = config(&ctx.app).await;
    c["attributes"] = json!([]);
    let (st, v) = put_config(&ctx.app, c).await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    // Everything is in the audit log.
    let audit = ok(&ctx.app, "GET", "/api/audit", None).await;
    let actions: Vec<&str> = audit
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(
        actions.contains(&"project_config.update") && actions.contains(&"interview.attributes")
    );
}

#[tokio::test]
async fn a23_memo_template_overrides_answers_and_versions() {
    let ctx = setup().await;
    let m = ok(&ctx.app, "GET", "/api/interviews/INT-01/memo", None).await;
    assert_eq!(m["template_id"], "default");
    assert_eq!(m["questions"].as_array().unwrap().len(), 4);

    // A second template, then choose it for INT-02 only.
    let mut c = config(&ctx.app).await;
    c["memo_templates"].as_array_mut().unwrap().push(json!({
        "id": "library", "name": "Bibliothèque", "default": false, "version": 1,
        "questions": [{"id": "l1", "text": "Combien de prêts par an ?"}, {"id": "l2", "text": "Qui décide du système ?"}]
    }));
    let (st, v) = put_config(&ctx.app, c).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let m2 = ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-02/memo/template",
        Some(json!({"template_id": "library"})),
    )
    .await;
    assert_eq!(m2["questions"].as_array().unwrap().len(), 2);
    assert_eq!(
        ok(&ctx.app, "GET", "/api/interviews/INT-01/memo", None).await["template_id"],
        "default"
    );
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-02/memo/template",
        Some(json!({"template_id": "zz"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // The interview adds, rewords and removes for itself.
    let m = ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/memo/overrides",
        Some(json!({
            "added": [{"id": "", "text": "Qui décide de quoi ?"}],
            "removed": ["q3"],
            "reworded": {"q1": "Que raconte la responsable ?"}
        })),
    )
    .await;
    let texts: Vec<&str> = m["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts.len(), 4);
    assert!(
        texts.contains(&"Que raconte la responsable ?") && texts.contains(&"Qui décide de quoi ?")
    );
    assert!(!texts.contains(&"Quelles contradictions apparaissent ?"));
    // INT-02 and the template are untouched.
    assert_eq!(
        config(&ctx.app).await["memo_templates"][0]["questions"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    // Answers carry a timestamp.
    let m = ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/memo/answers",
        Some(json!({"answers": {"q1": "Elle gère seule."}})),
    )
    .await;
    assert_eq!(m["questions"][0]["answer"]["text"], "Elle gère seule.");
    assert!(m["questions"][0]["answer"]["at"].as_str().unwrap().len() > 10);
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/memo/answers",
        Some(json!({"answers": {"q3": "non"}})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "q3 was removed for this interview"
    );

    // Editing the template bumps the version, the interview keeps its own.
    let mut c = config(&ctx.app).await;
    c["memo_templates"][0]["questions"][3]["text"] = json!("Que demander ensuite ?");
    let stored = ok(&ctx.app, "PUT", "/api/project-config", Some(c)).await;
    assert_eq!(stored["memo_templates"][0]["version"], 2);
    let m = ok(&ctx.app, "GET", "/api/interviews/INT-01/memo", None).await;
    assert_eq!(m["template_version"], 1);
    assert_eq!(m["current_version"], 2);
    assert_eq!(m["outdated"], true);
    assert_eq!(m["questions"][0]["answer"]["text"], "Elle gère seule.");
}

#[tokio::test]
async fn a24_the_checklist_state_is_per_interview_and_configurable() {
    let ctx = setup().await;
    let c = ok(&ctx.app, "GET", "/api/interviews/INT-01/checklist", None).await;
    assert_eq!(c["items"].as_array().unwrap().len(), 4);
    assert_eq!(c["complete"], false);
    let c = ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/checklist",
        Some(json!({"checked": {"turns_seen": true, "later_handled": true}, "note": "Mécanisme central : le réseau."})),
    )
    .await;
    assert_eq!(c["items"][0]["checked"], true);
    assert_eq!(c["items"][2]["checked"], false);
    assert_eq!(c["note"], "Mécanisme central : le réseau.");
    let other = ok(&ctx.app, "GET", "/api/interviews/INT-02/checklist", None).await;
    assert_eq!(other["items"][0]["checked"], false);
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/checklist",
        Some(json!({"checked": {"nope": true}})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // Not a lock: coding still works with the checklist incomplete.
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": unit})),
    )
    .await;
    // Editing the list changes what is shown.
    let mut cfg = config(&ctx.app).await;
    cfg["checklist"] = json!([{"id": "turns_seen", "label": "Tours vus"}, {"id": "extra", "label": "Autre point"}]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    let c = ok(&ctx.app, "GET", "/api/interviews/INT-01/checklist", None).await;
    assert_eq!(c["items"].as_array().unwrap().len(), 2);
    assert_eq!(c["items"][0]["checked"], true);
}

#[tokio::test]
async fn a24_the_framework_is_configuration_for_analysis_only() {
    let ctx = setup().await;
    let study_before = ok(&ctx.app, "GET", "/api/config", None).await;
    let codes_before = ok(&ctx.app, "GET", "/api/codes", None).await;
    let book_before = ok(&ctx.app, "GET", "/api/codebook", None).await;
    let preset = ok(&ctx.app, "GET", "/api/project-config/presets", None).await;
    let mut c = config(&ctx.app).await;
    c["interpretive_framework"] = preset["frameworks"]["tam_utaut"].clone();
    ok(&ctx.app, "PUT", "/api/project-config", Some(c)).await;
    assert_eq!(
        config(&ctx.app).await["interpretive_framework"]["preset"],
        "tam_utaut"
    );
    // The coding side (study configuration that feeds the prompt, codes,
    // codebook) is byte for byte what it was.
    assert_eq!(ok(&ctx.app, "GET", "/api/config", None).await, study_before);
    assert_eq!(
        ok(&ctx.app, "GET", "/api/codes", None).await["codes"],
        codes_before["codes"]
    );
    assert_eq!(
        ok(&ctx.app, "GET", "/api/codebook", None).await,
        book_before
    );
    // Custom dimensions, then none again.
    let mut c = config(&ctx.app).await;
    c["interpretive_framework"] = json!({"preset": "custom", "dimensions": [{"id": "pouvoir", "label": "Pouvoir", "definition": "Qui décide."}]});
    ok(&ctx.app, "PUT", "/api/project-config", Some(c.clone())).await;
    c["interpretive_framework"] = Value::Null;
    ok(&ctx.app, "PUT", "/api/project-config", Some(c)).await;
    assert!(config(&ctx.app).await["interpretive_framework"].is_null());
}

#[tokio::test]
async fn editable_lists_can_be_replaced() {
    let ctx = setup().await;
    let mut c = config(&ctx.app).await;
    c["theme_checks"] = json!([{"id": "one", "label": "Un seul contrôle"}]);
    c["evidence_natures"] = json!([{"id": "a", "label": "A"}, {"id": "b", "label": "B"}]);
    c["link_types"] = json!([{"id": "supports", "label": "soutient"}]);
    c["matrix_dimensions"] = json!([{"id": "who_pays", "label": "Qui paie"}]);
    let s = ok(&ctx.app, "PUT", "/api/project-config", Some(c)).await;
    assert_eq!(s["theme_checks"].as_array().unwrap().len(), 1);
    assert_eq!(s["matrix_dimensions"][0]["id"], "who_pays");
    // Empty link types are refused, a duplicate id too.
    let mut bad = config(&ctx.app).await;
    bad["link_types"] = json!([]);
    assert_eq!(put_config(&ctx.app, bad).await.0, StatusCode::BAD_REQUEST);
    let mut bad = config(&ctx.app).await;
    bad["evidence_natures"] = json!([{"id": "a", "label": "A"}, {"id": "a", "label": "B"}]);
    assert_eq!(put_config(&ctx.app, bad).await.0, StatusCode::BAD_REQUEST);
}
