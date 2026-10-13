//! Transcript formats beyond Markdown and raw text: DOCX, text-layer PDF,
//! SRT and VTT subtitle files. Every reader must return the same turn
//! structure: speaker label, start time (`--:--` when absent), text.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use cs_ingest::{
    build_segmentation, classify, parse_document, parse_source,
    scan_library,
};
use cs_model::{SourceKind, SpeakerRole, Turn};

type Row = (String, String, SpeakerRole, String);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn rows(turns: &[Turn]) -> Vec<Row> {
    turns
        .iter()
        .map(|t| {
            (
                t.speaker_raw.clone(),
                t.timestamp.clone(),
                t.role,
                t.text.clone(),
            )
        })
        .collect()
}

fn row(speaker: &str, ts: &str, role: SpeakerRole, text: &str) -> Row {
    (speaker.into(), ts.into(), role, text.into())
}

fn write_tmp(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A minimal DOCX: one `<w:p>` per entry, each made of `(text, bold)` runs.
fn docx_bytes(paragraphs: &[Vec<(&str, bool)>]) -> Vec<u8> {
    let mut body = String::new();
    for runs in paragraphs {
        body.push_str("<w:p>");
        for (text, bold) in runs {
            body.push_str("<w:r>");
            if *bold {
                body.push_str("<w:rPr><w:b/></w:rPr>");
            }
            body.push_str(&format!("<w:t xml:space=\"preserve\">{text}</w:t></w:r>"));
        }
        body.push_str("</w:p>");
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}</w:body></w:document>"
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(b"<?xml version=\"1.0\"?><Types/>").unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(document.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

// ---------------------------------------------------------------- DOCX

/// The label the fixtures give to the interviewer.
fn fixture_names() -> Vec<String> {
    vec!["Interviewer Person A".to_string()]
}

#[test]
fn docx_speakers_from_name_prefix_and_bold_run() {
    let bytes = docx_bytes(&[
        vec![("[00:01:02] Interviewer Person A: Hello there.", false)],
        vec![("Roueche Celine", true), (": Hi, good morning.", false)],
        vec![("and thanks for having me.", false)],
        vec![("Interviewer Person A: How are you today?", false)],
    ]);
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "interview.docx", &bytes);

    let (turns, warnings) = parse_document(&path, &fixture_names()).unwrap();

    assert_eq!(
        rows(&turns),
        vec![
            row(
                "Interviewer Person A",
                "00:01:02",
                SpeakerRole::Interviewer,
                "Hello there."
            ),
            row(
                "Roueche Celine",
                "--:--",
                SpeakerRole::Respondent,
                "Hi, good morning. and thanks for having me."
            ),
            row(
                "Interviewer Person A",
                "--:--",
                SpeakerRole::Interviewer,
                "How are you today?"
            ),
        ]
    );
    assert!(
        warnings.iter().any(|w| w.contains("timestamp")),
        "{warnings:?}"
    );
}

#[test]
fn docx_bold_label_without_colon_after_timestamp_and_entities() {
    let bytes = docx_bytes(&[vec![
        ("[00:03:10] ", false),
        ("Roueche Celine", true),
        (" Hi &amp; welcome.", false),
    ]]);
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "bold.docx", &bytes);

    let (turns, _) = parse_document(&path, &[]).unwrap();

    assert_eq!(
        rows(&turns),
        vec![row(
            "Roueche Celine",
            "00:03:10",
            SpeakerRole::Unknown,
            "Hi & welcome."
        )]
    );
}

#[test]
fn docx_same_speaker_consecutive_paragraphs_merge_keeping_first_time() {
    let bytes = docx_bytes(&[
        vec![("[00:00:10] Roueche Celine: First part.", false)],
        vec![("Roueche Celine: Second part.", false)],
    ]);
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "merge.docx", &bytes);

    let (turns, _) = parse_document(&path, &[]).unwrap();

    assert_eq!(
        rows(&turns),
        vec![row(
            "Roueche Celine",
            "00:00:10",
            SpeakerRole::Unknown,
            "First part. Second part."
        )]
    );
}

#[test]
fn docx_not_a_zip_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "broken.docx", b"this is not a zip archive");
    let err = parse_document(&path, &[]).unwrap_err();
    assert!(format!("{err:#}").contains("DOCX"), "{err:#}");
}

#[test]
fn docx_without_document_xml_is_an_error() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("readme.txt", opts).unwrap();
    zip.write_all(b"nothing here").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "empty.docx", &bytes);
    let err = parse_document(&path, &[]).unwrap_err();
    assert!(format!("{err:#}").contains("document.xml"), "{err:#}");
}

// ----------------------------------------------------------------- PDF

#[test]
fn pdf_text_layer_gives_speaker_turns() {
    let (turns, warnings) = parse_document(
        &fixture("interview_text_layer.pdf"),
        &fixture_names(),
    )
    .unwrap();

    assert_eq!(
        rows(&turns),
        vec![
            row(
                "Interviewer Person A",
                "--:--",
                SpeakerRole::Interviewer,
                "Hello there."
            ),
            row(
                "Roueche Celine",
                "--:--",
                SpeakerRole::Respondent,
                "Hi, good morning. and thanks for having me."
            ),
            row(
                "Interviewer Person A",
                "--:--",
                SpeakerRole::Interviewer,
                "How are you today?"
            ),
        ]
    );
    assert!(!warnings.iter().any(|w| w.contains("text layer")));
}

#[test]
fn pdf_without_text_layer_warns_and_yields_no_turns() {
    let (turns, warnings) = parse_document(
        &fixture("interview_no_text_layer.pdf"),
        &fixture_names(),
    )
    .unwrap();

    assert!(turns.is_empty());
    assert!(
        warnings.iter().any(|w| w.contains("no text layer")),
        "{warnings:?}"
    );
}

#[test]
fn broken_pdf_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "broken.pdf", b"%PDF-1.4\n\x00\x01 garbage \xff\xfe");
    let err = parse_document(&path, &[]).unwrap_err();
    assert!(format!("{err:#}").contains("PDF"), "{err:#}");
}

// ----------------------------------------------------------- SRT and VTT

#[test]
fn srt_name_prefix_merges_cues_keeping_first_timestamp() {
    let names = fixture_names();
    let (turns, warnings) = parse_document(&fixture("interview_subtitles.srt"), &names).unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        rows(&turns),
        vec![
            row(
                "Interviewer Person A",
                "00:00:01",
                SpeakerRole::Interviewer,
                "Hello there."
            ),
            row(
                "Roueche Celine",
                "00:00:03",
                SpeakerRole::Respondent,
                "Hi, good morning. And thanks for having me."
            ),
            row(
                "Interviewer Person A",
                "00:00:09",
                SpeakerRole::Interviewer,
                "How are you today?"
            ),
        ]
    );
}

#[test]
fn srt_is_read_the_same_through_the_kind_based_api() {
    let names = fixture_names();
    let path = fixture("interview_subtitles.srt");
    assert_eq!(classify("interview_subtitles.srt"), SourceKind::Vtt);
    let (direct, _) = parse_document(&path, &names).unwrap();
    let (via_kind, _) = parse_source(&path, SourceKind::Vtt, &names).unwrap();
    assert_eq!(rows(&direct), rows(&via_kind));
}

#[test]
fn srt_with_crlf_line_endings_parses() {
    let text = "1\r\n00:00:01,000 --> 00:00:02,000\r\nInterviewer Person A: Hi.\r\n\r\n\
                2\r\n00:00:02,500 --> 00:00:04,000\r\nRoueche Celine: Hello.\r\n";
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "crlf.srt", text.as_bytes());
    let (turns, _) = parse_document(&path, &fixture_names()).unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[1].speaker_raw, "Roueche Celine");
    assert_eq!(turns[1].timestamp, "00:00:02");
}

#[test]
fn vtt_file_voice_tags_merge_and_decode_entities() {
    let names = fixture_names();
    let (turns, _) = parse_document(&fixture("interview_conference.vtt"), &names).unwrap();
    assert_eq!(
        rows(&turns),
        vec![
            row(
                "Interviewer Person A",
                "00:00:01",
                SpeakerRole::Interviewer,
                "Hello there."
            ),
            row(
                "Roueche Céline",
                "00:00:03",
                SpeakerRole::Respondent,
                "Hi, good morning. And thanks for having me."
            ),
        ]
    );
}

// ------------------------------------------------------------ failures

#[test]
fn unknown_extension_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "notes.rtf", b"{\\rtf1 hello}");
    let err = parse_document(&path, &[]).unwrap_err();
    assert!(format!("{err:#}").contains("unsupported"), "{err:#}");
}

#[test]
fn missing_file_is_an_error() {
    let err = parse_document(Path::new("/nonexistent/interview.docx"), &[]).unwrap_err();
    assert!(format!("{err:#}").contains("cannot read"), "{err:#}");
}

// ------------------------------------------------- library scan and segmentation

/// One interview folder holding a single file, under a fresh library root.
fn library_with(file_name: &str, bytes: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("INT33_RomanEppenberger");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(file_name), bytes).unwrap();
    dir
}

#[test]
fn docx_only_interview_is_scanned_and_segmentable() {
    let bytes = docx_bytes(&[
        vec![("[00:01:02] Interviewer Person A: Hello there.", false)],
        vec![("Roueche Celine", true), (": Hi, good morning.", false)],
        vec![("Interviewer Person A: How are you today?", false)],
    ]);
    let lib = library_with("EMBA - Interview - Roman.docx", &bytes);
    let entries = scan_library(lib.path()).unwrap();
    let entry = &entries[0];

    let docx = entry
        .sources
        .iter()
        .find(|s| s.rel_path == "EMBA - Interview - Roman.docx")
        .unwrap();
    assert_eq!(docx.kind, SourceKind::Docx);
    assert_eq!(
        entry.primary_source.as_deref(),
        Some("EMBA - Interview - Roman.docx")
    );

    let seg = build_segmentation(
        entry,
        "EMBA - Interview - Roman.docx",
        &fixture_names(),
    )
    .unwrap();
    assert_eq!(seg.units.len(), 3);
}

#[test]
fn text_pdf_only_interview_is_scanned_and_segmentable() {
    let bytes = std::fs::read(fixture("interview_text_layer.pdf")).unwrap();
    let lib = library_with("EMBA - Interview - Roman.pdf", &bytes);
    let entries = scan_library(lib.path()).unwrap();
    let entry = &entries[0];

    let pdf = entry
        .sources
        .iter()
        .find(|s| s.rel_path == "EMBA - Interview - Roman.pdf")
        .unwrap();
    assert_eq!(pdf.kind, SourceKind::Pdf);
    assert_eq!(
        entry.primary_source.as_deref(),
        Some("EMBA - Interview - Roman.pdf")
    );
    assert!(!entry.warnings.iter().any(|w| w.contains("skipped")));

    let seg = build_segmentation(
        entry,
        "EMBA - Interview - Roman.pdf",
        &fixture_names(),
    )
    .unwrap();
    assert_eq!(seg.units.len(), 3);
}
