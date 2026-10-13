//! V2 codes and search through the HTTP API: statistics (F-45).

mod common;

use axum::http::StatusCode;
use common::{call, code_id, ok, setup, unit_ids, Ctx};
use serde_json::{json, Value};

async fn passage(ctx: &Ctx, interview: &str, n: usize, code: &str) -> String {
    let unit = unit_ids(&ctx.app, interview).await[n].clone();
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/{interview}/passages"),
        Some(json!({ "unit_id": unit, "code": code })),
    )
    .await;
    v["passage"]["passage_id"].as_str().unwrap().to_string()
}

async fn with_sector(ctx: &Ctx) {
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([{"id": "sector", "label": "Secteur", "kind": "choice", "choices": ["public", "privé"]}]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    for (id, v) in [("INT-01", "public"), ("INT-02", "privé")] {
        ok(
            &ctx.app,
            "PUT",
            &format!("/api/interviews/{id}/attributes"),
            Some(json!({"values": {"sector": v}})),
        )
        .await;
    }
}

fn find<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code_id"] == id)
        .unwrap()
}

#[tokio::test]
async fn f45_stats_are_in_api_codes_and_on_the_per_code_route() {
    let ctx = setup().await;
    with_sector(&ctx).await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    passage(&ctx, "INT-01", 1, &adv).await;
    passage(&ctx, "INT-01", 3, &adv).await;
    passage(&ctx, "INT-02", 1, &adv).await;

    let list = ok(&ctx.app, "GET", "/api/codes", None).await;
    let c = &find(&list, &adv)["counts"];
    assert_eq!(c["passages"], 3);
    assert_eq!(c["interviews"], 2);
    assert_eq!(c["cases"], 0);
    assert_eq!(c["attribute_values"]["sector"], 2);

    let st = ok(&ctx.app, "GET", &format!("/api/codes/{adv}/stats"), None).await;
    assert_eq!(st["own"]["passages"], 3);
    assert_eq!(st["own"]["interview_ids"], json!(["INT-01", "INT-02"]));
    let sector = &st["own"]["attributes"][0];
    assert_eq!(sector["attribute_id"], "sector");
    assert_eq!(sector["distinct_values"], 2);
    assert_eq!(sector["values"][0], json!({"value": "privé", "interviews": 1}));

    let st = ok(
        &ctx.app,
        "GET",
        &format!("/api/codes/{adv}/stats?attribute=sector&value=public"),
        None,
    )
    .await;
    assert_eq!(st["own"]["passages"], 2);
    assert_eq!(st["filter"]["value"], "public");

    // The name works as a key; a half filter and the reserved code are refused.
    ok(&ctx.app, "GET", "/api/codes/TECH-ADV/stats", None).await;
    let (s, _) = call(
        &ctx.app,
        "GET",
        &format!("/api/codes/{adv}/stats?attribute=sector"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&ctx.app, "GET", "/api/codes/LATER/stats", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
