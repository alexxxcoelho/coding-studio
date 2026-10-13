//! The lexical index of a corpus: tokens and stems of every unit, TF-IDF
//! vectors of the respondent units, and the queries that read them.
//!
//! The index is a pure value built from the current text of the units. The
//! store keeps one in memory and rebuilds it when a fingerprint of the
//! transcripts changes. Nothing here calls a model.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use serde::Serialize;

use crate::lang::{fold, Lang, Stemmers};
use crate::token::{tokenize, Token};

const NONE: u32 = u32::MAX;

/// A unit of the corpus, as the store gives it (corrections applied).
#[derive(Debug, Clone)]
pub struct CorpusUnit {
    pub interview_id: String,
    pub unit_id: String,
    pub seq: usize,
    /// False for an interviewer turn.
    pub respondent: bool,
    pub text: String,
}

/// What an index is built from. `units` are ordered by interview, then by
/// position; the order of the interviews is the order of first appearance.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub default_lang: Lang,
    /// Interviews whose language differs from the project's.
    pub interview_langs: BTreeMap<String, Lang>,
    pub units: Vec<CorpusUnit>,
}

#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    items: Vec<String>,
}

impl Interner {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(i) = self.ids.get(s) {
            return *i;
        }
        let i = self.items.len() as u32;
        self.items.push(s.to_string());
        self.ids.insert(s.to_string(), i);
        i
    }
    fn get(&self, s: &str) -> Option<u32> {
        self.ids.get(s).copied()
    }
}

struct Tok {
    start: u32,
    end: u32,
    /// Folded word, `NONE` for a number.
    word: u32,
    /// Stem, `NONE` for a number.
    stem: u32,
    /// Counts as a term of the vocabulary: a word that is not a stop word.
    term: bool,
}

struct UnitRec {
    interview: u32,
    unit_id: String,
    seq: usize,
    respondent: bool,
    text: String,
    lang: Lang,
    toks: Vec<Tok>,
    n_words: u32,
}

/// The index.
pub struct LexIndex {
    default_lang: Lang,
    interviews: Vec<String>,
    interview_ix: HashMap<String, u32>,
    interview_langs: BTreeMap<String, Lang>,
    units: Vec<UnitRec>,
    unit_ix: HashMap<(u32, String), usize>,
    words: Interner,
    stems: Interner,
    /// Vector of each respondent unit (index into `units`), weights
    /// normalised, sorted by stem id.
    docs: Vec<(usize, Vec<(u32, f64)>)>,
    doc_of_unit: HashMap<usize, usize>,
    /// Postings per stem id: (index into `docs`, weight).
    postings: Vec<Vec<(u32, f64)>>,
    df: Vec<u32>,
    occ: Vec<u32>,
    n_terms_total: u64,
    stemmers: Mutex<Stemmers>,
}

/// Size of an index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexStats {
    pub n_interviews: usize,
    pub n_units: usize,
    pub n_respondent_units: usize,
    pub n_words: usize,
    pub n_stems: usize,
}

/// Which speakers a search reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speakers {
    Respondent,
    Interviewer,
    All,
}

impl Speakers {
    pub fn parse(s: &str) -> Option<Speakers> {
        match s {
            "respondent" => Some(Speakers::Respondent),
            "interviewer" => Some(Speakers::Interviewer),
            "all" => Some(Speakers::All),
            _ => None,
        }
    }
    fn admits(&self, respondent: bool) -> bool {
        match self {
            Speakers::All => true,
            Speakers::Respondent => respondent,
            Speakers::Interviewer => !respondent,
        }
    }
}

fn round(x: f64, digits: i32) -> f64 {
    let p = 10f64.powi(digits);
    (x * p).round() / p
}

impl LexIndex {
    pub fn build(corpus: &Corpus) -> LexIndex {
        let mut stemmers = Stemmers::default();
        let mut words = Interner::default();
        let mut stems = Interner::default();
        let mut interviews: Vec<String> = Vec::new();
        let mut interview_ix: HashMap<String, u32> = HashMap::new();
        let mut units: Vec<UnitRec> = Vec::with_capacity(corpus.units.len());
        let mut unit_ix = HashMap::new();
        for cu in &corpus.units {
            let iv = *interview_ix
                .entry(cu.interview_id.clone())
                .or_insert_with(|| {
                    interviews.push(cu.interview_id.clone());
                    (interviews.len() - 1) as u32
                });
            let lang = corpus
                .interview_langs
                .get(&cu.interview_id)
                .copied()
                .unwrap_or(corpus.default_lang);
            let tokens = tokenize(&cu.text);
            let mut toks = Vec::with_capacity(tokens.len());
            let mut n_words = 0u32;
            for t in &tokens {
                toks.push(make_tok(t, lang, &mut words, &mut stems, &mut stemmers));
                if t.is_word {
                    n_words += 1;
                }
            }
            unit_ix.insert((iv, cu.unit_id.clone()), units.len());
            units.push(UnitRec {
                interview: iv,
                unit_id: cu.unit_id.clone(),
                seq: cu.seq,
                respondent: cu.respondent,
                text: cu.text.clone(),
                lang,
                toks,
                n_words,
            });
        }
        // TF-IDF over the respondent units.
        let n_stems = stems.items.len();
        let mut df = vec![0u32; n_stems];
        let mut occ = vec![0u32; n_stems];
        let mut n_terms_total = 0u64;
        let mut raw: Vec<(usize, BTreeMap<u32, u32>)> = Vec::new();
        for (ui, u) in units.iter().enumerate() {
            if !u.respondent {
                continue;
            }
            let mut tf: BTreeMap<u32, u32> = BTreeMap::new();
            for t in u.toks.iter().filter(|t| t.term) {
                *tf.entry(t.stem).or_insert(0) += 1;
                occ[t.stem as usize] += 1;
                n_terms_total += 1;
            }
            if tf.is_empty() {
                continue;
            }
            for s in tf.keys() {
                df[*s as usize] += 1;
            }
            raw.push((ui, tf));
        }
        let n_docs = raw.len() as f64;
        let idf = |d: u32| ((1.0 + n_docs) / (1.0 + d as f64)).ln() + 1.0;
        let mut docs = Vec::with_capacity(raw.len());
        let mut postings: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n_stems];
        let mut doc_of_unit = HashMap::new();
        for (ui, tf) in raw {
            let mut vec: Vec<(u32, f64)> = tf
                .into_iter()
                .map(|(s, c)| (s, (1.0 + (c as f64).ln()) * idf(df[s as usize])))
                .collect();
            let norm = vec.iter().map(|(_, w)| w * w).sum::<f64>().sqrt();
            for (_, w) in vec.iter_mut() {
                *w /= norm;
            }
            let di = docs.len() as u32;
            for (s, w) in &vec {
                postings[*s as usize].push((di, *w));
            }
            doc_of_unit.insert(ui, docs.len());
            docs.push((ui, vec));
        }
        LexIndex {
            default_lang: corpus.default_lang,
            interviews,
            interview_ix,
            interview_langs: corpus.interview_langs.clone(),
            units,
            unit_ix,
            words,
            stems,
            docs,
            doc_of_unit,
            postings,
            df,
            occ,
            n_terms_total,
            stemmers: Mutex::new(stemmers),
        }
    }

    pub fn stats(&self) -> IndexStats {
        IndexStats {
            n_interviews: self.interviews.len(),
            n_units: self.units.len(),
            n_respondent_units: self.units.iter().filter(|u| u.respondent).count(),
            n_words: self.units.iter().map(|u| u.n_words as usize).sum(),
            n_stems: self.stems.items.len(),
        }
    }

    pub fn default_lang(&self) -> Lang {
        self.default_lang
    }

    pub fn language_of(&self, interview_id: &str) -> Lang {
        self.interview_langs
            .get(interview_id)
            .copied()
            .unwrap_or(self.default_lang)
    }

    fn stem_of(&self, lang: Lang, lower: &str) -> String {
        self.stemmers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stem(lang, lower)
    }

    // ------------------------------------------------------------ similar

    /// Respondent units whose vocabulary is closest to `query.text`.
    pub fn similar(&self, query: &SimilarQuery) -> Vec<SimilarHit> {
        let lang = query.lang;
        let n_docs = self.docs.len() as f64;
        let idf = |d: u32| ((1.0 + n_docs) / (1.0 + d as f64)).ln() + 1.0;
        // Query vector over the stems of the text.
        let mut tf: BTreeMap<String, u32> = BTreeMap::new();
        let mut forms: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
        for t in tokenize(&query.text) {
            if !is_term_token(&t, lang) {
                continue;
            }
            let stem = self.stem_of(lang, &t.lower);
            *tf.entry(stem.clone()).or_insert(0) += 1;
            *forms.entry(stem).or_default().entry(t.lower).or_insert(0) += 1;
        }
        if tf.is_empty() {
            return Vec::new();
        }
        let mut q: Vec<(Option<u32>, String, f64)> = Vec::new();
        for (stem, c) in &tf {
            let id = self.stems.get(stem);
            let d = id.map(|i| self.df[i as usize]).unwrap_or(0);
            q.push((id, stem.clone(), (1.0 + (*c as f64).ln()) * idf(d)));
        }
        let norm = q.iter().map(|(_, _, w)| w * w).sum::<f64>().sqrt();
        for e in q.iter_mut() {
            e.2 /= norm;
        }
        // Scores through the postings, in the order of the stems (sorted by
        // their string) so that the sums are reproducible.
        let mut scores: BTreeMap<u32, f64> = BTreeMap::new();
        for (id, _, w) in &q {
            let Some(id) = id else { continue };
            for (di, dw) in &self.postings[*id as usize] {
                *scores.entry(*di).or_insert(0.0) += w * dw;
            }
        }
        let mut hits: Vec<(u32, f64)> = scores.into_iter().collect();
        hits.retain(|(di, s)| {
            let u = &self.units[self.docs[*di as usize].0];
            *s >= query.min_score
                && *s > 0.0
                && query.exclude.as_ref().is_none_or(|(i, ux)| {
                    !(u.unit_id == *ux && self.interviews[u.interview as usize] == *i)
                })
                && query
                    .interviews
                    .as_ref()
                    .is_none_or(|set| set.contains(&self.interviews[u.interview as usize]))
        });
        hits.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ua = &self.units[self.docs[a.0 as usize].0];
                    let ub = &self.units[self.docs[b.0 as usize].0];
                    (ua.interview, ua.seq).cmp(&(ub.interview, ub.seq))
                })
        });
        hits.truncate(query.k);
        hits.into_iter()
            .map(|(di, score)| {
                let (ui, dvec) = &self.docs[di as usize];
                let u = &self.units[*ui];
                let mut shared: Vec<SharedTerm> = Vec::new();
                for (id, stem, w) in &q {
                    let Some(id) = id else { continue };
                    if let Ok(pos) = dvec.binary_search_by_key(id, |(s, _)| *s) {
                        let form = forms
                            .get(stem)
                            .and_then(|m| {
                                m.iter()
                                    .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                                    .map(|(f, _)| f.clone())
                            })
                            .unwrap_or_else(|| stem.clone());
                        shared.push(SharedTerm {
                            stem: stem.clone(),
                            form,
                            weight: round(w * dvec[pos].1, 6),
                        });
                    }
                }
                shared.sort_by(|a, b| {
                    b.weight
                        .partial_cmp(&a.weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.stem.cmp(&b.stem))
                });
                shared.truncate(8);
                SimilarHit {
                    interview_id: self.interviews[u.interview as usize].clone(),
                    unit_id: u.unit_id.clone(),
                    seq: u.seq,
                    score: round(score, 6),
                    shared_terms: shared,
                    text: u.text.clone(),
                }
            })
            .collect()
    }

    /// Closest respondent units to a unit of the index (itself excluded).
    pub fn similar_to_unit(
        &self,
        interview_id: &str,
        unit_id: &str,
        k: usize,
        interviews: Option<BTreeSet<String>>,
        min_score: f64,
    ) -> Option<Vec<SimilarHit>> {
        let iv = self.interview_ix.get(interview_id)?;
        let ui = self.unit_ix.get(&(*iv, unit_id.to_string()))?;
        let u = &self.units[*ui];
        Some(self.similar(&SimilarQuery {
            text: u.text.clone(),
            lang: u.lang,
            k,
            exclude: Some((interview_id.to_string(), unit_id.to_string())),
            interviews,
            min_score,
        }))
    }

    // --------------------------------------------------------------- kwic

    /// Every occurrence of a word or phrase, in context.
    pub fn kwic(&self, q: &KwicQuery) -> KwicResult {
        let query_tokens: Vec<Token> = tokenize(&q.query)
            .into_iter()
            .filter(|t| t.is_word)
            .collect();
        let mut result = KwicResult {
            query_tokens: query_tokens.iter().map(|t| t.lower.clone()).collect(),
            stem: q.stem,
            total: 0,
            by_interview: Vec::new(),
            hits: Vec::new(),
        };
        if query_tokens.is_empty() {
            return result;
        }
        // Keys of the query per language: folded word ids, or stem ids.
        let mut keys: HashMap<Lang, Option<Vec<u32>>> = HashMap::new();
        let mut langs: BTreeSet<Lang> = self.units.iter().map(|u| u.lang).collect();
        langs.insert(self.default_lang);
        for lang in langs {
            let ids: Option<Vec<u32>> = query_tokens
                .iter()
                .map(|t| {
                    if q.stem {
                        self.stems.get(&self.stem_of(lang, &t.lower))
                    } else {
                        self.words.get(&fold(&t.lower))
                    }
                })
                .collect();
            keys.insert(lang, ids);
        }
        let n = query_tokens.len();
        let mut per_interview: BTreeMap<u32, usize> = BTreeMap::new();
        let mut seen = 0usize;
        for u in &self.units {
            if !q.speakers.admits(u.respondent) {
                continue;
            }
            let interview_id = &self.interviews[u.interview as usize];
            if q.interviews
                .as_ref()
                .is_some_and(|set| !set.contains(interview_id))
            {
                continue;
            }
            let Some(Some(key)) = keys.get(&u.lang) else {
                continue;
            };
            if u.toks.len() < n {
                continue;
            }
            for i in 0..=(u.toks.len() - n) {
                let hit = (0..n).all(|k| {
                    let t = &u.toks[i + k];
                    let id = if q.stem { t.stem } else { t.word };
                    id != NONE && id == key[k]
                });
                if !hit {
                    continue;
                }
                result.total += 1;
                *per_interview.entry(u.interview).or_insert(0) += 1;
                if seen >= q.offset && result.hits.len() < q.limit {
                    let start = u.toks[i].start as usize;
                    let end = u.toks[i + n - 1].end as usize;
                    result.hits.push(KwicHit {
                        interview_id: interview_id.clone(),
                        unit_id: u.unit_id.clone(),
                        seq: u.seq,
                        role: if u.respondent {
                            "respondent"
                        } else {
                            "interviewer"
                        },
                        start: u.text[..start].chars().count(),
                        end: u.text[..end].chars().count(),
                        left: left_context(&u.text, start, q.width),
                        matched: u.text[start..end].to_string(),
                        right: right_context(&u.text, end, q.width),
                    });
                }
                seen += 1;
            }
        }
        result.by_interview = per_interview
            .into_iter()
            .map(|(i, n)| (self.interviews[i as usize].clone(), n))
            .collect();
        result
    }

    // ------------------------------------------------------ keyword table

    /// The terms of a set of passages (highlights), with their counts and how
    /// much more frequent they are there than in the respondent text of the
    /// corpus (log-likelihood, G2; negative when rarer).
    pub fn keyword_table(&self, passages: &[PassageText], opts: &KeywordOptions) -> KeywordTable {
        struct Acc {
            forms: BTreeMap<String, u32>,
            passages: BTreeSet<String>,
            interviews: BTreeSet<String>,
            occ: u32,
        }
        let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
        let mut h_total = 0u64;
        for p in passages {
            let lang = self.language_of(&p.interview_id);
            for t in tokenize(&p.text) {
                if !is_term_token(&t, lang) {
                    continue;
                }
                let stem = self.stem_of(lang, &t.lower);
                h_total += 1;
                let a = acc.entry(stem).or_insert_with(|| Acc {
                    forms: BTreeMap::new(),
                    passages: BTreeSet::new(),
                    interviews: BTreeSet::new(),
                    occ: 0,
                });
                *a.forms.entry(t.lower).or_insert(0) += 1;
                a.passages.insert(p.passage_id.clone());
                a.interviews.insert(p.interview_id.clone());
                a.occ += 1;
            }
        }
        let n1 = h_total as f64;
        let n2 = self.n_terms_total.saturating_sub(h_total) as f64;
        let mut rows: Vec<KeywordRow> = acc
            .into_iter()
            .filter(|(_, a)| a.passages.len() >= opts.min_passages.max(1))
            .map(|(stem, a)| {
                let corpus_occ = self
                    .stems
                    .get(&stem)
                    .map(|i| self.occ[i as usize])
                    .unwrap_or(0) as f64;
                let x = a.occ as f64;
                let y = (corpus_occ - x).max(0.0);
                let keyness = if n2 <= 0.0 || n1 <= 0.0 {
                    0.0
                } else {
                    let e1 = n1 * (x + y) / (n1 + n2);
                    let e2 = n2 * (x + y) / (n1 + n2);
                    let mut g = 0.0;
                    if x > 0.0 {
                        g += x * (x / e1).ln();
                    }
                    if y > 0.0 {
                        g += y * (y / e2).ln();
                    }
                    let g2 = 2.0 * g;
                    if x / n1 >= y / n2 {
                        g2
                    } else {
                        -g2
                    }
                };
                let mut forms: Vec<(String, u32)> = a.forms.into_iter().collect();
                forms.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
                KeywordRow {
                    stem,
                    forms: forms.into_iter().take(5).map(|f| f.0).collect(),
                    n_passages: a.passages.len(),
                    n_interviews: a.interviews.len(),
                    interviews: a.interviews.into_iter().collect(),
                    passage_ids: a.passages.into_iter().collect(),
                    n_occurrences: a.occ as usize,
                    corpus_occurrences: corpus_occ as usize,
                    keyness: round(keyness, 4),
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            b.keyness
                .partial_cmp(&a.keyness)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.n_passages.cmp(&a.n_passages))
                .then_with(|| a.stem.cmp(&b.stem))
        });
        let n_rows = rows.len();
        rows.truncate(opts.limit);
        KeywordTable {
            n_passages: passages.len(),
            n_terms_in_passages: h_total as usize,
            n_rows,
            rows,
        }
    }

    // ----------------------------------------------------- term frequency

    /// Occurrences of words per value of an interview attribute. `groups`
    /// maps an interview to its value (absent or `None`: not filled).
    pub fn term_frequency(&self, req: &TermFreqRequest) -> Vec<TermFreqRow> {
        let mut out = Vec::new();
        // Words and units per group, once.
        let value_of = |iv: &str| -> Option<String> { req.groups.get(iv).cloned().flatten() };
        let mut group_words: BTreeMap<Option<String>, u64> = BTreeMap::new();
        let mut group_interviews: BTreeMap<Option<String>, BTreeSet<u32>> = BTreeMap::new();
        for u in &self.units {
            if !req.speakers.admits(u.respondent) {
                continue;
            }
            let v = value_of(&self.interviews[u.interview as usize]);
            *group_words.entry(v.clone()).or_insert(0) += u.n_words as u64;
            group_interviews.entry(v).or_default().insert(u.interview);
        }
        for term in &req.terms {
            let Some(tok) = tokenize(term).into_iter().find(|t| t.is_word) else {
                continue;
            };
            struct Cnt {
                occ: u64,
                units: u64,
                interviews: BTreeSet<u32>,
            }
            let mut counts: BTreeMap<Option<String>, Cnt> = BTreeMap::new();
            let mut key_by_lang: HashMap<Lang, Option<u32>> = HashMap::new();
            for u in &self.units {
                if !req.speakers.admits(u.respondent) {
                    continue;
                }
                let key = *key_by_lang.entry(u.lang).or_insert_with(|| {
                    if req.stem {
                        self.stems.get(&self.stem_of(u.lang, &tok.lower))
                    } else {
                        self.words.get(&fold(&tok.lower))
                    }
                });
                let Some(key) = key else { continue };
                let n = u
                    .toks
                    .iter()
                    .filter(|t| (if req.stem { t.stem } else { t.word }) == key)
                    .count() as u64;
                if n == 0 {
                    continue;
                }
                let v = value_of(&self.interviews[u.interview as usize]);
                let c = counts.entry(v).or_insert(Cnt {
                    occ: 0,
                    units: 0,
                    interviews: BTreeSet::new(),
                });
                c.occ += n;
                c.units += 1;
                c.interviews.insert(u.interview);
            }
            let mut by_value: Vec<TermFreqValue> = group_words
                .iter()
                .map(|(v, words)| {
                    let c = counts.get(v);
                    let occ = c.map(|c| c.occ).unwrap_or(0);
                    TermFreqValue {
                        value: v.clone(),
                        n_interviews: group_interviews.get(v).map(|s| s.len()).unwrap_or(0),
                        n_interviews_with_term: c.map(|c| c.interviews.len()).unwrap_or(0),
                        n_units_with_term: c.map(|c| c.units as usize).unwrap_or(0),
                        n_occurrences: occ as usize,
                        n_words: *words as usize,
                        per_1000_words: if *words == 0 {
                            0.0
                        } else {
                            round(occ as f64 * 1000.0 / *words as f64, 3)
                        },
                    }
                })
                .collect();
            by_value.sort_by(|a, b| match (&a.value, &b.value) {
                (Some(x), Some(y)) => x.cmp(y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
            out.push(TermFreqRow {
                term: term.clone(),
                by_value,
            });
        }
        out
    }

    /// The `n` most frequent terms of each value of an attribute.
    pub fn top_terms(
        &self,
        groups: &BTreeMap<String, Option<String>>,
        speakers: Speakers,
        n: usize,
    ) -> Vec<TopTermsRow> {
        // (occurrences, interviews, last interview seen) per stem. Units of an
        // interview are contiguous in the index, so a change of interview
        // since the last occurrence marks a new interview.
        type Acc = HashMap<u32, (u64, usize, u32)>;
        let mut per: BTreeMap<Option<String>, Acc> = BTreeMap::new();
        for u in &self.units {
            if !speakers.admits(u.respondent) {
                continue;
            }
            let v = groups
                .get(&self.interviews[u.interview as usize])
                .cloned()
                .flatten();
            let m = per.entry(v).or_default();
            for t in u.toks.iter().filter(|t| t.term) {
                let e = m.entry(t.stem).or_insert((0, 0, NONE));
                e.0 += 1;
                if e.2 != u.interview {
                    e.1 += 1;
                    e.2 = u.interview;
                }
            }
        }
        let mut chosen: BTreeMap<Option<String>, Vec<(u32, u64, usize)>> = BTreeMap::new();
        for (value, m) in per {
            let mut items: Vec<(u32, u64, usize)> =
                m.into_iter().map(|(s, (c, ni, _))| (s, c, ni)).collect();
            items.sort_by(|a, b| {
                b.1.cmp(&a.1).then_with(|| {
                    self.stems.items[a.0 as usize].cmp(&self.stems.items[b.0 as usize])
                })
            });
            items.truncate(n);
            chosen.insert(value, items);
        }
        // Most frequent written form of each chosen stem, in one more pass.
        let wanted: BTreeSet<u32> = chosen
            .values()
            .flat_map(|v| v.iter().map(|i| i.0))
            .collect();
        let mut forms: HashMap<u32, HashMap<u32, u32>> = HashMap::new();
        for u in &self.units {
            if !speakers.admits(u.respondent) {
                continue;
            }
            for t in u.toks.iter().filter(|t| t.term && wanted.contains(&t.stem)) {
                *forms.entry(t.stem).or_default().entry(t.word).or_insert(0) += 1;
            }
        }
        let mut out: Vec<TopTermsRow> = chosen
            .into_iter()
            .map(|(value, items)| TopTermsRow {
                value,
                terms: items
                    .into_iter()
                    .map(|(s, c, ni)| TopTerm {
                        stem: self.stems.items[s as usize].clone(),
                        form: forms
                            .get(&s)
                            .and_then(|f| {
                                f.iter()
                                    .max_by(|a, b| {
                                        a.1.cmp(b.1).then_with(|| {
                                            self.words.items[*b.0 as usize]
                                                .cmp(&self.words.items[*a.0 as usize])
                                        })
                                    })
                                    .map(|(w, _)| self.words.items[*w as usize].clone())
                            })
                            .unwrap_or_default(),
                        n_occurrences: c as usize,
                        n_interviews: ni,
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| match (&a.value, &b.value) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        out
    }

    /// Text of a unit of the index.
    pub fn unit_text(&self, interview_id: &str, unit_id: &str) -> Option<&str> {
        let iv = self.interview_ix.get(interview_id)?;
        let ui = self.unit_ix.get(&(*iv, unit_id.to_string()))?;
        Some(&self.units[*ui].text)
    }

    /// True when the unit is a respondent unit that has a TF-IDF vector.
    pub fn has_vector(&self, interview_id: &str, unit_id: &str) -> bool {
        self.interview_ix
            .get(interview_id)
            .and_then(|iv| self.unit_ix.get(&(*iv, unit_id.to_string())))
            .is_some_and(|ui| self.doc_of_unit.contains_key(ui))
    }
}

fn is_term_token(t: &Token, lang: Lang) -> bool {
    t.is_word && t.lower.chars().count() >= 2 && !lang.is_stop(&fold(&t.lower))
}

fn make_tok(
    t: &Token,
    lang: Lang,
    words: &mut Interner,
    stems: &mut Interner,
    stemmers: &mut Stemmers,
) -> Tok {
    if !t.is_word {
        return Tok {
            start: t.start as u32,
            end: t.end as u32,
            word: NONE,
            stem: NONE,
            term: false,
        };
    }
    let word = words.intern(&fold(&t.lower));
    let stem = stems.intern(&stemmers.stem(lang, &t.lower));
    Tok {
        start: t.start as u32,
        end: t.end as u32,
        word,
        stem,
        term: is_term_token(t, lang),
    }
}

fn left_context(text: &str, start: usize, width: usize) -> String {
    let before = &text[..start];
    let n = before.chars().count();
    let skip = n.saturating_sub(width);
    let s: String = before.chars().skip(skip).collect();
    s.trim_start().to_string()
}

fn right_context(text: &str, end: usize, width: usize) -> String {
    let s: String = text[end..].chars().take(width).collect();
    s.trim_end().to_string()
}

// ------------------------------------------------------------------ types

#[derive(Debug, Clone)]
pub struct SimilarQuery {
    pub text: String,
    pub lang: Lang,
    pub k: usize,
    /// (interview_id, unit_id) left out, normally the unit of the query.
    pub exclude: Option<(String, String)>,
    pub interviews: Option<BTreeSet<String>>,
    pub min_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SharedTerm {
    pub stem: String,
    /// The most frequent surface form of the stem in the query text.
    pub form: String,
    /// Contribution of the term to the score.
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SimilarHit {
    pub interview_id: String,
    pub unit_id: String,
    pub seq: usize,
    pub score: f64,
    pub shared_terms: Vec<SharedTerm>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct KwicQuery {
    pub query: String,
    pub stem: bool,
    /// Characters of context on each side.
    pub width: usize,
    pub interviews: Option<BTreeSet<String>>,
    pub speakers: Speakers,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KwicHit {
    pub interview_id: String,
    pub unit_id: String,
    pub seq: usize,
    pub role: &'static str,
    /// Unicode scalar positions of the match in the unit text, end excluded.
    pub start: usize,
    pub end: usize,
    pub left: String,
    #[serde(rename = "match")]
    pub matched: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KwicResult {
    pub query_tokens: Vec<String>,
    pub stem: bool,
    /// Occurrences found, whatever `limit` and `offset`.
    pub total: usize,
    pub by_interview: Vec<(String, usize)>,
    pub hits: Vec<KwicHit>,
}

#[derive(Debug, Clone)]
pub struct PassageText {
    pub passage_id: String,
    pub interview_id: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct KeywordOptions {
    pub min_passages: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeywordRow {
    pub stem: String,
    pub forms: Vec<String>,
    pub n_passages: usize,
    pub n_interviews: usize,
    pub interviews: Vec<String>,
    pub passage_ids: Vec<String>,
    pub n_occurrences: usize,
    pub corpus_occurrences: usize,
    pub keyness: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeywordTable {
    pub n_passages: usize,
    pub n_terms_in_passages: usize,
    /// Rows before `limit`.
    pub n_rows: usize,
    pub rows: Vec<KeywordRow>,
}

#[derive(Debug, Clone)]
pub struct TermFreqRequest {
    pub terms: Vec<String>,
    pub stem: bool,
    pub groups: BTreeMap<String, Option<String>>,
    pub speakers: Speakers,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TermFreqValue {
    /// Value of the attribute, `None` for the interviews where it is empty.
    pub value: Option<String>,
    pub n_interviews: usize,
    pub n_interviews_with_term: usize,
    pub n_units_with_term: usize,
    pub n_occurrences: usize,
    pub n_words: usize,
    pub per_1000_words: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TermFreqRow {
    pub term: String,
    pub by_value: Vec<TermFreqValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopTerm {
    pub stem: String,
    pub form: String,
    pub n_occurrences: usize,
    pub n_interviews: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopTermsRow {
    pub value: Option<String>,
    pub terms: Vec<TopTerm>,
}
