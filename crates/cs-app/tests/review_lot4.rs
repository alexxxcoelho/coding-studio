//! V1 lot 4 (Review) through the HTTP API: configured prompt, pseudonymisation,
//! candidate concepts, frozen human reference, passage to unit sync,
//! harmonisation worklist and review state.
//! Acceptance tests A9, A10, A13 to A19 and A28.

mod common;

use axum::http::StatusCode;
use common::{call, code_id, ok, setup, unit_ids, Ctx};
use serde_json::{json, Value};

const I: &str = "/api/interviews/INT-01";
const P2: &str = "/api/interviews/INT-01/phases/2";

/// Codes the three respondent units of INT-01 in phase 2.
async fn code_respondent_units(ctx: &Ctx) -> Vec<String> {
    let units = unit_ids(&ctx.app, "INT-01").await;
    for (i, c) in [(1, "TECH-ADV"), (3, "TECH-ADV"), (5, "TECH-COST")] {
        ok(
            &ctx.app,
            "PUT",
            &format!("{P2}/coding/{}", units[i]),
            Some(json!({ "codes": [c] })),
        )
        .await;
    }
    units
}

async fn phase2(ctx: &Ctx) -> Value {
    ok(&ctx.app, "GET", I, None).await["phase2"].clone()
}

static LABEL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

async fn prompts(ctx: &Ctx) -> Vec<Value> {
    ok(
        &ctx.app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({ "batch_size": 3 })),
    )
    .await
    .as_array()
    .unwrap()
    .clone()
}

fn answer(part: &Value, candidates: Value) -> String {
    let codings: Vec<Value> = part["unit_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| json!({ "unit_id": u, "codes": ["TECH-COST"] }))
        .collect();
    let obj = json!({
        "schema": cs_ai::OUTPUT_SCHEMA_ID,
        "interview_id": part["interview_id"],
        "phase": part["phase"],
        "segmentation_hash": part["segmentation_hash"],
        "part": part["part"],
        "of": part["of"],
        "codings": codings,
        "candidate_concepts": candidates,
    });
    format!("```json\n{obj}\n```\n")
}

async fn run_and_import(ctx: &Ctx, part: &Value, candidates: Value) -> (StatusCode, Value) {
    let n = LABEL.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let run = ok(
        &ctx.app,
        "POST",
        &format!("{P2}/runs"),
        Some(
            json!({ "run_label": format!("R{n}"), "model_name": "model-a", "model_version": "1" }),
        ),
    )
    .await;
    let run_id = run["run_id"].as_i64().unwrap();
    call(
        &ctx.app,
        "POST",
        &format!("{P2}/runs/{run_id}/outputs"),
        Some(json!({ "raw": answer(part, candidates), "part": 1, "of": 1 })),
    )
    .await
}

#[tokio::test]
async fn a9_a18_candidate_concepts_are_checked_stored_and_decided() {
    let ctx = setup().await;
    let units = code_respondent_units(&ctx).await;
    let parts = prompts(&ctx).await;
    let part = &parts[0];
    let text = part["text"].as_str().unwrap();
    assert!(text.contains("candidate_concepts"));

    // A9: a quote that is not in the unit is refused.
    let (st, e) = run_and_import(
        &ctx,
        part,
        json!([{ "label": "Data quality", "unit_ids": [units[3]],
                 "evidence": [{ "unit_id": units[3], "quote": "nothing of the sort was said" }] }]),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    let none = ok(&ctx.app, "GET", &format!("{I}/concept-candidates"), None).await;
    assert!(none.as_array().unwrap().is_empty());

    // A word for word quote is accepted.
    let (st, r) = run_and_import(
        &ctx,
        part,
        json!([{ "label": "Data quality", "description": "Accuracy of the stock records",
                 "unit_ids": [units[3]],
                 "evidence": [{ "unit_id": units[3], "quote": "inventory accuracy went from sixty" }] }]),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let list = ok(&ctx.app, "GET", &format!("{I}/concept-candidates"), None).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 1);
    let c = &list[0];
    assert_eq!(c["label"], "Data quality");
    assert_eq!(c["decision"], Value::Null);
    assert_eq!(c["unit_ids"], json!([units[3]]));
    let cid = c["candidate_id"].as_str().unwrap().to_string();

    // A18: a decision needs a reason and changes no coding.
    let before = phase2(&ctx).await;
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("{I}/concept-candidates/{cid}"),
        Some(json!({ "action": "reject", "rationale": "" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("{I}/concept-candidates/{cid}"),
        Some(json!({ "action": "map", "rationale": "same idea" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "map needs a code: {e}");
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let d = ok(
        &ctx.app,
        "PUT",
        &format!("{I}/concept-candidates/{cid}"),
        Some(json!({ "action": "map", "code_id": adv, "rationale": "same idea as advantage" })),
    )
    .await;
    assert_eq!(d["decision"]["action"], "map");
    let d = ok(
        &ctx.app,
        "PUT",
        &format!("{I}/concept-candidates/{cid}"),
        Some(json!({ "action": "create", "new_code_name": "Data quality",
                     "rationale": "no code covers it" })),
    )
    .await;
    assert_eq!(d["decision"]["action"], "create");
    assert!(d["decision"]["code_id"].as_str().is_some());
    let after = phase2(&ctx).await;
    assert_eq!(before, after, "a decision changes no coding");
    let codes = ok(&ctx.app, "GET", "/api/codes", None).await;
    assert!(codes["codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "Data quality" || c["label"] == "Data quality"));
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("{I}/concept-candidates/CC-9999"),
        Some(json!({ "action": "reject", "rationale": "x" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a10_a17_no_listed_name_reaches_the_prompt_and_the_run_says_so() {
    let ctx = setup().await;
    code_respondent_units(&ctx).await;
    let put = ok(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({ "enabled": true, "entries": [
            { "kind": "organisation", "original": "retailer", "pseudonym": "Organisation A" },
            { "kind": "person", "original": "Interviewer", "pseudonym": "Personne B" } ] })),
    )
    .await;
    assert_eq!(put["enabled"], true);
    assert_eq!(put["digest"].as_str().unwrap().len(), 64);
    assert!(put["note"].as_str().unwrap().len() > 10);
    let parts = prompts(&ctx).await;
    let text = parts[0]["text"].as_str().unwrap();
    assert!(!text.to_lowercase().contains("retailer"));
    assert!(text.contains("Organisation A"));
    assert!(text.contains("pseudonymisation_digest:"));

    let (st, r) = run_and_import(&ctx, &parts[0], json!([])).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let runs = ok(&ctx.app, "GET", &format!("{P2}/runs"), None).await;
    let rec = runs.to_string();
    assert!(rec.contains(put["digest"].as_str().unwrap()), "{rec}");
    assert!(!rec.contains("retailer"), "no clear name in the run record");

    // Per interview view: counts, original shown to the owner only here.
    let v = ok(&ctx.app, "GET", &format!("{I}/pseudonymisation"), None).await;
    let e = v["entries"].as_array().unwrap();
    assert_eq!(e.len(), 1, "only the names present in this interview");
    let r = e.iter().find(|x| x["original"] == "retailer").unwrap();
    assert_eq!(r["pseudonym"], "Organisation A");
    assert_eq!(r["count"], 1);

    // The list is validated.
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({ "entries": [
            { "kind": "person", "original": "X", "pseudonym": "A" },
            { "kind": "person", "original": "x", "pseudonym": "B" } ] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a13_a14_a15_the_human_coding_never_reaches_the_prompt_and_is_frozen() {
    let ctx = setup().await;
    let units = code_respondent_units(&ctx).await;
    ok(
        &ctx.app,
        "PUT",
        "/api/config",
        Some({
            let mut cfg = ok(&ctx.app, "GET", "/api/config", None).await;
            cfg["description"] = json!("Why retailers adopt tagging.");
            cfg["coding_rules"] = json!("One code per distinct idea.");
            cfg
        }),
    )
    .await;
    let first = prompts(&ctx).await;
    let text = first[0]["text"].as_str().unwrap().to_string();
    assert!(text.contains("Why retailers adopt tagging."));
    assert!(text.contains("One code per distinct idea."));
    assert!(text.contains("configuration_digest:"));
    let snaps = ok(&ctx.app, "GET", &format!("{I}/snapshots"), None).await;
    assert_eq!(snaps.as_array().unwrap().len(), 1);

    // The candidate changes the human coding afterwards: a new batch text is
    // byte-identical, and the frozen reference keeps the first coding.
    ok(
        &ctx.app,
        "PUT",
        &format!("{P2}/coding/{}", units[3]),
        Some(json!({ "codes": ["ORG-SKILLS"] })),
    )
    .await;
    let again = prompts(&ctx).await;
    assert_eq!(again[0]["text"].as_str().unwrap(), text, "A13");
    assert_eq!(
        again[0]["prompt_sha256"], first[0]["prompt_sha256"],
        "the digest is recorded and stable"
    );
    let snaps = ok(&ctx.app, "GET", &format!("{I}/snapshots"), None).await;
    assert!(
        snaps.as_array().unwrap()[0]
            .to_string()
            .contains("TECH-ADV"),
        "A14: the first snapshot keeps the coding as it was"
    );
    for tok in ["TECH-ADV\"]", "ORG-SKILLS\"]"] {
        assert!(!text.contains(tok), "no human code in the prompt: {tok}");
    }
}

#[tokio::test]
async fn a28_interviewer_units_are_context_only() {
    let ctx = setup().await;
    let units = code_respondent_units(&ctx).await;
    let parts = prompts(&ctx).await;
    let ids = parts[0]["unit_ids"].as_array().unwrap();
    for i in [0, 2, 4] {
        assert!(!ids.iter().any(|u| *u == units[i]), "interviewer unit sent");
    }
    // Interviewer unit coding is refused.
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("{P2}/coding/{}", units[0]),
        Some(json!({ "codes": ["TECH-ADV"] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a16_passages_write_the_unit_coding_and_open_the_worklist() {
    let ctx = setup().await;
    let units = unit_ids(&ctx.app, "INT-01").await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let cost = code_id(&ctx.app, "TECH-COST").await;
    // A passage that covers the whole unit writes the unit coding.
    let u = ok(&ctx.app, "GET", I, None).await;
    let text = u["segmentation"]["units"][3]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let n = text.chars().count();
    let v = ok(
        &ctx.app,
        "POST",
        &format!("{I}/passages"),
        Some(json!({ "unit_id": units[3], "start": 0, "end": n, "text": text, "code": adv })),
    )
    .await;
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    let coding = phase2(&ctx).await;
    assert_eq!(coding["n_human_coded"], 1, "{coding}");

    // Removing the code removes the synced unit coding.
    ok(
        &ctx.app,
        "DELETE",
        &format!("/api/passages/{pid}/codings/{adv}?reason=wrong"),
        None,
    )
    .await;
    let coding = phase2(&ctx).await;
    assert_eq!(coding["n_human_coded"], 0, "{coding}");

    // Merging a code that carries a passage opens the worklist.
    ok(
        &ctx.app,
        "POST",
        &format!("{I}/passages"),
        Some(json!({ "unit_id": units[3], "start": 0, "end": 10, "code": cost })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{cost}/merge"),
        Some(json!({ "into": adv, "reason": "same", "rationale": "same" })),
    )
    .await;
    let w = ok(&ctx.app, "GET", &format!("{I}/harmonisation"), None).await;
    assert!(w["n_open"].as_i64().unwrap() >= 1, "{w}");
    let item = w["items"][0]["item_id"].as_i64().unwrap();
    assert_eq!(w["items"][0]["cause"], "merge");
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("{I}/harmonisation/{item}"),
        Some(json!({ "resolution": "maybe" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    ok(
        &ctx.app,
        "PUT",
        &format!("{I}/harmonisation/{item}"),
        Some(json!({ "resolution": "confirmed", "note": "reread" })),
    )
    .await;
    let w = ok(&ctx.app, "GET", "/api/harmonisation", None).await;
    assert_eq!(w["n_open"], 0);

    // Review state of the interview and of the corpus.
    let s = ok(&ctx.app, "GET", &format!("{I}/review-state"), None).await;
    assert!(s["state"].is_string(), "{s}");
    let c = ok(&ctx.app, "GET", "/api/review-state", None).await;
    assert!(c.is_object() || c.is_array());
}

#[tokio::test]
async fn the_export_carries_the_review_without_any_name() {
    let ctx = setup().await;
    code_respondent_units(&ctx).await;
    ok(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({ "enabled": true, "entries": [
            { "kind": "organisation", "original": "retailer", "pseudonym": "Organisation A" } ] })),
    )
    .await;
    let exp = ok(&ctx.app, "POST", &format!("{I}/export"), None).await;
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    let raw = std::fs::read_to_string(dir.join("review.json")).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["pseudonymisation"]["n_entries"], 1);
    assert_eq!(v["pseudonymisation"]["digest"].as_str().unwrap().len(), 64);
    assert!(!raw.contains("retailer"), "the list names stay out");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("review.json"));
    let state = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert!(state.contains("\"review\""));
}

#[tokio::test]
async fn pasted_arbitrations_preview_atomic_import_and_replacement() {
    let ctx = setup().await;
    let units = code_respondent_units(&ctx).await;
    let parts = prompts(&ctx).await;
    let (st, v) = run_and_import(&ctx, &parts[0], json!([])).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let before = ok(&ctx.app, "GET", &format!("{I}/review"), None).await;
    let snapshots = ok(&ctx.app, "GET", &format!("{I}/snapshots"), None).await;
    let cid = code_id(&ctx.app, "TECH-COST").await;
    let raw=json!({"interview_id":"INT-01","decisions":[
        {"unit_id":units[1],"decision":"accept_reviewer","final_codes":[cid],"comment":"Cost applies."},
        {"unit_id":units[3],"decision":"merge","final_codes":["TECH-COST","ORG-SKILLS"],"comment":"Both dimensions apply."}
    ]}).to_string();
    let base = format!("{I}/review/import");
    let preview = ok(
        &ctx.app,
        "POST",
        &format!("{base}/preview"),
        Some(json!({"raw":raw})),
    )
    .await;
    assert_eq!(preview["count"], 2);
    assert_eq!(preview["changes"][0]["after"]["choice"]["type"], "ai");
    assert_eq!(
        before,
        ok(&ctx.app, "GET", &format!("{I}/review"), None).await
    );
    let bad = raw.replace("ORG-SKILLS", "MISSING");
    let (st, _) = call(
        &ctx.app,
        "POST",
        &base,
        Some(json!({"raw":bad,"expected_digest":preview["digest"]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(
        before,
        ok(&ctx.app, "GET", &format!("{I}/review"), None).await
    );
    let result = ok(
        &ctx.app,
        "POST",
        &base,
        Some(json!({"raw":raw,"expected_digest":preview["digest"]})),
    )
    .await;
    assert_eq!(result["count"], 2);
    assert_eq!(
        snapshots,
        ok(&ctx.app, "GET", &format!("{I}/snapshots"), None).await
    );
    let preview = ok(
        &ctx.app,
        "POST",
        &format!("{base}/preview"),
        Some(json!({"raw":raw})),
    )
    .await;
    assert_eq!(preview["replacements"], 2);
    let (st, _) = call(
        &ctx.app,
        "POST",
        &base,
        Some(json!({"raw":raw,"expected_digest":preview["digest"]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    ok(
        &ctx.app,
        "POST",
        &base,
        Some(json!({"raw":raw,"expected_digest":preview["digest"],"replace_existing":true})),
    )
    .await;
    let after = ok(&ctx.app, "GET", &format!("{I}/review"), None).await;
    assert_eq!(after["items"][0]["history"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn pasted_arbitrations_reject_wrong_interview_duplicates_and_stale_preview() {
    let ctx = setup().await;
    let units = code_respondent_units(&ctx).await;
    let parts = prompts(&ctx).await;
    assert_eq!(
        run_and_import(&ctx, &parts[0], json!([])).await.0,
        StatusCode::OK
    );
    let row = json!({"unit_id":units[1],"decision":"keep_mine","final_codes":["TECH-ADV"],"comment":"Keep advantage."});
    let base = format!("{I}/review/import");
    for input in [
        json!({"interview_id":"INT-02","decisions":[row]}),
        json!({"interview_id":"INT-01","decisions":[row,row]}),
        json!({"interview_id":"INT-01","decisions":[]}),
    ] {
        assert_eq!(
            call(
                &ctx.app,
                "POST",
                &format!("{base}/preview"),
                Some(json!({"raw":input.to_string()}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let raw = json!({"interview_id":"INT-01","decisions":[row]}).to_string();
    let p = ok(
        &ctx.app,
        "POST",
        &format!("{base}/preview"),
        Some(json!({"raw":raw})),
    )
    .await;
    ok(&ctx.app,"PUT",&format!("{I}/review/{}",units[1]),Some(json!({"choice":{"type":"human"},"final_codes":["TECH-ADV"],"rationale":"Intervening decision"}))).await;
    assert_eq!(
        call(
            &ctx.app,
            "POST",
            &base,
            Some(json!({"raw":raw,"expected_digest":p["digest"],"replace_existing":true}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn pasted_arbitrations_roll_back_the_whole_batch_on_storage_failure() {
    let ctx = setup().await;
    let units = code_respondent_units(&ctx).await;
    let parts = prompts(&ctx).await;
    assert_eq!(
        run_and_import(&ctx, &parts[0], json!([])).await.0,
        StatusCode::OK
    );
    let before = ok(&ctx.app, "GET", &format!("{I}/review"), None).await;
    let raw = json!({"interview_id":"INT-01","decisions":[
        {"unit_id":units[1],"decision":"custom","final_codes":["NA"],"comment":"First decision"},
        {"unit_id":units[3],"decision":"custom","final_codes":["NA"],"comment":"Second decision"}
    ]})
    .to_string();
    let base = format!("{I}/review/import");
    let p = ok(
        &ctx.app,
        "POST",
        &format!("{base}/preview"),
        Some(json!({"raw":raw})),
    )
    .await;
    let conn = rusqlite::Connection::open(&ctx.db).unwrap();
    conn.execute_batch("CREATE TRIGGER test_batch_failure BEFORE INSERT ON arbitrations WHEN NEW.unit_id='INT-01-U004' BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert_eq!(
        call(
            &ctx.app,
            "POST",
            &base,
            Some(json!({"raw":raw,"expected_digest":p["digest"]}))
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        before,
        ok(&ctx.app, "GET", &format!("{I}/review"), None).await
    );
}
