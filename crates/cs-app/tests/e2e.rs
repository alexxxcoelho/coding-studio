#![allow(clippy::needless_range_loop)]
//! The v4 procedure, end to end, through the HTTP router on a temporary
//! library and database, with fictional data.
//!
//! One scenario, nine steps, in the order of docs/METHOD.md: coding in two
//! phases, the blind prompt and its frozen reference, the AI answer and the
//! review, flags, corrections and verification, claims, cases across two
//! interviews, the research question memo and the saturation table, the two
//! export packages, and the audit chain.
//!
//! The scenario is written as a report of soft checks: a failed check is
//! recorded with its step and the scenario goes on, so one run shows every
//! gap at once. The test fails at the end if any check failed. Plumbing that a
//! later step cannot do without (a status, a missing field) is also a check,
//! and later steps read JSON through accessors that never panic.
//!
//! Where a route or a behaviour does not exist yet, the expectation is
//! written next to the check as `// MISSING: ...`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use cs_app::{router, AppState};
use cs_store::Store;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

// ------------------------------------------------------------------ fixtures

/// Eight respondent turns, in order, after one interviewer question each.
/// Respondent turn r is unit INT-01-U(2r+2); the interviewer's question is the
/// unit before it. All names, figures and facts are invented.
const RESPONDENTS: [&str; 8] = [
    "I run the logistics of a mid-sized retailer and our supplier tags every carton at the source.",
    "The inventory accuracy went from sixty to ninety-eight percent in one year.",
    "Tags were expensive at first, and our team lacked the skills for the ERP integration.",
    "Our largest customer asked us to label all pallets from next spring.",
    "We have not yet decided whether to extend the tags to the smaller articles.",
    "A competitor told us that they gave up because the readers failed near metal shelves.",
    "The warehouse team now counts a full aisle in ten minutes instead of two hours.",
    "Without the training budget approved by the board we would not have started.",
];

const QUESTIONS: [&str; 8] = [
    "Thank you for your time. Could you describe your role?",
    "What made the business case work?",
    "And the costs?",
    "Who else was involved in the decision?",
    "What is still open?",
    "Did you hear of other companies trying this?",
    "What changed for the team?",
    "What would you do differently?",
];

fn transcript() -> String {
    let mut t = String::from(
        "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n",
    );
    for r in 0..8 {
        let s = r * 40;
        t.push_str(&format!(
            "**INT `[{:02}:{:02}]`:** {}\n\n**RESP `[{:02}:{:02}]`:** {}\n\n",
            s / 60,
            s % 60,
            QUESTIONS[r],
            (s + 10) / 60,
            (s + 10) % 60,
            RESPONDENTS[r]
        ));
    }
    t
}

const VTT: &str = "WEBVTT\n\nabc/1-0\n00:00:01.000 --> 00:00:03.000\n<v Interviewer Person A>Hello, can you hear me?</v>\n\nabc/2-0\n00:00:04.000 --> 00:00:08.000\n<v Jane Roe>Yes, perfectly. We use RFID in two stores.</v>\n\nabc/3-0\n00:00:09.000 --> 00:00:12.000\n<v Interviewer Person A>Why only two?</v>\n\nabc/4-0\n00:00:13.000 --> 00:00:18.000\n<v Jane Roe>Our partner did not want to tag at source.</v>\n";

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
    root: PathBuf,
    db: PathBuf,
    coding_db: PathBuf,
}

fn setup() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Interviews");
    std::fs::create_dir_all(root.join("INT-01_Test")).unwrap();
    std::fs::write(
        root.join("INT-01_Test/02_transcript_cleaned_diarised.md"),
        transcript(),
    )
    .unwrap();
    let coding_db = dir.path().join("coding.db");
    make_codebook(&coding_db);
    let db = dir.path().join("studio/coding_studio.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let store = Store::open(&db).unwrap();
    {
        let mut cfg = cs_model::StudyConfig::default();
        cfg.id_prefixes = vec!["INT".into(), "ECO".into()];
        cfg.interviewers = vec!["Interviewer Person A".into()];
        store.set_study_config(&cfg, "candidate").unwrap();
    }
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

/// An AI answer for one prompt part, wrapped in prose and a fence as chat
/// interfaces often do.
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

/// Unit id of INT-01, 1 based: INT-01-U001.
fn uid(n: usize) -> String {
    format!("INT-01-U{n:03}")
}

/// Unit id of respondent turn r (0 based).
fn resp(r: usize) -> String {
    uid(2 * r + 2)
}

// ------------------------------------------------------------ soft accessors

fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}

/// Sorted strings of a JSON array.
fn sset(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = arr(v)
        .iter()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

fn sorted(list: &[&str]) -> Vec<String> {
    let mut out = codes(list);
    out.sort();
    out
}

/// First element of `v` (an array of objects) whose `key` equals `val`.
fn find_by(v: &Value, key: &str, val: &str) -> Value {
    arr(v)
        .into_iter()
        .find(|e| e[key] == val)
        .unwrap_or(Value::Null)
}

fn text_of(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap_or_default()
}

fn json_of(dir: &Path, name: &str) -> Value {
    serde_json::from_str(&text_of(dir, name)).unwrap_or(Value::Null)
}

fn section<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let Some(a) = text.find(from) else { return "" };
    let rest = &text[a..];
    let b = rest[from.len()..]
        .find(to)
        .map_or(rest.len(), |i| i + from.len());
    &rest[..b]
}

// -------------------------------------------------------------------- report

#[derive(Default)]
struct Rep {
    step: String,
    rows: Vec<(String, bool, String)>,
}

impl Rep {
    fn step(&mut self, name: &str) {
        self.step = name.to_string();
        eprintln!("== {name}");
    }

    fn ok(&mut self, cond: bool, what: &str) -> bool {
        let line = format!("{} {what}", if cond { "pass" } else { "FAIL" });
        eprintln!("   {line}");
        self.rows.push((self.step.clone(), cond, what.to_string()));
        cond
    }

    fn eq<T: PartialEq + Debug>(&mut self, left: T, right: T, what: &str) -> bool {
        let cond = left == right;
        let label = if cond {
            what.to_string()
        } else {
            format!("{what}: left {left:?}, right {right:?}")
        };
        self.ok(cond, &label)
    }

    /// Status is one of `want`; on failure the body is part of the message.
    fn status(&mut self, got: StatusCode, want: &[StatusCode], body: &Value, what: &str) -> bool {
        let cond = want.contains(&got);
        let label = if cond {
            what.to_string()
        } else {
            let mut shown = body.to_string();
            shown.truncate(300);
            format!("{what}: status {got}, wanted {want:?}, body {shown}")
        };
        self.ok(cond, &label)
    }

    fn finish(self) {
        let mut per_step: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        let mut order: Vec<&str> = Vec::new();
        for (step, ok, _) in &self.rows {
            if !order.contains(&step.as_str()) {
                order.push(step);
            }
            let e = per_step.entry(step).or_default();
            e.1 += 1;
            if *ok {
                e.0 += 1;
            }
        }
        eprintln!("\n---- summary");
        for s in order {
            let (p, n) = per_step[s];
            eprintln!("{p:>3}/{n:<3} {s}");
        }
        let failed: Vec<String> = self
            .rows
            .iter()
            .filter(|(_, ok, _)| !ok)
            .map(|(s, _, w)| format!("[{s}] {w}"))
            .collect();
        assert!(
            failed.is_empty(),
            "{} of {} checks failed:\n{}",
            failed.len(),
            self.rows.len(),
            failed.join("\n")
        );
    }
}

async fn actions(app: &Router) -> BTreeSet<String> {
    let (_, events) = call(app, "GET", "/api/audit", None).await;
    arr(&events)
        .iter()
        .filter_map(|e| e["action"].as_str().map(str::to_string))
        .collect()
}

/// Every wanted action appears in the audit log.
async fn audited(app: &Router, rep: &mut Rep, wanted: &[&str]) {
    let have = actions(app).await;
    for w in wanted {
        rep.ok(have.contains(*w), &format!("audit log has {w}"));
    }
}

async fn put_coding(
    app: &Router,
    rep: &mut Rep,
    phase: u8,
    interview: &str,
    unit: &str,
    body: Value,
) -> Value {
    let (st, r) = call(
        app,
        "PUT",
        &format!("/api/interviews/{interview}/phases/{phase}/coding/{unit}"),
        Some(body),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK],
        &r,
        &format!("phase {phase} coding of {unit}"),
    );
    r
}

// ------------------------------------------------------------------ scenario

#[tokio::test]
async fn v4_procedure_end_to_end() {
    let ctx = setup();
    let app = &ctx.app;
    let mut rep = Rep::default();
    const I: &str = "/api/interviews/INT-01";
    const P1: &str = "/api/interviews/INT-01/phases/1";
    const P2: &str = "/api/interviews/INT-01/phases/2";
    let all_units: Vec<String> = (1..=16).map(uid).collect();

    // =====================================================================
    // 1. Scan, segment, phase 1, phase 2 (several codes, a label, resolution)
    // =====================================================================
    rep.step("1 scan, segment, coding");
    let (st, book) = call(
        app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &book, "codebook import");
    rep.eq(arr(&book["codes"]).len(), 6, "six imported codes");

    let (st, scan) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &scan, "first scan");
    rep.eq(
        arr(&scan["interviews"]).len(),
        1,
        "one interview after the first scan",
    );
    let (st, seg) = call(app, "POST", &format!("{I}/segment"), None).await;
    rep.status(st, &[StatusCode::OK], &seg, "segment INT-01");
    let seg_ids: Vec<String> = arr(&seg["units"])
        .iter()
        .map(|u| u["unit_id"].as_str().unwrap_or("").to_string())
        .collect();
    rep.eq(seg_ids.clone(), all_units.clone(), "16 units, U001 to U016");
    rep.eq(
        &seg["units"][1]["role"],
        &json!("respondent"),
        "U002 is a respondent turn",
    );
    rep.eq(
        &seg["units"][0]["role"],
        &json!("interviewer"),
        "U001 is an interviewer turn",
    );
    let original_u10_text = seg["units"][9]["text"].as_str().unwrap_or("").to_string();

    // Phase 1: every unit gets a provisional category; the interviewer's are NA.
    let p1: [&[&str]; 8] = [
        &["CONTEXT", "ENV"],
        &["TECH"],
        &["TECH", "ORG"],
        &["ENV"],
        &["TECH"],
        &["TECH"],
        &["TECH"],
        &["ORG"],
    ];
    for r in 0..8 {
        put_coding(
            app,
            &mut rep,
            1,
            "INT-01",
            &uid(2 * r + 1),
            json!({ "codes": ["NA"] }),
        )
        .await;
        put_coding(
            app,
            &mut rep,
            1,
            "INT-01",
            &resp(r),
            json!({ "codes": p1[r] }),
        )
        .await;
    }
    let (_, ph1) = call(app, "GET", P1, None).await;
    rep.eq(
        &ph1["summary"]["n_units"],
        &json!(16),
        "phase 1 sees 16 units",
    );
    rep.eq(
        &ph1["summary"]["n_human_coded"],
        &json!(16),
        "phase 1: 16 units coded",
    );
    rep.eq(
        &ph1["guard"]["complete"],
        &json!(true),
        "phase 1 guard: complete",
    );
    let (_, rows) = call(app, "GET", &format!("{P2}/units"), None).await;
    rep.eq(
        arr(&rows).len(),
        8,
        "eight units in phase 2 scope (phase 1 not NA)",
    );

    // Phase 2: concrete codes, several on one unit, an excerpt and a memo.
    // The memo is a sentinel that must never reach a prompt.
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(1),
        json!({ "codes": ["TECH-ADV", "OUT-REALISED"], "excerpt": "sixty to ninety-eight percent",
                 "memo": "SENTINEL-MEMO-ALPHA" }),
    )
    .await;
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(2),
        json!({ "codes": ["TECH-COST", "ORG-SKILLS"] }),
    )
    .await;
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(3),
        json!({ "codes": ["ENV-PARTNER"] }),
    )
    .await;
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(4),
        json!({ "codes": ["TECH-COST"] }),
    )
    .await;
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(5),
        json!({ "codes": ["TECH-COST"] }),
    )
    .await;
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(6),
        json!({ "codes": ["TECH-ADV"] }),
    )
    .await;
    let r = put_coding(app, &mut rep, 2, "INT-01", &resp(1), json!({ "codes": ["TECH-ADV", "OUT-REALISED"], "excerpt": "sixty to ninety-eight percent", "memo": "SENTINEL-MEMO-ALPHA" })).await;
    rep.eq(
        sset(&r["codes"]),
        sorted(&["TECH-ADV", "OUT-REALISED"]),
        "U004 carries two codes",
    );

    // A label to clarify on U002.
    let (st, label) = call(
        app,
        "POST",
        &format!("{I}/units/{}/to-clarify", resp(0)),
        Some(json!({ "label": "sentinel label supplier tagging", "note": "SENTINEL-NOTE who pays?" })),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &label,
        "label to clarify on U002",
    );
    let tid = label["id"].as_i64().unwrap_or(0);

    // The batch cannot leave while U016 is uncoded, and no snapshot is taken.
    let (st, e) = call(
        app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 4 })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "prompt refused while U016 has no phase 2 coding",
    );
    rep.ok(
        e["error"].as_str().unwrap_or("").contains(&resp(7)),
        "refusal names U016",
    );
    let (st, snaps) = call(app, "GET", &format!("{I}/snapshots"), None).await;
    rep.status(st, &[StatusCode::OK], &snaps, "snapshots route answers");
    rep.eq(arr(&snaps).len(), 0, "a refused prompt takes no snapshot");
    put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(7),
        json!({ "codes": ["ORG-SKILLS"] }),
    )
    .await;

    // The label becomes a field code. Its texts name units: the prompt must redact them.
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
                "label": "Supplier tags at source (see INT-01-U002)",
                "definition": "Items arrive tagged by the supplier, as in INT-01-U002 and ECO-02-U004.",
                "example": "INT-01-U002",
                "reason": "Created from INT-01-U002",
                "toe": "E",
                "research_questions": ["SQ3"],
            },
        })),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK],
        &res,
        "label resolved into a new field code",
    );
    rep.eq(
        &res["resolved"][0]["resolved"]["kind"],
        &json!("new_code"),
        "resolution kind",
    );
    let new = find_by(&res["codebook"]["codes"], "code", "SUPPLIER-TAGGING");
    rep.eq(
        &new["origin"],
        &json!("field"),
        "the new code is a field code",
    );
    let (_, rows) = call(app, "GET", &format!("{P2}/units"), None).await;
    let r2 = find_by(
        &Value::Array(arr(&rows).iter().map(|r| r["unit"].clone()).collect()),
        "unit_id",
        &resp(0),
    );
    rep.ok(!r2.is_null(), "U002 is in the phase 2 list");
    let row2 = arr(&rows)
        .into_iter()
        .find(|r| r["unit"]["unit_id"] == resp(0).as_str())
        .unwrap_or(Value::Null);
    rep.eq(
        sset(&row2["coding"]["codes"]),
        sorted(&["SUPPLIER-TAGGING"]),
        "U002 now carries the field code",
    );
    rep.ok(
        !row2["to_clarify"][0]["resolved"].is_null(),
        "U002 label is resolved in the row",
    );

    // Annotations that must not reach the AI: a flag, and a claim with a memo.
    let (st, f) = call(
        app,
        "PUT",
        &format!("{I}/units/{}/flags", resp(5)),
        Some(json!({ "flags": ["SECOND-HAND", "VERIFY"] })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &f, "flags on U012 before the prompt");
    let (st, c) = call(
        app,
        "POST",
        &format!("{I}/claims"),
        Some(json!({
            "unit_id": resp(3),
            "excerpt": "largest customer asked us to label all pallets",
            "codes": ["ENV-PARTNER"],
            "ev_level": "EV2",
            "provenance": "PROBED",
            "role": "constraint",
            "memo": "SENTINEL-CLAIM-MEMO",
        })),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &c,
        "claim on U008 before the prompt",
    );
    audited(
        app,
        &mut rep,
        &[
            "library.interview.add",
            "codebook.import",
            "segmentation.save",
            "coding.set",
            "to_clarify.create",
            "to_clarify.resolve",
            "codebook.code.create",
            "unit.flags",
            "claim.create",
        ],
    )
    .await;

    // =====================================================================
    // 2. Prompt generation: snapshot, blindness, redaction
    // =====================================================================
    rep.step("2 prompt and snapshot");
    let (st, parts) = call(
        app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 4 })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &parts, "prompt generation");
    let parts = arr(&parts);
    rep.eq(parts.len(), 2, "eight units at four per part: two parts");
    let all_text: String = parts
        .iter()
        .map(|p| p["text"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    for p in &parts {
        rep.eq(
            &p["prompt_version"],
            &json!("CS-P04-EN v3.1"),
            "prompt_version field",
        );
        rep.ok(
            p["text"]
                .as_str()
                .unwrap_or("")
                .contains("prompt_version: CS-P04-EN v3.1"),
            "prompt text names its version",
        );
        rep.eq(&p["batch_id"], &json!(1), "batch 1");
    }

    // The human snapshot is created with the prompts.
    let (st, snaps) = call(app, "GET", &format!("{I}/snapshots"), None).await;
    rep.status(st, &[StatusCode::OK], &snaps, "GET snapshots");
    let snaps = arr(&snaps);
    rep.eq(snaps.len(), 1, "one snapshot for the batch");
    let snap = snaps.first().cloned().unwrap_or(Value::Null);
    rep.eq(&snap["batch_id"], &json!(1), "snapshot of batch 1");
    rep.eq(&snap["phase"], &json!(2), "snapshot of phase 2");
    rep.eq(
        snap["codings"].as_object().map_or(0, |o| o.len()),
        8,
        "snapshot holds the eight units",
    );
    rep.eq(
        sset(&snap["codings"][resp(0).as_str()]),
        sorted(&["SUPPLIER-TAGGING"]),
        "snapshot U002 holds the resolved field code",
    );
    rep.eq(
        sset(&snap["codings"][resp(2).as_str()]),
        sorted(&["TECH-COST", "ORG-SKILLS"]),
        "snapshot U006",
    );
    rep.eq(
        snap["sha256"].as_str().map_or(0, str::len),
        64,
        "snapshot digest",
    );
    rep.eq(
        &snap["segmentation_hash"],
        &parts[0]["segmentation_hash"],
        "snapshot segmentation hash",
    );
    rep.eq(
        &snap["codebook_hash"],
        &parts[0]["codebook_hash"],
        "snapshot codebook hash",
    );

    // Blindness: nothing the candidate wrote beyond the codebook and the units.
    for sentinel in [
        "SENTINEL-MEMO",
        "SENTINEL-NOTE",
        "SENTINEL-CLAIM",
        "sentinel label",
        "supplier tagging",
    ] {
        rep.ok(
            !all_text.contains(sentinel),
            &format!("prompt lacks {sentinel}"),
        );
    }
    for human in ["\"CONTEXT\"", "\"TECH\"", "\"ORG\"", "\"ENV\""] {
        rep.ok(
            !all_text.contains(human),
            &format!("prompt lacks phase 1 context {human}"),
        );
    }
    for annotation in [
        "CO-CONSTRUCTED",
        "SECOND-HAND",
        "UNFINISHED",
        "VERIFY",
        "EV2",
        "PROBED",
        "constraint",
        "excerpt",
    ] {
        rep.ok(
            !all_text.contains(annotation),
            &format!("prompt lacks annotation {annotation}"),
        );
    }
    let mut seen_units = 0;
    for p in &parts {
        let text = p["text"].as_str().unwrap_or("");
        let units_block = section(text, "## Units", "## Output");
        let book_block = section(text, "## Codebook", "## Rules");
        for line in units_block.lines().filter(|l| l.starts_with('{')) {
            seen_units += 1;
            let v: Value = serde_json::from_str(line).unwrap_or(Value::Null);
            let keys: BTreeSet<String> = v
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            rep.eq(
                keys,
                ["speaker", "text", "timestamp", "unit_id"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                "a unit line carries only unit_id, speaker, timestamp, text",
            );
            for code in [
                "TECH-ADV",
                "TECH-COST",
                "ORG-SKILLS",
                "ENV-PARTNER",
                "OUT-REALISED",
                "SUPPLIER-TAGGING",
            ] {
                rep.ok(!line.contains(code), "a unit line holds no code");
            }
        }
        rep.ok(
            !book_block.contains("INT-01-U") && !book_block.contains("ECO-02-U"),
            "no unit id inside the code texts",
        );
        rep.ok(
            book_block.contains("[unit]") || !book_block.contains("SUPPLIER-TAGGING"),
            "unit ids in code texts are redacted as [unit]",
        );
    }
    rep.eq(seen_units, 8, "the eight units are in the prompt parts");
    rep.ok(
        all_text.contains("SUPPLIER-TAGGING"),
        "the field code is offered to the AI",
    );
    rep.ok(all_text.contains("[unit]"), "redaction marker present");

    // A run and its answer.
    let (st, run) = call(
        app,
        "POST",
        &format!("{P2}/runs"),
        Some(json!({ "run_label": "R1", "model_name": "model-a", "model_version": "1" })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &run, "run created");
    rep.eq(&run["batch_id"], &json!(1), "run attached to batch 1");
    let run_id = run["run_id"].as_i64().unwrap_or(0);
    audited(
        app,
        &mut rep,
        &["prompt.generate", "human_snapshot.take", "ai.run.create"],
    )
    .await;

    // =====================================================================
    // 3. Import, review on the snapshot, revision after import, arbitration
    // =====================================================================
    rep.step("3 import and review");
    let outputs = format!("{P2}/runs/{run_id}/outputs");
    let (st, e) = call(
        app,
        "POST",
        &outputs,
        Some(json!({ "raw": answer(&parts[0], |_| codes(&["NOT-A-CODE"])), "part": 1, "of": 2 })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "an answer with an unknown code is refused",
    );
    rep.ok(!arr(&e["issues"]).is_empty(), "the refusal lists issues");

    let ai: BTreeMap<String, Vec<String>> = [
        (resp(0), codes(&["SUPPLIER-TAGGING"])),
        (resp(1), codes(&["TECH-ADV", "OUT-REALISED"])),
        (resp(2), codes(&["TECH-COST"])), // disagreement: the candidate also has ORG-SKILLS
        (resp(3), codes(&["ENV-PARTNER"])),
        (resp(4), codes(&["TECH-COST"])),
        (resp(5), codes(&["TECH-COST"])),
        (resp(6), codes(&["OUT-REALISED"])), // disagreement: the candidate has TECH-ADV
        (resp(7), codes(&["ORG-SKILLS"])),
    ]
    .into_iter()
    .collect();
    let mut last = Value::Null;
    for p in &parts {
        let (st, r) = call(
            app,
            "POST",
            &outputs,
            Some(json!({ "raw": answer(p, |u| ai[u].clone()), "part": p["part"], "of": p["of"] })),
        )
        .await;
        rep.status(st, &[StatusCode::OK], &r, "valid answer imported");
        last = r;
    }
    rep.eq(
        &last["complete"],
        &json!(true),
        "run complete after both parts",
    );
    rep.eq(
        &last["parts_received"],
        &json!([1, 2]),
        "both parts received",
    );

    let (st, review) = call(app, "GET", &format!("{I}/review"), None).await;
    rep.status(st, &[StatusCode::OK], &review, "review");
    rep.eq(&review["n_disagreements"], &json!(2), "two disagreements");
    rep.eq(
        &review["n_sampled"],
        &json!(5),
        "agreement sample: max(5, ceil(10%)) of the six agreements",
    );
    let items = arr(&review["items"]);
    let disagreements: Vec<String> = items
        .iter()
        .filter(|i| i["kind"] == "disagreement")
        .map(|i| i["unit_id"].as_str().unwrap_or("").to_string())
        .collect();
    let sample: Vec<String> = items
        .iter()
        .filter(|i| i["kind"] == "agreement_sample")
        .map(|i| i["unit_id"].as_str().unwrap_or("").to_string())
        .collect();
    rep.eq(
        disagreements.clone(),
        vec![resp(2), resp(6)],
        "disagreements are U006 and U014, in order, first",
    );
    rep.eq(
        items
            .iter()
            .take(2)
            .filter(|i| i["kind"] == "disagreement")
            .count(),
        2,
        "disagreements come first",
    );
    let agreeing: BTreeSet<String> = [0usize, 1, 3, 4, 5, 7].iter().map(|r| resp(*r)).collect();
    rep.ok(
        sample.iter().all(|u| agreeing.contains(u)),
        "the sample is drawn among agreements only",
    );
    rep.eq(
        sample.iter().collect::<BTreeSet<_>>().len(),
        5,
        "five distinct sampled units",
    );
    rep.eq(
        &review["seeds"][0]["run_label"],
        &json!("R1"),
        "sample seed recorded for R1",
    );
    rep.ok(!review["seeds"][0]["seed"].is_null(), "the seed is present");
    rep.eq(
        arr(&review["snapshots"]).len(),
        1,
        "review names the snapshot used",
    );
    rep.eq(
        &review["snapshots"][0]["sha256"],
        &snap["sha256"],
        "same snapshot digest as the snapshots route",
    );
    let (_, review_again) = call(app, "GET", &format!("{I}/review"), None).await;
    rep.eq(
        &review_again["items"],
        &review["items"],
        "the review is deterministic",
    );
    let (_, ag_before) = call(app, "GET", &format!("{I}/agreement"), None).await;
    rep.eq(
        &ag_before["reports"][0]["n_units"],
        &json!(8),
        "agreement over eight units",
    );
    rep.eq(
        &ag_before["reports"][0]["n_exact"],
        &json!(6),
        "six exact on the snapshot",
    );

    // The candidate revises U006 after the import: the live coding now equals the AI.
    let r = put_coding(
        app,
        &mut rep,
        2,
        "INT-01",
        &resp(2),
        json!({ "codes": ["TECH-COST"] }),
    )
    .await;
    rep.eq(
        &r["revised_after_ai"],
        &json!(true),
        "revision after the AI is flagged",
    );
    let (_, review2) = call(app, "GET", &format!("{I}/review"), None).await;
    rep.eq(
        &review2["n_disagreements"],
        &json!(2),
        "still two disagreements after the revision",
    );
    let item = find_by(&review2["items"], "unit_id", &resp(2));
    rep.eq(
        &item["kind"],
        &json!("disagreement"),
        "U006 stays a disagreement on the frozen reference",
    );
    rep.eq(
        sset(&item["human"]),
        sorted(&["TECH-COST", "ORG-SKILLS"]),
        "human is the frozen value",
    );
    rep.eq(
        sset(&item["human_current"]),
        sorted(&["TECH-COST"]),
        "human_current is the live value",
    );
    rep.eq(
        &item["revised_since_snapshot"],
        &json!(true),
        "revised_since_snapshot",
    );
    let item14 = find_by(&review2["items"], "unit_id", &resp(6));
    rep.eq(
        &item14["revised_since_snapshot"],
        &json!(false),
        "U014 not revised",
    );
    let (_, ag_after) = call(app, "GET", &format!("{I}/agreement"), None).await;
    rep.eq(
        &ag_after["reports"],
        &ag_before["reports"],
        "the initial comparison does not change",
    );
    rep.eq(
        &ag_after["reports"][0]["divergent_unit_ids"],
        &json!([resp(2), resp(6)]),
        "divergent units unchanged",
    );
    let (_, snaps_after) = call(app, "GET", &format!("{I}/snapshots"), None).await;
    rep.eq(
        &snaps_after[0]["sha256"],
        &snap["sha256"],
        "the snapshot is never edited",
    );

    // Arbitration of U014: a rationale is required.
    let (st, _) = call(
        app,
        "PUT",
        &format!("{I}/review/{}", resp(6)),
        Some(json!({ "choice": {"type": "human"}, "final_codes": ["TECH-ADV"], "rationale": " " })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "a disagreement cannot be decided without a rationale",
    );
    let (st, d) = call(
        app,
        "PUT",
        &format!("{I}/review/{}", resp(6)),
        Some(json!({ "choice": {"type": "human"}, "final_codes": ["TECH-ADV"],
                      "rationale": "The unit speaks of a gain in time, not of a realised outcome." })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &d, "arbitration of U014");
    let (_, review3) = call(app, "GET", &format!("{I}/review"), None).await;
    rep.eq(&review3["n_decided"], &json!(1), "one decision");
    rep.eq(
        &find_by(&review3["items"], "unit_id", &resp(6))["decision"]["choice"]["type"],
        &json!("human"),
        "decision on the item",
    );
    let (_, fin) = call(app, "GET", &format!("{I}/final"), None).await;
    rep.eq(&fin["n_open_disagreements"], &json!(1), "U006 still open");
    audited(
        app,
        &mut rep,
        &["ai.output.import", "review.sample", "review.decide"],
    )
    .await;

    // =====================================================================
    // 4. Flags, corrections, verification
    // =====================================================================
    rep.step("4 flags, corrections, verification");
    let (st, _) = call(
        app,
        "PUT",
        &format!("{I}/units/{}/flags", resp(7)),
        Some(json!({ "flags": ["CO-CONSTRUCTED"] })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "CO-CONSTRUCTED without a kind is refused",
    );
    let (st, _) = call(
        app,
        "PUT",
        &format!("{I}/units/{}/flags", resp(7)),
        Some(json!({ "flags": ["CO-CONSTRUCTED"], "co_constructed_kind": "other" })),
    )
    .await;
    rep.eq(st, StatusCode::BAD_REQUEST, "an unknown kind is refused");
    let (st, f) = call(
        app,
        "PUT",
        &format!("{I}/units/{}/flags", resp(7)),
        Some(json!({ "flags": ["CO-CONSTRUCTED"], "co_constructed_kind": "assent" })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &f, "CO-CONSTRUCTED with kind assent");
    rep.eq(&f["co_constructed_kind"], &json!("assent"), "kind stored");
    let (_, flags) = call(app, "GET", &format!("{I}/flags"), None).await;
    rep.eq(arr(&flags).len(), 2, "two flagged units");
    rep.eq(
        sset(&find_by(&flags, "unit_id", &resp(5))["flags"]),
        sorted(&["SECOND-HAND", "VERIFY"]),
        "flags of U012",
    );
    rep.eq(
        sset(&find_by(&flags, "unit_id", &resp(7))["flags"]),
        sorted(&["CO-CONSTRUCTED"]),
        "flags of U016",
    );

    // A correction of the text, with its reason; the segmentation keeps the original.
    let u10 = resp(4);
    let fixed1 = "We have not yet decided whether to extend the tags to the smaller articles, mainly because of the cost.";
    let (st, e) = call(
        app,
        "POST",
        &format!("{I}/units/{u10}/corrections"),
        Some(json!({ "text": fixed1, "role": "respondent", "reason": " " })),
    )
    .await;
    rep.eq(st, StatusCode::BAD_REQUEST, "a correction needs a reason");
    rep.ok(e["kind"] == "validation", "validation error kind");
    let (st, c1) = call(app, "POST", &format!("{I}/units/{u10}/corrections"), Some(json!({ "text": fixed1, "role": "respondent", "reason": "Audio: the respondent adds the cost reason" }))).await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &c1,
        "first correction of U010",
    );
    rep.eq(
        &c1["reason"],
        &json!("Audio: the respondent adds the cost reason"),
        "reason stored",
    );
    let (_, rows) = call(app, "GET", &format!("{P2}/units"), None).await;
    let row10 = arr(&rows)
        .into_iter()
        .find(|r| r["unit"]["unit_id"] == u10.as_str())
        .unwrap_or(Value::Null);
    rep.eq(
        &row10["unit"]["text"],
        &json!(fixed1),
        "the phase 2 row shows the corrected text",
    );
    rep.eq(&row10["unit"]["corrected"], &json!(true), "unit.corrected");
    let (_, detail) = call(app, "GET", I, None).await;
    rep.eq(
        &arr(&detail["segmentation"]["units"])[9]["text"],
        &json!(original_u10_text),
        "the segmentation still holds the original text",
    );

    // Verification targets: quoted, flagged, disagreement, plus a seeded sample.
    let (st, v) = call(app, "GET", &format!("{I}/verification"), None).await;
    rep.status(st, &[StatusCode::OK], &v, "verification state");
    let targets = arr(&v["targets"]);
    let reasons = |u: &str| sset(&find_by(&v["targets"], "unit_id", u)["reasons"]);
    rep.ok(
        reasons(&resp(1)).contains(&"quoted".to_string()),
        "U004 target: quoted (excerpt)",
    );
    rep.ok(
        reasons(&resp(3)).contains(&"quoted".to_string()),
        "U008 target: quoted (claim)",
    );
    rep.ok(
        reasons(&resp(5)).contains(&"flagged".to_string()),
        "U012 target: flagged",
    );
    rep.ok(
        reasons(&resp(2)).contains(&"disagreement".to_string()),
        "U006 target: disagreement",
    );
    rep.ok(
        reasons(&resp(6)).contains(&"disagreement".to_string()),
        "U014 target: disagreement",
    );
    let sample_ids: Vec<String> = targets
        .iter()
        .filter(|t| sset(&t["reasons"]).contains(&"sample".to_string()))
        .map(|t| t["unit_id"].as_str().unwrap_or("").to_string())
        .collect();
    let other = targets.len() - sample_ids.len();
    // Interviewer turns are never targets: the eight respondent units are the pool.
    let remaining = 8 - other;
    let expected = 5usize.max(remaining.div_ceil(10)).min(remaining);
    rep.eq(
        sample_ids.len(),
        expected,
        "sample size is max(5, ceil(10%)) of the remaining units",
    );
    rep.ok(
        targets
            .iter()
            .filter(|t| sset(&t["reasons"]).contains(&"sample".to_string()))
            .all(|t| arr(&t["reasons"]).len() == 1),
        "a sampled unit has no other reason",
    );
    let (_, v_again) = call(app, "GET", &format!("{I}/verification"), None).await;
    rep.eq(
        &v_again["targets"],
        &v["targets"],
        "the sample is deterministic once drawn",
    );
    rep.eq(
        &v["coverage"]["n_units"],
        &json!(16),
        "coverage over 16 units",
    );
    rep.eq(
        &v["coverage"]["n_verified"],
        &json!(0),
        "nothing verified yet",
    );

    // Verify U010 and U004, then correct U010 again.
    for u in [&u10, &resp(1)] {
        let (st, s) = call(
            app,
            "PUT",
            &format!("{I}/units/{u}/verification"),
            Some(json!({ "state": "verified", "note": "listened" })),
        )
        .await;
        rep.status(st, &[StatusCode::OK], &s, &format!("verify {u}"));
        rep.eq(&s["state"], &json!("verified"), "state verified");
        rep.eq(&s["stale"], &json!(false), "not stale");
    }
    let (_, v) = call(app, "GET", &format!("{I}/verification"), None).await;
    rep.eq(
        &v["coverage"]["n_verified"],
        &json!(2),
        "two units verified",
    );
    let fixed2 = "We have not yet decided whether to extend the tags to the smaller articles, mainly because of the tag cost.";
    let (st, c2) = call(
        app,
        "POST",
        &format!("{I}/units/{u10}/corrections"),
        Some(
            json!({ "text": fixed2, "role": "respondent", "reason": "Second listening, wording" }),
        ),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &c2,
        "second correction of U010",
    );
    rep.ok(
        c2["correction_id"].as_i64().unwrap_or(0) > c1["correction_id"].as_i64().unwrap_or(0),
        "append-only: a new correction id",
    );
    let (_, list) = call(app, "GET", &format!("{I}/units/{u10}/corrections"), None).await;
    rep.eq(arr(&list).len(), 2, "both corrections kept");
    let (_, v) = call(app, "GET", &format!("{I}/verification"), None).await;
    let s10 = find_by(&v["status"], "unit_id", &u10);
    rep.eq(
        &s10["state"],
        &json!("unverified"),
        "U010 back to unverified after a correction",
    );
    rep.eq(&s10["stale"], &json!(true), "U010 stale");
    let s4 = find_by(&v["status"], "unit_id", &resp(1));
    rep.eq(&s4["state"], &json!("verified"), "U004 still verified");
    rep.eq(&v["coverage"]["n_stale"], &json!(1), "one stale unit");
    rep.eq(
        &v["coverage"]["n_verified"],
        &json!(1),
        "one verified unit left",
    );
    audited(
        app,
        &mut rep,
        &[
            "unit.flags",
            "unit.correct",
            "unit.verify",
            "verification.sample",
        ],
    )
    .await;

    // =====================================================================
    // 5. Claims
    // =====================================================================
    rep.step("5 claims");
    let u4 = resp(1);
    let (st, e) = call(
        app,
        "POST",
        &format!("{I}/claims"),
        Some(json!({ "unit_id": u4, "excerpt": "not in this unit", "codes": ["TECH-ADV"] })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "an excerpt that is not a substring is rejected",
    );
    rep.ok(
        e["error"].as_str().unwrap_or("").contains("excerpt"),
        "the error names excerpt",
    );
    let (st, e) = call(
        app,
        "POST",
        &format!("{I}/claims"),
        Some(json!({ "unit_id": u4, "excerpt": "sixty to ninety-eight percent", "codes": ["TECH-ADV"], "role": "boss" })),
    )
    .await;
    rep.eq(st, StatusCode::BAD_REQUEST, "an unknown role is rejected");
    rep.ok(
        e["error"].as_str().unwrap_or("").contains("role"),
        "the error names role",
    );
    let (st, _) = call(
        app,
        "POST",
        &format!("{I}/claims"),
        Some(json!({ "unit_id": u4, "excerpt": "sixty to ninety-eight percent", "codes": ["ENV-PARTNER"] })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "claim codes must be among the unit's codes",
    );
    let (st, claim) = call(
        app,
        "POST",
        &format!("{I}/claims"),
        Some(json!({
            "unit_id": u4,
            "excerpt": "sixty to ninety-eight percent",
            "codes": ["TECH-ADV"],
            "ev_level": "EV3",
            "provenance": "SPONTANEOUS",
            "role": "trigger",
            "memo": "Figure given without prompting",
        })),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &claim,
        "claim with excerpt, EV level, provenance and role",
    );
    let cid = claim["claim_id"].as_i64().unwrap_or(0);
    rep.eq(
        &claim["excerpt"],
        &json!("sixty to ninety-eight percent"),
        "excerpt stored",
    );
    rep.eq(&claim["ev_level"], &json!("EV3"), "ev_level stored");
    rep.eq(
        &claim["provenance"],
        &json!("SPONTANEOUS"),
        "provenance stored",
    );
    rep.eq(&claim["role"], &json!("trigger"), "role stored");
    let mut upd = claim.clone();
    upd["ev_level"] = json!("EV4");
    let (st, got) = call(app, "PUT", &format!("/api/claims/{cid}"), Some(upd)).await;
    rep.status(st, &[StatusCode::OK], &got, "claim updated");
    rep.eq(&got["ev_level"], &json!("EV4"), "ev_level updated");
    let (_, all) = call(app, "GET", &format!("{I}/claims"), None).await;
    rep.eq(
        arr(&all).len(),
        2,
        "two claims in the interview (U008 and U004)",
    );
    let (_, mine) = call(app, "GET", &format!("{I}/units/{u4}/claims"), None).await;
    rep.eq(arr(&mine).len(), 1, "one claim on U004");
    let (_, rev) = call(app, "GET", &format!("{I}/claims/review"), None).await;
    rep.eq(
        &rev,
        &json!([]),
        "nothing to review while claims agree with the units",
    );
    // The code of the claim is removed from the unit.
    put_coding(app, &mut rep, 2, "INT-01", &u4, json!({ "codes": ["OUT-REALISED"], "excerpt": "sixty to ninety-eight percent", "memo": "SENTINEL-MEMO-ALPHA" })).await;
    let (_, rev) = call(app, "GET", &format!("{I}/claims/review"), None).await;
    rep.eq(arr(&rev).len(), 1, "one claim to review");
    rep.eq(
        &rev[0]["claim_id"],
        &json!(cid),
        "the claim whose code was removed",
    );
    rep.ok(
        !rev[0]["reason"].as_str().unwrap_or("").is_empty(),
        "with a reason",
    );
    audited(app, &mut rep, &["claim.create", "claim.update"]).await;

    // =====================================================================
    // 6. Cases across two interviews, cross-case matrix
    // =====================================================================
    rep.step("6 cases and cross-case");
    std::fs::create_dir_all(ctx.root.join("ECO-02_Other")).unwrap();
    std::fs::write(ctx.root.join("ECO-02_Other/01_transcript_raw.vtt"), VTT).unwrap();
    let (st, scan) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &scan, "second scan");
    rep.eq(
        arr(&scan["interviews"]).len(),
        2,
        "two interviews after the second scan",
    );
    let (st, seg2) = call(app, "POST", "/api/interviews/ECO-02/segment", None).await;
    rep.status(st, &[StatusCode::OK], &seg2, "segment ECO-02");
    let e2 = seg2["units"][1]["unit_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let e4 = seg2["units"][3]["unit_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    rep.eq(
        e2.clone(),
        "ECO-02-U002".to_string(),
        "ECO-02 respondent unit",
    );
    for (u, c) in [
        ("ECO-02-U001", "NA"),
        ("ECO-02-U002", "TECH"),
        ("ECO-02-U003", "NA"),
        ("ECO-02-U004", "ENV"),
    ] {
        put_coding(app, &mut rep, 1, "ECO-02", u, json!({ "codes": [c] })).await;
    }
    put_coding(
        app,
        &mut rep,
        2,
        "ECO-02",
        &e2,
        json!({ "codes": ["TECH-ADV"] }),
    )
    .await;
    put_coding(
        app,
        &mut rep,
        2,
        "ECO-02",
        &e4,
        json!({ "codes": ["ENV-PARTNER"] }),
    )
    .await;

    let u8_ = resp(3);
    let (st, case1) = call(
        app,
        "POST",
        &format!("{I}/cases"),
        Some(json!({
            "label": "Source tagging",
            "units": [u8_, e2],
            "decision": "adopted",
            "granularity": "item",
            "carrier_chosen": "RAIN RFID",
            "carriers_rejected": ["QR"],
            "account_type": "first-hand",
            "swiss_link": "Swiss retailer, supplier in the EU",
            "trigger": { "text": "A customer requirement", "evidence": [u8_] },
        })),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &case1,
        "case 1 with units of two interviews",
    );
    rep.eq(&case1["case_id"], &json!("INT-01-C1"), "case id");
    rep.eq(
        &case1["interviews"],
        &json!(["ECO-02", "INT-01"]),
        "the case spans both interviews",
    );
    let (st, case2) = call(
        app,
        "POST",
        &format!("{I}/cases"),
        Some(json!({ "label": "Deferred extension", "trigger": { "text": "Open question on small articles" } })),
    )
    .await;
    rep.status(
        st,
        &[StatusCode::OK, StatusCode::CREATED],
        &case2,
        "case 2 without any unit",
    );
    rep.eq(&case2["case_id"], &json!("INT-01-C2"), "second case id");
    let (_, all) = call(app, "GET", "/api/cases", None).await;
    rep.eq(arr(&all).len(), 2, "two cases");
    let (_, eco_cases) = call(app, "GET", "/api/interviews/ECO-02/cases", None).await;
    rep.eq(
        arr(&eco_cases).len(),
        1,
        "ECO-02 lists the case that holds its unit",
    );

    let (_, cc) = call(app, "GET", "/api/cross-case", None).await;
    let cells = arr(&cc["cells"]);
    let cell = |rq: &str, case_id: &str| -> Value {
        cells
            .iter()
            .find(|c| c["rq"] == rq && c["case_id"] == case_id)
            .cloned()
            .unwrap_or(Value::Null)
    };
    let sq3 = cell("SQ3", "INT-01-C1");
    rep.eq(&sq3["documented"], &json!(true), "case 1 is documented");
    rep.eq(
        &sq3["codes"],
        &json!([{ "code": "ENV-PARTNER", "units": [u8_] }]),
        "SQ3: only the attributed U008, not U002 or ECO-02-U004",
    );
    let sq1 = cell("SQ1", "INT-01-C1");
    rep.eq(
        &sq1["codes"],
        &json!([{ "code": "TECH-ADV", "units": [e2] }]),
        "SQ1: only the attributed ECO-02 unit, not U004 or U014",
    );
    for rq in ["SQ1", "SQ2", "SQ3"] {
        let c = cell(rq, "INT-01-C2");
        rep.eq(
            &c["documented"],
            &json!(false),
            &format!("{rq}: case 2 undocumented"),
        );
        rep.eq(
            &c["codes"],
            &json!([]),
            &format!("{rq}: case 2 holds no code"),
        );
    }
    let (_, v) = call(app, "GET", &format!("{I}/verification"), None).await;
    rep.ok(
        sset(&find_by(&v["targets"], "unit_id", &u8_)["reasons"]).contains(&"case".to_string()),
        "U008 is now also a case target",
    );
    let (_, v2) = call(app, "GET", "/api/interviews/ECO-02/verification", None).await;
    rep.ok(
        sset(&find_by(&v2["targets"], "unit_id", &e2)["reasons"]).contains(&"case".to_string()),
        "ECO-02-U002 is a case target of its own interview",
    );
    audited(app, &mut rep, &["case.create"]).await;

    // =====================================================================
    // 7. Research question memo and saturation
    // =====================================================================
    rep.step("7 memo and saturation");
    let (st, e) = call(
        app,
        "PUT",
        "/api/research-questions/SQ1/memo",
        Some(json!({ "documented": "x", "units_for": ["INT-01-U999"] })),
    )
    .await;
    rep.eq(
        st,
        StatusCode::BAD_REQUEST,
        "a memo naming an unknown unit is refused",
    );
    rep.eq(&e["kind"], &json!("validation"), "validation kind");
    let (st, put) = call(
        app,
        "PUT",
        "/api/research-questions/SQ1/memo",
        Some(json!({
            "documented": "Accuracy gains are reported by two respondents.",
            "interpretation": "Advantage is perceived through inventory accuracy and counting time.",
            "rivals": "Partner pressure explains adoption better than advantage.",
            "limits": "Two interviews, one sector.",
            "units_for": [resp(1), resp(6), e2],
            "units_against": [resp(2)],
        })),
    )
    .await;
    rep.status(st, &[StatusCode::OK], &put, "memo with four parts");
    rep.eq(
        &put["units_for"],
        &json!([resp(1), resp(6), e2]),
        "units for",
    );
    rep.eq(&put["units_against"], &json!([resp(2)]), "units against");
    let (st, got) = call(app, "GET", "/api/research-questions/SQ1/memo", None).await;
    rep.status(st, &[StatusCode::OK], &got, "memo read back");
    for part in ["documented", "interpretation", "rivals", "limits"] {
        rep.eq(&got[part], &put[part], &format!("memo part {part}"));
    }
    let (st, _) = call(app, "GET", "/api/research-questions/SQ2/memo", None).await;
    rep.eq(st, StatusCode::NOT_FOUND, "SQ2 has no memo");

    let (st, sat) = call(app, "GET", "/api/saturation", None).await;
    rep.status(st, &[StatusCode::OK], &sat, "saturation");
    let rows = arr(&sat);
    rep.eq(rows.len(), 2, "one row per coded interview");
    rep.eq(&rows[0]["interview_id"], &json!("INT-01"), "INT-01 first");
    rep.eq(&rows[0]["order"], &json!(1), "order 1");
    rep.eq(&rows[1]["interview_id"], &json!("ECO-02"), "ECO-02 second");
    rep.eq(&rows[1]["order"], &json!(2), "order 2");
    rep.ok(
        sset(&rows[0]["codes_first_used"]).contains(&"TECH-ADV".to_string()),
        "TECH-ADV first used in INT-01",
    );
    rep.eq(
        sset(&rows[0]["field_codes_created"]),
        sorted(&["SUPPLIER-TAGGING"]),
        "field code created in INT-01",
    );
    rep.eq(&rows[0]["labels_opened"], &json!(1), "one label opened");
    rep.eq(&rows[0]["labels_resolved"], &json!(1), "one label resolved");
    rep.eq(
        &rows[1]["codes_first_used"],
        &json!([]),
        "ECO-02 uses no code for the first time",
    );
    rep.ok(
        !rows[0]["first_coded_at"].as_str().unwrap_or("").is_empty(),
        "first_coded_at present",
    );
    audited(app, &mut rep, &["rq_memo.update"]).await;

    // =====================================================================
    // 8. Exports
    // =====================================================================
    rep.step("8 interview export");
    let (_, before_export) = call(app, "GET", "/api/audit?interview_id=INT-01", None).await;
    let (st, exp) = call(app, "POST", &format!("{I}/export"), None).await;
    rep.status(st, &[StatusCode::OK], &exp, "interview export");
    let dir = PathBuf::from(exp["dir"].as_str().unwrap_or("/nonexistent"));
    rep.ok(
        dir.starts_with(ctx.db.parent().unwrap()),
        "package under the database folder",
    );
    let mut expected: Vec<String> = [
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
        "cases.csv",
        "case_units.csv",
        "human_snapshot_batch1.csv",
        "human_snapshot_batch1.json",
        "units_verification.csv",
        "unit_corrections.csv",
        "claims.csv",
        "ev_distribution.csv",
        "provenance_distribution.csv",
        "roles_distribution.csv",
        "state.json",
        "db_snapshot.sqlite",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for part in 1..=2 {
        let prefix = format!("phase2_batch1_prompt_part{part}_of2_");
        let found = std::fs::read_dir(&dir)
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .any(|n| n.starts_with(&prefix) && n.ends_with(".txt"))
            })
            .unwrap_or(false);
        rep.ok(
            found,
            &format!("prompt part {part} file {prefix}<hash>.txt"),
        );
        expected.push(format!("phase2_ai_run{run_id}_R1_part{part}_raw.txt"));
    }
    expected.push(format!("phase2_run{run_id}_R1.json"));
    expected.push(format!("phase2_coding_ai_run{run_id}_R1.csv"));
    for f in &expected {
        rep.ok(dir.join(f).is_file(), &format!("export file {f}"));
    }
    let manifest = json_of(&dir, "manifest.json");
    let listed: BTreeSet<String> = arr(&manifest["files"])
        .iter()
        .filter_map(|f| f["name"].as_str().map(str::to_string))
        .collect();
    for f in &arr(&manifest["files"]) {
        let name = f["name"].as_str().unwrap_or("");
        let bytes = std::fs::read(dir.join(name)).unwrap_or_default();
        rep.eq(
            cs_model::sha256_hex(&bytes),
            f["sha256"].as_str().unwrap_or("").to_string(),
            &format!("manifest digest of {name}"),
        );
    }
    for f in [
        "state.json",
        "db_snapshot.sqlite",
        "claims.csv",
        "units_verification.csv",
        "unit_corrections.csv",
        "human_snapshot_batch1.json",
    ] {
        rep.ok(listed.contains(f), &format!("manifest lists {f}"));
    }
    rep.eq(
        &exp["manifest_sha256"],
        &json!(cs_model::sha256_hex(
            &std::fs::read(dir.join("manifest.json")).unwrap_or_default()
        )),
        "manifest_sha256 in the answer",
    );

    // state.json
    let state = json_of(&dir, "state.json");
    rep.eq(
        state["sha256"].as_str().map_or(0, str::len),
        64,
        "state.json digest",
    );
    rep.eq(
        &state["generator"]["version"],
        &json!(env!("CARGO_PKG_VERSION")),
        "generator version",
    );
    rep.ok(
        state["generator"]["git"]
            .as_str()
            .is_some_and(|g| !g.is_empty()),
        "generator git field",
    );
    rep.ok(state["parts"].is_object(), "state.json carries its parts");

    // The database snapshot opens and holds the claims.
    let snap_path = dir.join("db_snapshot.sqlite");
    match rusqlite::Connection::open_with_flags(
        &snap_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(conn) => {
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM claims", [], |r| r.get(0))
                .unwrap_or(-1);
            rep.eq(n, 2, "db_snapshot.sqlite: two claims rows");
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM claims WHERE excerpt = 'sixty to ninety-eight percent'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(-1);
            rep.eq(n, 1, "db_snapshot.sqlite: the exact excerpt");
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM human_snapshots", [], |r| r.get(0))
                .unwrap_or(-1);
            rep.eq(n, 1, "db_snapshot.sqlite: the frozen reference");
            let ok: String = conn
                .query_row("PRAGMA integrity_check", [], |r| r.get(0))
                .unwrap_or_default();
            rep.eq(ok, "ok".to_string(), "db_snapshot.sqlite integrity");
        }
        Err(e) => {
            rep.ok(
                false,
                &format!("db_snapshot.sqlite opens with rusqlite: {e}"),
            );
        }
    }

    // Contents of the main files.
    rep.eq(
        text_of(&dir, "claims.csv").lines().count(),
        3,
        "claims.csv: header and two claims",
    );
    rep.ok(
        text_of(&dir, "ev_distribution.csv").contains("EV4")
            && !text_of(&dir, "ev_distribution.csv").contains("EV3"),
        "ev_distribution.csv reflects the update",
    );
    rep.eq(
        text_of(&dir, "unit_corrections.csv").lines().count(),
        3,
        "unit_corrections.csv: header and two corrections",
    );
    rep.eq(
        text_of(&dir, "cases.csv").lines().count(),
        3,
        "cases.csv: header and two cases",
    );
    rep.ok(
        text_of(&dir, "case_units.csv").contains("ECO-02-U002"),
        "case_units.csv names the unit of the other interview",
    );
    rep.ok(
        text_of(&dir, "units_verification.csv").contains(&format!("{u10},unverified,1,")),
        "units_verification.csv: U010 unverified and stale",
    );
    let review_md = text_of(&dir, "phase2_review.md");
    rep.ok(
        review_md.contains("## Snapshots") && review_md.contains("## Revisions since the snapshot"),
        "phase2_review.md sections",
    );
    rep.ok(
        review_md.contains(&resp(2)),
        "phase2_review.md names the revised unit",
    );
    let snapshot_json = json_of(&dir, "human_snapshot_batch1.json");
    rep.eq(
        &snapshot_json["sha256"],
        &snap["sha256"],
        "exported snapshot has the digest of the stored one",
    );
    let audit_jsonl = text_of(&dir, "audit_log.jsonl");
    rep.eq(
        audit_jsonl.lines().count(),
        arr(&before_export).len(),
        "audit_log.jsonl holds every event of the interview",
    );
    rep.eq(
        &json_of(&dir, "audit_chain.json")["ok"],
        &json!(true),
        "audit_chain.json verifies",
    );
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        rep.ok(
            !text.contains('\u{2014}') && !text.contains('\u{2013}'),
            &format!("no dash in {}", path.file_name().unwrap().to_string_lossy()),
        );
    }

    rep.step("8 cross-case export");
    let (st, cc) = call(app, "POST", "/api/export/cross-case", None).await;
    rep.status(st, &[StatusCode::OK], &cc, "cross-case export");
    let cdir = PathBuf::from(cc["folder"].as_str().unwrap_or("/nonexistent"));
    for f in [
        "answers.md",
        "cross_case.csv",
        "saturation.csv",
        "saturation.md",
        "cases.csv",
        "state.json",
        "manifest.json",
        "README.md",
    ] {
        rep.ok(cdir.join(f).is_file(), &format!("cross-case file {f}"));
    }
    let cman = json_of(&cdir, "manifest.json");
    for f in &arr(&cman["files"]) {
        let name = f["name"].as_str().unwrap_or("");
        let bytes = std::fs::read(cdir.join(name)).unwrap_or_default();
        rep.eq(
            cs_model::sha256_hex(&bytes),
            f["sha256"].as_str().unwrap_or("").to_string(),
            &format!("cross-case manifest digest of {name}"),
        );
    }
    let clisted: BTreeSet<String> = arr(&cman["files"])
        .iter()
        .filter_map(|f| f["name"].as_str().map(str::to_string))
        .collect();
    for f in [
        "answers.md",
        "cross_case.csv",
        "saturation.csv",
        "saturation.md",
        "cases.csv",
        "state.json",
    ] {
        rep.ok(
            clisted.contains(f),
            &format!("cross-case manifest lists {f}"),
        );
    }
    rep.eq(
        &cman["state_sha256"],
        &json_of(&cdir, "state.json")["sha256"],
        "manifest state digest equals state.json",
    );
    let answers = text_of(&cdir, "answers.md");
    for part in [
        "Accuracy gains are reported by two respondents.",
        "Advantage is perceived through inventory accuracy and counting time.",
        "Partner pressure explains adoption better than advantage.",
        "Two interviews, one sector.",
    ] {
        rep.ok(answers.contains(part), "answers.md carries the memo part");
    }
    rep.ok(
        answers.contains(&format!("`{}`", resp(1))) && answers.contains(&format!("`{}`", resp(2))),
        "answers.md lists units for and against",
    );
    rep.ok(
        answers.contains("no memo yet"),
        "answers.md marks questions without a memo",
    );
    let csv = text_of(&cdir, "cross_case.csv");
    rep.ok(
        csv.lines()
            .any(|l| l.starts_with("SQ3,INT-01-C1,1,ENV-PARTNER,")),
        "cross_case.csv: documented cell of case 1",
    );
    rep.ok(
        csv.lines().filter(|l| l.contains("INT-01-C2")).all(|l| {
            !["TECH-", "ENV-", "ORG-", "OUT-"]
                .iter()
                .any(|c| l.contains(c))
        }),
        "cross_case.csv: the case without units holds no code",
    );
    let sat_csv = text_of(&cdir, "saturation.csv");
    rep.eq(
        sat_csv.lines().count(),
        3,
        "saturation.csv: header and two interviews",
    );
    rep.ok(
        sat_csv
            .lines()
            .nth(1)
            .is_some_and(|l| l.starts_with("INT-01,1,")),
        "saturation.csv: INT-01 first",
    );
    rep.ok(
        text_of(&cdir, "saturation.md")
            .to_lowercase()
            .contains("provisional"),
        "saturation.md says provisional",
    );
    rep.eq(
        text_of(&cdir, "cases.csv").lines().count(),
        3,
        "cross-case cases.csv",
    );
    for entry in std::fs::read_dir(&cdir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        rep.ok(
            !text.contains('\u{2014}') && !text.contains('\u{2013}'),
            &format!(
                "no dash in cross-case {}",
                path.file_name().unwrap().to_string_lossy()
            ),
        );
    }
    audited(app, &mut rep, &["export.package", "export.cross_case"]).await;

    // =====================================================================
    // 9. Audit chain
    // =====================================================================
    rep.step("9 audit chain");
    let (st, events) = call(app, "GET", "/api/audit", None).await;
    rep.status(st, &[StatusCode::OK], &events, "audit log");
    let events = arr(&events);
    let have: BTreeSet<String> = events
        .iter()
        .filter_map(|e| e["action"].as_str().map(str::to_string))
        .collect();
    for a in [
        "library.interview.add",
        "codebook.import",
        "segmentation.save",
        "coding.set",
        "to_clarify.create",
        "to_clarify.resolve",
        "codebook.code.create",
        "prompt.generate",
        "human_snapshot.take",
        "ai.run.create",
        "ai.output.import",
        "review.sample",
        "review.decide",
        "unit.flags",
        "unit.correct",
        "unit.verify",
        "verification.sample",
        "claim.create",
        "claim.update",
        "case.create",
        "rq_memo.update",
        "export.package",
        "export.cross_case",
    ] {
        rep.ok(have.contains(a), &format!("every action is audited: {a}"));
    }
    rep.ok(
        events
            .iter()
            .filter(|e| e["action"] == "coding.set")
            .all(|e| e["actor"] == "candidate"),
        "coding events carry the actor",
    );
    let seqs: Vec<i64> = events.iter().filter_map(|e| e["seq"].as_i64()).collect();
    rep.ok(
        seqs.windows(2).all(|w| w[1] == w[0] + 1),
        "sequence numbers are contiguous",
    );
    rep.ok(
        events.windows(2).all(|w| w[1]["prev_hash"] == w[0]["hash"]),
        "each event stores the digest of the previous one",
    );
    let (st, chain) = call(app, "GET", "/api/audit/verify", None).await;
    rep.status(st, &[StatusCode::OK], &chain, "chain verification");
    rep.eq(&chain["ok"], &json!(true), "the hash chain verifies");
    rep.eq(
        &chain["n_events"],
        &json!(events.len()),
        "the chain covers every event",
    );
    rep.ok(chain["first_bad_seq"].is_null(), "no bad event");
    let (_, only_eco) = call(app, "GET", "/api/audit?interview_id=ECO-02", None).await;
    rep.ok(
        !arr(&only_eco).is_empty() && arr(&only_eco).iter().all(|e| e["interview_id"] == "ECO-02"),
        "audit filter by interview",
    );

    // The record cannot be edited, even by someone with the database file.
    let raw = rusqlite::Connection::open(&ctx.db).unwrap();
    rep.ok(
        raw.execute("UPDATE audit_log SET actor = 'x' WHERE seq = 1", [])
            .is_err(),
        "audit_log refuses UPDATE",
    );
    rep.ok(
        raw.execute("DELETE FROM audit_log WHERE seq = 1", [])
            .is_err(),
        "audit_log refuses DELETE",
    );
    rep.ok(
        raw.execute("DELETE FROM human_snapshots", []).is_err(),
        "human_snapshots refuses DELETE",
    );
    rep.ok(
        raw.execute("DELETE FROM unit_corrections", []).is_err(),
        "unit_corrections refuses DELETE",
    );
    let (_, chain) = call(app, "GET", "/api/audit/verify", None).await;
    rep.eq(
        &chain["ok"],
        &json!(true),
        "the chain still verifies after the attempts",
    );

    rep.finish();
}
