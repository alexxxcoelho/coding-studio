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
    assert_eq!(
        sector["values"][0],
        json!({"value": "privé", "interviews": 1})
    );

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
    let r = ok(
        &ctx.app,
        "GET",
        "/api/codes/search?q=plus&include_system=true",
        None,
    )
    .await;
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
        .map(|i| {
            (
                i["interview_id"].as_str().unwrap(),
                i["mentioned"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(mentioned, [("INT-01", false), ("INT-02", true)]);
    // Phrase, attribute filter, coded filter.
    let r = ok(
        &ctx.app,
        "GET",
        "/api/search/text?q=le%20r%C3%A9seau&mode=phrase",
        None,
    )
    .await;
    assert_eq!(r["total_units"], 1);
    let r = ok(
        &ctx.app,
        "GET",
        "/api/search/text?q=reseau%20le&mode=phrase",
        None,
    )
    .await;
    assert_eq!(r["total_units"], 0);
    let r = ok(
        &ctx.app,
        "GET",
        "/api/search/text?q=tags&attribute=sector&value=public",
        None,
    )
    .await;
    assert_eq!(r["total_units"], 2);
    let r = ok(
        &ctx.app,
        "GET",
        "/api/search/text?q=tags&attribute=sector&value=priv%C3%A9",
        None,
    )
    .await;
    assert_eq!(r["total_units"], 0);
    let r = ok(&ctx.app, "GET", "/api/search/text?q=tags&coded=true", None).await;
    assert_eq!(r["total_units"], 0);
    // Interviewer turns need the flag.
    let r = ok(&ctx.app, "GET", "/api/search/text?q=thank", None).await;
    assert_eq!(r["total_units"], 0);
    let r = ok(
        &ctx.app,
        "GET",
        "/api/search/text?q=thank&include_interviewer=true",
        None,
    )
    .await;
    assert_eq!(r["items"][0]["role"], "interviewer");
    let r = ok(
        &ctx.app,
        "GET",
        "/api/search/text?q=livres&interviews=INT-01",
        None,
    )
    .await;
    assert_eq!(r["total_units"], 0);
    // Bad requests.
    for uri in [
        "/api/search/text?q=",
        "/api/search/text?q=a&mode=regex",
        "/api/search/text?q=a&attribute=sector",
        "/api/search/text?q=a&interviews=NOPE",
    ] {
        let (s, _) = call(&ctx.app, "GET", uri, None).await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND,
            "{uri}: {s}"
        );
    }
}

#[tokio::test]
async fn f56_cooccurrence_matrix_and_drill_down() {
    let ctx = setup().await;
    with_sector(&ctx).await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let cost = code_id(&ctx.app, "TECH-COST").await;
    let skills = code_id(&ctx.app, "ORG-SKILLS").await;
    let u = unit_ids(&ctx.app, "INT-01").await;
    let p1 = passage(&ctx, "INT-01", 1, &adv).await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{p1}/codings"),
        Some(json!({"code": cost})),
    )
    .await;
    passage(&ctx, "INT-01", 3, &adv).await;
    passage(&ctx, "INT-01", 3, &skills).await;
    passage(&ctx, "INT-02", 1, &adv).await;
    let _ = u;

    let m = ok(
        &ctx.app,
        "GET",
        "/api/codes/cooccurrence?relation=same_passage",
        None,
    )
    .await;
    let cells = m["groups"][0]["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0]["units"], 1);
    assert!(m["note"].as_str().unwrap().contains("not causation"));
    let m = ok(
        &ctx.app,
        "GET",
        "/api/codes/cooccurrence?relation=same_unit&group_by=interview",
        None,
    )
    .await;
    assert_eq!(m["groups"].as_array().unwrap().len(), 2);
    let m = ok(
        &ctx.app,
        "GET",
        "/api/codes/cooccurrence?group_by=attribute&attribute=sector",
        None,
    )
    .await;
    assert_eq!(m["groups"][0]["key"], "public");
    let d = ok(
        &ctx.app,
        "GET",
        &format!("/api/codes/cooccurrence/passages?a={adv}&b={skills}&relation=same_unit"),
        None,
    )
    .await;
    assert_eq!(d["n_units"], 1);
    assert_eq!(d["items"][0]["pairs"].as_array().unwrap().len(), 1);
    for uri in [
        "/api/codes/cooccurrence?relation=near",
        "/api/codes/cooccurrence?group_by=attribute",
        "/api/codes/cooccurrence/passages?a=TECH-ADV",
        "/api/codes/cooccurrence?codes=NOPE",
    ] {
        let (s, _) = call(&ctx.app, "GET", uri, None).await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND,
            "{uri}: {s}"
        );
    }
}

#[tokio::test]
async fn f39_split_worklist_decisions_journal_and_undo() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let p1 = passage(&ctx, "INT-01", 1, &adv).await;
    let p2 = passage(&ctx, "INT-01", 3, &adv).await;
    let split = ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{adv}/split"),
        Some(json!({
            "rationale": "two ideas",
            "parts": [
                {"name": "Avantage coût", "definition": "Coût."},
                {"name": "Avantage temps", "definition": "Temps."}
            ]
        })),
    )
    .await;
    let op = split["op_id"].as_i64().unwrap();
    assert_eq!(split["worklist"]["pending"], 2);
    let new_ids: Vec<String> = split["new_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code_id"].as_str().unwrap().to_string())
        .collect();

    let wl = ok(
        &ctx.app,
        "GET",
        &format!("/api/splits/{op}/worklist?status=pending"),
        None,
    )
    .await;
    assert_eq!(wl["items"].as_array().unwrap().len(), 2);
    let (s, _) = call(
        &ctx.app,
        "GET",
        &format!("/api/splits/{op}/worklist?status=bogus"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let wl = ok(
        &ctx.app,
        "PUT",
        &format!("/api/splits/{op}/worklist"),
        Some(json!({"decisions": [
            {"passage_id": p1, "action": "assign", "targets": [new_ids[0]]},
            {"passage_id": p2, "action": "leave", "note": "ambigu"}
        ]})),
    )
    .await;
    assert_eq!(
        (
            wl["assigned"].as_i64(),
            wl["left"].as_i64(),
            wl["pending"].as_i64()
        ),
        (Some(1), Some(1), Some(0))
    );

    let ops = ok(&ctx.app, "GET", &format!("/api/code-ops?code={adv}"), None).await;
    assert_eq!(ops["operations"][0]["kind"], "split");
    assert!(ops["operations"][0]["undone"].is_null());

    let (s, _) = call(
        &ctx.app,
        "POST",
        &format!("/api/code-ops/{op}/undo"),
        Some(json!({"rationale": ""})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let undo = ok(
        &ctx.app,
        "POST",
        &format!("/api/code-ops/{op}/undo"),
        Some(json!({"rationale": "erreur"})),
    )
    .await;
    assert_eq!(undo["report"]["kind"], "split");
    let (s, _) = call(
        &ctx.app,
        "POST",
        &format!("/api/code-ops/{op}/undo"),
        Some(json!({"rationale": "encore"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let ops = ok(&ctx.app, "GET", "/api/code-ops", None).await;
    assert_eq!(ops["operations"][0]["undone"]["rationale"], "erreur");
}

#[tokio::test]
async fn f39_a_merge_is_journaled_and_can_be_undone_through_the_api() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let cost = code_id(&ctx.app, "TECH-COST").await;
    passage(&ctx, "INT-01", 1, &adv).await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/codes/{adv}/merge"),
        Some(json!({"into": cost, "rationale": "doublon"})),
    )
    .await;
    let ops = ok(&ctx.app, "GET", "/api/code-ops", None).await;
    let op = ops["operations"][0]["op_id"].as_i64().unwrap();
    assert_eq!(ops["operations"][0]["kind"], "merge");
    let undo = ok(
        &ctx.app,
        "POST",
        &format!("/api/code-ops/{op}/undo"),
        Some(json!({"rationale": "non"})),
    )
    .await;
    assert_eq!(undo["report"]["kind"], "merge");
    let (s, _) = call(&ctx.app, "GET", "/api/splits/999/worklist", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn f56_co_occurrence_csv_is_exported_and_in_the_manifest() {
    let ctx = setup().await;
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let cost = code_id(&ctx.app, "TECH-COST").await;
    let p1 = passage(&ctx, "INT-01", 1, &adv).await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{p1}/codings"),
        Some(json!({"code": cost})),
    )
    .await;
    for (uri, key) in [
        ("/api/interviews/INT-01/export", "dir"),
        ("/api/export/cross-case", "folder"),
    ] {
        let exp = ok(&ctx.app, "POST", uri, None).await;
        let dir = std::path::PathBuf::from(exp[key].as_str().unwrap());
        let csv = std::fs::read_to_string(dir.join("co-occurrence.csv")).unwrap();
        let mut lines = csv.lines();
        assert_eq!(
            lines.next().unwrap(),
            "relation,group,kind,code_a_id,code_a_name,code_b_id,code_b_name,passages,units,interviews,passage_pairs,jaccard"
        );
        let pair = csv
            .lines()
            .find(|l| l.starts_with("same_passage,all,pair,"))
            .unwrap_or_else(|| panic!("no pair row: {csv}"));
        assert!(
            pair.contains(&adv) && pair.contains(&cost) && pair.ends_with(",1,1,1,1.0000"),
            "{pair}"
        );
        assert!(csv.lines().any(|l| l.starts_with("same_unit,all,single,")));
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        assert!(manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "co-occurrence.csv"));
        let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(readme.contains("`co-occurrence.csv`") || readme.contains("co-occurrence.csv"));
    }
}
