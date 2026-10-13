#![allow(clippy::field_reassign_with_default, clippy::type_complexity)]
//! Study configuration through the HTTP API (v5), codebook import by kind,
//! and the optional `--coding-db`. Red until implemented, except the tests
//! marked as regression guards.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use cs_app::{prepare, router, AppState, Startup};
use cs_model::{Category, FrameworkPreset, StudyConfig};
use cs_store::Store;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const CLEANED: &str = "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Thank you for your time. Could you describe your role?\n\n**RESP `[00:05]`:** I run the logistics of a mid-sized retailer and our supplier tags every carton at the source.\n\n**INT `[00:20]`:** What made the business case work?\n\n**RESP `[00:24]`:** The inventory accuracy went from sixty to ninety-eight percent in one year.\n\n**INT `[00:40]`:** And the costs?\n\n**RESP `[00:43]`:** Tags were expensive at first, and our team lacked the skills for the ERP integration.\n";

const VTT: &str = "WEBVTT\n\nabc/1-0\n00:00:01.000 --> 00:00:03.000\n<v Interviewer Person A>Hello, can you hear me?</v>\n\nabc/2-0\n00:00:04.000 --> 00:00:08.000\n<v Jane Roe>Yes, perfectly. We use RFID in two stores and we tag every carton that arrives at the dock before it goes to the shelf.</v>\n\nabc/3-0\n00:00:09.000 --> 00:00:12.000\n<v Interviewer Person A>Why only two?</v>\n\nabc/4-0\n00:00:13.000 --> 00:00:18.000\n<v Jane Roe>Our partner did not want to tag at source.</v>\n";

fn make_codebook(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT, definition TEXT,
           sub_question TEXT, cycle INTEGER DEFAULT 1, origin TEXT DEFAULT 'codebook',
           active INTEGER NOT NULL DEFAULT 1, notes TEXT);
         INSERT INTO codes (code,family,label,definition,sub_question,origin,active) VALUES
          ('TECH','TECH','Technology, unrefined','Family-level code.',NULL,'legacy',1),
          ('TECH-ADV','TECH','Relative advantage','Perceived advantage over the status quo.','SQ1','codebook',1),
          ('TECH-COST','TECH','Cost','Lifecycle cost and investment burden.','SQ1','codebook',1),
          ('ORG-SKILLS','ORG','Skills','Skills and expertise.','SQ2','codebook',1),
          ('ENV-PARTNER','ENV','Partner pressure','Partner pressure or mandate.','SQ3','codebook',1),
          ('OUT-REALISED','OUTCOME','Realised outcome','A result supported by evidence.','SQ1','codebook',1);",
    )
    .unwrap();
}

struct Ctx {
    app: Router,
    dir: tempfile::TempDir,
    root: std::path::PathBuf,
    db: std::path::PathBuf,
    coding_db: std::path::PathBuf,
}

fn setup() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Interviews");
    std::fs::create_dir_all(root.join("INT-01_Test")).unwrap();
    std::fs::create_dir_all(root.join("ECO-02_Other")).unwrap();
    std::fs::write(
        root.join("INT-01_Test/02_transcript_cleaned_diarised.md"),
        CLEANED,
    )
    .unwrap();
    std::fs::write(root.join("ECO-02_Other/01_transcript_raw.vtt"), VTT).unwrap();
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
        interviewer_names: vec!["Interviewer Person A".into()],
        picker: None,
    };
    Ctx {
        app: router(state),
        dir,
        root,
        db,
        coding_db,
    }
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
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

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap().to_string())
        .collect()
}

fn cat(name: &str) -> Category {
    Category {
        name: name.into(),
        label: format!("{name} label"),
        definition: format!("{name} definition"),
        attachment: None,
    }
}

fn custom() -> StudyConfig {
    StudyConfig {
        framework: FrameworkPreset::Custom,
        phase1_categories: vec![cat("ALPHA"), cat("BETA")],
        study_name: "Pilot".into(),
        ..StudyConfig::default()
    }
}

async fn put_config(app: &Router, cfg: &StudyConfig) -> (StatusCode, Value) {
    call(
        app,
        "PUT",
        "/api/config",
        Some(serde_json::to_value(cfg).unwrap()),
    )
    .await
}

async fn scan(ctx: &Ctx) -> Value {
    let (st, scan) = call(
        &ctx.app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{scan}");
    scan
}

fn ids(scan: &Value) -> Vec<String> {
    scan["interviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["interview_id"].as_str().unwrap().to_string())
        .collect()
}

// ------------------------------------------------------- GET and PUT config

#[tokio::test]
async fn get_config_returns_the_default_study_config() {
    let ctx = setup();
    let (st, v) = call(&ctx.app, "GET", "/api/config", None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v, serde_json::to_value(StudyConfig::default()).unwrap());
    assert_eq!(v["framework"], "toe");
    assert_eq!(v["max_words"], 150);
    assert_eq!(v["id_prefixes"], json!(["INT", "ECO", "RET", "ADJ"]));
    assert_eq!(v["prompt_template"]["version"], "CS-P04-EN v3.1");
    assert_eq!(
        names(&v["phase1_categories"]),
        ["TECH", "ORG", "ENV", "CONTEXT"]
    );
}

#[tokio::test]
async fn put_config_stores_and_get_returns_it() {
    let ctx = setup();
    let cfg = custom();
    let (st, v) = put_config(&ctx.app, &cfg).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v, serde_json::to_value(&cfg).unwrap());
    let (_, got) = call(&ctx.app, "GET", "/api/config", None).await;
    assert_eq!(got, v);

    // Stored as JSON in the settings table of the database file.
    let conn = rusqlite::Connection::open(&ctx.db).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'study_config'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let stored: StudyConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(stored, cfg);

    // Audited.
    let (_, events) = call(&ctx.app, "GET", "/api/audit", None).await;
    let ev = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["action"] == "config.update")
        .expect("config.update event");
    assert_eq!(ev["actor"], "candidate");
    assert_eq!(ev["detail"]["after"]["study_name"], "Pilot");
    assert_eq!(
        ev["detail"]["before"]["study_name"],
        serde_json::to_value(StudyConfig::default().study_name).unwrap()
    );
}

#[tokio::test]
async fn put_config_validates_with_400() {
    let ctx = setup();
    let mut bad: Vec<(StudyConfig, &str)> = Vec::new();
    let mut c = StudyConfig::default();
    c.max_words = 19;
    bad.push((c, "max_words"));
    let mut c = custom();
    c.phase1_categories.push(cat("NA"));
    bad.push((c, "NA"));
    let mut c = custom();
    c.phase1_categories.push(cat("ALPHA"));
    bad.push((c, "duplicate"));
    let mut c = custom();
    c.phase1_categories.push(cat(""));
    bad.push((c, "empty"));
    let mut c = StudyConfig::default();
    c.id_prefixes = vec![];
    bad.push((c, "id_prefixes"));
    for (cfg, needle) in bad {
        let (st, e) = put_config(&ctx.app, &cfg).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{needle}: {e}");
        assert_eq!(e["kind"], "validation", "{e}");
        assert!(e["error"].as_str().unwrap().contains(needle), "{e}");
    }
    // Not valid JSON for a StudyConfig at all.
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/config",
        Some(json!({ "max_words": "many" })),
    )
    .await;
    assert!(st == StatusCode::BAD_REQUEST || st == StatusCode::UNPROCESSABLE_ENTITY);

    // Nothing was stored.
    let (_, got) = call(&ctx.app, "GET", "/api/config", None).await;
    assert_eq!(got, serde_json::to_value(StudyConfig::default()).unwrap());
}

// --------------------------------------------- state and phase 1 reflect it

#[tokio::test]
async fn state_includes_study_name_and_framework() {
    let ctx = setup();
    let (st, s) = call(&ctx.app, "GET", "/api/state", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["framework"], "toe");
    assert!(s["study_name"].is_string(), "{s}");
    assert_eq!(
        s["study_name"],
        serde_json::to_value(StudyConfig::default().study_name).unwrap()
    );

    let (st, _) = put_config(&ctx.app, &custom()).await;
    assert_eq!(st, StatusCode::OK);
    let (_, s) = call(&ctx.app, "GET", "/api/state", None).await;
    assert_eq!(s["framework"], "custom");
    assert_eq!(s["study_name"], "Pilot");
    // The former fields stay.
    for key in [
        "version",
        "db_path",
        "actor",
        "codebook_size",
        "chain",
        "native_dialogs",
    ] {
        assert!(s.get(key).is_some(), "{key}");
    }
}

#[tokio::test]
async fn phase1_codebook_route_reflects_the_config() {
    let ctx = setup();
    let (_, book) = call(&ctx.app, "GET", "/api/codebook/phase1", None).await;
    assert_eq!(names(&book["codes"]), ["TECH", "ORG", "ENV", "CONTEXT"]);

    let (st, v) = put_config(&ctx.app, &custom()).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let (st, book) = call(&ctx.app, "GET", "/api/codebook/phase1", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(names(&book["codes"]), ["ALPHA", "BETA"]);
    assert_eq!(book["codes"][0]["origin"], "phase1");
    assert_eq!(book["codes"][0]["definition"], "ALPHA definition");

    let tam = StudyConfig {
        framework: FrameworkPreset::TamUtaut,
        ..StudyConfig::default()
    };
    let (st, _) = put_config(&ctx.app, &tam).await;
    assert_eq!(st, StatusCode::OK);
    let (_, book) = call(&ctx.app, "GET", "/api/codebook/phase1", None).await;
    assert_eq!(
        names(&book["codes"]),
        [
            "PERFORMANCE_EXPECTANCY",
            "EFFORT_EXPECTANCY",
            "SOCIAL_INFLUENCE",
            "FACILITATING_CONDITIONS",
            "CONTEXT"
        ]
    );
}

#[tokio::test]
async fn phase1_units_and_coding_use_the_configured_categories() {
    let ctx = setup();
    scan(&ctx).await;
    let (st, seg) = call(&ctx.app, "POST", "/api/interviews/INT-01/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let first = seg["units"][0]["unit_id"].as_str().unwrap().to_string();
    let (st, v) = put_config(&ctx.app, &custom()).await;
    assert_eq!(st, StatusCode::OK, "{v}");

    let (_, rows) = call(
        &ctx.app,
        "GET",
        "/api/interviews/INT-01/phases/1/units",
        None,
    )
    .await;
    assert_eq!(names(&rows[0]["allowed"]), ["ALPHA", "BETA"]);

    let uri = format!("/api/interviews/INT-01/phases/1/coding/{first}");
    let (st, r) = call(&ctx.app, "PUT", &uri, Some(json!({ "codes": ["ALPHA"] }))).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let (st, r) = call(&ctx.app, "PUT", &uri, Some(json!({ "codes": ["TECH"] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{r}");
}

// ---------------------------------------------------- change after coding

#[tokio::test]
async fn changing_categories_after_coding_needs_force() {
    let ctx = setup();
    scan(&ctx).await;
    let (_, seg) = call(&ctx.app, "POST", "/api/interviews/INT-01/segment", None).await;
    let first = seg["units"][0]["unit_id"].as_str().unwrap().to_string();
    let (st, r) = call(
        &ctx.app,
        "PUT",
        &format!("/api/interviews/INT-01/phases/1/coding/{first}"),
        Some(json!({ "codes": ["TECH"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");

    let (st, e) = put_config(&ctx.app, &custom()).await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    assert_eq!(e["kind"], "guard", "{e}");
    let (_, got) = call(&ctx.app, "GET", "/api/config", None).await;
    assert_eq!(got["framework"], "toe");

    // A change that leaves the categories alone is fine.
    let mut renamed = StudyConfig::default();
    renamed.study_name = "Renamed".into();
    let (st, v) = put_config(&ctx.app, &renamed).await;
    assert_eq!(st, StatusCode::OK, "{v}");

    // Forced.
    let (st, v) = call(
        &ctx.app,
        "PUT",
        "/api/config?force=true",
        Some(serde_json::to_value(custom()).unwrap()),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["framework"], "custom");
    let (_, events) = call(&ctx.app, "GET", "/api/audit", None).await;
    let forced: Vec<&Value> = events
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "config.update" && e["detail"]["force"] == true)
        .collect();
    assert_eq!(forced.len(), 1, "{events}");
}

// ----------------------------------------- the library follows the config

#[tokio::test]
async fn scan_uses_the_configured_id_prefixes() {
    let ctx = setup();
    std::fs::create_dir_all(ctx.root.join("XYZ-05_Custom")).unwrap();
    std::fs::write(ctx.root.join("XYZ-05_Custom/01_transcript_raw.vtt"), VTT).unwrap();
    assert_eq!(ids(&scan(&ctx).await), ["ECO-02", "INT-01"]);

    let mut cfg = StudyConfig::default();
    cfg.id_prefixes = vec!["XYZ".into()];
    let (st, v) = put_config(&ctx.app, &cfg).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(ids(&scan(&ctx).await), ["XYZ-05"]);
}

#[tokio::test]
async fn segmentation_uses_the_configured_interviewers_and_max_words() {
    let ctx = setup();
    scan(&ctx).await;
    let (_, seg) = call(&ctx.app, "POST", "/api/interviews/ECO-02/segment", None).await;
    assert_eq!(seg["units"].as_array().unwrap().len(), 4);
    assert_eq!(seg["units"][0]["role"], "interviewer");

    // Changing the segmentation settings applies to the next segmentation.
    let mut cfg = StudyConfig::default();
    cfg.interviewers = vec!["Jane Roe".into()];
    cfg.max_words = 20;
    let (st, v) = put_config(&ctx.app, &cfg).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let (st, seg) = call(&ctx.app, "POST", "/api/interviews/ECO-02/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let units = seg["units"].as_array().unwrap();
    assert_eq!(units.len(), 5, "'Yes, perfectly.' is split off at 20 words");
    assert_eq!(units[0]["role"], "respondent");
    assert_eq!(units[1]["role"], "interviewer");
}

// ------------------------------------------------------- codebook import

fn write(ctx: &Ctx, name: &str, content: &str) -> std::path::PathBuf {
    let p = ctx.dir.path().join(name);
    std::fs::write(&p, content).unwrap();
    p
}

#[tokio::test]
async fn codebook_import_by_kind_csv() {
    let ctx = setup();
    let path = write(
        &ctx,
        "codebook.csv",
        "code,family,label,definition,origin,toe,research_questions,version,example,reason\nTECH-ADV,TECH,Advantage,\"Advantage, clear.\",literature,T,SQ1;SQ2,1,,\nNEW-FIELD,OTHER,New,Created in the field.,field,,,1,,seen\n",
    );
    let (st, book) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "kind": "csv", "path": path })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(names(&book["codes"]), ["TECH-ADV", "NEW-FIELD"]);
    assert_eq!(
        book["codes"][0]["research_questions"],
        json!(["SQ1", "SQ2"])
    );
    let (_, again) = call(&ctx.app, "GET", "/api/codebook", None).await;
    assert_eq!(again["codes"], book["codes"]);
}

#[tokio::test]
async fn codebook_import_by_kind_json() {
    let ctx = setup();
    let code = |c: &str, toe: Value| {
        json!({
            "code": c, "family": "OTHER", "label": c, "definition": format!("{c} definition"),
            "origin": "field", "toe": toe, "active": true, "in_prompt": true,
            "research_questions": [], "merged_into": null, "version": 1,
            "example": null, "reason": null, "created_at": null
        })
    };
    let path = write(
        &ctx,
        "codebook.json",
        &json!({ "source": "test", "codes": [code("LEGAL-OBLIGATION", json!("E")), code("COST-NO-BENEFIT", Value::Null)] })
            .to_string(),
    );
    let (st, book) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "kind": "json", "path": path })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(
        names(&book["codes"]),
        ["LEGAL-OBLIGATION", "COST-NO-BENEFIT"]
    );
    assert_eq!(book["codes"][0]["toe"], "E");
}

#[tokio::test]
async fn codebook_import_by_kind_coding_db() {
    let ctx = setup();
    let (st, book) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "kind": "coding_db", "path": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(book["codes"].as_array().unwrap().len(), 6);
    assert_eq!(book["codes"][1]["origin"], "literature");
}

#[tokio::test]
async fn codebook_import_errors_are_typed() {
    let ctx = setup();
    let bad_csv = write(
        &ctx,
        "bad.csv",
        "code,definition\ntech-adv,lower case is refused\n",
    );
    let dup_csv = write(&ctx, "dup.csv", "code,definition\nA-ONE,x\nA-ONE,y\n");
    for (body, needle) in [
        (json!({ "kind": "csv", "path": bad_csv }), "tech-adv"),
        (json!({ "kind": "csv", "path": dup_csv }), "duplicate"),
        (json!({ "kind": "xml", "path": bad_csv }), "kind"),
        (json!({ "kind": "csv" }), "path"),
        (
            json!({ "kind": "csv", "path": ctx.dir.path().join("absent.csv") }),
            "not a file",
        ),
        (json!({ "kind": "json", "path": bad_csv }), "json"),
    ] {
        let (st, e) = call(&ctx.app, "POST", "/api/codebook/import", Some(body.clone())).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}: {e}");
        assert_eq!(e["kind"], "validation", "{e}");
        assert!(
            e["error"].as_str().unwrap().to_lowercase().contains(needle),
            "{body}: {e}"
        );
    }
    let (_, book) = call(&ctx.app, "GET", "/api/codebook", None).await;
    assert!(book["codes"].as_array().unwrap().is_empty());
}

/// Regression guard: the former body `{coding_db}` and the empty body keep working.
#[tokio::test]
async fn codebook_import_legacy_bodies_still_work() {
    let ctx = setup();
    let (st, book) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(book["codes"].as_array().unwrap().len(), 6);
    let (st, book) = call(&ctx.app, "POST", "/api/codebook/import", Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(book["codes"].as_array().unwrap().len(), 6);
}

// ------------------------------------------------- optional --coding-db

fn startup_without_coding_db(dir: &Path) -> Startup {
    Startup {
        db: dir.join("studio/coding_studio.db"),
        library: None,
        coding_db: None,
        actor: "candidate".into(),
        interviewer_names: vec![],
    }
}

/// Without a coding.db the state shows `coding_db: null` (guard, passes today),
/// the codebook is empty and the legacy import has nothing to fall back on;
/// the `/api/config` part at the end is red until implemented.
#[tokio::test]
async fn state_without_coding_db_is_null() {
    let dir = tempfile::tempdir().unwrap();
    let state = prepare(startup_without_coding_db(dir.path()), None).unwrap();
    let app = router(state);
    let (st, s) = call(&app, "GET", "/api/state", None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(s["coding_db"].is_null(), "{s}");
    assert_eq!(s["codebook_size"], 0);

    let (st, e) = call(&app, "POST", "/api/codebook/import", Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");

    // The configuration works on a fresh database as well.
    let (st, cfg) = call(&app, "GET", "/api/config", None).await;
    assert_eq!(st, StatusCode::OK, "{cfg}");
    assert_eq!(cfg["framework"], "toe");
    let (_, s) = call(&app, "GET", "/api/state", None).await;
    assert_eq!(s["framework"], "toe");
}

/// The binary starts without `--coding-db` and reports `coding_db: null`.
/// Regression guard for the state fields that exist today.
#[test]
fn binary_runs_without_coding_db_argument() {
    let exe = env!("CARGO_BIN_EXE_coding-studio");
    let help = std::process::Command::new(exe)
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout).to_string();
    let usage = help.lines().find(|l| l.starts_with("Usage:")).unwrap();
    assert!(usage.contains("--db <DB>"), "{usage}");
    assert!(!usage.contains("--coding-db"), "optional: {usage}");

    let dir = tempfile::tempdir().unwrap();
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut child = std::process::Command::new(exe)
        .args(["--db"])
        .arg(dir.path().join("studio.db"))
        .args(["--port", &port.to_string(), "--no-open"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut body = None;
    for _ in 0..100 {
        if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            s.write_all(b"GET /api/state HTTP/1.0\r\nHost: localhost\r\n\r\n")
                .unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            body = out.split("\r\n\r\n").nth(1).map(str::to_string);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    let state: Value = serde_json::from_str(&body.expect("server answered")).unwrap();
    assert!(state["coding_db"].is_null(), "{state}");
    assert_eq!(state["codebook_size"], 0);
}

/// v5 contract: the import answer is the codebook plus `report: {added,
/// updated, absent}` (counts). `added` are new codes, `updated` existing codes
/// rewritten from the file, `absent` codes of the codebook the file lacks.
/// Red until implemented.
#[tokio::test]
async fn codebook_import_reports_counts() {
    let ctx = setup();
    let (st, first) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "kind": "coding_db", "path": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{first}");
    assert_eq!(
        first["report"],
        json!({ "added": 6, "updated": 0, "absent": 0 }),
        "{first}"
    );
    let path = write(
        &ctx,
        "partial.csv",
        "code,definition,origin\nTECH-ADV,A reworded definition.,literature\nNEW-FIELD,Created in the field.,field\n",
    );
    let (st, second) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "kind": "csv", "path": path })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{second}");
    assert_eq!(
        second["report"],
        json!({ "added": 1, "updated": 1, "absent": 5 }),
        "{second}"
    );
    assert!(second["codes"].is_array());
}
