mod common;
use axum::http::StatusCode;
use common::{call, ok, setup};
use serde_json::{json, Value};

async fn review(app: &axum::Router, id: &str) {
    let path = format!("/api/anonymisation/interviews/{id}");
    let p = ok(app, "GET", &path, None).await;
    ok(
        app,
        "PUT",
        &path,
        Some(json!({"digest":p["digest"],"reviewed":true})),
    )
    .await;
}
#[tokio::test]
async fn local_copy_preserves_every_other_character_and_private_table_stays_local() {
    let c = setup().await;
    let raw="Um, Sophie, my manager, told Jean Dupont… Jean Dupont?\nYes, yes! Annabelle, 1 200. Mail jean@example.org or +41 79 123 45 67.";
    ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/units/INT-01-U002/corrections",
        Some(json!({"text":raw,"role":"respondent","reason":"Synthetic fixture"})),
    )
    .await;
    ok(
        &c.app,
        "PUT",
        "/api/anonymisation",
        Some(json!({"entries":[
            {"kind":"person","original":"Sophie","pseudonym":"[Person 1]"},
            {"kind":"person","original":"Jean Dupont","pseudonym":"[Person 2]"},
            {"kind":"person","original":"Anna","pseudonym":"[Person 3]"}
        ]})),
    )
    .await;
    let p = ok(&c.app, "GET", "/api/anonymisation/interviews/INT-01", None).await;
    let u = p["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"] == "INT-01-U002")
        .unwrap();
    assert_eq!(u["original"], raw);
    assert_eq!(u["anonymised"],"Um, [Person 1], my manager, told [Person 2]… [Person 2]?\nYes, yes! Annabelle, 1 200. Mail [Email] or [Phone].");
    let (status, _) = call(
        &c.app,
        "POST",
        "/api/anonymisation/interviews/INT-01/export",
        None,
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    review(&c.app, "INT-01").await;
    let e = ok(
        &c.app,
        "POST",
        "/api/anonymisation/interviews/INT-01/export",
        None,
    )
    .await;
    for entry in std::fs::read_dir(e["dir"].as_str().unwrap()).unwrap() {
        let t = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for identity in ["Sophie", "Jean Dupont", "jean@example.org", "+41 79"] {
            assert!(!t.contains(identity));
        }
    }
    let db = rusqlite::Connection::open(&c.db).unwrap();
    let audit:String=db.query_row("SELECT group_concat(detail) FROM audit_log WHERE action='settings.set' AND target='anonymisation_v1'",[],|r|r.get(0)).unwrap();
    assert!(audit.contains("after_sha256"));
    assert!(!audit.contains("Sophie"));
    // Original correction is retained, never rewritten into a pseudo original.
    let source:String=db.query_row("SELECT text FROM unit_corrections WHERE interview_id='INT-01' ORDER BY correction_id DESC LIMIT 1",[],|r|r.get(0)).unwrap();
    assert_eq!(source, raw);
    assert_eq!(
        ok(&c.app, "GET", "/api/audit/verify", None).await["ok"],
        true
    );
}
#[tokio::test]
async fn approval_is_invalidated_by_rule_or_source_change_and_stale_approval_fails() {
    let c = setup().await;
    review(&c.app, "INT-01").await;
    let path = "/api/anonymisation/interviews/INT-01";
    let before = ok(&c.app, "GET", path, None).await;
    ok(&c.app,"POST","/api/interviews/INT-01/units/INT-01-U002/corrections",Some(json!({"text":"Another person appeared.","role":"respondent","reason":"Synthetic edit"}))).await;
    let p = ok(&c.app, "GET", path, None).await;
    assert_eq!(p["reviewed"], false);
    let (s, _) = call(
        &c.app,
        "PUT",
        path,
        Some(json!({"digest":before["digest"],"reviewed":true})),
    )
    .await;
    assert_ne!(s, StatusCode::OK);
    review(&c.app, "INT-01").await;
    ok(
        &c.app,
        "PUT",
        "/api/anonymisation",
        Some(json!({"entries":[]})),
    )
    .await;
    assert_eq!(ok(&c.app, "GET", path, None).await["reviewed"], false);
}
#[tokio::test]
async fn reviewer_export_refuses_originals_and_unreviewed_corpus() {
    let c = setup().await;
    let (s, _) = call(
        &c.app,
        "GET",
        "/api/export/reviewer/catalogue?pseudonymise=false",
        None,
    )
    .await;
    assert_ne!(s, StatusCode::OK);
    let cat = ok(&c.app, "GET", "/api/export/reviewer/catalogue", None).await;
    assert_eq!(cat["file_ids"], json!({}));
    assert!(cat["fields"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["path"][0] != "Source files"));
    let selected: Vec<_> = cat["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["selected_by_default"] == true)
        .map(|f| f["id"].clone())
        .collect();
    let body = json!({"snapshot":cat["snapshot"],"selected":selected,"password":"Synthetic protected export test","privacy_reviewed":true});
    assert_ne!(
        call(&c.app, "POST", "/api/export/reviewer", Some(body.clone()))
            .await
            .0,
        StatusCode::OK
    );
    review(&c.app, "INT-01").await;
    review(&c.app, "INT-02").await;
    let mut raw = body.clone();
    raw["pseudonymise"] = json!(false);
    assert_ne!(
        call(&c.app, "POST", "/api/export/reviewer", Some(raw))
            .await
            .0,
        StatusCode::OK
    );
    let mut unchecked = body;
    unchecked["privacy_reviewed"] = json!(false);
    assert_ne!(
        call(&c.app, "POST", "/api/export/reviewer", Some(unchecked))
            .await
            .0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn opted_out_person_records_and_contact_fields_are_still_protected() {
    let c = setup().await;
    let person=ok(&c.app,"POST","/api/persons",Some(json!({"name":"Synthetic Private","kind":"person","anonymise":false,"pseudonym":"[Person 42]","email":"synthetic@example.org"}))).await;
    assert!(person.is_object());
    let q = ok(&c.app, "GET", "/api/anonymisation", None).await;
    let entries = q["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["original"] == "Synthetic Private"));
    let cat = ok(&c.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let text = cat.to_string();
    assert!(!text.contains("Synthetic Private"));
    assert!(!text.contains("synthetic@example.org"));
    assert!(cat["fields"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| !(f["path"][0] == "People"
            && [
                Value::from("email"),
                Value::from("phone"),
                Value::from("consent_document")
            ]
            .contains(f["path"].as_array().unwrap().last().unwrap()))));
}

#[tokio::test]
async fn reviewer_spans_follow_replacements_even_for_repeated_quotes_and_unicode() {
    let c = setup().await;
    let raw = "😀 Sophie said yes. Sophie said yes.";
    ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/units/INT-01-U002/corrections",
        Some(json!({"text":raw,"role":"respondent","reason":"Synthetic repeated quote"})),
    )
    .await;
    let start = raw[..raw.rfind("Sophie").unwrap()].chars().count();
    let code = common::code_id(&c.app, "TECH-ADV").await;
    ok(
        &c.app,
        "POST",
        "/api/interviews/INT-01/passages",
        Some(json!({"unit_id":"INT-01-U002","start":start,"end":start+6,"code":code})),
    )
    .await;
    ok(
        &c.app,
        "PUT",
        "/api/anonymisation",
        Some(json!({"entries":[{"kind":"person","original":"Sophie","pseudonym":"[Person 1]"}]})),
    )
    .await;
    let cat = ok(&c.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let fields = cat["fields"].as_array().unwrap();
    let val = |category: &str, field: &str| {
        fields
            .iter()
            .find(|f| {
                f["path"][0] == category && f["path"].as_array().unwrap().last().unwrap() == field
            })
            .unwrap()["value"]
            .clone()
    };
    let masked = "😀 [Person 1] said yes. [Person 1] said yes.";
    let a = val("Passages", "start_offset").as_u64().unwrap() as usize;
    let b = val("Passages", "end_offset").as_u64().unwrap() as usize;
    assert_eq!(
        a,
        masked[..masked.rfind("[Person 1]").unwrap()]
            .chars()
            .count()
    );
    assert_eq!(val("Passages", "text"), "[Person 1]");
    assert_eq!(
        masked.chars().skip(a).take(b - a).collect::<String>(),
        "[Person 1]"
    );
    ok(&c.app,"POST","/api/interviews/INT-01/units/INT-01-U002/corrections",Some(json!({"text":"The source changed after coding.","role":"respondent","reason":"Synthetic stale anchor"}))).await;
    let c2 = ok(&c.app, "GET", "/api/export/reviewer/catalogue", None).await;
    let fields = c2["fields"].as_array().unwrap();
    let passage_value = |field: &str| {
        fields
            .iter()
            .find(|f| {
                f["path"][0] == "Passages" && f["path"].as_array().unwrap().last().unwrap() == field
            })
            .unwrap()["value"]
            .clone()
    };
    assert_eq!(
        passage_value("text"),
        "[Person 1]",
        "never substitute a different source slice for an old quote"
    );
    assert!(passage_value("start_offset").is_null());
    assert_eq!(
        passage_value("anchor_status"),
        "unresolved_after_source_edit"
    );
}
