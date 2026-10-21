mod common;
use axum::http::StatusCode;
use common::{call, ok, setup};
use serde_json::{json, Value};
const BASE: &str = "/api/codebook/json/import";

async fn document(ctx: &common::Ctx) -> Value {
    ok(&ctx.app, "GET", "/api/codebook/json", None).await
}

#[tokio::test]
async fn codebook_json_roundtrip_rename_nest_and_reorder_preserves_identity() {
    let ctx = setup().await;
    let original = document(&ctx).await;
    let preview = ok(
        &ctx.app,
        "POST",
        &format!("{BASE}/preview"),
        Some(json!({"raw":original.to_string()})),
    )
    .await;
    assert_eq!(preview["count"], 0);
    let mut next = original.clone();
    let nodes = next["codes"].as_array_mut().unwrap();
    let mut child = nodes.remove(1);
    let token = child["token"].clone();
    let id = child["code_id"].clone();
    child["name"] = json!("Renamed cost");
    child["definition"] = json!("A simpler definition.");
    nodes[0]["children"] = json!([child]);
    nodes.reverse();
    let p = ok(
        &ctx.app,
        "POST",
        &format!("{BASE}/preview"),
        Some(json!({"raw":next.to_string()})),
    )
    .await;
    assert!(p["count"].as_u64().unwrap() > 0);
    assert_eq!(original, document(&ctx).await);
    ok(
        &ctx.app,
        "POST",
        BASE,
        Some(json!({"raw":next.to_string(),"expected_digest":p["digest"]})),
    )
    .await;
    assert_eq!(next, document(&ctx).await);
    let all = ok(&ctx.app, "GET", "/api/codes", None).await;
    let child = all["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["code_id"] == id)
        .unwrap();
    assert_eq!(child["token"], token);
    let conn = rusqlite::Connection::open(&ctx.db).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM code_aliases WHERE reason <> 'legacy_token'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn codebook_json_rejects_duplicate_tree_ids_changed_tokens_and_stale_preview() {
    let ctx = setup().await;
    let original = document(&ctx).await;
    let mut duplicate = original.clone();
    let node = duplicate["codes"][0].clone();
    duplicate["codes"].as_array_mut().unwrap().push(node);
    let mut badtoken = original.clone();
    badtoken["codes"][0]["token"] = json!("CHANGED");
    let mut badname = original.clone();
    badname["codes"][0]["name"] = badname["codes"][1]["name"].clone();
    for doc in [duplicate, badtoken, badname] {
        assert_eq!(
            call(
                &ctx.app,
                "POST",
                &format!("{BASE}/preview"),
                Some(json!({"raw":doc.to_string()}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(original, document(&ctx).await);
    }
    let p = ok(
        &ctx.app,
        "POST",
        &format!("{BASE}/preview"),
        Some(json!({"raw":original.to_string()})),
    )
    .await;
    let mut edited = original.clone();
    edited["codes"][0]["name"] = json!("Changed");
    assert_eq!(
        call(
            &ctx.app,
            "POST",
            BASE,
            Some(json!({"raw":edited.to_string(),"expected_digest":p["digest"]}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(original, document(&ctx).await);
}

#[tokio::test]
async fn codebook_json_adds_new_children_and_preserves_omitted_codes() {
    let ctx = setup().await;
    let original = document(&ctx).await;
    let mut doc = original.clone();
    let root = doc["codes"][0].clone();
    doc["codes"] = json!([root]);
    doc["codes"][0]["children"] = json!([{"code_id":"C-9000","token":"C-9000","name":"New child","definition":"New dimension","active":true,"in_prompt":true}]);
    let p = ok(
        &ctx.app,
        "POST",
        &format!("{BASE}/preview"),
        Some(json!({"raw":doc.to_string()})),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        BASE,
        Some(json!({"raw":doc.to_string(),"expected_digest":p["digest"]})),
    )
    .await;
    let all = ok(&ctx.app, "GET", "/api/codes", None).await;
    assert_eq!(all["codes"].as_array().unwrap().len(), 6); // four original + new + virtual system
    assert!(all["codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["code_id"] == "C-9000"));
}

#[tokio::test]
async fn codebook_json_rolls_back_every_change_on_storage_failure() {
    let ctx = setup().await;
    let original = document(&ctx).await;
    let mut edited = original.clone();
    edited["codes"][0]["name"] = json!("First renamed");
    edited["codes"][1]["name"] = json!("Second renamed");
    let p = ok(
        &ctx.app,
        "POST",
        &format!("{BASE}/preview"),
        Some(json!({"raw":edited.to_string()})),
    )
    .await;
    let conn = rusqlite::Connection::open(&ctx.db).unwrap();
    conn.execute_batch("CREATE TRIGGER test_codebook_failure BEFORE UPDATE ON codes WHEN NEW.name='Second renamed' BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert_eq!(
        call(
            &ctx.app,
            "POST",
            BASE,
            Some(json!({"raw":edited.to_string(),"expected_digest":p["digest"]}))
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(original, document(&ctx).await);
}
