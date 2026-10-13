//! Lexical workbench of Coding Studio: tokenisation and normalisation for
//! French, English and German, stop words, Snowball stems, YAKE keyphrases,
//! TF-IDF similarity of respondent units, keyword in context, a keyword table
//! across passages and term frequencies per attribute value.
//!
//! Everything is deterministic and local: no generative model, no network. The
//! functions are pure; `cs-store` owns the corpus and the cache of the index.

mod index;
mod lang;
mod token;
mod yake;

pub use index::{
    Corpus, CorpusUnit, IndexStats, KeywordOptions, KeywordRow, KeywordTable, KwicHit, KwicQuery,
    KwicResult, LexIndex, PassageText, SharedTerm, SimilarHit, SimilarQuery, Speakers,
    TermFreqRequest, TermFreqRow, TermFreqValue, TopTerm, TopTermsRow,
};
pub use lang::{fold, Lang, Stemmers};
pub use token::{tokenize, Gap, Token};
pub use yake::{extract as extract_keyphrases, Keyphrase};

use std::collections::BTreeMap;

use serde::Serialize;

/// A stem of a text with the forms it was written in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StemRow {
    pub stem: String,
    pub forms: Vec<String>,
    pub count: usize,
}

/// What the lexical panel shows for a selected passage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LexicalPanel {
    pub language: Lang,
    pub keyphrases: Vec<Keyphrase>,
    pub stems: Vec<StemRow>,
    /// A provisional label that the researcher may use or ignore.
    pub label: Option<String>,
    pub provisional: bool,
    pub note: String,
}

/// Note shown with every similarity result.
pub const SIMILARITY_NOTE: &str = "La proximité se calcule sur les mots partagés (racines). Elle ne voit ni les synonymes ni les autres langues.";

/// Note shown with the lexical panel.
pub const PANEL_NOTE: &str = "Calcul statistique sur ce seul texte. Le libellé est provisoire : il peut être repris ou ignoré.";

/// Keyphrases, stems and a provisional label for `text`.
pub fn analyse_text(text: &str, lang: Lang) -> LexicalPanel {
    let keyphrases = extract_keyphrases(text, lang, 3, 8);
    let mut stemmers = Stemmers::default();
    let mut by_stem: BTreeMap<String, (usize, BTreeMap<String, usize>)> = BTreeMap::new();
    for t in tokenize(text) {
        if !t.is_word || t.lower.chars().count() < 2 || lang.is_stop(&fold(&t.lower)) {
            continue;
        }
        let stem = stemmers.stem(lang, &t.lower);
        let e = by_stem.entry(stem).or_default();
        e.0 += 1;
        *e.1.entry(t.lower).or_insert(0) += 1;
    }
    let mut stems: Vec<StemRow> = by_stem
        .into_iter()
        .map(|(stem, (count, forms))| {
            let mut f: Vec<(String, usize)> = forms.into_iter().collect();
            f.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            StemRow {
                stem,
                forms: f.into_iter().map(|x| x.0).collect(),
                count,
            }
        })
        .collect();
    stems.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.stem.cmp(&b.stem)));
    stems.truncate(20);
    let label = keyphrases.first().map(|k| {
        let mut chars = k.phrase.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    });
    LexicalPanel {
        language: lang,
        keyphrases,
        stems,
        label,
        provisional: true,
        note: PANEL_NOTE.to_string(),
    }
}

#[cfg(test)]
mod tests;
