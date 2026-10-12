//! End-to-end round trip through the HTTP API (v3) on a temporary library:
//! scan, segment, provisional phase 1, concrete phase 2 codes, a label to
//! clarify resolved into a field code, blind AI challenge with batches, the
//! review with its agreement sample, codebook edits and merge, research
//! questions, cases and the cross-case matrix, exports and the audit chain.
//! Plus the embedded React build and the native picker.

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

const VTT: &str = "WEBVTT\n\nabc/1-0\n00:00:01.000 --> 00:00:03.000\n<v Dos Santos Alexandre>Hello, can you hear me?</v>\n\nabc/2-0\n00:00:04.000 --> 00:00:08.000\n<v Jane Roe>Yes, perfectly. We use RFID in two stores.</v>\n\nabc/3-0\n00:00:09.000 --> 00:00:12.000\n<v Dos Santos Alexandre>Why only two?</v>\n\nabc/4-0\n00:00:13.000 --> 00:00:18.000\n<v Jane Roe>Our partner did not want to tag at source.</v>\n";

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
        picker: None,
    };
    Ctx {
        app: router(state),
        root,
        db,
        coding_db,
        _dir: dir,
    }
}

/// Raw response: status, content type, cache control, body bytes.
async fn raw(app: &Router, uri: &str) -> (StatusCode, String, String, Vec<u8>) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let header = |name: &str| {
        res.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (ct, cc) = (header("content-type"), header("cache-control"));
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, ct, cc, bytes)
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

/// An AI answer for one prompt part, wrapped in prose and a fence as chat
/// interfaces often do. `codes_for` gives the codes of each unit.
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
    format!("Here is the coding:\n```json\n{obj}\n```\n")
}

fn codes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn v3_round_trip() {
    let ctx = setup();
    let app = &ctx.app;
    const I: &str = "/api/interviews/INT-01";
    const P1: &str = "/api/interviews/INT-01/phases/1";
    const P2: &str = "/api/interviews/INT-01/phases/2";

    // Codebook, library, segmentation.
    let (st, book) = call(
        app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    assert_eq!(book["codes"].as_array().unwrap().len(), 6);
    assert_eq!(book["codes"][1]["toe"], "T");
    assert_eq!(book["codes"][1]["origin"], "literature");
    let (_, rqs) = call(app, "GET", "/api/research-questions", None).await;
    assert_eq!(rqs.as_array().unwrap().len(), 3);
    let (_, p1book) = call(app, "GET", "/api/codebook/phase1", None).await;
    assert_eq!(names(&p1book["codes"]), ["TECH", "ORG", "ENV", "CONTEXT"]);
    let (st, scan) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{scan}");
    assert_eq!(scan["interviews"].as_array().unwrap().len(), 2);
    let (st, seg2) = call(app, "POST", "/api/interviews/ECO-02/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg2}");
    assert_eq!(seg2["units"][0]["role"], "interviewer");
    let (st, _) = call(app, "GET", &format!("{I}/file?path=..%2Fsecret"), None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, seg) = call(app, "POST", &format!("{I}/segment"), None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let units: Vec<String> = seg["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["unit_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(units.len(), 6);
    let u = |i: usize| units[i].clone();

    // ---- Phase 1: provisional pre-classification, no AI pass.
    let (_, rows) = call(app, "GET", &format!("{P1}/units"), None).await;
    assert_eq!(
        names(&rows[0]["allowed"]),
        ["TECH", "ORG", "ENV", "CONTEXT"]
    );
    for (i, c) in [
        (0, vec!["NA"]),
        (1, vec!["CONTEXT", "ENV"]),
        (2, vec!["NA"]),
        (3, vec!["TECH"]),
        (4, vec!["NA"]),
        (5, vec!["TECH", "ORG"]),
    ] {
        let (st, r) = call(
            app,
            "PUT",
            &format!("{P1}/coding/{}", u(i)),
            Some(json!({ "codes": c })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }
    let (st, _) = call(
        app,
        "PUT",
        &format!("{P1}/coding/{}", u(3)),
        Some(json!({ "codes": ["TECH-ADV"] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, e) = call(app, "POST", &format!("{P1}/prompts"), Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");

    // ---- Phase 2: concrete codes on the units not NA in phase 1.
    let (_, rows) = call(app, "GET", &format!("{P2}/units"), None).await;
    let rows = rows.as_array().unwrap().clone();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2]["unit"]["unit_id"], u(5));
    assert_eq!(rows[2]["phase1"], json!(["ORG", "TECH"]));
    assert_eq!(rows[2]["in_scope"], true);
    let first: Vec<String> = names(&rows[2]["allowed"]);
    assert_eq!(
        &first[..3],
        ["TECH-ADV", "TECH-COST", "ORG-SKILLS"],
        "attached first"
    );
    assert!(
        first.contains(&"ENV-PARTNER".to_string()),
        "every code offered"
    );
    assert!(!first.contains(&"TECH".to_string()), "legacy not offered");
    let (st, r) = call(
        app,
        "PUT",
        &format!("{P2}/coding/{}", u(3)),
        Some(json!({ "codes": ["TECH-ADV", "OUT-REALISED"], "excerpt": "sixty to ninety-eight percent" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let (st, label) = call(
        app,
        "POST",
        &format!("{I}/units/{}/to-clarify", u(1)),
        Some(json!({ "label": "source tagging by the supplier", "note": "who pays?" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{label}");
    let tid = label["id"].as_i64().unwrap();
    let (_, open) = call(app, "GET", "/api/to-clarify?open=true", None).await;
    assert_eq!(open[0]["unit_text"], seg["units"][1]["text"]);
    assert_eq!(open[0]["interview_id"], "INT-01");

    // Prompts are refused while a unit of the batch is uncoded: the human
    // reference is frozen at generation, so it must be complete.
    let (st, e) = call(
        app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(e["kind"], "validation", "{e}");
    assert!(e["error"].as_str().unwrap().contains(&u(5)), "{e}");
    let (st, _) = call(
        app,
        "PUT",
        &format!("{P2}/coding/{}", u(5)),
        Some(json!({ "codes": ["TECH-COST", "ORG-SKILLS"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, parts) = call(
        app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    let parts = parts.as_array().unwrap().clone();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["batch_id"], 1);
    assert_eq!(parts[1]["batch_id"], 1);
    let text = parts[0]["text"].as_str().unwrap();
    assert!(text.contains("### Technological context (T)"));
    for absent in [
        "source tagging by the supplier",
        "\"CONTEXT\"",
        "OUT-REALISED\"]}",
    ] {
        assert!(!text.contains(absent), "{absent}");
    }
    let (st, run) = call(
        app,
        "POST",
        &format!("{P2}/runs"),
        Some(json!({ "run_label": "R1", "model_name": "model-a", "model_version": "1" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    assert_eq!(run["batch_id"], 1);
    let run_id = run["run_id"].as_i64().unwrap();
    let outputs = format!("{P2}/runs/{run_id}/outputs");
    let ai = |unit: &str| -> Vec<String> {
        match unit {
            x if x == u(1) => codes(&["ENV-PARTNER"]),
            x if x == u(3) => codes(&["TECH-ADV", "OUT-REALISED"]),
            _ => codes(&["TECH-COST"]),
        }
    };
    let (st, e) = call(
        app,
        "POST",
        &outputs,
        Some(json!({ "raw": answer(&parts[0], |_| codes(&["CONTEXT"])), "part": 1, "of": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert!(e["issues"].as_array().unwrap().len() >= 2);
    for (i, p) in parts.iter().enumerate() {
        let (st, r) = call(
            app,
            "POST",
            &outputs,
            Some(json!({ "raw": answer(p, ai), "part": i + 1, "of": 2 })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }

    // Revising after the AI answer is allowed and flagged.
    let (_, r) = call(
        app,
        "PUT",
        &format!("{P2}/coding/{}", u(3)),
        Some(json!({ "codes": ["TECH-ADV", "OUT-REALISED"], "memo": "re-read" })),
    )
    .await;
    assert_eq!(r["revised_after_ai"], true, "{r}");

    // ---- Review.
    let (_, review) = call(app, "GET", &format!("{I}/review"), None).await;
    assert_eq!(review["n_disagreements"], 1, "{review}");
    assert_eq!(review["n_sampled"], 1);
    assert_eq!(review["items"][0]["unit_id"], u(5));
    assert_eq!(review["items"][0]["kind"], "disagreement");
    assert_eq!(review["items"][0]["unit"]["unit_id"], u(5));
    assert_eq!(review["items"][1]["kind"], "agreement_sample");
    assert_eq!(review["seeds"][0]["run_label"], "R1");
    let (st, _) = call(
        app,
        "PUT",
        &format!("{I}/review/{}", u(5)),
        Some(json!({ "choice": {"type": "human"}, "final_codes": ["TECH-COST", "ORG-SKILLS"], "rationale": " " })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, d) = call(
        app,
        "PUT",
        &format!("{I}/review/{}", u(5)),
        Some(
            json!({ "choice": {"type": "human"}, "final_codes": ["TECH-COST", "ORG-SKILLS"],
                     "rationale": "The skills gap is stated, not only the cost." }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    let (st, _) = call(
        app,
        "PUT",
        &format!("{I}/review/{}", u(3)),
        Some(json!({ "choice": {"type": "human"}, "final_codes": ["TECH-ADV", "OUT-REALISED"] })),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::OK,
        "sampled agreement confirmed without a note"
    );
    let (_, ag) = call(app, "GET", &format!("{I}/agreement"), None).await;
    assert!(ag["markdown"].as_str().unwrap().contains("Phase 2"));

    // ---- The label becomes a field code.
    let (st, res) = call(
        app,
        "POST",
        &format!("/api/to-clarify/{tid}/resolve"),
        Some(json!({
            "kind": "new_code",
            "code": "SUPPLIER-TAGGING",
            "all_with_label": true,
            "rationale": "Recurs as a condition of the business case.",
            "new_code": {
                "code": "SUPPLIER-TAGGING",
                "label": "Supplier tags at source",
                "definition": "Items arrive tagged by the supplier.",
                "toe": "E",
                "research_questions": ["SQ3"],
            },
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{res}");
    assert_eq!(res["resolved"][0]["resolved"]["kind"], "new_code");
    let new = res["codebook"]["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "SUPPLIER-TAGGING")
        .unwrap()
        .clone();
    assert_eq!(new["origin"], "field");
    // The human reference is frozen: U002 carried only a label when the
    // prompts were generated, so it is not compared with the AI afterwards.
    // Its final coding is the candidate's current one.
    let (_, fin) = call(app, "GET", &format!("{I}/final"), None).await;
    assert_eq!(fin["n_open_disagreements"], 0, "{fin}");
    let f2 = fin["finals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["unit_id"] == u(1).as_str())
        .unwrap()
        .clone();
    assert_eq!(f2["codes"], json!(["SUPPLIER-TAGGING"]), "{fin}");
    assert_eq!(f2["basis"], "candidate", "{fin}");
    assert_eq!(fin["finals"].as_array().unwrap().len(), 3);
    assert!(fin["uncoded"].as_array().unwrap().is_empty());

    // ---- Codebook: edit, units, merge, export.
    let (st, b) = call(
        app,
        "PUT",
        "/api/codebook/codes/OUT-REALISED",
        Some(json!({ "toe": "O", "definition": "A result the respondent supports." })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, cu) = call(app, "GET", "/api/codebook/codes/OUT-REALISED/units", None).await;
    assert_eq!(cu[0]["unit"]["unit_id"], u(3));
    let (st, m) = call(
        app,
        "POST",
        "/api/codebook/codes/OUT-REALISED/merge",
        Some(json!({ "into": "TECH-ADV", "rationale": "Same mechanism in this corpus." })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{m}");
    assert_eq!(m["rewritten_codings"], 1);
    let (st, x) = call(app, "POST", "/api/codebook/export", None).await;
    assert_eq!(st, StatusCode::OK, "{x}");
    assert_eq!(x["files"].as_array().unwrap().len(), 2);
    let sql = std::fs::read_to_string(x["files"][1]["path"].as_str().unwrap()).unwrap();
    assert!(sql.contains("'SUPPLIER-TAGGING','ENV'"));

    // ---- Research questions, cases, cross-case.
    let (st, _) = call(
        app,
        "POST",
        "/api/research-questions",
        Some(json!({ "id": "RQ1", "text": "Why do projects stall?" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = call(app, "DELETE", "/api/research-questions/SQ3", None).await;
    assert_eq!(st, StatusCode::CONFLICT, "still mapped");
    let (st, case) = call(
        app,
        "POST",
        &format!("{I}/cases"),
        Some(json!({
            "label": "Retail rollout",
            "units": [u(3), u(5)],
            "trigger": { "text": "Inventory accuracy", "evidence": [u(3)] },
            "constraints": { "text": "Skills for the ERP integration", "evidence": [u(5)] },
            "result": { "text": "Ninety-eight percent accuracy", "evidence": [u(3)] },
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{case}");
    assert_eq!(case["case_id"], "INT-01-C1");
    let (st, _) = call(
        app,
        "POST",
        &format!("{I}/cases"),
        Some(json!({ "label": "x", "choice": { "text": "", "evidence": ["INT-01-U999"] } })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, cc) = call(app, "GET", "/api/cross-case", None).await;
    let cell = cc["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["rq"] == "SQ1" && c["case_id"] == "INT-01-C1")
        .unwrap()
        .clone();
    let found: Vec<String> = cell["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(found, ["TECH-ADV", "TECH-COST"]);

    // ---- Export package.
    let (st, exp) = call(app, "POST", &format!("{I}/export"), None).await;
    assert_eq!(st, StatusCode::OK, "{exp}");
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    assert!(dir.starts_with(ctx.db.parent().unwrap()));
    for f in [
        "units.csv",
        "phase1_contexts.csv",
        "phase2_coding_human.csv",
        "phase2_human_full.csv",
        "phase2_agreement.md",
        "phase2_review.md",
        "phase2_coding_final.csv",
        "to_clarify.csv",
        "cases.json",
        "cases.md",
        "codebook.csv",
        "audit_log.jsonl",
        "audit_chain.json",
        "manifest.json",
        "README.md",
    ] {
        assert!(dir.join(f).is_file(), "missing {f}");
    }
    let human = std::fs::read_to_string(dir.join("phase2_coding_human.csv")).unwrap();
    assert!(human.contains(&format!("{},TECH-ADV,1", u(3))), "{human}");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    for f in manifest["files"].as_array().unwrap() {
        let bytes = std::fs::read(dir.join(f["name"].as_str().unwrap())).unwrap();
        assert_eq!(cs_model::sha256_hex(&bytes), f["sha256"].as_str().unwrap());
    }
    for name in std::fs::read_dir(&dir).unwrap() {
        let path = name.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !text.contains('\u{2014}') && !text.contains('\u{2013}'),
            "dash in {}",
            path.display()
        );
    }
    let (_, exp2) = call(app, "POST", &format!("{I}/export"), None).await;
    assert_ne!(exp2["dir"], exp["dir"]);

    let (_, chain) = call(app, "GET", "/api/audit/verify", None).await;
    assert_eq!(chain["ok"], true, "{chain}");
    let (_, detail) = call(app, "GET", I, None).await;
    assert_eq!(detail["phase2"]["reconciled"], true);
    assert_eq!(detail["phase2"]["n_units"], 3);
}

/// Library, codebook, segmentation of INT-01 and a phase 1 where only U004 and
/// U006 stay in scope for phase 2. Returns the unit ids of INT-01.
async fn bootstrap_two_units(ctx: &Ctx) -> Vec<String> {
    let app = &ctx.app;
    let (st, b) = call(
        app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, s) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{s}");
    let (st, seg) = call(app, "POST", "/api/interviews/INT-01/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let units: Vec<String> = seg["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["unit_id"].as_str().unwrap().to_string())
        .collect();
    for (i, c) in [
        (0, "NA"),
        (1, "NA"),
        (2, "NA"),
        (3, "TECH"),
        (4, "NA"),
        (5, "TECH"),
    ] {
        let (st, r) = call(
            app,
            "PUT",
            &format!("/api/interviews/INT-01/phases/1/coding/{}", units[i]),
            Some(json!({ "codes": [c] })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }
    units
}

async fn code_phase2(app: &Router, unit: &str, list: &[&str]) {
    let (st, r) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/phases/2/coding/{unit}"),
        Some(json!({ "codes": list })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
}

#[tokio::test]
async fn v4_snapshots_and_review() {
    let ctx = setup();
    let app = &ctx.app;
    const I: &str = "/api/interviews/INT-01";
    const P2: &str = "/api/interviews/INT-01/phases/2";
    let units = bootstrap_two_units(&ctx).await;
    let (u4, u6) = (units[3].clone(), units[5].clone());

    code_phase2(app, &u4, &["TECH-ADV", "OUT-REALISED"]).await;
    code_phase2(app, &u6, &["TECH-COST", "ORG-SKILLS"]).await;

    let (st, parts) = call(
        app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    let parts = parts.as_array().unwrap().clone();
    assert_eq!(parts.len(), 1);
    let (st, run) = call(
        app,
        "POST",
        &format!("{P2}/runs"),
        Some(json!({ "run_label": "R1", "model_name": "model-a", "model_version": "1" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    let run_id = run["run_id"].as_i64().unwrap();
    let ai = |unit: &str| -> Vec<String> {
        if unit == u4 {
            codes(&["TECH-ADV", "OUT-REALISED"])
        } else {
            codes(&["TECH-COST"])
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

    // The candidate revises U006 after the freeze: live coding now equals the AI.
    code_phase2(app, &u6, &["TECH-COST"]).await;

    // Agreement is computed on the snapshot: 1 of 2 exact, although the live
    // coding would give 2 of 2.
    let (st, ag) = call(app, "GET", &format!("{I}/agreement"), None).await;
    assert_eq!(st, StatusCode::OK, "{ag}");
    let rep = &ag["reports"][0];
    assert_eq!(rep["label_a"], "Candidate", "{ag}");
    assert_eq!(rep["label_b"], "AI R1", "{ag}");
    assert_eq!(rep["n_units"], 2, "{rep}");
    assert_eq!(rep["n_exact"], 1, "snapshot, not live coding: {rep}");
    assert_eq!(rep["divergent_unit_ids"], json!([u6]), "{rep}");

    let (st, review) = call(app, "GET", &format!("{I}/review"), None).await;
    assert_eq!(st, StatusCode::OK, "{review}");
    let snaps = review["snapshots"].as_array().expect("snapshots array");
    assert_eq!(snaps.len(), 1, "{review}");
    assert_eq!(snaps[0]["run_label"], "R1");
    assert!(snaps[0]["snapshot_id"].as_i64().is_some());
    let sha = snaps[0]["sha256"].as_str().unwrap().to_string();
    assert_eq!(sha.len(), 64);
    let item = review["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["unit_id"] == u6.as_str())
        .unwrap_or_else(|| panic!("no review item for {u6}: {review}"))
        .clone();
    assert_eq!(item["kind"], "disagreement", "{item}");
    assert_eq!(
        item["human"],
        json!(["ORG-SKILLS", "TECH-COST"]),
        "snapshot"
    );
    assert_eq!(item["human_current"], json!(["TECH-COST"]), "{item}");
    assert_eq!(item["revised_since_snapshot"], true, "{item}");
    let item4 = review["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["unit_id"] == u4.as_str())
        .unwrap()
        .clone();
    assert_eq!(item4["revised_since_snapshot"], false, "{item4}");

    // The snapshot route.
    let (st, list) = call(app, "GET", &format!("{I}/snapshots"), None).await;
    assert_eq!(st, StatusCode::OK, "{list}");
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["batch_id"], 1);
    assert_eq!(list[0]["sha256"], sha.as_str());
    let snapped = list[0]["codings"]
        .as_object()
        .expect("codings is an object");
    assert_eq!(snapped.len(), 2);
    assert_eq!(snapped[&u6], json!(["ORG-SKILLS", "TECH-COST"]));
    assert_eq!(snapped[&u4], json!(["OUT-REALISED", "TECH-ADV"]));
}

#[tokio::test]
async fn v4_cases_across_interviews() {
    let ctx = setup();
    let app = &ctx.app;
    const I: &str = "/api/interviews/INT-01";
    let units = bootstrap_two_units(&ctx).await;
    let (st, seg2) = call(app, "POST", "/api/interviews/ECO-02/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg2}");
    let eco_unit = seg2["units"][1]["unit_id"].as_str().unwrap().to_string();
    assert!(eco_unit.starts_with("ECO-02-U"));
    let (u4, u6) = (units[3].clone(), units[5].clone());
    code_phase2(app, &u4, &["TECH-ADV"]).await;
    code_phase2(app, &u6, &["TECH-COST", "ORG-SKILLS"]).await;

    // A case attributing units of two interviews, given unsorted.
    let body = json!({
        "label": "Source tagging",
        "units": [u6, eco_unit, u4, u6],
        "decision": "adopted",
        "granularity": "item",
        "carrier_chosen": "RAIN RFID",
        "carriers_rejected": ["QR"],
        "account_type": "first-hand",
        "swiss_link": "Swiss retailer, supplier in the EU",
        "trigger": {
            "text": "Inventory accuracy",
            "contradictions": "The respondent also cites cost",
            "missing": "No figure on tag price",
            "evidence": [u4],
        },
    });
    let (st, case) = call(app, "POST", &format!("{I}/cases"), Some(body.clone())).await;
    assert!(
        st == StatusCode::OK || st == StatusCode::CREATED,
        "{st} {case}"
    );
    let case_id = case["case_id"].as_str().unwrap().to_string();
    assert_eq!(case_id, "INT-01-C1");
    let mut sorted = vec![u4.clone(), u6.clone(), eco_unit.clone()];
    sorted.sort();
    assert_eq!(case["units"], json!(sorted), "{case}");
    assert_eq!(case["interviews"], json!(["ECO-02", "INT-01"]), "{case}");
    assert_eq!(case["decision"], "adopted");
    assert_eq!(case["granularity"], "item");
    assert_eq!(case["carrier_chosen"], "RAIN RFID");
    assert_eq!(case["carriers_rejected"], json!(["QR"]));
    assert_eq!(case["account_type"], "first-hand");
    assert_eq!(case["swiss_link"], "Swiss retailer, supplier in the EU");
    assert_eq!(
        case["trigger"]["contradictions"],
        "The respondent also cites cost"
    );
    assert_eq!(case["trigger"]["missing"], "No figure on tag price");
    assert_eq!(case["trigger"]["evidence"], json!([u4]));

    // Evidence must be among the attributed units.
    let mut bad = body.clone();
    bad["label"] = json!("Bad");
    bad["units"] = json!([u4]);
    bad["trigger"]["evidence"] = json!([u6]);
    let (st, e) = call(app, "POST", &format!("{I}/cases"), Some(bad)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(e["kind"], "validation", "{e}");
    assert!(e["error"].as_str().unwrap().contains(&u6), "{e}");

    // Listings: all cases, then by interview (created-from or attributed).
    let (_, all) = call(app, "GET", "/api/cases", None).await;
    assert_eq!(all.as_array().unwrap().len(), 1, "{all}");
    assert_eq!(all[0]["case_id"], case_id.as_str());
    let (st, mine) = call(app, "GET", &format!("{I}/cases"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let (_, other) = call(app, "GET", "/api/interviews/ECO-02/cases", None).await;
    assert_eq!(
        other.as_array().unwrap().len(),
        1,
        "attributed unit: {other}"
    );

    // Documented: attributed units with a final phase 2 coding fill cells.
    let (_, cc) = call(app, "GET", "/api/cross-case", None).await;
    let cells: Vec<Value> = cc["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["case_id"] == case_id.as_str())
        .cloned()
        .collect();
    assert!(!cells.is_empty());
    assert!(cells.iter().all(|c| c["documented"] == true), "{cells:?}");
    assert!(
        cells
            .iter()
            .any(|c| c["documented"] == true && !c["codes"].as_array().unwrap().is_empty()),
        "{cells:?}"
    );

    // Detaching every unit leaves the case undocumented, with no code at all.
    let mut empty = case.clone();
    empty["units"] = json!([]);
    for f in ["trigger", "alternatives", "constraints", "choice", "result"] {
        empty[f]["evidence"] = json!([]);
    }
    let (st, upd) = call(app, "PUT", &format!("/api/cases/{case_id}"), Some(empty)).await;
    assert_eq!(st, StatusCode::OK, "{upd}");
    assert_eq!(upd["units"], json!([]));
    let (_, cc) = call(app, "GET", "/api/cross-case", None).await;
    let cells: Vec<&Value> = cc["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["case_id"] == case_id.as_str())
        .collect();
    assert!(!cells.is_empty());
    for c in cells {
        assert_eq!(c["documented"], false, "{c}");
        assert_eq!(c["codes"], json!([]), "{c}");
    }
}

#[tokio::test]
async fn react_build_is_served() {
    let ctx = setup();
    let (st, ct, cc, body) = raw(&ctx.app, "/").await;
    assert_eq!(st, StatusCode::OK);
    assert!(ct.starts_with("text/html"), "{ct}");
    assert_eq!(cc, "no-store");
    let html = String::from_utf8(body).unwrap();

    // Client-side routes fall back to index.html.
    let (st, ct, _, deep) = raw(&ctx.app, "/interview/INT-01/coding").await;
    assert_eq!(st, StatusCode::OK);
    assert!(ct.starts_with("text/html"));
    assert_eq!(String::from_utf8(deep).unwrap(), html);

    // The script referenced by index.html is served with its type, immutable.
    let src = html
        .split("src=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
        .find(|s| s.starts_with("/assets/") && s.ends_with(".js"))
        .expect("index.html references a script under /assets");
    let (st, ct, cc, _) = raw(&ctx.app, src).await;
    assert_eq!(st, StatusCode::OK);
    assert!(ct.contains("javascript"), "{ct}");
    assert!(cc.contains("immutable"), "{cc}");

    // Unknown API routes stay JSON errors.
    let (st, e) = call(&ctx.app, "GET", "/api/nope", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(e["kind"], "not_found");
}

#[tokio::test]
async fn errors_are_typed() {
    let ctx = setup();
    let (st, e) = call(&ctx.app, "GET", "/api/interviews/NOPE-9", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(e["kind"], "not_found");
    let (st, e) = call(&ctx.app, "GET", "/api/interviews/NOPE-9/phases/1", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{e}");
    let (st, e) = call(&ctx.app, "GET", "/api/interviews/NOPE-9/phases/3", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(e["kind"], "validation");
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

#[tokio::test]
async fn native_picker_only_in_desktop_mode() {
    let ctx = setup();
    let (status, body) = call(
        &ctx.app,
        "POST",
        "/api/pick",
        Some(json!({"kind": "folder"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{body}");
    let (_, state) = call(&ctx.app, "GET", "/api/state", None).await;
    assert_eq!(state["native_dialogs"], json!(false));

    // Desktop mode: the picker given at start-up answers the request.
    let dir = tempfile::tempdir().unwrap();
    let state = cs_app::prepare(
        cs_app::Startup {
            db: dir.path().join("sub/studio.db"),
            library: None,
            coding_db: None,
            actor: "candidate".into(),
            interviewer_names: vec![],
        },
        Some(Arc::new(|kind, title: &str| {
            assert_eq!(kind, cs_app::PickKind::File);
            assert_eq!(title, "Choose coding.db");
            Some(std::path::PathBuf::from("/tmp/chosen.db"))
        })),
    )
    .unwrap();
    let app = router(state);
    let (status, body) = call(
        &app,
        "POST",
        "/api/pick",
        Some(json!({"kind": "file", "title": "Choose coding.db"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["path"], json!("/tmp/chosen.db"));
    let (_, state) = call(&app, "GET", "/api/state", None).await;
    assert_eq!(state["native_dialogs"], json!(true));
}
