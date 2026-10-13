//! F-102: anonymised academic export.

mod common;

use std::path::{Path, PathBuf};

use common::{call, code_id, ok, setup, unit_ids, Ctx};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

async fn filled() -> Ctx {
    let ctx = setup().await;
    ok(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({ "enabled": false, "entries": [
            { "kind": "organisation", "original": "retailer", "pseudonym": "Organisation A" },
            { "kind": "person", "original": "Notre bibliothèque", "pseudonym": "Lieu B" } ] })),
    )
    .await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let units = unit_ids(&ctx.app, "INT-01").await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": units[1], "start": 2, "end": 40, "code": adv})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/interviews/INT-01/memo/answers",
        Some(
            json!({"answers": {"q1": "Le retailer est pressé, et Interviewer Person A le sait."}}),
        ),
    )
    .await;
    ctx
}

fn files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

fn all_text(dir: &Path) -> String {
    files(dir)
        .iter()
        .filter(|n| !n.ends_with(".docx"))
        .map(|n| std::fs::read_to_string(dir.join(n)).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

fn dir_of(v: &Value) -> PathBuf {
    PathBuf::from(v["dir"].as_str().unwrap())
}

#[tokio::test]
async fn an_interview_package_is_pseudonymised_in_every_text_and_keeps_its_ids() {
    let ctx = filled().await;
    let r = ok(
        &ctx.app,
        "POST",
        "/api/export/anonymised",
        Some(json!({"scope": "interview", "interview_id": "INT-01",
                    "researcher_names": ["Interviewer Person A"], "researcher_label": "Chercheur"})),
    )
    .await;
    let dir = dir_of(&r);
    assert!(
        dir.to_string_lossy().ends_with("_INT-01_anonymised")
            || dir.to_string_lossy().contains("INT-01_anonymised")
    );
    let names = files(&dir);
    for kept in [
        "units.csv",
        "passages.csv",
        "memos.json",
        "codebook.csv",
        "state.json",
        "manifest.json",
        "README.md",
        "study_config.json",
    ] {
        assert!(
            names.contains(&kept.to_string()),
            "{kept} missing: {names:?}"
        );
    }
    assert!(!names.contains(&"db_snapshot.sqlite".to_string()));
    assert!(!names.contains(&"audit_log.jsonl".to_string()));

    let text = all_text(&dir);
    assert!(
        !text.to_lowercase().contains("retailer"),
        "organisation still there"
    );
    assert!(!text.contains("Interviewer Person A"), "researcher still there");
    let units = std::fs::read_to_string(dir.join("units.csv")).unwrap();
    assert!(units.contains("Organisation A"), "{units}");
    assert!(units.contains("INT-01-U002"), "ids kept");
    let memos = std::fs::read_to_string(dir.join("memos.json")).unwrap();
    assert!(
        memos.contains("Le Organisation A est pressé, et Chercheur le sait."),
        "{memos}"
    );
    let passages = std::fs::read_to_string(dir.join("passages.csv")).unwrap();
    assert!(passages.contains("INT-01"));

    // README: what was replaced and what was not, never the names.
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(readme.contains("What was replaced") && readme.contains("What was not replaced"));
    assert!(
        readme.contains("texte libre"),
        "the free text note is stated"
    );
    assert!(readme.contains("db_snapshot.sqlite") && readme.contains("audit_log.jsonl"));
    assert!(readme.contains("`Chercheur`"));

    // Manifest digests are those of the files, and state.json is self-consistent.
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["kind"], "anonymised");
    assert_eq!(manifest["anonymisation"]["free_text_covered"], false);
    for f in manifest["files"].as_array().unwrap() {
        let n = f["name"].as_str().unwrap();
        let bytes = std::fs::read(dir.join(n)).unwrap();
        assert_eq!(f["sha256"], hex::encode(Sha256::digest(&bytes)), "{n}");
    }
    assert!(!manifest.to_string().contains("retailer"));
    let state: Value =
        serde_json::from_slice(&std::fs::read(dir.join("state.json")).unwrap()).unwrap();
    let sha = hex::encode(Sha256::digest(
        serde_json::to_string(&state["parts"]).unwrap().as_bytes(),
    ));
    assert_eq!(state["sha256"], sha);

    // Counts by kind are reported, and the audit never holds a name.
    assert!(r["replacements"]["organisation"].as_u64().unwrap() >= 1);
    assert!(r["replacements"]["researcher"].as_u64().unwrap() >= 1);
    let audit = ok(&ctx.app, "GET", "/api/audit", None).await.to_string();
    assert!(audit.contains("export.anonymised"));
    assert!(
        !audit.contains("\"export.package\""),
        "the temporary source package is not logged"
    );
    assert_eq!(
        ok(&ctx.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );
}

#[tokio::test]
async fn cross_case_and_writeup_packages_are_pseudonymised_too() {
    let ctx = filled().await;
    let cc = ok(
        &ctx.app,
        "POST",
        "/api/export/anonymised",
        Some(json!({"scope": "cross-case"})),
    )
    .await;
    let text = all_text(&dir_of(&cc));
    assert!(!text.to_lowercase().contains("retailer"));
    let w = ok(
        &ctx.app,
        "POST",
        "/api/export/anonymised",
        Some(json!({"scope": "writeup"})),
    )
    .await;
    let dir = dir_of(&w);
    assert!(files(&dir).contains(&"writeup.docx".to_string()));
    assert!(files(&dir).contains(&"writeup.md".to_string()));
}

#[tokio::test]
async fn bad_scopes_and_unknown_interviews_are_refused() {
    let ctx = filled().await;
    for body in [
        json!({"scope": "everything"}),
        json!({"scope": "interview"}),
        json!({"scope": "interview", "interview_id": "NOPE-01"}),
    ] {
        let (st, v) = call(
            &ctx.app,
            "POST",
            "/api/export/anonymised",
            Some(body.clone()),
        )
        .await;
        assert!(st == 400 || st == 404, "{body}: {st} {v}");
    }
}

#[tokio::test]
async fn a_listed_name_inside_an_interview_id_is_refused() {
    let ctx = setup().await;
    ok(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({ "enabled": false, "entries": [
            { "kind": "person", "original": "INT", "pseudonym": "X" } ] })),
    )
    .await;
    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/export/anonymised",
        Some(json!({"scope": "interview", "interview_id": "INT-01"})),
    )
    .await;
    assert_eq!(st, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("interview id"));
}
