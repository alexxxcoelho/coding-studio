//! V1 lot 1 through the HTTP API: stable code ids, names, aliases, the code
//! tree, creation from a passage, merge. Acceptance tests A3 (API side), A4
//! and A5.

mod common;

use axum::http::StatusCode;
use common::{call, code, code_id, ok, setup, unit_ids};
use serde_json::{json, Value};

async fn passage(
    ctx: &common::Ctx,
    interview: &str,
    n: usize,
    code: &str,
    a: usize,
    b: usize,
) -> String {
    let unit = unit_ids(&ctx.app, interview).await[n].clone();
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/{interview}/passages"),
        Some(json!({ "unit_id": unit, "start": a, "end": b, "code": code })),
    )
    .await;
    v["passage"]["passage_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn codes_have_stable_ids_names_aliases_and_the_codebook_has_a_version() {
    let ctx = setup().await;
    let v = ok(&ctx.app, "GET", "/api/codes", None).await;
    let codes = v["codes"].as_array().unwrap();
    assert_eq!(codes.len(), 5, "four codes and the reserved one");
    assert_eq!(codes[0]["code_id"], "C-0001");
    assert_eq!(codes[0]["name"], codes[0]["token"]);
    assert_eq!(codes[0]["aliases"][0]["reason"], "legacy_token");
    assert_eq!(codes[4]["code_id"], "LATER");
    assert!(v["codebook_version"].as_i64().unwrap() >= 1);
    let rev = ok(&ctx.app, "GET", "/api/codebook/revisions", None).await;
    assert_eq!(rev["codebook_version"], v["codebook_version"]);
    assert!(!rev["revisions"].as_array().unwrap().is_empty());
    // The older route is unchanged and does not list the reserved code.
    let old = ok(&ctx.app, "GET", "/api/codebook", None).await;
    let tokens: Vec<&str> = old["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        tokens,
        vec!["TECH-ADV", "TECH-COST", "ORG-SKILLS", "ENV-PARTNER"]
    );
}

#[tokio::test]
async fn a4_renaming_keeps_the_id_and_the_codings_and_the_old_name_finds_the_code() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let p = passage(&ctx, "INT-01", 1, &adv, 2, 20).await;
    let v0 = ok(&ctx.app, "GET", "/api/codes", None).await["codebook_version"]
        .as_i64()
        .unwrap();
    let renamed = ok(
        &ctx.app,
        "PUT",
        &format!("/api/codes/{adv}"),
        Some(json!({ "name": "Avantage relatif perçu" })),
    )
    .await;
    assert_eq!(renamed["code_id"], json!(adv));
    assert_eq!(renamed["name"], "Avantage relatif perçu");
    assert_eq!(renamed["token"], "TECH-ADV");
    assert_eq!(renamed["version"], 2);
    assert_eq!(renamed["counts"]["passages"], 1);
    let aliases: Vec<&str> = renamed["aliases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["alias"].as_str().unwrap())
        .collect();
    assert!(aliases.contains(&"TECH-ADV"));
    let v1 = ok(&ctx.app, "GET", "/api/codes", None).await["codebook_version"]
        .as_i64()
        .unwrap();
    assert_eq!(v1, v0 + 1);
    // The codings are the same.
    let ps = ok(&ctx.app, "GET", &format!("/api/passages/{p}"), None).await;
    assert_eq!(ps["codings"][0]["code_id"], json!(adv));
    assert_eq!(ps["codings"][0]["code_name"], "Avantage relatif perçu");
    // The old name is found like autocomplete would: by coding with it.
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let by_old = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "code": "tech-adv" })),
    )
    .await;
    assert_eq!(by_old["passage"]["codings"][0]["code_id"], json!(adv));
    // The legacy unit-level route still knows the token.
    let old = ok(&ctx.app, "GET", "/api/codebook", None).await;
    assert!(old["codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["code"] == "TECH-ADV"));
}

#[tokio::test]
async fn a_name_already_used_or_reserved_is_refused() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    for name in ["org-skills", "Plus tard", "NA", ""] {
        let (st, v) = call(
            &ctx.app,
            "PUT",
            &format!("/api/codes/{adv}"),
            Some(json!({ "name": name })),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{name:?}: {v}");
    }
    let (st, _) = call(&ctx.app, "GET", "/api/codes/C-9999", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn synonyms_are_added_and_removed_through_the_api() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{adv}/aliases"),
        Some(json!({ "alias": "bénéfice perçu" })),
    )
    .await;
    assert!(v["aliases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["alias"] == "bénéfice perçu" && a["reason"] == "synonym"));
    let (st, _) = call(
        &ctx.app,
        "POST",
        &format!("/api/codes/{adv}/aliases"),
        Some(json!({ "alias": "Bénéfice perçu" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let v = ok(
        &ctx.app,
        "DELETE",
        &format!("/api/codes/{adv}/aliases/b%C3%A9n%C3%A9fice%20per%C3%A7u"),
        None,
    )
    .await;
    assert!(!v["aliases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["alias"] == "bénéfice perçu"));
    let (st, _) = call(
        &ctx.app,
        "DELETE",
        &format!("/api/codes/{adv}/aliases/TECH-ADV"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a5_moving_a_code_under_another_keeps_its_passages_and_rolls_up_to_the_parent() {
    let ctx = setup().await;
    let a = code_id(&ctx.app, "TECH-ADV").await;
    let b = code_id(&ctx.app, "ORG-SKILLS").await;
    let p1 = passage(&ctx, "INT-01", 1, &a, 2, 20).await;
    let p2 = passage(&ctx, "INT-01", 3, &b, 0, 10).await;
    let p3 = passage(&ctx, "INT-02", 1, &b, 0, 10).await;
    let before = ok(
        &ctx.app,
        "GET",
        &format!("/api/interviews/INT-01/passages?code={b}"),
        None,
    )
    .await;
    let moved = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{b}/move"),
        Some(json!({ "parent_id": a, "rationale": "regroupement" })),
    )
    .await;
    assert_eq!(moved["parent_id"], json!(a));
    let after = ok(
        &ctx.app,
        "GET",
        &format!("/api/interviews/INT-01/passages?code={b}"),
        None,
    )
    .await;
    assert_eq!(before, after, "the passages of B are untouched");
    let nb = code(&ctx.app, &b).await;
    assert_eq!(nb["counts"]["passages"], 2);
    assert_eq!(nb["counts"]["interviews"], 2);
    let na = code(&ctx.app, &a).await;
    assert_eq!(na["counts"]["passages"], 1);
    assert_eq!(na["counts"]["passages_rollup"], 3);
    assert_eq!(na["counts"]["interviews_rollup"], 2);
    let _ = (p1, p2, p3);
    // Passages of the parent, with and without descendants.
    let own = ok(&ctx.app, "GET", &format!("/api/codes/{a}/passages"), None).await;
    assert_eq!(own.as_array().unwrap().len(), 1);
    let all = ok(
        &ctx.app,
        "GET",
        &format!("/api/codes/{a}/passages?descendants=true"),
        None,
    )
    .await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    let first = &all[0];
    assert!(first["previous"].is_object());
    assert!(first["speaker"].is_string());
}

#[tokio::test]
async fn a_move_making_a_cycle_or_without_rationale_is_refused() {
    let ctx = setup().await;
    let a = code_id(&ctx.app, "TECH-ADV").await;
    let b = code_id(&ctx.app, "ORG-SKILLS").await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{b}/move"),
        Some(json!({ "parent_id": a, "rationale": "r" })),
    )
    .await;
    for (child, body) in [
        (&a, json!({ "parent_id": b, "rationale": "r" })),
        (&a, json!({ "parent_id": a, "rationale": "r" })),
        (&b, json!({ "parent_id": "C-9999", "rationale": "r" })),
        (&b, json!({ "parent_id": null, "rationale": "  " })),
    ] {
        let (st, v) = call(
            &ctx.app,
            "POST",
            &format!("/api/codes/{child}/move"),
            Some(body.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}: {v}");
    }
    // Back to the root.
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{b}/move"),
        Some(json!({ "parent_id": null, "rationale": "retour" })),
    )
    .await;
    assert!(v["parent_id"].is_null());
}

#[tokio::test]
async fn a_code_is_created_from_a_passage_in_one_call() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let parent = code_id(&ctx.app, "TECH-ADV").await;
    let v0 = ok(&ctx.app, "GET", "/api/codes", None).await["codebook_version"]
        .as_i64()
        .unwrap();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/codes/from-passage",
        Some(json!({
            "name": "Étiquetage à la source par le fournisseur",
            "definition": "Le fournisseur pose l'étiquette avant l'expédition.",
            "includes": "Pose à l'usine", "excludes": "Pose à la réception",
            "parent_id": parent,
            "passage": { "interview_id": "INT-01", "unit_id": unit, "start": 48, "end": 93 },
        })),
    )
    .await;
    let new = &v["code"];
    assert_eq!(new["parent_id"], json!(parent));
    assert_eq!(new["includes"], "Pose à l'usine");
    assert_eq!(new["origin"], "field");
    assert_eq!(new["counts"]["passages"], 1);
    assert_eq!(v["passage"]["codings"][0]["code_id"], new["code_id"]);
    assert_eq!(new["origin_passage_id"], v["passage"]["passage_id"]);
    assert_eq!(v["codebook_version"].as_i64().unwrap(), v0 + 1);
    // Audited.
    let audit = ok(&ctx.app, "GET", "/api/audit", None).await;
    assert!(audit
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "codebook.code.create_from_passage"));
    // Refused: no definition, used name, missing passage.
    for body in [
        json!({ "name": "Autre", "definition": " ", "passage": { "interview_id": "INT-01", "unit_id": unit } }),
        json!({ "name": "ORG-SKILLS", "definition": "d", "passage": { "interview_id": "INT-01", "unit_id": unit } }),
        json!({ "name": "Autre", "definition": "d" }),
    ] {
        let (st, v) = call(
            &ctx.app,
            "POST",
            "/api/codes/from-passage",
            Some(body.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}: {v}");
    }
    let after = ok(&ctx.app, "GET", "/api/codes", None).await;
    assert_eq!(after["codebook_version"].as_i64().unwrap(), v0 + 1);
}

#[tokio::test]
async fn a_code_can_be_created_from_a_highlight() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let h = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 2, "end": 20 })),
    )
    .await;
    let pid = h["passage"]["passage_id"].clone();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/codes/from-passage",
        Some(json!({ "name": "Gestion logistique", "definition": "Le répondant décrit son rôle.", "passage_id": pid })),
    )
    .await;
    assert_eq!(v["passage"]["kind"], "coded");
    let hl = ok(&ctx.app, "GET", "/api/interviews/INT-01/highlights", None).await;
    assert!(hl.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a3_merging_through_the_api_keeps_every_source_and_deduplicates() {
    let ctx = setup().await;
    let a = code_id(&ctx.app, "TECH-ADV").await;
    let b = code_id(&ctx.app, "ORG-SKILLS").await;
    passage(&ctx, "INT-01", 1, &a, 0, 10).await;
    passage(&ctx, "INT-01", 3, &a, 0, 10).await;
    let shared = passage(&ctx, "INT-02", 1, &a, 0, 10).await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{shared}/codings"),
        Some(json!({ "code": b })),
    )
    .await;
    passage(&ctx, "INT-01", 5, &b, 0, 10).await;
    passage(&ctx, "INT-02", 3, &b, 0, 10).await;
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{b}/merge"),
        Some(json!({ "into": a, "rationale": "synonymes" })),
    )
    .await;
    assert_eq!(v["outcome"]["deduplicated_passage_codings"], 1);
    assert_eq!(v["outcome"]["moved_passage_codings"], 2);
    assert_eq!(v["code"]["counts"]["passages"], 5);
    assert_eq!(v["code"]["counts"]["interviews"], 2);
    assert_eq!(v["merged"]["merged_into"], json!(a));
    assert_eq!(v["merged"]["active"], false);
    // The name of the merged code is an alias of the target.
    assert!(v["code"]["aliases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["reason"] == "merge" && x["alias"] == "ORG-SKILLS"));
    // Coding with the old name now codes with the target.
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let p = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 30, "end": 40, "code": "ORG-SKILLS" })),
    )
    .await;
    assert_eq!(p["passage"]["codings"][0]["code_id"], json!(a));
    let (st, _) = call(
        &ctx.app,
        "PUT",
        &format!("/api/codes/{b}"),
        Some(json!({ "definition": "x" })),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "a merged code cannot be edited"
    );
}

#[tokio::test]
async fn a_deactivated_code_cannot_be_applied_and_can_be_reactivated() {
    let ctx = setup().await;
    let a = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{a}/active"),
        Some(json!({ "active": false })),
    )
    .await;
    assert_eq!(v["active"], false);
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "code": a })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{a}/active"),
        Some(json!({ "active": true })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "code": a })),
    )
    .await;
}

#[tokio::test]
async fn every_codebook_change_is_a_revision_and_the_chain_stays_valid() {
    let ctx = setup().await;
    let a = code_id(&ctx.app, "TECH-ADV").await;
    let b = code_id(&ctx.app, "ORG-SKILLS").await;
    let before = ok(&ctx.app, "GET", "/api/codebook/revisions", None).await;
    let n0 = before["revisions"].as_array().unwrap().len();
    ok(
        &ctx.app,
        "PUT",
        &format!("/api/codes/{a}"),
        Some(json!({ "name": "Un" })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{b}/move"),
        Some(json!({ "parent_id": a, "rationale": "r" })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{a}/aliases"),
        Some(json!({ "alias": "synonyme" })),
    )
    .await;
    let after = ok(&ctx.app, "GET", "/api/codebook/revisions", None).await;
    let revs = after["revisions"].as_array().unwrap();
    assert_eq!(revs.len(), n0 + 3);
    let reasons: Vec<&str> = revs[n0..]
        .iter()
        .map(|r| r["reason"].as_str().unwrap())
        .collect();
    assert_eq!(reasons, vec!["rename", "move", "alias"]);
    assert_eq!(after["codebook_version"], revs.last().unwrap()["rev"]);
    let chain: Value = ok(&ctx.app, "GET", "/api/audit/verify", None).await;
    assert_eq!(chain["ok"], true);
}
