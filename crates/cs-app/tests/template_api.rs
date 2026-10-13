//! V1 lot 2: the project template through the HTTP API. Acceptance tests
//! A21 (a second fictional study from a template), A26 (round trip).

mod common;

use axum::http::StatusCode;
use axum::Router;
use common::{call, ok, setup};
use serde_json::{json, Value};

const EXAMPLE: &str = include_str!("../../../examples/project_template.example.json");

/// An empty project: no interview, no codebook, so no research question.
fn bare_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("x.db");
    let store = cs_store::Store::open(&db).unwrap();
    let state = cs_app::AppState {
        store: std::sync::Arc::new(std::sync::Mutex::new(store)),
        db_path: db,
        coding_db: None,
        actor: "candidate".into(),
        interviewer_names: vec![],
        picker: None,
    };
    (cs_app::router(state), dir)
}

/// The fictional library study of the guide: three questions, its own
/// attributes, a memo template, matrix dimensions. No framework.
async fn library_study(app: &Router) {
    for (id, text) in [
        (
            "Q1",
            "Pourquoi une petite bibliothèque change-t-elle de système de prêt ?",
        ),
        ("Q2", "Qu'est-ce qui rend le changement difficile ?"),
        ("Q3", "Quel rôle joue le réseau de bibliothèques ?"),
    ] {
        ok(
            app,
            "POST",
            "/api/research-questions",
            Some(json!({"id": id, "text": text})),
        )
        .await;
    }
    let mut cfg = ok(app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([
        {"id": "library_kind", "label": "Type de bibliothèque", "kind": "choice",
         "choices": ["communale", "scolaire", "associative"], "shared_with_model": true},
        {"id": "volunteers", "label": "Nombre de bénévoles", "kind": "number"},
        {"id": "interview_date", "label": "Date de l'entretien", "kind": "date"},
        {"id": "statement_status", "label": "Statut du propos", "kind": "choice",
         "choices": ["vécu", "projet", "opinion", "observation indirecte"]}
    ]);
    cfg["memo_templates"].as_array_mut().unwrap().push(json!({
        "id": "library", "name": "Bibliothèque", "default": false, "version": 1,
        "questions": [
            {"id": "l1", "text": "Comment fonctionne le prêt aujourd'hui ?"},
            {"id": "l2", "text": "Qui décide du système, et qui paie ?"},
            {"id": "l3", "text": "Que dit la personne du réseau ?"}
        ]
    }));
    cfg["matrix_dimensions"] = json!([
        {"id": "who_imposes", "label": "Qui impose"},
        {"id": "who_pays", "label": "Qui paie"}
    ]);
    ok(app, "PUT", "/api/project-config", Some(cfg)).await;
    let mut study = ok(app, "GET", "/api/config", None).await;
    study["study_name"] =
        json!("Petite bibliothèque et changement de système de prêt (étude fictive)");
    ok(app, "PUT", "/api/config", Some(study)).await;
}

#[tokio::test]
async fn a26_export_import_export_gives_the_same_bytes() {
    let a = setup().await;
    library_study(&a.app).await;
    let t1 = ok(&a.app, "GET", "/api/project/template", None).await;
    assert_eq!(t1["format"], "coding-studio-project-template");
    assert_eq!(t1["version"], 1);
    assert!(t1.get("memo_history").is_none());
    // Deterministic: twice the same.
    assert_eq!(ok(&a.app, "GET", "/api/project/template", None).await, t1);

    let b = setup().await;
    let report = ok(
        &b.app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": t1})),
    )
    .await;
    assert_eq!(report["applied"], true, "{report}");
    assert!(report["problems"].as_array().unwrap().is_empty());
    assert!(report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["part"] == "attributes" && c["action"] == "added"));
    let t2 = ok(&b.app, "GET", "/api/project/template", None).await;
    assert_eq!(
        serde_json::to_string(&t2).unwrap(),
        serde_json::to_string(&t1).unwrap()
    );
    // Importing again changes nothing.
    let again = ok(
        &b.app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": t1})),
    )
    .await;
    assert_eq!(again["applied"], true);
    assert!(again["changes"].as_array().unwrap().is_empty(), "{again}");
    let audit = ok(&b.app, "GET", "/api/audit", None).await;
    assert!(audit
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "project.template.import"));
    assert_eq!(
        ok(&b.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );
}

#[tokio::test]
async fn the_template_carries_no_interview_data_and_no_secret() {
    let a = setup().await;
    library_study(&a.app).await;
    ok(
        &a.app,
        "PUT",
        "/api/interviews/INT-01/attributes",
        Some(json!({"values": {"volunteers": "12"}})),
    )
    .await;
    ok(
        &a.app,
        "PUT",
        "/api/interviews/INT-01/memo/answers",
        Some(json!({"answers": {"q1": "REPONSE-SECRETE"}})),
    )
    .await;
    let t = ok(&a.app, "GET", "/api/project/template", None)
        .await
        .to_string();
    for forbidden in [
        "REPONSE-SECRETE",
        "INT-01",
        "INT-02",
        "logistics",
        "api_key",
        "providers",
    ] {
        assert!(!t.contains(forbidden), "{forbidden} found in the template");
    }
}

#[tokio::test]
async fn a_dry_run_lists_the_changes_and_writes_nothing() {
    let a = setup().await;
    library_study(&a.app).await;
    let t = ok(&a.app, "GET", "/api/project/template", None).await;
    let b = setup().await;
    let r = ok(
        &b.app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": t, "dry_run": true})),
    )
    .await;
    assert_eq!(r["dry_run"], true);
    assert_eq!(r["applied"], false);
    assert!(!r["changes"].as_array().unwrap().is_empty());
    assert!(
        ok(&b.app, "GET", "/api/project-config", None).await["attributes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let rq = ok(&b.app, "GET", "/api/research-questions", None).await;
    assert!(rq
        .as_array()
        .unwrap()
        .iter()
        .all(|q| q["id"].as_str().unwrap().starts_with("SQ")));
}

#[tokio::test]
async fn every_problem_is_reported_at_once_and_nothing_is_applied() {
    let b = setup().await;
    let a = setup().await;
    let mut t = ok(&a.app, "GET", "/api/project/template", None).await;
    t["version"] = json!(7);
    t["project_config"]["attributes"] = json!([
        {"id": "x", "label": "X", "kind": "choice", "choices": []},
        {"id": "y", "label": "", "kind": "text"}
    ]);
    t["project_config"]["checklist"] = json!([{"id": "A B", "label": "bad id"}]);
    let r = ok(
        &b.app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": t})),
    )
    .await;
    assert_eq!(r["applied"], false);
    assert!(r["problems"].as_array().unwrap().len() >= 4, "{r}");
    assert!(
        ok(&b.app, "GET", "/api/project-config", None).await["attributes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (st, _) = call(
        &b.app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": {"hello": 1}})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a21_a_second_study_with_other_questions_loads_into_a_project_with_data() {
    // A project with interview data and its own attribute values.
    let ctx = setup().await;
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([{"id": "sector", "label": "Secteur", "kind": "choice", "choices": ["retail", "public"]}]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/attributes",
        Some(json!({"values": {"sector": "retail"}})),
    )
    .await;

    let other = setup().await;
    library_study(&other.app).await;
    let t = ok(&other.app, "GET", "/api/project/template", None).await;
    let r = ok(
        &ctx.app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": t})),
    )
    .await;
    assert_eq!(r["applied"], true, "{r}");
    // The attribute that has a value stays, the new ones arrive.
    let cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    let ids: Vec<&str> = cfg["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"sector") && ids.contains(&"library_kind"));
    assert!(r["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["action"] == "kept"));
    assert_eq!(
        ok(&ctx.app, "GET", "/api/interviews/INT-01/attributes", None).await["values"]["sector"],
        "retail"
    );
    let rq = ok(&ctx.app, "GET", "/api/research-questions", None).await;
    let rq_ids: Vec<&str> = rq
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    assert!(
        ["Q1", "Q2", "Q3"].iter().all(|q| rq_ids.contains(q)),
        "{rq_ids:?}"
    );
    // Nothing of the interviews moved.
    assert_eq!(
        ok(&ctx.app, "GET", "/api/interviews", None)
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn the_shipped_example_template_imports_cleanly_and_round_trips() {
    let example: Value = serde_json::from_str(EXAMPLE).unwrap();
    let (app, _dir) = bare_app();
    let r = ok(
        &app,
        "POST",
        "/api/project/template/import",
        Some(json!({"template": example})),
    )
    .await;
    assert_eq!(r["applied"], true, "{r}");
    let t = ok(&app, "GET", "/api/project/template", None).await;
    assert_eq!(t, example, "the example is exactly what the tool exports");
    let q: Vec<&str> = t["research_questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    assert_eq!(q, ["Q1", "Q2", "Q3"]);
    assert!(!EXAMPLE.contains('\u{2014}'));
}

/// Regenerates `examples/project_template.example.json`:
/// `WRITE_EXAMPLE=1 cargo test -p cs-app --test template_api write_example -- --ignored`
#[tokio::test]
#[ignore]
async fn write_example() {
    let (app, _dir) = bare_app();
    library_study(&app).await;
    let t = ok(&app, "GET", "/api/project/template", None).await;
    if std::env::var("WRITE_EXAMPLE").is_ok() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/project_template.example.json"
        );
        std::fs::write(path, serde_json::to_string_pretty(&t).unwrap() + "\n").unwrap();
    }
}
