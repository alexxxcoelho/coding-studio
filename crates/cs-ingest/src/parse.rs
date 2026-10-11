//! Readers that turn a transcript file into speaker turns.
//!
//! Each reader returns the turns in source order plus plain-language warnings
//! for every line that was deliberately not turned into text (notes, cues
//! without a speaker). Text is only trimmed, whitespace-collapsed, stripped of
//! markdown bold markers and decoded from HTML entities: words are never changed.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use cs_model::{SpeakerRole, Turn};
use regex::Regex;

use crate::text::{decode_entities, normalise_ws, skip_frontmatter, strip_bold};

/// Parsed turns and the warnings raised while reading.
pub type Parsed = (Vec<Turn>, Vec<String>);

const TS: &str = r"\d{1,2}:\d{2}(?::\d{2})?";

// ------------------------------------------------------------------ helpers

/// Appends a turn, or extends the previous one when `merge` is set and the
/// speaker is the same.
fn push_turn(turns: &mut Vec<Turn>, turn: Turn, merge: bool) {
    if turn.text.is_empty() {
        return;
    }
    if merge {
        if let Some(last) = turns.last_mut() {
            if last.speaker_raw == turn.speaker_raw {
                last.text.push(' ');
                last.text.push_str(&turn.text);
                return;
            }
        }
    }
    turns.push(turn);
}

fn is_interviewer(name: &str, interviewer_names: &[String]) -> bool {
    let n = name.trim().to_lowercase();
    interviewer_names
        .iter()
        .any(|i| i.trim().to_lowercase() == n)
}

/// Role of a label written by the cleaning pass (`INT`, `RESP`, `REP`).
fn role_from_label(label: &str) -> SpeakerRole {
    match label {
        "INT" => SpeakerRole::Interviewer,
        l if l.starts_with("RESP") || l == "REP" || l == "RES" => SpeakerRole::Respondent,
        _ => SpeakerRole::Unknown,
    }
}

// ---------------------------------------------------------------------- VTT

/// WebVTT with `<v Name>` voice tags. Consecutive cues of the same speaker are
/// merged into one turn, stamped with the start of the first cue.
pub fn parse_vtt(content: &str, interviewer_names: &[String]) -> Parsed {
    static VOICE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<v\s+([^>]*)>").unwrap());
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?[a-zA-Z][^>]*>").unwrap());
    static TIMING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{1,2}:\d{2}(?::\d{2})?)(?:[.,]\d+)?\s*-->").unwrap());

    let mut raw: Vec<(String, String, String)> = Vec::new(); // speaker, ts, text
    let mut warnings = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(caps) = TIMING.captures(line) {
            let ts = caps[1].to_string();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                body.push(lines[i].trim());
                i += 1;
            }
            let joined = body.join(" ");
            let speaker = VOICE
                .captures(&joined)
                .map(|c| normalise_ws(&decode_entities(&c[1])));
            let text = normalise_ws(&decode_entities(&TAG.replace_all(&joined, " ")));
            if text.is_empty() {
                continue;
            }
            match speaker {
                Some(s) => raw.push((s, ts, text)),
                None => match raw.last() {
                    // A cue without a voice tag continues the previous speaker.
                    Some((prev, _, _)) => {
                        let prev = prev.clone();
                        raw.push((prev, ts, text));
                    }
                    None => warnings.push(format!("cue at {ts} has no speaker tag; skipped")),
                },
            }
            continue;
        }
        i += 1;
    }

    let speakers: BTreeSet<&str> = raw.iter().map(|(s, _, _)| s.as_str()).collect();
    let two_speakers = speakers.len() == 2;
    let mut turns = Vec::new();
    for (speaker, ts, text) in &raw {
        let role = if is_interviewer(speaker, interviewer_names) {
            SpeakerRole::Interviewer
        } else if two_speakers
            && speakers
                .iter()
                .any(|s| is_interviewer(s, interviewer_names))
        {
            SpeakerRole::Respondent
        } else {
            SpeakerRole::Unknown
        };
        push_turn(
            &mut turns,
            Turn {
                speaker_raw: speaker.clone(),
                role,
                timestamp: ts.clone(),
                text: text.clone(),
            },
            true,
        );
    }
    (turns, warnings)
}

// --------------------------------------------------------- timestamped text

/// Raw ASR text: `[00:00:11] S2: text` or `[01:09] [Speaker 1]: text`, one per
/// line or many on a single line. Splits on every marker; consecutive turns of
/// the same speaker are merged.
pub fn parse_timestamped_text(content: &str) -> Parsed {
    static MARK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(&format!(
            r"\[({TS})\]\s*(?:\[([^\]\n]{{1,40}})\]|([A-Za-z][A-Za-z0-9 _.-]{{0,30}}?))\s*:\s"
        ))
        .unwrap()
    });
    let content = content.replace('\r', "");
    let mut turns = Vec::new();
    let mut warnings = Vec::new();
    let marks: Vec<_> = MARK.captures_iter(&content).collect();
    if let Some(first) = marks.first() {
        let lead = normalise_ws(&content[..first.get(0).unwrap().start()]);
        if !lead.is_empty() {
            warnings.push(format!(
                "text before the first timestamp skipped: \"{}\"",
                truncate(&lead, 60)
            ));
        }
    } else if !content.trim().is_empty() {
        warnings.push("no `[timestamp] speaker:` marker found".to_string());
    }
    for (k, caps) in marks.iter().enumerate() {
        let whole = caps.get(0).unwrap();
        let end = marks
            .get(k + 1)
            .map(|n| n.get(0).unwrap().start())
            .unwrap_or(content.len());
        let speaker = caps
            .get(2)
            .or_else(|| caps.get(3))
            .map(|m| normalise_ws(m.as_str()))
            .unwrap_or_default();
        let text = normalise_ws(&content[whole.end()..end]);
        push_turn(
            &mut turns,
            Turn {
                speaker_raw: speaker,
                role: SpeakerRole::Unknown,
                timestamp: caps[1].to_string(),
                text,
            },
            true,
        );
    }
    (turns, warnings)
}

// --------------------------------------------------------- cleaned markdown

/// `02_transcript_cleaned_diarised.md`. Recognises the four turn styles of
/// the cleaning pass; a turn runs over following paragraphs until the next
/// turn, a heading or a `---` rule.
pub fn parse_cleaned_markdown(content: &str) -> Parsed {
    static STYLE_A: LazyLock<Regex> = LazyLock::new(|| {
        // **INT `[01:53]`:** text
        Regex::new(&format!(
            r"^\*\*([A-Z][A-Z0-9]{{0,7}})\s+`\[({TS})(\+?)\]`\s*:?\s*\*\*\s*:?\s*(.*)$"
        ))
        .unwrap()
    });
    static STYLE_B: LazyLock<Regex> = LazyLock::new(|| {
        // **RESP** `[01:36]` text   /  **INT** `[00:00+]` text
        Regex::new(&format!(
            r"^\*\*([A-Z][A-Z0-9]{{0,7}})\*\*\s*:?\s*`\[({TS})(\+?)\]`\s*:?\s*(.*)$"
        ))
        .unwrap()
    });
    static STYLE_C: LazyLock<Regex> = LazyLock::new(|| {
        // **[00:17] INT:** text
        Regex::new(&format!(
            r"^\*\*\[({TS})(\+?)\]\s+([A-Z][A-Z0-9]{{0,7}})\s*:?\s*\*\*\s*:?\s*(.*)$"
        ))
        .unwrap()
    });
    static NOTE_RANGE: LazyLock<Regex> = LazyLock::new(|| {
        // **[00:03 to 01:20] [inaudible: ...]**
        Regex::new(&format!(r"^\*{{0,2}}\[({TS})\s+(?:to|-)\s+({TS})\]")).unwrap()
    });
    static EDITORIAL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\*{1,2}\[[^\]]*\]\.?\*{1,2}$").unwrap());

    static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--.*?-->").unwrap());

    let mut turns: Vec<Turn> = Vec::new();
    let mut warnings = Vec::new();
    let mut in_turn = false;
    // Start line of an HTML comment block being skipped (`<!-- internal -->`
    // ... `<!-- /internal -->`, or any multi-line `<!-- ... -->`), and the
    // marker that closes it.
    let mut skipping: Option<(usize, &str)> = None;
    let mut hidden_turns = 0usize;
    for (n, raw_line) in skip_frontmatter(content).iter().enumerate() {
        let line = raw_line.trim();
        if let Some((start, closer)) = skipping {
            if STYLE_A.is_match(line) || STYLE_B.is_match(line) || STYLE_C.is_match(line) {
                hidden_turns += 1;
            }
            if line.contains(closer) {
                let mut w = format!("comment block skipped (body lines {}-{})", start + 1, n + 1);
                if hidden_turns > 0 {
                    w.push_str(&format!(
                        ", it holds {hidden_turns} speaker turn(s) left out of the units"
                    ));
                }
                warnings.push(w);
                skipping = None;
                hidden_turns = 0;
            }
            continue;
        }
        if line == "<!-- internal -->" {
            skipping = Some((n, "<!-- /internal -->"));
            continue;
        }
        if line.starts_with("<!--") && !line.contains("-->") {
            skipping = Some((n, "-->"));
            continue;
        }
        let without_comments = COMMENT.replace_all(line, "");
        let line = without_comments.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') || line == "---" || line == "***" {
            in_turn = false;
            continue;
        }
        let turn = if let Some(c) = STYLE_A.captures(line) {
            Some((c[1].to_string(), c[2].to_string(), c[4].to_string()))
        } else if let Some(c) = STYLE_B.captures(line) {
            Some((c[1].to_string(), c[2].to_string(), c[4].to_string()))
        } else {
            STYLE_C
                .captures(line)
                .map(|c| (c[3].to_string(), c[1].to_string(), c[4].to_string()))
        };
        if let Some((label, ts, text)) = turn {
            turns.push(Turn {
                role: role_from_label(&label),
                speaker_raw: label,
                timestamp: ts,
                text: normalise_ws(&strip_bold(&text)),
            });
            in_turn = true;
            continue;
        }
        if NOTE_RANGE.is_match(line) || (in_turn && EDITORIAL.is_match(line)) {
            warnings.push(format!(
                "note skipped (body line {}): {}",
                n + 1,
                truncate(&strip_bold(line), 80)
            ));
            continue;
        }
        if in_turn {
            if let Some(last) = turns.last_mut() {
                let extra = normalise_ws(&strip_bold(line));
                if last.text.is_empty() {
                    last.text = extra;
                } else {
                    last.text.push(' ');
                    last.text.push_str(&extra);
                }
            }
        }
    }
    if let Some((start, _)) = skipping {
        warnings.push(format!(
            "unclosed comment block from body line {} to the end skipped",
            start + 1
        ));
    }
    let before = turns.len();
    turns.retain(|t| !t.text.is_empty());
    if turns.len() < before {
        warnings.push(format!("{} empty turn(s) dropped", before - turns.len()));
    }
    (turns, warnings)
}

// ---------------------------------------------------------- Budgie markdown

/// Budgie Echo export: `**Speaker 2** — text`, no per-turn timestamp. Every
/// turn carries the last `[mm:ss]` mark seen in the file, or `--:--`.
pub fn parse_budgie_markdown(content: &str) -> Parsed {
    static TURN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\*\*([^*]{1,40}?)\*\*\s*(?:—|–|-|:)\s*(.*)$").unwrap());
    static MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\[({TS})\]")).unwrap());

    let mut turns: Vec<Turn> = Vec::new();
    let mut warnings = Vec::new();
    let mut last_mark = "--:--".to_string();
    let mut in_turn = false;
    for line in skip_frontmatter(content) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if let Some(m) = MARK.captures(line) {
                last_mark = m[1].to_string();
            }
            in_turn = false;
            continue;
        }
        if let Some(c) = TURN.captures(line) {
            if let Some(m) = MARK.captures(&c[2]) {
                last_mark = m[1].to_string();
            }
            push_turn(
                &mut turns,
                Turn {
                    speaker_raw: normalise_ws(&c[1]),
                    role: SpeakerRole::Unknown,
                    timestamp: last_mark.clone(),
                    text: normalise_ws(&strip_bold(&c[2])),
                },
                true,
            );
            in_turn = true;
        } else if in_turn {
            if let Some(last) = turns.last_mut() {
                last.text.push(' ');
                last.text.push_str(&normalise_ws(&strip_bold(line)));
            }
        }
    }
    if last_mark == "--:--" {
        warnings.push("no timestamp in the file; units are stamped --:--".to_string());
    } else {
        warnings.push(format!(
            "Budgie export has no per-turn timestamps; units carry the last mark seen ({last_mark})"
        ));
    }
    (turns, warnings)
}

// ---------------------------------------------------------- written answers

/// Written answer received by e-mail: the body after the first `---` rule that
/// follows the frontmatter; each paragraph is one turn stamped `P01`, `P02`...
pub fn parse_written_response(content: &str) -> Parsed {
    let lines = skip_frontmatter(content);
    let mut warnings = Vec::new();
    let body: Vec<&str> = match lines.iter().position(|l| l.trim() == "---") {
        Some(i) => lines[i + 1..].to_vec(),
        None => {
            warnings.push("no `---` separator after the header; whole file read".to_string());
            lines
        }
    };
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in body.iter().chain(std::iter::once(&"")) {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(normalise_ws(&current.join(" ")));
                current.clear();
            }
        } else {
            current.push(line.trim());
        }
    }
    let turns = paragraphs
        .into_iter()
        .enumerate()
        .map(|(i, text)| Turn {
            speaker_raw: "RESP".to_string(),
            role: SpeakerRole::Respondent,
            timestamp: format!("P{:02}", i + 1),
            text,
        })
        .collect();
    (turns, warnings)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["Dos Santos Alexandre".to_string()]
    }

    #[test]
    fn vtt_merges_cues_and_assigns_roles() {
        let vtt = "WEBVTT\n\nabc/15-0\n00:01:53.169 --> 00:01:53.689\n<v Dos Santos Alexandre>Hello!</v>\n\n\
abc/16-0\n00:01:54.569 --> 00:01:56.649\n<v Roueche C&#233;line >Hey, good\nmorning.</v>\n\n\
abc/16-1\n00:01:56.649 --> 00:01:58.000\n<v Roueche C&#233;line >How are you?</v>\n\n\
abc/17-0\n00:01:59.009 --> 00:02:00.000\n<v Dos Santos Alexandre>Fine.</v>\n";
        let (turns, warnings) = parse_vtt(vtt, &names());
        assert!(warnings.is_empty());
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, SpeakerRole::Interviewer);
        assert_eq!(turns[0].timestamp, "00:01:53");
        assert_eq!(turns[1].speaker_raw, "Roueche Céline");
        assert_eq!(turns[1].role, SpeakerRole::Respondent);
        assert_eq!(turns[1].text, "Hey, good morning. How are you?");
        assert_eq!(turns[1].timestamp, "00:01:54");
    }

    #[test]
    fn vtt_three_speakers_are_unknown_unless_interviewer() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<v A>x</v>\n\n00:00:03.000 --> 00:00:04.000\n<v B>y</v>\n\n00:00:05.000 --> 00:00:06.000\n<v Dos Santos Alexandre>z</v>\n";
        let (turns, _) = parse_vtt(vtt, &names());
        assert_eq!(turns[0].role, SpeakerRole::Unknown);
        assert_eq!(turns[2].role, SpeakerRole::Interviewer);
    }

    #[test]
    fn timestamped_text_one_turn_per_line() {
        let txt = "[00:00:00] S1: All these are relevant.\n[00:00:11] S2: And so yeah.\n[00:00:15] S2: Second line same speaker.\n[00:01:47] S1: So\n";
        let (turns, _) = parse_timestamped_text(txt);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[1].text, "And so yeah. Second line same speaker.");
        assert_eq!(turns[1].timestamp, "00:00:11");
        assert_eq!(turns[2].speaker_raw, "S1");
    }

    #[test]
    fn timestamped_text_bracketed_speakers_on_a_single_line() {
        let txt = "[00:42] [Speaker 1]: Hello? Hello? Hi, Nia. Nice [00:46] [Speaker 3]: to meet you. Nice to meet [00:49] [Speaker 1]: you too. [00:55] [Speaker 1]: Thanks.";
        let (turns, warnings) = parse_timestamped_text(txt);
        assert!(warnings.is_empty());
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].speaker_raw, "Speaker 1");
        assert_eq!(turns[0].text, "Hello? Hello? Hi, Nia. Nice");
        assert_eq!(turns[1].speaker_raw, "Speaker 3");
        assert_eq!(turns[2].text, "you too. Thanks.");
    }

    #[test]
    fn cleaned_markdown_four_styles_and_continuations() {
        let md = "---\ninterview_id: X\n---\n\n# Title\n\n**Processing note.** ignored before first turn.\n\n---\n\n\
**INT `[01:53]`:** Hello!\n\n**RESP `[01:54]`:** Hey, good morning.\n\n\
**RESP** `[01:36]` That's from my side.\n\n**we may transfer all our standards to ISO directly. So there will be no\nproblem.** Then more.\n\n\
*[The question and the answer are both on the recording.]*\n\n\
**INT** `[00:09+]` Great. Inline `[00:49]` mark kept.\n\n\
**REP** `[00:00:03]` Oui.\n\n## 00:00 — Section\n\nStray paragraph after heading.\n\n\
**[00:17] INT:** Right, so.\n\n**[00:03 to 01:20] [inaudible: 13 cues of ASR noise]**\n\n**[00:31] RESP:** Yeah, exactly.\n";
        let (turns, warnings) = parse_cleaned_markdown(md);
        let labels: Vec<_> = turns
            .iter()
            .map(|t| (t.speaker_raw.as_str(), t.timestamp.as_str()))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("INT", "01:53"),
                ("RESP", "01:54"),
                ("RESP", "01:36"),
                ("INT", "00:09"),
                ("REP", "00:00:03"),
                ("INT", "00:17"),
                ("RESP", "00:31"),
            ]
        );
        assert_eq!(
            turns[2].text,
            "That's from my side. we may transfer all our standards to ISO directly. So there will be no problem. Then more."
        );
        assert_eq!(turns[3].text, "Great. Inline `[00:49]` mark kept.");
        assert_eq!(turns[4].role, SpeakerRole::Respondent);
        assert_eq!(turns[5].role, SpeakerRole::Interviewer);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(turns.iter().all(|t| !t.text.contains("Stray")));
    }

    #[test]
    fn cleaned_markdown_skips_internal_and_comment_blocks() {
        let md = "<!-- internal -->\n## Processing notes\n**INT** `[00:01]` not a turn\n<!-- /internal -->\n\n\
**INT** `[00:02]` Hello.\n\n<!-- internal -->\n*[note]* inside a turn\n<!-- /internal -->\n\nStill talking. <!-- inline -->\n\n\
<!--\nmulti\nline\n-->\n\n**RESP** `[00:03]` Bye. <!-- internal -->\n";
        let (turns, warnings) = parse_cleaned_markdown(md);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].timestamp, "00:02");
        assert_eq!(turns[0].text, "Hello. Still talking.");
        assert_eq!(turns[1].text, "Bye.");
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert!(
            warnings[0].contains("holds 1 speaker turn(s)"),
            "{warnings:?}"
        );
    }

    #[test]
    fn budgie_markdown() {
        let md = "# [00:23] [Speaker 1]: So let's first settle on wh…\n\n**Speaker 3** — So let's first settle.\n\n**Speaker 2** — French. So English and\n\n**Speaker 2** — more.\n\n**Speaker 1** — Ok.\n";
        let (turns, warnings) = parse_budgie_markdown(md);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[1].text, "French. So English and more.");
        assert_eq!(turns[0].timestamp, "00:23");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn written_response_paragraphs() {
        let md = "---\nkind: written\n---\n\n# Header\n\nIntro notes.\n\n---\n\nDear Sir,\n\nI was involved\nin the trial.\n\nBest regards,\nF\n";
        let (turns, warnings) = parse_written_response(md);
        assert!(warnings.is_empty());
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[1].text, "I was involved in the trial.");
        assert_eq!(turns[2].timestamp, "P03");
        assert_eq!(turns[0].role, SpeakerRole::Respondent);
    }
}
