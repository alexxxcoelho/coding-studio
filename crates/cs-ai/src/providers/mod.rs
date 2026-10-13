//! Optional automation of the blind second reader. The copy-and-paste
//! workflow stays the reference (see docs/METHOD.md, step 5); a provider only
//! replaces the manual copy of the prompt and the paste of the answer, and
//! records what left the machine and which parameters the run could not fix.
//!
use anyhow::bail;
use serde::{Deserialize, Serialize};

mod cli;
mod oauth;
pub mod oauth_flow;
mod openai;

pub use cli::{CliConfig, CliProvider, PromptVia};
pub use oauth::{OAuthChatGptConfig, OAuthChatGptProvider};
pub use openai::{OpenAiCompatibleConfig, OpenAiCompatibleProvider, Preset};

/// Reserved id of the manual workflow, always present and listed first.
pub const COPY_PASTE_ID: &str = "copy_paste";

/// What one call needs. `None` means "not set by the caller": the parameter
/// is then not sent and is reported in `parameters_not_exposed`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub seed: Option<u64>,
    pub max_tokens: Option<u32>,
}

/// Where the model runs. A command can be local while the model it calls is
/// hosted, so this is declared by configuration, never guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Execution {
    Local,
    Hosted { vendor: String, endpoint: String },
}

/// Journal-ready record of one call. Never holds a credential.
/// `parameters_exposed` / `parameters_not_exposed` use the names
/// "temperature", "seed", "max_tokens" (and "model" for a CLI, whose model is
/// fixed by its own configuration). `request_sha256` is the lower-case hex
/// SHA-256 of the prompt bytes. Timestamps are RFC 3339 UTC strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub raw: String,
    pub model: String,
    pub provider: String,
    pub execution: Execution,
    pub parameters_exposed: Vec<String>,
    pub parameters_not_exposed: Vec<String>,
    pub request_sha256: String,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Cli,
    OpenAiCompatible,
    #[serde(rename = "oauth_chat_gpt")]
    OAuthChatGpt,
    /// Added to the agreed design so that the manual workflow can be listed
    /// in the registry like any other provider.
    CopyPaste,
}

/// What a provider is and what leaves the machine through it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescription {
    pub id: String,
    pub kind: ProviderKind,
    /// What leaves the machine: "prompt", "model name", ...
    pub sends: Vec<String>,
    pub execution: Execution,
}

impl ProviderDescription {
    /// One line for the journal. A CLI is described by two facts:
    /// "command local, model hosted" or "command local, model local".
    /// Other kinds: "hosted (<vendor>, <endpoint>)" or "local".
    pub fn journal_line(&self) -> String {
        match (self.kind, &self.execution) {
            (ProviderKind::Cli, Execution::Local) => "command local, model local".into(),
            (ProviderKind::Cli, Execution::Hosted { .. }) => "command local, model hosted".into(),
            (_, Execution::Local) => "local".into(),
            (_, Execution::Hosted { vendor, endpoint }) => format!("hosted ({vendor}, {endpoint})"),
        }
    }
}

/// Result of the isolation probe. `ok` is true only when the model's answer
/// to the probe reports nothing (the trimmed answer is `NONE`, ignoring case).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationReport {
    pub ok: bool,
    pub cwd_empty: bool,
    pub env_scrubbed: bool,
    pub instruction_files_absent: bool,
    pub probe_answer: String,
}

pub trait ModelProvider {
    fn id(&self) -> &str;
    fn describe(&self) -> ProviderDescription;
    fn probe_isolation(&self) -> anyhow::Result<IsolationReport>;
    fn complete(&self, req: &ProviderRequest) -> anyhow::Result<ProviderResponse>;
}

/// Providers listed in the configuration, by id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderConfig {
    Cli(CliConfig),
    OpenAiCompatible(OpenAiCompatibleConfig),
    #[serde(rename = "oauth_chat_gpt")]
    OAuthChatGpt(OAuthChatGptConfig),
}

/// The manual workflow, always first in the registry. It never completes
/// anything by itself: `complete` and `probe_isolation` return errors.
#[derive(Debug, Clone, Default)]
pub struct CopyPasteProvider;

impl ModelProvider for CopyPasteProvider {
    fn id(&self) -> &str {
        COPY_PASTE_ID
    }
    fn describe(&self) -> ProviderDescription {
        ProviderDescription {
            id: COPY_PASTE_ID.into(),
            kind: ProviderKind::CopyPaste,
            sends: Vec::new(),
            execution: Execution::Local,
        }
    }
    fn probe_isolation(&self) -> anyhow::Result<IsolationReport> {
        bail!("the copy-and-paste workflow runs no model, so there is nothing to probe")
    }
    fn complete(&self, _req: &ProviderRequest) -> anyhow::Result<ProviderResponse> {
        bail!("the copy-and-paste workflow sends nothing; the prompt is copied by hand")
    }
}

/// Builds the registry: `copy_paste` first, then the configured providers in
/// order. An entry that cannot be built (Custom preset without base_url) or
/// that reuses the reserved id `copy_paste` is skipped.
pub fn providers_from_config(config: &ProvidersConfig) -> Vec<Box<dyn ModelProvider>> {
    let mut registry: Vec<Box<dyn ModelProvider>> = vec![Box::new(CopyPasteProvider)];
    for entry in &config.providers {
        let id = match entry {
            ProviderConfig::Cli(c) => c.id.as_str(),
            ProviderConfig::OpenAiCompatible(c) => c.id.as_str(),
            ProviderConfig::OAuthChatGpt(c) => c.id.as_str(),
        };
        if id == COPY_PASTE_ID {
            continue;
        }
        let built: Option<Box<dyn ModelProvider>> = match entry {
            ProviderConfig::Cli(c) => {
                Some(Box::new(CliProvider::from_config(c.clone())) as Box<dyn ModelProvider>)
            }
            ProviderConfig::OpenAiCompatible(c) => OpenAiCompatibleProvider::new(c.clone())
                .ok()
                .map(|p| Box::new(p) as Box<dyn ModelProvider>),
            ProviderConfig::OAuthChatGpt(c) => {
                Some(Box::new(OAuthChatGptProvider::new(c.clone())) as Box<dyn ModelProvider>)
            }
        };
        registry.extend(built);
    }
    registry
}
