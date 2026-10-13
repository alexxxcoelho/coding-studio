#![allow(dead_code)]
//! Shared fixtures of the V1 API tests: a project folder with two fictional
//! interviews, a small codebook, and a JSON call helper.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use cs_app::{router, AppState};
use cs_store::Store;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

pub const CLEANED: &str = "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Thank you for your time. Could you describe your role?\n\n**RESP `[00:05]`:** I run the logistics of a mid-sized retailer and our supplier tags every carton at the source.\n\n**INT `[00:20]`:** What made the business case work?\n\n**RESP `[00:24]`:** The inventory accuracy went from sixty to ninety-eight percent in one year.\n\n**INT `[00:40]`:** And the costs?\n\n**RESP `[00:43]`:** Tags were expensive at first, and our team lacked the skills for the ERP integration.\n";

pub const SECOND: &str = "---\ninterview_id: INT-02\nstatus: test fixture\n---\n\n# INT-02, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Hello, tell me about the library.\n\n**RESP `[00:05]`:** Notre bibliothèque prête 1 200 livres par an, l'été c'est calme 😀 et le réseau décide du système.\n\n**INT `[00:20]`:** Et pour la formation ?\n\n**RESP `[00:24]`:** Les bénévoles ont suivi deux soirées de formation.\n";

pub struct Ctx {
    pub app: Router,
    pub dir: tempfile::TempDir,
    pub db: std::path::PathBuf,
    pub root: std::path::PathBuf,
}

fn make_codebook(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT, definition TEXT,
           sub_question TEXT, cycle INTEGER DEFAULT 1, origin TEXT DEFAULT 'codebook',
           active INTEGER NOT NULL DEFAULT 1, notes TEXT);
         INSERT INTO codes (code,family,label,definition,sub_question,origin,active) VALUES
          ('TECH-ADV','TECH','Relative advantage','Perceived advantage over the status quo.','SQ1','codebook',1),
          ('TECH-COST','TECH','Cost','Lifecycle cost and investment burden.','SQ1','codebook',1),
          ('ORG-SKILLS','ORG','Skills','Skills and expertise.','SQ2','codebook',1),
          ('ENV-PARTNER','ENV','Partner pressure','Partner pressure or mandate.','SQ3','codebook',1);",
    )
    .unwrap();
}

/// A project with INT-01 (6 units, odd ones are the interviewer) and INT-02
/// (4 units), both scanned and segmented, and four codes imported.
pub async fn setup() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Interviews");
    std::fs::create_dir_all(root.join("INT-01_Test")).unwrap();
    std::fs::create_dir_all(root.join("INT-02_Library")).unwrap();
    std::fs::write(
        root.join("INT-01_Test/02_transcript_cleaned_diarised.md"),
        CLEANED,
    )
    .unwrap();
    std::fs::write(
        root.join("INT-02_Library/02_transcript_cleaned_diarised.md"),
        SECOND,
    )
    .unwrap();
    let coding_db = dir.path().join("coding.db");
    make_codebook(&coding_db);
    let db = dir.path().join("studio/coding_studio.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let store = Store::open(&db).unwrap();
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        db_path: db.clone(),
        coding_db: Some(coding_db.clone()),
        actor: "candidate".into(),
        interviewer_names: vec!["Dos Santos Alexandre".into()],
        picker: None,
    };
    let ctx = Ctx {
        app: router(state),
        dir,
        db,
        root,
    };
    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    for id in ["INT-01", "INT-02"] {
        let (st, v) = call(
            &ctx.app,
            "POST",
            &format!("/api/interviews/{id}/segment"),
            Some(json!({})),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
    }
    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "kind": "coding_db", "path": coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    ctx
}

pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

/// `call` that expects 200 and returns the body.
pub async fn ok(app: &Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (st, v) = call(app, method, uri, body).await;
    assert_eq!(st, StatusCode::OK, "{method} {uri}: {v}");
    v
}

/// Stable id of a code from its legacy token.
pub async fn code_id(app: &Router, token: &str) -> String {
    let v = ok(app, "GET", "/api/codes", None).await;
    v["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["token"] == token)
        .unwrap_or_else(|| panic!("no code {token}"))["code_id"]
        .as_str()
        .unwrap()
        .to_string()
}

pub async fn code(app: &Router, key: &str) -> Value {
    ok(app, "GET", &format!("/api/codes/{key}"), None).await
}

/// The interview's unit ids in order.
pub async fn unit_ids(app: &Router, interview: &str) -> Vec<String> {
    let v = ok(app, "GET", &format!("/api/interviews/{interview}"), None).await;
    v["segmentation"]["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["unit_id"].as_str().unwrap().to_string())
        .collect()
}

pub fn text_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}
