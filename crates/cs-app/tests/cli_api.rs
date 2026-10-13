//! Command line subcommands (F-109) and the example project (F-113).

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cs_app::cli::{self, Command, ConfigCmd, ExampleCmd, Format, ProjectCmd, QdpxCmd};
use cs_app::{prepare, router, AppState, Startup};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn state(dir: &Path) -> AppState {
    prepare(
        Startup {
            db: dir.join("studio/cli.db"),
            library: None,
            coding_db: None,
            actor: "candidate".into(),
            interviewer_names: vec![],
        },
        None,
    )
    .unwrap()
}

async fn run(st: &AppState, c: Command) -> cli::Outcome {
    cli::run(st, &c).await.unwrap()
}

async fn http(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn load_cmd(library: &Path) -> Command {
    Command::Example {
        action: ExampleCmd::Load {
            library: Some(library.to_path_buf()),
        },
    }
}

#[tokio::test]
async fn example_load_route_fills_an_empty_database_only() {
    let dir = tempfile::tempdir().unwrap();
    let st = state(dir.path());
    let app = router(st);
    let lib = dir.path().join("library");
    let (code, v) = http(
        &app,
        "POST",
        "/api/example/load",
        Some(json!({ "library": lib })),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{v}");
    assert_eq!(v["interview_id"], "INT-01");
    assert_eq!(v["codes"], 10);
    assert_eq!(v["passages"], 3);
    assert!(v["research_questions"].as_u64().unwrap() >= 3);
    assert!(v["units"].as_u64().unwrap() > 10);
    assert!(lib
        .join("INT-01_Example_Retailer/02_transcript_cleaned_diarised.md")
        .is_file());

    let (_, interviews) = http(&app, "GET", "/api/interviews", None).await;
    assert_eq!(interviews.as_array().unwrap().len(), 1);
    let (_, passages) = http(
        &app,
        "GET",
        "/api/interviews/INT-01/passages?kind=coded",
        None,
    )
    .await;
    assert_eq!(passages.as_array().map_or(0, Vec::len), 3, "{passages}");
    let (_, verify) = http(&app, "GET", "/api/audit/verify", None).await;
    assert_eq!(verify["ok"], true);

    // A second load is refused and changes nothing.
    let (code, v) = http(&app, "POST", "/api/example/load", Some(json!({}))).await;
    assert_eq!(code, StatusCode::CONFLICT, "{v}");
    let (_, interviews) = http(&app, "GET", "/api/interviews", None).await;
    assert_eq!(interviews.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn example_load_without_a_folder_uses_a_temporary_one() {
    let dir = tempfile::tempdir().unwrap();
    let app = router(state(dir.path()));
    let (code, v) = http(&app, "POST", "/api/example/load", None).await;
    assert_eq!(code, StatusCode::OK, "{v}");
    let lib = PathBuf::from(v["library"].as_str().unwrap());
    assert!(lib.starts_with(std::env::temp_dir()));
    assert!(lib.join("INT-01_Example_Retailer").is_dir());
}

#[tokio::test]
async fn scan_segment_export_and_verify_commands() {
    let dir = tempfile::tempdir().unwrap();
    let st = state(dir.path());

    let o = run(
        &st,
        Command::Scan {
            root: Some(examples().join("library")),
        },
    )
    .await;
    assert_eq!(o.code, 0);
    assert_eq!(o.value["interviews"].as_array().unwrap().len(), 1);
    let o = run(
        &st,
        Command::Segment {
            id: "INT-01".into(),
            source: None,
        },
    )
    .await;
    assert!(
        o.value["units"].as_array().unwrap().len() > 10,
        "{}",
        o.text
    );

    // An unknown interview is an error, not a panic.
    let err = cli::run(
        &st,
        &Command::Segment {
            id: "NOPE-1".into(),
            source: None,
        },
    )
    .await
    .unwrap_err();
    assert!(format!("{err:#}").contains("not found"), "{err:#}");

    let export = |target: &str, anonymised: bool, format: Format| Command::Export {
        target: target.into(),
        anonymised,
        format,
        researcher_names: vec![],
    };
    let o = run(&st, export("INT-01", false, Format::Md)).await;
    let dir_one = PathBuf::from(o.value["exports"][0]["result"]["dir"].as_str().unwrap());
    assert!(dir_one.join("manifest.json").is_file());
    let o = run(&st, export("all", false, Format::Md)).await;
    assert_eq!(
        o.value["exports"].as_array().unwrap().len(),
        2,
        "interview and cross-case"
    );
    let o = run(&st, export("all", true, Format::Md)).await;
    assert_eq!(o.value["exports"].as_array().unwrap().len(), 2);
    assert!(o.text.contains("anonymised"));
    let o = run(&st, export("all", false, Format::Docx)).await;
    let docx = PathBuf::from(o.value["file"].as_str().unwrap());
    assert!(std::fs::read(&docx).unwrap().starts_with(b"PK"));
    assert!(cli::run(&st, &export("INT-01", false, Format::Docx))
        .await
        .is_err());
    assert!(cli::run(&st, &export("all", true, Format::Docx))
        .await
        .is_err());

    let o = run(&st, Command::Verify).await;
    assert_eq!(o.code, 0, "{}", o.text);
    assert!(o.value["manifests"].as_array().unwrap().len() >= 4);

    // A file changed after the export fails the check.
    let victim = std::fs::read_dir(&dir_one)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "md"))
        .unwrap();
    std::fs::write(&victim, "changed").unwrap();
    let o = run(&st, Command::Verify).await;
    assert_eq!(o.code, 1);
    assert_eq!(o.value["ok"], false);
    assert!(o.text.contains("failing"));
}

#[tokio::test]
async fn project_qdpx_and_config_commands_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let st = state(dir.path());
    run(&st, load_cmd(&dir.path().join("library"))).await;

    // Whole project: export, dry run into a new database, apply.
    let bundle = dir.path().join("out/example.csproj.zip");
    let o = run(
        &st,
        Command::Project {
            action: ProjectCmd::Export {
                out: Some(bundle.clone()),
            },
        },
    )
    .await;
    assert_eq!(o.code, 0);
    assert!(bundle.is_file());
    let target = dir.path().join("copy/copy.db");
    let import = |apply| Command::Project {
        action: ProjectCmd::Import {
            file: bundle.clone(),
            apply,
            target: Some(target.clone()),
        },
    };
    let o = run(&st, import(false)).await;
    assert_eq!(o.code, 0, "{}", o.value);
    assert!(!target.exists(), "a dry run writes nothing");
    let o = run(&st, import(true)).await;
    assert_eq!(o.code, 0, "{}", o.value);
    assert!(target.is_file());
    // A damaged bundle is refused with a non-zero code.
    let bad = dir.path().join("bad.zip");
    std::fs::write(&bad, b"not a zip").unwrap();
    let refused = cli::run(
        &st,
        &Command::Project {
            action: ProjectCmd::Import {
                file: bad,
                apply: false,
                target: Some(dir.path().join("x.db")),
            },
        },
    )
    .await;
    assert!(refused.map_or(true, |o| o.code != 0));

    // QDPX.
    let qdpx = dir.path().join("out/example.qdpx");
    let o = run(
        &st,
        Command::Qdpx {
            action: QdpxCmd::Export {
                out: Some(qdpx.clone()),
                highlights: false,
            },
        },
    )
    .await;
    assert_eq!(o.code, 0);
    assert!(std::fs::read(&qdpx).unwrap().starts_with(b"PK"));
    let o = run(
        &st,
        Command::Qdpx {
            action: QdpxCmd::Import {
                file: qdpx,
                apply: false,
            },
        },
    )
    .await;
    assert_eq!(o.code, 0);
    assert_eq!(o.text, "dry run, nothing written");

    // Configuration.
    let cfg = dir.path().join("out/study.json");
    let o = run(
        &st,
        Command::Config {
            action: ConfigCmd::Export {
                out: Some(cfg.clone()),
            },
        },
    )
    .await;
    assert_eq!(o.code, 0);
    let mut v: Value = serde_json::from_slice(&std::fs::read(&cfg).unwrap()).unwrap();
    v["max_words"] = json!(120);
    std::fs::write(&cfg, serde_json::to_vec(&v).unwrap()).unwrap();
    run(
        &st,
        Command::Config {
            action: ConfigCmd::Import {
                file: cfg,
                force: false,
            },
        },
    )
    .await;
    let o = run(
        &st,
        Command::Config {
            action: ConfigCmd::Export { out: None },
        },
    )
    .await;
    assert_eq!(o.value["max_words"], 120);
}

#[test]
fn binary_runs_subcommands_with_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_coding-studio");
    let db = dir.path().join("bin.db");
    let lib = dir.path().join("lib");
    let out = std::process::Command::new(exe)
        .args(["--db"])
        .arg(&db)
        .args(["--json", "example", "load", "--library"])
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["interview_id"], "INT-01");

    let out = std::process::Command::new(exe)
        .args(["--db"])
        .arg(&db)
        .args(["--json", "verify"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);

    // A second load fails with a non-zero code and a JSON error.
    let out = std::process::Command::new(exe)
        .args(["--db"])
        .arg(&db)
        .args(["--json", "example", "load"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
}
