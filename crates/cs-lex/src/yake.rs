//! YAKE keyphrase extraction (Campos et al., 2020), reimplemented here so that
//! the result is deterministic and uses the tokeniser and the stop words of
//! the workbench.
//!
//! The score of a term combines five statistical features computed on the
//! text alone: casing, position of its sentences, frequency, relatedness to
//! the words around it and spread over the sentences. The score of a phrase is
//! the product of the scores of its terms divided by its frequency and by one
//! plus their sum. A LOWER score is a better keyphrase. No training and no
//! model are involved.
//!
//! Simplifications, documented in `docs/DEVGRAPH_v1_lots3-4.md`: terms of one
//! or two letters are ignored unless they are acronyms; a phrase may hold one
//! stop word between two terms ("prise de décision") that does not count in
//! its score; duplicates are removed with a Levenshtein ratio of 0.9.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::lang::{fold, Lang};
use crate::token::{tokenize, Gap, Token};

/// One keyphrase with its YAKE score (lower is better).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Keyphrase {
    pub phrase: String,
    pub score: f64,
    /// Occurrences of the phrase in the text.
    pub count: usize,
}

#[derive(Default)]
struct TermStats {
    tf: usize,
    tf_upper: usize,
    tf_acronym: usize,
    sentences: Vec<usize>,
    left: Vec<String>,
    right: Vec<String>,
}

/// Window, in words, on each side of a term for the relatedness feature.
const WINDOW: usize = 2;

fn is_term(t: &Token, lang: Lang) -> bool {
    if !t.is_word {
        return false;
    }
    let n = t.lower.chars().count();
    if n < 3 && !(t.acronym && n >= 2) {
        return false;
    }
    !lang.is_stop(&fold(&t.lower))
}

fn median(sorted: &[usize]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        sorted[n / 2] as f64
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn similar(a: &str, b: &str) -> bool {
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        return true;
    }
    1.0 - levenshtein(a, b) as f64 / longest as f64 >= 0.9
}

/// The `top` best keyphrases of `text`, phrases of up to `max_ngram` terms.
pub fn extract(text: &str, lang: Lang, max_ngram: usize, top: usize) -> Vec<Keyphrase> {
    let tokens = tokenize(text);
    if tokens.is_empty() || top == 0 {
        return Vec::new();
    }
    let max_ngram = max_ngram.clamp(1, 5);
    let term: Vec<bool> = tokens.iter().map(|t| is_term(t, lang)).collect();

    // Sentence index of each token.
    let mut sentence = Vec::with_capacity(tokens.len());
    let mut s = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 && t.gap == Gap::Sentence {
            s += 1;
        }
        sentence.push(s);
    }
    let n_sentences = s + 1;

    // Statistics per term.
    let mut stats: BTreeMap<&str, TermStats> = BTreeMap::new();
    for (i, t) in tokens.iter().enumerate() {
        if !term[i] {
            continue;
        }
        let st = stats.entry(t.lower.as_str()).or_default();
        st.tf += 1;
        let starts_sentence = i == 0 || t.gap == Gap::Sentence;
        if t.acronym {
            st.tf_acronym += 1;
        } else if t.capital && !starts_sentence {
            st.tf_upper += 1;
        }
        st.sentences.push(sentence[i]);
        // Left and right neighbours among the terms, inside the clause.
        let mut seen = 0;
        let mut k = i;
        while k > 0 && seen < WINDOW {
            if tokens[k].gap >= Gap::Clause {
                break;
            }
            k -= 1;
            if term[k] {
                st.left.push(tokens[k].lower.clone());
            }
            seen += 1;
        }
        let mut seen = 0;
        let mut k = i;
        while k + 1 < tokens.len() && seen < WINDOW {
            if tokens[k + 1].gap >= Gap::Clause {
                break;
            }
            k += 1;
            if term[k] {
                st.right.push(tokens[k].lower.clone());
            }
            seen += 1;
        }
    }
    if stats.is_empty() {
        return Vec::new();
    }
    let tfs: Vec<f64> = stats.values().map(|s| s.tf as f64).collect();
    let mean = tfs.iter().sum::<f64>() / tfs.len() as f64;
    let var = tfs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / tfs.len() as f64;
    let sigma = var.sqrt();
    let max_tf = tfs.iter().cloned().fold(1.0, f64::max);

    let mut score: BTreeMap<&str, f64> = BTreeMap::new();
    for (word, st) in &stats {
        let tf = st.tf as f64;
        let w_case = st.tf_upper.max(st.tf_acronym) as f64 / (1.0 + tf.ln());
        let mut sents = st.sentences.clone();
        sents.sort_unstable();
        let w_pos = (3.0 + median(&sents)).ln().ln();
        let w_freq = tf / (mean + sigma);
        let rel_side = |side: &Vec<String>| -> (f64, f64) {
            if side.is_empty() {
                return (0.0, 0.0);
            }
            let mut distinct = side.clone();
            distinct.sort();
            distinct.dedup();
            (
                distinct.len() as f64 / side.len() as f64,
                distinct.len() as f64 / max_tf,
            )
        };
        let (wl, pl) = rel_side(&st.left);
        let (wr, pr) = rel_side(&st.right);
        let w_rel = 1.0 + (wl + wr) * (tf / max_tf) + pl + pr;
        let mut distinct_sents = sents.clone();
        distinct_sents.dedup();
        let w_diff = distinct_sents.len() as f64 / n_sentences as f64;
        let s = (w_rel * w_pos) / (w_case + w_freq / w_rel + w_diff / w_rel);
        score.insert(word, s);
    }

    // Candidates: runs of words inside one clause, from a term to a term,
    // with at most one non-term word in the middle.
    let mut cands: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    for i in 0..tokens.len() {
        if !term[i] {
            continue;
        }
        let mut sum = 0.0;
        let mut prod = 1.0;
        let mut n_terms = 0usize;
        let mut inner_stops = 0usize;
        let mut words: Vec<&str> = Vec::new();
        for j in i..tokens.len() {
            if j > i && tokens[j].gap >= Gap::Clause {
                break;
            }
            if !tokens[j].is_word {
                break;
            }
            words.push(tokens[j].lower.as_str());
            if term[j] {
                let sc = score[tokens[j].lower.as_str()];
                sum += sc;
                prod *= sc;
                n_terms += 1;
                let key = words.join(" ");
                let e = cands.entry(key).or_insert((prod, sum, 0));
                e.0 = prod;
                e.1 = sum;
                e.2 += 1;
                if n_terms >= max_ngram {
                    break;
                }
            } else {
                inner_stops += 1;
                if inner_stops > 1 {
                    break;
                }
            }
        }
    }
    let mut ranked: Vec<Keyphrase> = cands
        .into_iter()
        .map(|(phrase, (prod, sum, count))| Keyphrase {
            score: prod / (count as f64 * (1.0 + sum)),
            phrase,
            count,
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.phrase.cmp(&b.phrase))
    });
    let mut out: Vec<Keyphrase> = Vec::new();
    for c in ranked {
        if out.iter().any(|k| similar(&k.phrase, &c.phrase)) {
            continue;
        }
        out.push(Keyphrase {
            score: (c.score * 1e8).round() / 1e8,
            ..c
        });
        if out.len() >= top {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "Le coût de l'étiquetage RFID reste le frein principal. \
Nos fournisseurs refusent d'absorber le coût de l'étiquetage. \
Sans accord sur le coût, le projet d'étiquetage ne démarre pas.";

    #[test]
    fn frequent_content_words_rank_first() {
        let k = extract(TEXT, Lang::Fr, 3, 5);
        assert!(!k.is_empty());
        let phrases: Vec<&str> = k.iter().map(|k| k.phrase.as_str()).collect();
        assert!(
            phrases
                .iter()
                .any(|p| p.contains("coût") || p.contains("étiquetage")),
            "{phrases:?}"
        );
        assert!(!phrases.contains(&"le"));
    }

    #[test]
    fn scores_ascend_and_are_deterministic() {
        let a = extract(TEXT, Lang::Fr, 3, 8);
        let b = extract(TEXT, Lang::Fr, 3, 8);
        assert_eq!(a, b);
        assert!(a.windows(2).all(|w| w[0].score <= w[1].score));
    }

    #[test]
    fn phrases_do_not_cross_clauses_or_sentences() {
        let k = extract(
            "Prix élevé, délai long. Fournisseur absent.",
            Lang::Fr,
            3,
            20,
        );
        assert!(k.iter().all(|k| k.phrase != "élevé délai"));
        assert!(k.iter().all(|k| k.phrase != "long fournisseur"));
    }

    #[test]
    fn empty_and_stop_only_texts_give_nothing() {
        assert!(extract("", Lang::Fr, 3, 5).is_empty());
        assert!(extract("le la les de du", Lang::Fr, 3, 5).is_empty());
    }

    #[test]
    fn english_text_works_too() {
        let k = extract(
            "Supplier tagging costs too much. The cost of tagging blocks the project.",
            Lang::En,
            2,
            3,
        );
        assert!(k.iter().any(|k| k.phrase.contains("tagging")));
    }
}
