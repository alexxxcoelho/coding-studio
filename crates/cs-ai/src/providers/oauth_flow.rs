//! Native ChatGPT plan OAuth, ported from Munrel (`munrel-connectors`,
//! `providers/chatgpt.rs`) to synchronous Rust. The caller owns the browser
//! and the loopback listener (see [`LoopbackCallback`]); this service owns
//! PKCE, the verified identity, the issued registrations and the renewable
//! tokens. HTTP and storage are injectable ([`HttpTransport`], [`TokenStore`]).
//!
//! Flow: `begin` -> open `Authorization::url` in a browser -> the loopback
//! listener receives the callback -> `complete(state, callback_url)` ->
//! `access_token(profile)` -> bearer for `https://api.openai.com/v1`.
//!
//! Deliberately no `Debug` on anything carrying a credential or an
//! authorization URL.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, DecodingKey, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use url::Url;

pub const RESOURCE: &str = "https://api.openai.com/v1";
pub const ISSUER: &str = "https://auth.openai.com";
pub const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const INDEX: &str = "ai:openai:registrations";
const HOST: &str = "ai:openai:host";
pub const SCOPE: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
/// Placeholder client id of the first sign-in; the real client id is issued
/// by OpenAI and comes back in the callback query.
pub const DYNAMIC_CLIENT_ID: &str = "dynamic_agent_client";
const CALLBACK_PATH: &str = "/auth/callback";
const PENDING_TTL: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------- transport

/// Minimal JSON-over-HTTP surface the flow needs. Synchronous; production is
/// [`UreqTransport`], tests inject a scripted one.
pub trait HttpTransport: Send + Sync {
    /// GET `url`, parse the body as JSON.
    fn get_json(&self, url: &str) -> Result<Value>;
    /// GET `url` with an `Authorization: Bearer` header, parse the body as JSON.
    fn get_json_bearer(&self, _url: &str, _token: &str) -> Result<Value> {
        bail!("Bearer GET is not supported by this transport")
    }
    /// POST an `application/x-www-form-urlencoded` body, parse the response as JSON.
    fn post_form(&self, url: &str, form: &[(String, String)]) -> Result<Value>;
    /// A form POST whose success response has no JSON body (revocation).
    fn post_form_empty(&self, _url: &str, _form: &[(String, String)]) -> Result<()> {
        bail!("Empty-response form POST is not supported by this transport")
    }
}

/// Production transport over `ureq`: 30 s timeout, no redirect followed.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl UreqTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

fn json_body(mut response: ureq::http::Response<ureq::Body>) -> Result<Value> {
    response
        .body_mut()
        .read_json::<Value>()
        .context("response is not JSON")
}

impl HttpTransport for UreqTransport {
    fn get_json(&self, url: &str) -> Result<Value> {
        let response = self.agent.get(url).call().context("GET request failed")?;
        if !response.status().is_success() {
            bail!("GET answered HTTP {}", response.status().as_u16());
        }
        json_body(response)
    }

    fn get_json_bearer(&self, url: &str, token: &str) -> Result<Value> {
        let response = self
            .agent
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .call()
            .context("GET request failed")?;
        if !response.status().is_success() {
            bail!("GET answered HTTP {}", response.status().as_u16());
        }
        json_body(response)
    }

    /// The status is not checked here: an error answer is a JSON body that the
    /// caller fails to parse as tokens, as in the reference.
    fn post_form(&self, url: &str, form: &[(String, String)]) -> Result<Value> {
        let response = self
            .agent
            .post(url)
            .send_form(form.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .context("POST request failed")?;
        json_body(response)
    }

    fn post_form_empty(&self, url: &str, form: &[(String, String)]) -> Result<()> {
        let response = self
            .agent
            .post(url)
            .send_form(form.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .context("POST request failed")?;
        if !response.status().is_success() {
            bail!("POST answered HTTP {}", response.status().as_u16());
        }
        Ok(())
    }
}

// -------------------------------------------------------------------- store

/// Where the flow keeps its registrations and sessions. Values are opaque
/// strings (JSON). The OS keychain can be plugged later by implementing this
/// trait; [`FileTokenStore`] is the portable default and [`MemoryTokenStore`]
/// serves tests.
pub trait TokenStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn remove(&self, key: &str);
}

/// In-memory store. Clones share the same map.
#[derive(Clone, Default)]
pub struct MemoryTokenStore {
    inner: Arc<Mutex<BTreeMap<String, String>>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryTokenStore {
    fn get(&self, key: &str) -> Option<String> {
        self.inner.lock().unwrap().get(key).cloned()
    }
    fn set(&self, key: &str, value: &str) {
        self.inner
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
    }
    fn remove(&self, key: &str) {
        self.inner.lock().unwrap().remove(key);
    }
}

/// File store: one JSON object (`key -> value`) in `<dir>/oauth-tokens.json`,
/// written atomically with permissions 0600 on Unix (the directory is created
/// with 0700). A failed write is reported by [`FileTokenStore::last_error`]
/// since the trait is infallible.
pub struct FileTokenStore {
    path: PathBuf,
    guard: Mutex<Option<String>>,
}

impl FileTokenStore {
    pub const FILE_NAME: &'static str = "oauth-tokens.json";

    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            path: dir.as_ref().join(Self::FILE_NAME),
            guard: Mutex::new(None),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Message of the last failed write, if any (never contains a value).
    pub fn last_error(&self) -> Option<String> {
        self.guard.lock().unwrap().clone()
    }

    fn read_map(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write_map(&self, map: &BTreeMap<String, String>) -> Result<()> {
        let dir = self
            .path
            .parent()
            .ok_or_else(|| anyhow!("invalid token store path"))?;
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        // NamedTempFile is created 0600 on Unix; state it explicitly anyway.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tmp.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        tmp.write_all(serde_json::to_string(map)?.as_bytes())?;
        tmp.as_file().sync_all()?;
        tmp.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }

    fn mutate(&self, f: impl FnOnce(&mut BTreeMap<String, String>)) {
        let mut error = self.guard.lock().unwrap();
        let mut map = self.read_map();
        f(&mut map);
        *error = self
            .write_map(&map)
            .err()
            .map(|e| format!("token store write failed: {e}"));
    }
}

impl TokenStore for FileTokenStore {
    fn get(&self, key: &str) -> Option<String> {
        let _held = self.guard.lock().unwrap();
        self.read_map().remove(key)
    }
    fn set(&self, key: &str, value: &str) {
        self.mutate(|m| {
            m.insert(key.to_string(), value.to_string());
        });
    }
    fn remove(&self, key: &str) {
        self.mutate(|m| {
            m.remove(key);
        });
    }
}

// --------------------------------------------------------------------- PKCE

/// Cryptographically random code verifier (64 URL-safe characters).
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64url_encode(&bytes)
}

/// S256 code challenge: `base64url(sha256(verifier))`.
pub fn generate_code_challenge(verifier: &str) -> String {
    base64url_encode(&sha2::Sha256::digest(verifier.as_bytes()))
}

/// Random CSRF state / nonce token.
pub fn generate_state() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// BASE64URL without padding.
pub fn base64url_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 63) as usize] as char);
        }
    }
    out
}

// --------------------------------------------------------------------- types

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub email: String,
    pub subject: String,
    pub client_id: String,
    pub connected: bool,
}

/// An authorization to open in a browser. No `Debug`: the URL carries the
/// PKCE challenge, the state and the nonce.
pub struct Authorization {
    pub state: String,
    pub url: String,
}

/// A model of the ChatGPT account catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

// Deliberately no Debug on anything carrying a credential.
#[derive(Serialize, Deserialize)]
struct Session {
    access_token: String,
    refresh_token: String,
    id_token: String,
    scope: String,
    expires_at: i64,
}

struct Pending {
    verifier: String,
    nonce: String,
    redirect: String,
    profile: Option<Profile>,
    created: Instant,
}

#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    scope: String,
    expires_in: u64,
    token_type: String,
}

#[derive(Deserialize)]
struct Identity {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
    #[allow(dead_code)]
    exp: usize,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Store key of the session of profile `id`.
pub fn session_key(id: &str) -> String {
    format!("ai:openai:{id}:session")
}

fn has_plan(scope: &str) -> bool {
    scope.split_whitespace().any(|s| s == PLAN_SCOPE)
}

fn trusted_auth_url(raw: &str) -> Result<()> {
    let url = Url::parse(raw)?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Untrusted OpenAI authentication endpoint");
    }
    Ok(())
}

// ------------------------------------------------------------------ service

pub struct ChatGptAuth {
    http: Box<dyn HttpTransport>,
    store: Box<dyn TokenStore>,
    pending: Mutex<HashMap<String, Pending>>,
    // Serialize credential updates, including rotating refreshes.
    credentials: Mutex<()>,
}

impl ChatGptAuth {
    pub fn new(http: Box<dyn HttpTransport>, store: Box<dyn TokenStore>) -> Self {
        Self {
            http,
            store,
            pending: Mutex::default(),
            credentials: Mutex::default(),
        }
    }

    pub fn profiles(&self) -> Result<Vec<Profile>> {
        let _lock = self.credentials.lock().unwrap();
        self.read_profiles()
    }

    fn read_profiles(&self) -> Result<Vec<Profile>> {
        match self.store.get(INDEX) {
            Some(v) => serde_json::from_str(&v).map_err(Into::into),
            None => Ok(Vec::new()),
        }
    }

    fn write_profiles(&self, profiles: &[Profile]) -> Result<()> {
        self.store.set(INDEX, &serde_json::to_string(profiles)?);
        Ok(())
    }

    fn read_session(&self, id: &str) -> Result<Session> {
        let raw = self
            .store
            .get(&session_key(id))
            .ok_or_else(|| anyhow!("Sign in to ChatGPT again"))?;
        serde_json::from_str(&raw).map_err(|_| anyhow!("Invalid saved ChatGPT session"))
    }

    /// Starts a sign-in. `redirect` must be a loopback callback address
    /// (`http://127.0.0.1:<port>/auth/callback`). With a `profile_id` the
    /// registration issued earlier is reused.
    pub fn begin(&self, redirect: &str, profile_id: Option<&str>) -> Result<Authorization> {
        let uri = Url::parse(redirect)?;
        if uri.scheme() != "http"
            || uri.host_str() != Some("127.0.0.1")
            || uri.path() != CALLBACK_PATH
            || uri.port().is_none()
            || uri.query().is_some()
            || uri.fragment().is_some()
            || !uri.username().is_empty()
            || uri.password().is_some()
        {
            bail!("Invalid OAuth callback address");
        }
        let _lock = self.credentials.lock().unwrap();
        let profile = match profile_id {
            Some(id) => Some(
                self.read_profiles()?
                    .into_iter()
                    .find(|p| p.id == id)
                    .ok_or_else(|| anyhow!("Unknown ChatGPT account"))?,
            ),
            None => None,
        };
        let host = match self.store.get(HOST) {
            Some(host) => host,
            None => {
                let host = format!("urn:uuid:{}", uuid::Uuid::new_v4());
                self.store.set(HOST, &host);
                host
            }
        };
        let state = generate_state();
        let nonce = generate_state();
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        let client_id = profile
            .as_ref()
            .map(|p| p.client_id.as_str())
            .unwrap_or(DYNAMIC_CLIENT_ID);
        let mut url = Url::parse(&format!("{ISSUER}/api/accounts/authorize"))?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id),
            ("ext_agent_host_id", &host),
            ("response_type", "code"),
            ("redirect_uri", redirect),
            ("scope", SCOPE),
            ("resource", RESOURCE),
            ("state", &state),
            ("nonce", &nonce),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge),
        ]);
        if let Some(profile) = &profile {
            if let Ok(session) = self.read_session(&profile.id) {
                url.query_pairs_mut()
                    .append_pair("id_token_hint", &session.id_token);
            }
            if !profile.subject.is_empty() {
                url.query_pairs_mut()
                    .append_pair("login_hint", &profile.email);
            }
        } else {
            url.query_pairs_mut()
                .append_pair("agent_name_hint", "Coding Studio");
        }
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, p| p.created.elapsed() < PENDING_TTL);
        pending.insert(
            state.clone(),
            Pending {
                verifier,
                nonce,
                redirect: redirect.to_string(),
                profile,
                created: Instant::now(),
            },
        );
        Ok(Authorization {
            state,
            url: url.to_string(),
        })
    }

    /// Forgets a pending sign-in (browser could not be opened, user gave up).
    pub fn cancel(&self, state: &str) {
        self.pending.lock().unwrap().remove(state);
    }

    /// Finishes a sign-in with the callback URL received on the loopback
    /// listener. One-shot: the attempt is consumed whatever the outcome.
    pub fn complete(&self, expected_state: &str, callback_url: &str) -> Result<Profile> {
        let callback = Url::parse(callback_url).map_err(|_| anyhow!("Invalid OAuth callback"))?;
        let mut pairs = HashMap::new();
        for (key, value) in callback.query_pairs().into_owned() {
            if pairs.insert(key, value).is_some() {
                bail!("Ambiguous OAuth callback");
            }
        }
        if pairs.get("state").map(String::as_str) != Some(expected_state) {
            bail!("OAuth state mismatch");
        }
        let pending = self
            .pending
            .lock()
            .unwrap()
            .remove(expected_state)
            .ok_or_else(|| anyhow!("OAuth attempt expired"))?;
        if pending.created.elapsed() >= PENDING_TTL {
            bail!("OAuth attempt expired");
        }
        let expected = Url::parse(&pending.redirect)?;
        if callback.scheme() != expected.scheme()
            || callback.host_str() != expected.host_str()
            || callback.port() != expected.port()
            || callback.path() != expected.path()
            || !callback.username().is_empty()
            || callback.password().is_some()
            || callback.fragment().is_some()
        {
            bail!("OAuth callback mismatch");
        }
        if pairs.contains_key("error") {
            bail!("ChatGPT sign-in was declined");
        }
        let client_id = match &pending.profile {
            Some(p) => {
                if pairs.get("client_id").is_some_and(|id| id != &p.client_id) {
                    bail!("OAuth client mismatch");
                }
                p.client_id.clone()
            }
            None => pairs
                .get("client_id")
                .filter(|id| !id.is_empty() && id.as_str() != DYNAMIC_CLIENT_ID)
                .cloned()
                .ok_or_else(|| anyhow!("ChatGPT registration incomplete"))?,
        };
        let code = pairs
            .get("code")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("Missing OAuth code"))?;
        let _lock = self.credentials.lock().unwrap();
        // Registration survives an exchange/network failure. The next sign-in
        // reuses the issued client rather than registering a new app.
        let registration = pending.profile.clone().unwrap_or_else(|| Profile {
            id: uuid::Uuid::new_v4().to_string(),
            email: "ChatGPT".into(),
            subject: String::new(),
            client_id: client_id.clone(),
            connected: false,
        });
        if pending.profile.is_none() {
            let mut profiles = self.read_profiles()?;
            profiles.push(registration.clone());
            self.write_profiles(&profiles)?;
        }
        let tokens = self.token(&[
            ("grant_type", "authorization_code".into()),
            ("client_id", client_id.clone()),
            ("code", code.clone()),
            ("code_verifier", pending.verifier),
            ("redirect_uri", pending.redirect),
            ("resource", RESOURCE.into()),
        ])?;
        if !has_plan(&tokens.scope) {
            bail!("Enable ChatGPT plan usage and sign in again");
        }
        let identity = self.identity(
            tokens
                .id_token
                .as_deref()
                .ok_or_else(|| anyhow!("Missing verified identity"))?,
            &client_id,
            Some(&pending.nonce),
        )?;
        if pending
            .profile
            .as_ref()
            .is_some_and(|p| !p.subject.is_empty() && p.subject != identity.sub)
        {
            bail!("ChatGPT account mismatch");
        }
        let profile = Profile {
            id: registration.id,
            email: identity.email.unwrap_or_else(|| identity.sub.clone()),
            subject: identity.sub,
            client_id,
            connected: true,
        };
        let session = Session {
            access_token: tokens.access_token,
            refresh_token: tokens
                .refresh_token
                .ok_or_else(|| anyhow!("Missing renewable session"))?,
            id_token: tokens.id_token.unwrap_or_default(),
            scope: tokens.scope,
            expires_at: now() + tokens.expires_in.min(i64::MAX as u64) as i64,
        };
        let mut profiles = self.read_profiles()?;
        profiles.retain(|p| p.id != profile.id);
        profiles.push(profile.clone());
        self.store
            .set(&session_key(&profile.id), &serde_json::to_string(&session)?);
        self.write_profiles(&profiles)?;
        Ok(profile)
    }

    fn token(&self, form: &[(&str, String)]) -> Result<Tokens> {
        let form: Vec<(String, String)> = form
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let value = self
            .http
            .post_form(TOKEN_URL, &form)
            .map_err(|_| anyhow!("OpenAI token request failed"))?;
        let tokens: Tokens = serde_json::from_value(value)
            .map_err(|_| anyhow!("OpenAI refused the token request; sign in again"))?;
        if !tokens.token_type.eq_ignore_ascii_case("bearer")
            || tokens.access_token.is_empty()
            || tokens.expires_in == 0
            || tokens.expires_in > 86400 * 31
        {
            bail!("Invalid OpenAI token response");
        }
        Ok(tokens)
    }

    fn identity(&self, token: &str, client: &str, nonce: Option<&str>) -> Result<Identity> {
        let header = decode_header(token).map_err(|_| anyhow!("Invalid OpenAI identity"))?;
        if header.alg != jsonwebtoken::Algorithm::RS256 {
            bail!("Unexpected identity signature");
        }
        let metadata = self
            .http
            .get_json(&format!("{ISSUER}/.well-known/openid-configuration"))
            .map_err(|_| anyhow!("Identity discovery failed"))?;
        let jwks_uri = metadata["jwks_uri"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing identity keys"))?;
        trusted_auth_url(jwks_uri)?;
        let keys: JwkSet = serde_json::from_value(
            self.http
                .get_json(jwks_uri)
                .map_err(|_| anyhow!("Identity keys unavailable"))?,
        )
        .map_err(|_| anyhow!("Invalid identity keys"))?;
        let key = keys
            .find(
                header
                    .kid
                    .as_deref()
                    .ok_or_else(|| anyhow!("Missing identity key ID"))?,
            )
            .ok_or_else(|| anyhow!("Unknown identity key"))?;
        let mut validation = Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[client]);
        validation.set_required_spec_claims(&["exp", "sub", "iss", "aud"]);
        let identity = decode::<Identity>(
            token,
            &DecodingKey::from_jwk(key).map_err(|_| anyhow!("Invalid identity key"))?,
            &validation,
        )
        .map_err(|_| anyhow!("Identity validation failed"))?
        .claims;
        if identity.sub.is_empty()
            || nonce.is_some_and(|nonce| identity.nonce.as_deref() != Some(nonce))
        {
            bail!("Identity nonce mismatch");
        }
        Ok(identity)
    }

    /// The bearer for `id`, refreshed when it expires within a minute. A
    /// rotating refresh token is never raced by two calls of this service.
    pub fn access_token(&self, id: &str) -> Result<String> {
        let _lock = self.credentials.lock().unwrap();
        let profile = self
            .read_profiles()?
            .into_iter()
            .find(|p| p.id == id && p.connected)
            .ok_or_else(|| anyhow!("Sign in to ChatGPT again"))?;
        let mut session = self.read_session(id)?;
        if !has_plan(&session.scope) {
            bail!("ChatGPT plan usage is not enabled");
        }
        if session.expires_at <= now() + 60 {
            let tokens = self.token(&[
                ("grant_type", "refresh_token".into()),
                ("client_id", profile.client_id.clone()),
                ("refresh_token", session.refresh_token.clone()),
                ("resource", RESOURCE.into()),
            ])?;
            if !tokens.scope.is_empty() && !has_plan(&tokens.scope) {
                bail!("ChatGPT plan usage is no longer enabled");
            }
            if let Some(id_token) = &tokens.id_token {
                if self.identity(id_token, &profile.client_id, None)?.sub != profile.subject {
                    bail!("Refreshed ChatGPT identity mismatch");
                }
            }
            session.access_token = tokens.access_token;
            if let Some(token) = tokens.refresh_token {
                session.refresh_token = token;
            }
            if let Some(token) = tokens.id_token {
                session.id_token = token;
            }
            if !tokens.scope.is_empty() {
                session.scope = tokens.scope;
            }
            session.expires_at = now() + tokens.expires_in as i64;
            self.store
                .set(&session_key(id), &serde_json::to_string(&session)?);
        }
        Ok(session.access_token)
    }

    /// Models listed for the account (`GET {RESOURCE}/models`), as the Munrel
    /// shell does it.
    pub fn list_models(&self, id: &str) -> Result<Vec<ModelInfo>> {
        let token = self.access_token(id)?;
        let value = self
            .http
            .get_json_bearer(&format!("{RESOURCE}/models"), &token)
            .map_err(|_| anyhow!("OpenAI model catalog unavailable"))?;
        let models = value["models"]
            .as_array()
            .ok_or_else(|| anyhow!("OpenAI did not return an account model catalog"))?;
        Ok(models
            .iter()
            .filter(|m| m["visibility"] == "list")
            .filter_map(|m| {
                Some(ModelInfo {
                    id: m["slug"].as_str()?.into(),
                    name: m["display_name"].as_str()?.into(),
                })
            })
            .collect())
    }

    /// Revokes the refresh token (best effort) and drops the session. The
    /// registration stays, so signing in again reuses the issued client.
    /// Returns whether the revocation was accepted.
    pub fn disconnect(&self, id: &str) -> Result<bool> {
        let _lock = self.credentials.lock().unwrap();
        let mut profiles = self.read_profiles()?;
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("Unknown ChatGPT account"))?;
        let mut revoked = false;
        if let Ok(session) = self.read_session(id) {
            if let Ok(metadata) = self
                .http
                .get_json(&format!("{ISSUER}/.well-known/openid-configuration"))
            {
                if let Some(endpoint) = metadata["revocation_endpoint"].as_str() {
                    if trusted_auth_url(endpoint).is_ok() {
                        let form = vec![
                            ("token".to_string(), session.refresh_token),
                            ("token_type_hint".to_string(), "refresh_token".to_string()),
                            ("client_id".to_string(), profile.client_id.clone()),
                        ];
                        for attempt in 0..2 {
                            if attempt > 0 {
                                std::thread::sleep(Duration::from_millis(250));
                            }
                            if self.http.post_form_empty(endpoint, &form).is_ok() {
                                revoked = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        profile.connected = false;
        self.store.remove(&session_key(id));
        self.write_profiles(&profiles)?;
        Ok(revoked)
    }
}

// ----------------------------------------------------------------- loopback

/// One-shot loopback listener for the OAuth callback (std::net only).
pub struct LoopbackCallback;

/// Waits for the callback on a listener bound by [`LoopbackCallback::listen`].
pub struct CallbackHandle {
    listener: TcpListener,
    port: u16,
}

impl LoopbackCallback {
    /// Binds `127.0.0.1:<port>` (any free port when `None`) and returns the
    /// redirect URL to give to [`ChatGptAuth::begin`] with the handle that
    /// waits for the callback.
    pub fn listen(port: Option<u16>) -> Result<(String, CallbackHandle)> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port.unwrap_or(0)))
            .context("cannot listen on the loopback interface")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok((
            format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
            CallbackHandle { listener, port },
        ))
    }
}

impl CallbackHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Waits for one `GET /auth/callback?...` and returns the full callback
    /// URL. Other requests (wrong path, wrong Host header, malformed) get a
    /// 400 and are ignored. Fails once `timeout` has elapsed.
    pub fn wait(self, timeout: Duration) -> Result<String> {
        self.wait_inner(None, timeout)
    }

    /// Like [`Self::wait`], but only a request carrying `state` ends the wait.
    pub fn wait_for_state(self, state: &str, timeout: Duration) -> Result<String> {
        self.wait_inner(Some(state), timeout)
    }

    fn wait_inner(self, state: Option<&str>, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                bail!("Timed out waiting for the ChatGPT sign-in callback");
            }
            let mut stream = match self.listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut bytes = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        bytes.extend_from_slice(&buf[..n]);
                        if bytes.windows(4).any(|w| w == b"\r\n\r\n") || bytes.len() > 8192 {
                            break;
                        }
                    }
                }
            }
            let reply = |stream: &mut std::net::TcpStream, status: &str, body: &str| {
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            };
            if bytes.len() > 8192 {
                reply(&mut stream, "400 Bad Request", "");
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let hosts: Vec<&str> = text
                .lines()
                .filter_map(|l| l.split_once(':'))
                .filter(|(k, _)| k.eq_ignore_ascii_case("host"))
                .map(|(_, v)| v.trim())
                .collect();
            let target = text
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("GET "))
                .and_then(|l| l.strip_suffix(" HTTP/1.1"));
            let url = target
                .filter(|t| t.starts_with(&format!("{CALLBACK_PATH}?")))
                .and_then(|t| Url::parse(&format!("http://127.0.0.1:{}{t}", self.port)).ok());
            let valid = hosts == [format!("127.0.0.1:{}", self.port)]
                && url.as_ref().is_some_and(|u| {
                    state.is_none_or(|s| u.query_pairs().any(|(k, v)| k == "state" && v == s))
                });
            match (valid, url) {
                (true, Some(url)) => {
                    reply(
                        &mut stream,
                        "200 OK",
                        "Sign-in received. You can return to Coding Studio.",
                    );
                    return Ok(url.to_string());
                }
                _ => reply(&mut stream, "400 Bad Request", ""),
            }
        }
    }
}
