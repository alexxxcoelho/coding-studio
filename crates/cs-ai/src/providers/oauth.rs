//! ChatGPT subscription login. PENDING munrel-connectors: the ChatGptAuth
//! port is not available yet, so this provider is not configured.

use anyhow::bail;
use serde::{Deserialize, Serialize};

use super::{
    Execution, IsolationReport, ModelProvider, ProviderDescription, ProviderKind, ProviderRequest,
    ProviderResponse,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuthChatGptConfig {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct OAuthChatGptProvider {
    pub id: String,
}

impl OAuthChatGptProvider {
    pub fn new(config: OAuthChatGptConfig) -> Self {
        Self { id: config.id }
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
                endpoint: "https://chatgpt.com/backend-api".into(),
            },
        }
    }
    fn probe_isolation(&self) -> anyhow::Result<IsolationReport> {
        bail!("ChatGPT login is not configured yet ({})", self.id)
    }
    /// Always an error containing "not configured" until the port exists.
    fn complete(&self, _req: &ProviderRequest) -> anyhow::Result<ProviderResponse> {
        bail!("ChatGPT login is not configured yet ({})", self.id)
    }
}
