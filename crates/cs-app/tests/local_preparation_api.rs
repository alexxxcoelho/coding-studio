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
    fake_model_auth(bad, None)
}
fn fake_model_auth(bad: bool, token: Option<String>) -> (String, Arc<AtomicUsize>) {
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
            if let Some(token) = &token {
                assert!(String::from_utf8_lossy(&raw[..head])
                    .to_lowercase()
                    .contains(&format!("authorization: bearer {}", token.to_lowercase())));
            }
            counter.fetch_add(1, Ordering::SeqCst);
            let request: Value = serde_json::from_slice(&raw[head..head + len]).unwrap();
            let native = request.get("reasoning").is_some();
            let input: Value = serde_json::from_str(if native {
                request["input"].as_str().unwrap()
            } else {
                request["messages"][1]["content"].as_str().unwrap()
            })
            .unwrap();
            if native {
                assert_eq!(request["store"], false);
                assert_eq!(request["integrations"], json!([]));
                assert_eq!(request["reasoning"], "off");
                assert_eq!(request["context_length"], 131072);
            }

            let output = if input.get("existing_replacements").is_some() {
                json!({"entities":[{"original":if input["units"].to_string().contains("Sophie"){"Sophie"}else{"Paul"},"same_as":""}]})
            } else {
                let mut units:Vec<_>=input["units"].as_array().unwrap().iter().map(|u|json!({"unit_id":u["unit_id"],"text":u["text"].as_str().unwrap().replace("euh, ","")})).collect();
                if bad {
                    units.pop();
                }
                json!({"units":units})
            };
            let response = if native {
                json!({"output":[{"type":"message","content":serde_json::to_string(&output).unwrap()}],"stats":{"total_output_tokens":50,"reasoning_output_tokens":0}})
            } else {
                json!({"choices":[{"finish_reason":"stop","message":{"content":serde_json::to_string(&output).unwrap()}}]})
            };
            let bytes = serde_json::to_vec(&response).unwrap();
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
async fn hosted_destinations_are_refused_and_coded_interviews_keep_their_anchors() {
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
    let before = ok(&c.app, "GET", "/api/interviews/INT-01", None).await;
    let passages = ok(&c.app, "GET", "/api/interviews/INT-01/passages", None).await;
    let (url, calls) = fake_model(false);
    let d = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(
            json!({"url":url,"model":"fake","anonymise":true,"cleanup":true,"target_language":""}),
        ),
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(d["coding_will_be_preserved"], true);
    let answer = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation/apply",
        Some(json!({"digest":d["draft_digest"],"reviewed":true,"use_prepared":true})),
    )
    .await;
    assert_eq!(answer["coding_preserved"], true);
    assert_eq!(answer["applied_to_coding"], false);
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before
    );
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01/passages", None).await,
        passages
    );
    let private = ok(
        &c.app,
        "GET",
        "/api/interviews/INT-01/local-preparation",
        None,
    )
    .await;
    assert_eq!(private["reviewed"], true);
    assert_eq!(private["applied_to_coding"], false);
    let catalog = ok(
        &c.app,
        "GET",
        "/api/export/reviewer/catalogue?pseudonymise=false",
        None,
    )
    .await;
    assert!(catalog["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["path"][0] == "Original transcript"));
    assert!(catalog["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["path"][0] == "Prepared transcript"));
    assert_eq!(
        ok(&c.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );
}

#[tokio::test]
async fn native_lm_studio_thinking_off_prepares_a_private_draft_without_applying_it() {
    let c = fixture().await;
    let before = ok(&c.app, "GET", "/api/interviews/INT-01", None).await;
    let (url, calls) = fake_model(false);
    let d=ok(&c.app,"POST","/api/interviews/INT-01/local-preparation",Some(json!({"url":url,"model":"fictional-model","anonymise":true,"cleanup":true,"target_language":"","context_tokens":131072,"reasoning":"off"}))).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(d["reviewed"], false);
    assert_eq!(d["options"]["reasoning"], "off");
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before
    );
}

#[tokio::test]
async fn staged_flow_refuses_transform_before_review_and_keeps_the_original_until_final_acceptance()
{
    let c = fixture().await;
    let before = ok(&c.app, "GET", "/api/interviews/INT-01", None).await;
    let (url, calls) = fake_model(false);
    let opts = json!({"url":url,"model":"fictional-model","anonymise":true,"cleanup":true,"target_language":"","stage":"anonymisation"});
    let first = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(opts.clone()),
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first["stage"], "anonymisation");
    let mut transform = opts.clone();
    transform["stage"] = json!("transformation");
    transform["draft_digest"] = first["draft_digest"].clone();
    assert_eq!(
        call(
            &c.app,
            "POST",
            "/api/interviews/INT-01/local-preparation",
            Some(transform.clone())
        )
        .await
        .0,
        409
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let units = first["anonymised"]["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| json!({"unit_id":u["unit_id"],"text":u["text"],"speaker_raw":u["speaker_raw"]}))
        .collect::<Vec<_>>();
    let body =
        json!({"digest":first["draft_digest"],"reviewed":true,"pending_diffs":1,"units":units});
    assert_eq!(
        call(
            &c.app,
            "POST",
            "/api/interviews/INT-01/local-preparation/review",
            Some(body.clone())
        )
        .await
        .0,
        400
    );
    let mut reviewed = body;
    reviewed["pending_diffs"] = json!(0);
    let approved = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation/review",
        Some(reviewed),
    )
    .await;
    assert_eq!(approved["anonymisation_approved"], true);
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before
    );
    transform["draft_digest"] = approved["draft_digest"].clone();
    let second = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(transform),
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        call(
            &c.app,
            "POST",
            "/api/interviews/INT-01/local-preparation/apply",
            Some(json!({"digest":second["draft_digest"],"reviewed":true,"use_prepared":true}))
        )
        .await
        .0,
        409
    );
    let reviewed=ok(&c.app,"POST","/api/interviews/INT-01/local-preparation/review",Some(json!({"digest":second["draft_digest"],"reviewed":true,"pending_diffs":0,"units":second["prepared"]["units"]}))).await;
    ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation/apply",
        Some(json!({"digest":reviewed["draft_digest"],"reviewed":true,"use_prepared":true})),
    )
    .await;
    assert_ne!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before
    );
}
#[tokio::test]
async fn feedback_revises_a_stage_without_applying_it() {
    let c = fixture().await;
    let before = ok(&c.app, "GET", "/api/interviews/INT-01", None).await;
    let (url, calls) = fake_model(false);
    let mut opts = json!({"url":url,"model":"fictional-model","anonymise":true,"cleanup":true,"target_language":"","stage":"anonymisation"});
    let first = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(opts.clone()),
    )
    .await;
    opts["draft_digest"] = first["draft_digest"].clone();
    opts["feedback"] = json!([{"unit_id":"INT-01-U002","instruction":"Vérifier le prénom Sophie dans ce segment."}]);
    let revised = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(opts),
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(revised["anonymisation_approved"], false);
    assert_eq!(revised["feedback"][0]["unit_id"], "INT-01-U002");
    assert_eq!(
        ok(&c.app, "GET", "/api/interviews/INT-01", None).await,
        before
    );
}

#[tokio::test]
async fn batch_drafts_have_distinct_labels_and_remain_reviewable_after_another_is_applied() {
    let c = fixture().await;
    ok(&c.app,"POST","/api/interviews/INT-02/units/INT-02-U002/corrections",Some(json!({"text":"Paul checks 3 stores.","role":"respondent","reason":"Synthetic batch fixture"}))).await;
    let (url, _) = fake_model(false);
    let opts = json!({"url":url,"model":"fictional-model","anonymise":true,"cleanup":false,"target_language":"","stage":"anonymisation"});
    let a = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(opts.clone()),
    )
    .await;
    let mut second = opts.clone();
    second["batch_entries"] = a["entries"].clone();
    let b = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-02/local-preparation",
        Some(second),
    )
    .await;
    assert_eq!(
        a["anonymised"]["units"][1]["text"],
        "euh, [Person 1] checks 3 stores."
    );
    assert_eq!(
        b["anonymised"]["units"][1]["text"],
        "[Person 2] checks 3 stores."
    );
    for (id, d) in [("INT-01", a), ("INT-02", b)] {
        let path = format!("/api/interviews/{id}/local-preparation");
        let approved=ok(&c.app,"POST",&format!("{path}/review"),Some(json!({"digest":d["draft_digest"],"reviewed":true,"pending_diffs":0,"units":d["anonymised"]["units"]}))).await;
        ok(
            &c.app,
            "POST",
            &format!("{path}/apply"),
            Some(json!({"digest":approved["draft_digest"],"reviewed":true,"use_prepared":false})),
        )
        .await;
    }
}
#[tokio::test]
async fn bearer_is_sent_to_the_model_and_never_persisted_in_settings_drafts_or_audit() {
    let c = fixture().await;
    let token = "fictional-test-secret-123";
    let (url, _) = fake_model_auth(false, Some(token.into()));
    let opts = json!({"url":url,"model":"fictional-model","anonymise":true,"cleanup":false,"target_language":"","bearer_token":format!("Bearer {token}")});
    let saved = ok(
        &c.app,
        "PUT",
        "/api/local-preparation/config",
        Some(opts.clone()),
    )
    .await;
    assert!(!saved.to_string().contains(token));
    let mut without = opts;
    without.as_object_mut().unwrap().remove("bearer_token");
    let d = ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/local-preparation",
        Some(without),
    )
    .await;
    assert!(!d.to_string().contains(token));
    let config = ok(&c.app, "GET", "/api/local-preparation/config", None).await;
    assert_eq!(config["has_bearer_token"], true);
    assert!(!config.to_string().contains(token));
    let conn = rusqlite::Connection::open(&c.db).unwrap();
    for table in ["settings", "audit_log"] {
        let mut stmt = conn.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let count = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..count {
                if let Ok(text) = row.get::<_, String>(i) {
                    assert!(!text.contains(token));
                }
            }
        }
    }
}
#[tokio::test]
async fn model_discovery_sends_the_supplied_bearer_header() {
    let c = fixture().await;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut raw = Vec::new();
        let mut buf = [0; 1024];
        while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buf[..n]);
        }
        let headers = String::from_utf8(raw).unwrap().to_lowercase();
        assert!(headers.starts_with("get /v1/models "));
        assert!(headers.contains("authorization: bearer synthetic-model-discovery"));
        let body = r#"{"data":[{"id":"fictional-model"}]}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let result=ok(&c.app,"POST","/api/local-preparation/models",Some(json!({"url":format!("http://127.0.0.1:{port}"),"bearer_token":"Bearer synthetic-model-discovery"}))).await;
    assert_eq!(result["models"], json!(["fictional-model"]));
    server.join().unwrap();
    let (status, _) = call(
        &c.app,
        "POST",
        "/api/local-preparation/models",
        Some(json!({"url":format!("http://127.0.0.1:{port}"),"bearer_token":"bad\r\nheader"})),
    )
    .await;
    assert_ne!(status, 200);
}
