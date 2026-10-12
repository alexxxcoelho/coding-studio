//! End-to-end round trip through the HTTP API (v2, two coding phases) on a
//! temporary library: scan, segment, phase 1 (TOE contexts) with blind guard,
//! AI import, coding lock, arbitration and reconciliation, phase 2 restricted
//! to the reconciled families, inductive codes, phase 1 frozen, export,
//! audit chain. Plus the embedded React build and the native picker.

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
         INSERT INTO codes (code,family,label,definition,origin,active) VALUES
          ('TECH','TECH','Technology, unrefined','Family-level code.','legacy',1),
          ('TECH-ADV','TECH','Relative advantage','Perceived advantage over the status quo.','codebook',1),
          ('TECH-COST','TECH','Cost','Lifecycle cost and investment burden.','codebook',1),
          ('ORG-SKILLS','ORG','Skills','Skills and expertise.','codebook',1),
          ('ENV-PARTNER','ENV','Partner pressure','Partner pressure or mandate.','codebook',1),
          ('OUT-REALISED','OUTCOME','Realised outcome','A result supported by evidence.','codebook',1);",
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
async fn two_phase_round_trip() {
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
    let (st, p1book) = call(app, "GET", "/api/codebook/phase1", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        names(&p1book["codes"]),
        ["TECH", "ORG", "ENV", "CONTEXT", "INDUCTIVE-PENDING"]
    );
    let (st, scan) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{scan}");
    assert_eq!(scan["interviews"].as_array().unwrap().len(), 2);
    assert!(scan["interviews"][0]["phase1"].is_object());

    let (st, seg2) = call(app, "POST", "/api/interviews/ECO-02/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg2}");
    assert_eq!(seg2["units"][0]["role"], "interviewer");

    let (st, _) = call(app, "GET", &format!("{I}/file?path=..%2Fsecret"), None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, f) = call(
        app,
        "GET",
        &format!("{I}/file?path=02_transcript_cleaned_diarised.md"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{f}");
    assert_eq!(f["changed_since_scan"], false);
    assert_eq!(f["sha256"].as_str().unwrap().len(), 64);

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

    // ---- Phase 1: TOE contexts.
    let (st, rows) = call(app, "GET", &format!("{P1}/units"), None).await;
    assert_eq!(st, StatusCode::OK, "{rows}");
    assert_eq!(rows.as_array().unwrap().len(), 6);
    assert_eq!(
        names(&rows[0]["allowed"]),
        ["TECH", "ORG", "ENV", "CONTEXT", "INDUCTIVE-PENDING"]
    );

    let (st, parts) = call(
        app,
        "POST",
        &format!("{P1}/prompts"),
        Some(json!({ "batch_size": 0 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    let part1 = parts[0].clone();
    assert_eq!(part1["phase"], 1);
    assert!(part1["text"].as_str().unwrap().contains("Phase 1 of 2"));
    let (_, listed) = call(app, "GET", &format!("{P1}/prompts"), None).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let (st, run) = call(
        app,
        "POST",
        &format!("{P1}/runs"),
        Some(json!({ "run_label": "R1", "model_name": "test-model", "temperature": 0.0 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    assert_eq!(run["phase"], 1);
    assert_eq!(run["meta"]["prompt_id_version"], cs_ai::PROMPT_VERSION);
    let run1 = run["run_id"].as_i64().unwrap();

    // Phase 1 human coding: INT turns NA, respondent turns on the contexts.
    let human1 = |id: &str| -> Vec<String> {
        match units.iter().position(|x| x == id).unwrap() {
            1 => codes(&["ENV"]),
            3 => codes(&["TECH"]),
            5 => codes(&["TECH", "ORG"]),
            _ => codes(&["NA"]),
        }
    };
    let out1 = format!("{P1}/runs/{run1}/outputs");
    // Blind guard before the human coding is complete.
    let (st, e) = call(
        app,
        "POST",
        &out1,
        Some(json!({ "raw": answer(&part1, human1), "part": 1, "of": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    assert_eq!(e["kind"], "guard");

    // A study-specific code is refused in phase 1.
    let (st, e) = call(
        app,
        "PUT",
        &format!("{P1}/coding/{}", u(1)),
        Some(json!({ "codes": ["TECH-ADV"] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    for id in &units {
        let (st, saved) = call(
            app,
            "PUT",
            &format!("{P1}/coding/{id}"),
            Some(json!({ "codes": human1(id), "memo": "test" })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{saved}");
        assert_eq!(saved["unit_id"], id.as_str());
    }
    // Phase 2 cannot start before phase 1 is reconciled.
    let (st, e) = call(
        app,
        "PUT",
        &format!("{P2}/coding/{}", u(1)),
        Some(json!({ "codes": ["ENV-PARTNER"] })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");

    // Invalid answer: every problem is listed.
    let (st, e) = call(
        app,
        "POST",
        &out1,
        Some(json!({ "raw": answer(&part1, |_| codes(&["NOT-A-CODE"])), "part": 1, "of": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert!(e["issues"].as_array().unwrap().len() >= 6);

    // Valid answer, diverging on unit 3 only.
    let ai1 = |id: &str| -> Vec<String> {
        if id == units[3] {
            codes(&["ORG"])
        } else {
            human1(id)
        }
    };
    let (st, run) = call(
        app,
        "POST",
        &out1,
        Some(json!({ "raw": answer(&part1, ai1), "part": 1, "of": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    assert_eq!(run["complete"], true);

    // Human coding is now locked in phase 1.
    let (st, _) = call(
        app,
        "PUT",
        &format!("{P1}/coding/{}", u(0)),
        Some(json!({ "codes": ["TECH"] })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, state1) = call(app, "GET", P1, None).await;
    assert_eq!(state1["locked"], true);
    assert_eq!(state1["reconciled"], false);
    assert_eq!(state1["guard"]["complete"], true);

    let (st, ag) = call(app, "GET", &format!("{P1}/agreement"), None).await;
    assert_eq!(st, StatusCode::OK, "{ag}");
    assert_eq!(ag["reports"][0]["n_units"], 6);
    assert_eq!(ag["reports"][0]["n_exact"], 5);
    assert!(ag["markdown"].as_str().unwrap().contains("Phase 1"));

    let (st, div) = call(app, "GET", &format!("{P1}/divergences"), None).await;
    assert_eq!(st, StatusCode::OK, "{div}");
    assert_eq!(div["n_divergences"], 1);
    assert_eq!(div["items"][0]["divergence"]["unit_id"], u(3).as_str());
    assert_eq!(names(&div["items"][0]["allowed"]).len(), 5);

    let arb1 = format!("{P1}/arbitration/{}", u(3));
    let (st, _) = call(
        app,
        "PUT",
        &arb1,
        Some(json!({ "choice": {"type": "human"}, "final_codes": ["TECH"], "rationale": "  " })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        app,
        "PUT",
        &format!("{P1}/arbitration/{}", u(1)),
        Some(json!({ "choice": {"type": "human"}, "final_codes": ["ENV"], "rationale": "x" })),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "a non-divergent unit is not arbitrated"
    );
    let (st, a) = call(
        app,
        "PUT",
        &arb1,
        Some(
            json!({ "choice": {"type": "human"}, "final_codes": ["TECH"],
                     "rationale": "The respondent speaks of tag cost, a technology property." }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{a}");
    assert_eq!(a["phase"], 1);
    let (_, fin1) = call(app, "GET", &format!("{P1}/final"), None).await;
    assert_eq!(fin1["reconciled"], true, "{fin1}");
    assert_eq!(fin1["finals"].as_array().unwrap().len(), 6);

    // ---- Phase 2: study-specific codes on the relevant units.
    let (st, rows2) = call(app, "GET", &format!("{P2}/units"), None).await;
    assert_eq!(st, StatusCode::OK, "{rows2}");
    let scope2: Vec<String> = rows2
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["unit"]["unit_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(scope2, [u(1), u(3), u(5)]);
    assert_eq!(rows2[1]["phase1_final"], json!(["TECH"]));
    let allowed3 = names(&rows2[1]["allowed"]);
    assert_eq!(allowed3, ["TECH-ADV", "TECH-COST", "OUT-REALISED"]);
    assert_eq!(
        names(&rows2[2]["allowed"]),
        ["TECH-ADV", "TECH-COST", "ORG-SKILLS", "OUT-REALISED"]
    );

    // A code outside the unit's families is refused.
    let (st, e) = call(
        app,
        "PUT",
        &format!("{P2}/coding/{}", u(3)),
        Some(json!({ "codes": ["ORG-SKILLS"] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");

    // Inductive code: definition and reason are required.
    let (st, _) = call(
        app,
        "POST",
        "/api/codebook/codes",
        Some(json!({ "code": "SOURCE-TAGGING", "label": "Source tagging", "definition": "Tags applied by the supplier." })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, book) = call(
        app,
        "POST",
        "/api/codebook/codes",
        Some(json!({ "code": "SOURCE-TAGGING", "label": "Source tagging",
                     "definition": "Tags applied by the supplier before goods reach the retailer.",
                     "example": "our supplier tags every carton at the source",
                     "reason": "Recurrent mechanism not covered by ENV-PARTNER." })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{book}");
    let added = book["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "SOURCE-TAGGING")
        .unwrap()
        .clone();
    assert_eq!(added["origin"], "inductive");
    assert!(added["created_at"].is_string());

    let human2 = |id: &str| -> Vec<String> {
        if id == units[1] {
            codes(&["ENV-PARTNER", "SOURCE-TAGGING"])
        } else if id == units[3] {
            codes(&["TECH-ADV"])
        } else {
            codes(&["TECH-COST", "ORG-SKILLS"])
        }
    };
    // Phase 1 is frozen as soon as phase 2 coding starts.
    let (st, _) = call(
        app,
        "PUT",
        &format!("{P2}/coding/{}", u(1)),
        Some(json!({ "codes": human2(&u(1)), "excerpt": "our supplier tags every carton" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, e) = call(
        app,
        "POST",
        &format!("{P1}/runs"),
        Some(json!({ "run_label": "R2", "model_name": "other" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    let (st, _) = call(
        app,
        "PUT",
        &arb1,
        Some(json!({ "choice": {"type": "custom"}, "final_codes": ["ORG"], "rationale": "late" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, state1) = call(app, "GET", P1, None).await;
    assert_eq!(state1["frozen"], true);

    // Phase 2 prompt carries the allowed codes; inductive codes never reach the AI.
    let (st, parts2) = call(
        app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{parts2}");
    assert_eq!(parts2.as_array().unwrap().len(), 2);
    let text2 = parts2[0]["text"].as_str().unwrap();
    assert!(text2.contains("allowed_codes"));
    assert!(text2.contains("Phase 2 of 2"));
    assert!(!text2.contains("SOURCE-TAGGING"));

    let (st, run) = call(
        app,
        "POST",
        &format!("{P2}/runs"),
        Some(json!({ "run_label": "R1", "model_name": "test-model" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{run}");
    let run2 = run["run_id"].as_i64().unwrap();
    let out2 = format!("{P2}/runs/{run2}/outputs");
    let ai2 = |id: &str| -> Vec<String> {
        if id == units[1] {
            codes(&["ENV-PARTNER"])
        } else {
            human2(id)
        }
    };
    let (st, e) = call(
        app,
        "POST",
        &out2,
        Some(json!({ "raw": answer(&parts2[0], ai2), "part": 1, "of": 2 })),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::CONFLICT,
        "blind guard of phase 2 while human coding is incomplete: {e}"
    );
    for id in [u(3), u(5)] {
        let (st, e) = call(
            app,
            "PUT",
            &format!("{P2}/coding/{id}"),
            Some(json!({ "codes": human2(&id) })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{e}");
    }
    // The AI may not use an inductive code.
    let (st, e) = call(
        app,
        "POST",
        &out2,
        Some(json!({ "raw": answer(&parts2[0], human2), "part": 1, "of": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    for p in parts2.as_array().unwrap() {
        let (st, run) = call(
            app,
            "POST",
            &out2,
            Some(json!({ "raw": answer(p, ai2), "part": p["part"], "of": p["of"] })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{run}");
    }
    let (_, div2) = call(app, "GET", &format!("{P2}/divergences"), None).await;
    assert_eq!(div2["n_divergences"], 1, "{div2}");
    assert!(names(&div2["items"][0]["allowed"]).contains(&"SOURCE-TAGGING".to_string()));
    let (st, a) = call(
        app,
        "PUT",
        &format!("{P2}/arbitration/{}", u(1)),
        Some(
            json!({ "choice": {"type": "custom"}, "final_codes": ["ENV-PARTNER", "SOURCE-TAGGING"],
                     "rationale": "The supplier's source tagging is a distinct mechanism." }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{a}");
    let (_, fin2) = call(app, "GET", &format!("{P2}/final"), None).await;
    assert_eq!(fin2["reconciled"], true, "{fin2}");

    let (_, detail) = call(app, "GET", I, None).await;
    assert_eq!(detail["phase2_started"], true);
    assert_eq!(detail["phase1"]["reconciled"], true);
    assert_eq!(detail["phase2"]["reconciled"], true);
    assert_eq!(detail["phase2"]["n_units"], 3);

    // Export: both phases, inductive log, manifest hashes, never overwritten.
    let (st, exp) = call(app, "POST", &format!("{I}/export"), None).await;
    assert_eq!(st, StatusCode::OK, "{exp}");
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    assert!(dir.starts_with(ctx.db.parent().unwrap()));
    for f in [
        "units.csv",
        "phase1_coding_human.csv",
        "phase1_coding_final.csv",
        "phase1_agreement.md",
        "phase1_adjudication_log.md",
        "phase2_coding_human.csv",
        "phase2_human_full.csv",
        "phase2_coding_final.csv",
        "phase2_adjudication_log.md",
        "inductive_log.csv",
        "audit_log.jsonl",
        "audit_chain.json",
        "manifest.json",
        "README.md",
    ] {
        assert!(dir.join(f).is_file(), "missing {f}");
    }
    let final2 = std::fs::read_to_string(dir.join("phase2_coding_final.csv")).unwrap();
    assert!(final2.contains("ENV-PARTNER;SOURCE-TAGGING"));
    let log = std::fs::read_to_string(dir.join("inductive_log.csv")).unwrap();
    assert!(log.contains("SOURCE-TAGGING") && log.contains("Recurrent mechanism"));
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
    let (_, events) = call(app, "GET", "/api/audit?interview_id=INT-01", None).await;
    assert!(events.as_array().unwrap().len() > 20);
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
