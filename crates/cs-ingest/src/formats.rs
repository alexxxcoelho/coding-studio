//! Readers for DOCX, text-layer PDF and SRT transcripts.
//!
//! Each reader turns the file into `Line`s (optional start time, optional
//! speaker label, text) and hands them to `build_turns`, so the output has the
//! same shape as the Markdown and VTT readers. A line without a label continues
//! the previous speaker. Words are never changed: only whitespace is collapsed.

use std::io::{Cursor, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::LazyLock;

use cs_model::{CsError, SpeakerRole, Turn};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use regex::Regex;

use crate::parse::{assign_roles, push_turn, truncate};
use crate::text::{decode_entities, normalise_ws};

/// Turns and the warnings raised while reading.
pub(crate) type Parsed = (Vec<Turn>, Vec<String>);

/// Start time of a turn whose source carries none.
const NO_TIME: &str = "--:--";

/// One line of the source before turns are built.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Line {
    pub ts: Option<String>,
    pub speaker: Option<String>,
    pub text: String,
}

fn validation(msg: impl Into<String>) -> anyhow::Error {
    CsError::Validation(msg.into()).into()
}

static TS_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[?(\d{1,2}:\d{2}(?::\d{2})?)\]?\s*").unwrap());
static TS_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[?(\d{1,2}:\d{2}(?::\d{2})?)\]?$").unwrap());
static LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\p{L}[\p{L}\p{N} .'\u{2019}-]{0,39}?)\s*:\s*").unwrap());

/// A speaker label is at most five words and starts with a letter.
fn valid_label(label: &str) -> bool {
    !label.is_empty() && label.split_whitespace().count() <= 5
}

/// Splits `[hh:mm:ss] Name: text` into its parts. Text without a label keeps
/// `speaker` empty and continues the previous speaker.
pub(crate) fn split_line(raw: &str) -> Line {
    let text = normalise_ws(raw);
    let (ts, rest) = match TS_PREFIX.captures(&text) {
        Some(c) => (Some(c[1].to_string()), &text[c.get(0).unwrap().end()..]),
        None => (None, text.as_str()),
    };
    if let Some(c) = LABEL.captures(rest) {
        let label = c[1].trim().to_string();
        if valid_label(&label) {
            return Line {
                ts,
                speaker: Some(label),
                text: rest[c.get(0).unwrap().end()..].to_string(),
            };
        }
    }
    Line {
        ts,
        speaker: None,
        text: rest.to_string(),
    }
}

/// Turns lines into turns. A labelled line sets the current speaker even when
/// it has no text (a label on its own line). Consecutive lines of the same
/// speaker merge into one turn that keeps the first start time.
pub(crate) fn build_turns(
    lines: Vec<Line>,
    interviewer_names: &[String],
    warnings: &mut Vec<String>,
) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut speaker: Option<String> = None;
    for line in lines {
        if let Some(s) = line.speaker {
            speaker = Some(s);
        }
        if line.text.is_empty() {
            continue;
        }
        let Some(who) = speaker.clone() else {
            warnings.push(format!(
                "text without a speaker label skipped: \"{}\"",
                truncate(&line.text, 60)
            ));
            continue;
        };
        push_turn(
            &mut turns,
            Turn {
                speaker_raw: who,
                role: SpeakerRole::Unknown,
                timestamp: line.ts.unwrap_or_else(|| NO_TIME.to_string()),
                text: line.text,
            },
            true,
        );
    }
    assign_roles(&mut turns, interviewer_names);
    if turns.iter().any(|t| t.timestamp == NO_TIME) {
        warnings.push("some turns have no timestamp; they are stamped --:--".to_string());
    }
    turns
}

// --------------------------------------------------------------- DOCX

/// True when `attr` in `w:b` turns bold off (`w:val="0"` or `"false"`).
fn bold_is_on(e: &BytesStart) -> bool {
    !e.attributes().flatten().any(|a| {
        let key = a.key.local_name();
        let key: &str = key.as_ref();
        key == "val" && matches!(&*a.value, "0" | "false")
    })
}

/// Reads the paragraphs of `word/document.xml` as runs `(text, bold)`.
fn docx_paragraphs(bytes: &[u8]) -> Result<Vec<Vec<(String, bool)>>, anyhow::Error> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| validation(format!("not a readable DOCX file: {e}")))?;
    let mut entry = archive
        .by_name("word/document.xml")
        .map_err(|_| validation("DOCX has no word/document.xml: not a Word document"))?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|e| validation(format!("DOCX document.xml is unreadable: {e}")))?;

    let mut reader = Reader::from_str(&xml);
    let mut paragraphs = Vec::new();
    let mut para: Option<Vec<(String, bool)>> = None;
    let mut run: Option<(String, bool)> = None;
    let mut in_text = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| validation(format!("DOCX XML is malformed: {e}")))?;
        match event {
            Event::Eof => break,
            Event::Start(e) => {
                let local = e.local_name();
                let local: &str = local.as_ref();
                match local {
                    "p" => para = Some(Vec::new()),
                    "r" => run = Some((String::new(), false)),
                    "t" => in_text = true,
                    "b" => {
                        if let Some(r) = run.as_mut() {
                            r.1 = bold_is_on(&e);
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let local = e.local_name();
                let local: &str = local.as_ref();
                match local {
                    "b" => {
                        if let Some(r) = run.as_mut() {
                            r.1 = bold_is_on(&e);
                        }
                    }
                    "tab" | "br" | "cr" => {
                        if let Some(r) = run.as_mut() {
                            r.0.push(' ');
                        }
                    }
                    "p" => paragraphs.push(Vec::new()),
                    _ => {}
                }
            }
            Event::Text(t) if in_text => {
                if let Some(r) = run.as_mut() {
                    let raw: &str = &t;
                    r.0.push_str(
                        &quick_xml::escape::unescape(raw)
                            .map_or(raw.to_string(), |c| c.into_owned()),
                    );
                }
            }
            Event::GeneralRef(g) if in_text => {
                let name: &str = &g;
                let resolved = match g.resolve_char_ref() {
                    Ok(Some(c)) => c.to_string(),
                    _ => match name {
                        "amp" => "&".to_string(),
                        "lt" => "<".to_string(),
                        "gt" => ">".to_string(),
                        "quot" => "\"".to_string(),
                        "apos" => "'".to_string(),
                        other => format!("&{other};"),
                    },
                };
                if let Some(r) = run.as_mut() {
                    r.0.push_str(&resolved);
                }
            }
            Event::End(e) => {
                let local = e.local_name();
                let local: &str = local.as_ref();
                match local {
                    "t" => in_text = false,
                    "r" => {
                        if let (Some(r), Some(p)) = (run.take(), para.as_mut()) {
                            p.push(r);
                        }
                    }
                    "p" => {
                        if let Some(p) = para.take() {
                            paragraphs.push(p);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(paragraphs)
}

/// Line of a DOCX paragraph: a `Name:` prefix wins; otherwise a leading bold
/// run (after an optional timestamp) is the speaker label.
fn docx_line(runs: &[(String, bool)]) -> Line {
    let flat: String = runs.iter().map(|(t, _)| t.as_str()).collect();
    let plain = split_line(&flat);
    if plain.speaker.is_some() {
        return plain;
    }

    let mut i = 0;
    while i < runs.len() && runs[i].0.trim().is_empty() {
        i += 1;
    }
    let mut ts = None;
    if i < runs.len() && !runs[i].1 {
        if let Some(c) = TS_ONLY.captures(runs[i].0.trim()) {
            ts = Some(c[1].to_string());
            i += 1;
            while i < runs.len() && runs[i].0.trim().is_empty() {
                i += 1;
            }
        }
    }
    let start = i;
    while i < runs.len() && runs[i].1 {
        i += 1;
    }
    if i == start {
        return plain;
    }
    let label_raw: String = runs[start..i].iter().map(|(t, _)| t.as_str()).collect();
    let label = normalise_ws(&label_raw)
        .trim_end_matches(':')
        .trim()
        .to_string();
    if !valid_label(&label) {
        return plain;
    }
    let rest: String = runs[i..].iter().map(|(t, _)| t.as_str()).collect();
    let text =
        normalise_ws(rest.trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace()));
    Line {
        ts: ts.or(plain.ts),
        speaker: Some(label),
        text,
    }
}

/// Reads a DOCX transcript.
pub(crate) fn parse_docx(
    bytes: &[u8],
    interviewer_names: &[String],
) -> Result<Parsed, anyhow::Error> {
    let paragraphs = docx_paragraphs(bytes)?;
    let mut warnings = Vec::new();
    let lines: Vec<Line> = paragraphs.iter().map(|runs| docx_line(runs)).collect();
    let turns = build_turns(lines, interviewer_names, &mut warnings);
    Ok((turns, warnings))
}

// ---------------------------------------------------------------- PDF

/// Text of a PDF. Fails on files that are not PDFs or that the reader cannot
/// parse; a reader panic on a malformed file becomes an error too.
fn pdf_text(bytes: &[u8]) -> Result<String, anyhow::Error> {
    let head = &bytes[..bytes.len().min(1024)];
    if !head.windows(5).any(|w| w == b"%PDF-") {
        return Err(validation(
            "cannot read PDF: the file is not a PDF (no %PDF header)",
        ));
    }
    match catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    })) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(validation(format!("cannot read PDF text: {e}"))),
        Err(_) => Err(validation(
            "cannot read PDF text: the PDF reader failed on this file",
        )),
    }
}

/// Reads a text-layer PDF transcript, one line of text per PDF line. A PDF
/// without a text layer (a scan) yields no turns and a warning.
pub(crate) fn parse_pdf(
    bytes: &[u8],
    interviewer_names: &[String],
) -> Result<Parsed, anyhow::Error> {
    let text = pdf_text(bytes)?;
    let mut warnings = Vec::new();
    if text.trim().is_empty() {
        warnings.push(
            "PDF has no text layer (probably a scan): nothing can be read without OCR".to_string(),
        );
        return Ok((Vec::new(), warnings));
    }
    let lines: Vec<Line> = text.lines().map(split_line).collect();
    let turns = build_turns(lines, interviewer_names, &mut warnings);
    Ok((turns, warnings))
}

// ---------------------------------------------------------------- SRT

/// SRT subtitles: each cue's text becomes a line, stamped with the cue's start
/// time. A `Name:` prefix sets the speaker; cues without one continue the
/// previous speaker.
pub(crate) fn parse_srt(content: &str, interviewer_names: &[String]) -> Parsed {
    static TIMING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{1,2}:\d{2}(?::\d{2})?)(?:[.,]\d+)?\s*-->").unwrap());
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?[a-zA-Z][^>]*>").unwrap());

    let all: Vec<&str> = content.lines().collect();
    let mut lines = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let Some(c) = TIMING.captures(all[i].trim()) else {
            i += 1;
            continue;
        };
        let ts = c[1].to_string();
        i += 1;
        let mut body = Vec::new();
        while i < all.len() && !all[i].trim().is_empty() {
            body.push(all[i].trim());
            i += 1;
        }
        let raw = body.join(" ");
        let joined = TAG.replace_all(&raw, " ");
        let mut line = split_line(&decode_entities(&joined));
        line.ts = Some(ts);
        lines.push(line);
    }
    let mut warnings = Vec::new();
    let turns = build_turns(lines, interviewer_names, &mut warnings);
    (turns, warnings)
}
