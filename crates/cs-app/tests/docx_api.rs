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
