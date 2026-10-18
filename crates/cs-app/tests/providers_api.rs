#![cfg(unix)]
#![allow(clippy::type_complexity)]
//! Model providers through the HTTP API (v5): the registry, its stored
//! configuration, the isolation probe and the run of a whole prompt batch
//! through a provider. Red until implemented.
//!
//! Contracts fixed by these tests (see also the report of the test writer):
//! - `GET /api/providers` -> `[{id, kind, description, configured, config}]`,
//!   `copy_paste` first; `config` is the stored entry or null.
//! - `PUT /api/providers` `ProvidersConfig` -> the same list; settings key
//!   `providers_config`; audit `providers.update` `{before, after}`.
//! - `POST /api/providers/{id}/probe` -> `IsolationReport`.
//! - `POST /api/interviews/{id}/batches/{batch_id}/run` -> `{run: AiRun,
//!   journal_line}`; the run meta gains `provider`.
//! - `POST /api/providers/{id}/oauth/begin` -> `{url, state}`;
//!   `GET /api/providers/{id}/oauth/status` -> `{connected, pending, profile,
//!   error}`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use cs_app::{router, AppState};
use cs_store::Store;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const CLEANED: &str = "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Thank you for your time. Could you describe your role?\n\n**RESP `[00:05]`:** I run the logistics of a mid-sized retailer and our supplier tags every carton at the source.\n\n**INT `[00:20]`:** What made the business case work?\n\n**RESP `[00:24]`:** The inventory accuracy went from sixty to ninety-eight percent in one year.\n\n**INT `[00:40]`:** And the costs?\n\n**RESP `[00:43]`:** Tags were expensive at first, and our team lacked the skills for the ERP integration.\n";

fn make_codebook(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE codes (code TEXT PRIMARY KEY, family TEXT NOT NULL, label TEXT, definition TEXT,
           sub_question TEXT, cycle INTEGER DEFAULT 1, origin TEXT DEFAULT 'codebook',
           active INTEGER NOT NULL DEFAULT 1, notes TEXT);
         INSERT INTO codes (code,family,label,definition,sub_question,origin,active) VALUES
          ('TECH','TECH','Technology, unrefined','Family-level code.',NULL,'legacy',1),
          ('TECH-ADV','TECH','Relative advantage','Perceived advantage over the status quo.','SQ1','codebook',1),
          ('TECH-COST','TECH','Cost','Lifecycle cost and investment burden.','SQ1','codebook',1),
          ('ORG-SKILLS','ORG','Skills','Skills and expertise.','SQ2','codebook',1),
          ('ENV-PARTNER','ENV','Partner pressure','Partner pressure or mandate.','SQ3','codebook',1),
          ('OUT-REALISED','OUTCOME','Realised outcome','A result supported by evidence.','SQ1','codebook',1);",
    )
    .unwrap();
}

struct Ctx {
    app: Router,
    store: Arc<Mutex<Store>>,
    dir: tempfile::TempDir,
    root: PathBuf,
    coding_db: PathBuf,
}

fn setup() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Interviews");
    std::fs::create_dir_all(root.join("INT-01_Test")).unwrap();
    std::fs::write(
        root.join("INT-01_Test/02_transcript_cleaned_diarised.md"),
        CLEANED,
    )
    .unwrap();
    let coding_db = dir.path().join("coding.db");
    make_codebook(&coding_db);
    let db = dir.path().join("studio/coding_studio.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let store = Arc::new(Mutex::new(Store::open(&db).unwrap()));
    let state = AppState {
        store: store.clone(),
        db_path: db,
        coding_db: Some(coding_db.clone()),
        actor: "candidate".into(),
        interviewer_names: vec!["Interviewer Person A".into()],
        picker: None,
    };
    Ctx {
        app: router(state),
        store,
        dir,
        root,
        coding_db,
    }
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

/// Shell prelude shared by the fake clients: reads the prompt on stdin and
/// extracts what the prompt text offers (see `cs-ai/src/prompt.rs`): header
/// lines `interview_id:`, `segmentation_hash:`, `part: N of M`, codebook lines
/// `{"code":"..."` and unit lines `{"unit_id":"..."`.
const PRELUDE: &str = r#"#!/bin/sh
PROMPT=$(cat)
iid=$(printf '%s\n' "$PROMPT" | sed -n 's/^interview_id: //p' | head -1)
hash=$(printf '%s\n' "$PROMPT" | sed -n 's/^segmentation_hash: //p' | head -1)
pof=$(printf '%s\n' "$PROMPT" | sed -n 's/^part: \([0-9]*\) of \([0-9]*\)$/\1 \2/p' | head -1)
part=${pof% *}
of=${pof#* }
code=$(printf '%s\n' "$PROMPT" | sed -n 's/^{"code":"\([^"]*\)".*/\1/p' | head -1)
units=$(printf '%s\n' "$PROMPT" | sed -n 's/^{"unit_id":"\([^"]*\)".*/\1/p')
"#;

/// Valid answer: the first offered code for every unit of the part.
const VALID_BODY: &str = r#"
items=""
sep=""
for u in $units; do
  items="$items$sep{\"unit_id\":\"$u\",\"codes\":[\"$code\"]}"
  sep=","
done
printf '{"schema":"coding-studio/ai-coding/v1","interview_id":"%s","phase":2,"segmentation_hash":"%s","part":%s,"of":%s,"codings":[%s]}\n' "$iid" "$hash" "$part" "$of" "$items"
"#;

/// Isolation report of the process, as the model would see it. Prints NONE
/// when the working directory is empty, no CS_CANARY variable is set and no
/// CLAUDE.md or AGENTS.md exists in the working directory or its ancestors.
const SEEN: &str = r#"
seen=""
if [ -n "$(ls -A "$PWD")" ]; then seen="$seen cwd:$(ls -A "$PWD" | tr '\n' ' ')"; fi
canary=$(env | grep '^CS_CANARY' | tr '\n' ' ')
if [ -n "$canary" ]; then seen="$seen env:$canary"; fi
d="$PWD"
while :; do
  for n in CLAUDE.md AGENTS.md; do
    if [ -e "$d/$n" ]; then seen="$seen file:$d/$n"; fi
  done
  if [ "$d" = / ]; then break; fi
  d=$(dirname "$d")
done
"#;

#[derive(Clone, Copy)]
enum Behaviour {
    /// Probe prompt: NONE or what is visible. Coding prompt: a valid answer.
    Valid,
    /// Always prose, never JSON.
    Garbage,
    /// Valid, except that part 2 is prose.
    GarbageOnPart2,
    /// Prints the isolation facts for any prompt, whatever it is.
    Echo,
    /// Exits with an error.
    Fails,
}

fn write_script(dir: &Path, name: &str, behaviour: Behaviour) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join(name);
    let tail = match behaviour {
        Behaviour::Valid => format!(
            "case \"$PROMPT\" in\n  *\"This is a check of your environment\"*)\n{SEEN}\n    if [ -z \"$seen\" ]; then echo NONE; else echo \"$seen\"; fi\n    exit 0;;\nesac\n{VALID_BODY}"
        ),
        Behaviour::Garbage => "echo 'I am sorry, I cannot code these units.'\n".to_string(),
        Behaviour::GarbageOnPart2 => format!(
            "if [ \"$part\" = \"2\" ]; then echo 'I am sorry, I cannot code these units.'; exit 0; fi\n{VALID_BODY}"
        ),
        Behaviour::Echo => format!(
            "{SEEN}\necho \"cwd_entries=$(ls -A \"$PWD\" | wc -l | tr -d ' ')\"\necho \"canary=[$canary]\"\necho \"seen=[$seen]\"\n"
        ),
        Behaviour::Fails => "echo 'boom' >&2\nexit 3\n".to_string(),
    };
    let text = format!("{PRELUDE}{tail}");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
        f.sync_all().unwrap();
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn cli_entry(id: &str, script: &Path) -> Value {
    json!({
        "type": "cli",
        "id": id,
        "command": script,
        "args": [],
        "prompt_via": "stdin",
        "disable_flags": [],
        "declared_execution": { "kind": "hosted", "vendor": "fake-vendor", "endpoint": "https://fake.example/v1" },
        "model_label": "fake-model-1"
    })
}

async fn put_providers(ctx: &Ctx, entries: Vec<Value>) -> (StatusCode, Value) {
    call(
        &ctx.app,
        "PUT",
        "/api/providers",
        Some(json!({ "providers": entries })),
    )
    .await
}

async fn bootstrap_two_units(ctx: &Ctx) -> Vec<String> {
    let app = &ctx.app;
    let (st, b) = call(
        app,
        "POST",
        "/api/codebook/import",
        Some(json!({ "coding_db": ctx.coding_db })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, s) = call(
        app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{s}");
    let (st, seg) = call(app, "POST", "/api/interviews/INT-01/segment", None).await;
    assert_eq!(st, StatusCode::OK, "{seg}");
    let units: Vec<String> = seg["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["unit_id"].as_str().unwrap().to_string())
        .collect();
    for (i, c) in [
        (0, "NA"),
        (1, "NA"),
        (2, "NA"),
        (3, "TECH"),
        (4, "NA"),
        (5, "TECH"),
    ] {
        let (st, r) = call(
            app,
            "PUT",
            &format!("/api/interviews/INT-01/phases/1/coding/{}", units[i]),
            Some(json!({ "codes": [c] })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }
    units
}

async fn code_phase2(app: &Router, unit: &str, list: &[&str]) {
    let (st, r) = call(
        app,
        "PUT",
        &format!("/api/interviews/INT-01/phases/2/coding/{unit}"),
        Some(json!({ "codes": list })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
}

/// Two coded units in scope, then a prompt batch of `batch_size`. Returns the
/// units (u4, u6) and the batch parts.
async fn frozen_batch(ctx: &Ctx, batch_size: usize) -> (String, String, Vec<Value>) {
    let units = bootstrap_two_units(ctx).await;
    let (u4, u6) = (units[3].clone(), units[5].clone());
    code_phase2(&ctx.app, &u4, &["TECH-ADV", "OUT-REALISED"]).await;
    code_phase2(&ctx.app, &u6, &["TECH-COST", "ORG-SKILLS"]).await;
    let (st, parts) = call(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/phases/2/prompts",
        Some(json!({ "batch_size": batch_size })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{parts}");
    let path = "/api/anonymisation/interviews/INT-01";
    let (_, p) = call(&ctx.app, "GET", path, None).await;
    assert_eq!(
        call(
            &ctx.app,
            "PUT",
            path,
            Some(json!({"digest":p["digest"],"reviewed":true}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let bid = parts[0]["batch_id"].as_i64().unwrap();
    let path = format!("/api/anonymisation/interviews/INT-01/batches/{bid}");
    let (_, p) = call(&ctx.app, "GET", &path, None).await;
    assert_eq!(
        call(
            &ctx.app,
            "PUT",
            &path,
            Some(json!({"digest":p["digest"],"reviewed":true}))
        )
        .await
        .0,
        StatusCode::OK
    );
    (u4, u6, parts.as_array().unwrap().clone())
}

fn batch_id_of(parts: &[Value]) -> i64 {
    parts[0]["batch_id"].as_i64().unwrap()
}

fn run_uri(batch_id: i64) -> String {
    format!("/api/interviews/INT-01/batches/{batch_id}/run")
}

async fn runs(ctx: &Ctx) -> Vec<Value> {
    let (st, r) = call(
        &ctx.app,
        "GET",
        "/api/interviews/INT-01/phases/2/runs",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    r.as_array().unwrap().clone()
}

async fn audit_actions(ctx: &Ctx) -> Vec<Value> {
    let (st, ev) = call(&ctx.app, "GET", "/api/audit", None).await;
    assert_eq!(st, StatusCode::OK, "{ev}");
    ev.as_array().unwrap().clone()
}

// ------------------------------------------------------------- registry

#[tokio::test]
async fn registry_lists_copy_paste_first_even_when_empty() {
    let ctx = setup();
    let (st, list) = call(&ctx.app, "GET", "/api/providers", None).await;
    assert_eq!(st, StatusCode::OK, "{list}");
    let list = list.as_array().expect("an array");
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["id"], "copy_paste");
    assert_eq!(list[0]["kind"], "copy_paste");
    assert_eq!(list[0]["configured"], true);
    assert_eq!(list[0]["description"]["id"], "copy_paste");
    assert_eq!(list[0]["description"]["kind"], "copy_paste");
    assert_eq!(list[0]["description"]["sends"], json!([]));
    assert_eq!(
        list[0]["description"]["execution"],
        json!({ "kind": "local" })
    );
    assert!(list[0]["config"].is_null(), "{list:?}");
}

#[tokio::test]
async fn put_providers_stores_audits_and_lists_without_secrets() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    // The key is set in the environment: only the variable name may appear.
    std::env::set_var("CS_TEST_KEY_PUT", "sk-SECRET-VALUE-123");
    let token_dir = ctx.dir.path().join("tokens");
    std::fs::create_dir_all(&token_dir).unwrap();
    let entries = vec![
        cli_entry("fake_cli", &script),
        json!({ "type": "open_ai_compatible", "id": "gpt", "preset": "openai",
                "api_key_env": "CS_TEST_KEY_PUT", "model": "gpt-4o" }),
        json!({ "type": "open_ai_compatible", "id": "nokey", "preset": "zai",
                "api_key_env": "CS_TEST_KEY_UNSET_XYZ", "model": "glm" }),
        json!({ "type": "oauth_chat_gpt", "id": "chatgpt", "profile_id": null,
                "model": "gpt-5", "token_dir": token_dir }),
        cli_entry("missing_cli", Path::new("/nonexistent/bin/nothing")),
    ];
    let (st, list) = put_providers(&ctx, entries.clone()).await;
    assert_eq!(st, StatusCode::OK, "{list}");

    let (st, got) = call(&ctx.app, "GET", "/api/providers", None).await;
    assert_eq!(st, StatusCode::OK, "{got}");
    assert_eq!(got, list, "PUT answers with the same list as GET");
    let ids: Vec<&str> = got
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "copy_paste",
            "fake_cli",
            "gpt",
            "nokey",
            "chatgpt",
            "missing_cli"
        ]
    );
    let by = |id: &str| -> &Value {
        got.as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap()
    };
    assert_eq!(by("fake_cli")["kind"], "cli");
    assert_eq!(by("fake_cli")["configured"], true);
    assert_eq!(
        by("fake_cli")["description"]["execution"],
        json!({ "kind": "hosted", "vendor": "fake-vendor", "endpoint": "https://fake.example/v1" })
    );
    assert_eq!(by("fake_cli")["description"]["sends"], json!(["prompt"]));
    assert_eq!(by("gpt")["kind"], "open_ai_compatible");
    assert_eq!(by("gpt")["configured"], true, "key variable is set");
    assert_eq!(by("nokey")["configured"], false, "key variable is not set");
    assert_eq!(by("chatgpt")["kind"], "oauth_chat_gpt");
    assert_eq!(
        by("chatgpt")["configured"],
        false,
        "no profile connected yet"
    );
    assert_eq!(by("missing_cli")["configured"], false, "command not found");
    assert_eq!(by("gpt")["config"]["api_key_env"], "CS_TEST_KEY_PUT");

    // No secret anywhere in the answer; the name of the variable may appear.
    assert!(!got.to_string().contains("sk-SECRET-VALUE-123"), "{got}");

    // Stored as JSON under the settings key, in the order given.
    let stored = ctx
        .store
        .lock()
        .unwrap()
        .setting("providers_config")
        .unwrap()
        .expect("providers_config is stored");
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored, json!({ "providers": entries }));
    assert!(!stored.to_string().contains("sk-SECRET-VALUE-123"));

    // Audited with before and after.
    let ev = audit_actions(&ctx).await;
    let e = ev
        .iter()
        .find(|e| e["action"] == "providers.update")
        .unwrap_or_else(|| panic!("no providers.update event: {ev:?}"));
    assert_eq!(e["actor"], "candidate");
    assert_eq!(e["detail"]["before"], json!({ "providers": [] }));
    assert_eq!(e["detail"]["after"], json!({ "providers": entries }));

    // A second update records the previous configuration as before.
    let (st, _) = put_providers(&ctx, vec![entries[0].clone()]).await;
    assert_eq!(st, StatusCode::OK);
    let ev = audit_actions(&ctx).await;
    let last = ev
        .iter()
        .rev()
        .find(|e| e["action"] == "providers.update")
        .unwrap();
    assert_eq!(last["detail"]["before"], json!({ "providers": entries }));
    assert_eq!(
        last["detail"]["after"],
        json!({ "providers": [entries[0].clone()] })
    );
    let (_, chain) = call(&ctx.app, "GET", "/api/audit/verify", None).await;
    assert_eq!(chain["ok"], true, "{chain}");
}

#[tokio::test]
async fn put_providers_rejects_unbuildable_entries() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    let good = cli_entry("good", &script);
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({ "type": "open_ai_compatible", "id": "c", "preset": "custom",
                    "api_key_env": "K", "model": "m" }),
            "base_url",
        ),
        (cli_entry("copy_paste", &script), "copy_paste"),
        (cli_entry("", &script), "id"),
    ];
    for (bad, needle) in cases {
        let (st, e) = put_providers(&ctx, vec![good.clone(), bad.clone()]).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}: {e}");
        assert_eq!(e["kind"], "validation", "{e}");
        assert!(e["error"].as_str().unwrap().contains(needle), "{bad}: {e}");
    }
    let (st, e) = put_providers(&ctx, vec![good.clone(), good.clone()]).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "duplicate ids: {e}");
    assert!(e["error"].as_str().unwrap().contains("duplicate"), "{e}");

    // Nothing was stored, nothing audited.
    assert!(ctx
        .store
        .lock()
        .unwrap()
        .setting("providers_config")
        .unwrap()
        .is_none());
    let ev = audit_actions(&ctx).await;
    assert!(ev.iter().all(|e| e["action"] != "providers.update"));
    let (_, list) = call(&ctx.app, "GET", "/api/providers", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------- probe

#[tokio::test]
async fn probe_runs_the_isolation_probe_of_a_cli_provider() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    // Instruction files next to the script and a canary in our own
    // environment: neither may reach the child.
    std::fs::write(script.parent().unwrap().join("CLAUDE.md"), "SECRET RULES").unwrap();
    std::env::set_var("CS_CANARY_PROBE", "canary-probe-7f3a");
    let (st, _) = put_providers(&ctx, vec![cli_entry("fake_cli", &script)]).await;
    assert_eq!(st, StatusCode::OK);

    let (st, rep) = call(&ctx.app, "POST", "/api/providers/fake_cli/probe", None).await;
    assert_eq!(st, StatusCode::OK, "{rep}");
    assert_eq!(rep["probe_answer"], "NONE", "{rep}");
    assert_eq!(rep["ok"], true, "{rep}");
    assert_eq!(rep["cwd_empty"], true, "{rep}");
    assert_eq!(rep["env_scrubbed"], true, "{rep}");
    assert_eq!(rep["instruction_files_absent"], true, "{rep}");
    assert!(!rep.to_string().contains("canary-probe-7f3a"));
    assert!(!rep.to_string().contains("SECRET RULES"));
}

#[tokio::test]
async fn probe_does_not_apply_to_api_providers_or_copy_paste() {
    let ctx = setup();
    let (st, _) = put_providers(
        &ctx,
        vec![
            json!({ "type": "open_ai_compatible", "id": "gpt", "preset": "openai",
                    "api_key_env": "CS_TEST_KEY_X", "model": "gpt-4o" }),
            json!({ "type": "oauth_chat_gpt", "id": "chatgpt", "model": "gpt-5" }),
        ],
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    for id in ["gpt", "chatgpt", "copy_paste"] {
        let (st, e) = call(
            &ctx.app,
            "POST",
            &format!("/api/providers/{id}/probe"),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{id}: {e}");
        assert_eq!(e["kind"], "validation");
        assert!(
            e["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("does not apply"),
            "{id}: {e}"
        );
    }
    let (st, e) = call(&ctx.app, "POST", "/api/providers/unknown/probe", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{e}");
}

// ------------------------------------------------------------ run a batch

#[tokio::test]
async fn run_batch_through_a_cli_provider_records_everything() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    let (st, _) = put_providers(&ctx, vec![cli_entry("fake_cli", &script)]).await;
    assert_eq!(st, StatusCode::OK);
    // One unit per part: two parts, so the per-part records can be checked.
    let (u4, u6, parts) = frozen_batch(&ctx, 1).await;
    assert_eq!(parts.len(), 2);
    let batch = batch_id_of(&parts);

    let (st, out) = call(
        &ctx.app,
        "POST",
        &run_uri(batch),
        Some(json!({
            "provider_id": "fake_cli", "run_label": "R1",
            "temperature": 0.2, "seed": 7, "max_tokens": 512
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{out}");
    assert_eq!(out["journal_line"], "command local, model hosted", "{out}");
    let run = &out["run"];
    assert_eq!(run["complete"], true, "{run}");
    assert_eq!(run["parts_received"], json!([1, 2]));
    assert_eq!(run["parts_expected"], 2);
    assert_eq!(run["batch_id"], batch);
    assert_eq!(run["codings"][&u4], json!(["TECH-ADV"]));
    assert_eq!(run["codings"][&u6], json!(["TECH-ADV"]));

    // Meta: model name from the response, parameters honestly split.
    let meta = &run["meta"];
    assert_eq!(meta["run_label"], "R1");
    assert_eq!(meta["model_name"], "fake-model-1");
    assert_eq!(meta["prompt_id_version"], parts[0]["prompt_version"]);
    assert_eq!(meta["batch_id"], batch);
    assert!(meta["temperature"].is_null(), "a CLI cannot set it: {meta}");
    let prov = &meta["provider"];
    assert_eq!(prov["id"], "fake_cli", "{meta}");
    assert_eq!(
        prov["execution"],
        json!({ "kind": "hosted", "vendor": "fake-vendor", "endpoint": "https://fake.example/v1" })
    );
    for p in ["temperature", "seed", "max_tokens"] {
        let missing = prov["parameters_not_exposed"].as_array().unwrap();
        assert!(missing.iter().any(|v| v == p), "{p} not exposed: {prov}");
    }
    assert_eq!(prov["parameters_exposed"], json!([]));
    let per_part = prov["parts"].as_array().unwrap();
    assert_eq!(per_part.len(), 2, "{prov}");
    for (i, p) in per_part.iter().enumerate() {
        assert_eq!(p["part"], (i + 1) as i64);
        assert_eq!(
            p["request_sha256"], parts[i]["sha256"],
            "the prompt is sent byte for byte"
        );
        assert!(p["started_at"].as_str().unwrap().contains('T'));
        assert!(p["finished_at"].as_str().unwrap().contains('T'));
    }

    // The same run is listed.
    let all = runs(&ctx).await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["meta"]["provider"]["id"], "fake_cli");

    // The review shows the disagreements with the human reference.
    let (st, review) = call(&ctx.app, "GET", "/api/interviews/INT-01/review", None).await;
    assert_eq!(st, StatusCode::OK, "{review}");
    assert_eq!(review["runs"], json!(["R1"]));
    assert!(review["n_disagreements"].as_i64().unwrap() >= 1, "{review}");
    let item = review["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["unit_id"] == u6.as_str())
        .unwrap();
    assert_eq!(item["kind"], "disagreement");
    assert_eq!(item["ai"]["R1"], json!(["TECH-ADV"]));

    // Audited with the journal line.
    let ev = audit_actions(&ctx).await;
    let e = ev
        .iter()
        .find(|e| e["action"] == "ai_run.provider")
        .unwrap_or_else(|| panic!("no ai_run.provider event: {ev:?}"));
    assert_eq!(e["interview_id"], "INT-01");
    assert_eq!(e["detail"]["provider_id"], "fake_cli");
    assert_eq!(e["detail"]["journal_line"], "command local, model hosted");
    assert_eq!(e["detail"]["batch_id"], batch);

    // Export: raw answers byte for byte next to the pasted ones, and the run
    // json carries the provider record.
    let (st, exp) = call(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    assert_eq!(st, StatusCode::OK, "{exp}");
    let dir = PathBuf::from(exp["dir"].as_str().unwrap());
    let run_id = run["run_id"].as_i64().unwrap();
    for (n, unit) in [(1, &u4), (2, &u6)] {
        let raw =
            std::fs::read_to_string(dir.join(format!("phase2_ai_run{run_id}_R1_part{n}_raw.txt")))
                .unwrap_or_else(|e| panic!("raw answer of part {n}: {e}"));
        assert!(
            raw.ends_with("}\n"),
            "kept as returned, newline included: {raw:?}"
        );
        assert!(raw.contains(unit.as_str()), "{raw}");
        let parsed: Value = serde_json::from_str(raw.trim()).unwrap();
        assert_eq!(parsed["part"], n);
    }
    let meta_json: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("phase2_run{run_id}_R1.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(meta_json["meta"]["provider"]["id"], "fake_cli");
    assert_eq!(
        meta_json["meta"]["provider"]["execution"]["vendor"],
        "fake-vendor"
    );
    assert_eq!(
        meta_json["meta"]["provider"]["parts"][0]["request_sha256"],
        parts[0]["sha256"]
    );
    assert!(meta_json["meta"]["provider"]["parameters_not_exposed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "seed"));
}

#[tokio::test]
async fn invalid_answer_is_rejected_with_issues_and_nothing_is_kept() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "garbage-cli", Behaviour::Garbage);
    let (st, _) = put_providers(&ctx, vec![cli_entry("garbage", &script)]).await;
    assert_eq!(st, StatusCode::OK);
    let (_, _, parts) = frozen_batch(&ctx, 2).await;
    let before = audit_actions(&ctx).await.len();

    let (st, e) = call(
        &ctx.app,
        "POST",
        &run_uri(batch_id_of(&parts)),
        Some(json!({ "provider_id": "garbage", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(e["kind"], "validation");
    assert!(
        !e["issues"].as_array().expect("issues list").is_empty(),
        "{e}"
    );
    // The rejected answer is returned so that the candidate can read it.
    assert_eq!(
        e["rejected"][0]["raw"], "I am sorry, I cannot code these units.\n",
        "{e}"
    );
    assert_eq!(e["rejected"][0]["part"], 1);

    assert!(runs(&ctx).await.is_empty(), "no run is created");
    let ev = audit_actions(&ctx).await;
    assert!(ev.iter().all(|e| e["action"] != "ai_run.provider"));
    // Only failed attempts leave no write at all in the journal.
    assert_eq!(ev.len(), before, "{ev:?}");
}

#[tokio::test]
async fn invalid_second_part_leaves_no_partial_run() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "half-cli", Behaviour::GarbageOnPart2);
    let (st, _) = put_providers(&ctx, vec![cli_entry("half", &script)]).await;
    assert_eq!(st, StatusCode::OK);
    let (_, _, parts) = frozen_batch(&ctx, 1).await;
    assert_eq!(parts.len(), 2);

    let (st, e) = call(
        &ctx.app,
        "POST",
        &run_uri(batch_id_of(&parts)),
        Some(json!({ "provider_id": "half", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(e["rejected"][0]["part"], 2, "{e}");
    assert!(runs(&ctx).await.is_empty(), "part 1 is not kept either");
    let (_, review) = call(&ctx.app, "GET", "/api/interviews/INT-01/review", None).await;
    assert_eq!(review["runs"], json!([]), "{review}");
}

#[tokio::test]
async fn a_failing_command_creates_no_run() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fail-cli", Behaviour::Fails);
    let (st, _) = put_providers(&ctx, vec![cli_entry("fails", &script)]).await;
    assert_eq!(st, StatusCode::OK);
    let (_, _, parts) = frozen_batch(&ctx, 2).await;
    let (st, e) = call(
        &ctx.app,
        "POST",
        &run_uri(batch_id_of(&parts)),
        Some(json!({ "provider_id": "fails", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::INTERNAL_SERVER_ERROR, "{e}");
    assert_eq!(e["kind"], "internal");
    assert!(e["error"].as_str().unwrap().contains("exited"), "{e}");
    assert!(runs(&ctx).await.is_empty());
}

#[tokio::test]
async fn run_refusals() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    let (st, _) = put_providers(&ctx, vec![cli_entry("fake_cli", &script)]).await;
    assert_eq!(st, StatusCode::OK);

    // No prompt part for the batch: guard.
    let (st, e) = call(
        &ctx.app,
        "POST",
        &run_uri(1),
        Some(json!({ "provider_id": "fake_cli", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    assert_eq!(e["kind"], "guard");

    let (_, _, parts) = frozen_batch(&ctx, 2).await;
    let uri = run_uri(batch_id_of(&parts));

    // The manual workflow is not a provider to run.
    let (st, e) = call(
        &ctx.app,
        "POST",
        &uri,
        Some(json!({ "provider_id": "copy_paste", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert!(
        e["error"]
            .as_str()
            .unwrap()
            .contains("paste the answer instead"),
        "{e}"
    );

    let (st, e) = call(
        &ctx.app,
        "POST",
        &uri,
        Some(json!({ "provider_id": "nobody", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{e}");

    let (st, e) = call(
        &ctx.app,
        "POST",
        &uri,
        Some(json!({ "provider_id": "fake_cli", "run_label": "  " })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "a run label is required: {e}");

    let (st, e) = call(
        &ctx.app,
        "POST",
        "/api/interviews/NOPE-99/batches/1/run",
        Some(json!({ "provider_id": "fake_cli", "run_label": "R1" })),
    )
    .await;
    assert!(
        st == StatusCode::NOT_FOUND || st == StatusCode::CONFLICT,
        "{st} {e}"
    );
    assert!(runs(&ctx).await.is_empty());
}

// ------------------------------------------------------------- isolation

#[tokio::test]
async fn a_cli_provider_sees_an_empty_directory_and_no_canary() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "echo-cli", Behaviour::Echo);
    std::fs::write(script.parent().unwrap().join("CLAUDE.md"), "PROJECT RULES").unwrap();
    std::env::set_var("CS_CANARY_ECHO", "canary-echo-91bc");
    let (st, _) = put_providers(&ctx, vec![cli_entry("echo", &script)]).await;
    assert_eq!(st, StatusCode::OK);
    let (_, _, parts) = frozen_batch(&ctx, 2).await;

    // The echo script answers with what it sees, which no answer validates:
    // the rejected raw answer is returned in the 400 body.
    let (st, e) = call(
        &ctx.app,
        "POST",
        &run_uri(batch_id_of(&parts)),
        Some(json!({ "provider_id": "echo", "run_label": "R1" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    let raw = e["rejected"][0]["raw"]
        .as_str()
        .expect("rejected raw answer");
    assert!(
        raw.contains("cwd_entries=0"),
        "empty working directory: {raw}"
    );
    assert!(raw.contains("canary=[]"), "no canary variable: {raw}");
    assert!(raw.contains("seen=[]"), "nothing visible at all: {raw}");
    assert!(!raw.contains("canary-echo-91bc"), "{raw}");
    assert!(!raw.contains("PROJECT RULES"), "{raw}");
    assert!(runs(&ctx).await.is_empty());
}

// ----------------------------------------------------------------- oauth

#[tokio::test]
async fn oauth_routes_apply_to_the_chatgpt_provider_only() {
    let ctx = setup();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    let tokens = ctx.dir.path().join("tokens");
    std::fs::create_dir_all(&tokens).unwrap();
    let (st, _) = put_providers(
        &ctx,
        vec![
            cli_entry("fake_cli", &script),
            json!({ "type": "oauth_chat_gpt", "id": "chatgpt", "model": "gpt-5", "token_dir": tokens }),
            json!({ "type": "oauth_chat_gpt", "id": "notdir", "model": "gpt-5" }),
        ],
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    for (m, path) in [
        ("POST", "/api/providers/fake_cli/oauth/begin"),
        ("GET", "/api/providers/fake_cli/oauth/status"),
        ("POST", "/api/providers/copy_paste/oauth/begin"),
        ("POST", "/api/providers/notdir/oauth/begin"),
        ("GET", "/api/providers/notdir/oauth/status"),
    ] {
        let (st, e) = call(&ctx.app, m, path, None).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{m} {path}: {e}");
        assert_eq!(e["kind"], "validation");
    }
    let (st, _) = call(&ctx.app, "GET", "/api/providers/nobody/oauth/status", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Nothing connected yet and no sign-in under way.
    let (st, s) = call(&ctx.app, "GET", "/api/providers/chatgpt/oauth/status", None).await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(
        s,
        json!({ "connected": false, "pending": false, "profile": null, "error": null })
    );
}

// ---------------------------------------------------------- harness guard

fn run_script(script: &Path, prompt: &str) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let cwd = tempfile::tempdir().unwrap();
    // A clean environment, like the provider's: other tests set canaries.
    let mut child = Command::new(script)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .current_dir(cwd.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let bytes = prompt.as_bytes().to_vec();
    let t = std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let out = child.wait_with_output().unwrap();
    t.join().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

/// Regression guard for the harness itself (passes today): the fake client
/// extracts units and codes from a real generated prompt and answers in the
/// format the parser accepts; the probe and echo modes print what they see.
#[tokio::test]
async fn harness_fake_client_answers_a_real_prompt() {
    let ctx = setup();
    let (u4, u6, parts) = frozen_batch(&ctx, 0).await;
    let text = parts[0]["text"].as_str().unwrap();
    let script = write_script(ctx.dir.path(), "fake-cli", Behaviour::Valid);
    let raw = run_script(&script, text);
    let v: Value = serde_json::from_str(raw.trim()).unwrap_or_else(|e| panic!("{e}: {raw}"));
    assert_eq!(v["schema"], cs_ai::OUTPUT_SCHEMA_ID);
    assert_eq!(v["interview_id"], parts[0]["interview_id"]);
    assert_eq!(v["segmentation_hash"], parts[0]["segmentation_hash"]);
    assert_eq!((v["part"].clone(), v["of"].clone()), (json!(1), json!(1)));
    let got: Vec<&str> = v["codings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["unit_id"].as_str().unwrap())
        .collect();
    assert_eq!(got, [u4.as_str(), u6.as_str()]);
    assert_eq!(v["codings"][0]["codes"], json!(["TECH-ADV"]));
    let probe = run_script(&script, "This is a check of your environment, not a task.");
    assert_eq!(probe.trim(), "NONE");
    let echo = write_script(ctx.dir.path(), "echo-cli", Behaviour::Echo);
    let seen = run_script(&echo, text);
    assert!(seen.contains("cwd_entries=0"), "{seen}");
}

#[tokio::test]
async fn privacy_guard_blocks_copy_and_provider_before_invocation_and_after_context_change() {
    let ctx = setup();
    let source = ctx
        .root
        .join("INT-01_Test/02_transcript_cleaned_diarised.md");
    std::fs::write(
        &source,
        CLEANED.replace(
            "The inventory accuracy",
            "Sophie says the inventory accuracy",
        ),
    )
    .unwrap();
    assert_eq!(
        call(
            &ctx.app,
            "PUT",
            "/api/anonymisation",
            Some(
                json!({"entries":[{"kind":"person","original":"Sophie","pseudonym":"[Person 88]"}]})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, _, parts) = frozen_batch(&ctx, 0).await;
    let bid = batch_id_of(&parts);
    let script = write_script(ctx.dir.path(), "privacy-provider", Behaviour::Valid);
    let marker = ctx.dir.path().join("provider-invoked");
    let text = std::fs::read_to_string(&script).unwrap().replace(
        "PROMPT=$(cat)",
        &format!(
            "PROMPT=$(cat)\nprintf '%s' \"$PROMPT\" > '{}'",
            marker.display()
        ),
    );
    std::fs::write(&script, text).unwrap();
    assert_eq!(
        put_providers(&ctx, vec![cli_entry("privacy", &script)])
            .await
            .0,
        StatusCode::OK
    );
    // Clearing local review deliberately revokes every previous approval.
    assert_eq!(
        call(
            &ctx.app,
            "PUT",
            "/api/anonymisation",
            Some(
                json!({"entries":[{"kind":"person","original":"Sophie","pseudonym":"[Person 88]"}]})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let copy = format!("/api/anonymisation/interviews/INT-01/batches/{bid}/copy");
    assert_eq!(
        call(&ctx.app, "POST", &copy, None).await.0,
        StatusCode::CONFLICT
    );
    let body = json!({"provider_id":"privacy","run_label":"privacy-test"});
    assert_eq!(
        call(&ctx.app, "POST", &run_uri(bid), Some(body.clone()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert!(
        !marker.exists(),
        "provider must not even start before review"
    );
    let path = "/api/anonymisation/interviews/INT-01";
    let (_, p) = call(&ctx.app, "GET", path, None).await;
    call(
        &ctx.app,
        "PUT",
        path,
        Some(json!({"digest":p["digest"],"reviewed":true})),
    )
    .await;
    let path = format!("/api/anonymisation/interviews/INT-01/batches/{bid}");
    let (_, p) = call(&ctx.app, "GET", &path, None).await;
    call(
        &ctx.app,
        "PUT",
        &path,
        Some(json!({"digest":p["digest"],"reviewed":true})),
    )
    .await;
    assert_eq!(call(&ctx.app, "POST", &copy, None).await.0, StatusCode::OK);
    let approved = json!({"provider_id":"privacy","run_label":"approved"});
    let (status, result) = call(&ctx.app, "POST", &run_uri(bid), Some(approved)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let sent = std::fs::read_to_string(&marker).unwrap();
    assert!(!sent.contains("Sophie"));
    assert!(sent.contains("[Person 88]"));
    std::fs::remove_file(&marker).unwrap();
    // A changed study context invalidates prompt approval, although the
    // transcript itself is still reviewed. No real network/model is used.
    {
        let store = ctx.store.lock().unwrap();
        let mut study = store.study_config().unwrap();
        study.description.push_str("Synthetic new context");
        store.set_study_config(&study, "test").unwrap();
    }
    assert_eq!(
        call(&ctx.app, "POST", &copy, None).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(&ctx.app, "POST", &run_uri(bid), Some(body)).await.0,
        StatusCode::CONFLICT
    );
    assert!(!marker.exists());
}
