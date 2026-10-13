//! V1 lot 1 through the HTTP API: passages, codings, highlights, LATER.
//! Acceptance tests A1, A2 and A11, and the rules around them.

mod common;

use axum::http::StatusCode;
use common::{call, code_id, ok, setup, unit_ids};
use serde_json::{json, Value};

const LONG: &str = "The first wave of the rollout took six months for the two flagship stores, mostly because the receiving teams needed training on the new handheld readers, while the hardware itself had been ready in three weeks and nobody on the project expected that gap.";

async fn add_long_interview(ctx: &common::Ctx) {
    let dir = ctx.root.join("INT-03_Long");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("02_transcript_cleaned_diarised.md"),
        format!("---\ninterview_id: INT-03\n---\n\n**INT `[00:01]`:** How long did it take?\n\n**RESP `[00:05]`:** {LONG}\n"),
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

fn chars(s: &str, a: usize, b: usize) -> String {
    s.chars().skip(a).take(b - a).collect()
}

#[tokio::test]
async fn a1_select_chars_120_to_245_code_reopen_and_compare() {
    let ctx = setup().await;
    add_long_interview(&ctx).await;
    let units = unit_ids(&ctx.app, "INT-03").await;
    let unit = &units[1];
    assert!(LONG.chars().count() > 245);
    let expected = chars(LONG, 120, 245);
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-03/passages",
        Some(json!({
            "unit_id": unit, "start": 120, "end": 245,
            "text": expected, "code": adv,
        })),
    )
    .await;
    assert_eq!(v["created"], true);
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    // Reopen.
    let p = ok(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(p["text"], expected);
    assert_eq!(p["start"], 120);
    assert_eq!(p["end"], 245);
    assert_eq!(p["before"], chars(LONG, 60, 120));
    assert_eq!(p["after"], chars(LONG, 245, 305.min(LONG.chars().count())));
    assert_eq!(p["stale"], false);
    assert_eq!(p["segmentation_hash"].as_str().unwrap().len(), 64);
    assert_eq!(p["anchor_digest"].as_str().unwrap().len(), 64);
    // Listed for the interview and the unit.
    let list = ok(
        &ctx.app,
        "GET",
        &format!("/api/interviews/INT-03/passages?unit_id={unit}"),
        None,
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a1_offsets_that_do_not_return_the_expected_text_are_refused() {
    let ctx = setup().await;
    add_long_interview(&ctx).await;
    let unit = unit_ids(&ctx.app, "INT-03").await[1].clone();
    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/interviews/INT-03/passages",
        Some(json!({ "unit_id": unit, "start": 120, "end": 245, "text": "not that" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["kind"], "validation");
    let list = ok(&ctx.app, "GET", "/api/interviews/INT-03/passages", None).await;
    assert!(list.as_array().unwrap().is_empty());
    for body in [
        json!({ "unit_id": unit, "start": 10, "end": 10 }),
        json!({ "unit_id": unit, "start": 20, "end": 10 }),
        json!({ "unit_id": unit, "start": 0, "end": 99999 }),
        json!({ "unit_id": unit, "start": 5 }),
        json!({ "unit_id": "INT-03-U999" }),
    ] {
        let (st, v) = call(
            &ctx.app,
            "POST",
            "/api/interviews/INT-03/passages",
            Some(body.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body} -> {v}");
    }
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/interviews/NOPE/passages",
        Some(json!({ "unit_id": "x" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a2_two_codes_on_one_passage_and_overlapping_passages() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let skills = code_id(&ctx.app, "ORG-SKILLS").await;
    let a = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 2, "end": 20, "code": adv })),
    )
    .await;
    let pid = a["passage"]["passage_id"].as_str().unwrap().to_string();
    let b = ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{pid}/codings"),
        Some(json!({ "code": skills })),
    )
    .await;
    assert_eq!(b["codings"].as_array().unwrap().len(), 2);
    // Overlapping passage with the first code again: a different passage.
    let c = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 10, "end": 40, "code": adv })),
    )
    .await;
    assert_ne!(c["passage"]["passage_id"], json!(pid));
    let list = ok(
        &ctx.app,
        "GET",
        &format!("/api/interviews/INT-01/passages?unit_id={unit}"),
        None,
    )
    .await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    let first = list.iter().find(|p| p["passage_id"] == json!(pid)).unwrap();
    let names: Vec<&str> = first["codings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code_id"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec![adv.as_str(), skills.as_str()]);
    // Filtered by code.
    let by_code = ok(
        &ctx.app,
        "GET",
        &format!("/api/interviews/INT-01/passages?code={skills}"),
        None,
    )
    .await;
    assert_eq!(by_code.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn whole_unit_coding_stays_possible() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[3].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "code": adv })),
    )
    .await;
    assert_eq!(v["passage"]["whole_unit"], true);
    assert_eq!(v["passage"]["start"], 0);
}

#[tokio::test]
async fn a11_utf16_offsets_from_the_browser_round_trip_without_shift() {
    let ctx = setup().await;
    let units = unit_ids(&ctx.app, "INT-02").await;
    let unit = units[1].clone();
    let detail = ok(&ctx.app, "GET", "/api/interviews/INT-02", None).await;
    let text = detail["segmentation"]["units"][1]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains('😀') && text.contains('é') && text.contains('\''));
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    // Passage "et le réseau" which comes after the emoji.
    let target = "et le réseau";
    let scalar_start = text
        .find(target)
        .map(|b| text[..b].chars().count())
        .unwrap();
    let utf16_start: usize = text.chars().take(scalar_start).map(char::len_utf16).sum();
    let utf16_len: usize = target.chars().map(char::len_utf16).sum();
    assert_ne!(
        utf16_start, scalar_start,
        "the emoji shifts UTF-16 positions"
    );
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-02/passages",
        Some(json!({
            "unit_id": unit, "offset_unit": "utf16",
            "start": utf16_start, "end": utf16_start + utf16_len,
            "text": target, "code": adv,
        })),
    )
    .await;
    let p = &v["passage"];
    assert_eq!(p["text"], target);
    assert_eq!(p["start"], scalar_start);
    assert_eq!(p["start_utf16"], utf16_start);
    assert_eq!(p["end_utf16"], utf16_start + utf16_len);
    // The emoji itself, and an apostrophe passage.
    for needle in ["😀", "l'été"] {
        let s = text
            .find(needle)
            .map(|b| text[..b].chars().count())
            .unwrap();
        let u: usize = text.chars().take(s).map(char::len_utf16).sum();
        let n: usize = needle.chars().map(char::len_utf16).sum();
        let v = ok(
            &ctx.app,
            "POST",
            "/api/interviews/INT-02/passages",
            Some(json!({
                "unit_id": unit, "offset_unit": "utf16",
                "start": u, "end": u + n, "text": needle,
            })),
        )
        .await;
        assert_eq!(v["passage"]["text"], needle);
        assert_eq!(v["passage"]["start"], s);
    }
    // Half an emoji is refused.
    let emoji_scalar = text.chars().position(|c| c == '😀').unwrap();
    let emoji_utf16: usize = text.chars().take(emoji_scalar).map(char::len_utf16).sum();
    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/interviews/INT-02/passages",
        Some(json!({
            "unit_id": unit, "offset_unit": "utf16",
            "start": emoji_utf16 + 1, "end": emoji_utf16 + 3,
        })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    // The same emoji by scalar positions gives the same passage.
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-02/passages",
        Some(json!({
            "unit_id": unit, "start": emoji_scalar, "end": emoji_scalar + 1,
        })),
    )
    .await;
    assert_eq!(v["created"], false);
}

#[tokio::test]
async fn an_interviewer_turn_is_refused_with_a_clear_error() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[0].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    for body in [
        json!({ "unit_id": unit }),
        json!({ "unit_id": unit, "code": adv }),
        json!({ "unit_id": unit, "code": "LATER" }),
        json!({ "unit_id": unit, "start": 0, "end": 5 }),
    ] {
        let (st, v) = call(
            &ctx.app,
            "POST",
            "/api/interviews/INT-01/passages",
            Some(body),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
        assert!(
            v["error"].as_str().unwrap().contains("interviewer"),
            "{}",
            v["error"]
        );
    }
    let list = ok(&ctx.app, "GET", "/api/interviews/INT-01/passages", None).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn highlights_are_listed_per_interview_and_convert_to_coded_passages() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 2, "end": 20 })),
    )
    .await;
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    assert_eq!(v["passage"]["kind"], "highlight");
    let hl = ok(&ctx.app, "GET", "/api/interviews/INT-01/highlights", None).await;
    assert_eq!(hl.as_array().unwrap().len(), 1);
    let other = ok(&ctx.app, "GET", "/api/interviews/INT-02/highlights", None).await;
    assert!(other.as_array().unwrap().is_empty());
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let coded = ok(
        &ctx.app,
        "POST",
        &format!("/api/passages/{pid}/codings"),
        Some(json!({ "code": adv })),
    )
    .await;
    assert_eq!(coded["kind"], "coded");
    let hl = ok(&ctx.app, "GET", "/api/interviews/INT-01/highlights", None).await;
    assert!(hl.as_array().unwrap().is_empty());
    // Remove the code with a reason: back to a highlight.
    let back = ok(
        &ctx.app,
        "DELETE",
        &format!("/api/passages/{pid}/codings/{adv}?reason=erreur"),
        None,
    )
    .await;
    assert_eq!(back["kind"], "highlight");
}

#[tokio::test]
async fn later_parks_a_passage_and_is_listed_per_interview() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 2, "end": 20, "code": "LATER", "note": "sens du mot" })),
    )
    .await;
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    assert_eq!(v["passage"]["codings"][0]["code_name"], "Plus tard");
    assert_eq!(v["passage"]["codings"][0]["note"], "sens du mot");
    let items = ok(&ctx.app, "GET", "/api/interviews/INT-01/later", None).await;
    assert_eq!(items.as_array().unwrap().len(), 1);
    let all = ok(&ctx.app, "GET", "/api/later", None).await;
    assert_eq!(
        all["summary"],
        json!([{ "interview_id": "INT-01", "n": 1 }])
    );
    // Not counted in the codes, and listed with the system flag.
    let codes = ok(&ctx.app, "GET", "/api/codes", None).await;
    let later = codes["codes"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(later["code_id"], "LATER");
    assert_eq!(later["system"], true);
    assert_eq!(later["counts"]["passages"], 0);
    let total: u64 = codes["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["counts"]["passages"].as_u64().unwrap())
        .sum();
    assert_eq!(total, 0);
    // Resolved.
    ok(
        &ctx.app,
        "DELETE",
        &format!("/api/passages/{pid}/codings/LATER?reason=traité"),
        None,
    )
    .await;
    let all = ok(&ctx.app, "GET", "/api/later", None).await;
    assert_eq!(all["summary"], json!([]));
    // It cannot be edited or moved like a normal code.
    let (st, _) = call(
        &ctx.app,
        "PUT",
        "/api/codes/LATER",
        Some(json!({ "name": "Autre" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn deleting_a_passage_is_logical_and_audited() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 2, "end": 20, "code": adv })),
    )
    .await;
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "DELETE",
        &format!("/api/passages/{pid}?reason=mauvaise%20s%C3%A9lection"),
        None,
    )
    .await;
    let (st, _) = call(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let list = ok(&ctx.app, "GET", "/api/interviews/INT-01/passages", None).await;
    assert!(list.as_array().unwrap().is_empty());
    let audit = ok(&ctx.app, "GET", "/api/audit?interview_id=INT-01", None).await;
    let actions: Vec<&str> = audit
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"passage.create"));
    assert!(actions.contains(&"passage.code"));
    assert!(actions.contains(&"passage.delete"));
    let chain = ok(&ctx.app, "GET", "/api/audit/verify", None).await;
    assert_eq!(chain["ok"], true);
}

#[tokio::test]
async fn a_corrected_unit_flags_its_passages_stale_without_moving_them() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    let v = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "start": 2, "end": 20, "code": adv })),
    )
    .await;
    let pid = v["passage"]["passage_id"].as_str().unwrap().to_string();
    let before = v["passage"]["text"].as_str().unwrap().to_string();
    ok(
        &ctx.app,
        "POST",
        &format!("/api/interviews/INT-01/units/{unit}/corrections"),
        Some(json!({ "text": "Well, I run the logistics of a retailer.", "role": "respondent", "reason": "reécoute" })),
    )
    .await;
    let p = ok(&ctx.app, "GET", &format!("/api/passages/{pid}"), None).await;
    assert_eq!(p["stale"], true);
    assert_eq!(p["stale_reason"], "text_changed");
    assert_eq!(p["start"], 2);
    assert_eq!(p["text"], before.as_str());
}

#[tokio::test]
async fn the_unit_level_coding_of_the_older_flow_keeps_working_beside_passages() {
    let ctx = setup().await;
    let unit = unit_ids(&ctx.app, "INT-01").await[1].clone();
    let (st, v) = call(
        &ctx.app,
        "PUT",
        &format!("/api/interviews/INT-01/phases/2/coding/{unit}"),
        Some(json!({ "unit_id": unit, "codes": ["TECH-ADV"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let adv = code_id(&ctx.app, "TECH-ADV").await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({ "unit_id": unit, "code": adv })),
    )
    .await;
    let units = ok(
        &ctx.app,
        "GET",
        "/api/interviews/INT-01/phases/2/units",
        None,
    )
    .await;
    let row = units
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["unit"]["unit_id"] == json!(unit))
        .unwrap();
    assert_eq!(row["coding"]["codes"], json!(["TECH-ADV"]));
    let _: &Value = row;
}
