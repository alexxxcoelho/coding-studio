//! The register of the people interviewed through the HTTP API and the
//! command line: persons, groups, links, pseudonymisation, prompts, anonymised
//! export, verification package, CSV import.

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use common::{call, ok, setup, unit_ids, Ctx};
use cs_app::cli::{self, Command};
use cs_app::AppState;
use cs_store::Store;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const NAMES: &str = "---\ninterview_id: INT-01\nstatus: test fixture\n---\n\n# INT-01, cleaned and diarised transcript\n\nKey: INT = interviewer, RESP = respondent.\n\n---\n\n**INT `[00:01]`:** Merci de nous recevoir. Vous êtes Anna Muller, de la Banque Alpha ?\n\n**RESP `[00:05]`:** Oui, je suis Anna Muller. Mon collègue Luc Favre travaille avec Mme Muller depuis dix ans, et Anna aime la Banque Alpha.\n\n**INT `[00:20]`:** Et Eva Roth, de la Cave Beta ?\n\n**RESP `[00:24]`:** Eva Roth gère la Cave Beta ; je la connais bien.\n";

const I: &str = "/api/interviews/INT-01";
const P2: &str = "/api/interviews/INT-01/phases/2";

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The standard project with INT-01 rewritten to a transcript that holds names.
async fn named_project() -> Ctx {
    let ctx = setup().await;
    std::fs::write(
        ctx.root
            .join("INT-01_Test/02_transcript_cleaned_diarised.md"),
        NAMES,
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
        "/api/interviews/INT-01/segment",
        Some(json!({})),
    )
    .await;
    ctx
}

/// Anna (anonymised, with aliases) and her bank (anonymised organisation),
/// Luc and Eva (not anonymised), a cave (organisation, not anonymised).
async fn register(ctx: &Ctx) {
    for body in [
        json!({"name": "Anna Muller", "aliases": ["Mme Muller", "Anna"], "organisation": "Banque Alpha",
               "function": "Directrice", "email": "anna@secret.ch", "notes": "note privée",
               "consent_status": "given", "consent_date": "2026-09-01"}),
        json!({"kind": "organisation", "name": "Banque Alpha"}),
        json!({"name": "Luc Favre", "anonymise": false}),
        json!({"name": "Eva Roth", "anonymise": false}),
        json!({"kind": "organisation", "name": "Cave Beta", "anonymise": false}),
    ] {
        ok(&ctx.app, "POST", "/api/persons", Some(body)).await;
    }
    for (pid, role) in [
        ("P-0001", "respondent"),
        ("P-0003", "mentioned"),
        ("P-0004", "mentioned"),
    ] {
        ok(
            &ctx.app,
            "POST",
            &format!("{I}/persons"),
            Some(json!({"person_id": pid, "role": role})),
        )
        .await;
    }
}

fn read_tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap().to_string_lossy().to_string();
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn all_text(dir: &Path) -> String {
    read_tree(dir)
        .into_iter()
        .filter(|(n, _)| !n.ends_with(".docx") && !n.ends_with(".sqlite"))
        .map(|(_, b)| String::from_utf8_lossy(&b).to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------------ persons

#[tokio::test]
async fn persons_and_organisations_are_created_listed_and_filtered() {
    let ctx = named_project().await;
    register(&ctx).await;
    let v = ok(&ctx.app, "GET", "/api/persons", None).await;
    assert_eq!(v["n"], 5);
    let anna = &v["persons"][0];
    assert_eq!(anna["person_id"], "P-0001");
    assert_eq!(anna["anonymise"], true);
    assert_eq!(anna["pseudonym"], "Répondant 01");
    assert_eq!(anna["n_interviews"], 1);
    assert_eq!(v["persons"][1]["pseudonym"], "Organisation 01");
    let orgs = ok(&ctx.app, "GET", "/api/persons?kind=organisation", None).await;
    assert_eq!(orgs["n"], 2);
    let flagged = ok(&ctx.app, "GET", "/api/persons?anonymise=true", None).await;
    assert_eq!(flagged["n"], 2);
    let q = ok(&ctx.app, "GET", "/api/persons?q=luc", None).await;
    assert_eq!(q["n"], 1);
    let in_iv = ok(&ctx.app, "GET", "/api/persons?interview_id=INT-01", None).await;
    assert_eq!(in_iv["n"], 3);
    let (st, _) = call(&ctx.app, "GET", "/api/persons?kind=animal", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // The detail gives the interviews, groups and attributes.
    let d = ok(&ctx.app, "GET", "/api/persons/P-0001", None).await;
    assert_eq!(d["interviews"][0]["interview_id"], "INT-01");
    assert_eq!(d["interviews"][0]["role"], "respondent");
    assert!(d["groups"].as_array().unwrap().is_empty());
    assert_eq!(d["consent_status"], "given");
    // Update, and errors.
    let u = ok(
        &ctx.app,
        "PUT",
        "/api/persons/P-0001",
        Some(json!({"function": "Présidente", "pseudonym": "Dirigeante A"})),
    )
    .await;
    assert_eq!(u["function"], "Présidente");
    assert_eq!(u["pseudonym"], "Dirigeante A");
    let (st, _) = call(&ctx.app, "GET", "/api/persons/P-0099", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/persons",
        Some(json!({"name": "anna muller"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // Deleting a linked person needs with_links.
    let (st, _) = call(&ctx.app, "DELETE", "/api/persons/P-0001", None).await;
    assert_eq!(st, StatusCode::CONFLICT);
    ok(
        &ctx.app,
        "DELETE",
        "/api/persons/P-0001?with_links=true",
        None,
    )
    .await;
    let (st, _) = call(&ctx.app, "GET", "/api/persons/P-0001", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_persons_of_an_interview_with_their_roles() {
    let ctx = named_project().await;
    register(&ctx).await;
    let v = ok(&ctx.app, "GET", &format!("{I}/persons"), None).await;
    let roles: Vec<(&str, &str)> = v["persons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["role"].as_str().unwrap(),
                p["person_id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        roles,
        vec![
            ("respondent", "P-0001"),
            ("mentioned", "P-0003"),
            ("mentioned", "P-0004")
        ]
    );
    assert_eq!(v["persons"][0]["name"], "Anna Muller");
    // Link from the person side, change a role, unlink.
    ok(
        &ctx.app,
        "POST",
        "/api/persons/P-0003/interviews",
        Some(json!({"interview_id": "INT-02", "role": "co_respondent"})),
    )
    .await;
    let d = ok(&ctx.app, "GET", "/api/persons/P-0003", None).await;
    assert_eq!(d["interviews"].as_array().unwrap().len(), 2);
    let (st, _) = call(
        &ctx.app,
        "POST",
        &format!("{I}/persons"),
        Some(json!({"person_id": "P-0003", "role": "boss"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        &ctx.app,
        "DELETE",
        "/api/persons/P-0003/interviews/INT-02",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = call(&ctx.app, "DELETE", &format!("{I}/persons/P-0009"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // Replace the whole set.
    let v = ok(
        &ctx.app,
        "PUT",
        &format!("{I}/persons"),
        Some(json!({"persons": [{"person_id": "P-0001", "role": "respondent"}]})),
    )
    .await;
    assert_eq!(v["persons"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn groups_form_a_tree_through_the_api() {
    let ctx = named_project().await;
    register(&ctx).await;
    let a = ok(
        &ctx.app,
        "POST",
        "/api/person-groups",
        Some(json!({"name": "Détaillants"})),
    )
    .await;
    let b = ok(
        &ctx.app,
        "POST",
        "/api/person-groups",
        Some(json!({"name": "Suisse romande", "parent_id": a["group_id"]})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/persons/P-0001",
        Some(json!({"group_ids": [b["group_id"]]})),
    )
    .await;
    let t = ok(&ctx.app, "GET", "/api/person-groups", None).await;
    assert_eq!(t["n_groups"], 2);
    assert_eq!(t["n_ungrouped_persons"], 4);
    assert_eq!(t["groups"][0]["n_persons"], 0);
    assert_eq!(t["groups"][0]["n_persons_total"], 1);
    assert_eq!(t["groups"][0]["n_interviews_total"], 1);
    assert_eq!(
        t["groups"][0]["children"][0]["path"],
        "Détaillants/Suisse romande"
    );
    // A cycle is refused (400); a non-empty group cannot be deleted (409).
    let (st, e) = call(
        &ctx.app,
        "PUT",
        &format!("/api/person-groups/{}", a["group_id"].as_str().unwrap()),
        Some(json!({"parent_id": b["group_id"]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    let (st, _) = call(
        &ctx.app,
        "DELETE",
        &format!("/api/person-groups/{}", b["group_id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = call(
        &ctx.app,
        "DELETE",
        &format!("/api/person-groups/{}", a["group_id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // Move to the root, rename; the group with its members.
    let moved = ok(
        &ctx.app,
        "PUT",
        &format!("/api/person-groups/{}", b["group_id"].as_str().unwrap()),
        Some(json!({"parent_id": null, "name": "Romandie"})),
    )
    .await;
    assert!(moved["parent_id"].is_null());
    let g = ok(
        &ctx.app,
        "GET",
        &format!("/api/person-groups/{}", b["group_id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(g["persons"].as_array().unwrap().len(), 1);
    // Delete with reassignment.
    let (st, _) = call(
        &ctx.app,
        "DELETE",
        &format!(
            "/api/person-groups/{}?reassign_to={}",
            b["group_id"].as_str().unwrap(),
            a["group_id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let p = ok(&ctx.app, "GET", "/api/persons/P-0001", None).await;
    assert_eq!(p["groups"][0]["path"], "Détaillants");
}

// ------------------------------------------------------- pseudonymisation

#[tokio::test]
async fn the_pseudonymisation_view_marks_where_each_entry_comes_from() {
    let ctx = named_project().await;
    ok(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({"enabled": false, "entries": [
            {"kind": "organisation", "original": "Cave Gamma", "pseudonym": "Organisation X"}]})),
    )
    .await;
    register(&ctx).await;
    let v = ok(&ctx.app, "GET", "/api/pseudonymisation", None).await;
    assert_eq!(
        v["enabled"], true,
        "a flagged record puts the list in force"
    );
    assert_eq!(v["enabled_manual"], false);
    let by: Vec<(String, String, String)> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["original"].as_str().unwrap().to_string(),
                e["source"].as_str().unwrap().to_string(),
                e["pseudonym"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(by.contains(&("Anna Muller".into(), "person".into(), "Répondant 01".into())));
    assert!(by.contains(&("Mme Muller".into(), "person".into(), "Répondant 01".into())));
    assert!(by.contains(&(
        "Banque Alpha".into(),
        "person".into(),
        "Organisation 01".into()
    )));
    assert!(by.contains(&(
        "Cave Gamma".into(),
        "manual".into(),
        "Organisation X".into()
    )));
    assert!(
        !by.iter().any(|e| e.0 == "Luc Favre"),
        "unflagged names are not listed"
    );
    assert_eq!(v["n_manual_entries"], 1);
    // Sending the view back does not turn the person entries into manual ones.
    let put = ok(
        &ctx.app,
        "PUT",
        "/api/pseudonymisation",
        Some(json!({"entries": v["entries"]})),
    )
    .await;
    assert_eq!(put["n_manual_entries"], 1);
    assert_eq!(put["entries"], v["entries"]);
}

#[tokio::test]
async fn all_personal_names_and_aliases_are_masked_even_when_the_record_is_unflagged() {
    let ctx = named_project().await;
    register(&ctx).await;
    // A person attribute shared with the model, holding a name of the list.
    let mut cfg = ok(&ctx.app, "GET", "/api/project-config", None).await;
    cfg["attributes"] = json!([{"id": "bio", "label": "Parcours", "kind": "text",
        "shared_with_model": true, "scope": "person"}]);
    ok(&ctx.app, "PUT", "/api/project-config", Some(cfg)).await;
    ok(
        &ctx.app,
        "PUT",
        "/api/persons/P-0001",
        Some(json!({"attributes": {"bio": "Dirige la Banque Alpha, connue sous le nom de Mme Muller"}})),
    )
    .await;
    // The interview sheet does not show the person attribute.
    let attrs = ok(&ctx.app, "GET", &format!("{I}/attributes"), None).await;
    assert!(attrs["definitions"].as_array().unwrap().is_empty());

    let units = unit_ids(&ctx.app, "INT-01").await;
    for (i, c) in [(1, "TECH-ADV"), (3, "TECH-COST")] {
        ok(
            &ctx.app,
            "PUT",
            &format!("{P2}/coding/{}", units[i]),
            Some(json!({ "codes": [c] })),
        )
        .await;
    }
    let parts = ok(
        &ctx.app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({"batch_size": 5})),
    )
    .await;
    let text = parts[0]["text"].as_str().unwrap();
    for gone in ["Anna", "Muller", "Banque Alpha", "Mme "] {
        assert!(!text.contains(gone), "{gone} reached the prompt:\n{text}");
    }
    assert!(text.contains("Répondant 01"), "{text}");
    assert!(text.contains("Organisation 01"), "{text}");
    // The shared attribute of the respondent is there, masked.
    assert!(
        text.contains("Parcours: Dirige la Organisation 01, connue sous le nom de Répondant 01"),
        "{text}"
    );
    // Personal names are always masked; unflagged organisations remain unchanged.
    assert!(!text.contains("Luc Favre"), "{text}");
    assert!(!text.contains("Eva Roth"), "{text}");
    assert!(text.contains("Cave Beta"), "{text}");
    // Opting out in the register never bypasses personal-name masking for AI.
    ok(
        &ctx.app,
        "PUT",
        "/api/persons/P-0001",
        Some(json!({"anonymise": false})),
    )
    .await;
    let parts = ok(
        &ctx.app,
        "POST",
        &format!("{P2}/prompts"),
        Some(json!({"batch_size": 5})),
    )
    .await;
    assert!(!parts[0]["text"].as_str().unwrap().contains("Anna Muller"));
}

// ---------------------------------------------------------------- exports

#[tokio::test]
async fn packages_hold_persons_and_groups_and_the_anonymised_one_has_no_flagged_name() {
    let ctx = named_project().await;
    register(&ctx).await;
    ok(
        &ctx.app,
        "POST",
        "/api/person-groups",
        Some(json!({"name": "Détaillants"})),
    )
    .await;
    let full = ok(&ctx.app, "POST", &format!("{I}/export"), None).await;
    let dir = PathBuf::from(full["dir"].as_str().unwrap());
    let persons = std::fs::read_to_string(dir.join("persons.csv")).unwrap();
    assert!(persons.starts_with("person_id,kind,name,aliases,organisation,function,email,"));
    assert!(persons.contains("Anna Muller") && persons.contains("anna@secret.ch"));
    assert!(persons.contains("Luc Favre"));
    assert!(dir.join("groups.csv").exists());
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(readme.contains("`persons.csv`") && readme.contains("`groups.csv`"));
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let names: Vec<&str> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"persons.csv") && names.contains(&"groups.csv"));

    let preview = ok(
        &ctx.app,
        "GET",
        "/api/anonymisation/interviews/INT-01",
        None,
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/anonymisation/interviews/INT-01",
        Some(json!({"digest":preview["digest"],"reviewed":true})),
    )
    .await;
    let r = ok(
        &ctx.app,
        "POST",
        "/api/export/anonymised",
        Some(json!({"scope": "interview", "interview_id": "INT-01"})),
    )
    .await;
    let adir = PathBuf::from(r["dir"].as_str().unwrap());
    let text = all_text(&adir);
    for gone in [
        "Anna",
        "Muller",
        "Banque Alpha",
        "anna@secret.ch",
        "note privée",
    ] {
        assert!(!text.contains(gone), "{gone} is in the anonymised export");
    }
    assert!(
        !text.contains("Luc Favre") && !text.contains("Eva Roth") && text.contains("Cave Beta")
    );
    let public = std::fs::read_to_string(adir.join("persons.csv")).unwrap();
    assert!(public.starts_with("person_id,kind,name,anonymise,pseudonym,"));
    assert!(public.contains("Répondant 01") && !public.contains("Luc Favre"));
    assert!(!public.contains("secret.ch"));
    // The cross-case package has the whole register.
    let cc = ok(&ctx.app, "POST", "/api/export/cross-case", None).await;
    let cdir = PathBuf::from(cc["folder"].as_str().unwrap());
    assert!(std::fs::read_to_string(cdir.join("persons.csv"))
        .unwrap()
        .contains("Cave Beta"));
}

#[tokio::test]
async fn the_project_bundle_carries_the_register_and_restores_it() {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::io::Read;
    use tower::ServiceExt;

    let ctx = named_project().await;
    register(&ctx).await;
    ok(
        &ctx.app,
        "POST",
        "/api/person-groups",
        Some(json!({"name": "Détaillants"})),
    )
    .await;
    let send = |uri: String, body: Vec<u8>, method: &'static str| {
        let app = ctx.app.clone();
        async move {
            let res = app
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = res.status().as_u16();
            (
                status,
                res.into_body().collect().await.unwrap().to_bytes().to_vec(),
            )
        }
    };
    let (st, bytes) = send("/api/project/bundle".into(), vec![], "GET").await;
    assert_eq!(st, 200);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut names = Vec::new();
    for i in 0..z.len() {
        names.push(z.by_index(i).unwrap().name().unwrap().to_string());
    }
    assert!(
        names.contains(&"config/persons.csv".to_string()),
        "{names:?}"
    );
    assert!(names.contains(&"config/groups.csv".to_string()));
    let manifest: Value = {
        let mut f = z.by_name("manifest.json").unwrap();
        let mut v = Vec::new();
        f.read_to_end(&mut v).unwrap();
        serde_json::from_slice(&v).unwrap()
    };
    assert_eq!(manifest["counts"]["persons"], 5);
    assert_eq!(manifest["counts"]["person_groups"], 1);
    assert_eq!(manifest["counts"]["person_links"], 3);
    let target = ctx.dir.path().join("restored/new.db");
    let (st, body) = send(
        format!(
            "/api/project/bundle/import?apply=true&target={}",
            target.display()
        ),
        bytes,
        "POST",
    )
    .await;
    assert_eq!(st, 200);
    let done: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(done["applied"], true, "{done}");
    let store = Store::open(&target).unwrap();
    let anna = store.person("P-0001").unwrap();
    assert!(anna.anonymise);
    assert_eq!(anna.pseudonym, "Répondant 01");
    assert_eq!(store.person_links().unwrap().len(), 3);
    let live = ok(&ctx.app, "GET", "/api/pseudonymisation", None).await;
    assert_eq!(
        store.pseudonym_config().unwrap().digest(),
        live["digest"].as_str().unwrap()
    );
}

#[tokio::test]
async fn the_verification_package_keeps_unflagged_names_replaces_flagged_ones_and_matches_the_originals(
) {
    let ctx = named_project().await;
    register(&ctx).await;
    let g = ok(
        &ctx.app,
        "POST",
        "/api/person-groups",
        Some(json!({"name": "Détaillants"})),
    )
    .await;
    ok(
        &ctx.app,
        "PUT",
        "/api/persons/P-0003",
        Some(json!({"group_ids": [g["group_id"]]})),
    )
    .await;
    // Code something so that the coded transcript has a passage.
    let units = unit_ids(&ctx.app, "INT-01").await;
    let adv = common::code_id(&ctx.app, "TECH-ADV").await;
    ok(
        &ctx.app,
        "POST",
        &format!("{I}/passages"),
        Some(json!({"unit_id": units[1], "start": 3, "end": 40, "code": adv})),
    )
    .await;

    let r = ok(
        &ctx.app,
        "POST",
        "/api/export/verification",
        Some(json!({"interview_ids": ["INT-01", "INT-02"]})),
    )
    .await;
    let dir = PathBuf::from(r["dir"].as_str().unwrap());
    assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();

    // Delivered files match their digests in the manifest.
    for f in manifest["files"].as_array().unwrap() {
        let bytes = std::fs::read(dir.join(f["name"].as_str().unwrap())).unwrap();
        assert_eq!(sha(&bytes), f["sha256"].as_str().unwrap());
        assert_eq!(bytes.len() as u64, f["size"].as_u64().unwrap());
    }

    // The original digests are those of the files on the researcher's disk.
    let original_01 = std::fs::read(
        ctx.root
            .join("INT-01_Test/02_transcript_cleaned_diarised.md"),
    )
    .unwrap();
    let original_02 = std::fs::read(
        ctx.root
            .join("INT-02_Library/02_transcript_cleaned_diarised.md"),
    )
    .unwrap();
    let ivs = manifest["interviews"].as_array().unwrap();
    let s1 = &ivs[0]["sources"][0];
    let s2 = &ivs[1]["sources"][0];
    assert_eq!(s1["original_sha256"], sha(&original_01));
    assert_eq!(s2["original_sha256"], sha(&original_02));
    assert_eq!(s1["intact"], true);
    assert_eq!(s1["recorded_sha256"], s1["original_sha256"]);

    // INT-01 holds flagged names: replaced, so the delivered digest differs.
    assert_eq!(s1["replaced"], true);
    assert_ne!(s1["delivered_sha256"], s1["original_sha256"]);
    let delivered_01 =
        std::fs::read_to_string(dir.join(s1["delivered_as"].as_str().unwrap())).unwrap();
    for gone in ["Anna", "Muller", "Banque Alpha"] {
        assert!(!delivered_01.contains(gone), "{gone}");
    }
    assert!(delivered_01.contains("Répondant 01") && delivered_01.contains("Organisation 01"));
    assert!(
        delivered_01.contains("Luc Favre")
            && delivered_01.contains("Eva Roth")
            && delivered_01.contains("Cave Beta")
    );
    // The replaced text is the original with the names turned into pseudonyms.
    let expected = String::from_utf8(original_01.clone())
        .unwrap()
        .replace("Anna Muller", "Répondant 01")
        .replace("Mme Muller", "Répondant 01")
        .replace("Anna", "Répondant 01")
        .replace("Banque Alpha", "Organisation 01");
    assert_eq!(delivered_01, expected);
    // The receipt lists pseudonyms and counts, never names.
    let receipt = s1["replacements"].to_string();
    assert!(receipt.contains("Répondant 01") && !receipt.contains("Anna"));

    // INT-02 has nothing to replace: byte for byte the original.
    assert_eq!(s2["replaced"], false);
    assert_eq!(s2["delivered_sha256"], s2["original_sha256"]);
    let delivered_02 = std::fs::read(dir.join(s2["delivered_as"].as_str().unwrap())).unwrap();
    assert_eq!(delivered_02, original_02);

    // Coded transcript, register, README, no flagged name anywhere.
    let coded = std::fs::read_to_string(dir.join("interviews/INT-01/transcript_coded.md")).unwrap();
    assert!(
        coded.contains("Relative advantage") || coded.contains("TECH-ADV") || coded.contains("« "),
        "{coded}"
    );
    assert!(manifest["interviews"][1]["coded_transcript"]["delivered"] == true);
    let register = std::fs::read_to_string(dir.join("persons.csv")).unwrap();
    assert!(register.contains("Répondant 01") && register.contains("Luc Favre"));
    assert!(register.contains("Détaillants"));
    let all = all_text(&dir);
    for gone in [
        "Anna",
        "Muller",
        "Banque Alpha",
        "anna@secret.ch",
        "note privée",
    ] {
        assert!(!all.contains(gone), "{gone} is in the verification package");
    }
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(readme.contains("Vérifier les empreintes") && readme.contains("original_sha256"));
    assert!(!readme.contains('\u{2014}') && !readme.contains('\u{2013}'));
    assert!(
        manifest["anonymisation"]["replacements"]["person"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        manifest["anonymisation"]["pseudonymisation_digest"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );

    // Errors.
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/export/verification",
        Some(json!({"interview_ids": ["NOPE"]})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = call(
        &ctx.app,
        "POST",
        "/api/export/verification",
        Some(json!({})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_source_changed_since_the_scan_is_reported() {
    let ctx = named_project().await;
    register(&ctx).await;
    std::fs::write(
        ctx.root
            .join("INT-02_Library/02_transcript_cleaned_diarised.md"),
        "changed after the scan",
    )
    .unwrap();
    let r = ok(
        &ctx.app,
        "POST",
        "/api/export/verification",
        Some(json!({"interview_ids": ["INT-02"]})),
    )
    .await;
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(PathBuf::from(r["dir"].as_str().unwrap()).join("manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["interviews"][0]["sources"][0]["intact"], false);
}

#[tokio::test]
async fn the_command_line_writes_the_verification_package() {
    let ctx = named_project().await;
    register(&ctx).await;
    let st = AppState {
        store: Arc::new(Mutex::new(Store::open(&ctx.db).unwrap())),
        db_path: ctx.db.clone(),
        coding_db: None,
        actor: "candidate".into(),
        interviewer_names: vec![],
        picker: None,
    };
    let out = ctx.dir.path().join("for-examiners");
    let o = cli::run(
        &st,
        &Command::VerificationExport {
            targets: vec!["all".into()],
            out: Some(out.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(o.code, 0, "{}", o.text);
    let dir = PathBuf::from(o.value["dir"].as_str().unwrap());
    assert!(dir.starts_with(&out));
    assert!(dir.join("manifest.json").exists());
    assert_eq!(o.value["interview_ids"], json!(["INT-01", "INT-02"]));
    let bad = cli::run(
        &st,
        &Command::VerificationExport {
            targets: vec!["all".into(), "INT-01".into()],
            out: None,
        },
    )
    .await;
    assert!(bad.is_err());
    // `verify` accepts the manifest of the package.
    let v = cli::run(&st, &Command::Verify).await.unwrap();
    assert!(v.value["audit_chain"]["ok"].as_bool().unwrap());
}

// ----------------------------------------------------------------- import

#[tokio::test]
async fn csv_import_reports_then_applies() {
    let ctx = named_project().await;
    let csv = "name,organisation,role,groups,anonymise,interviews\nAnna Muller,Banque Alpha,Directrice,Détaillants/Suisse romande,oui,INT-01\nLuc Favre,,Acheteur,Détaillants/Suisse romande,non,INT-01:mentioned;INT-02\n";
    let dry = ok(
        &ctx.app,
        "POST",
        "/api/persons/import",
        Some(json!({"csv": csv})),
    )
    .await;
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["ok"], true);
    assert_eq!(dry["n_create"], 2);
    assert_eq!(dry["groups_to_create"].as_array().unwrap().len(), 2);
    assert_eq!(ok(&ctx.app, "GET", "/api/persons", None).await["n"], 0);
    let done = ok(
        &ctx.app,
        "POST",
        "/api/persons/import",
        Some(json!({"csv": csv, "dry_run": false})),
    )
    .await;
    assert_eq!(done["applied"], true);
    assert_eq!(ok(&ctx.app, "GET", "/api/persons", None).await["n"], 2);
    let t = ok(&ctx.app, "GET", "/api/person-groups", None).await;
    assert_eq!(t["groups"][0]["n_persons_total"], 2);
    assert_eq!(t["groups"][0]["n_interviews_total"], 2);
    // A bad file is reported with its rows and writes nothing.
    let bad = ok(
        &ctx.app,
        "POST",
        "/api/persons/import",
        Some(json!({"csv": "name,anonymise\nZoé Blanc,peut-être\n", "dry_run": false})),
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["problems"][0]["row"], 2);
    assert_eq!(ok(&ctx.app, "GET", "/api/persons", None).await["n"], 2);
    // The exported register can be read back as is.
    let full = ok(&ctx.app, "POST", &format!("{I}/export"), None).await;
    let exported =
        std::fs::read_to_string(PathBuf::from(full["dir"].as_str().unwrap()).join("persons.csv"))
            .unwrap();
    let again = ok(
        &ctx.app,
        "POST",
        "/api/persons/import",
        Some(json!({"csv": exported, "dry_run": true})),
    )
    .await;
    assert_eq!(again["ok"], true, "{again}");
    assert_eq!(again["n_create"], 0);
}
