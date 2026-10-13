//! F-24 through the HTTP API: a rescan with a modified file, a unit
//! correction, the review list and its three decisions, the version in the
//! exports.

mod common;

use axum::http::StatusCode;
use common::{call, code_id, ok, setup, unit_ids, Ctx, CLEANED};
use serde_json::{json, Value};

const SENTENCE: &str = "The inventory accuracy went from sixty to ninety-eight percent in one year.";

/// "ninety-eight percent" of the second respondent unit, coded TECH-ADV.
async fn coded(ctx: &Ctx) -> (String, String) {
    let units = unit_ids(&ctx.app, "INT-01").await;
    let unit = units[3].clone();
    let a = SENTENCE.find("ninety-eight percent").unwrap();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({
            "unit_id": unit, "start": a, "end": a + 20, "code": adv,
            "text": "ninety-eight percent",
        })),
    )
    .await;
    (unit, v["passage"]["passage_id"].as_str().unwrap().to_string())
}

async fn rewrite_and_rescan(ctx: &Ctx, from: &str, to: &str) {
    let path = ctx.root.join("INT-01_Test/02_transcript_cleaned_diarised.md");
    std::fs::write(&path, CLEANED.replace(from, to)).unwrap();
    ok(
        &ctx.app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    ok(&ctx.app, "POST", "/api/interviews/INT-01/segment", Some(json!({}))).await;
}

#[tokio::test]
async fn a_rescan_with_a_modified_file_adds_a_version_and_re_anchors_untouched_passages() {
    let ctx = setup().await;
    let (_, pid) = coded(&ctx).await;
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-01/transcript/versions", None).await;
    assert_eq!(v["current"], 1);
    rewrite_and_rescan(&ctx, "The inventory accuracy", "Honestly, the inventory accuracy").await;
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-01/transcript/versions", None).await;
    assert_eq!(v["current"], 2);
    assert_eq!(v["versions"][1]["cause"], "segmentation");
    assert_eq!(v["versions"][1]["reanchored"], 1);
    let p = ok(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(p["stale"], false);
    assert_eq!(p["text"], "ninety-eight percent");
    let r = ok(&ctx.app, "GET", "/api/transcript/review", None).await;
    assert_eq!(r["open"], 0);
    let old = ok(&ctx.app, "GET", "/api/interviews/INT-01/transcript/versions/1", None).await;
    assert!(old["units"][3]["text"].as_str().unwrap().starts_with("The inventory"));
}

#[tokio::test]
async fn a_correction_touching_a_passage_is_listed_and_can_be_accepted() {
    let ctx = setup().await;
    let (unit, pid) = coded(&ctx).await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/INT-01/units/{unit}/corrections"),
        Some(json!({
            "text": SENTENCE.replace("ninety-eight percent", "ninety-nine percent"),
            "role": "respondent", "reason": "heard again",
        })),
    )
    .await;
    let r = ok(&ctx.app, "GET", "/api/interviews/INT-01/transcript/review", None).await;
    assert_eq!(r["open"], 1);
    let it = &r["items"][0];
    assert_eq!(it["passage_id"], pid);
    assert_eq!(it["old"]["text"], "ninety-eight percent");
    assert!(it["proposed"]["text"].as_str().unwrap().contains("nine"));
    assert_eq!(it["codes"][0]["code_name"].is_string(), true);
    let id = it["projection_id"].as_i64().unwrap();
    let done = ok(
        &ctx.app,
        "POST",
        &format!("/api/transcript/review/{id}/accept"),
        Some(json!({"note": "ok"})),
    )
    .await;
    assert_eq!(done["status"], "accepted");
    let r = ok(&ctx.app, "GET", "/api/transcript/review", None).await;
    assert_eq!(r["open"], 0);
    let (st, again) = call(&ctx.app, "POST", &format!("/api/transcript/review/{id}/accept"), None).await;
    assert_eq!(st, StatusCode::CONFLICT, "{again}");
}

#[tokio::test]
async fn adjust_and_detach_through_the_api() {
    let ctx = setup().await;
    let (unit, pid) = coded(&ctx).await;
    let corrected = SENTENCE.replace("ninety-eight percent", "ninety-nine percent");
    ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/INT-01/units/{unit}/corrections"),
        Some(json!({"text": corrected, "role": "respondent", "reason": "r"})),
    )
    .await;
    let r = ok(&ctx.app, "GET", "/api/transcript/review?interview_id=INT-01", None).await;
    let id = r["items"][0]["projection_id"].as_i64().unwrap();
    let (st, bad) = call(
        &ctx.app,
        "POST",
        &format!("/api/transcript/review/{id}/adjust"),
        Some(json!({"start": 0, "end": 900})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}");
    let a = corrected.find("one year").unwrap();
    let done = ok(
        &ctx.app,
        "POST",
        &format!("/api/transcript/review/{id}/adjust"),
        Some(json!({"start": a, "end": a + 8, "offset_unit": "utf16", "note": "moved"})),
    )
    .await;
    assert_eq!(done["status"], "adjusted");
    assert_eq!(done["final"]["text"], "one year");
    let p = ok(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(p["text"], "one year");

    // A second correction, then detach.
    ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/INT-01/units/{unit}/corrections"),
        Some(json!({"text": corrected.replace("one year", "two years"), "role": "respondent", "reason": "r"})),
    )
    .await;
    let r = ok(&ctx.app, "GET", "/api/transcript/review", None).await;
    let id = r["items"][0]["projection_id"].as_i64().unwrap();
    let done = ok(
        &ctx.app,
        "POST",
        &format!("/api/transcript/review/{id}/detach"),
        Some(json!({"note": "no longer said"})),
    )
    .await;
    assert_eq!(done["status"], "detached");
    let (st, _) = call(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let all = ok(&ctx.app, "GET", "/api/transcript/review?status=all", None).await;
    let statuses: Vec<&str> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["status"].as_str().unwrap())
        .collect();
    assert!(statuses.contains(&"adjusted") && statuses.contains(&"detached"), "{statuses:?}");
    let (st, bad) = call(&ctx.app, "GET", "/api/transcript/review?status=nope", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}");
    let _: Value = bad;
}

#[tokio::test]
async fn exports_carry_the_transcript_version_and_the_projections() {
    let ctx = setup().await;
    let (unit, pid) = coded(&ctx).await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/INT-01/units/{unit}/corrections"),
        Some(json!({
            "text": format!("Well, {SENTENCE}"),
            "role": "respondent", "reason": "added a word",
        })),
    )
    .await;
    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    let csv = std::fs::read_to_string(dir.join("passages.csv")).unwrap();
    let mut lines = csv.lines();
    assert!(lines
        .next()
        .unwrap()
        .ends_with(",created_by,transcript_version"));
    let row = lines.find(|l| l.starts_with(&pid)).unwrap();
    assert!(row.ends_with(",2"), "{row}");
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("transcript_versions.json")).unwrap(),
    )
    .unwrap();
    let i = &v["interviews"][0];
    assert_eq!(i["current_version"], 2);
    assert_eq!(i["versions"].as_array().unwrap().len(), 2);
    assert_eq!(i["projections"][0]["status"], "reanchored");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("transcript_versions.json"));
    let state = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert!(state.contains("\"transcript_versions\""));
}
