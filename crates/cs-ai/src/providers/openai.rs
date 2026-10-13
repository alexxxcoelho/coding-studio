//! Provider for OpenAI-compatible `POST {base_url}/chat/completions`.
//!
//! Loopback hosts bypass any proxy from the environment, so that a local
//! server is always reached directly. Other hosts use the usual proxy
//! variables, with `NO_PROXY` honoured by the HTTP client.

use anyhow::{anyhow, bail, Context};
use chrono::Utc;
use cs_model::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    Execution, IsolationReport, ModelProvider, ProviderDescription, ProviderKind, ProviderRequest,
    ProviderResponse,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "zai")]
    ZAi,
    #[serde(rename = "custom")]
    Custom,
}

impl Preset {
    /// OpenAi: https://api.openai.com/v1, ZAi: https://api.z.ai/api/paas/v4,
    /// Custom: None.
    pub fn default_base_url(&self) -> Option<&'static str> {
        match self {
            Preset::OpenAi => Some("https://api.openai.com/v1"),
            Preset::ZAi => Some("https://api.z.ai/api/paas/v4"),
            Preset::Custom => None,
        }
    }

    fn vendor(&self) -> &'static str {
        match self {
            Preset::OpenAi => "openai",
            Preset::ZAi => "z.ai",
            Preset::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAiCompatibleConfig {
    pub id: String,
    pub preset: Preset,
    /// Overrides the preset; required for `Preset::Custom`.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Name of the environment variable that holds the bearer token. The
    /// token itself is read at call time and never stored or journalled.
    pub api_key_env: String,
    pub model: String,
}

/// Holds the name of the variable that carries the key, never the key.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    pub id: String,
    pub preset: Preset,
    pub api_key_env: String,
    pub model: String,
    base_url: String,
}

impl OpenAiCompatibleProvider {
    /// Errors for `Preset::Custom` without a base URL. A trailing `/` of the
    /// base URL is removed.
    pub fn new(config: OpenAiCompatibleConfig) -> anyhow::Result<Self> {
        let base = config
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| config.preset.default_base_url())
            .ok_or_else(|| anyhow!("provider `{}` needs a base_url", config.id))?;
        Ok(Self {
            id: config.id,
            preset: config.preset,
            api_key_env: config.api_key_env,
            model: config.model,
            base_url: base.trim_end_matches('/').to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn agent(&self) -> ureq::Agent {
        if is_loopback(&self.base_url) {
            ureq::Agent::config_builder()
                .proxy(None)
                .build()
                .new_agent()
        } else {
            ureq::Agent::new_with_defaults()
        }
    }
}

/// True for `localhost`, `127.x.x.x` and `::1`, with or without a port.
fn is_loopback(base_url: &str) -> bool {
    let rest = base_url.split_once("://").map_or(base_url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or_default();
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

impl ModelProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn describe(&self) -> ProviderDescription {
        ProviderDescription {
            id: self.id.clone(),
            kind: ProviderKind::OpenAiCompatible,
            sends: vec!["prompt".into(), "model name".into()],
            execution: Execution::Hosted {
                vendor: self.preset.vendor().into(),
                endpoint: self.endpoint(),
            },
        }
    }

    /// An HTTP API runs no local process, so there is no working directory or
    /// environment to probe: the probe does not apply.
    fn probe_isolation(&self) -> anyhow::Result<IsolationReport> {
        bail!(
            "the isolation probe does not apply to an HTTP API ({})",
            self.id
        )
    }

    fn complete(&self, req: &ProviderRequest) -> anyhow::Result<ProviderResponse> {
        let key = std::env::var(&self.api_key_env)
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| anyhow!("environment variable {} is not set", self.api_key_env))?;

        let model = req.model.clone().unwrap_or_else(|| self.model.clone());
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
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {key}"))
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
