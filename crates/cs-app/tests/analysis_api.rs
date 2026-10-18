//! V1 lot 5 over HTTP: themes, matrix, typed links, write-up, final audit,
//! decisions and the exports that carry them (SPEC 4.7, 4.8, 4.9; tests A8,
//! A25, A29).

mod common;

use std::path::PathBuf;

use axum::http::StatusCode;
use common::{call, ok, setup, unit_ids, Ctx};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// A project with one coded passage, one theme and a research question.
async fn filled() -> (Ctx, String, String) {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/codes/from-passage",
        Some(json!({
            "name": "Exactitude des stocks",
            "definition": "Le répondant parle de l'exactitude de l'inventaire.",
            "passage": { "interview_id": "INT-01", "unit_id": unit, "start": 4, "end": 31 },
        })),
    )
    .await;
    let code = v["code"]["code_id"].as_str().unwrap().to_string();
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "POST",
        "/api/research-questions",
        Some(json!({"id": "RQ1", "text": "Pourquoi adopter ?"})),
    )
    .await;
    (ctx, code, pid)
}

async fn make_theme(ctx: &Ctx, code: &str) -> Value {
    ok(
        &ctx.app,
        "POST",
        "/api/themes",
        Some(json!({
            "title": "L'exactitude des stocks justifie l'investissement",
            "proposition": "Deux phrases.",
            "codes": [code],
            "research_questions": ["RQ1"],
        })),
    )
    .await
}

#[tokio::test]
async fn a_theme_goes_through_its_sheet_checks_and_status() {
    let (ctx, code, _pid) = filled().await;
    let t = make_theme(&ctx, &code).await;
    let id = t["theme_id"].as_str().unwrap().to_string();
    assert_eq!(t["status"], "candidate");
    assert_eq!(t["codes_count"], 1);
    assert_eq!(t["interviews_count"], 1);
    assert_eq!(t["ord"], 1);
    let list = ok(&ctx.app, "GET", "/api/themes", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["interviews_count"], 1);
    // sheet edit with a title change
    let t = ok(
        &ctx.app,
        "PUT",
        &format!("/api/themes/{id}"),
        Some(json!({"limits": "Un seul site.", "title": "Titre deux", "reason": "plus net"})),
    )
    .await;
    assert_eq!(t["limits"], "Un seul site.");
    let h = ok(&ctx.app, "GET", &format!("/api/themes/{id}/history"), None).await;
    assert_eq!(h.as_array().unwrap().len(), 2);
    assert_eq!(h[1]["kind"], "title");
    assert_eq!(h[1]["override"], false);
    // retained: 409 until every check is done or an override is given
    let (st, v) = call(
        &ctx.app,
        "POST",
        &format!("/api/themes/{id}/status"),
        Some(json!({"status": "retained"})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    let (st, _) = call(
        &ctx.app,
        "POST",
        &format!("/api/themes/{id}/status"),
        Some(json!({"status": "retained", "override_reason": "relu par le directeur"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let h = ok(&ctx.app, "GET", &format!("/api/themes/{id}/history"), None).await;
    assert_eq!(h[2]["override"], true);
    // checks
    let t = ok(&ctx.app, "GET", &format!("/api/themes/{id}"), None).await;
    let first = t["checks"][0]["check_id"].as_str().unwrap().to_string();
    let t = ok(
        &ctx.app,
        "PUT",
        &format!("/api/themes/{id}/checks/{first}"),
        Some(json!({"done": true, "note": "relu"})),
    )
    .await;
    assert_eq!(t["checks"][0]["done"], true);
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("/api/themes/{id}/checks/inconnu"),
        Some(json!({"done": true})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // passages of the theme
    let ps = ok(&ctx.app, "GET", &format!("/api/themes/{id}/passages"), None).await;
    assert_eq!(ps.as_array().unwrap().len(), 1);
    assert_eq!(ps[0]["interview_id"], "INT-01");
    assert_eq!(ps[0]["code_ids"], json!([code]));
    assert_eq!(ps[0]["cell_value"], Value::Null);
    // code add and remove, order
    let t = ok(
        &ctx.app,
        "DELETE",
        &format!("/api/themes/{id}/codes/{code}"),
        None,
    )
    .await;
    assert_eq!(t["codes_count"], 0);
    let t = ok(
        &ctx.app,
        "POST",
        &format!("/api/themes/{id}/codes"),
        Some(json!({"code_id": code})),
    )
    .await;
    assert_eq!(t["codes_count"], 1);
    let t2 = make_theme(&ctx, &code).await;
    let id2 = t2["theme_id"].as_str().unwrap().to_string();
    let ordered = ok(
        &ctx.app,
        "PUT",
        "/api/themes/order",
        Some(json!({"ids": [id2, id]})),
    )
    .await;
    assert_eq!(ordered[0]["theme_id"], json!(id2));
    // unknown theme
    let (st, _) = call(&ctx.app, "GET", "/api/themes/T-0099", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_matrix_is_computed_stored_and_saved_as_views() {
    let (ctx, code, pid) = filled().await;
    let t = make_theme(&ctx, &code).await;
    let tid = t["theme_id"].as_str().unwrap().to_string();
    let cfg = json!({"rows": {"kind": "themes"}, "cols": {"kind": "interviews"}, "filter": null});
    let m = ok(
        &ctx.app,
        "POST",
        "/api/matrix",
        Some(json!({"config": cfg})),
    )
    .await;
    assert_eq!(m["rows"][0]["key"], json!(format!("theme:{tid}")));
    let cols: Vec<&str> = m["cols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert_eq!(cols, ["interview:INT-01", "interview:INT-02"]);
    assert_eq!(m["cols"][0]["coded"], true);
    assert_eq!(m["cols"][1]["coded"], false, "INT-02 has no coded passage");
    assert_eq!(m["cells"][0]["n_passages"], 1);
    assert_eq!(m["cells"][0]["value"], Value::Null);
    assert_eq!(m["cells"][1]["n_passages"], 0);
    // the body may also be the configuration itself
    ok(&ctx.app, "POST", "/api/matrix", Some(cfg.clone())).await;
    // open and fill a cell
    let rk = format!("theme:{tid}");
    let cell = ok(
        &ctx.app,
        "GET",
        &format!("/api/matrix/cell?row={rk}&col=interview:INT-01"),
        None,
    )
    .await;
    assert_eq!(cell["passages"][0]["passage_id"], json!(pid));
    assert_eq!(cell["passages"][0]["speaker"], "RESP");
    let cell = ok(
        &ctx.app,
        "PUT",
        "/api/matrix/cell",
        Some(json!({"row_key": rk, "col_key": "interview:INT-01", "value": "present", "summary": "Il y tient."})),
    )
    .await;
    assert_eq!(cell["value"], "present");
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/matrix/cell",
        Some(
            json!({"row_key": rk, "col_key": "interview:INT-01", "value": "maybe", "summary": ""}),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let m = ok(
        &ctx.app,
        "POST",
        "/api/matrix",
        Some(json!({"config": cfg})),
    )
    .await;
    assert_eq!(m["cells"][0]["value"], "present");
    assert_eq!(m["cells"][0]["summary"], "Il y tient.");
    // the value shows in the theme's passages
    let ps = ok(
        &ctx.app,
        "GET",
        &format!("/api/themes/{tid}/passages"),
        None,
    )
    .await;
    assert_eq!(ps[0]["cell_value"], "present");
    // views
    let v = ok(
        &ctx.app,
        "POST",
        "/api/matrix/views",
        Some(json!({"name": "Thèmes par entretien", "config": cfg})),
    )
    .await;
    assert_eq!(v["view_id"], "M-0001");
    assert_eq!(v["config"]["rows"]["kind"], "themes");
    let views = ok(&ctx.app, "GET", "/api/matrix/views", None).await;
    assert_eq!(views.as_array().unwrap().len(), 1);
    ok(&ctx.app, "DELETE", "/api/matrix/views/M-0001", None).await;
    assert!(ok(&ctx.app, "GET", "/api/matrix/views", None)
        .await
        .as_array()
        .unwrap()
        .is_empty());
    let o = ok(&ctx.app, "GET", "/api/matrix/options", None).await;
    assert_eq!(o["cell_values"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn links_parts_synthesis_and_the_audit_make_a_chain() {
    let (ctx, code, pid) = filled().await;
    let t = make_theme(&ctx, &code).await;
    let tid = t["theme_id"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "POST",
        &format!("/api/themes/{tid}/status"),
        Some(json!({"status": "retained", "override_reason": "test"})),
    )
    .await;
    // a part and a link
    let p = ok(
        &ctx.app,
        "PUT",
        &format!("/api/writing/themes/{tid}/parts/mechanism_evidence"),
        Some(json!({"text": "L'inventaire passe de soixante à quatre-vingt-dix-huit pour cent.", "nature": "first_hand"})),
    )
    .await;
    assert_eq!(p["nature"], "first_hand");
    let link = ok(
        &ctx.app,
        "POST",
        "/api/links",
        Some(json!({
            "passage_id": pid,
            "target": {"kind": "theme_part", "theme_id": tid, "part": "mechanism_evidence"},
            "link_type": "supports", "note": "le chiffre",
        })),
    )
    .await;
    let lid = link["link_id"].as_str().unwrap().to_string();
    assert_eq!(link["passage"]["verification"], "unverified");
    let w = ok(&ctx.app, "GET", &format!("/api/writing/themes/{tid}"), None).await;
    assert_eq!(w["parts"].as_object().unwrap().len(), 5);
    assert_eq!(
        w["parts"]["mechanism_evidence"]["links"][0]["link_id"],
        json!(lid)
    );
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/links",
        Some(json!({
            "passage_id": pid,
            "target": {"kind": "theme_part", "theme_id": tid, "part": "mechanism_evidence"},
            "link_type": "supports",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("/api/writing/themes/{tid}/parts/nope"),
        Some(json!({"text": "x"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/links/{lid}"),
        Some(json!({"link_type": "nuances"})),
    )
    .await;
    // synthesis of the research question
    let q = ok(
        &ctx.app,
        "PUT",
        "/api/writing/questions/RQ1",
        Some(json!({"answer": "Parce que le stock est exact.", "limits": "Un seul site."})),
    )
    .await;
    assert_eq!(q["themes"][0]["theme_id"], json!(tid));
    assert_eq!(q["n_versions"], 1);
    // final audit: no audit yet, then a draw
    assert_eq!(
        ok(&ctx.app, "GET", "/api/final-audit/latest", None).await,
        Value::Null
    );
    let a = ok(
        &ctx.app,
        "POST",
        "/api/final-audit",
        Some(json!({"n": 3, "seed": "graine"})),
    )
    .await;
    assert_eq!(a["seed"], "graine");
    let claims = a["claims"].as_array().unwrap();
    assert_eq!(
        claims.len(),
        2,
        "two writable parts, fewer than three asked"
    );
    let linked = claims
        .iter()
        .find(|c| c["claim_key"] == json!(format!("theme_part:{tid}:mechanism_evidence")))
        .unwrap();
    assert!(!linked["links"][0]["passage"]["text"]
        .as_str()
        .unwrap()
        .is_empty());
    let kinds: Vec<&str> = linked["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["unverified_passage"]);
    let aid = a["audit_id"].as_i64().unwrap();
    let a = ok(
        &ctx.app,
        "POST",
        &format!("/api/final-audit/{aid}/verdicts"),
        Some(json!({"claim_key": linked["claim_key"], "verdict": "to_nuance", "note": "trop net"})),
    )
    .await;
    let v = a["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["claim_key"] == linked["claim_key"])
        .unwrap();
    assert_eq!(v["verdict"]["verdict"], "to_nuance");
    let (st, _) = call(
        &ctx.app,
        "POST",
        &format!("/api/final-audit/{aid}/verdicts"),
        Some(json!({"claim_key": linked["claim_key"], "verdict": "fine"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let latest = ok(&ctx.app, "GET", "/api/final-audit/latest", None).await;
    assert_eq!(latest["audit_id"], json!(aid));
    // removal
    ok(
        &ctx.app,
        "DELETE",
        &format!("/api/links/{lid}?reason=doublon"),
        None,
    )
    .await;
    let w = ok(&ctx.app, "GET", &format!("/api/writing/themes/{tid}"), None).await;
    assert!(w["parts"]["mechanism_evidence"]["links"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn the_framework_reading_needs_a_framework() {
    let (ctx, code, _pid) = filled().await;
    let t = make_theme(&ctx, &code).await;
    let tid = t["theme_id"].as_str().unwrap().to_string();
    let (st, _) = call(
        &ctx.app,
        "GET",
        &format!("/api/themes/{tid}/framework"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["interpretive_framework"] = json!({"preset": "custom", "dimensions": [
        {"id": "tech", "label": "Technologie", "definition": ""}]});
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    let r = ok(
        &ctx.app,
        "PUT",
        &format!("/api/themes/{tid}/framework"),
        Some(
            json!({"cells": {"tech": {"explains": true, "note": "le délai"}},
                    "out_of_frame": false, "out_of_frame_note": "",
                    "complementary_dimension": "", "evidence_limit": "Un site."}),
        ),
    )
    .await;
    assert_eq!(r["cells"]["tech"]["explains"], true);
    let table = ok(&ctx.app, "GET", "/api/writing/framework", None).await;
    assert!(table["rows"][0]["title"].as_str().is_some());
    assert_eq!(table["rows"][0]["cells"]["tech"]["note"], "le délai");
}

#[tokio::test]
async fn decisions_use_the_words_of_the_guide() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let d = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/decisions",
        Some(json!({
            "name": "Étiquetage RFID", "outcome": "adopter",
            "characteristics": {"taille": "grande"}, "units": [unit],
        })),
    )
    .await;
    assert_eq!(d["name"], "Étiquetage RFID");
    assert_eq!(d["outcome"], "adopt");
    assert_eq!(d["outcome_label"], "adopter");
    assert_eq!(d["characteristics"]["taille"], "grande");
    let id = d["decision_id"].as_str().unwrap().to_string();
    // the same decision through the earlier route reads the same
    let old = ok(&ctx.app, "GET", "/api/cases", None).await;
    assert_eq!(old[0]["outcome"], "adopt");
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("/api/decisions/{id}"),
        Some(json!({"name": "Étiquetage RFID", "outcome": "peut-être"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let all = ok(&ctx.app, "GET", "/api/decisions", None).await;
    assert_eq!(all.as_array().unwrap().len(), 1);
    // what a read answers (`name` and `label` together) can be written back
    let mut read = ok(&ctx.app, "GET", &format!("/api/decisions/{id}"), None).await;
    read["outcome"] = json!("defer");
    let back = ok(&ctx.app, "PUT", &format!("/api/decisions/{id}"), Some(read)).await;
    assert_eq!(back["outcome"], "defer");
}

fn dir_of(v: &Value) -> PathBuf {
    PathBuf::from(v["dir"].as_str().unwrap())
}

#[tokio::test]
async fn the_writeup_export_carries_every_lot_5_file_under_the_manifest() {
    let (ctx, code, pid) = filled().await;
    let t = make_theme(&ctx, &code).await;
    let tid = t["theme_id"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "POST",
        &format!("/api/themes/{tid}/status"),
        Some(json!({"status": "retained", "override_reason": "test"})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/writing/themes/{tid}/parts/mechanism_evidence"),
        Some(json!({"text": "Le stock devient exact."})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/links",
        Some(json!({
            "passage_id": pid,
            "target": {"kind": "theme_part", "theme_id": tid, "part": "mechanism_evidence"},
            "link_type": "supports",
        })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/matrix/views",
        Some(json!({"name": "V", "config": {"rows": {"kind": "themes"}, "cols": {"kind": "interviews"}, "filter": null}})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/final-audit",
        Some(json!({"n": 3, "seed": "s1"})),
    )
    .await;
    common::approve_all_anonymisation(&ctx.app).await;
    let state_before = ok(
        &ctx.app,
        "POST",
        "/api/writing/export",
        Some(json!({"format": "markdown"})),
    )
    .await;
    let dir = dir_of(&state_before);
    let md = state_before["markdown"].as_str().unwrap();
    assert!(md.contains("Le stock devient exact.[^1]"), "{md}");
    assert!(md.contains("[^1]: INT-01, unité INT-01-U004"));
    assert!(md.contains("s1"));
    assert!(!md.contains('\u{2014}'));
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let listed: Vec<&str> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    for name in [
        "themes.csv",
        "theme_sheets.json",
        "matrix.csv",
        "links.csv",
        "writeup.md",
        "final_audit.json",
        "decisions.csv",
        "decisions.json",
        "state.json",
    ] {
        assert!(listed.contains(&name), "manifest lacks {name}");
        let f = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap();
        let bytes = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(f["sha256"], hex::encode(Sha256::digest(&bytes)), "{name}");
    }
    let themes = std::fs::read_to_string(dir.join("themes.csv")).unwrap();
    assert!(themes.contains("L'exactitude des stocks justifie l'investissement"));
    let links = std::fs::read_to_string(dir.join("links.csv")).unwrap();
    assert!(links.contains("supports"));
    let matrix = std::fs::read_to_string(dir.join("matrix.csv")).unwrap();
    assert!(matrix.contains("interview:INT-01"));
    let audit: Value =
        serde_json::from_slice(&std::fs::read(dir.join("final_audit.json")).unwrap()).unwrap();
    assert_eq!(audit["audits"][0]["seed"], "s1");
    // the state digest moves when a theme changes
    let sha_a = serde_json::from_slice::<Value>(&std::fs::read(dir.join("state.json")).unwrap())
        .unwrap()["sha256"]
        .clone();
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/themes/{tid}"),
        Some(json!({"limits": "Un site."})),
    )
    .await;
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = ok(&ctx.app, "POST", "/api/writing/export", Some(json!({}))).await;
    let sha_b = serde_json::from_slice::<Value>(
        &std::fs::read(dir_of(&second).join("state.json")).unwrap(),
    )
    .unwrap()["sha256"]
        .clone();
    assert_ne!(sha_a, sha_b);
    // the cross-case package carries the same files
    let cc = ok(&ctx.app, "POST", "/api/export/cross-case", None).await;
    let names: Vec<&str> = cc["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    for n in [
        "themes.csv",
        "writeup.md",
        "final_audit.json",
        "decisions.csv",
    ] {
        assert!(names.contains(&n), "cross-case lacks {n}");
    }
    // the audit trail records the writes
    let events = ok(&ctx.app, "GET", "/api/audit", None).await;
    let actions: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    for a in [
        "theme.create",
        "theme.status",
        "link.create",
        "writing.part",
        "audit.draw",
        "export.anonymised",
    ] {
        assert!(actions.contains(&a), "no {a}");
    }
    assert_eq!(
        ok(&ctx.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );
}
