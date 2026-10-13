//! Accent-insensitive, case-insensitive text matching shared by the code
//! search (F-41) and the full-text search (F-55).
//!
//! A text is folded character by character (`cs_lex::fold`), keeping for each
//! folded character the position of the Unicode scalar it came from, so that
//! a match found on the folded text can be shown on the original one.

/// Folded characters and, for each, the scalar index in the source text.
pub(crate) struct Folded {
    pub chars: Vec<char>,
    pub src: Vec<usize>,
}

pub(crate) fn fold_text(s: &str) -> Folded {
    let mut chars = Vec::with_capacity(s.len());
    let mut src = Vec::with_capacity(s.len());
    let mut buf = [0u8; 4];
    for (i, c) in s.chars().enumerate() {
        let one: &str = c.encode_utf8(&mut buf);
        for f in cs_lex::fold(one).chars() {
            chars.push(f);
            src.push(i);
        }
    }
    Folded { chars, src }
}

/// A character that belongs to a word.
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// Ranges `[start, end)` of the words of a folded text (indexes into
/// `Folded::chars`).
pub(crate) fn word_ranges(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in chars.iter().enumerate() {
        match (is_word_char(*c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, chars.len()));
    }
    out
}

/// The folded words of a string.
pub(crate) fn folded_words(s: &str) -> Vec<String> {
    let f = fold_text(s);
    word_ranges(&f.chars)
        .into_iter()
        .map(|(a, b)| f.chars[a..b].iter().collect())
        .collect()
}
