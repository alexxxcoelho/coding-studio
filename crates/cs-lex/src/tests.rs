use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use crate::*;

fn unit(iv: &str, n: usize, respondent: bool, text: &str) -> CorpusUnit {
    CorpusUnit {
        interview_id: iv.into(),
        unit_id: format!("{iv}-U{n:03}"),
        seq: n,
        respondent,
        text: text.into(),
    }
}

fn corpus(units: Vec<CorpusUnit>) -> Corpus {
    Corpus {
        default_lang: Lang::Fr,
        interview_langs: BTreeMap::new(),
        units,
    }
}

fn sample() -> LexIndex {
    LexIndex::build(&corpus(vec![
        unit(
            "INT-01",
            1,
            false,
            "Que pensez-vous du coût des étiquettes ?",
        ),
        unit(
            "INT-01",
            2,
            true,
            "Le coût des étiquettes RFID reste le frein principal pour nos fournisseurs.",
        ),
        unit(
            "INT-01",
            3,
            true,
            "Nos clients demandent une traçabilité complète des lots.",
        ),
        unit("INT-02", 1, false, "Et les fournisseurs ?"),
        unit(
            "INT-02",
            2,
            true,
            "Les fournisseurs refusent de payer l'étiquette, le coût est trop élevé.",
        ),
        unit(
            "INT-02",
            3,
            true,
            "La traçabilité des lots est exigée par les clients.",
        ),
    ]))
}

#[test]
fn language_parsing_and_stop_words() {
    assert_eq!(Lang::parse("FR"), Some(Lang::Fr));
    assert_eq!(Lang::parse("en-GB"), Some(Lang::En));
    assert_eq!(Lang::parse("de"), Some(Lang::De));
    assert_eq!(Lang::parse("xx"), None);
    assert!(Lang::Fr.is_stop("les"));
    assert!(Lang::Fr.is_stop(&fold("été")));
    assert!(!Lang::Fr.is_stop("etiquette"));
    assert!(Lang::En.is_stop("the"));
}

#[test]
fn snowball_stems_join_inflected_forms() {
    let mut s = Stemmers::default();
    assert_eq!(
        s.stem(Lang::Fr, "étiquettes"),
        s.stem(Lang::Fr, "étiquette")
    );
    assert_eq!(
        s.stem(Lang::Fr, "fournisseurs"),
        s.stem(Lang::Fr, "fournisseur")
    );
    assert_eq!(s.stem(Lang::En, "tagging"), s.stem(Lang::En, "tagged"));
}

#[test]
fn fold_removes_accents_and_case() {
    assert_eq!(fold("Économie"), "economie");
    assert_eq!(fold("Cœur"), "coeur");
    assert_eq!(fold("l’outil"), "l'outil");
}

#[test]
fn panel_gives_keyphrases_stems_and_a_provisional_label() {
    let p = analyse_text(
        "Le coût de l'étiquette est élevé. Le coût bloque les fournisseurs.",
        Lang::Fr,
    );
    assert!(p.provisional);
    assert!(!p.keyphrases.is_empty());
    assert!(p.label.is_some());
    let top = &p.stems[0];
    assert_eq!(top.count, 2, "{top:?}");
    assert!(top.forms.contains(&"coût".to_string()));
    assert!(p.stems.iter().all(|s| s.stem != "le"));
}

#[test]
fn similar_ranks_by_shared_vocabulary_and_skips_interviewer_turns() {
    let idx = sample();
    let hits = idx
        .similar_to_unit("INT-01", "INT-01-U002", 5, None, 0.0)
        .unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].unit_id, "INT-02-U002", "{hits:?}");
    assert!(hits.iter().all(|h| h.unit_id != "INT-01-U002"));
    // The interviewer turns share "coût" and "étiquettes" but are not indexed.
    assert!(hits.iter().all(|h| !h.unit_id.ends_with("-U001")));
    let top = &hits[0];
    assert!(top.score > 0.0 && top.score <= 1.0);
    let stems: Vec<&str> = top.shared_terms.iter().map(|t| t.stem.as_str()).collect();
    assert!(stems.contains(&"fournisseur") || stems.iter().any(|s| s.starts_with("fournisseur")));
    // Deterministic: twice the same answer.
    assert_eq!(
        hits,
        idx.similar_to_unit("INT-01", "INT-01-U002", 5, None, 0.0)
            .unwrap()
    );
}

#[test]
fn similar_filters_by_interview_and_min_score() {
    let idx = sample();
    let only_02: BTreeSet<String> = ["INT-02".to_string()].into();
    let hits = idx
        .similar_to_unit("INT-01", "INT-01-U002", 5, Some(only_02), 0.0)
        .unwrap();
    assert!(hits.iter().all(|h| h.interview_id == "INT-02"));
    let none = idx
        .similar_to_unit("INT-01", "INT-01-U002", 5, None, 0.99)
        .unwrap();
    assert!(none.is_empty());
    assert!(idx
        .similar_to_unit("INT-01", "NOPE", 5, None, 0.0)
        .is_none());
}

#[test]
fn similar_of_a_free_text_works_like_a_passage() {
    let idx = sample();
    let hits = idx.similar(&SimilarQuery {
        text: "traçabilité des lots".into(),
        lang: Lang::Fr,
        k: 2,
        exclude: None,
        interviews: None,
        min_score: 0.0,
    });
    assert_eq!(hits.len(), 2);
    let ids: BTreeSet<&str> = hits.iter().map(|h| h.unit_id.as_str()).collect();
    assert!(ids.contains("INT-01-U003") && ids.contains("INT-02-U003"));
}

fn kwic(idx: &LexIndex, q: &str, stem: bool, speakers: Speakers) -> KwicResult {
    idx.kwic(&KwicQuery {
        query: q.into(),
        stem,
        width: 20,
        interviews: None,
        speakers,
        limit: 100,
        offset: 0,
    })
}

#[test]
fn kwic_finds_words_ignoring_case_and_accents() {
    let idx = sample();
    let r = kwic(&idx, "COUT", false, Speakers::Respondent);
    assert_eq!(r.total, 2, "{r:?}");
    assert_eq!(r.hits[0].matched, "coût");
    assert_eq!(r.hits[0].left, "Le ");
    assert!(r.hits[0].right.trim_start().starts_with("des"));
    assert_eq!(
        r.by_interview,
        vec![("INT-01".into(), 1), ("INT-02".into(), 1)]
    );
}

#[test]
fn kwic_with_stem_matches_inflected_forms() {
    let idx = sample();
    let exact = kwic(&idx, "étiquette", false, Speakers::Respondent);
    let stem = kwic(&idx, "étiquette", true, Speakers::Respondent);
    assert_eq!(exact.total, 1);
    assert_eq!(stem.total, 2);
}

#[test]
fn kwic_speakers_phrases_and_filters() {
    let idx = sample();
    assert_eq!(
        kwic(&idx, "fournisseurs", false, Speakers::Interviewer).total,
        1
    );
    assert_eq!(kwic(&idx, "fournisseurs", false, Speakers::All).total, 3);
    assert_eq!(
        kwic(&idx, "traçabilité des lots", false, Speakers::Respondent).total,
        1
    );
    assert_eq!(kwic(&idx, "des lots", false, Speakers::Respondent).total, 2);
    assert_eq!(
        kwic(&idx, "lots traçabilité", false, Speakers::Respondent).total,
        0
    );
    assert_eq!(kwic(&idx, "", false, Speakers::All).total, 0);
    let only_01: BTreeSet<String> = ["INT-01".to_string()].into();
    let r = idx.kwic(&KwicQuery {
        query: "coût".into(),
        stem: false,
        width: 10,
        interviews: Some(only_01),
        speakers: Speakers::All,
        limit: 1,
        offset: 0,
    });
    assert_eq!(r.total, 2);
    assert_eq!(
        r.hits.len(),
        1,
        "limit applies to the page, not to the total"
    );
    assert_eq!(r.hits[0].role, "interviewer");
}

#[test]
fn kwic_positions_are_unicode_scalars() {
    let idx = LexIndex::build(&corpus(vec![unit(
        "INT-01",
        1,
        true,
        "😀 Voilà le coût réel",
    )]));
    let r = kwic(&idx, "coût", false, Speakers::All);
    assert_eq!(r.hits[0].start, 11);
    assert_eq!(r.hits[0].end, 15);
    let text: Vec<char> = "😀 Voilà le coût réel".chars().collect();
    assert_eq!(text[11..15].iter().collect::<String>(), "coût");
}

#[test]
fn keyword_table_counts_passages_and_interviews() {
    let idx = sample();
    let passages = vec![
        PassageText {
            passage_id: "P-00001".into(),
            interview_id: "INT-01".into(),
            text: "le coût des étiquettes".into(),
        },
        PassageText {
            passage_id: "P-00002".into(),
            interview_id: "INT-02".into(),
            text: "payer l'étiquette, le coût est trop élevé".into(),
        },
        PassageText {
            passage_id: "P-00003".into(),
            interview_id: "INT-02".into(),
            text: "le coût encore".into(),
        },
    ];
    let t = idx.keyword_table(
        &passages,
        &KeywordOptions {
            min_passages: 1,
            limit: 50,
        },
    );
    let cout = t.rows.iter().find(|r| r.stem == "coût").unwrap();
    assert_eq!(cout.n_passages, 3);
    assert_eq!(cout.n_interviews, 2);
    assert_eq!(cout.n_occurrences, 3);
    assert_eq!(cout.passage_ids.len(), 3);
    assert!(cout.corpus_occurrences >= 2);
    let etiq = t
        .rows
        .iter()
        .find(|r| r.stem.starts_with("étiquet"))
        .unwrap();
    assert_eq!(etiq.n_passages, 2);
    assert!(etiq.forms.len() >= 2);
    let at_least_two = idx.keyword_table(
        &passages,
        &KeywordOptions {
            min_passages: 2,
            limit: 50,
        },
    );
    assert!(at_least_two.rows.iter().all(|r| r.n_passages >= 2));
    assert!(at_least_two.rows.len() < t.rows.len());
    // Sorted by keyness, then passages, then stem.
    assert!(t.rows.windows(2).all(|w| w[0].keyness >= w[1].keyness));
}

#[test]
fn term_frequency_per_attribute_value() {
    let idx = sample();
    let groups: BTreeMap<String, Option<String>> = [
        ("INT-01".to_string(), Some("PME".to_string())),
        ("INT-02".to_string(), Some("Grand groupe".to_string())),
    ]
    .into();
    let rows = idx.term_frequency(&TermFreqRequest {
        terms: vec!["coût".into(), "inconnu".into()],
        stem: true,
        groups: groups.clone(),
        speakers: Speakers::Respondent,
    });
    assert_eq!(rows.len(), 2);
    let cout = &rows[0];
    assert_eq!(cout.by_value.len(), 2);
    assert_eq!(cout.by_value[0].value.as_deref(), Some("Grand groupe"));
    assert_eq!(cout.by_value[0].n_occurrences, 1);
    assert_eq!(cout.by_value[1].value.as_deref(), Some("PME"));
    assert_eq!(cout.by_value[1].n_interviews, 1);
    assert_eq!(cout.by_value[1].n_interviews_with_term, 1);
    assert!(cout.by_value[1].per_1000_words > 0.0);
    assert!(rows[1].by_value.iter().all(|v| v.n_occurrences == 0));
    let top = idx.top_terms(&groups, Speakers::Respondent, 3);
    assert_eq!(top.len(), 2);
    assert!(top.iter().all(|t| t.terms.len() == 3));
}

#[test]
fn another_language_per_interview() {
    let mut langs = BTreeMap::new();
    langs.insert("EN-01".to_string(), Lang::En);
    let idx = LexIndex::build(&Corpus {
        default_lang: Lang::Fr,
        interview_langs: langs,
        units: vec![
            unit("EN-01", 1, true, "The suppliers refuse tagging costs"),
            unit("FR-01", 1, true, "Les fournisseurs refusent le marquage"),
        ],
    });
    assert_eq!(idx.language_of("EN-01"), Lang::En);
    assert_eq!(idx.language_of("FR-01"), Lang::Fr);
    // "the" is a stop word in English only.
    let r = kwic(&idx, "suppliers", true, Speakers::All);
    assert_eq!(r.total, 1);
    let r = kwic(&idx, "fournisseur", true, Speakers::All);
    assert_eq!(r.total, 1);
}

// ------------------------------------------------------------- performance

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn synthetic(n_interviews: usize, words_per: usize) -> Corpus {
    const SYL: [&str; 24] = [
        "ba", "re", "ti", "con", "pro", "du", "ment", "sion", "tri", "ca", "ble", "lo", "mar",
        "ché", "fo", "ur", "nis", "seur", "éti", "quet", "tra", "ça", "bi", "li",
    ];
    let mut rng = Lcg(42);
    let vocab: Vec<String> = (0..4000)
        .map(|_| {
            let n = 2 + (rng.next() % 3) as usize;
            (0..n)
                .map(|_| SYL[(rng.next() % SYL.len() as u64) as usize])
                .collect::<String>()
        })
        .collect();
    let mut units = Vec::new();
    for i in 0..n_interviews {
        let iv = format!("INT-{i:02}");
        let mut written = 0;
        let mut n = 0;
        while written < words_per {
            n += 1;
            let len = 40 + (rng.next() % 60) as usize;
            let mut text = String::new();
            for w in 0..len {
                // A skewed draw so that frequent words exist.
                let r = rng.next() % 100;
                let ix = if r < 40 {
                    (rng.next() % 50) as usize
                } else {
                    (rng.next() % 4000) as usize
                };
                text.push_str(&vocab[ix]);
                text.push(if w % 17 == 16 { '.' } else { ' ' });
                if w % 17 == 16 {
                    text.push(' ');
                }
            }
            written += len;
            units.push(unit(&iv, n, n % 3 != 1, &text));
        }
    }
    corpus(units)
}

#[test]
fn thirty_interviews_of_fifteen_thousand_words_answer_under_300_ms() {
    let c = synthetic(30, 15_000);
    let t0 = Instant::now();
    let idx = LexIndex::build(&c);
    let built = t0.elapsed();
    let stats = idx.stats();
    assert_eq!(stats.n_interviews, 30);
    assert!(stats.n_words >= 450_000, "{stats:?}");
    eprintln!("index built in {built:?}: {stats:?}");

    let mut worst = std::time::Duration::ZERO;
    let mut timed = |name: &str, f: &mut dyn FnMut()| {
        let t = Instant::now();
        f();
        let d = t.elapsed();
        eprintln!("{name}: {d:?}");
        worst = worst.max(d);
        assert!(d.as_millis() < 300, "{name} took {d:?}");
    };
    let unit_id = c.units[10].unit_id.clone();
    let iv = c.units[10].interview_id.clone();
    timed("similar", &mut || {
        let r = idx.similar_to_unit(&iv, &unit_id, 10, None, 0.0).unwrap();
        assert_eq!(r.len(), 10);
    });
    let word = c.units[10]
        .text
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    timed("kwic stem", &mut || {
        let r = kwic(&idx, &word, true, Speakers::Respondent);
        assert!(r.total > 0);
    });
    timed("kwic exact", &mut || {
        let _ = kwic(&idx, &word, false, Speakers::All);
    });
    let passages: Vec<PassageText> = c
        .units
        .iter()
        .step_by(10)
        .take(300)
        .map(|u| PassageText {
            passage_id: u.unit_id.clone(),
            interview_id: u.interview_id.clone(),
            text: u.text.chars().take(200).collect(),
        })
        .collect();
    timed("keyword table", &mut || {
        let t = idx.keyword_table(
            &passages,
            &KeywordOptions {
                min_passages: 1,
                limit: 100,
            },
        );
        assert!(!t.rows.is_empty());
    });
    let groups: BTreeMap<String, Option<String>> = (0..30)
        .map(|i| (format!("INT-{i:02}"), Some(format!("v{}", i % 3))))
        .collect();
    timed("term frequency", &mut || {
        let r = idx.term_frequency(&TermFreqRequest {
            terms: vec![word.clone()],
            stem: true,
            groups: groups.clone(),
            speakers: Speakers::Respondent,
        });
        assert_eq!(r[0].by_value.len(), 3);
    });
    timed("top terms", &mut || {
        let _ = idx.top_terms(&groups, Speakers::Respondent, 10);
    });
    timed("panel", &mut || {
        let _ = analyse_text(&c.units[10].text, Lang::Fr);
    });
}
