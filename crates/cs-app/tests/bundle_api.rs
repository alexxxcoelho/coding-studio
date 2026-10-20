//! F-101: whole-project bundle (`.csproj.zip`): export, dry run, import with
//! verification of digests and chain.

mod common;

use std::io::{Cursor, Read, Write};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::Request;
use axum::Router;
use common::{code_id, ok, setup, unit_ids, Ctx};
use cs_app::{router, AppState};
use cs_store::Store;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn raw(app: &Router, method: &str, uri: &str, body: Vec<u8>) -> (u16, Vec<u8>) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    (
        status,
        res.into_body().collect().await.unwrap().to_bytes().to_vec(),
    )
}

/// A project with a passage, an attribute, a memo answer, a theme.
async fn filled() -> Ctx {
    let ctx = setup().await;
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([{"id": "kind", "label": "Type", "kind": "choice", "choices": ["communale", "scolaire"]}]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/attributes",
        Some(json!({"values": {"kind": "communale"}})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/memo/answers",
        Some(json!({"answers": {"q1": "Elle gère, seule."}})),
    )
    .await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let units = unit_ids(&ctx.app, "INT-01").await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[1], "start": 2, "end": 20, "code": adv})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/themes",
        Some(json!({"title": "Un thème qui affirme quelque chose", "codes": [adv]})),
    )
    .await;
    ctx
}

async fn bundle_of(ctx: &Ctx) -> Vec<u8> {
    let (st, bytes) = raw(&ctx.app, "GET", "/api/project/bundle", Vec::new()).await;
    assert_eq!(st, 200, "{}", String::from_utf8_lossy(&bytes));
    bytes
}

fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            (f.name().unwrap().to_string(), v)
        })
        .collect()
}

fn rezip(items: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in items {
        w.start_file(n.as_str(), zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(d).unwrap();
    }
    w.finish().unwrap().into_inner()
}

async fn bare(dir: &std::path::Path) -> (Router, std::path::PathBuf) {
    let db = dir.join("bare/coding_studio.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let state = AppState {
        store: Arc::new(Mutex::new(Store::open(&db).unwrap())),
        db_path: db.clone(),
        coding_db: None,
        actor: "candidate".into(),
        interviewer_names: vec![],
        picker: None,
    };
    (router(state), db)
}

#[tokio::test]
async fn the_bundle_holds_database_sources_config_audit_and_a_manifest_with_digests() {
    let ctx = filled().await;
    let bytes = bundle_of(&ctx).await;
    let items = entries(&bytes);
    let names: Vec<&str> = items.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names[0], "manifest.json");
    for expected in [
        "database.sqlite",
        "config/project_template.json",
        "config/study_config.json",
        "state.json",
        "audit/audit_log.jsonl",
        "audit/audit_chain.json",
        "sources/INT-01/02_transcript_cleaned_diarised.md",
        "sources/INT-02/02_transcript_cleaned_diarised.md",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
    let manifest: Value = serde_json::from_slice(&items[0].1).unwrap();
    assert_eq!(manifest["format"], "coding-studio-project-bundle");
    assert_eq!(manifest["counts"]["interviews"], 2);
    assert_eq!(manifest["counts"]["passages"], 1);
    assert_eq!(manifest["counts"]["themes"], 1);
    assert_eq!(manifest["audit"]["chain_ok"], true);
    for f in manifest["files"].as_array().unwrap() {
        let n = f["name"].as_str().unwrap();
        let data = &items.iter().find(|(x, _)| x == n).unwrap().1;
        assert_eq!(f["sha256"], cs_model::sha256_hex(data), "{n}");
    }
    let audit = ok(&ctx.app, "GET", "/api/audit", None).await.to_string();
    assert!(audit.contains("project.export"));
}

#[tokio::test]
async fn a_dry_run_reports_and_writes_nothing_then_apply_restores_the_project() {
    let ctx = filled().await;
    let bytes = bundle_of(&ctx).await;
    let target = ctx.dir.path().join("restored/new.db");
    let uri = |apply: bool| {
        format!(
            "/api/project/bundle/import?apply={apply}&target={}",
            target.display()
        )
    };

    let (st, body) = raw(&ctx.app, "POST", &uri(false), bytes.clone()).await;
    assert_eq!(st, 200);
    let dry: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["applied"], false);
    assert_eq!(dry["ok"], true, "{dry}");
    assert_eq!(dry["audit_chain_ok"], true);
    assert_eq!(dry["database_chain_ok"], true);
    assert_eq!(dry["state_ok"], true);
    assert!(dry["files_checked"].as_u64().unwrap() >= 8);
    assert_eq!(dry["counts_expected"]["passages"], 1);
    assert!(!target.exists(), "a dry run writes nothing");

    let (st, body) = raw(&ctx.app, "POST", &uri(true), bytes.clone()).await;
    assert_eq!(st, 200);
    let done: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(done["applied"], true, "{done}");
    assert_eq!(done["sources_restored"], 2);
    assert!(target.exists());

    // The new database holds the same project, verified independently.
    let store = Store::open(&target).unwrap();
    assert!(store.verify_chain().unwrap().ok);
    let counts = cs_app::bundle::project_counts(&store).unwrap();
    assert_eq!(counts["interviews"], 2);
    assert_eq!(counts["passages"], 1);
    assert_eq!(counts["themes"], 1);
    assert_eq!(counts["memos"], 1);
    assert_eq!(counts["attributes"], 1);
    assert_eq!(
        store.interview_attributes("INT-01").unwrap().values["kind"],
        "communale"
    );
    let events = store.audit_events(None).unwrap();
    assert_eq!(events.last().unwrap().action, "project.import");
    // Source files are back, with the digests the scan recorded, and the
    // interview points at them.
    let entry = store.interview("INT-01").unwrap().unwrap();
    for s in &entry.sources {
        let data =
            std::fs::read(std::path::Path::new(&entry.folder_path).join(&s.rel_path)).unwrap();
        assert_eq!(cs_model::sha256_hex(&data), s.sha256);
    }
    assert!(entry.folder_path.contains("new_sources"));
    // The imported state has the digest of the manifest.
    let manifest: Value = serde_json::from_slice(&entries(&bytes)[0].1).unwrap();
    assert_eq!(done["actual_state_sha256"], manifest["state_sha256"]);

    // Importing again into the same target is refused: it is not empty.
    let (_, body) = raw(&ctx.app, "POST", &uri(true), bytes).await;
    let again: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(again["ok"], false);
    assert_eq!(again["applied"], false);
    assert!(again["problems"]
        .to_string()
        .contains("already holds a project"));
}

#[tokio::test]
async fn a_modified_file_or_a_broken_chain_is_found_and_nothing_is_written() {
    let ctx = filled().await;
    let bytes = bundle_of(&ctx).await;
    let target = ctx.dir.path().join("tampered/new.db");
    let uri = format!(
        "/api/project/bundle/import?apply=true&target={}",
        target.display()
    );

    // 1. a transcript modified after the export
    let mut items = entries(&bytes);
    for (n, d) in items.iter_mut() {
        if n == "sources/INT-02/02_transcript_cleaned_diarised.md" {
            d.extend_from_slice(b"\nan added line");
        }
    }
    let (_, body) = raw(&ctx.app, "POST", &uri, rezip(&items)).await;
    let r: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r["ok"], false);
    assert_eq!(
        r["files_bad"][0]["name"],
        "sources/INT-02/02_transcript_cleaned_diarised.md"
    );
    assert!(!target.exists());

    // 2. an audit event edited, manifest digests recomputed to hide it
    let mut items = entries(&bytes);
    let mut manifest: Value = serde_json::from_slice(&items[0].1).unwrap();
    for (n, d) in items.iter_mut() {
        if n == "audit/audit_log.jsonl" {
            let text = String::from_utf8(d.clone()).unwrap();
            *d = text
                .replacen("\"candidate\"", "\"someone-else\"", 1)
                .into_bytes();
            for f in manifest["files"].as_array_mut().unwrap() {
                if f["name"] == "audit/audit_log.jsonl" {
                    f["sha256"] = json!(cs_model::sha256_hex(d));
                }
            }
        }
    }
    items[0].1 = serde_json::to_vec(&manifest).unwrap();
    let (_, body) = raw(&ctx.app, "POST", &uri, rezip(&items)).await;
    let r: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["audit_chain_ok"], false);
    assert!(r["problems"].to_string().contains("audit chain"));
    assert!(!target.exists());

    // 3. not a bundle at all
    let (st, _) = raw(&ctx.app, "POST", &uri, b"hello".to_vec()).await;
    assert_eq!(st, 400);
    let (st, _) = raw(
        &ctx.app,
        "POST",
        &uri,
        rezip(&[("a.txt".into(), b"x".to_vec())]),
    )
    .await;
    assert_eq!(st, 400);
}

#[tokio::test]
async fn the_current_empty_database_can_receive_the_project() {
    let ctx = filled().await;
    let bytes = bundle_of(&ctx).await;
    let (app, _db) = bare(ctx.dir.path()).await;

    let (_, body) = raw(&app, "POST", "/api/project/bundle/import", bytes.clone()).await;
    let dry: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(dry["ok"], true, "{dry}");
    assert_eq!(dry["applied"], false);

    let (st, body) = raw(
        &app,
        "POST",
        "/api/project/bundle/import?apply=true",
        bytes.clone(),
    )
    .await;
    assert_eq!(st, 200);
    let done: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(done["applied"], true, "{done}");
    let list = ok(&app, "GET", "/api/interviews", None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(ok(&app, "GET", "/api/audit/verify", None).await["ok"], true);

    // Now it is not empty any more.
    let (_, body) = raw(&app, "POST", "/api/project/bundle/import?apply=true", bytes).await;
    let again: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(again["applied"], false);
    assert_eq!(
        ok(&app, "GET", "/api/interviews", None)
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
