#![cfg(unix)]
//! Tests of the optional AI providers. Offline and deterministic: the CLI is
//! a shell script written by the test, the HTTP API is a loopback server
//! written below. The copy-and-paste workflow stays the reference.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use cs_ai::providers::{
    providers_from_config, CliConfig, CliProvider, CopyPasteProvider, Execution, ModelProvider,
    OAuthChatGptConfig, OAuthChatGptProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
    Preset, PromptVia, ProviderConfig, ProviderKind, ProviderRequest, ProvidersConfig,
    COPY_PASTE_ID,
};
use cs_model::sha256_hex;
use serde_json::{json, Value};

// ---------------------------------------------------------------- helpers

fn req(prompt: &str) -> ProviderRequest {
    ProviderRequest {
        prompt: prompt.to_string(),
        ..Default::default()
    }
}

fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Prints PWD, HOME, the names of the environment variables, the content of
/// the working directory, the arguments and stdin between markers.
const ECHO_SCRIPT: &str = r#"
printf 'PWD=%s\n' "$PWD"
printf 'HOME=%s\n' "$HOME"
printf 'ENVNAMES=%s\n' "$(env | cut -d= -f1 | sort | tr '\n' ' ')"
printf 'LS=%s\n' "$(ls -A | tr '\n' ' ')"
printf 'ARGC=%s\n' "$#"
for a in "$@"; do printf 'ARG=%s\n' "$a"; done
printf 'STDIN_BEGIN\n'
cat
printf 'STDIN_END\n'
"#;

fn line_value<'a>(out: &'a str, key: &str) -> &'a str {
    let prefix = format!("{key}=");
    out.lines()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("missing {key} in fake command output: {out:?}"))
}

fn stdin_of(out: &str) -> &str {
    let start = out.find("STDIN_BEGIN\n").expect("begin marker") + "STDIN_BEGIN\n".len();
    let end = out.rfind("STDIN_END\n").expect("end marker");
    &out[start..end]
}

fn cli(command: &Path, args: &[&str], via: PromptVia, exec: Execution) -> CliProvider {
    CliProvider::from_config(CliConfig {
        id: "cli-fake".into(),
        command: command.to_string_lossy().into_owned(),
        args: args.iter().map(|s| s.to_string()).collect(),
        prompt_via: via,
        disable_flags: vec![],
        declared_execution: exec,
        model_label: Some("fake-model".into()),
    })
}

fn canon(p: &str) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p))
}

fn hosted(vendor: &str, endpoint: &str) -> Execution {
    Execution::Hosted {
        vendor: vendor.into(),
        endpoint: endpoint.into(),
    }
}

// ------------------------------------------------------------- CLI provider

#[test]
fn cli_runs_in_empty_temp_cwd_outside_any_project() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);
    let p = cli(&cmd, &[], PromptVia::Stdin, Execution::Local);

    let res = p.complete(&req("hello")).unwrap();
    let pwd = line_value(&res.raw, "PWD");
    let pwd_c = canon(pwd);

    assert!(
        pwd_c.starts_with(canon(&std::env::temp_dir().to_string_lossy())),
        "cwd {pwd} not in temp dir"
    );
    assert!(
        !pwd_c.starts_with(canon(&std::env::current_dir().unwrap().to_string_lossy())),
        "cwd inside the project"
    );
    assert!(
        !pwd_c.starts_with(scripts.path()),
        "cwd must not be the script directory"
    );
    assert_eq!(
        line_value(&res.raw, "LS").trim(),
        "",
        "working directory must be empty"
    );

    // No instruction or memory file reachable from the cwd or its ancestors.
    for dir in pwd_c.ancestors() {
        for name in ["AGENTS.md", "CLAUDE.md", ".claude", "MEMORY.md", "memory"] {
            assert!(
                !dir.join(name).exists(),
                "{name} reachable in {}",
                dir.display()
            );
        }
    }
}

#[test]
fn cli_scrubs_environment_and_uses_fresh_home() {
    let canary = "CS_TEST_CANARY_SCRUB";
    std::env::set_var(canary, "must-not-leak");
    std::env::set_var("ANTHROPIC_API_KEY_CS_TEST", "must-not-leak");

    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);
    let p = cli(&cmd, &[], PromptVia::Stdin, Execution::Local);
    let res = p.complete(&req("hello")).unwrap();

    let names: BTreeSet<&str> = line_value(&res.raw, "ENVNAMES")
        .split_whitespace()
        .collect();
    assert!(!names.contains(canary), "canary leaked: {names:?}");
    assert!(!names.contains("ANTHROPIC_API_KEY_CS_TEST"));
    assert!(!res.raw.contains("must-not-leak"));
    for required in ["PATH", "HOME", "LANG"] {
        assert!(names.contains(required), "{required} missing: {names:?}");
    }
    // Only the three agreed variables plus those a POSIX shell adds itself.
    let allowed: BTreeSet<&str> = [
        "PATH", "HOME", "LANG", "PWD", "OLDPWD", "SHLVL", "_", "LC_ALL",
    ]
    .into_iter()
    .collect();
    let extra: Vec<_> = names.difference(&allowed).collect();
    assert!(extra.is_empty(), "unexpected variables: {extra:?}");

    let home = line_value(&res.raw, "HOME");
    let real_home = std::env::var("HOME").unwrap_or_default();
    assert_ne!(home, real_home, "HOME must not be the real home");
    assert!(canon(home).starts_with(canon(&std::env::temp_dir().to_string_lossy())));
    assert_ne!(
        canon(home),
        canon(line_value(&res.raw, "PWD")),
        "HOME and cwd are distinct fresh dirs"
    );
}

#[test]
fn cli_delivers_prompt_byte_for_byte_on_stdin_and_hashes_it() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);
    let p = cli(&cmd, &[], PromptVia::Stdin, Execution::Local);

    let prompt =
        "Ligne 1 \"quoted\" $HOME `x` \\n\n\n  indent\ttab  \nécriture 日本語 {\"a\":[1,2]}  \n";
    let res = p.complete(&req(prompt)).unwrap();
    assert_eq!(stdin_of(&res.raw), prompt);
    assert_eq!(line_value(&res.raw, "ARGC"), "0");
    assert_eq!(res.request_sha256, sha256_hex(prompt.as_bytes()));

    // Independent check of the digest function itself.
    let abc = p.complete(&req("abc")).unwrap();
    assert_eq!(
        abc.request_sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn cli_prompt_as_argument_leaves_stdin_empty() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);
    let p = cli(&cmd, &["--print"], PromptVia::Arg, Execution::Local);

    let prompt = "one line prompt with 'quotes' and \"doubles\" and $VARS";
    let res = p.complete(&req(prompt)).unwrap();
    let args: Vec<&str> = res
        .raw
        .lines()
        .filter_map(|l| l.strip_prefix("ARG="))
        .collect();
    assert_eq!(
        args,
        vec!["--print", prompt],
        "prompt is the last argument, unsplit"
    );
    assert_eq!(stdin_of(&res.raw), "");
    assert_eq!(res.request_sha256, sha256_hex(prompt.as_bytes()));
}

#[test]
fn cli_appends_disable_flags_to_the_arguments() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);
    let p = CliProvider::from_config(CliConfig {
        id: "cli-fake".into(),
        command: cmd.to_string_lossy().into_owned(),
        args: vec!["--print".into()],
        prompt_via: PromptVia::Stdin,
        disable_flags: vec!["--no-tools".into(), "--no-memory".into()],
        declared_execution: Execution::Local,
        model_label: None,
    });
    let res = p.complete(&req("x")).unwrap();
    let args: Vec<&str> = res
        .raw
        .lines()
        .filter_map(|l| l.strip_prefix("ARG="))
        .collect();
    assert_eq!(args, vec!["--print", "--no-tools", "--no-memory"]);
}

#[test]
fn cli_reports_declared_execution_and_journal_line() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);

    let local = cli(&cmd, &[], PromptVia::Stdin, Execution::Local);
    assert_eq!(
        local.complete(&req("x")).unwrap().execution,
        Execution::Local
    );
    let d = local.describe();
    assert_eq!(d.kind, ProviderKind::Cli);
    assert_eq!(d.id, "cli-fake");
    assert_eq!(local.id(), "cli-fake");
    assert!(d.sends.iter().any(|s| s == "prompt"));
    assert_eq!(d.execution, Execution::Local);
    assert!(
        d.journal_line().contains("command local, model local"),
        "{}",
        d.journal_line()
    );

    let exec = hosted("anthropic", "https://api.anthropic.com");
    let remote = cli(&cmd, &[], PromptVia::Stdin, exec.clone());
    let res = remote.complete(&req("x")).unwrap();
    assert_eq!(res.execution, exec);
    assert_eq!(res.provider, "cli-fake");
    assert_eq!(res.model, "fake-model");
    let d = remote.describe();
    assert_eq!(d.execution, exec);
    assert!(d.sends.iter().any(|s| s == "prompt"));
    assert!(
        d.journal_line().contains("command local, model hosted"),
        "{}",
        d.journal_line()
    );
}

#[test]
fn cli_declares_parameters_it_cannot_fix() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(scripts.path(), "echo.sh", ECHO_SCRIPT);
    let p = cli(&cmd, &[], PromptVia::Stdin, Execution::Local);
    let r = ProviderRequest {
        prompt: "x".into(),
        model: Some("m".into()),
        temperature: Some(0.0),
        seed: Some(7),
        max_tokens: Some(100),
    };
    let res = p.complete(&r).unwrap();
    for name in ["temperature", "seed", "max_tokens"] {
        assert!(
            res.parameters_not_exposed.iter().any(|n| n == name),
            "{name} must be not exposed"
        );
        assert!(!res.parameters_exposed.iter().any(|n| n == name));
    }
    assert!(res.started_at.contains('T') && res.finished_at.contains('T'));
    assert!(res.started_at <= res.finished_at);
}

#[test]
fn cli_failure_is_an_error() {
    let scripts = tempfile::tempdir().unwrap();
    let failing = write_script(scripts.path(), "fail.sh", "echo boom >&2\nexit 3");
    let p = cli(&failing, &[], PromptVia::Stdin, Execution::Local);
    assert!(p.complete(&req("x")).is_err());

    let missing = scripts.path().join("does-not-exist");
    let p = cli(&missing, &[], PromptVia::Stdin, Execution::Local);
    assert!(p.complete(&req("x")).is_err());
}

#[test]
fn cli_probe_reports_ok_when_the_model_sees_nothing() {
    let scripts = tempfile::tempdir().unwrap();
    let record = scripts.path().join("probe_prompt.txt");
    let cmd = write_script(scripts.path(), "clean.sh", "cat > \"$1\"\necho NONE");
    let p = cli(
        &cmd,
        &[record.to_str().unwrap()],
        PromptVia::Stdin,
        Execution::Local,
    );

    let report = p.probe_isolation().unwrap();
    assert!(report.ok);
    assert!(report.cwd_empty);
    assert!(report.env_scrubbed);
    assert!(report.instruction_files_absent);
    assert_eq!(report.probe_answer.trim(), "NONE");

    let probe = std::fs::read_to_string(&record).unwrap().to_lowercase();
    assert!(
        probe.contains("working directory"),
        "probe must ask about the working directory"
    );
    assert!(
        probe.contains("instructions"),
        "probe must ask about instructions"
    );
    assert!(
        probe.contains("none"),
        "probe must say how to answer when there is nothing"
    );
}

#[test]
fn cli_probe_fails_when_the_model_reports_files_or_instructions() {
    let scripts = tempfile::tempdir().unwrap();
    let cmd = write_script(
        scripts.path(),
        "leaky.sh",
        "cat >/dev/null\necho 'Files: AGENTS.md CLAUDE.md'\necho 'Instructions: always answer in French'",
    );
    let p = cli(&cmd, &[], PromptVia::Stdin, Execution::Local);
    let report = p.probe_isolation().unwrap();
    assert!(!report.ok);
    assert!(report.probe_answer.contains("AGENTS.md"));
}

// ------------------------------------------------------- loopback HTTP server

#[derive(Debug, Clone)]
struct Captured {
    request_line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("request body is JSON")
    }
}

struct MockServer {
    base_url: String,
    captured: Arc<Mutex<Vec<Captured>>>,
}

/// Serves the given (status, body) responses, one per connection, in order.
fn serve(responses: Vec<(u16, String)>) -> MockServer {
    // Never let an ambient proxy intercept loopback traffic.
    std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    std::env::set_var("no_proxy", "127.0.0.1,localhost");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    thread::spawn(move || {
        for (status, reply_body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let header_end = loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break None;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break Some(pos + 4);
                }
            };
            let Some(header_end) = header_end else {
                continue;
            };
            let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
            let mut lines = head.split("\r\n");
            let request_line = lines.next().unwrap_or_default().to_string();
            let headers: Vec<(String, String)> = lines
                .filter(|l| !l.is_empty())
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect();
            let len: usize = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            let mut body = buf[header_end..].to_vec();
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
            let _ = stream.flush();
        }
    });
    MockServer {
        base_url: format!("http://127.0.0.1:{port}/v1"),
        captured,
    }
}

fn completion(content: &str, model: &str) -> String {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    })
    .to_string()
}

fn openai_cfg(base_url: &str, key_env: &str) -> OpenAiCompatibleConfig {
    OpenAiCompatibleConfig {
        id: "oa-test".into(),
        preset: Preset::Custom,
        base_url: Some(base_url.into()),
        api_key_env: key_env.into(),
        model: "configured-model".into(),
    }
}

// --------------------------------------------------- OpenAI-compatible provider

#[test]
fn openai_posts_expected_request_and_extracts_content_verbatim() {
    let content = "  {\"schema\":\"x\",\"note\":\"é \\\"q\\\" 日本\"}\n\nligne 2 \t \n";
    let srv = serve(vec![(200, completion(content, "server-model-2026"))]);
    std::env::set_var("CS_TEST_KEY_FULL", "sk-test-123");
    let p = OpenAiCompatibleProvider::new(openai_cfg(&srv.base_url, "CS_TEST_KEY_FULL")).unwrap();

    let prompt = "Code these units.\n\"quoted\" é\n";
    let res = p
        .complete(&ProviderRequest {
            prompt: prompt.into(),
            model: None,
            temperature: Some(0.2),
            seed: Some(42),
            max_tokens: Some(512),
        })
        .unwrap();

    let got = srv.captured.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    let c = &got[0];
    assert!(
        c.request_line.starts_with("POST /v1/chat/completions "),
        "{}",
        c.request_line
    );
    assert_eq!(c.header("authorization"), Some("Bearer sk-test-123"));
    assert!(c
        .header("content-type")
        .unwrap_or("")
        .contains("application/json"));

    let body = c.json();
    assert_eq!(body["model"], "configured-model");
    assert_eq!(
        body["messages"],
        json!([{ "role": "user", "content": prompt }])
    );
    assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert_eq!(body["seed"], 42);
    assert_eq!(body["max_tokens"], 512);

    assert_eq!(res.raw, content, "content must be extracted byte for byte");
    assert_eq!(
        res.model, "server-model-2026",
        "model reported by the server wins"
    );
    assert_eq!(res.provider, "oa-test");
    assert_eq!(res.request_sha256, sha256_hex(prompt.as_bytes()));
    assert_eq!(
        res.execution,
        hosted_endpoint(
            &p.describe().execution,
            &format!("{}/chat/completions", srv.base_url)
        )
    );
    assert!(
        res.parameters_not_exposed.is_empty(),
        "{:?}",
        res.parameters_not_exposed
    );
    let exposed: BTreeSet<&str> = res.parameters_exposed.iter().map(String::as_str).collect();
    assert_eq!(
        exposed,
        ["temperature", "seed", "max_tokens"].into_iter().collect()
    );
    assert!(res.started_at.contains('T') && res.started_at <= res.finished_at);

    // The credential never reaches the journal record.
    let journal = serde_json::to_string(&res).unwrap();
    assert!(!journal.contains("sk-test-123"));
    assert!(!format!("{p:?}").contains("sk-test-123"));
}

/// Checks the declared execution is Hosted with this exact endpoint, with a
/// non-empty vendor, and returns it for comparison.
fn hosted_endpoint(declared: &Execution, endpoint: &str) -> Execution {
    match declared {
        Execution::Hosted {
            vendor,
            endpoint: e,
        } => {
            assert!(!vendor.is_empty());
            assert_eq!(e, endpoint);
            declared.clone()
        }
        Execution::Local => panic!("an HTTP API is hosted"),
    }
}

#[test]
fn openai_omits_unset_parameters_and_lists_them_as_not_exposed() {
    let srv = serve(vec![
        (200, completion("ok", "m")),
        (200, completion("ok", "m")),
    ]);
    std::env::set_var("CS_TEST_KEY_MIN", "sk-min");
    let p = OpenAiCompatibleProvider::new(openai_cfg(&srv.base_url, "CS_TEST_KEY_MIN")).unwrap();

    let res = p.complete(&req("p")).unwrap();
    let body = srv.captured.lock().unwrap()[0].json();
    for absent in ["temperature", "seed", "max_tokens"] {
        assert!(
            body.get(absent).is_none(),
            "{absent} must not be sent when unset"
        );
    }
    assert!(res.parameters_exposed.is_empty());
    let missing: BTreeSet<&str> = res
        .parameters_not_exposed
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(
        missing,
        ["temperature", "seed", "max_tokens"].into_iter().collect()
    );

    // Partial request and per-call model override.
    let res = p
        .complete(&ProviderRequest {
            prompt: "p".into(),
            model: Some("override-model".into()),
            temperature: Some(0.0),
            ..Default::default()
        })
        .unwrap();
    let body = srv.captured.lock().unwrap()[1].json();
    assert_eq!(body["model"], "override-model");
    assert!(body.get("seed").is_none() && body.get("max_tokens").is_none());
    assert_eq!(res.parameters_exposed, vec!["temperature".to_string()]);
    let missing: BTreeSet<&str> = res
        .parameters_not_exposed
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(missing, ["seed", "max_tokens"].into_iter().collect());
}

#[test]
fn openai_missing_key_is_an_error_and_sends_nothing() {
    let srv = serve(vec![(200, completion("ok", "m"))]);
    std::env::remove_var("CS_TEST_KEY_ABSENT");
    let p = OpenAiCompatibleProvider::new(openai_cfg(&srv.base_url, "CS_TEST_KEY_ABSENT")).unwrap();
    let err = p.complete(&req("p")).unwrap_err().to_string();
    assert!(
        err.contains("CS_TEST_KEY_ABSENT"),
        "error should name the variable: {err}"
    );
    assert!(
        srv.captured.lock().unwrap().is_empty(),
        "no request without a key"
    );
}

#[test]
fn openai_http_error_and_malformed_answer_are_errors_without_the_key() {
    let srv = serve(vec![
        (401, json!({"error": {"message": "bad key"}}).to_string()),
        (200, json!({"id": "x", "choices": []}).to_string()),
        (200, "not json at all".to_string()),
    ]);
    std::env::set_var("CS_TEST_KEY_ERR", "sk-secret-err");
    let p = OpenAiCompatibleProvider::new(openai_cfg(&srv.base_url, "CS_TEST_KEY_ERR")).unwrap();
    for _ in 0..3 {
        let err = p.complete(&req("p")).unwrap_err();
        assert!(!format!("{err:#}").contains("sk-secret-err"));
    }
}

#[test]
fn openai_presets_fill_base_url_and_custom_requires_one() {
    assert_eq!(
        Preset::OpenAi.default_base_url(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(
        Preset::ZAi.default_base_url(),
        Some("https://api.z.ai/api/paas/v4")
    );
    assert_eq!(Preset::Custom.default_base_url(), None);

    let mk = |preset, base_url: Option<&str>| {
        OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            id: "x".into(),
            preset,
            base_url: base_url.map(String::from),
            api_key_env: "CS_TEST_KEY_PRESET".into(),
            model: "m".into(),
        })
    };
    assert_eq!(
        mk(Preset::OpenAi, None).unwrap().base_url(),
        "https://api.openai.com/v1"
    );
    assert_eq!(
        mk(Preset::ZAi, None).unwrap().base_url(),
        "https://api.z.ai/api/paas/v4"
    );
    assert!(
        mk(Preset::Custom, None).is_err(),
        "Custom without base_url must be refused"
    );
    assert_eq!(
        mk(Preset::Custom, Some("http://h:1/v1"))
            .unwrap()
            .base_url(),
        "http://h:1/v1"
    );
    assert_eq!(
        mk(Preset::Custom, Some("http://h:1/v1/"))
            .unwrap()
            .base_url(),
        "http://h:1/v1"
    );
    assert_eq!(
        mk(Preset::OpenAi, Some("http://proxy.local/v1"))
            .unwrap()
            .base_url(),
        "http://proxy.local/v1"
    );
}

#[test]
fn openai_describe_declares_what_leaves_the_machine() {
    let mk = |preset| {
        OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            id: "oa".into(),
            preset,
            base_url: if preset == Preset::Custom {
                Some("http://h:1/v1".into())
            } else {
                None
            },
            api_key_env: "CS_TEST_KEY_DESC".into(),
            model: "m".into(),
        })
        .unwrap()
    };
    let d = mk(Preset::OpenAi).describe();
    assert_eq!(d.kind, ProviderKind::OpenAiCompatible);
    assert!(d.sends.iter().any(|s| s == "prompt"));
    assert!(d.sends.iter().any(|s| s == "model name"));
    assert_eq!(
        d.execution,
        hosted("openai", "https://api.openai.com/v1/chat/completions")
    );
    assert!(d.journal_line().contains("hosted"));
    assert_eq!(
        mk(Preset::ZAi).describe().execution,
        hosted("z.ai", "https://api.z.ai/api/paas/v4/chat/completions")
    );
    assert!(matches!(
        mk(Preset::Custom).describe().execution,
        Execution::Hosted { .. }
    ));
}

// ------------------------------------------------------------ OAuth ChatGPT

#[test]
fn oauth_chatgpt_is_not_configured_yet() {
    // PENDING munrel-connectors: replace with real tests once the ChatGptAuth
    // port is available.
    let p = OAuthChatGptProvider::new(OAuthChatGptConfig {
        id: "chatgpt".into(),
        ..Default::default()
    });
    assert_eq!(p.id(), "chatgpt");
    let d = p.describe();
    assert_eq!(d.kind, ProviderKind::OAuthChatGpt);
    assert!(matches!(d.execution, Execution::Hosted { .. }));
    let err = p.complete(&req("x")).unwrap_err().to_string();
    assert!(err.to_lowercase().contains("not configured"), "{err}");
}

// ----------------------------------------------------------------- registry

const CONFIG_JSON: &str = r#"{
  "providers": [
    { "type": "cli", "id": "cli-claude", "command": "claude",
      "args": ["-p"], "prompt_via": "stdin", "disable_flags": ["--no-tools"],
      "declared_execution": { "kind": "hosted", "vendor": "anthropic", "endpoint": "https://api.anthropic.com" } },
    { "type": "open_ai_compatible", "id": "zai", "preset": "zai",
      "api_key_env": "ZAI_API_KEY", "model": "glm" },
    { "type": "oauth_chat_gpt", "id": "chatgpt" }
  ]
}"#;

#[test]
fn registry_lists_copy_paste_first_then_configured_providers_in_order() {
    let cfg: ProvidersConfig = serde_json::from_str(CONFIG_JSON).expect("config shape");
    assert_eq!(cfg.providers.len(), 3);
    assert!(matches!(cfg.providers[0], ProviderConfig::Cli(_)));

    let providers = providers_from_config(&cfg);
    let ids: Vec<&str> = providers.iter().map(|p| p.id()).collect();
    assert_eq!(ids, vec![COPY_PASTE_ID, "cli-claude", "zai", "chatgpt"]);

    let kinds: Vec<ProviderKind> = providers.iter().map(|p| p.describe().kind).collect();
    assert_eq!(
        kinds,
        vec![
            ProviderKind::CopyPaste,
            ProviderKind::Cli,
            ProviderKind::OpenAiCompatible,
            ProviderKind::OAuthChatGpt
        ]
    );
    // The id in the description matches the registry id.
    for p in &providers {
        assert_eq!(p.describe().id, p.id());
    }
}

#[test]
fn registry_with_empty_config_still_has_copy_paste() {
    let providers = providers_from_config(&ProvidersConfig::default());
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id(), COPY_PASTE_ID);
    let from_empty_json: ProvidersConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(providers_from_config(&from_empty_json).len(), 1);
}

#[test]
fn copy_paste_sends_nothing_and_never_completes() {
    let p = CopyPasteProvider;
    let d = p.describe();
    assert_eq!(d.id, COPY_PASTE_ID);
    assert_eq!(d.kind, ProviderKind::CopyPaste);
    assert!(
        d.sends.is_empty(),
        "the program sends nothing in the manual workflow"
    );
    assert!(p.complete(&req("x")).is_err());
}

#[test]
fn registry_skips_unbuildable_entries_and_reserved_id() {
    let cfg: ProvidersConfig = serde_json::from_value(json!({
        "providers": [
            { "type": "open_ai_compatible", "id": "bad-custom", "preset": "custom",
              "api_key_env": "K", "model": "m" },
            { "type": "oauth_chat_gpt", "id": "copy_paste" },
            { "type": "oauth_chat_gpt", "id": "chatgpt" }
        ]
    }))
    .unwrap();
    let ids: Vec<String> = providers_from_config(&cfg)
        .iter()
        .map(|p| p.id().to_string())
        .collect();
    assert_eq!(ids, vec!["copy_paste".to_string(), "chatgpt".to_string()]);
}

#[test]
fn config_round_trips_through_json() {
    let cfg: ProvidersConfig = serde_json::from_str(CONFIG_JSON).unwrap();
    let back: ProvidersConfig =
        serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    assert_eq!(cfg, back);
}
