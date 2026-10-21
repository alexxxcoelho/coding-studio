mod common;
use axum::{body::Body, http::Request};
use common::{ok, setup};
use http_body_util::BodyExt;
use serde_json::json;
use std::io::Read;
use tower::ServiceExt;
#[tokio::test]
async fn participant_word_contains_only_the_selected_interview_and_approved_translation() {
    let ctx = setup().await;
    let code = common::code_id(&ctx.app, "TECH-ADV").await;
    ok(&ctx.app,"PUT",&format!("/api/codes/{code}"),Some(json!({"name":"Stock visibility"}))).await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id":"INT-01-U002","start":0,"end":5,"code":code})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/themes",
        Some(json!({"title":"Fictional relevant theme","codes":[code]})),
    )
    .await;
    let store = cs_store::Store::open(&ctx.db).unwrap();
    let original = store.corrected_segmentation("INT-01").unwrap().unwrap();
    let mut prepared = original.clone();
    for u in &mut prepared.units {
        u.text = "Approved English translation marker".into();
    }
    store.set_setting("local_preparation:INT-01",&json!({"reviewed":true,"accepted_variant":"prepared","applied_to_coding":false,"original":original,"prepared":prepared,"options":{"model":"fictional-model","target_language":"English"}}).to_string(),"test").unwrap();
    let before = store.audit_events(Some("INT-01")).unwrap().len();
    drop(store);
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/interviews/INT-01/participant-review.docx")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    if let Ok(path) = std::env::var("CS_PARTICIPANT_QA_PATH") {
        std::fs::write(path, &bytes).unwrap();
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("Interview review and validation — INT-01"));
    assert!(xml.contains("I run the logistics"));
    let parsed = roxmltree::Document::parse(&xml).unwrap();
    let paragraphs = parsed
        .descendants()
        .filter(|n| n.tag_name().name() == "p")
        .map(|n| {
            n.descendants()
                .filter(|c| c.tag_name().name() == "t")
                .filter_map(|c| c.text())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let turn = paragraphs
        .iter()
        .position(|p| p.starts_with("I run the logistics"))
        .unwrap();
    assert!(
        paragraphs[turn + 1].starts_with("Codes: Stock visibility [C-"),
        "codes must immediately follow the turn: {:?}", &paragraphs[turn..turn+3]
    );
    assert_eq!(
        paragraphs
            .iter()
            .filter(|p| p.starts_with("Codes: "))
            .count(),
        12,
        "each original and translated turn has a code annotation"
    );
    let mut styles = String::new();
    zip.by_name("word/styles.xml")
        .unwrap()
        .read_to_string(&mut styles)
        .unwrap();
    assert!(styles.contains("en-GB"));
    assert!(!xml.contains("Relecture et validation"));
    assert!(xml.contains("Approved English translation marker"));
    assert!(xml.contains("Fictional relevant theme"));
    assert!(xml.contains("Perceived advantage over the status quo"));
    assert!(!xml.contains("INT-02"));
    assert!(!xml.contains("1 200 livres"));
    assert!(!xml.contains("AI phase 2"));
    let store = cs_store::Store::open(&ctx.db).unwrap();
    assert_eq!(store.audit_events(Some("INT-01")).unwrap().len(), before);
}
