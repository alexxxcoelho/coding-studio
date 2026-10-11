//! End-to-end round trip through the HTTP API on a temporary library:
//! scan, segment, code, prompt, blind guard, AI import (valid and invalid),
//! coding lock, divergences, arbitration, export, audit chain.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use cs_app::{router, AppState};
use cs_store::Store;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const CLEANED: &str = "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Thank you for your time. Could you describe your role?\n\n**RESP `[00:05]`:** I run the logistics of a mid-sized retailer and we tag every carton at the source.\n\n**INT `[00:20]`:** What made the business case work?\n\n**RESP `[00:24]`:** The inventory accuracy went from sixty to ninety-eight percent in one year.\n\n**INT `[00:40]`:** And the costs?\n\n**RESP `[00:43]`:** Tags were expensive at first, and the integration with our ERP took longer than planned.\n";

const VTT: &str = "WEBVTT\n\nabc/1-0\n00:00:01.000 --> 00:00:03.000\n<v Dos Santos Alexandre>Hello, can you hear me?</v>\n\nabc/2-0\n00:00:04.000 --> 00:00:08.000\n<v Jane Roe>Yes, perfectly. We use RFID in two stores.</v>\n\nabc/3-0\n00:00:09.000 --> 00:00:12.000\n<v Dos Santos Alexandre>Why only two?</v>\n\nabc/4-0\n00:00:13.000 --> 00:00:18.000\n<v Jane Roe>Our partner did not want to tag at source.</v>\n";

fn make_codebook(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT, definition TEXT,
           sub_question TEXT, cycle INTEGER DEFAULT 1, origin TEXT DEFAULT 'codebook',
           active INTEGER NOT NULL DEFAULT 1, notes TEXT);
         INSERT INTO codes (code,family,label,definition,origin,active) VALUES
          ('TECH-ADV','TECH','Relative advantage','Perceived advantage over the status quo.','codebook',1),
          ('TECH-COST','TECH','Cost','Lifecycle cost and investment burden.','codebook',1),
          ('ORG-SKILLS','ORG','Skills','Skills and expertise.','codebook',1),
          ('ENV-PARTNER','ENV','Partner pressure','Partner pressure or mandate.','codebook',1);",
    )
    .unwrap();
}

struct Ctx {
    app: Router,
    _dir: tempfile::TempDir,
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
        interviewer_names: vec!["Dos Santos Alexandre".into()],
    };
    Ctx {
        app: router(state),
        root,
        db,
        coding_db,
        _dir: dir,
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

fn answer(seg_hash: &str, units: &[(String, Vec<&str>)]) -> String {
    let codings: Vec<Value> = units
        .iter()
        .map(|(u, c)| json!({ "unit_id": u, "codes": c }))
        .collect();
    let obj = json!({
        "schema": cs_ai::OUTPUT_SCHEMA_ID,
        "interview_id": "INT-01",
        "segmentation_hash": seg_hash,
        "part": 1,
        "of": 1,
        "codings": codings,
    });
    // Wrapped in a fence with prose, as chat interfaces often do.
    format!("Here is the coding:\n```json\n{}\n```\n", obj)
}

#[tokio::test]
async fn full_round_trip() {
    let ctx = setup();
    let app = &ctx.app;

    // Static UI is served.
    let (st, _) = call(app, "GET", "/", None).await;
    assert_eq!(st, StatusCode::OK);

    // Codebook.
    let (st, book) = call(
        app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(book["codes"].as_array().unwrap().len(), 4);

    // Library scan.
    let (st, scan) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{scan}");
    let ids: Vec<&str> = scan["interviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["interview_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["ECO-02", "INT-01"]);

    // The VTT interview segments with the interviewer recognised.
    let (st, seg2) = call(app, "POST", "/api/interviews/ECO-02/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg2}");
    assert_eq!(seg2["units"][0]["role"], "interviewer");
    assert_eq!(seg2["units"][1]["role"], "respondent");

    // Read-only file view refuses paths outside the inventory.
    let (st, _) = call(
        app,
        "GET",
        "/api/interviews/INT-01/file?path=..%2F..%2Fsecret",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, f) = call(
        app,
        "GET",
        "/api/interviews/INT-01/file?path=02_transcript_cleaned_diarised.md",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(f["sha256_now"], f["sha256_recorded"]);

    // Segment INT-01.
    let (st, seg) = call(app, "POST", "/api/interviews/INT-01/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let units: Vec<Value> = seg["units"].as_array().unwrap().clone();
    assert_eq!(units.len(), 6);
    let seg_hash = seg["hash"].as_str().unwrap().to_string();
    let uid = |i: usize| units[i]["unit_id"].as_str().unwrap().to_string();

    // Prompt.
    let (st, parts) = call(
        app,
        "POST",
        "/api/interviews/INT-01/prompts",
        Some(json!({ "batch_size": 0 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    assert_eq!(parts.as_array().unwrap().len(), 1);
    let prompt_ids: Vec<String> = parts[0]["unit_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(prompt_ids.len(), 6);

    // Run.
    let (st, run) = call(
        app,
        "POST",
        "/api/interviews/INT-01/runs",
        Some(json!({ "run_label": "R1", "model_name": "test-model", "temperature": 0.0 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    let run_id = run["run_id"].as_i64().unwrap();
    assert_eq!(run["meta"]["prompt_id_version"], cs_ai::PROMPT_VERSION);

    // Human codes: interviewer turns NA, respondent turns a code.
    let human: Vec<(String, Vec<&str>)> = vec![
        (uid(0), vec!["NA"]),
        (uid(1), vec!["TECH-ADV"]),
        (uid(2), vec!["NA"]),
        (uid(3), vec!["TECH-ADV"]),
        (uid(4), vec!["NA"]),
        (uid(5), vec!["TECH-COST"]),
    ];

    // Only one unit coded: AI import refused by the blind guard.
    let (st, c) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/coding/{}", uid(0)),
        Some(json!({ "codes": ["NA"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    let valid = answer(
        &seg_hash,
        &[
            (uid(0), vec!["NA"]),
            (uid(1), vec!["TECH-ADV"]),
            (uid(2), vec!["NA"]),
            (uid(3), vec!["TECH-ADV", "ORG-SKILLS"]),
            (uid(4), vec!["NA"]),
            (uid(5), vec!["TECH-COST"]),
        ],
    );
    let out_uri = format!("/api/interviews/INT-01/runs/{run_id}/outputs");
    let (st, e) = call(
        app,
        "POST",
        &out_uri,
        Some(json!({ "raw": valid, "part": 1, "of": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    assert_eq!(e["kind"], "guard");

    // Validation of a human coding: unknown code, NA mixed, excerpt not in unit.
    let (st, _) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/coding/{}", uid(1)),
        Some(json!({ "codes": ["NOPE"] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/coding/{}", uid(1)),
        Some(json!({ "codes": ["TECH-ADV"], "excerpt": "words never said" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Complete the human coding, with an exact excerpt on one unit.
    for (u, codes) in &human {
        let mut body = json!({ "codes": codes });
        if *u == uid(3) {
            body["excerpt"] = json!("went from sixty to ninety-eight percent");
            body["ev_level"] = json!("EV2");
        }
        let (st, r) = call(
            app,
            "PUT",
            &format!("/api/interviews/INT-01/coding/{u}"),
            Some(body),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }

    // Re-segmentation is now refused.
    let (st, _) = call(app, "POST", "/api/interviews/INT-01/segment", None).await;
    let (_, seg_again) = call(app, "GET", "/api/interviews/INT-01", None).await;
    assert!(st == StatusCode::OK || st == StatusCode::CONFLICT);
    assert_eq!(seg_again["segmentation"]["hash"], seg_hash);

    // Invalid answer: unknown code, missing unit. All problems listed.
    let invalid = answer(
        &seg_hash,
        &[(uid(0), vec!["FOO"]), (uid(1), vec!["NA", "TECH-ADV"])],
    );
    let (st, e) = call(app, "POST", &out_uri, Some(json!({ "raw": invalid }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert!(e["issues"].as_array().unwrap().len() >= 3, "{e}");

    // Valid answer.
    let (st, r) = call(
        app,
        "POST",
        &out_uri,
        Some(json!({ "raw": valid, "part": 1, "of": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["complete"], true);

    // Same part twice is refused.
    let (st, _) = call(app, "POST", &out_uri, Some(json!({ "raw": valid }))).await;
    assert_ne!(st, StatusCode::OK);

    // Human coding is now locked.
    let (st, e) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/coding/{}", uid(1)),
        Some(json!({ "codes": ["ORG-SKILLS"] })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");

    // Agreement and divergences.
    let (st, agr) = call(app, "GET", "/api/interviews/INT-01/agreement", None).await;
    assert_eq!(st, StatusCode::OK, "{agr}");
    assert_eq!(agr["reports"][0]["n_exact"], 5);
    assert!(agr["markdown"].as_str().unwrap().contains("TECH-ADV"));
    let (st, dv) = call(app, "GET", "/api/interviews/INT-01/divergences", None).await;
    assert_eq!(st, StatusCode::OK, "{dv}");
    assert_eq!(dv["n_divergences"], 1);
    assert_eq!(dv["items"][0]["divergence"]["unit_id"], uid(3));

    // Export before arbitration: no final coding, pending listed.
    let (st, ex1) = call(app, "POST", "/api/interviews/INT-01/export", None).await;
    assert_eq!(st, StatusCode::OK, "{ex1}");
    assert_eq!(ex1["final_complete"], false);
    assert_eq!(ex1["pending_units"][0], uid(3));

    // Arbitration: rationale required, non-divergent unit refused.
    let arb_uri = format!("/api/interviews/INT-01/arbitration/{}", uid(3));
    let (st, _) = call(
        app,
        "PUT",
        &arb_uri,
        Some(json!({ "choice": { "type": "human" }, "final_codes": ["TECH-ADV"], "rationale": "  " })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/arbitration/{}", uid(1)),
        Some(
            json!({ "choice": { "type": "human" }, "final_codes": ["TECH-ADV"], "rationale": "x" }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, a) = call(
        app,
        "PUT",
        &arb_uri,
        Some(json!({
            "choice": { "type": "ai", "run_label": "R1" },
            "final_codes": ["TECH-ADV", "ORG-SKILLS"],
            "rationale": "The respondent attributes the gain to the team's skills as well."
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{a}");
    assert_eq!(a["decided_by"], "candidate");

    // Export after arbitration: final coding present, manifest hashes match.
    let (st, ex) = call(app, "POST", "/api/interviews/INT-01/export", None).await;
    assert_eq!(st, StatusCode::OK, "{ex}");
    assert_eq!(ex["final_complete"], true);
    let dir = Path::new(ex["dir"].as_str().unwrap());
    assert_ne!(ex["dir"], ex1["dir"], "exports never overwrite");
    assert!(dir.starts_with(ctx.db.parent().unwrap().join("exports")));
    for name in [
        "units.csv",
        "coding_human.csv",
        "human_full.csv",
        "agreement.md",
        "adjudication_log.md",
        "coding_final.csv",
        "audit_log.jsonl",
        "audit_chain.json",
        "README.md",
        "manifest.json",
    ] {
        assert!(dir.join(name).is_file(), "missing {name}");
    }
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let files = manifest["files"].as_array().unwrap();
    assert!(files.len() >= 13, "{manifest}");
    for f in files {
        let bytes = std::fs::read(dir.join(f["name"].as_str().unwrap())).unwrap();
        assert_eq!(cs_model::sha256_hex(&bytes), f["sha256"].as_str().unwrap());
    }
    let raw_name = files
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .find(|n| n.starts_with("ai_run") && n.ends_with("_raw.txt"))
        .expect("raw AI answer exported");
    assert_eq!(std::fs::read_to_string(dir.join(raw_name)).unwrap(), valid);
    let final_csv = std::fs::read_to_string(dir.join("coding_final.csv")).unwrap();
    assert!(final_csv.contains("ORG-SKILLS;TECH-ADV"), "{final_csv}");

    // Audit trail: chain intact, key actions present.
    let (st, chain) = call(app, "GET", "/api/audit/verify", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(chain["ok"], true, "{chain}");
    let (_, events) = call(app, "GET", "/api/audit?interview_id=INT-01", None).await;
    let actions: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    for needed in [
        "segmentation.save",
        "coding.set",
        "prompt.generate",
        "ai.run.create",
        "ai.output.import",
        "arbitration.record",
        "export.package",
    ] {
        assert!(actions.contains(&needed), "missing {needed} in {actions:?}");
    }

    // State and summaries.
    let (st, state) = call(app, "GET", "/api/state", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(state["codebook_size"], 4);
    let (_, list) = call(app, "GET", "/api/interviews", None).await;
    let int01 = list
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["interview_id"] == "INT-01")
        .unwrap();
    assert_eq!(int01["human_complete"], true);
    assert_eq!(int01["n_divergences"], 1);
    assert_eq!(int01["n_arbitrated"], 1);
}

#[tokio::test]
async fn errors_are_typed() {
    let ctx = setup();
    let (st, e) = call(&ctx.app, "GET", "/api/interviews/NOPE-9", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(e["kind"], "not_found");
    let (st, e) = call(&ctx.app, "POST", "/api/library/scan", Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": "/does/not/exist.db" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
