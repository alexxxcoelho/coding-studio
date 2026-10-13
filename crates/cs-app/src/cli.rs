//! Subcommands of the `coding-studio` binary. Each one calls the same routes
//! as the web UI, in process (no server is started), so that a command line
//! export is the export the UI makes. `serve` (the default) starts the server.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use clap::{Subcommand, ValueEnum};
use cs_model::sha256_hex;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::AppState;

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Start the local server and open the web UI (default).
    Serve,
    /// Scan the library folder and register the interviews found.
    Scan {
        /// Library folder; absent, the one saved with the project.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Segment one interview into units.
    Segment {
        /// Interview id, e.g. INT-01.
        id: String,
        /// Transcript to segment (path relative to the interview folder).
        #[arg(long)]
        source: Option<String>,
    },
    /// Write the export package of one interview, or of all of them.
    Export {
        /// An interview id, or `all` (every interview and the cross-case export).
        target: String,
        /// Pseudonymise names in the export.
        #[arg(long)]
        anonymised: bool,
        /// `md` (the package of files) or `docx` (the write-up as a Word document).
        #[arg(long, value_enum, default_value_t = Format::Md)]
        format: Format,
        /// Name under which the researcher appears (repeatable, with --anonymised).
        #[arg(long = "researcher-name")]
        researcher_names: Vec<String>,
    },
    /// Check the audit chain and the manifests of the exports already written.
    Verify,
    /// Whole-project bundle (.csproj.zip).
    Project {
        #[command(subcommand)]
        action: ProjectCmd,
    },
    /// REFI-QDA project exchange (.qdpx).
    Qdpx {
        #[command(subcommand)]
        action: QdpxCmd,
    },
    /// Study configuration (JSON).
    Config {
        #[command(subcommand)]
        action: ConfigCmd,
    },
    /// The fictional example project.
    Example {
        #[command(subcommand)]
        action: ExampleCmd,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProjectCmd {
    /// Write the bundle.
    Export {
        /// File or folder to write to; absent, the exports folder.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Check a bundle (dry run), or restore it with --apply.
    Import {
        file: PathBuf,
        #[arg(long)]
        apply: bool,
        /// New database to create; absent, the current one, which must be empty.
        #[arg(long)]
        target: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum QdpxCmd {
    Export {
        #[arg(long)]
        out: Option<PathBuf>,
        /// Export highlights under the code "Surlignages".
        #[arg(long)]
        highlights: bool,
    },
    /// Report what the file would change (dry run), or apply it with --apply.
    Import {
        file: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCmd {
    /// Print the study configuration, or write it to a file.
    Export {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Replace the study configuration with a JSON file.
    Import {
        file: PathBuf,
        /// Allow a change of the phase 1 category names after coding.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ExampleCmd {
    /// Fill an empty database with the example project.
    Load {
        /// Folder the example transcript is copied to; absent, a temporary one.
        #[arg(long)]
        library: Option<PathBuf>,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Md,
    Docx,
}

/// What a command did: the process exit code and what to print.
#[derive(Debug)]
pub struct Outcome {
    pub code: i32,
    /// The result, as JSON.
    pub value: Value,
    /// One-line summary for people.
    pub text: String,
}

struct Reply {
    status: StatusCode,
    disposition: Option<String>,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Result<Value> {
        serde_json::from_slice(&self.bytes).context("the answer is not JSON")
    }
    /// The JSON answer, or the error the route gave.
    fn ok_json(&self) -> Result<Value> {
        let v: Value = self.json().unwrap_or(Value::Null);
        if !self.status.is_success() {
            let msg = v["error"].as_str().unwrap_or("request failed");
            let mut text = format!("{msg} (HTTP {})", self.status.as_u16());
            if let Some(issues) = v["issues"].as_array() {
                for i in issues {
                    text.push_str(&format!(
                        "\n  {}: {}",
                        i["location"].as_str().unwrap_or(""),
                        i["message"].as_str().unwrap_or("")
                    ));
                }
            }
            bail!(text);
        }
        Ok(v)
    }
    fn file_name(&self) -> Option<String> {
        let d = self.disposition.as_deref()?;
        let start = d.find("filename=\"")? + 10;
        let end = d[start..].find('"')?;
        Some(d[start..start + end].to_string())
    }
}

async fn call(
    st: &AppState,
    method: Method,
    uri: &str,
    json_body: Option<Value>,
    raw: Option<Vec<u8>>,
) -> Result<Reply> {
    let mut req = Request::builder().method(method).uri(uri);
    let body = match (json_body, raw) {
        (Some(v), _) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        (None, Some(b)) => Body::from(b),
        _ => Body::empty(),
    };
    let res = crate::router(st.clone())
        .oneshot(req.body(body)?)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let status = res.status();
    let disposition = res
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(res.into_body(), usize::MAX).await?.to_vec();
    Ok(Reply {
        status,
        disposition,
        bytes,
    })
}

fn exports_dir(st: &AppState) -> PathBuf {
    crate::v2::exports_base(&st.db_path)
}

/// Writes `bytes` to `out` (a file, or a folder that receives `default_name`),
/// or into the exports folder. Returns the path and the SHA-256.
fn save(
    st: &AppState,
    out: Option<&Path>,
    default_name: &str,
    bytes: &[u8],
) -> Result<(PathBuf, String)> {
    let path = match out {
        Some(p) if p.is_dir() => p.join(default_name),
        Some(p) => p.to_path_buf(),
        None => exports_dir(st).join(default_name),
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok((path, sha256_hex(bytes)))
}

fn done(value: Value, text: impl Into<String>) -> Result<Outcome> {
    Ok(Outcome {
        code: 0,
        value,
        text: text.into(),
    })
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Runs a subcommand other than `serve`.
pub async fn run(st: &AppState, command: &Command) -> Result<Outcome> {
    match command {
        Command::Serve => bail!("serve starts the server; it is not run as a command"),
        Command::Scan { root } => {
            let body = json!({ "root": root.as_ref().map(|r| r.display().to_string()) });
            let v = call(st, Method::POST, "/api/library/scan", Some(body), None)
                .await?
                .ok_json()?;
            let n = v["interviews"].as_array().map_or(0, Vec::len);
            done(v, format!("{n} interview(s) in the library"))
        }
        Command::Segment { id, source } => {
            let body = json!({ "rel_path": source });
            let v = call(
                st,
                Method::POST,
                &format!("/api/interviews/{}/segment", encode(id)),
                Some(body),
                None,
            )
            .await?
            .ok_json()?;
            let n = v["units"].as_array().map_or(0, Vec::len);
            done(v, format!("{id}: {n} unit(s)"))
        }
        Command::Export {
            target,
            anonymised,
            format,
            researcher_names,
        } => export(st, target, *anonymised, *format, researcher_names).await,
        Command::Verify => verify(st).await,
        Command::Project { action } => match action {
            ProjectCmd::Export { out } => {
                let r = call(st, Method::GET, "/api/project/bundle", None, None).await?;
                if !r.status.is_success() {
                    r.ok_json()?;
                }
                let name = r.file_name().unwrap_or_else(|| "project.csproj.zip".into());
                let (path, sha) = save(st, out.as_deref(), &name, &r.bytes)?;
                done(
                    json!({ "file": path, "sha256": sha, "bytes": r.bytes.len() }),
                    format!("bundle written to {}", path.display()),
                )
            }
            ProjectCmd::Import {
                file,
                apply,
                target,
            } => {
                let mut uri = format!("/api/project/bundle/import?apply={apply}");
                if let Some(t) = target {
                    uri.push_str(&format!("&target={}", encode(&t.display().to_string())));
                }
                let v = call(st, Method::POST, &uri, None, Some(read(file)?))
                    .await?
                    .ok_json()?;
                let ok = v["ok"].as_bool().unwrap_or(false);
                let text = match (ok, *apply) {
                    (false, _) => "the bundle is refused, see the checks".to_string(),
                    (true, true) => "bundle imported".to_string(),
                    (true, false) => "the bundle can be imported (dry run, nothing written)".into(),
                };
                Ok(Outcome {
                    code: if ok { 0 } else { 1 },
                    value: v,
                    text,
                })
            }
        },
        Command::Qdpx { action } => match action {
            QdpxCmd::Export { out, highlights } => {
                let r = call(
                    st,
                    Method::GET,
                    &format!("/api/export/qdpx?highlights={highlights}"),
                    None,
                    None,
                )
                .await?;
                if !r.status.is_success() {
                    r.ok_json()?;
                }
                let name = r.file_name().unwrap_or_else(|| "project.qdpx".into());
                let (path, sha) = save(st, out.as_deref(), &name, &r.bytes)?;
                done(
                    json!({ "file": path, "sha256": sha, "bytes": r.bytes.len() }),
                    format!("QDPX written to {}", path.display()),
                )
            }
            QdpxCmd::Import { file, apply } => {
                let v = call(
                    st,
                    Method::POST,
                    &format!("/api/import/qdpx?apply={apply}"),
                    None,
                    Some(read(file)?),
                )
                .await?
                .ok_json()?;
                done(
                    v,
                    if *apply {
                        "QDPX imported"
                    } else {
                        "dry run, nothing written"
                    },
                )
            }
        },
        Command::Config { action } => match action {
            ConfigCmd::Export { out } => {
                let v = call(st, Method::GET, "/api/config", None, None)
                    .await?
                    .ok_json()?;
                match out {
                    Some(p) => {
                        let (path, _) = save(
                            st,
                            Some(p),
                            "study_config.json",
                            &serde_json::to_vec_pretty(&v)?,
                        )?;
                        done(v, format!("configuration written to {}", path.display()))
                    }
                    None => done(v.clone(), serde_json::to_string_pretty(&v)?),
                }
            }
            ConfigCmd::Import { file, force } => {
                let body: Value = serde_json::from_slice(&read(file)?)
                    .with_context(|| format!("{} is not JSON", file.display()))?;
                let v = call(
                    st,
                    Method::PUT,
                    &format!("/api/config?force={force}"),
                    Some(body),
                    None,
                )
                .await?
                .ok_json()?;
                done(v, "configuration stored")
            }
        },
        Command::Example { action } => match action {
            ExampleCmd::Load { library } => {
                let body = json!({ "library": library.as_ref().map(|p| p.display().to_string()) });
                let v = call(st, Method::POST, "/api/example/load", Some(body), None)
                    .await?
                    .ok_json()?;
                done(
                    v.clone(),
                    format!(
                        "example loaded: {} unit(s), {} code(s), {} passage(s); transcripts in {}",
                        v["units"], v["codes"], v["passages"], v["library"]
                    ),
                )
            }
        },
    }
}

async fn export(
    st: &AppState,
    target: &str,
    anonymised: bool,
    format: Format,
    researcher_names: &[String],
) -> Result<Outcome> {
    let all = target == "all";
    if format == Format::Docx {
        if anonymised {
            bail!("--anonymised works with --format md only");
        }
        if !all {
            bail!("the write-up covers the whole project: use `export all --format docx`");
        }
        let r = call(st, Method::GET, "/api/writing/export.docx", None, None).await?;
        if !r.status.is_success() {
            r.ok_json()?;
        }
        let (path, sha) = save(st, None, "writeup.docx", &r.bytes)?;
        return done(
            json!({ "file": path, "sha256": sha, "bytes": r.bytes.len() }),
            format!("write-up written to {}", path.display()),
        );
    }
    let mut ids: Vec<String> = Vec::new();
    if all {
        let v = call(st, Method::GET, "/api/interviews", None, None)
            .await?
            .ok_json()?;
        for s in v.as_array().into_iter().flatten() {
            if let Some(id) = s["interview_id"].as_str() {
                ids.push(id.to_string());
            }
        }
    } else {
        ids.push(target.to_string());
    }
    let mut results = Vec::new();
    for id in &ids {
        let r = if anonymised {
            let body = json!({
                "scope": "interview",
                "interview_id": id,
                "researcher_names": researcher_names,
            });
            call(st, Method::POST, "/api/export/anonymised", Some(body), None).await?
        } else {
            call(
                st,
                Method::POST,
                &format!("/api/interviews/{}/export", encode(id)),
                None,
                None,
            )
            .await?
        };
        results.push(entry(id, &r, all)?);
    }
    if all {
        let r = if anonymised {
            let body = json!({ "scope": "cross-case", "researcher_names": researcher_names });
            call(st, Method::POST, "/api/export/anonymised", Some(body), None).await?
        } else {
            call(st, Method::POST, "/api/export/cross-case", None, None).await?
        };
        results.push(entry("cross-case", &r, all)?);
    }
    let failed = results.iter().filter(|r| r.get("error").is_some()).count();
    let dirs: Vec<String> = results
        .iter()
        .filter_map(|r| r["result"]["dir"].as_str().map(str::to_string))
        .collect();
    Ok(Outcome {
        code: i32::from(failed > 0),
        value: json!({ "ok": failed == 0, "exports": results }),
        text: format!(
            "{} export(s) written, {failed} failed{}{}",
            results.len() - failed,
            if anonymised { " (anonymised)" } else { "" },
            dirs.first()
                .map(|d| format!(", the first in {d}"))
                .unwrap_or_default()
        ),
    })
}

/// One line of an export run. With `all`, an interview that cannot be
/// exported (no segmentation yet) is reported and the run goes on.
fn entry(scope: &str, r: &Reply, all: bool) -> Result<Value> {
    match r.ok_json() {
        Ok(v) => Ok(json!({ "scope": scope, "result": v })),
        Err(e) if all => Ok(json!({ "scope": scope, "error": format!("{e:#}") })),
        Err(e) => Err(e),
    }
}

/// Audit chain, then the manifest of every export folder under `exports/`.
async fn verify(st: &AppState) -> Result<Outcome> {
    let chain = call(st, Method::GET, "/api/audit/verify", None, None)
        .await?
        .ok_json()?;
    let mut ok = chain["ok"].as_bool().unwrap_or(false);
    let mut manifests = Vec::new();
    let mut stack = vec![exports_dir(st)];
    let mut found = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "manifest.json") {
                found.push(p);
            }
        }
    }
    found.sort();
    for m in found {
        let dir = m.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut problems: Vec<String> = Vec::new();
        match std::fs::read(&m)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            None => problems.push("manifest.json is not readable".into()),
            Some(v) => {
                let files = v["files"].as_array().cloned().unwrap_or_default();
                if files.is_empty() {
                    problems.push("the manifest lists no file".into());
                }
                for f in &files {
                    let name = f["name"].as_str().unwrap_or("");
                    match std::fs::read(dir.join(name)) {
                        Err(_) => problems.push(format!("{name}: missing")),
                        Ok(b) => {
                            if sha256_hex(&b) != f["sha256"].as_str().unwrap_or("") {
                                problems.push(format!("{name}: digest differs"));
                            }
                            if let Some(size) = f["size"].as_u64() {
                                if size != b.len() as u64 {
                                    problems.push(format!("{name}: size differs"));
                                }
                            }
                        }
                    }
                }
            }
        }
        ok &= problems.is_empty();
        manifests.push(json!({
            "dir": dir,
            "ok": problems.is_empty(),
            "problems": problems,
        }));
    }
    let n_bad = manifests.iter().filter(|m| m["ok"] == false).count();
    let text = format!(
        "audit chain {} ({} event(s)); {} manifest(s) checked, {} failing",
        if chain["ok"].as_bool().unwrap_or(false) {
            "intact"
        } else {
            "BROKEN"
        },
        chain["n_events"],
        manifests.len(),
        n_bad
    );
    Ok(Outcome {
        code: if ok { 0 } else { 1 },
        value: json!({ "ok": ok, "audit_chain": chain, "manifests": manifests }),
        text,
    })
}
