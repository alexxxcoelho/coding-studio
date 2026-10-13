//! Export of a project as a REFI-QDA codebook and project: structure,
//! mapping rules, determinism.

mod common;

use common::*;
use cs_model::passage::slice_scalars;
use cs_model::SpeakerRole;
use cs_qdpx::guid;
use cs_qdpx::{export_qdc, export_qdpx, ExportOptions};
use roxmltree::{Document, Node};

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
    n.children()
        .filter(|c| c.is_element() && c.tag_name().name() == name)
        .collect()
}

fn first<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Node<'a, 'i> {
    kids(n, name)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no <{name}>"))
}

fn names(codes: Node) -> Vec<String> {
    kids(codes, "Code")
        .iter()
        .map(|c| c.attribute("name").unwrap().to_string())
        .collect()
}

#[test]
fn qdc_follows_the_codebook_schema() {
    let s = reference_store();
    let out = export_qdc(&s, ACTOR).unwrap();
    assert_eq!(out.file_name, "codebook.qdc");
    let xml = String::from_utf8(out.bytes.clone()).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\""));
    let doc = Document::parse(&xml).unwrap();
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "CodeBook");
    assert_eq!(
        root.tag_name().namespace(),
        Some("urn:QDA-XML:codebook:1.0")
    );
    assert!(root
        .attribute("origin")
        .unwrap()
        .starts_with("Coding Studio"));
    let codes = first(root, "Codes");
    // Roots in codebook order, children nested.
    assert_eq!(names(codes), vec!["Coûts", "Compétences"]);
    let cost = kids(codes, "Code")[0];
    assert_eq!(names(cost), vec!["Licences", "Prix des étiquettes"]);
    // Required attributes on every code, GUID from the stable id.
    for c in doc.descendants().filter(|n| n.has_tag_name("Code")) {
        assert!(guid::is_guid(c.attribute("guid").unwrap()));
        assert!(c.attribute("name").is_some());
        assert_eq!(c.attribute("isCodable"), Some("true"));
    }
    let id = s.code_node("COUT").unwrap().code_id;
    assert_eq!(cost.attribute("guid"), Some(guid::code(&id).as_str()));
    // Definition as Description, escaped and read back.
    let desc = first(cost, "Description").text().unwrap();
    assert_eq!(desc, "Coûts directs & indirects.");
    assert_eq!(out.summary.codes, 4);
    assert_eq!(out.summary.sha256.len(), 64);
}

#[test]
fn qdc_leaves_out_inactive_merged_and_reserved_codes() {
    let s = reference_store();
    add_code(&s, "OLD", "Ancien", "Obsolète.", None);
    s.set_code_active("OLD", false, ACTOR).unwrap();
    add_code(&s, "DUP", "Doublon", "Doublon de compétences.", None);
    s.merge_code("DUP", "SKILLS", "doublon", ACTOR).unwrap();
    // A passage parked with "Plus tard" exists, the code is virtual.
    code_span(&s, "INT-02", 2, Some((0, 10)), "LATER");
    let xml = String::from_utf8(export_qdc(&s, ACTOR).unwrap().bytes).unwrap();
    assert!(!xml.contains("Ancien"));
    assert!(!xml.contains("Doublon"));
    assert!(!xml.contains("Plus tard"));
    assert!(!xml.contains("LATER"));
}

#[test]
fn a_child_of_an_inactive_code_moves_up() {
    let s = reference_store();
    s.set_code_active("COUT", false, ACTOR).unwrap();
    let xml = String::from_utf8(export_qdc(&s, ACTOR).unwrap().bytes).unwrap();
    let doc = Document::parse(&xml).unwrap();
    let codes = first(doc.root_element(), "Codes");
    let top = names(codes);
    assert!(top.contains(&"Licences".to_string()), "{top:?}");
    assert!(!top.contains(&"Coûts".to_string()));
}

fn project(s: &cs_store::Store, opts: &ExportOptions) -> (Vec<u8>, cs_qdpx::ExportSummary) {
    let out = export_qdpx(s, opts, ACTOR).unwrap();
    (out.bytes, out.summary)
}

#[test]
fn qdpx_is_a_zip_with_project_sources_and_offsets() {
    let s = reference_store();
    let (bytes, summary) = project(&s, &ExportOptions::default());
    let g1 = guid::source("INT-01");
    let g2 = guid::source("INT-02");
    assert_eq!(
        zip_names(&bytes),
        vec![
            "project.qde".to_string(),
            format!("Sources/{g1}.txt"),
            format!("Sources/{g2}.txt"),
            "coding-studio-offsets.json".to_string(),
        ]
    );
    assert_eq!(summary.sources, 2);
    assert_eq!(summary.selections, 5);
    assert_eq!(summary.codings, 6);

    let qde = String::from_utf8(zip_entry(&bytes, "project.qde")).unwrap();
    let doc = Document::parse(&qde).unwrap();
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "Project");
    assert_eq!(root.tag_name().namespace(), Some("urn:QDA-XML:project:1.0"));
    assert_eq!(root.attribute("name"), Some("Coding Studio"));
    assert_eq!(kids(first(root, "Users"), "User").len(), 1);
    let sources = first(root, "Sources");
    let texts = kids(sources, "TextSource");
    assert_eq!(texts.len(), 2);
    let t1 = texts[0];
    assert_eq!(t1.attribute("name"), Some("INT-01"));
    assert_eq!(t1.attribute("guid"), Some(g1.as_str()));
    assert_eq!(
        t1.attribute("plainTextPath"),
        Some(format!("internal://{g1}.txt").as_str())
    );
}

#[test]
fn plain_text_holds_every_turn_and_positions_point_into_it() {
    let s = reference_store();
    let (bytes, _) = project(&s, &ExportOptions::default());
    let g1 = guid::source("INT-01");
    let text = String::from_utf8(zip_entry(&bytes, &format!("Sources/{g1}.txt"))).unwrap();
    let turns = interview_one();
    let expected = turns.iter().map(|t| t.1).collect::<Vec<_>>().join("\n\n");
    assert_eq!(text, expected, "text is the units as stored, joined");

    let qde = String::from_utf8(zip_entry(&bytes, "project.qde")).unwrap();
    let doc = Document::parse(&qde).unwrap();
    let root = doc.root_element();
    let t1 = kids(first(root, "Sources"), "TextSource")[0];
    let code_guids: Vec<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("Code"))
        .map(|c| c.attribute("guid").unwrap().to_string())
        .collect();

    // Interviewer ranges of the plain text.
    let mut pos = 0usize;
    let mut interviewer = Vec::new();
    for (role, t) in &turns {
        let len = t.chars().count();
        if *role == SpeakerRole::Interviewer {
            interviewer.push((pos, pos + len));
        }
        pos += len + 2;
    }
    let sels = kids(t1, "PlainTextSelection");
    assert_eq!(sels.len(), 4);
    let passages = passages(&s);
    for sel in &sels {
        let a: usize = sel.attribute("startPosition").unwrap().parse().unwrap();
        let b: usize = sel.attribute("endPosition").unwrap().parse().unwrap();
        assert!(a < b);
        // Never over an interviewer turn.
        assert!(
            interviewer.iter().all(|(s, e)| b <= *s || a >= *e),
            "selection {a}..{b} overlaps an interviewer turn"
        );
        // The selected text is a stored passage, with the codes of the passage.
        let selected = slice_scalars(&text, a, b).unwrap();
        let p = passages
            .iter()
            .find(|p| p.0 == "INT-01" && p.4 == selected)
            .unwrap_or_else(|| panic!("no passage with text {selected:?}"));
        let codings = kids(*sel, "Coding");
        assert_eq!(codings.len(), p.5.len());
        for c in codings {
            let target = first(c, "CodeRef").attribute("targetGUID").unwrap();
            assert!(code_guids.iter().any(|g| g == target), "dangling {target}");
        }
    }
}

#[test]
fn offset_map_is_recorded_beside_the_project() {
    let s = reference_store();
    let (bytes, _) = project(&s, &ExportOptions::default());
    let v: serde_json::Value =
        serde_json::from_slice(&zip_entry(&bytes, "coding-studio-offsets.json")).unwrap();
    assert_eq!(v["format"], "coding-studio-offsets/1");
    assert_eq!(
        v["guid_namespace"],
        guid::NAMESPACE.hyphenated().to_string()
    );
    let src = &v["sources"][0];
    assert_eq!(src["interview_id"], "INT-01");
    let units = src["units"].as_array().unwrap();
    assert_eq!(units.len(), 4);
    assert_eq!(units[0]["role"], "interviewer");
    assert_eq!(units[0]["start"], 0);
    assert_eq!(units[1]["role"], "respondent");
    let first_len = interview_one()[0].1.chars().count();
    assert_eq!(units[1]["start"], first_len + 2);
    let g1 = guid::source("INT-01");
    let text = zip_entry(&bytes, &format!("Sources/{g1}.txt"));
    assert_eq!(src["sha256"], cs_model::sha256_hex(&text));
}

#[test]
fn identical_input_gives_identical_bytes() {
    let s = reference_store();
    let (a, _) = project(&s, &ExportOptions::default());
    let (b, _) = project(&s, &ExportOptions::default());
    assert_eq!(a, b);
    let q1 = export_qdc(&s, ACTOR).unwrap().bytes;
    let q2 = export_qdc(&s, ACTOR).unwrap().bytes;
    assert_eq!(q1, q2);
    // The audit log grew, the exported bytes did not depend on it.
    assert!(
        audit_actions(&s)
            .iter()
            .filter(|a| *a == "refi.export.qdpx")
            .count()
            >= 2
    );
}

#[test]
fn guids_do_not_depend_on_the_database() {
    // Two projects built the same way give the same code GUIDs (the stable
    // ids are assigned in order), so exports of copies are comparable.
    let a = reference_store();
    let b = reference_store();
    let qa = String::from_utf8(export_qdc(&a, ACTOR).unwrap().bytes).unwrap();
    let qb = String::from_utf8(export_qdc(&b, ACTOR).unwrap().bytes).unwrap();
    assert_eq!(qa, qb);
}

#[test]
fn highlights_and_later_are_not_codes_unless_asked() {
    let s = reference_store();
    // A highlight: a passage with no code.
    s.create_passage(
        &cs_store::NewPassage {
            interview_id: "INT-02".into(),
            unit_id: "INT-02-U002".into(),
            start: Some(0),
            end: Some(12),
            offset_unit: Default::default(),
            text: None,
        },
        ACTOR,
    )
    .unwrap();
    // A passage parked with "Plus tard" only.
    code_span(&s, "INT-02", 2, Some((20, 40)), "LATER");

    let (bytes, summary) = project(&s, &ExportOptions::default());
    assert_eq!(summary.skipped_highlights, 1);
    assert_eq!(summary.skipped_without_exported_code, 1);
    assert_eq!(summary.selections, 5);
    let qde = String::from_utf8(zip_entry(&bytes, "project.qde")).unwrap();
    assert!(!qde.contains("Surlignages"));
    assert!(!qde.contains("Plus tard"));

    let (bytes, summary) = project(
        &s,
        &ExportOptions {
            include_highlights: true,
            ..Default::default()
        },
    );
    assert_eq!(summary.skipped_highlights, 0);
    assert_eq!(summary.selections, 6);
    let qde = String::from_utf8(zip_entry(&bytes, "project.qde")).unwrap();
    let doc = Document::parse(&qde).unwrap();
    let hl = doc
        .descendants()
        .find(|n| n.has_tag_name("Code") && n.attribute("name") == Some("Surlignages"))
        .expect("a code Surlignages");
    assert_eq!(hl.attribute("guid"), Some(guid::highlights_code().as_str()));
    let refs = doc
        .descendants()
        .filter(|n| n.has_tag_name("CodeRef") && n.attribute("targetGUID") == hl.attribute("guid"))
        .count();
    assert_eq!(refs, 1);
    assert!(!qde.contains("LATER"));
    assert!(!qde.contains("Plus tard"));
}

#[test]
fn a_stale_passage_is_left_out_and_counted() {
    let s = reference_store();
    s.correct_unit(
        "INT-02",
        "INT-02-U002",
        "Texte corrigé après le codage, entièrement différent.",
        SpeakerRole::Respondent,
        "erreur de transcription",
        ACTOR,
    )
    .unwrap();
    let (bytes, summary) = project(&s, &ExportOptions::default());
    assert_eq!(summary.skipped_stale, 1);
    assert_eq!(summary.selections, 4);
    // The corrected text is what the source holds.
    let g2 = guid::source("INT-02");
    let text = String::from_utf8(zip_entry(&bytes, &format!("Sources/{g2}.txt"))).unwrap();
    assert!(text.contains("Texte corrigé après le codage"));
}

#[test]
fn export_is_audited() {
    let s = reference_store();
    export_qdc(&s, ACTOR).unwrap();
    project(&s, &ExportOptions::default());
    let acts = audit_actions(&s);
    assert!(acts.contains(&"refi.export.qdc".to_string()));
    assert!(acts.contains(&"refi.export.qdpx".to_string()));
    assert!(s.verify_chain().unwrap().ok);
}
