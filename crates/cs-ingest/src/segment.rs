//! Deterministic segmentation, same rule as `00_Tools/Scripts/segment_transcript.py`.

use cs_model::{Turn, Unit, MAX_WORDS};

/// Splits a text after `.`, `!`, `?` or `…` when followed by whitespace,
/// mirroring the Python pattern `(?<=[.!?…])\s+`. Empty pieces are dropped.
pub fn split_sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if matches!(c, '.' | '!' | '?' | '…') {
            if let Some(&(j, next)) = chars.peek() {
                if next.is_whitespace() {
                    out.push(&text[start..j]);
                    // Skip the whitespace run.
                    let mut k = j;
                    while let Some(&(idx, ch)) = chars.peek() {
                        if ch.is_whitespace() {
                            chars.next();
                            k = idx + ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                    start = k;
                }
            }
        }
    }
    out.push(&text[start..]);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Splits a turn longer than `max_words` at sentence boundaries, keeping
/// order. A single sentence longer than the limit stays whole.
pub fn split_long(text: &str, max_words: usize) -> Vec<String> {
    if word_count(text) <= max_words {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for sentence in split_sentences(text) {
        let candidate_words = word_count(&current.join(" ")) + word_count(sentence);
        if !current.is_empty() && candidate_words > max_words {
            chunks.push(current.join(" ").trim().to_string());
            current = vec![sentence];
        } else {
            current.push(sentence);
        }
    }
    if !current.is_empty() {
        chunks.push(current.join(" ").trim().to_string());
    }
    chunks
}

/// One unit per turn, turns over `MAX_WORDS` words split at sentence ends.
/// Unit ids are `<interview_id>-U001`, `-U002`... in source order.
pub fn segment(interview_id: &str, turns: &[Turn]) -> Vec<Unit> {
    segment_max(interview_id, turns, MAX_WORDS)
}

/// `segment` with turns over `max_words` words split at sentence ends.
pub fn segment_max(interview_id: &str, turns: &[Turn], max_words: usize) -> Vec<Unit> {
    let mut units = Vec::new();
    for turn in turns {
        let text = turn.text.trim();
        if text.is_empty() {
            continue;
        }
        for chunk in split_long(text, max_words) {
            if chunk.is_empty() {
                continue;
            }
            let seq = units.len() as u32 + 1;
            units.push(Unit {
                unit_id: format!("{interview_id}-U{seq:03}"),
                seq,
                speaker_raw: turn.speaker_raw.clone(),
                role: turn.role,
                timestamp: turn.timestamp.clone(),
                text: chunk,
                corrected: false,
            });
        }
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_model::SpeakerRole;

    fn turn(text: &str) -> Turn {
        Turn {
            speaker_raw: "RESP".into(),
            role: SpeakerRole::Respondent,
            timestamp: "01:00".into(),
            text: text.into(),
        }
    }

    #[test]
    fn sentences_split_like_python() {
        assert_eq!(
            split_sentences("One. Two!  Three? Four… Five 3.5 six"),
            vec!["One.", "Two!", "Three?", "Four…", "Five 3.5 six"]
        );
    }

    #[test]
    fn short_turn_is_one_unit_and_ids_are_padded() {
        let units = segment("INT-16", &[turn("Hello."), turn("  "), turn("World.")]);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].unit_id, "INT-16-U001");
        assert_eq!(units[1].unit_id, "INT-16-U002");
        assert_eq!(units[1].seq, 2);
    }

    #[test]
    fn long_turn_is_split_at_sentence_boundaries() {
        let sentence = "word ".repeat(60).trim().to_string() + "."; // 60 words
        let text = vec![sentence.clone(); 4].join(" "); // 240 words
        let units = segment("X-01", &[turn(&text)]);
        // 60+60 = 120 <= 150, +60 = 180 > 150 -> chunks of 2 sentences.
        assert_eq!(units.len(), 2);
        assert!(units
            .iter()
            .all(|u| u.text.split_whitespace().count() == 120));
        assert_eq!(units[0].timestamp, units[1].timestamp);
        // Nothing lost.
        let rejoined = units
            .iter()
            .map(|u| u.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(rejoined, text);
    }

    #[test]
    fn a_single_overlong_sentence_stays_whole() {
        let text = "word ".repeat(200).trim().to_string();
        let units = segment("X-01", &[turn(&text)]);
        assert_eq!(units.len(), 1);
    }

    #[test]
    fn ids_widen_past_999() {
        let turns: Vec<Turn> = (0..1001).map(|_| turn("a.")).collect();
        let units = segment("X-01", &turns);
        assert_eq!(units[999].unit_id, "X-01-U1000");
    }
}
