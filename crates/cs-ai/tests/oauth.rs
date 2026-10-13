//! Tests of the ChatGPT OAuth flow and of the provider built on it. Offline
//! and deterministic: HTTP is a fake transport that records every request,
//! the identity token is signed with the test-only RSA key of the fixtures,
//! the chat completions API is a loopback server written below.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use cs_ai::providers::oauth_flow::{
    generate_code_challenge, session_key, ChatGptAuth, FileTokenStore, HttpTransport,
    LoopbackCallback, MemoryTokenStore, TokenStore, ISSUER, PLAN_SCOPE, RESOURCE, SCOPE, TOKEN_URL,
};
use cs_ai::providers::{ModelProvider, OAuthChatGptConfig, OAuthChatGptProvider, ProviderRequest};
use serde_json::{json, Value};
use url::Url;

const REDIRECT: &str = "http://127.0.0.1:1455/auth/callback";

// ------------------------------------------------------------ fake transport

type FormPost = (String, Vec<(String, String)>);

#[derive(Clone, Default)]
struct FakeHttp {
    tokens: Arc<Mutex<Value>>,
    forms: Arc<Mutex<Vec<FormPost>>>,
    gets: Arc<Mutex<Vec<String>>>,
    revocations: Arc<Mutex<usize>>,
}

impl FakeHttp {
    fn set_tokens(&self, v: Value) {
        *self.tokens.lock().unwrap() = v;
    }
    fn refresh_forms(&self) -> usize {
        self.forms
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, f)| {
                f.iter()
                    .any(|(k, v)| k == "grant_type" && v == "refresh_token")
            })
            .count()
    }
}

impl HttpTransport for FakeHttp {
    fn get_json(&self, url: &str) -> Result<Value> {
        self.gets.lock().unwrap().push(url.to_string());
        if url.ends_with("openid-configuration") {
            Ok(json!({
                "jwks_uri": "https://auth.openai.com/jwks",
                "revocation_endpoint": "https://auth.openai.com/revoke"
            }))
        } else {
            Ok(serde_json::from_str(include_str!(
                "testdata/chatgpt_test_jwks.json"
            ))?)
        }
    }
    fn post_form(&self, url: &str, form: &[(String, String)]) -> Result<Value> {
        self.forms
            .lock()
            .unwrap()
            .push((url.to_string(), form.to_vec()));
        Ok(self.tokens.lock().unwrap().clone())
    }
    fn post_form_empty(&self, _: &str, _: &[(String, String)]) -> Result<()> {
        *self.revocations.lock().unwrap() += 1;
        Ok(())
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn signed(nonce: &str, client: &str, subject: &str) -> String {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("test-only".into());
    jsonwebtoken::encode(
        &header,
        &json!({
            "sub": subject, "email": "test@example.invalid", "nonce": nonce,
            "aud": client, "iss": ISSUER, "exp": now() + 3600
        }),
        &jsonwebtoken::EncodingKey::from_rsa_der(include_bytes!(
            "testdata/chatgpt_test_only_rsa.der"
        )),
    )
    .unwrap()
}

fn query(url: &str, key: &str) -> String {
    Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("no {key} in {url}"))
        .1
        .into_owned()
}

fn has_query(url: &str, key: &str) -> bool {
    Url::parse(url)
        .unwrap()
        .query_pairs()
        .any(|(k, _)| k == key)
}

fn tokens(identity: String, scope: &str) -> Value {
    json!({
        "access_token": "test-only-access", "refresh_token": "test-only-refresh",
        "id_token": identity, "scope": scope, "expires_in": 3600, "token_type": "Bearer"
    })
}

fn callback(state: &str, client: Option<&str>) -> String {
    let mut url = Url::parse(REDIRECT).unwrap();
    url.query_pairs_mut()
        .extend_pairs([("state", state), ("code", "test-only-code")]);
    if let Some(id) = client {
        url.query_pairs_mut().append_pair("client_id", id);
    }
    url.to_string()
}

fn setup() -> (ChatGptAuth, FakeHttp, MemoryTokenStore) {
    let http = FakeHttp::default();
    let store = MemoryTokenStore::new();
    let auth = ChatGptAuth::new(Box::new(http.clone()), Box::new(store.clone()));
    (auth, http, store)
}

/// Runs a whole sign-in and returns the connected profile id.
fn sign_in(auth: &ChatGptAuth, http: &FakeHttp, client: &str, subject: &str) -> String {
    let a = auth.begin(REDIRECT, None).unwrap();
    http.set_tokens(tokens(
        signed(&query(&a.url, "nonce"), client, subject),
        SCOPE,
    ));
    auth.complete(&a.state, &callback(&a.state, Some(client)))
        .unwrap()
        .id
}

/// Rewrites a field of the stored session of `id`.
fn patch_session(store: &MemoryTokenStore, id: &str, field: &str, value: Value) {
    let mut v: Value = serde_json::from_str(&store.get(&session_key(id)).unwrap()).unwrap();
    v[field] = value;
    store.set(&session_key(id), &v.to_string());
}

// ------------------------------------------------------------------- begin

#[test]
fn begin_builds_a_correct_authorize_url() {
    let (auth, _http, _store) = setup();
    let a = auth.begin(REDIRECT, None).unwrap();
    let url = Url::parse(&a.url).unwrap();
    assert_eq!(url.origin().ascii_serialization(), ISSUER);
    assert_eq!(url.path(), "/api/accounts/authorize");
    assert_eq!(query(&a.url, "client_id"), "dynamic_agent_client");
    assert_eq!(query(&a.url, "response_type"), "code");
    assert_eq!(query(&a.url, "redirect_uri"), REDIRECT);
    assert_eq!(
        query(&a.url, "scope"),
        "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"
    );
    assert_eq!(query(&a.url, "scope"), SCOPE);
    assert_eq!(query(&a.url, "resource"), RESOURCE);
    assert_eq!(query(&a.url, "code_challenge_method"), "S256");
    assert_eq!(query(&a.url, "code_challenge").len(), 43);
    assert_eq!(query(&a.url, "state"), a.state);
    assert!(!query(&a.url, "nonce").is_empty());
    assert_ne!(query(&a.url, "nonce"), a.state);
    assert!(query(&a.url, "ext_agent_host_id").starts_with("urn:uuid:"));
    assert_eq!(query(&a.url, "agent_name_hint"), "Coding Studio");
    assert!(!has_query(&a.url, "id_token_hint"));
}

#[test]
fn begin_refuses_a_callback_address_that_is_not_loopback() {
    let (auth, _http, _store) = setup();
    for bad in [
        "https://example.com/auth/callback",
        "http://localhost:1455/auth/callback",
        "http://127.0.0.1:1455/other",
        "http://127.0.0.1/auth/callback",
        "http://127.0.0.1:1455/auth/callback?x=1",
        "http://user@127.0.0.1:1455/auth/callback",
        "not a url",
    ] {
        assert!(auth.begin(bad, None).is_err(), "{bad} must be refused");
    }
    assert!(auth
        .begin("http://127.0.0.1:1455/auth/callback", Some("unknown"))
        .is_err());
}

#[test]
fn registration_is_performed_once_stored_and_reused() {
    let (auth, http, store) = setup();
    let first = auth.begin(REDIRECT, None).unwrap();
    http.set_tokens(tokens(
        signed(&query(&first.url, "nonce"), "oaiapp_test", "sub-one"),
        SCOPE,
    ));
    let profile = auth
        .complete(&first.state, &callback(&first.state, Some("oaiapp_test")))
        .unwrap();
    assert!(profile.connected);
    assert_eq!(profile.client_id, "oaiapp_test");
    assert_eq!(profile.subject, "sub-one");
    assert_eq!(profile.email, "test@example.invalid");
    assert!(store
        .get("ai:openai:registrations")
        .unwrap()
        .contains("oaiapp_test"));

    let second = auth.begin(REDIRECT, Some(&profile.id)).unwrap();
    assert_eq!(query(&second.url, "client_id"), "oaiapp_test");
    assert_eq!(
        query(&first.url, "ext_agent_host_id"),
        query(&second.url, "ext_agent_host_id")
    );
    assert!(!has_query(&second.url, "agent_name_hint"));
    assert!(has_query(&second.url, "id_token_hint"));
    assert_eq!(query(&second.url, "login_hint"), "test@example.invalid");
    assert_eq!(auth.profiles().unwrap().len(), 1);
}

// ---------------------------------------------------------------- complete

#[test]
fn complete_rejects_a_wrong_state_then_exchanges_with_the_verifier() {
    let (auth, http, store) = setup();
    let a = auth.begin(REDIRECT, None).unwrap();
    http.set_tokens(tokens(
        signed(&query(&a.url, "nonce"), "oaiapp_test", "sub-one"),
        SCOPE,
    ));

    // A wrong expected state, or a callback carrying another state, fails
    // before any network call.
    assert!(auth
        .complete("wrong-state", &callback(&a.state, Some("oaiapp_test")))
        .is_err());
    assert!(auth
        .complete(&a.state, &callback("other", Some("oaiapp_test")))
        .is_err());
    assert!(http.forms.lock().unwrap().is_empty());

    let profile = auth
        .complete(&a.state, &callback(&a.state, Some("oaiapp_test")))
        .unwrap();

    let forms = http.forms.lock().unwrap();
    assert_eq!(forms.len(), 1);
    let (url, form) = &forms[0];
    assert_eq!(url, TOKEN_URL);
    let field = |k: &str| {
        form.iter()
            .find(|(n, _)| n == k)
            .unwrap_or_else(|| panic!("no {k}"))
            .1
            .clone()
    };
    assert_eq!(field("grant_type"), "authorization_code");
    assert_eq!(field("client_id"), "oaiapp_test");
    assert_eq!(field("code"), "test-only-code");
    assert_eq!(field("redirect_uri"), REDIRECT);
    assert_eq!(field("resource"), RESOURCE);
    // The verifier sent matches the challenge of the authorize URL (S256).
    assert_eq!(
        generate_code_challenge(&field("code_verifier")),
        query(&a.url, "code_challenge")
    );
    drop(forms);

    // The identity was checked against the JWKS and the session stored.
    assert!(http
        .gets
        .lock()
        .unwrap()
        .iter()
        .any(|u| u == "https://auth.openai.com/jwks"));
    let session: Value =
        serde_json::from_str(&store.get(&session_key(&profile.id)).unwrap()).unwrap();
    assert_eq!(session["access_token"], "test-only-access");
    assert_eq!(session["refresh_token"], "test-only-refresh");
    assert!(session["scope"].as_str().unwrap().contains(PLAN_SCOPE));
    assert!(session["expires_at"].as_i64().unwrap() > now() + 3000);

    // The attempt is consumed: replaying the callback fails.
    assert!(auth
        .complete(&a.state, &callback(&a.state, Some("oaiapp_test")))
        .is_err());
}

#[test]
fn complete_rejects_wrong_nonce_audience_signature_and_missing_plan_scope() {
    for failure in ["nonce", "audience", "scope", "key"] {
        let (auth, http, store) = setup();
        let a = auth.begin(REDIRECT, None).unwrap();
        let nonce = if failure == "nonce" {
            "wrong".to_string()
        } else {
            query(&a.url, "nonce")
        };
        let audience = if failure == "audience" {
            "wrong"
        } else {
            "oaiapp_test"
        };
        let mut id_token = signed(&nonce, audience, "sub-one");
        if failure == "key" {
            // Corrupt the signature.
            id_token.truncate(id_token.len() - 6);
            id_token.push_str("AAAAAA");
        }
        http.set_tokens(tokens(
            id_token,
            if failure == "scope" {
                "openid email"
            } else {
                SCOPE
            },
        ));
        let err = auth
            .complete(&a.state, &callback(&a.state, Some("oaiapp_test")))
            .unwrap_err()
            .to_string();
        if failure == "scope" {
            assert!(err.contains("plan usage"), "{err}");
        }
        assert!(
            auth.profiles().unwrap().iter().all(|p| !p.connected),
            "{failure}"
        );
        assert!(
            !store
                .get("ai:openai:registrations")
                .unwrap()
                .contains("\"connected\":true"),
            "{failure}"
        );
    }
}

#[test]
fn complete_refuses_a_declined_or_codeless_callback_and_keeps_the_registration() {
    let (auth, http, _store) = setup();
    let a = auth.begin(REDIRECT, None).unwrap();
    let declined = format!(
        "{}&error=access_denied",
        callback(&a.state, Some("oaiapp_retry"))
    );
    assert!(auth.complete(&a.state, &declined).is_err());

    // A failed exchange (no token JSON) still keeps the issued client.
    let a = auth.begin(REDIRECT, None).unwrap();
    http.set_tokens(Value::Null);
    assert!(auth
        .complete(&a.state, &callback(&a.state, Some("oaiapp_retry")))
        .is_err());
    let saved = auth.profiles().unwrap();
    assert_eq!(saved.len(), 1);
    assert!(!saved[0].connected);
    let retry = auth.begin(REDIRECT, Some(&saved[0].id)).unwrap();
    assert_eq!(query(&retry.url, "client_id"), "oaiapp_retry");
    assert!(auth.access_token(&saved[0].id).is_err());
}

#[test]
fn cancel_forgets_the_attempt() {
    let (auth, _http, _store) = setup();
    let a = auth.begin(REDIRECT, None).unwrap();
    auth.cancel(&a.state);
    assert!(auth
        .complete(&a.state, &callback(&a.state, Some("oaiapp_test")))
        .is_err());
}

// ------------------------------------------------------------ access token

#[test]
fn access_token_returns_the_stored_token_and_refreshes_when_expired() {
    let (auth, http, store) = setup();
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    assert_eq!(auth.access_token(&id).unwrap(), "test-only-access");
    assert_eq!(http.refresh_forms(), 0, "a valid token is not refreshed");

    patch_session(&store, &id, "expires_at", json!(0));
    http.set_tokens(json!({
        "access_token": "rotated-access", "refresh_token": "rotated-refresh",
        "scope": SCOPE, "expires_in": 3600, "token_type": "Bearer"
    }));
    assert_eq!(auth.access_token(&id).unwrap(), "rotated-access");
    assert_eq!(http.refresh_forms(), 1);
    {
        let forms = http.forms.lock().unwrap();
        let (url, form) = forms.last().unwrap();
        assert_eq!(url, TOKEN_URL);
        assert!(form.contains(&("refresh_token".into(), "test-only-refresh".into())));
        assert!(form.contains(&("client_id".into(), "oaiapp_test".into())));
    }
    let session: Value = serde_json::from_str(&store.get(&session_key(&id)).unwrap()).unwrap();
    assert_eq!(session["refresh_token"], "rotated-refresh");
    assert!(session["expires_at"].as_i64().unwrap() > now() + 3000);

    // Now fresh again: no second refresh.
    assert_eq!(auth.access_token(&id).unwrap(), "rotated-access");
    assert_eq!(http.refresh_forms(), 1);
}

#[test]
fn refresh_that_drops_the_plan_scope_is_an_error() {
    let (auth, http, store) = setup();
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    patch_session(&store, &id, "expires_at", json!(0));
    http.set_tokens(json!({
        "access_token": "x", "scope": "openid email", "expires_in": 3600, "token_type": "Bearer"
    }));
    assert!(auth.access_token(&id).is_err());
    let session: Value = serde_json::from_str(&store.get(&session_key(&id)).unwrap()).unwrap();
    assert_eq!(
        session["access_token"], "test-only-access",
        "nothing overwritten"
    );
}

#[test]
fn accounts_are_kept_separate() {
    let (auth, http, store) = setup();
    let a = sign_in(&auth, &http, "oaiapp_0", "sub-0");
    let b = sign_in(&auth, &http, "oaiapp_1", "sub-1");
    assert_ne!(a, b);
    assert_eq!(auth.profiles().unwrap().len(), 2);
    patch_session(&store, &a, "expires_at", json!(0));
    http.set_tokens(json!({
        "access_token": "rotated", "refresh_token": "new-refresh",
        "expires_in": 3600, "token_type": "Bearer"
    }));
    assert_eq!(auth.access_token(&a).unwrap(), "rotated");
    let other: Value = serde_json::from_str(&store.get(&session_key(&b)).unwrap()).unwrap();
    assert_eq!(other["refresh_token"], "test-only-refresh");
}

#[test]
fn disconnect_revokes_and_removes_the_session_but_keeps_the_registration() {
    let (auth, http, store) = setup();
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    assert!(auth.disconnect(&id).unwrap());
    assert_eq!(*http.revocations.lock().unwrap(), 1);
    assert!(store.get(&session_key(&id)).is_none());
    assert!(auth.access_token(&id).is_err());
    let kept = auth.profiles().unwrap();
    assert_eq!(kept[0].client_id, "oaiapp_test");
    assert!(!kept[0].connected);
    assert!(auth.disconnect("unknown").is_err());
}

// ------------------------------------------------------------------- stores

#[test]
fn file_store_round_trips_and_writes_0600() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("tokens");
    let store = FileTokenStore::new(&nested);
    assert_eq!(store.get("k"), None);
    store.set("k", "v1");
    store.set("other", "v2");
    store.set("k", "v3");
    assert_eq!(store.get("k").as_deref(), Some("v3"));
    assert_eq!(store.get("other").as_deref(), Some("v2"));
    assert!(store.last_error().is_none());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let dir_mode = std::fs::metadata(&nested).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
    }

    // A new instance over the same directory sees the data (JSON on disk).
    let again = FileTokenStore::new(&nested);
    assert_eq!(again.get("k").as_deref(), Some("v3"));
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(again.path()).unwrap()).unwrap();
    assert_eq!(raw["other"], "v2");
    again.remove("k");
    assert_eq!(FileTokenStore::new(&nested).get("k"), None);
    assert_eq!(
        FileTokenStore::new(&nested).get("other").as_deref(),
        Some("v2")
    );
}

#[test]
fn a_whole_login_survives_in_the_file_store() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    let auth = ChatGptAuth::new(
        Box::new(http.clone()),
        Box::new(FileTokenStore::new(dir.path())),
    );
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    let reopened = ChatGptAuth::new(
        Box::new(http.clone()),
        Box::new(FileTokenStore::new(dir.path())),
    );
    assert_eq!(reopened.access_token(&id).unwrap(), "test-only-access");
}

// ----------------------------------------------------------------- loopback

fn http_get(port: u16, target: &str, host: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes())
        .unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out
}

#[test]
fn loopback_returns_the_full_callback_url_and_ignores_strays() {
    let (redirect, handle) = LoopbackCallback::listen(None).unwrap();
    let port = handle.port();
    assert_eq!(redirect, format!("http://127.0.0.1:{port}/auth/callback"));
    let waiter = thread::spawn(move || handle.wait_for_state("S1", Duration::from_secs(10)));

    assert!(
        http_get(port, "/favicon.ico", &format!("127.0.0.1:{port}")).starts_with("HTTP/1.1 400")
    );
    assert!(
        http_get(port, "/auth/callback?state=S1&code=c", "evil.example")
            .starts_with("HTTP/1.1 400")
    );
    assert!(http_get(
        port,
        "/auth/callback?state=other&code=c",
        &format!("127.0.0.1:{port}")
    )
    .starts_with("HTTP/1.1 400"));
    let ok = http_get(
        port,
        "/auth/callback?state=S1&code=c%20d",
        &format!("127.0.0.1:{port}"),
    );
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");

    let url = waiter.join().unwrap().unwrap();
    assert_eq!(
        url,
        format!("http://127.0.0.1:{port}/auth/callback?state=S1&code=c%20d")
    );
    // The redirect built by listen() is accepted by begin().
    let (auth, _h, _s) = setup();
    auth.begin(&redirect, None).unwrap();
}

#[test]
fn loopback_times_out_when_nothing_arrives() {
    let (_redirect, handle) = LoopbackCallback::listen(None).unwrap();
    let started = std::time::Instant::now();
    let err = handle
        .wait(Duration::from_millis(200))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Timed out"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

// ----------------------------------------------------------------- provider

struct Captured {
    request_line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// Answers one request with (status, body) and records it.
fn serve_once(status: u16, reply_body: String) -> (String, Arc<Mutex<Vec<Captured>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let end = loop {
            let n = stream.read(&mut chunk).unwrap_or(0);
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break p + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..end]).into_owned();
        let mut lines = head.split("\r\n");
        let request_line = lines.next().unwrap_or_default().to_string();
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        let len: usize = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = buf[end..].to_vec();
        while body.len() < len {
            let n = stream.read(&mut chunk).unwrap_or(0);
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        sink.lock().unwrap().push(Captured {
            request_line,
            headers,
            body,
        });
        let reply = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply_body}",
            reply_body.len()
        );
        let _ = stream.write_all(reply.as_bytes());
    });
    (format!("http://127.0.0.1:{port}/v1"), captured)
}

fn completion(content: &str, model: &str) -> String {
    json!({
        "id": "chatcmpl-test", "model": model,
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": content }, "finish_reason": "stop" }]
    })
    .to_string()
}

fn cfg(profile: Option<&str>) -> OAuthChatGptConfig {
    OAuthChatGptConfig {
        id: "chatgpt".into(),
        profile_id: profile.map(String::from),
        model: Some("gpt-test".into()),
        token_dir: None,
    }
}

#[test]
fn provider_sends_the_bearer_and_returns_the_content_verbatim() {
    let (auth, http, _store) = setup();
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    let content = "  {\"a\": 1}\n\nÉcriture \"quotée\" \\ 日本語  ";
    let (base, captured) = serve_once(200, completion(content, "gpt-served"));
    let p = OAuthChatGptProvider::with_auth(cfg(Some(&id)), Arc::new(auth), base);

    let res = p
        .complete(&ProviderRequest {
            prompt: "hello".into(),
            temperature: Some(0.0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(res.raw, content);
    assert_eq!(res.model, "gpt-served");
    assert_eq!(res.provider, "chatgpt");
    assert_eq!(res.parameters_exposed, vec!["temperature".to_string()]);
    assert_eq!(
        res.parameters_not_exposed,
        vec!["seed".to_string(), "max_tokens".to_string()]
    );
    assert_eq!(res.request_sha256, cs_model::sha256_hex(b"hello"));

    let cap = captured.lock().unwrap();
    assert_eq!(cap[0].request_line, "POST /v1/chat/completions HTTP/1.1");
    let auth_header = cap[0]
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.as_str());
    assert_eq!(auth_header, Some("Bearer test-only-access"));
    let body: Value = serde_json::from_slice(&cap[0].body).unwrap();
    assert_eq!(body["model"], "gpt-test");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "hello");
    assert_eq!(body["temperature"], 0.0);
    assert!(body.get("seed").is_none() && body.get("max_tokens").is_none());
}

#[test]
fn provider_uses_the_first_connected_profile_and_a_per_call_model() {
    let (auth, http, _store) = setup();
    sign_in(&auth, &http, "oaiapp_test", "sub-one");
    let (base, captured) = serve_once(200, completion("ok", "m"));
    let p = OAuthChatGptProvider::with_auth(cfg(None), Arc::new(auth), base);
    p.complete(&ProviderRequest {
        prompt: "p".into(),
        model: Some("override".into()),
        ..Default::default()
    })
    .unwrap();
    let body: Value = serde_json::from_slice(&captured.lock().unwrap()[0].body).unwrap();
    assert_eq!(body["model"], "override");
}

#[test]
fn provider_without_a_connected_profile_is_not_configured_and_sends_nothing() {
    let (auth, http, _store) = setup();
    let (base, captured) = serve_once(200, completion("ok", "m"));
    let auth = Arc::new(auth);
    let p = OAuthChatGptProvider::with_auth(cfg(None), auth.clone(), base.clone());
    let err = p
        .complete(&ProviderRequest::default())
        .unwrap_err()
        .to_string();
    assert!(err.to_lowercase().contains("not configured"), "{err}");

    // A profile that was disconnected is not usable either.
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    auth.disconnect(&id).unwrap();
    let p = OAuthChatGptProvider::with_auth(cfg(Some(&id)), auth, base);
    let err = p
        .complete(&ProviderRequest::default())
        .unwrap_err()
        .to_string();
    assert!(err.to_lowercase().contains("not configured"), "{err}");

    // Without any token directory the provider has no auth at all.
    let bare = OAuthChatGptProvider::new(cfg(None));
    assert!(bare
        .complete(&ProviderRequest::default())
        .unwrap_err()
        .to_string()
        .to_lowercase()
        .contains("not configured"));
    assert!(captured.lock().unwrap().is_empty());
}

#[test]
fn provider_errors_never_carry_the_token() {
    let (auth, http, _store) = setup();
    let id = sign_in(&auth, &http, "oaiapp_test", "sub-one");
    let (base, _c) = serve_once(401, json!({"error": {"message": "nope"}}).to_string());
    let p = OAuthChatGptProvider::with_auth(cfg(Some(&id)), Arc::new(auth), base);
    let err = p
        .complete(&ProviderRequest {
            prompt: "p".into(),
            ..Default::default()
        })
        .unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("401"), "{text}");
    assert!(!text.contains("test-only-access"));
}

#[test]
fn provider_is_described_as_hosted_openai() {
    let p = OAuthChatGptProvider::new(cfg(None));
    let d = p.describe();
    assert!(d.sends.iter().any(|s| s == "prompt"));
    assert!(d.sends.iter().any(|s| s == "model name"));
    assert_eq!(
        d.execution,
        cs_ai::providers::Execution::Hosted {
            vendor: "openai".into(),
            endpoint: "https://api.openai.com/v1".into()
        }
    );
}
