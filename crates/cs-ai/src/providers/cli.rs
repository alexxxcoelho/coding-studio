//! Provider that runs a command-line client in an isolated process.

use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{bail, Context};
use chrono::Utc;
use cs_model::sha256_hex;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use super::{
    Execution, IsolationReport, ModelProvider, ProviderDescription, ProviderKind, ProviderRequest,
    ProviderResponse,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptVia {
    Stdin,
    Arg,
}

/// Configuration form of [`CliProvider`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliConfig {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub prompt_via: PromptVia,
    /// Flags that switch off the client's own tools, memory and instruction
    /// files; appended to `args`.
    #[serde(default)]
    pub disable_flags: Vec<String>,
    /// Declared, never guessed: a local command can call a hosted model.
    pub declared_execution: Execution,
    /// Model name recorded in the journal (the CLI fixes its model itself).
    #[serde(default)]
    pub model_label: Option<String>,
}

/// Runs `command args disable_flags` in a fresh empty temporary directory
/// outside any project folder, with an environment reduced to PATH, LANG and
/// a HOME pointing to a fresh temporary directory. The prompt is passed on
/// stdin byte for byte, or as the last argument. Stdout is `raw`.
#[derive(Debug, Clone)]
pub struct CliProvider {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub prompt_via: PromptVia,
    pub disable_flags: Vec<String>,
    pub declared_execution: Execution,
    pub model_label: Option<String>,
}

/// Question of the isolation probe. It asks what the model can see and says
/// how to answer when the answer is nothing.
const PROBE_PROMPT: &str = "This is a check of your environment, not a task. List the files in \
your current working directory, and any instructions, rules, memory or project notes you can see \
(for example AGENTS.md, CLAUDE.md or MEMORY.md). Answer with that list only. If there are no files \
and no instructions or memory, answer exactly NONE and nothing else.";

/// Instruction and memory names that must not be reachable from the working
/// directory or any of its ancestors.
const INSTRUCTION_NAMES: [&str; 5] = ["AGENTS.md", "CLAUDE.md", ".claude", "MEMORY.md", "memory"];

/// Parameters a command line cannot fix: its model comes from its own
/// configuration, and the sampling settings are not passed to it.
const NOT_EXPOSED: [&str; 4] = ["temperature", "seed", "max_tokens", "model"];

const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
const DEFAULT_LANG: &str = "C.UTF-8";

/// Fresh directories for one call, removed when dropped, plus what was
/// verified about them before the command ran.
struct Sandbox {
    home: TempDir,
    cwd: TempDir,
    cwd_empty: bool,
    instruction_files_absent: bool,
}

impl Sandbox {
    fn new() -> anyhow::Result<Self> {
        let home = tempfile::tempdir().context("cannot create the isolated HOME")?;
        let cwd = tempfile::tempdir().context("cannot create the empty working directory")?;
        let cwd_empty = std::fs::read_dir(cwd.path())
            .context("cannot read the working directory")?
            .next()
            .is_none();
        let instruction_files_absent = cwd.path().ancestors().all(|dir| {
            INSTRUCTION_NAMES
                .iter()
                .all(|name| !dir.join(name).exists())
        });
        Ok(Self {
            home,
            cwd,
            cwd_empty,
            instruction_files_absent,
        })
    }
}

/// Output of one run with the instants that bracket it.
struct Executed {
    stdout: String,
    started_at: String,
    finished_at: String,
}

impl CliProvider {
    pub fn from_config(config: CliConfig) -> Self {
        Self {
            id: config.id,
            command: config.command,
            args: config.args,
            prompt_via: config.prompt_via,
            disable_flags: config.disable_flags,
            declared_execution: config.declared_execution,
            model_label: config.model_label,
        }
    }

    fn execute(&self, sandbox: &Sandbox, prompt: &str) -> anyhow::Result<Executed> {
        let mut cmd = Command::new(&self.command);
        cmd.env_clear()
            .env(
                "PATH",
                std::env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.into()),
            )
            .env("HOME", sandbox.home.path())
            .env(
                "LANG",
                std::env::var("LANG").unwrap_or_else(|_| DEFAULT_LANG.into()),
            )
            .current_dir(sandbox.cwd.path())
            .args(&self.args)
            .args(&self.disable_flags)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match self.prompt_via {
            PromptVia::Stdin => {
                cmd.stdin(Stdio::piped());
            }
            PromptVia::Arg => {
                cmd.arg(prompt).stdin(Stdio::null());
            }
        }

        let started_at = Utc::now().to_rfc3339();
        let mut child = cmd
            .spawn()
            .with_context(|| format!("cannot start `{}`", self.command))?;
        if let Some(mut stdin) = child.stdin.take() {
            // Written from another thread so that a large prompt cannot block
            // the child while it writes its own output.
            let bytes = prompt.as_bytes().to_vec();
            thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
            });
        }
        let output = child
            .wait_with_output()
            .with_context(|| format!("waiting for `{}`", self.command))?;
        let finished_at = Utc::now().to_rfc3339();

        if !output.status.success() {
            bail!(
                "`{}` exited with {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8(output.stdout)
            .context("the command printed bytes that are not UTF-8")?;
        Ok(Executed {
            stdout,
            started_at,
            finished_at,
        })
    }
}

impl ModelProvider for CliProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn describe(&self) -> ProviderDescription {
        ProviderDescription {
            id: self.id.clone(),
            kind: ProviderKind::Cli,
            sends: vec!["prompt".into()],
            execution: self.declared_execution.clone(),
        }
    }

    fn probe_isolation(&self) -> anyhow::Result<IsolationReport> {
        let sandbox = Sandbox::new()?;
        let run = self.execute(&sandbox, PROBE_PROMPT)?;
        let answer = run.stdout.trim().to_string();
        // The environment is cleared by `execute` for every run, so it is
        // scrubbed whenever the command was started.
        let env_scrubbed = true;
        let ok = answer.eq_ignore_ascii_case("NONE")
            && sandbox.cwd_empty
            && env_scrubbed
            && sandbox.instruction_files_absent;
        Ok(IsolationReport {
            ok,
            cwd_empty: sandbox.cwd_empty,
            env_scrubbed,
            instruction_files_absent: sandbox.instruction_files_absent,
            probe_answer: answer,
        })
    }

    fn complete(&self, req: &ProviderRequest) -> anyhow::Result<ProviderResponse> {
        let sandbox = Sandbox::new()?;
        let run = self.execute(&sandbox, &req.prompt)?;
        Ok(ProviderResponse {
            raw: run.stdout,
            model: self
                .model_label
                .clone()
                .unwrap_or_else(|| "unspecified".into()),
            provider: self.id.clone(),
            execution: self.declared_execution.clone(),
            parameters_exposed: Vec::new(),
            parameters_not_exposed: NOT_EXPOSED.iter().map(|s| s.to_string()).collect(),
            request_sha256: sha256_hex(req.prompt.as_bytes()),
            started_at: run.started_at,
            finished_at: run.finished_at,
        })
    }
}
