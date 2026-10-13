//! F-62 through the HTTP API: audio files of an interview folder, range
//! requests, a route that reaches only those files, unit times in seconds and
//! the manual offset.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{call, ok, setup, Ctx};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// 1000 bytes: the byte at position i is `i % 251`.
fn audio_bytes() -> Vec<u8> {
    (0..1000u32).map(|i| (i % 251) as u8).collect()
}

fn put_audio(ctx: &Ctx) {
    std::fs::write(ctx.root.join("INT-01_Test/entretien.m4a"), audio_bytes()).unwrap();
}

struct Raw {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

async fn raw(ctx: &Ctx, method: &str, uri: &str, range: Option<&str>) -> Raw {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(r) = range {
        req = req.header("range", r);
    }
    let res = ctx
        .app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Raw {
        status,
        headers,
        body,
    }
}

const FILE: &str = "/api/interviews/INT-01/audio/file";

#[tokio::test]
async fn the_audio_map_lists_files_and_unit_times() {
    let ctx = setup().await;
    put_audio(&ctx);
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-01/audio", None).await;
    assert_eq!(v["has_audio"], true);
    assert_eq!(v["files"][0]["rel_path"], "entretien.m4a");
    assert_eq!(v["files"][0]["size"], 1000);
    assert_eq!(v["files"][0]["mime"], "audio/mp4");
    assert_eq!(
        v["files"][0]["url"],
        "/api/interviews/INT-01/audio/file?path=entretien.m4a"
    );
    assert_eq!(v["offset"]["offset_seconds"], 0.0);
    let units = v["units"].as_array().unwrap();
    assert_eq!(units.len(), 6);
    assert_eq!(units[1]["timestamp"], "00:05");
    assert_eq!(units[1]["start_seconds"], 5.0);
    assert_eq!(units[1]["audio_seconds"], 5.0);
    assert_eq!(units[1]["end_seconds"], 20.0);
    assert_eq!(units[5]["start_seconds"], 43.0);
    assert!(units[5]["end_seconds"].is_null());
    assert_eq!(v["timed_units"], 6);
    assert!(v["warnings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_interview_without_audio_says_so() {
    let ctx = setup().await;
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-02/audio", None).await;
    assert_eq!(v["has_audio"], false);
    assert!(v["files"].as_array().unwrap().is_empty());
    assert!(v["warnings"][0].as_str().unwrap().contains("no audio"));
    let r = raw(&ctx, "GET", "/api/interviews/INT-02/audio/file", None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_whole_file_is_served_with_its_type_and_range_support_announced() {
    let ctx = setup().await;
    put_audio(&ctx);
    let r = raw(&ctx, "GET", &format!("{FILE}?path=entretien.m4a"), None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["content-type"], "audio/mp4");
    assert_eq!(r.headers["accept-ranges"], "bytes");
    assert_eq!(r.headers["content-length"], "1000");
    assert_eq!(r.body, audio_bytes());
    // Without ?path the first recording is served.
    let r = raw(&ctx, "GET", FILE, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body.len(), 1000);
}

#[tokio::test]
async fn a_range_gets_a_206_with_exactly_those_bytes() {
    let ctx = setup().await;
    put_audio(&ctx);
    let all = audio_bytes();
    let uri = format!("{FILE}?path=entretien.m4a");
    let r = raw(&ctx, "GET", &uri, Some("bytes=100-199")).await;
    assert_eq!(r.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(r.headers["content-range"], "bytes 100-199/1000");
    assert_eq!(r.headers["content-length"], "100");
    assert_eq!(r.body, &all[100..200]);
    let r = raw(&ctx, "GET", &uri, Some("bytes=990-")).await;
    assert_eq!(r.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(r.headers["content-range"], "bytes 990-999/1000");
    assert_eq!(r.body, &all[990..]);
    let r = raw(&ctx, "GET", &uri, Some("bytes=-10")).await;
    assert_eq!(r.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(r.body, &all[990..]);
    let r = raw(&ctx, "GET", &uri, Some("bytes=0-")).await;
    assert_eq!(r.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(r.body, all);
    let r = raw(&ctx, "GET", &uri, Some("bytes=5000-6000")).await;
    assert_eq!(r.status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(r.headers["content-range"], "bytes */1000");
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn a_head_request_answers_without_a_body() {
    let ctx = setup().await;
    put_audio(&ctx);
    let r = raw(&ctx, "HEAD", &format!("{FILE}?path=entretien.m4a"), None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["content-length"], "1000");
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn the_route_reaches_only_audio_files_of_the_interview_folder() {
    let ctx = setup().await;
    put_audio(&ctx);
    // A file that is not audio, in the same folder.
    for bad in [
        "02_transcript_cleaned_diarised.md",
        "../INT-02_Library/02_transcript_cleaned_diarised.md",
        "../../coding.db",
        "/etc/passwd",
        "..%2F..%2Fcoding.db",
        "sub/../entretien.m4a",
        "missing.m4a",
        "entretien.m4a%00.txt",
    ] {
        let r = raw(&ctx, "GET", &format!("{FILE}?path={bad}"), None).await;
        assert!(
            r.status == StatusCode::BAD_REQUEST || r.status == StatusCode::NOT_FOUND,
            "{bad} gave {}",
            r.status
        );
        assert!(r.body.len() < 400, "{bad} leaked {} bytes", r.body.len());
    }
    // Another interview's id does not give access to this folder's file.
    let r = raw(
        &ctx,
        "GET",
        "/api/interviews/INT-02/audio/file?path=entretien.m4a",
        None,
    )
    .await;
    assert_ne!(r.status, StatusCode::OK);
    let (st, _) = call(&ctx.app, "GET", "/api/interviews/NOPE-1/audio/file", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_leading_out_of_the_folder_is_not_served() {
    let ctx = setup().await;
    let outside = ctx.dir.path().join("outside.mp3");
    std::fs::write(&outside, b"secret").unwrap();
    std::os::unix::fs::symlink(&outside, ctx.root.join("INT-01_Test/link.mp3")).unwrap();
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-01/audio", None).await;
    assert_eq!(v["has_audio"], false);
    let r = raw(&ctx, "GET", &format!("{FILE}?path=link.mp3"), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(!String::from_utf8_lossy(&r.body).contains("secret"));
}

#[tokio::test]
async fn an_interview_folder_outside_the_library_is_refused() {
    let ctx = setup().await;
    put_audio(&ctx);
    // The library folder setting now points elsewhere: the folder is outside it.
    let other = ctx.dir.path().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    let store = rusqlite::Connection::open(&ctx.db).unwrap();
    store
        .execute(
            "UPDATE settings SET value = ?1 WHERE key = 'library_root'",
            [other.display().to_string()],
        )
        .unwrap();
    drop(store);
    let r = raw(&ctx, "GET", &format!("{FILE}?path=entretien.m4a"), None).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_offset_shifts_every_audio_time_and_is_audited() {
    let ctx = setup().await;
    put_audio(&ctx);
    let o = ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/audio/offset",
        Some(json!({"offset_seconds": 2.5, "note": "recording started 2.5 s late"})),
    )
    .await;
    assert_eq!(o["offset_seconds"], 2.5);
    assert_eq!(o["set"], true);
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-01/audio", None).await;
    assert_eq!(v["offset"]["note"], "recording started 2.5 s late");
    let units = v["units"].as_array().unwrap();
    assert_eq!(units[1]["start_seconds"], 5.0);
    assert_eq!(units[1]["audio_seconds"], 7.5);
    assert_eq!(units[1]["end_seconds"], 22.5);
    // A negative offset never gives a time before the start of the recording.
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/audio/offset",
        Some(json!({"offset_seconds": -3.0})),
    )
    .await;
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-01/audio", None).await;
    assert_eq!(v["units"][0]["audio_seconds"], 0.0);
    assert_eq!(v["units"][1]["audio_seconds"], 2.0);
    let audit = ok(&ctx.app, "GET", "/api/audit", None).await;
    let n = audit
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "audio.offset")
        .count();
    assert_eq!(n, 2);
    let (st, bad) = call(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/audio/offset",
        Some(json!({"offset_seconds": 1e9})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}");
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/interviews/NOPE-1/audio/offset",
        Some(json!({"offset_seconds": 1.0})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let _: Value = v;
}

#[tokio::test]
async fn units_without_a_time_are_reported() {
    let ctx = setup().await;
    // A written response has paragraph stamps (P01...) rather than times.
    let dir = ctx.root.join("INT-05_Written");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("01_written_response.md"),
        "---\ninterview_id: INT-05\n---\n\nFirst paragraph of the answer.\n\nSecond paragraph of the answer.\n",
    )
    .unwrap();
    std::fs::write(dir.join("audio.wav"), b"RIFF").unwrap();
    ok(
        &ctx.app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-05/segment",
        Some(json!({})),
    )
    .await;
    let v = ok(&ctx.app, "GET", "/api/interviews/INT-05/audio", None).await;
    assert_eq!(v["has_audio"], true);
    assert_eq!(v["timed_units"], 0);
    assert!(v["units"][0]["audio_seconds"].is_null());
    let warnings = v["warnings"].to_string();
    assert!(warnings.contains("no unit carries a time"), "{warnings}");
}
