#![allow(clippy::type_complexity)]
//! The simple view through the HTTP API (v5): automatic phase 1 at
//! segmentation, the relevance shorthand, interviewer units out of the AI
//! path, `GET /api/interviews/{id}/simple` and `derived_contexts` in the export.
//!
//! Contract fixed here:
//! - `GET /api/interviews/{id}/simple` answers the `SimpleProgress` object
//!   (keys `n_respondent_units`, `n_relevant`, `n_coded`, `n_batches`,
//!   `n_runs_complete`, `n_disagreements`, `n_decided`, `n_sample`,
//!   `n_sample_reviewed`, `done`); unknown interview: 404.
//! - `PUT /phases/1/coding/{unit}` accepts `{"relevant": bool}`; the answer is
//!   the stored coding. `false` stores `NA`. `true` stores `CONTEXT` unless the
//!   unit already carries a category other than NA, which is kept. On an
//!   interviewer unit, or in phase 2: 400.
//! - `phase1_contexts.csv` is `unit_id,codes,derived_contexts`; contexts are
//!   joined with `;` in the order TECH, ORG, ENV; empty for a unit with no
//!   derived context (interviewer, or NA in phase 1).

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

const CLEANED: &str = "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Thank you for your time. Could you describe your role?\n\n**RESP `[00:05]`:** I run the logistics of a mid-sized retailer and our supplier tags every carton at the source.\n\n**INT `[00:20]`:** What made the business case work?\n\n**RESP `[00:24]`:** The inventory accuracy went from sixty to ninety-eight percent in one year.\n\n**INT `[00:40]`:** And the costs?\n\n**RESP `[00:43]`:** Tags were expensive at first, and our team lacked the skills for the ERP integration.\n";

const I: &str = "/api/interviews/INT-01";
const P1: &str = "/api/interviews/INT-01/phases/1";
const P2: &str = "/api/interviews/INT-01/phases/2";

fn make_codebook(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT, definition TEXT,
           sub_question TEXT, cycle INTEGER DEFAULT 1, origin TEXT DEFAULT 'codebook',
           active INTEGER NOT NULL DEFAULT 1, notes TEXT);
         INSERT INTO codes (code,family,label,definition,sub_question,origin,active) VALUES
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
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    coding_db: std::path::PathBuf,
}

fn setup() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Interviews");
    std::fs::create_dir_all(root.join("INT-01_Test")).unwrap();
    std::fs::write(
        root.join("INT-01_Test/02_transcript_cleaned_diarised.md"),
        CLEANED,
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
        interviewer_names: vec![],
        picker: None,
    };
    Ctx {
        app: router(state),
        root,
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

/// Codebook imported, library scanned, INT-01 segmented. Returns the six unit ids
/// (U001 INT, U002 RESP, U003 INT, U004 RESP, U005 INT, U006 RESP).
async fn segmented(ctx: &Ctx) -> Vec<String> {
    let app = &ctx.app;
    let (st, b) = call(
        app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, seg) = call(app, "POST", &format!("{I}/segment"), None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let units: Vec<String> = seg["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["unit_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(units.len(), 6);
    units
}

fn answer(part: &Value, codes_for: impl Fn(&str) -> Vec<String>) -> String {
    let codings: Vec<Value> = part["unit_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| {
            let u = u.as_str().unwrap();
            json!({ "unit_id": u, "codes": codes_for(u) })
        })
        .collect();
    let obj = json!({
        "schema": cs_ai::OUTPUT_SCHEMA_ID,
        "interview_id": part["interview_id"],
        "phase": part["phase"],
        "segmentation_hash": part["segmentation_hash"],
        "part": part["part"],
        "of": part["of"],
        "codings": codings,
    });
    format!("```json\n{obj}\n```\n")
}

fn codes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn unit_ids(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| r["unit"]["unit_id"].as_str().unwrap().to_string())
        .collect()
}

async fn phase1_codes(ctx: &Ctx, unit: &str) -> Value {
    let (_, rows) = call(&ctx.app, "GET", &format!("{P1}/units"), None).await;
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["unit"]["unit_id"] == unit)
        .unwrap()["coding"]["codes"]
        .clone()
}

// ------------------------------------------------ 1. automatic phase 1

#[tokio::test]
async fn the_segment_route_codes_phase_1_by_role() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    for (i, expected) in [
        (0, "NA"),
        (1, "CONTEXT"),
        (2, "NA"),
        (3, "CONTEXT"),
        (4, "NA"),
        (5, "CONTEXT"),
    ] {
        assert_eq!(
            phase1_codes(&ctx, &u[i]).await,
            json!([expected]),
            "{}",
            u[i]
        );
    }
    let (_, audit) = call(&ctx.app, "GET", "/api/audit?interview_id=INT-01", None).await;
    let auto: Vec<&Value> = audit
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "coding.auto_phase1")
        .collect();
    assert_eq!(auto.len(), 1, "{audit}");
    assert_eq!(auto[0]["detail"]["n_filled"], 6);
    assert_eq!(auto[0]["detail"]["n_na"], 3);
    assert_eq!(auto[0]["detail"]["n_context"], 3);
    assert_eq!(auto[0]["detail"]["n_kept"], 0);
    let (_, state) = call(&ctx.app, "GET", P1, None).await;
    assert_eq!(state["guard"]["complete"], true, "{state}");
}

#[tokio::test]
async fn segmenting_again_keeps_the_candidates_codings() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("{P1}/coding/{}", u[1]),
        Some(json!({ "codes": ["TECH"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // Same source, same rule: a no-op that must not reset the codings.
    let (st, b) = call(&ctx.app, "POST", &format!("{I}/segment"), None).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(phase1_codes(&ctx, &u[1]).await, json!(["TECH"]));
}

// ---------------------------------------------------- 4. the simple route

#[tokio::test]
async fn simple_route_serves_the_progress_object() {
    let ctx = setup();
    segmented(&ctx).await;
    let (st, p) = call(&ctx.app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(st, StatusCode::OK, "{p}");
    assert_eq!(
        p,
        json!({
            "n_respondent_units": 3,
            "n_relevant": 3,
            "n_coded": 0,
            "n_batches": 0,
            "n_runs_complete": 0,
            "n_disagreements": 0,
            "n_decided": 0,
            "n_sample": 0,
            "n_sample_reviewed": 0,
            "n_to_review": 0,
            "done": false,
        })
    );
}

#[tokio::test]
async fn simple_route_of_an_unknown_interview_is_404() {
    let ctx = setup();
    let (st, e) = call(&ctx.app, "GET", "/api/interviews/NOPE-01/simple", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{e}");
    assert_eq!(e["kind"], "not_found");
}

// -------------------------------------------- 5. the relevance shorthand

#[tokio::test]
async fn relevant_false_stores_na_and_true_stores_context() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let url = format!("{P1}/coding/{}", u[1]);
    let (st, r) = call(&ctx.app, "PUT", &url, Some(json!({ "relevant": false }))).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["codes"], json!(["NA"]));
    assert_eq!(phase1_codes(&ctx, &u[1]).await, json!(["NA"]));
    let (_, p) = call(&ctx.app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(
        p["n_relevant"], 2,
        "a unit marked not relevant leaves the scope"
    );
    assert_eq!(p["n_respondent_units"], 3);

    let (st, r) = call(&ctx.app, "PUT", &url, Some(json!({ "relevant": true }))).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["codes"], json!(["CONTEXT"]));
    let (_, p) = call(&ctx.app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(p["n_relevant"], 3);
}

#[tokio::test]
async fn relevant_true_keeps_a_toe_category_already_set() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let url = format!("{P1}/coding/{}", u[3]);
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &url,
        Some(json!({ "codes": ["TECH", "ORG"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, r) = call(&ctx.app, "PUT", &url, Some(json!({ "relevant": true }))).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["codes"], json!(["ORG", "TECH"]), "kept");
    // Not relevant, then relevant again: the category is gone, CONTEXT is back.
    let (_, r) = call(&ctx.app, "PUT", &url, Some(json!({ "relevant": false }))).await;
    assert_eq!(r["codes"], json!(["NA"]));
    let (_, r) = call(&ctx.app, "PUT", &url, Some(json!({ "relevant": true }))).await;
    assert_eq!(r["codes"], json!(["CONTEXT"]));
}

#[tokio::test]
async fn relevant_is_refused_on_an_interviewer_unit_and_in_phase_2() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("{P1}/coding/{}", u[0]),
        Some(json!({ "relevant": true })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(e["kind"], "validation");
    assert_eq!(phase1_codes(&ctx, &u[0]).await, json!(["NA"]));
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("{P2}/coding/{}", u[1]),
        Some(json!({ "relevant": false })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
}

#[tokio::test]
async fn the_full_phase_1_body_still_works() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let (st, r) = call(
        &ctx.app,
        "PUT",
        &format!("{P1}/coding/{}", u[1]),
        Some(json!({ "codes": ["ENV"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["codes"], json!(["ENV"]));
}

// -------------------------------- 2. interviewer units out of the AI path

#[tokio::test]
async fn interviewer_units_never_reach_phase_2_or_the_prompts() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let (_, rows) = call(&ctx.app, "GET", &format!("{P2}/units"), None).await;
    assert_eq!(unit_ids(&rows), [u[1].clone(), u[3].clone(), u[5].clone()]);
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("{P2}/coding/{}", u[0]),
        Some(json!({ "codes": ["TECH-ADV"] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    for (i, c) in [(1, "TECH-ADV"), (3, "TECH-COST"), (5, "ORG-SKILLS")] {
        let (st, r) = call(
            &ctx.app,
            "PUT",
            &format!("{P2}/coding/{}", u[i]),
            Some(json!({ "codes": [c] })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }
    let (st, parts) = call(&ctx.app, "POST", &format!("{P2}/prompts"), Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    let mut in_prompts: Vec<String> = parts
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["unit_ids"].as_array().unwrap().clone())
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    in_prompts.sort();
    assert_eq!(in_prompts, [u[1].clone(), u[3].clone(), u[5].clone()]);
    let text = parts[0]["text"].as_str().unwrap();
    // A28: an interviewer turn is no unit. It appears only as the question
    // that precedes a respondent unit, in the context section (lot 4).
    let units_block = &text[text.find("## Units").unwrap()..text.find("## Output").unwrap()];
    assert!(
        !units_block.contains("Could you describe your role?"),
        "no interviewer turn among the units"
    );
    for iu in [&u[0], &u[2], &u[4]] {
        assert!(
            !units_block.contains(iu.as_str()),
            "{iu} is not a unit of the prompt"
        );
    }
    let context =
        &text[text.find("## Interviewer questions").unwrap()..text.find("## Units").unwrap()];
    assert!(context.contains("Could you describe your role?"));
    assert!(context.contains(&format!("\"before_unit\":\"{}\"", u[1])));
}

#[tokio::test]
async fn verification_targets_exclude_interviewer_units() {
    let ctx = setup();
    let u = segmented(&ctx).await;
    let (st, v) = call(&ctx.app, "GET", &format!("{I}/verification"), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let targets: Vec<&str> = v["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["unit_id"].as_str().unwrap())
        .collect();
    assert!(!targets.is_empty(), "{v}");
    for iu in [&u[0], &u[2], &u[4]] {
        assert!(
            !targets.contains(&iu.as_str()),
            "{iu} is a verification target: {v}"
        );
    }
}

// ----------------------- full flow: done, and the derived contexts export

#[tokio::test]
async fn the_three_steps_reach_done_and_the_export_derives_contexts() {
    let ctx = setup();
    let app = &ctx.app;
    let u = segmented(&ctx).await;

    // Code: three relevant units.
    for (i, c) in [
        (1, vec!["TECH-ADV"]),
        (3, vec!["TECH-COST", "ORG-SKILLS"]),
        (5, vec!["OUT-REALISED"]),
    ] {
        let (st, r) = call(
            app,
            "PUT",
            &format!("{P2}/coding/{}", u[i]),
            Some(json!({ "codes": c })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }
    let (_, p) = call(app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(
        (p["n_relevant"].clone(), p["n_coded"].clone()),
        (json!(3), json!(3))
    );
    assert_eq!(
        p["done"], true,
        "coding can finish without optional AI review"
    );
    let (_, review) = call(app, "GET", &format!("{I}/review-state"), None).await;
    assert_eq!(review["state"], "not_done");
    assert_eq!(review["n_covered"], 0);

    // Confront: starting optional review does not fabricate completed coverage.
    let (st, parts) = call(app, "POST", &format!("{P2}/prompts"), Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    let parts = parts.as_array().unwrap().clone();
    assert_eq!(parts.len(), 1);
    let (_, p) = call(app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(
        (p["n_batches"].clone(), p["n_runs_complete"].clone()),
        (json!(1), json!(0))
    );
    let (st, run) = call(
        app,
        "POST",
        &format!("{P2}/runs"),
        Some(json!({ "run_label": "R1", "model_name": "model-a", "model_version": "1" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    let (_, p) = call(app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(
        p["done"], true,
        "no review decisions exist until a real response arrives"
    );
    assert_eq!(p["n_runs_complete"], 0);
    let (_, review) = call(app, "GET", &format!("{I}/review-state"), None).await;
    assert_eq!(review["state"], "in_progress");
    assert_eq!(review["n_covered"], 0);
    assert_eq!(review["coverage"], 0.0);
    let run_id = run["run_id"].as_i64().unwrap();
    let ai = |unit: &str| -> Vec<String> {
        if unit == u[3] {
            codes(&["ENV-PARTNER"])
        } else if unit == u[1] {
            codes(&["TECH-ADV"])
        } else {
            codes(&["OUT-REALISED"])
        }
    };
    let (st, r) = call(
        app,
        "POST",
        &format!("{P2}/runs/{run_id}/outputs"),
        Some(json!({ "raw": answer(&parts[0], ai), "part": 1, "of": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let (_, p) = call(app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(p["n_runs_complete"], 1);
    assert_eq!(p["n_disagreements"], 1);
    assert_eq!(p["n_sample"], 2);
    assert_eq!(p["done"], false);

    // Arbitrate: the disagreement needs a rationale, the sample does not.
    let (st, d) = call(
        app,
        "PUT",
        &format!("{I}/review/{}", u[3]),
        Some(
            json!({ "choice": {"type": "human"}, "final_codes": ["TECH-COST", "ORG-SKILLS"],
                     "rationale": "Both the cost and the skills gap are stated." }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    let (_, p) = call(app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(p["n_decided"], 1);
    assert_eq!(p["done"], false);
    for (i, c) in [(1, "TECH-ADV"), (5, "OUT-REALISED")] {
        let (st, d) = call(
            app,
            "PUT",
            &format!("{I}/review/{}", u[i]),
            Some(json!({ "choice": {"type": "human"}, "final_codes": [c] })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{d}");
    }
    let (_, p) = call(app, "GET", &format!("{I}/simple"), None).await;
    assert_eq!(p["n_sample_reviewed"], 2);
    assert_eq!(p["done"], true, "{p}");

    // Export: derived_contexts next to the provisional category.
    let (st, exp) = call(app, "POST", &format!("{I}/export"), None).await;
    assert_eq!(st, StatusCode::OK, "{exp}");
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    let csv = std::fs::read_to_string(dir.join("phase1_contexts.csv")).unwrap();
    let expected = format!(
        "unit_id,codes,derived_contexts\n{},NA,\n{},CONTEXT,TECH\n{},NA,\n{},CONTEXT,TECH;ORG\n{},NA,\n{},CONTEXT,CONTEXT\n",
        u[0], u[1], u[2], u[3], u[4], u[5]
    );
    assert_eq!(csv, expected);
}
