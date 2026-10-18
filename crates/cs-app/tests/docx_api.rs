//! F-104: the write-up as a Word document, from the package and from the route.

mod common;

use std::io::Read;

use axum::body::Body;
use axum::http::Request;
use common::{ok, setup, unit_ids, Ctx};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn written() -> Ctx {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/codes/from-passage",
        Some(json!({
            "name": "Exactitude des stocks",
            "definition": "Le répondant parle de l'exactitude de l'inventaire.",
            "passage": { "interview_id": "INT-01", "unit_id": unit, "start": 4, "end": 31 },
        })),
    )
    .await;
    let code = v["code"]["code_id"].as_str().unwrap().to_string();
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    let t = ok(
        &ctx.app,
        "POST",
        "/api/themes",
        Some(
            json!({"title": "L'exactitude des stocks justifie l'investissement", "codes": [code]}),
        ),
    )
    .await;
    let tid = t["theme_id"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "POST",
        &format!("/api/themes/{tid}/status"),
        Some(json!({"status": "retained", "override_reason": "test"})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/writing/themes/{tid}/parts/mechanism_evidence"),
        Some(json!({"text": "Le stock devient exact."})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/links",
        Some(json!({
            "passage_id": pid,
            "target": {"kind": "theme_part", "theme_id": tid, "part": "mechanism_evidence"},
            "link_type": "supports",
        })),
    )
    .await;
    common::approve_all_anonymisation(&ctx.app).await;
    ctx
}

fn entry(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[tokio::test]
async fn the_writeup_package_holds_a_docx_listed_in_the_manifest() {
    let ctx = written().await;
    let r = ok(&ctx.app, "POST", "/api/writing/export", Some(json!({}))).await;
    let dir = std::path::PathBuf::from(r["dir"].as_str().unwrap());
    let bytes = std::fs::read(dir.join("writeup.docx")).unwrap();
    let doc = entry(&bytes, "word/document.xml");
    assert!(doc.contains("L&apos;exactitude") || doc.contains("L'exactitude des stocks justifie"));
    assert!(doc.contains("w:val=\"Heading2\""));
    assert!(doc.contains("Le stock devient exact."));
    assert!(doc.contains("<w:pStyle w:val=\"Quote\"/>"));
    assert!(doc.contains("<w:footnoteReference w:id=\"1\"/>"));
    let notes = entry(&bytes, "word/footnotes.xml");
    assert!(notes.contains("INT-01, unité INT-01-U004"), "{notes}");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert!(manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "writeup.docx"));
}

#[tokio::test]
async fn the_route_streams_the_same_document() {
    let ctx = written().await;
    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/writing/export.docx")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(res.headers()["content-type"]
        .to_str()
        .unwrap()
        .contains("wordprocessingml.document"));
    assert!(res.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("writeup.docx"));
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..2], b"PK");
    assert!(entry(&bytes, "word/document.xml").contains("Le stock devient exact."));
}

#[tokio::test]
async fn analysis_report_is_deterministic_traceable_and_separate_from_writeup() {
    let ctx = written().await;
    let mut documents = Vec::new();
    for _ in 0..2 {
        let res = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/analysis/export.docx")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        assert!(res.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("analysis-report.docx"));
        documents.push(res.into_body().collect().await.unwrap().to_bytes());
    }
    assert_eq!(documents[0], documents[1]);
    let d = entry(&documents[0], "word/document.xml");
    for expected in [
        "Analysis report and evidence register",
        "Le stock devient exact.",
        "Cross-case comparison",
        "Codebook and descriptive coverage",
        "independent researcher validation",
        "no generative model at export time",
        "INT-01",
        "unassessed",
        "not evidence of absence",
    ] {
        assert!(d.contains(expected), "missing {expected}");
    }
    let notes = entry(&documents[0], "word/footnotes.xml");
    assert!(d.contains("passage P"));
    assert!(d.contains("verification: unverified"));
    assert!(d.contains("source timestamp: 00:24"));
    assert!(d.contains("not an audio verification"));
    assert!(d.contains("INT-01-U004"));
    assert!(!d.contains("<w:footnoteReference"));
    assert!(d.contains("w:val=\"Title\""));
    assert!(notes.contains("<w:footnotes"));
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&documents[0][..])).unwrap();
    for i in 0..z.len() {
        let mut s = String::new();
        z.by_index(i).unwrap().read_to_string(&mut s).unwrap();
        roxmltree::Document::parse(&s).unwrap();
    }
}

#[tokio::test]
async fn analysis_package_keeps_versioned_files_and_matching_hashes() {
    let ctx = written().await;
    let first = ok(&ctx.app, "POST", "/api/analysis/export", Some(json!({}))).await;
    let second = ok(&ctx.app, "POST", "/api/analysis/export", Some(json!({}))).await;
    assert_ne!(first["dir"], second["dir"]);
    assert_eq!(first["files"].as_array().unwrap().len(), 4);
    let store = cs_store::Store::open(&ctx.db).unwrap();
    let events = store.audit_events(None).unwrap();
    let exports: Vec<_> = events
        .iter()
        .filter(|e| e.action == "export.analysis")
        .collect();
    assert_eq!(exports.len(), 2);
    let event = serde_json::to_value(exports[0]).unwrap();
    assert!(event
        .to_string()
        .contains(first["files"][0]["sha256"].as_str().unwrap()));
    assert!(event.to_string().contains(first["dir"].as_str().unwrap()));

    let dir = std::path::PathBuf::from(first["dir"].as_str().unwrap());
    for f in first["files"].as_array().unwrap() {
        let bytes = std::fs::read(dir.join(f["name"].as_str().unwrap())).unwrap();
        assert_eq!(cs_model::sha256_hex(&bytes), f["sha256"].as_str().unwrap());
    }
    assert!(dir.join("analysis-evidence.json").exists());
    let md = std::fs::read_to_string(dir.join("analysis-report.md")).unwrap();
    assert!(md.contains("Le stock devient exact."));
    assert!(md.contains("unverified"));
}

#[tokio::test]
async fn report_masks_configured_names_even_when_ai_prompt_masking_is_disabled() {
    let ctx = written().await;
    ok(&ctx.app, "PUT", "/api/pseudonymisation", Some(json!({"enabled": false, "entries": [{"kind": "person", "original": "Le stock", "pseudonym": "Respondent A"}]}))).await;
    common::approve_all_anonymisation(&ctx.app).await;
    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/analysis/export.docx")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let d = entry(&bytes, "word/document.xml");
    assert!(d.contains("Respondent A devient exact."));
    assert!(!d.contains("Le stock devient exact."));
}
