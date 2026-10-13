//! V1 lots 1 and 2 in the exported packages: new files, code tree in
//! `codebook.csv`, manifest digests that cover them, state digest that moves.

mod common;

use std::path::{Path, PathBuf};

use common::{call, code_id, ok, setup, unit_ids, Ctx};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const NEW_FILES: [&str; 6] = [
    "passages.csv",
    "highlights.csv",
    "attributes.csv",
    "memos.json",
    "study_config.json",
    "review.json",
];

fn sha(path: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}

/// A project with a coded passage, a highlight, a LATER marker, an attribute
/// value, a memo answer and a checked item.
async fn filled() -> Ctx {
    let ctx = setup().await;
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([{"id": "kind", "label": "Type", "kind": "choice", "choices": ["communale", "scolaire"]}]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/attributes",
        Some(json!({"values": {"kind": "communale"}})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/memo/answers",
        Some(json!({"answers": {"q1": "Elle gère, seule."}})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/checklist",
        Some(json!({"checked": {"turns_seen": true}, "note": "ok"})),
    )
    .await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let skills = code_id(&ctx.app, "ORG-SKILLS").await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{skills}/move"),
        Some(json!({"parent_id": adv, "rationale": "regroupement"})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/codes/{skills}"),
        Some(json!({"name": "Compétences"})),
    )
    .await;
    let units = unit_ids(&ctx.app, "INT-01").await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[1], "start": 2, "end": 20, "code": adv})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[3], "start": 0, "end": 10})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[5], "code": "Plus tard", "note": "revoir"})),
    )
    .await;
    ctx
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

#[tokio::test]
async fn the_interview_package_carries_the_new_files_and_the_manifest_covers_them() {
    let ctx = filled().await;
    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = PathBuf::from(exp["dir"].as_str().unwrap());
    let m = manifest(&dir);
    let files = m["files"].as_array().unwrap();
    for f in NEW_FILES {
        assert!(dir.join(f).is_file(), "missing {f}");
        let entry = files
            .iter()
            .find(|e| e["name"] == f)
            .unwrap_or_else(|| panic!("{f} not in the manifest"));
        assert_eq!(entry["sha256"], sha(&dir.join(f)), "{f}");
    }
    // Every file of the folder other than the manifest is in it with its digest.
    for e in files {
        let name = e["name"].as_str().unwrap();
        assert_eq!(e["sha256"], sha(&dir.join(name)), "{name}");
    }
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    for f in NEW_FILES {
        assert!(
            readme.contains(&format!("`{f}`")),
            "README does not describe {f}"
        );
    }
}

#[tokio::test]
async fn passages_and_highlights_are_exported_with_their_codes() {
    let ctx = filled().await;
    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = PathBuf::from(exp["dir"].as_str().unwrap());
    let passages = std::fs::read_to_string(dir.join("passages.csv")).unwrap();
    let lines: Vec<&str> = passages.lines().collect();
    assert!(lines[0].starts_with("passage_id,interview_id,unit_id,start,end,text,kind,code_ids,code_names,later,later_note,stale"));
    assert_eq!(lines.len(), 4, "three passages and the header: {passages}");
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let coded = lines.iter().find(|l| l.contains(&adv)).unwrap();
    assert!(coded.contains(",coded,") && coded.contains(",2,20,"));
    let later = lines.iter().find(|l| l.contains("revoir")).unwrap();
    assert!(
        later.contains(",1,revoir,"),
        "the LATER flag is a column: {later}"
    );
    assert!(
        !later.contains("LATER,"),
        "LATER is not among the code ids: {later}"
    );
    let hl = std::fs::read_to_string(dir.join("highlights.csv")).unwrap();
    assert_eq!(hl.lines().count(), 2, "one highlight: {hl}");
}

#[tokio::test]
async fn the_codebook_csv_carries_the_code_tree() {
    let ctx = filled().await;
    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = PathBuf::from(exp["dir"].as_str().unwrap());
    let csv = std::fs::read_to_string(dir.join("codebook.csv")).unwrap();
    let head = csv.lines().next().unwrap();
    assert!(
        head.ends_with("code_id,name,parent_id,aliases,includes,excludes"),
        "{head}"
    );
    // The first thirteen columns are those of the older layout.
    assert!(head.starts_with("code,label,definition,toe,origin,active,in_prompt,research_questions,merged_into,version,example,reason,created_at,"));
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let skills_line = csv.lines().find(|l| l.starts_with("ORG-SKILLS,")).unwrap();
    assert!(
        skills_line.contains(&format!("Compétences,{adv},")),
        "{skills_line}"
    );
    assert!(
        skills_line.contains("ORG-SKILLS,"),
        "the old token is an alias: {skills_line}"
    );
    // The older import still reads this file.
    let fresh = setup().await;
    let (st, v) = call(
        &fresh.app,
        "POST",
        "/api/codebook/import",
        Some(json!({"kind": "csv", "path": dir.join("codebook.csv")})),
    )
    .await;
    assert_eq!(st, axum::http::StatusCode::OK, "{v}");
}

#[tokio::test]
async fn attributes_memos_and_study_configuration_are_exported() {
    let ctx = filled().await;
    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = PathBuf::from(exp["dir"].as_str().unwrap());
    let attrs = std::fs::read_to_string(dir.join("attributes.csv")).unwrap();
    assert_eq!(
        attrs,
        "interview_id,attribute_id,label,kind,value\nINT-01,kind,Type,choice,communale\n"
    );
    let memos: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("memos.json")).unwrap()).unwrap();
    let item = &memos["interviews"][0];
    assert_eq!(item["interview_id"], "INT-01");
    assert_eq!(
        item["memo"]["questions"][0]["answer"]["text"],
        "Elle gère, seule."
    );
    assert_eq!(item["checklist"]["items"][0]["checked"], true);
    assert_eq!(item["checklist"]["note"], "ok");
    let cfg: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("study_config.json")).unwrap())
            .unwrap();
    assert_eq!(cfg["format"], "coding-studio-project-template");
    let text = cfg.to_string();
    assert!(
        !text.contains("Elle gère") && !text.contains("INT-01"),
        "no interview data in the configuration"
    );
    // It is what the template export gives, so it imports as a template.
    assert_eq!(
        cfg,
        ok(&ctx.app, "GET", "/api/project/template", None).await
    );
}

#[tokio::test]
async fn the_state_digest_moves_with_the_new_data() {
    let ctx = filled().await;
    let state_sha = |dir: &str| -> String {
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(PathBuf::from(dir).join("state.json")).unwrap(),
        )
        .unwrap();
        v["sha256"].as_str().unwrap().to_string()
    };
    let a = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let b = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    assert_eq!(
        state_sha(a["dir"].as_str().unwrap()),
        state_sha(b["dir"].as_str().unwrap()),
        "same state, same digest"
    );
    let units = unit_ids(&ctx.app, "INT-01").await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[1], "start": 30, "end": 40})),
    )
    .await;
    let c = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    assert_ne!(
        state_sha(a["dir"].as_str().unwrap()),
        state_sha(c["dir"].as_str().unwrap())
    );
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(PathBuf::from(c["dir"].as_str().unwrap()).join("state.json"))
            .unwrap(),
    )
    .unwrap();
    for part in [
        "passages",
        "attribute_values",
        "memos_and_checklists",
        "project_config",
        "code_tree",
    ] {
        assert!(state["parts"].get(part).is_some(), "{part}");
    }
}

#[tokio::test]
async fn the_cross_corpus_package_covers_every_interview() {
    let ctx = filled().await;
    let units2 = unit_ids(&ctx.app, "INT-02").await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-02/passages",
        Some(json!({"unit_id": units2[1]})),
    )
    .await;
    let cc = ok(&ctx.app, "POST", "/api/export/cross-case", None).await;
    let dir = PathBuf::from(cc["folder"].as_str().unwrap());
    let m = manifest(&dir);
    for f in NEW_FILES.iter().chain(["codebook.csv"].iter()) {
        assert!(dir.join(f).is_file(), "missing {f}");
        let e = m["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == *f)
            .unwrap_or_else(|| panic!("{f} not in the manifest"));
        assert_eq!(e["sha256"], sha(&dir.join(f)));
    }
    let passages = std::fs::read_to_string(dir.join("passages.csv")).unwrap();
    assert!(passages.contains("INT-01") && passages.contains("INT-02"));
    let memos: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("memos.json")).unwrap()).unwrap();
    assert_eq!(memos["interviews"].as_array().unwrap().len(), 2);
}
