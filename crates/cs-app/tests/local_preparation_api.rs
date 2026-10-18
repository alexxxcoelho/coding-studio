mod common;
use common::{call, ok, setup};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
fn fake_model(bad: bool) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(2) {
            let mut s = stream.unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buf = [0; 4096];
            let (head, len) = loop {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    return;
                }
                raw.extend_from_slice(&buf[..n]);
                if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    let h = String::from_utf8_lossy(&raw[..i]);
                    let len = h
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    break (i + 4, len);
                }
            };
            while raw.len() < head + len {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    return;
                }
                raw.extend_from_slice(&buf[..n]);
            }
            counter.fetch_add(1, Ordering::SeqCst);
            let request: Value = serde_json::from_slice(&raw[head..head + len]).unwrap();
            let input: Value =
                serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            let output = if input.get("existing_replacements").is_some() {
                json!({"entities":[{"original":"Sophie","same_as":""}]})
            } else {
                let mut units:Vec<_>=input["units"].as_array().unwrap().iter().map(|u|json!({"unit_id":u["unit_id"],"text":u["text"].as_str().unwrap().replace("euh, ","")})).collect();
                if bad {
                    units.pop();
                }
                json!({"units":units})
            };
            let bytes=serde_json::to_vec(&json!({"choices":[{"finish_reason":"stop","message":{"content":serde_json::to_string(&output).unwrap()}}]})).unwrap();
            write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
            s.write_all(&bytes).unwrap();
        }
    });
    (format!("http://127.0.0.1:{port}"), calls)
}
async fn fixture() -> common::Ctx {
    let c = setup().await;
    ok(&c.app,"POST","/api/interviews/INT-01/units/INT-01-U002/corrections",Some(json!({"text":"euh, Sophie checks 3 stores.","role":"respondent","reason":"Synthetic fixture"}))).await;
    c
}
#[tokio::test]
async fn local_draft_is_separate_then_approval_commits_without_overwriting_original() {
    let c = fixture().await;
    let before = ok(&c.app, "GET", "/api/interviews/INT-01", None).await;
    let (url, calls) = fake_model(false);
    let opts = json!({"url":url,"model":"fictional-model","anonymise":true,"cleanup":true,"target_language":""});
    let d = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(opts),
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(d["reviewed"], false);
    assert!(d["draft_digest"].is_string());
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before,
        "draft must not change active transcript"
    );
    assert_eq!(
        d["original"]["units"][1]["text"],
        "euh, Sophie checks 3 stores."
    );
    assert_eq!(
        d["anonymised"]["units"][1]["text"],
        "euh, [Person 1] checks 3 stores."
    );
    assert_eq!(
        d["prepared"]["units"][1]["text"],
        "[Person 1] checks 3 stores."
    );
    let path = "/api/interviews/INT-01/local-preparation/apply";
    assert_ne!(
        call(
            &c.app,
            "POST",
            path,
            Some(json!({"digest":d["draft_digest"],"reviewed":false,"use_prepared":true}))
        )
        .await
        .0,
        200
    );
    ok(
        &c.app,
        "POST",
        path,
        Some(json!({"digest":d["draft_digest"],"reviewed":true,"use_prepared":true})),
    )
    .await;
    let p = ok(&c.app, "GET", "/api/anonymisation/interviews/INT-01", None).await;
    assert_eq!(
        p["units"][1]["original"], "[Person 1] checks 3 stores.",
        "old correction must not undo prepared text"
    );
    let db = rusqlite::Connection::open(&c.db).unwrap();
    let retained: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM unit_corrections WHERE text='euh, Sophie checks 3 stores.'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, 1);
    let audit = ok(&c.app, "GET", "/api/audit", None).await;
    for e in audit.as_array().unwrap().iter().filter(|e| {
        e["target"]
            .as_str()
            .unwrap_or("")
            .starts_with("local_preparation:")
            || e["action"] == "transcript.preparation"
    }) {
        assert!(!e.to_string().contains("Sophie"));
    }
    assert_eq!(
        ok(&c.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );
}
#[tokio::test]
async fn malformed_output_leaves_no_draft_or_active_change() {
    let c = fixture().await;
    let before = ok(&c.app, "GET", "/api/interviews/INT-01", None).await;
    let (url, _) = fake_model(true);
    let (status, _) = call(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(json!({"url":url,"model":"fake","anonymise":true,"cleanup":true})),
    )
    .await;
    assert_ne!(status, 200);
    assert!(ok(
        &c.app,
        "GET",
        "/api/interviews/INT-01/local-preparation",
        None
    )
    .await
    .is_null());
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before
    );
}
#[tokio::test]
async fn hosted_destinations_and_coded_interviews_are_refused_before_any_model_call() {
    let c = fixture().await;
    let body = json!({"url":"https://api.openai.com/v1","model":"fake","anonymise":true,"cleanup":false,"target_language":""});
    assert_eq!(
        call(
            &c.app,
            "POST",
            "/api/interviews/INT-01/local-preparation",
            Some(body)
        )
        .await
        .0,
        400
    );
    let code = common::code_id(&c.app, "TECH-ADV").await;
    ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id":"INT-01-U002","start":0,"end":3,"code":code})),
    )
    .await;
    let body = json!({"url":"http://127.0.0.1:1","model":"fake","anonymise":true,"cleanup":false,"target_language":""});
    assert_eq!(
        call(
            &c.app,
            "POST",
            "/api/interviews/INT-01/local-preparation",
            Some(body)
        )
        .await
        .0,
        409
    );
}
