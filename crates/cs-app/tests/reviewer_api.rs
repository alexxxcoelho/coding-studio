mod common;
use common::{call, ok, setup};
use serde_json::{json, Value};

fn selected(c: &Value, category: &str, field: &str) -> String {
    c["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| {
            f["path"][0] == category && f["path"].as_array().unwrap().last().unwrap() == field
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}

#[tokio::test]
async fn catalogue_is_whitelisted_and_private_fields_start_unchecked() {
    let ctx = setup().await;
    let c = ok(&ctx.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let fields = c["fields"].as_array().unwrap();
    assert!(!fields.is_empty());
    for field in fields {
        let path = field["path"].as_array().unwrap();
        assert!(!["settings", "audit_log", "providers", "folder_path"]
            .iter()
            .any(|s| path.iter().any(|p| p == s)));
        if ["Interviews", "Source files", "People"].contains(&path[0].as_str().unwrap()) {
            assert_eq!(field["selected_by_default"], false);
        }
    }
}

#[tokio::test]
async fn exact_selection_produces_only_a_protected_local_package() {
    let ctx = setup().await;
    let c = ok(&ctx.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let id = selected(&c, "Transcript", "text");
    let r=ok(&ctx.app,"POST","/api/export/reviewer",Some(json!({"snapshot":c["snapshot"],"selected":[id],"password":"synthetic API test only 12345"}))).await;
    assert_eq!(r["fields"], 1);
    assert_eq!(r["files"], 0);
    assert_eq!(r["published"], false);
    assert!(r["url"].is_null());
    let dir = std::path::Path::new(r["dir"].as_str().unwrap());
    assert!(!dir.join("QR.svg").exists());
    assert!(!dir.join("URL.txt").exists());
    let html = std::fs::read_to_string(dir.join("index.html")).unwrap();
    assert!(!html.contains("synthetic API test only 12345"));
    assert!(!html.contains("supplier tags every carton"));
    let zip = zip::ZipArchive::new(std::fs::File::open(dir.join("upload.zip")).unwrap()).unwrap();
    assert_eq!(
        zip.file_names().map(Result::unwrap).collect::<Vec<_>>(),
        ["README.txt", "index.html"]
    );
}

#[tokio::test]
async fn source_files_are_encrypted_and_changed_sources_fail_without_final_package() {
    let ctx = setup().await;
    let c = ok(&ctx.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let id = selected(&c, "Source files", "download");
    let body = json!({"snapshot":c["snapshot"],"selected":[id],"password":"synthetic API test only 12345"});
    let r = ok(&ctx.app, "POST", "/api/export/reviewer", Some(body.clone())).await;
    assert_eq!(r["files"], 1);
    let data = std::path::Path::new(r["dir"].as_str().unwrap()).join("data");
    let files: Vec<_> = std::fs::read_dir(data).unwrap().collect();
    assert_eq!(files.len(), 1);
    let bytes = std::fs::read(files[0].as_ref().unwrap().path()).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("interview_id"));
    // Both possible sources change; whichever catalogue record was selected must fail.
    for entry in std::fs::read_dir(&ctx.root).unwrap() {
        let p = entry
            .unwrap()
            .path()
            .join("02_transcript_cleaned_diarised.md");
        if p.exists() {
            std::fs::write(p, "changed").unwrap();
        }
    }
    let (status, _) = call(&ctx.app, "POST", "/api/export/reviewer", Some(body)).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(std::fs::read_dir(ctx.db.parent().unwrap().join("exports"))
        .unwrap()
        .all(|e| e
            .unwrap()
            .path()
            .extension()
            .is_none_or(|s| s != "building")));
}

#[tokio::test]
async fn unknown_stale_empty_and_bad_password_are_refused() {
    let ctx = setup().await;
    let c = ok(&ctx.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let id = selected(&c, "Transcript", "text");
    for (snapshot, ids, password) in [
        (
            "stale".to_string(),
            vec![id.clone()],
            "synthetic API test only 12345",
        ),
        (
            c["snapshot"].as_str().unwrap().into(),
            vec!["unknown".into()],
            "synthetic API test only 12345",
        ),
        (
            c["snapshot"].as_str().unwrap().into(),
            vec![],
            "synthetic API test only 12345",
        ),
        (c["snapshot"].as_str().unwrap().into(), vec![id], "short"),
    ] {
        let (status, _) = call(
            &ctx.app,
            "POST",
            "/api/export/reviewer",
            Some(json!({"snapshot":snapshot,"selected":ids,"password":password})),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn real_https_address_generates_a_qr_without_publishing() {
    let ctx = setup().await;
    let c = ok(&ctx.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let id = selected(&c, "Transcript", "text");
    let r=ok(&ctx.app,"POST","/api/export/reviewer",Some(json!({"snapshot":c["snapshot"],"selected":[id],"password":"synthetic API test only 12345","url":"https://example.org/thesis/"}))).await;
    let dir = std::path::Path::new(r["dir"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(dir.join("URL.txt")).unwrap(),
        "https://example.org/thesis/\n"
    );
    assert!(std::fs::read_to_string(dir.join("QR.svg"))
        .unwrap()
        .contains("<svg"));
    assert_eq!(r["published"], false);
}
