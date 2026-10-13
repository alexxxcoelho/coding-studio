//! The nine API gaps reported by the front-end, over HTTP (V1 lot 5).

mod common;

use axum::http::StatusCode;
use common::{call, code_id, ok, setup, unit_ids};
use serde_json::{json, Value};

#[tokio::test]
async fn gap_1_whole_unit_needs_both_offsets() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let part = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": unit, "code": adv, "start": 0, "end": 10})),
    )
    .await;
    assert_eq!(part["passage"]["start"], 0);
    assert_eq!(part["passage"]["whole_unit"], false);
    let whole = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": unit, "code": adv})),
    )
    .await;
    assert_eq!(whole["passage"]["whole_unit"], true);
}

#[tokio::test]
async fn gap_2_a_code_is_created_without_a_passage() {
    let ctx = setup().await;
    let c = ok(
        &ctx.app,
        "POST",
        "/api/codes",
        Some(
            json!({"name": "Freins à l'adoption", "definition": "Ce qui retarde.",
                    "includes": "coût", "excludes": "temps"}),
        ),
    )
    .await;
    assert_eq!(c["name"], "Freins à l'adoption");
    assert!(c["code_id"].as_str().unwrap().starts_with("C-"));
    assert_ne!(c["token"], c["name"]);
    let child = ok(
        &ctx.app,
        "POST",
        "/api/codes",
        Some(json!({"name": "Coût", "definition": "Le prix.", "parent_id": c["code_id"]})),
    )
    .await;
    assert_eq!(child["parent_id"], c["code_id"]);
    // GET still lists them (both methods on one path)
    let list = ok(&ctx.app, "GET", "/api/codes", None).await;
    assert!(list["codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["code_id"] == c["code_id"]));
    for bad in [
        json!({"name": "", "definition": "x"}),
        json!({"name": "Sans définition", "definition": " "}),
        json!({"name": "freins à l'adoption", "definition": "doublon"}),
    ] {
        let (st, _) = call(&ctx.app, "POST", "/api/codes", Some(bad)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn gap_3_description_and_coding_rules_are_explicit_fields() {
    let ctx = setup().await;
    let mut cfg = ok(&ctx.app, "GET", "/api/config", None).await;
    assert_eq!(cfg["description"], "");
    assert_eq!(cfg["coding_rules"], "");
    let prompt_before = cfg["prompt_template"].clone();
    cfg["description"] = json!("Pourquoi les petites bibliothèques changent de système.");
    cfg["coding_rules"] = json!("Un code par idée distincte.");
    let saved = ok(&ctx.app, "PUT", "/api/config", Some(cfg)).await;
    assert_eq!(
        saved["description"],
        "Pourquoi les petites bibliothèques changent de système."
    );
    let again = ok(&ctx.app, "GET", "/api/config", None).await;
    assert_eq!(again["coding_rules"], "Un code par idée distincte.");
    assert_eq!(
        again["prompt_template"], prompt_before,
        "prompt_template is kept"
    );
}

#[tokio::test]
async fn gaps_4_and_5_to_review_marks_and_the_library_summary() {
    let ctx = setup().await;
    let units = unit_ids(&ctx.app, "INT-01").await;
    let resp = units[1].clone();
    let interviewer = units[0].clone();
    let m = ok(
        &ctx.app,
        "PUT",
        &format!("/api/interviews/INT-01/units/{resp}/mark"),
        Some(json!({"note": "ambigu"})),
    )
    .await;
    assert_eq!(m["mark"], "to_review");
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("/api/interviews/INT-01/units/{interviewer}/mark"),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "an interviewer turn cannot be marked"
    );
    let marks = ok(&ctx.app, "GET", "/api/interviews/INT-01/marks", None).await;
    assert_eq!(marks.as_array().unwrap().len(), 1);
    let simple = ok(&ctx.app, "GET", "/api/interviews/INT-01/simple", None).await;
    assert_eq!(simple["n_to_review"], 1);
    // the library summary, one call
    let lib = ok(&ctx.app, "GET", "/api/library/progress", None).await;
    let rows = lib["interviews"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let r = rows.iter().find(|r| r["interview_id"] == "INT-01").unwrap();
    assert_eq!(r["units"], 6);
    assert_eq!(r["respondent_units"], 3);
    assert_eq!(r["to_review"], 1);
    for key in [
        "relevant",
        "not_relevant",
        "coded",
        "memo_answered",
        "memo_total",
        "review",
        "arbitrated",
    ] {
        assert!(r.get(key).is_some(), "missing {key}");
    }
    assert_eq!(r["review"]["state"], "none");
    assert_eq!(lib["totals"]["interviews"], 2);
    // clearing
    let c = ok(
        &ctx.app,
        "DELETE",
        &format!("/api/interviews/INT-01/units/{resp}/mark"),
        None,
    )
    .await;
    assert_eq!(c["cleared"], true);
    assert_eq!(
        ok(&ctx.app, "GET", "/api/interviews/INT-01/simple", None).await["n_to_review"],
        0
    );
}

#[tokio::test]
async fn gap_6_the_code_memo_is_its_own_field_with_a_history() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let r = ok(
        &ctx.app,
        "PUT",
        &format!("/api/codes/{adv}/memo"),
        Some(json!({"text": "Premier jet."})),
    )
    .await;
    assert_eq!(r["memo"], "Premier jet.");
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/codes/{adv}/memo"),
        Some(json!({"text": "Second jet."})),
    )
    .await;
    let h = ok(&ctx.app, "GET", &format!("/api/codes/{adv}/memo"), None).await;
    assert_eq!(h["history"].as_array().unwrap().len(), 2);
    assert_eq!(h["memo"], "Second jet.");
    let code = ok(&ctx.app, "GET", &format!("/api/codes/{adv}"), None).await;
    let node = code.get("code").unwrap_or(&code);
    assert_eq!(node["memo"], "Second jet.");
    assert!(node["reason"].is_null(), "reason is not overloaded");
}

#[tokio::test]
async fn gap_7_undo_create_code_from_passage() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/codes/from-passage",
        Some(json!({"name": "À défaire", "definition": "Pour le test.",
                    "passage": {"interview_id": "INT-01", "unit_id": unit, "start": 4, "end": 20}})),
    )
    .await;
    let code = v["code"]["code_id"].as_str().unwrap().to_string();
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    let out = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{code}/undo-create"),
        None,
    )
    .await;
    assert_eq!(out["code_deleted"], true);
    assert_eq!(out["passage_deleted"], true);
    let (st, _) = call(&ctx.app, "GET", &format!("/api/codes/{code}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = call(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let events = ok(&ctx.app, "GET", "/api/audit", None).await;
    assert!(events
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "codebook.code.undo_create_from_passage"));
    // a code that was not made from a passage cannot be undone this way
    let plain = ok(
        &ctx.app,
        "POST",
        "/api/codes",
        Some(json!({"name": "Libre", "definition": "x"})),
    )
    .await;
    let (st, _) = call(
        &ctx.app,
        "POST",
        &format!(
            "/api/codes/{}/undo-create",
            plain["code_id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn gap_8_passage_origin_role_and_claim_nature() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id": unit, "code": adv, "start": 4, "end": 20})),
    )
    .await;
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    assert!(v["passage"].get("origin").is_none());
    let q = ok(
        &ctx.app,
        "PUT",
        &format!("/api/passages/{pid}/qualification"),
        Some(json!({"origin": "lived_experience", "role": "trigger"})),
    )
    .await;
    assert_eq!(q["origin"], "lived_experience");
    assert_eq!(q["role"], "trigger");
    let got = ok(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(
        got.get("passage").unwrap_or(&got)["origin"],
        "lived_experience"
    );
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("/api/passages/{pid}/qualification"),
        Some(json!({"origin": "rumeur"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/passages/{pid}/qualification"),
        Some(json!({"origin": null})),
    )
    .await;
    // claim nature from the project list
    let excerpt = "inventory accuracy";
    let unit2 = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let claim = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/claims",
        Some(json!({"unit_id": unit2, "excerpt": excerpt, "nature": "first_hand"})),
    )
    .await;
    assert_eq!(claim["nature"], "first_hand");
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/claims",
        Some(json!({"unit_id": unit2, "excerpt": excerpt, "nature": "rumeur"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // a claim written before the field existed has no nature and stays readable
    let old: Value = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/claims",
        Some(json!({"unit_id": unit2, "excerpt": excerpt, "ev_level": "EV2"})),
    )
    .await;
    assert_eq!(old["nature"], Value::Null);
    assert_eq!(old["ev_level"], "EV2");
}
