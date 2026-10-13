//! F-87: case fields defined by the project, in the API and in the exports.

mod common;

use common::{call, ok, setup};
use serde_json::json;

fn case_body(label: &str, characteristics: serde_json::Value) -> serde_json::Value {
    json!({"label": label, "characteristics": characteristics})
}

#[tokio::test]
async fn case_fields_are_typed_by_the_project_and_exported() {
    let ctx = setup().await;
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["case_fields"] = json!([
        {"id": "budget", "label": "Budget (kCHF)", "kind": "number"},
        {"id": "scope", "label": "Scope", "kind": "choice", "choices": ["local", "global"]}
    ]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;

    let (st, v) = call(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/cases",
        Some(case_body("Bad", json!({"scope": "galactic"}))),
    )
    .await;
    assert_eq!(st, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("scope"), "{v}");

    let made = ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/cases",
        Some(case_body(
            "Pilot",
            json!({"budget": "12,5", "scope": "local"}),
        )),
    )
    .await;
    assert_eq!(made["characteristics"]["budget"], "12.5");

    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    let csv = std::fs::read_to_string(dir.join("cases.csv")).unwrap();
    let header = csv.lines().next().unwrap();
    assert!(
        header.ends_with("n_units,field:budget,field:scope"),
        "{header}"
    );
    let row = csv.lines().nth(1).unwrap();
    assert!(row.ends_with(",0,12.5,local"), "{row}");
    let md = std::fs::read_to_string(dir.join("cases.md")).unwrap();
    assert!(md.contains("Budget (kCHF): 12.5"), "{md}");
    assert!(md.contains("Scope: local"), "{md}");

    let template = ok(&ctx.app, "GET", "/api/project/template", None).await;
    assert_eq!(template["project_config"]["case_fields"][0]["id"], "budget");
}

#[tokio::test]
async fn first_study_fields_keep_their_columns() {
    let ctx = setup().await;
    ok(
        &ctx.app,
        "POST",
        "/api/interviews/INT-01/cases",
        Some(json!({"label": "Old style", "decision": "adopted", "swiss_link": "Migros"})),
    )
    .await;
    let exp = ok(&ctx.app, "POST", "/api/interviews/INT-01/export", None).await;
    let dir = std::path::PathBuf::from(exp["dir"].as_str().unwrap());
    let csv = std::fs::read_to_string(dir.join("cases.csv")).unwrap();
    assert!(csv.lines().next().unwrap().starts_with(
        "case_id,label,interview_id,interviews,decision,granularity,carrier_chosen,carriers_rejected,account_type,swiss_link,stage,outcome_status,n_units"
    ));
    assert!(csv.contains("adopted") && csv.contains("Migros"));
    let md = std::fs::read_to_string(dir.join("cases.md")).unwrap();
    assert!(md.contains("Decision: adopted") && md.contains("Swiss link: Migros"));
}
