//! Import of a REFI-QDA project (`.qdpx`): round trip with the exporter, and
//! files written by other tools.

mod common;

use common::*;
use cs_model::SpeakerRole;
use cs_qdpx::{export_qdpx, import_qdpx, Error, ExportOptions, Mode};
use cs_store::{PassageFilter, Store};

fn export(s: &Store) -> Vec<u8> {
    export_qdpx(s, &ExportOptions::default(), ACTOR)
        .unwrap()
        .bytes
}

fn n_passages(s: &Store) -> usize {
    s.passages(&PassageFilter::default()).unwrap().len()
}

// ------------------------------------------------------------ round trip

#[test]
fn round_trip_into_an_empty_project_gives_the_same_tree_and_passages() {
    let src = reference_store();
    let bytes = export(&src);
    let dst = store_with_interviews();
    assert!(tree(&dst).is_empty() && passages(&dst).is_empty());

    let r = import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    assert!(r.applied);
    assert_eq!(r.kind, "qdpx");
    assert_eq!(r.codes_created, 4);
    assert_eq!(r.sources_imported, 2);
    assert_eq!(r.sources_skipped, 0);
    assert_eq!(r.passages_created, 5);
    assert_eq!(r.codings_added, 6);
    assert_eq!(r.selections_skipped, 0);
    assert!(r.issues.is_empty(), "{:?}", r.issues);

    assert_eq!(tree(&dst), tree(&src), "same code tree");
    assert_eq!(passages(&dst), passages(&src), "same passages");
    // The passage with an emoji before and inside it came back exact.
    assert!(passages(&dst).iter().any(|p| p.4.contains('😀')));
}

#[test]
fn importing_the_same_project_twice_adds_nothing() {
    let src = reference_store();
    let bytes = export(&src);
    let dst = store_with_interviews();
    import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    let before = (tree(&dst), passages(&dst), n_passages(&dst));
    let r = import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 0);
    assert_eq!(r.codes_matched, 4);
    assert_eq!(r.passages_created, 0);
    assert_eq!(r.codings_added, 0);
    assert_eq!(r.codings_existing, 6);
    assert_eq!((tree(&dst), passages(&dst), n_passages(&dst)), before);
}

#[test]
fn importing_into_the_project_it_came_from_changes_nothing() {
    let src = reference_store();
    let bytes = export(&src);
    let before = (tree(&src), passages(&src));
    let r = import_qdpx(&src, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 0);
    assert_eq!(r.passages_created, 0);
    assert_eq!(r.codings_added, 0);
    assert_eq!((tree(&src), passages(&src)), before);
}

#[test]
fn dry_run_gives_the_counts_of_the_apply_and_writes_nothing() {
    let src = reference_store();
    let bytes = export(&src);
    let dst = store_with_interviews();
    let events = audit_actions(&dst).len();
    let dry = import_qdpx(&dst, &bytes, Mode::DryRun, ACTOR).unwrap();
    assert!(!dry.applied);
    assert!(tree(&dst).is_empty());
    assert_eq!(n_passages(&dst), 0);
    assert_eq!(audit_actions(&dst).len(), events);
    let applied = import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    for (a, b) in [
        (dry.codes_created, applied.codes_created),
        (dry.passages_created, applied.passages_created),
        (dry.codings_added, applied.codings_added),
        (dry.sources_imported, applied.sources_imported),
    ] {
        assert_eq!(a, b);
    }
    assert!(dry.codes.iter().all(|c| c.code_id.is_none()));
    assert!(applied.codes.iter().all(|c| c.code_id.is_some()));
}

#[test]
fn an_import_is_audited_end_to_end() {
    let src = reference_store();
    let bytes = export(&src);
    let dst = store_with_interviews();
    import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    let acts = audit_actions(&dst);
    assert_eq!(
        acts.iter().filter(|a| *a == "codebook.code.create").count(),
        4
    );
    assert!(acts.iter().filter(|a| *a == "passage.create").count() >= 5);
    assert_eq!(acts.iter().filter(|a| *a == "passage.code").count(), 6);
    assert_eq!(acts.last().map(String::as_str), Some("refi.import.qdpx"));
    assert!(dst.verify_chain().unwrap().ok);
}

// ---------------------------------------------------- source matching

#[test]
fn a_source_whose_text_changed_is_reported_and_skipped() {
    let src = reference_store();
    let bytes = export(&src);
    let dst = store_with_interviews();
    dst.correct_unit(
        "INT-02",
        "INT-02-U002",
        "Texte différent de celui exporté.",
        SpeakerRole::Respondent,
        "test",
        ACTOR,
    )
    .unwrap();
    let r = import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.sources_imported, 1);
    assert_eq!(r.sources_skipped, 1);
    let skipped = r.sources.iter().find(|s| !s.imported).unwrap();
    assert_eq!(skipped.name, "INT-02");
    assert_eq!(skipped.interview_id.as_deref(), Some("INT-02"));
    assert!(skipped.reason.as_deref().unwrap().contains("differs"));
    // The codes are still imported, INT-01 is coded, INT-02 is untouched.
    assert_eq!(r.codes_created, 4);
    assert!(passages(&dst).iter().all(|p| p.0 == "INT-01"));
}

#[test]
fn a_source_without_interview_is_reported_and_skipped() {
    let src = reference_store();
    let bytes = export(&src);
    let dst = Store::open_in_memory().unwrap();
    add_interview(&dst, "INT-01", &interview_one());
    let r = import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.sources_imported, 1);
    assert_eq!(r.sources_skipped, 1);
    let s = r.sources.iter().find(|s| !s.imported).unwrap();
    assert!(s.reason.as_deref().unwrap().contains("no interview"));
    assert_eq!(s.interview_id, None);
}

fn rewrite_zip(bytes: &[u8], f: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> = zip_names(bytes)
        .into_iter()
        .filter_map(|n| {
            let data = zip_entry(bytes, &n);
            f(&n, data).map(|d| (n, d))
        })
        .collect();
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    make_zip(&refs)
}

#[test]
fn a_renamed_source_is_found_by_its_guid_or_else_by_its_text() {
    let src = reference_store();
    let bytes = export(&src);
    let rename = |name: &str, data: Vec<u8>| -> Option<Vec<u8>> {
        if name == "project.qde" {
            let s = String::from_utf8(data)
                .unwrap()
                .replace("name=\"INT-01\"", "name=\"Entretien un\"");
            Some(s.into_bytes())
        } else {
            Some(data)
        }
    };
    // With the offsets file, the recorded GUID leads to the interview.
    let renamed = rewrite_zip(&bytes, rename);
    let dst = store_with_interviews();
    let r = import_qdpx(&dst, &renamed, Mode::DryRun, ACTOR).unwrap();
    let s = r.sources.iter().find(|s| s.name == "Entretien un").unwrap();
    assert_eq!(s.interview_id.as_deref(), Some("INT-01"));
    assert_eq!(s.matched_by.as_deref(), Some("guid"));
    // Without it, the text identifies the interview.
    let bare = rewrite_zip(&renamed, |n, d| {
        (n != "coding-studio-offsets.json").then_some(d)
    });
    let r = import_qdpx(&dst, &bare, Mode::DryRun, ACTOR).unwrap();
    let s = r.sources.iter().find(|s| s.name == "Entretien un").unwrap();
    assert_eq!(s.interview_id.as_deref(), Some("INT-01"));
    assert_eq!(s.matched_by.as_deref(), Some("digest"));
}

// ------------------------------------------- files from other tools

const CODES: &str = r#"<CodeBook>
    <Codes>
      <Code guid="11111111-1111-4111-8111-111111111111" name="Formation" isCodable="true"/>
      <Code guid="22222222-2222-4222-8222-222222222222" name="Réseau" isCodable="true"/>
    </Codes></CodeBook>"#;

fn foreign_project(sources: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Project xmlns="urn:QDA-XML:project:1.0" name="Étude" origin="MAXQDA 24" creatingUserGUID="99999999-9999-4999-8999-999999999999">
  <Users><User guid="99999999-9999-4999-8999-999999999999" name="Chercheur"/></Users>
  {CODES}
  <Variables/>
  <Sources>{sources}</Sources>
  <Notes/><Sets/>
</Project>"#
    )
}

fn sel(guid: &str, a: usize, b: usize, refs: &[&str]) -> String {
    let codings: String = refs
        .iter()
        .map(|r| format!(r#"<Coding guid="c-{r}"><CodeRef targetGUID="{r}"/></Coding>"#))
        .collect();
    format!(
        r#"<PlainTextSelection guid="{guid}" startPosition="{a}" endPosition="{b}">{codings}</PlainTextSelection>"#
    )
}

const FORMATION: &str = "11111111-1111-4111-8111-111111111111";
const RESEAU: &str = "22222222-2222-4222-8222-222222222222";

fn text_of(turns: &[Turn]) -> String {
    turns.iter().map(|t| t.1).collect::<Vec<_>>().join("\n\n")
}

fn foreign_zip(name: &str, text: &str, selections: &str) -> Vec<u8> {
    let src = format!(
        r#"<TextSource guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="{name}" plainTextPath="internal://t1.txt">{selections}</TextSource>"#
    );
    make_zip(&[
        ("project.qde", foreign_project(&src).as_bytes()),
        ("Sources/t1.txt", text.as_bytes()),
    ])
}

#[test]
fn a_foreign_project_is_matched_by_folder_name_and_clipped_to_respondent_turns() {
    // INT-02: interviewer 0..30, blank line 30..32, respondent from 32.
    let turns = interview_two();
    let int_len = turns[0].1.chars().count();
    assert_eq!(int_len, 30);
    let resp_len = turns[1].1.chars().count();
    let selections = [
        // Across interviewer and respondent: clipped to the respondent part.
        sel("s1", 20, 60, &[FORMATION]),
        // Only on the interviewer turn: skipped.
        sel("s2", 5, 20, &[FORMATION]),
        // Only on the blank line: skipped.
        sel("s3", 30, 32, &[FORMATION]),
        // Two codes on a respondent span.
        sel("s4", 40, 70, &[FORMATION, RESEAU]),
        // No coding (a highlight in the other tool): skipped.
        sel("s5", 40, 50, &[]),
        // An unknown code: skipped.
        sel("s6", 40, 50, &["deadbeef-dead-4ead-8ead-deadbeefdead"]),
        // Past the end of the text: skipped.
        sel("s7", 40, 32 + resp_len + 10, &[RESEAU]),
    ]
    .concat();
    let bytes = foreign_zip("INT-02_Test", &text_of(&turns), &selections);
    let dst = store_with_interviews();
    let r = import_qdpx(&dst, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.codes_created, 2);
    assert_eq!(r.sources_imported, 1);
    assert_eq!(r.sources[0].matched_by.as_deref(), Some("name"));
    assert_eq!(r.selections_skipped, 5, "{:#?}", r.issues);
    assert_eq!(r.passages_created, 2);
    let p = passages(&dst);
    assert_eq!(p.len(), 2);
    // s1: positions 32..60 of the text, 0..28 of the unit.
    assert_eq!((p[0].1.as_str(), p[0].2, p[0].3), ("INT-02-U002", 0, 28));
    assert_eq!(p[0].4, turns[1].1.chars().take(28).collect::<String>());
    assert_eq!(p[0].5, vec!["Formation"]);
    assert_eq!((p[1].2, p[1].3), (8, 38));
    assert_eq!(p[1].5, vec!["Formation", "Réseau"]);
    // Never a passage on the interviewer turn.
    assert!(p.iter().all(|x| x.1 != "INT-02-U001"));
    assert!(r.issues.iter().any(|i| i.message.contains("split")));
    assert!(r.issues.iter().any(|i| i.message.contains("interviewer")));
    assert!(r
        .issues
        .iter()
        .any(|i| i.message.contains("no usable code")));
    assert!(r.issues.iter().any(|i| i.message.contains("unknown code")));
    assert!(r.issues.iter().any(|i| i.message.contains("past the end")));
}

#[test]
fn a_selection_over_two_respondent_turns_becomes_two_passages() {
    let s = Store::open_in_memory().unwrap();
    let turns: Vec<Turn> = vec![
        (RESP, "Premier tour de parole."),
        (RESP, "Deuxième tour de parole."),
    ];
    add_interview(&s, "INT-09", &turns);
    // "tour de parole.\n\nDeuxième tour" = 8..23, blank line, 0..11 of unit 2.
    let first = turns[0].1.chars().count();
    let bytes = foreign_zip(
        "INT-09",
        &text_of(&turns),
        &sel("s1", 8, first + 2 + 11, &[FORMATION]),
    );
    let r = import_qdpx(&s, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.passages_created, 2);
    let p = passages(&s);
    assert_eq!(p[0].4, "tour de parole.");
    assert_eq!(p[1].4, "Deuxième to");
    assert_eq!((p[1].1.as_str(), p[1].2, p[1].3), ("INT-09-U002", 0, 11));
    assert!(r.issues.iter().any(|i| i.message.contains("2 passage(s)")));
}

#[test]
fn text_with_a_byte_order_mark_still_matches() {
    let turns = interview_two();
    let mut text = vec![0xEF, 0xBB, 0xBF];
    text.extend(text_of(&turns).as_bytes());
    let src = r#"<TextSource guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="INT-02" plainTextPath="internal://t1.txt"/>"#;
    let bytes = make_zip(&[
        ("project.qde", foreign_project(src).as_bytes()),
        ("Sources/t1.txt", &text),
    ]);
    let s = store_with_interviews();
    let r = import_qdpx(&s, &bytes, Mode::DryRun, ACTOR).unwrap();
    assert_eq!(r.sources_imported, 1);
}

#[test]
fn sources_that_are_not_text_or_not_reachable_are_listed() {
    let src = r#"<PDFSource guid="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" name="rapport" path="internal://r.pdf"/>
        <TextSource guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="INT-02" plainTextPath="relative://elsewhere.txt"/>
        <TextSource guid="cccccccc-cccc-4ccc-8ccc-cccccccccccc" name="INT-01" plainTextPath="internal://absent.txt"/>"#;
    let bytes = make_zip(&[("project.qde", foreign_project(src).as_bytes())]);
    let s = store_with_interviews();
    let r = import_qdpx(&s, &bytes, Mode::DryRun, ACTOR).unwrap();
    assert_eq!(r.other_sources_ignored, vec!["PDFSource rapport"]);
    assert_eq!(r.sources_skipped, 2);
    assert!(r.sources[0].reason.as_deref().unwrap().contains("external"));
    assert!(r.sources[1]
        .reason
        .as_deref()
        .unwrap()
        .contains("not in the archive"));
    assert_eq!(r.codes_created, 2, "the codebook is imported anyway");
}

#[test]
fn inline_text_is_read_when_there_is_no_file() {
    let turns = interview_two();
    let src = format!(
        r#"<TextSource guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="INT-02"><PlainTextContent>{}</PlainTextContent>{}</TextSource>"#,
        text_of(&turns),
        sel("s1", 32, 50, &[FORMATION])
    );
    let bytes = make_zip(&[("project.qde", foreign_project(&src).as_bytes())]);
    let s = store_with_interviews();
    let r = import_qdpx(&s, &bytes, Mode::Apply, ACTOR).unwrap();
    assert_eq!(r.passages_created, 1);
}

// ---------------------------------------------------------- bad files

fn rejected(bytes: &[u8]) -> Vec<String> {
    let s = store_with_interviews();
    let events = audit_actions(&s).len();
    match import_qdpx(&s, bytes, Mode::Apply, ACTOR) {
        Err(Error::Rejected(issues)) => {
            assert!(tree(&s).is_empty() && n_passages(&s) == 0);
            assert_eq!(
                audit_actions(&s).len(),
                events,
                "a rejected file leaves no trace"
            );
            issues.into_iter().map(|i| i.message).collect()
        }
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[test]
fn files_that_are_not_projects_get_an_error_report() {
    assert!(rejected(b"not a zip")
        .iter()
        .any(|m| m.contains("not a ZIP archive")));
    assert!(rejected(b"")
        .iter()
        .any(|m| m.contains("not a ZIP archive")));
    let no_project = make_zip(&[("Sources/t1.txt", b"x")]);
    assert!(rejected(&no_project)
        .iter()
        .any(|m| m.contains("no project.qde")));
    let broken = make_zip(&[("project.qde", b"<Project><CodeBook></Project>")]);
    assert!(rejected(&broken)
        .iter()
        .any(|m| m.contains("malformed XML")));
    let wrong = make_zip(&[("project.qde", b"<CodeBook/>")]);
    assert!(rejected(&wrong)
        .iter()
        .any(|m| m.contains("not a REFI-QDA project")));
    let dup = make_zip(&[(
        "project.qde",
        br#"<Project xmlns="urn:QDA-XML:project:1.0" name="x"><CodeBook><Codes>
            <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="A" isCodable="true"/>
            <Code guid="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" name="B" isCodable="true"/>
          </Codes></CodeBook></Project>"#,
    )]);
    assert!(rejected(&dup).iter().any(|m| m.contains("already used")));
}

#[test]
fn a_qdc_given_as_a_project_and_the_reverse_are_both_explained() {
    let qdc = include_bytes!("fixtures/nested.qdc");
    assert!(rejected(qdc)
        .iter()
        .any(|m| m.contains("not a ZIP archive")));
    let project = export(&reference_store());
    let s = store_with_interviews();
    match cs_qdpx::import_qdc(&s, &project, Mode::DryRun, ACTOR) {
        Err(Error::Rejected(_)) => {}
        other => panic!("{other:?}"),
    }
}
