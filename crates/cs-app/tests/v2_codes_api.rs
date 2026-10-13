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

#[tokio::test]
async fn f41_code_search_finds_names_aliases_and_definitions() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let made = ok(
        &ctx.app,
        "POST",
        "/api/codes",
        Some(json!({"name": "Économie circulaire", "definition": "Réutilisation des étiquettes."})),
    )
    .await;
    let eco = made["code_id"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{eco}/aliases"),
        Some(json!({"alias": "Circular economy"})),
    )
    .await;

    let r = ok(&ctx.app, "GET", "/api/codes/search?q=econo", None).await;
    assert_eq!(r["items"][0]["code_id"], json!(eco));
    assert_eq!(r["items"][0]["matched"]["field"], "name");
    let r = ok(&ctx.app, "GET", "/api/codes/search?q=circular", None).await;
    assert_eq!(r["items"][0]["matched"]["field"], "alias");
    let r = ok(&ctx.app, "GET", "/api/codes/search?q=%C3%A9tiquet", None).await;
    assert_eq!(r["items"][0]["matched"]["field"], "definition");
    // The legacy label of an imported code.
    let r = ok(&ctx.app, "GET", "/api/codes/search?q=relative", None).await;
    assert_eq!(r["items"][0]["code_id"], json!(adv));
    // Empty query: every usable code, the reserved one apart.
    let r = ok(&ctx.app, "GET", "/api/codes/search", None).await;
    assert_eq!(r["total"], 5);
    let r = ok(&ctx.app, "GET", "/api/codes/search?limit=2", None).await;
    assert_eq!(r["items"].as_array().unwrap().len(), 2);
    let r = ok(&ctx.app, "GET", "/api/codes/search?q=plus&include_system=true", None).await;
    assert_eq!(r["items"][0]["system"], true);
}

#[tokio::test]
async fn f55_full_text_search_with_filters_and_highlights() {
    let ctx = setup().await;
    with_sector(&ctx).await;
    // INT-02: "Notre bibliothèque prête 1 200 livres par an, l'été c'est calme 😀 et le réseau décide du système."
    let r = ok(&ctx.app, "GET", "/api/search/text?q=BIBLIOTHEQUE", None).await;
    assert_eq!(r["total_units"], 1);
    let hit = &r["items"][0];
    assert_eq!(hit["interview_id"], "INT-02");
    assert_eq!(hit["matches"][0]["start"], 6);
    assert_eq!(hit["matches"][0]["end"], 18);
    // Interviews with no mention are listed.
    let mentioned: Vec<(&str, bool)> = r["interviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["interview_id"].as_str().unwrap(), i["mentioned"].as_bool().unwrap()))
        .collect();
    assert_eq!(mentioned, [("INT-01", false), ("INT-02", true)]);
    // Phrase, attribute filter, coded filter.
    let r = ok(&ctx.app, "GET", "/api/search/text?q=le%20r%C3%A9seau&mode=phrase", None).await;
    assert_eq!(r["total_units"], 1);
    let r = ok(&ctx.app, "GET", "/api/search/text?q=reseau%20le&mode=phrase", None).await;
    assert_eq!(r["total_units"], 0);
    let r = ok(&ctx.app, "GET", "/api/search/text?q=tags&attribute=sector&value=public", None).await;
    assert_eq!(r["total_units"], 2);
    let r = ok(&ctx.app, "GET", "/api/search/text?q=tags&attribute=sector&value=priv%C3%A9", None).await;
    assert_eq!(r["total_units"], 0);
    let r = ok(&ctx.app, "GET", "/api/search/text?q=tags&coded=true", None).await;
    assert_eq!(r["total_units"], 0);
    // Interviewer turns need the flag.
    let r = ok(&ctx.app, "GET", "/api/search/text?q=thank", None).await;
    assert_eq!(r["total_units"], 0);
    let r = ok(&ctx.app, "GET", "/api/search/text?q=thank&include_interviewer=true", None).await;
    assert_eq!(r["items"][0]["role"], "interviewer");
    let r = ok(&ctx.app, "GET", "/api/search/text?q=livres&interviews=INT-01", None).await;
    assert_eq!(r["total_units"], 0);
    // Bad requests.
    for uri in [
        "/api/search/text?q=",
        "/api/search/text?q=a&mode=regex",
        "/api/search/text?q=a&attribute=sector",
        "/api/search/text?q=a&interviews=NOPE",
    ] {
        let (s, _) = call(&ctx.app, "GET", uri, None).await;
        assert!(s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND, "{uri}: {s}");
    }
}
