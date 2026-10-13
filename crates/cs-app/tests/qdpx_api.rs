//! REFI-QDA routes (F-103): export of the codebook and the project, import
//! with a dry run and an apply, error report for a bad file.

mod common;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use common::{call, code_id, ok, setup, unit_ids};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// Raw request and answer: the REFI files are not JSON.
async fn raw(
    app: &Router,
    method: &str,
    uri: &str,
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, bytes)
}

fn json_of(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into()))
}

async fn code_up(ctx: &common::Ctx) {
    let units = unit_ids(&ctx.app, "INT-01").await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let skills = code_id(&ctx.app, "ORG-SKILLS").await;
    // Unit 2 is a respondent turn.
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[1], "start": 3, "end": 40, "code": adv})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[3], "code": skills})),
    )
    .await;
    // A second code on the same span.
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[3], "code": adv})),
    )
    .await;
}

#[tokio::test]
async fn qdc_export_is_an_attachment_with_the_codes() {
    let ctx = setup().await;
    let (st, h, body) = raw(&ctx.app, "GET", "/api/export/qdc", vec![]).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(h["content-type"], "application/xml");
    assert!(h["content-disposition"]
        .to_str()
        .unwrap()
        .contains("codebook.qdc"));
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<CodeBook xmlns=\"urn:QDA-XML:codebook:1.0\""));
    assert!(xml.contains("name=\"TECH-ADV\""));
    assert!(xml.contains("Perceived advantage over the status quo."));
    assert!(!xml.contains("Plus tard"));
}

#[tokio::test]
async fn qdpx_export_is_a_zip_with_a_digest_header() {
    let ctx = setup().await;
    code_up(&ctx).await;
    let (st, h, body) = raw(&ctx.app, "GET", "/api/export/qdpx?name=%C3%89tude", vec![]).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(h["content-type"], "application/zip");
    assert!(h["content-disposition"]
        .to_str()
        .unwrap()
        .contains("project.qdpx"));
    assert_eq!(
        h["x-export-sha256"].to_str().unwrap(),
        cs_model::sha256_hex(&body)
    );
    let summary: Value = serde_json::from_str(h["x-export-summary"].to_str().unwrap()).unwrap();
    assert_eq!(summary["sources"], 2);
    assert_eq!(summary["selections"], 2);
    assert_eq!(summary["codings"], 3);
    assert_eq!(&body[..2], b"PK");
    // Same project, same bytes.
    let (_, _, again) = raw(&ctx.app, "GET", "/api/export/qdpx?name=%C3%89tude", vec![]).await;
    assert_eq!(body, again);
}

#[tokio::test]
async fn project_round_trip_between_two_projects_with_dry_run_then_apply() {
    let a = setup().await;
    code_up(&a).await;
    let (_, _, file) = raw(&a.app, "GET", "/api/export/qdpx", vec![]).await;

    // Another project with the same interviews and codebook, no passage.
    let b = setup().await;
    let (st, _, body) = raw(&b.app, "POST", "/api/import/qdpx", file.clone()).await;
    assert_eq!(st, StatusCode::OK);
    let dry = json_of(&body);
    assert_eq!(dry["applied"], false, "dry run by default");
    assert_eq!(dry["kind"], "qdpx");
    assert_eq!(dry["codes_matched"], 4);
    assert_eq!(dry["passages_created"], 2);
    assert_eq!(dry["codings_added"], 3);
    let none = ok(&b.app, "GET", "/api/interviews/INT-01/passages", None).await;
    assert_eq!(none.as_array().unwrap().len(), 0, "dry run wrote nothing");

    let (st, _, body) = raw(&b.app, "POST", "/api/import/qdpx?apply=true", file.clone()).await;
    assert_eq!(st, StatusCode::OK);
    let done = json_of(&body);
    assert_eq!(done["applied"], true);
    assert_eq!(done["passages_created"], 2);
    let list = ok(&b.app, "GET", "/api/interviews/INT-01/passages", None).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["text"].as_str().unwrap().chars().count(), 37);
    let events = ok(&b.app, "GET", "/api/audit", None).await;
    assert!(events
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "refi.import.qdpx"));
    assert_eq!(
        ok(&b.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );

    // Again: nothing new.
    let (_, _, body) = raw(&b.app, "POST", "/api/import/qdpx?apply=true", file).await;
    let again = json_of(&body);
    assert_eq!(again["passages_created"], 0);
    assert_eq!(again["codings_added"], 0);
}

#[tokio::test]
async fn qdc_import_creates_nested_codes_and_reports_the_plan_first() {
    let ctx = setup().await;
    let qdc = br#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
        <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="Freins" isCodable="false">
          <Description>Ce qui freine.</Description>
          <Code guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="Peur du changement" isCodable="true"/>
        </Code>
        <Code guid="cccccccc-cccc-4ccc-8ccc-cccccccccccc" name="tech-adv" isCodable="true"/>
      </Codes></CodeBook>"#
        .to_vec();
    let (st, _, body) = raw(&ctx.app, "POST", "/api/import/qdc", qdc.clone()).await;
    assert_eq!(st, StatusCode::OK);
    let dry = json_of(&body);
    assert_eq!(dry["applied"], false);
    assert_eq!(dry["codes_created"], 2);
    assert_eq!(dry["codes_matched"], 1);
    assert_eq!(dry["codes"][1]["parent"], "Freins");
    assert_eq!(dry["codes"][2]["outcome"], "match");

    let (st, _, body) = raw(&ctx.app, "POST", "/api/import/qdc?apply=true", qdc).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_of(&body)["applied"], true);
    let codes = ok(&ctx.app, "GET", "/api/codes", None).await;
    let codes = codes["codes"].as_array().unwrap();
    let child = codes
        .iter()
        .find(|c| c["name"] == "Peur du changement")
        .unwrap();
    let parent = codes.iter().find(|c| c["name"] == "Freins").unwrap();
    assert_eq!(child["parent_id"], parent["code_id"]);
    assert_eq!(parent["definition"], "Ce qui freine.");
}

#[tokio::test]
async fn a_bad_file_is_a_400_with_the_problems_listed() {
    let ctx = setup().await;
    for (uri, body) in [
        ("/api/import/qdc", b"<CodeBook><Codes>".to_vec()),
        ("/api/import/qdc?apply=true", b"not xml".to_vec()),
        ("/api/import/qdpx", b"not a zip".to_vec()),
        ("/api/import/qdpx?apply=true", Vec::new()),
    ] {
        let (st, _, bytes) = raw(&ctx.app, "POST", uri, body).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{uri}");
        let v = json_of(&bytes);
        assert_eq!(v["kind"], "validation", "{uri}: {v}");
        assert!(v["error"].as_str().unwrap().contains("file rejected"));
        let issues = v["issues"].as_array().unwrap();
        assert!(!issues.is_empty());
        assert!(issues[0]["message"].as_str().unwrap().len() > 5);
    }
    // Nothing was written by the failed applies.
    let (st, v) = call(&ctx.app, "GET", "/api/codes", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        v["codes"].as_array().unwrap().len(),
        5,
        "four codes and Plus tard"
    );
}
