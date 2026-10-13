//! Tokenisation of French, English and German interview text.
//!
//! A token is a run of letters and digits. Hyphens split words ("peut-être"
//! gives two tokens). An apostrophe splits after a French elision ("l'outil"
//! gives `l` and `outil`) and stays inside other words ("aujourd'hui",
//! "don't"). Positions are byte offsets into the text; callers convert them to
//! Unicode scalar positions when they publish them.

/// Boundary found between a token and the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gap {
    /// Only blanks, hyphens or elision apostrophes.
    Soft,
    /// A comma, a bracket, a quotation mark, a dash: a phrase cannot go
    /// across it.
    Clause,
    /// A full stop, a question or exclamation mark, a line break (or the start
    /// of the text).
    Sentence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Byte range in the text.
    pub start: usize,
    pub end: usize,
    /// Lower-case form, apostrophes normalised to `'`.
    pub lower: String,
    /// Starts with a capital letter.
    pub capital: bool,
    /// Two letters or more, all capitals (an acronym).
    pub acronym: bool,
    /// Contains at least one letter (numbers are not words).
    pub is_word: bool,
    /// Boundary before the token.
    pub gap: Gap,
}

/// Prefixes of the French elision: `l'`, `d'`, `qu'`...
const ELISIONS: [&str; 13] = [
    "l", "d", "j", "m", "t", "s", "n", "c", "qu", "jusqu", "lorsqu", "puisqu", "quoiqu",
];

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '\u{02bc}')
}

fn gap_of(c: char) -> Gap {
    match c {
        '.' | '!' | '?' | '\u{2026}' | '\n' | '\r' => Gap::Sentence,
        ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\u{ab}' | '\u{bb}'
        | '\u{201c}' | '\u{201d}' | '\u{2014}' | '\u{2013}' | '/' | '|' | '*' | '=' | '<' | '>' => {
            Gap::Clause
        }
        _ => Gap::Soft,
    }
}

fn normalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_apostrophe(c) {
            out.push('\'');
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Splits `text` into tokens, in reading order.
pub fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut pending = Gap::Sentence;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c.is_alphanumeric() {
            let start = pos;
            let mut j = i;
            while j < chars.len() {
                let cj = chars[j].1;
                if cj.is_alphanumeric() {
                    j += 1;
                } else if is_apostrophe(cj)
                    && j > i
                    && chars.get(j + 1).is_some_and(|n| n.1.is_alphabetic())
                {
                    let so_far = normalise(&text[start..chars[j].0]);
                    if ELISIONS.contains(&so_far.as_str()) {
                        break;
                    }
                    j += 1;
                } else {
                    break;
                }
            }
            let end = chars.get(j).map(|x| x.0).unwrap_or(text.len());
            let raw = &text[start..end];
            let mut letters = raw.chars().filter(|c| c.is_alphabetic());
            let first = raw.chars().next();
            let n_letters = letters.clone().count();
            let acronym = n_letters >= 2 && letters.all(|c| c.is_uppercase());
            tokens.push(Token {
                start,
                end,
                lower: normalise(raw),
                capital: first.is_some_and(|c| c.is_uppercase()),
                acronym,
                is_word: n_letters > 0,
                gap: pending,
            });
            pending = Gap::Soft;
            i = j;
        } else {
            let g = gap_of(c);
            if g > pending {
                pending = g;
            }
            i += 1;
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowers(text: &str) -> Vec<String> {
        tokenize(text).into_iter().map(|t| t.lower).collect()
    }

    #[test]
    fn splits_french_elisions_and_hyphens() {
        assert_eq!(
            lowers("L'outil d'étiquetage, c'est peut-être cher."),
            [
                "l",
                "outil",
                "d",
                "étiquetage",
                "c",
                "est",
                "peut",
                "être",
                "cher"
            ]
        );
    }

    #[test]
    fn keeps_inner_apostrophes() {
        assert_eq!(
            lowers("Aujourd’hui, we don't know."),
            ["aujourd'hui", "we", "don't", "know"]
        );
    }

    #[test]
    fn offsets_are_byte_ranges_of_the_text() {
        let text = "Économie réelle";
        let t = tokenize(text);
        assert_eq!(&text[t[0].start..t[0].end], "Économie");
        assert_eq!(&text[t[1].start..t[1].end], "réelle");
        assert!(t[0].capital && !t[1].capital);
    }

    #[test]
    fn gaps_tell_sentences_from_clauses() {
        let t = tokenize("Un prix, un coût. Fin");
        assert_eq!(t[0].gap, Gap::Sentence);
        assert_eq!(t[1].gap, Gap::Soft);
        assert_eq!(t[2].gap, Gap::Clause);
        assert_eq!(t[4].gap, Gap::Sentence);
    }

    #[test]
    fn numbers_are_not_words_and_acronyms_are_flagged() {
        let t = tokenize("Le RFID coûte 2500 francs");
        assert!(t[1].acronym);
        assert!(!t[3].is_word);
    }
}
