//! V1 lot 3 through the HTTP API: the lexical workbench. Acceptance test A27
//! (a highlight yields keyphrases and stems, the keyword table counts passages
//! and interviews, similar passages are ranked deterministically, no word cloud
//! anywhere, everything works with no model configured), plus KWIC, term
//! frequency per attribute value and the language setting.

mod common;

use axum::http::StatusCode;
use common::{call, ok, setup, unit_ids};
use serde_json::{json, Value};

/// A highlight (a passage with no code) over a whole unit.
async fn highlight(ctx: &common::Ctx, interview: &str, unit: &str) -> String {
    let v = ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/{interview}/passages"),
        Some(json!({ "unit_id": unit })),
    )
    .await;
    assert_eq!(v["passage"]["kind"], "highlight");
    v["passage"]["passage_id"].as_str().unwrap().to_string()
}

/// A third interview in English, near the vocabulary of INT-01.
async fn add_interview(ctx: &common::Ctx) {
    let dir = ctx.root.join("INT-03_Tags");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("02_transcript_cleaned_diarised.md"),
        "---\ninterview_id: INT-03\n---\n\n**INT `[00:01]`:** And the tags?\n\n**RESP `[00:05]`:** The tags were expensive and our skills for the integration were thin.\n\n**INT `[00:20]`:** Anything else?\n\n**RESP `[00:24]`:** Training the team took longer than the hardware itself.\n",
    )
    .unwrap();
    ok(
        &ctx.app,
        "POST",
        "/api/library/scan",
        Some(json!({ "root": ctx.root })),
    )
    .await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-03/segment",
        Some(json!({})),
    )
    .await;
}

async fn english(ctx: &common::Ctx) {
    let mut langs = json!({"INT-01": "en"});
    let has_third = common::call(&ctx.app, "GET", "/api/interviews/INT-03", None)
        .await
        .0
        == StatusCode::OK;
    if has_third {
        langs["INT-03"] = json!("en");
    }
    let v = ok(
        &ctx.app,
        "PUT",
        "/api/lexicon/config",
        Some(json!({"language": "fr", "interview_languages": langs})),
    )
    .await;
    assert_eq!(v["interview_languages"]["INT-01"], "en");
}

#[tokio::test]
async fn the_language_defaults_to_french_and_is_validated() {
    let ctx = setup().await;
    let v = ok(&ctx.app, "GET", "/api/lexicon/config", None).await;
    assert_eq!(v["language"], "fr");
    assert_eq!(v["interview_languages"], json!({}));
    for bad in [
        json!({"language": "xx"}),
        json!({"language": "fr", "interview_languages": {"INT-99": "en"}}),
        json!({"language": "fr", "interview_languages": {"INT-01": "klingon"}}),
    ] {
        let (st, v) = call(&ctx.app, "PUT", "/api/lexicon/config", Some(bad)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    }
    add_interview(&ctx).await;
    english(&ctx).await;
    let s = ok(&ctx.app, "GET", "/api/lexicon/status", None).await;
    assert_eq!(s["config"]["interview_languages"]["INT-03"], "en");
    assert_eq!(s["index"]["n_interviews"], 3);
    assert_eq!(s["method"]["ai"], false);
    assert_eq!(s["languages"], json!(["fr", "en", "de"]));
    let audit = ok(&ctx.app, "GET", "/api/audit", None).await;
    assert!(audit
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "lexicon.config.update"));
}

#[tokio::test]
async fn a27_a_highlight_yields_keyphrases_and_stems_with_no_model_configured() {
    let ctx = setup().await;
    english(&ctx).await;
    let units = unit_ids(&ctx.app, "INT-01").await;
    let pid = highlight(&ctx, "INT-01", &units[5]).await;
    let panel = ok(
        &ctx.app,
        "GET",
        &format!("/api/passages/{pid}/lexical"),
        None,
    )
    .await;
    assert_eq!(panel["language"], "en");
    assert_eq!(panel["provisional"], true);
    assert!(!panel["keyphrases"].as_array().unwrap().is_empty());
    assert!(panel["label"].is_string());
    let stems: Vec<&str> = panel["stems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["stem"].as_str().unwrap())
        .collect();
    assert!(stems.contains(&"skill"), "{stems:?}");
    assert!(stems.contains(&"expens"), "{stems:?}");
    assert!(panel["keyphrases"][0]["score"].as_f64().unwrap() > 0.0);
    // No model: the manual workflow is the only provider that is configured.
    let providers = ok(&ctx.app, "GET", "/api/providers", None).await;
    let configured: Vec<&Value> = providers
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["configured"] == true)
        .collect();
    assert_eq!(configured.len(), 1);
    assert_eq!(configured[0]["kind"], "copy_paste");
    // Same text, same panel.
    assert_eq!(
        panel,
        ok(
            &ctx.app,
            "GET",
            &format!("/api/passages/{pid}/lexical"),
            None
        )
        .await
    );
    let (st, _) = call(&ctx.app, "GET", "/api/passages/P-99999/lexical", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a27_the_keyword_table_counts_passages_and_interviews() {
    let ctx = setup().await;
    add_interview(&ctx).await;
    english(&ctx).await;
    let u1 = unit_ids(&ctx.app, "INT-01").await;
    let u3 = unit_ids(&ctx.app, "INT-03").await;
    // "tags" occurs in INT-01 U006 and INT-03 U002; "skills" too.
    let p1 = highlight(&ctx, "INT-01", &u1[5]).await;
    let p3 = highlight(&ctx, "INT-03", &u3[1]).await;
    let p3b = highlight(&ctx, "INT-03", &u3[3]).await;
    let t = ok(&ctx.app, "GET", "/api/lexicon/keywords", None).await;
    assert_eq!(t["scope"], "highlights");
    assert_eq!(t["n_passages"], 3);
    let rows = t["rows"].as_array().unwrap();
    let tag = rows.iter().find(|r| r["stem"] == "tag").unwrap();
    assert_eq!(tag["n_passages"], 2);
    assert_eq!(tag["n_interviews"], 2);
    let mut ids: Vec<String> = tag["passage_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    ids.sort();
    let mut expected = vec![p1.clone(), p3.clone()];
    expected.sort();
    assert_eq!(ids, expected);
    assert!(!ids.contains(&p3b));
    assert!(tag["keyness"].as_f64().is_some());
    // At least two passages.
    let t2 = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/keywords?min_passages=2",
        None,
    )
    .await;
    assert!(t2["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["n_passages"].as_u64().unwrap() >= 2));
    // One interview only.
    let t3 = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/keywords?interview_id=INT-03",
        None,
    )
    .await;
    assert_eq!(t3["n_passages"], 2);
    // A coded passage leaves the highlights; the scope "passages" has it back.
    ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{p1}/codings"),
        Some(json!({"code": "TECH-COST"})),
    )
    .await;
    let t4 = ok(&ctx.app, "GET", "/api/lexicon/keywords", None).await;
    assert_eq!(t4["n_passages"], 2);
    let t5 = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/keywords?scope=passages",
        None,
    )
    .await;
    assert_eq!(t5["n_passages"], 3);
    let (st, v) = call(&ctx.app, "GET", "/api/lexicon/keywords?scope=all", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    let (st, _) = call(
        &ctx.app,
        "GET",
        "/api/lexicon/keywords?attribute=sector",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a27_similar_passages_are_ranked_deterministically_and_say_what_they_cannot_see() {
    let ctx = setup().await;
    add_interview(&ctx).await;
    english(&ctx).await;
    let u1 = unit_ids(&ctx.app, "INT-01").await;
    let p = highlight(&ctx, "INT-01", &u1[5]).await;
    let a = ok(
        &ctx.app,
        "GET",
        &format!("/api/passages/{p}/similar?k=5"),
        None,
    )
    .await;
    let b = ok(
        &ctx.app,
        "GET",
        &format!("/api/passages/{p}/similar?k=5"),
        None,
    )
    .await;
    assert_eq!(a, b, "same input, same ranking");
    let items = a["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert_eq!(items[0]["interview_id"], "INT-03", "{items:?}");
    assert_eq!(items[0]["unit_id"], "INT-03-U002");
    let scores: Vec<f64> = items.iter().map(|i| i["score"].as_f64().unwrap()).collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]));
    assert!(items[0]["shared_terms"].as_array().unwrap().len() >= 2);
    assert!(items[0]["shared_terms"][0]["stem"].is_string());
    assert!(a["note"].as_str().unwrap().contains("synonymes"));
    // Interviewer turns are never proposed.
    for i in items {
        let u = i["unit_id"].as_str().unwrap();
        assert!(!["INT-03-U001", "INT-03-U003"].contains(&u), "{u}");
    }
    // From a unit, not a passage: itself left out.
    let from_unit = ok(
        &ctx.app,
        "GET",
        &format!("/api/interviews/INT-01/units/{}/similar", u1[5]),
        None,
    )
    .await;
    assert!(from_unit["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["unit_id"] != u1[5]));
    // Coverage: skip what already carries the code.
    let p3 = highlight(&ctx, "INT-03", "INT-03-U002").await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{p3}/codings"),
        Some(json!({"code": "ORG-SKILLS"})),
    )
    .await;
    let c = ok(
        &ctx.app,
        "GET",
        &format!("/api/passages/{p}/similar?k=5"),
        None,
    )
    .await;
    assert_eq!(c["items"][0]["coded_with"][0]["name"], "ORG-SKILLS");
    let d = ok(
        &ctx.app,
        "GET",
        &format!("/api/passages/{p}/similar?k=5&without_code=ORG-SKILLS"),
        None,
    )
    .await;
    assert!(d["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["unit_id"] != "INT-03-U002"));
    let none = ok(
        &ctx.app,
        "GET",
        &format!("/api/passages/{p}/similar?min_score=0.99"),
        None,
    )
    .await;
    assert!(none["items"].as_array().unwrap().is_empty());
    let (st, _) = call(&ctx.app, "GET", "/api/passages/P-99999/similar", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn kwic_searches_the_corpus_by_word_stem_attribute_and_speaker() {
    let ctx = setup().await;
    add_interview(&ctx).await;
    english(&ctx).await;
    // Exact word, accents and case ignored.
    let v = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=Tags", None).await;
    assert_eq!(v["total"], 3, "{v}");
    assert_eq!(v["n_interviews"], 2);
    let hit = &v["hits"][0];
    assert_eq!(hit["role"], "respondent");
    assert!(hit["left"].is_string() && hit["right"].is_string());
    assert!(hit["start"].as_u64().unwrap() < hit["end"].as_u64().unwrap());
    // Stem: "tag" finds "tags" and "tagged" alike; here the stem of "tags".
    let s = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=tag&stem=true", None).await;
    assert_eq!(s["total"], 3);
    let e = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=tag", None).await;
    assert_eq!(e["total"], 0, "no stem: exact word only");
    // Speakers.
    let i = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/kwic?q=costs&speakers=interviewer",
        None,
    )
    .await;
    assert_eq!(i["total"], 1);
    let resp = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=costs", None).await;
    assert_eq!(resp["total"], 0, "the default reads the respondent only");
    let all = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/kwic?q=costs&speakers=all",
        None,
    )
    .await;
    assert_eq!(all["total"], 1);
    assert_eq!(all["hits"][0]["role"], "interviewer");
    // Phrase.
    let ph = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=our%20skills", None).await;
    assert_eq!(ph["total"], 1);
    // Page.
    let pg = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/kwic?q=tags&limit=1&offset=1",
        None,
    )
    .await;
    assert_eq!(pg["total"], 3);
    assert_eq!(pg["hits"].as_array().unwrap().len(), 1);
    // Filter by attribute.
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([
        {"id": "kind", "label": "Type", "kind": "choice", "choices": ["communale", "privée"]}
    ]);
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
        "/api/interviews/INT-03/attributes",
        Some(json!({"values": {"kind": "privée"}})),
    )
    .await;
    let f = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/kwic?q=tags&attribute=kind&value=privée",
        None,
    )
    .await;
    assert_eq!(f["total"], 1);
    assert_eq!(f["hits"][0]["interview_id"], "INT-03");
    let f2 = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/kwic?q=tags&attribute=kind&value=communale&interview_id=INT-03",
        None,
    )
    .await;
    assert_eq!(f2["total"], 0, "intersection of the two filters");
    // Errors.
    for url in [
        "/api/lexicon/kwic",
        "/api/lexicon/kwic?q=%20",
        "/api/lexicon/kwic?q=tags&attribute=kind",
        "/api/lexicon/kwic?q=tags&attribute=nope&value=x",
        "/api/lexicon/kwic?q=tags&speakers=nobody",
    ] {
        let (st, v) = call(&ctx.app, "GET", url, None).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{url}: {v}");
    }
    let (st, _) = call(
        &ctx.app,
        "GET",
        "/api/lexicon/kwic?q=tags&interview_id=INT-99",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn kwic_follows_a_correction_of_the_transcript() {
    let ctx = setup().await;
    let before = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=inventory", None).await;
    assert_eq!(before["total"], 1);
    let units = unit_ids(&ctx.app, "INT-01").await;
    ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/INT-01/units/{}/corrections", units[3]),
        Some(json!({
            "text": "The stock accuracy went from sixty to ninety-eight percent in one year.",
            "role": "respondent",
            "reason": "wording checked against the recording",
        })),
    )
    .await;
    let after = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=inventory", None).await;
    assert_eq!(after["total"], 0);
    let stock = ok(&ctx.app, "GET", "/api/lexicon/kwic?q=stock", None).await;
    assert_eq!(stock["total"], 1);
}

#[tokio::test]
async fn term_frequency_per_attribute_value_and_top_terms() {
    let ctx = setup().await;
    add_interview(&ctx).await;
    english(&ctx).await;
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([
        {"id": "kind", "label": "Type", "kind": "choice", "choices": ["communale", "privée"]}
    ]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    for (iv, v) in [
        ("INT-01", "privée"),
        ("INT-03", "privée"),
        ("INT-02", "communale"),
    ] {
        ok(
            &ctx.app,
            "PUT",
            &format!("/api/interviews/{iv}/attributes"),
            Some(json!({"values": {"kind": v}})),
        )
        .await;
    }
    let v = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/term-frequency?attribute=kind&terms=tags,skills&stem=true",
        None,
    )
    .await;
    assert_eq!(v["mode"], "terms");
    let tags = &v["terms"][0];
    assert_eq!(tags["term"], "tags");
    let by: Vec<(String, u64, u64)> = tags["by_value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["value"].as_str().unwrap().to_string(),
                x["n_occurrences"].as_u64().unwrap(),
                x["n_interviews"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        by,
        vec![("communale".into(), 0, 1), ("privée".into(), 3, 2)]
    );
    assert!(tags["by_value"][1]["per_1000_words"].as_f64().unwrap() > 0.0);
    // No term: the most frequent terms of each value.
    let top = ok(
        &ctx.app,
        "GET",
        "/api/lexicon/term-frequency?attribute=kind&top=3",
        None,
    )
    .await;
    assert_eq!(top["mode"], "top_terms");
    assert_eq!(top["values"].as_array().unwrap().len(), 2);
    assert_eq!(top["values"][0]["terms"].as_array().unwrap().len(), 3);
    let (st, _) = call(
        &ctx.app,
        "GET",
        "/api/lexicon/term-frequency?terms=tags",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        &ctx.app,
        "GET",
        "/api/lexicon/term-frequency?attribute=nope&terms=tags",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a27_there_is_no_word_cloud_anywhere() {
    let ctx = setup().await;
    let v = ok(&ctx.app, "GET", "/api/lexicon/status", None).await;
    let text = v.to_string().to_lowercase();
    assert!(!text.contains("cloud") && !text.contains("nuage"));
    for url in [
        "/api/lexicon/cloud",
        "/api/lexicon/wordcloud",
        "/api/lexicon/word-cloud",
    ] {
        let (st, _) = call(&ctx.app, "GET", url, None).await;
        assert_ne!(st, StatusCode::OK, "{url}");
    }
    // The source tree of the routes does not mention it either.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lexicon.rs"))
        .unwrap()
        .to_lowercase();
    assert!(!src.contains("\"/api/lexicon/cloud") && !src.contains("wordcloud"));
}
