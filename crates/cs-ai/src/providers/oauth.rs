//! ChatGPT subscription login. The sign-in itself lives in [`super::oauth_flow`]
//! (`ChatGptAuth`, ported from Munrel); this provider takes the bearer it
//! issues and calls the OpenAI chat completions API with it, with the same
//! request body as the OpenAI-compatible provider.

use std::sync::Arc;

use anyhow::{anyhow, bail, Context};
use chrono::Utc;
use cs_model::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::oauth_flow::{ChatGptAuth, FileTokenStore, UreqTransport, RESOURCE};
use super::{
    Execution, IsolationReport, ModelProvider, ProviderDescription, ProviderKind, ProviderRequest,
    ProviderResponse,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OAuthChatGptConfig {
    pub id: String,
    /// Connected ChatGPT profile to use. When absent, the first connected
    /// profile is used.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Model used when the request names none.
    #[serde(default)]
    pub model: Option<String>,
    /// Directory of the file token store. Without it no login can exist and
    /// the provider is "not configured". The OS keychain can be plugged in
    /// later through `TokenStore`.
    #[serde(default)]
    pub token_dir: Option<String>,
}

#[derive(Clone)]
pub struct OAuthChatGptProvider {
    pub id: String,
    pub profile_id: Option<String>,
    pub model: Option<String>,
    auth: Option<Arc<ChatGptAuth>>,
    api_base: String,
}

// Hand-written: the auth service carries credentials and has no `Debug`.
impl std::fmt::Debug for OAuthChatGptProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthChatGptProvider")
            .field("id", &self.id)
            .field("profile_id", &self.profile_id)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl OAuthChatGptProvider {
    pub fn new(config: OAuthChatGptConfig) -> Self {
        let auth = config
            .token_dir
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .map(|dir| {
                Arc::new(ChatGptAuth::new(
                    Box::new(UreqTransport::new()),
                    Box::new(FileTokenStore::new(dir)),
                ))
            });
        Self {
            id: config.id,
            profile_id: config.profile_id,
            model: config.model,
            auth,
            api_base: RESOURCE.to_string(),
        }
    }

    /// A provider over an existing auth service. `api_base` replaces
    /// `https://api.openai.com/v1` (used by tests, which run a loopback server).
    pub fn with_auth(
        config: OAuthChatGptConfig,
        auth: Arc<ChatGptAuth>,
        api_base: impl Into<String>,
    ) -> Self {
        Self {
            id: config.id,
            profile_id: config.profile_id,
            model: config.model,
            auth: Some(auth),
            api_base: api_base.into().trim_end_matches('/').to_string(),
        }
    }

    fn not_configured(&self) -> anyhow::Error {
        anyhow!("ChatGPT login is not configured yet ({})", self.id)
    }

    /// Bearer of the configured (or first connected) profile.
    fn bearer(&self) -> anyhow::Result<String> {
        let auth = self.auth.as_ref().ok_or_else(|| self.not_configured())?;
        let profile = match &self.profile_id {
            Some(id) => id.clone(),
            None => auth
                .profiles()?
                .into_iter()
                .find(|p| p.connected)
                .map(|p| p.id)
                .ok_or_else(|| self.not_configured())?,
        };
        auth.access_token(&profile)
            .map_err(|_| self.not_configured())
    }

    fn agent(&self) -> ureq::Agent {
        let rest = self.api_base.split_once("://").map_or("", |(_, r)| r);
        if rest.starts_with("127.") || rest.starts_with("localhost") {
            ureq::Agent::config_builder()
                .proxy(None)
                .build()
                .new_agent()
        } else {
            ureq::Agent::new_with_defaults()
        }
    }
}

impl ModelProvider for OAuthChatGptProvider {
    fn id(&self) -> &str {
        &self.id
    }
    /// Kind `OAuthChatGpt`, execution Hosted (vendor "openai").
    fn describe(&self) -> ProviderDescription {
        ProviderDescription {
            id: self.id.clone(),
            kind: ProviderKind::OAuthChatGpt,
            sends: vec!["prompt".into(), "model name".into()],
            execution: Execution::Hosted {
                vendor: "openai".into(),
                endpoint: self.api_base.clone(),
            },
        }
    }
    fn probe_isolation(&self) -> anyhow::Result<IsolationReport> {
        bail!(
            "the isolation probe does not apply to an HTTP API ({})",
            self.id
        )
    }
    /// Errors with "not configured" while no ChatGPT profile is connected.
    fn complete(&self, req: &ProviderRequest) -> anyhow::Result<ProviderResponse> {
        let token = self.bearer()?;
        let model = req
            .model
            .clone()
            .or_else(|| self.model.clone())
            .filter(|m| !m.is_empty())
            .ok_or_else(|| anyhow!("provider `{}` has no model", self.id))?;
        let mut body = json!({
            "model": model,
            "messages": [{ "role": "user", "content": req.prompt }],
        });
        let mut exposed = Vec::new();
        let mut not_exposed = Vec::new();
        match req.temperature {
            Some(t) => {
                body["temperature"] = json!(t);
                exposed.push("temperature".to_string());
            }
            None => not_exposed.push("temperature".to_string()),
        }
        match req.seed {
            Some(s) => {
                body["seed"] = json!(s);
                exposed.push("seed".to_string());
            }
            None => not_exposed.push("seed".to_string()),
        }
        match req.max_tokens {
            Some(m) => {
                body["max_tokens"] = json!(m);
                exposed.push("max_tokens".to_string());
            }
            None => not_exposed.push("max_tokens".to_string()),
        }

        let started_at = Utc::now().to_rfc3339();
        let mut response = self
            .agent()
            .post(format!("{}/chat/completions", self.api_base))
            .header("Authorization", format!("Bearer {token}"))
            .send_json(&body)
            .map_err(|e| match e {
                ureq::Error::StatusCode(code) => {
                    anyhow!("provider `{}` answered HTTP {code}", self.id)
                }
                other => anyhow!("request to provider `{}` failed: {other}", self.id),
            })?;
        let answer: Value = response.body_mut().read_json().with_context(|| {
            format!(
                "provider `{}` answered with something that is not JSON",
                self.id
            )
        })?;
        let finished_at = Utc::now().to_rfc3339();

        let content = answer
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow!(
                    "provider `{}` answer has no choices[0].message.content",
                    self.id
                )
            })?;
        let served_model = answer
            .get("model")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .unwrap_or(model);

        Ok(ProviderResponse {
            raw: content.to_string(),
            model: served_model,
            provider: self.id.clone(),
            execution: self.describe().execution,
            parameters_exposed: exposed,
            parameters_not_exposed: not_exposed,
            request_sha256: sha256_hex(req.prompt.as_bytes()),
            started_at,
            finished_at,
        })
    }
}
