//! V1 lot 3: the lexical workbench over the stored corpus. Acceptance test
//! A27 (store side): keywords of highlights, similar passages, the cache that
//! follows the transcripts, attributes as filters.

use super::*;
use crate::lexicon::{LexiconConfig, SimilarOptions};
use crate::passages::NewPassage;
use cs_lex::{KeywordOptions, KwicQuery, Speakers};
use cs_model::passage::OffsetUnit;
use cs_model::project::*;

const TEXTS: [&str; 4] = [
    "So tell me about the pilot.",
    "Le coût des étiquettes RFID reste le frein principal pour nos fournisseurs.",
    "Les fournisseurs refusent de payer l'étiquette, le coût est trop élevé.",
    "Nos clients demandent une traçabilité complète des lots.",
];

fn lex_ready() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    s.import_codebook(&coding_db(&dir, &default_codes()), ACTOR)
        .unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    let mut seg = segmentation(&TEXTS);
    seg.units[0].role = SpeakerRole::Interviewer;
    seg.hash = Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
    (s, dir)
}

fn second(s: &Store, id: &str, texts: &[&str]) {
    let mut e = default_entry();
    e.interview_id = id.into();
    e.folder_name = format!("{id}_x");
    e.folder_path = format!("/tmp/{id}_x");
    s.upsert_interview(&e, ACTOR).unwrap();
    let units: Vec<Unit> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| Unit {
            unit_id: format!("{id}-U{:03}", i + 1),
            seq: i as u32 + 1,
            speaker_raw: "RESP".into(),
            role: SpeakerRole::Respondent,
            timestamp: format!("00:0{i}"),
            text: t.to_string(),
            corrected: false,
        })
        .collect();
    s.save_segmentation(
        &Segmentation {
            interview_id: id.into(),
            source_rel_path: "02_transcript_cleaned_diarised.md".into(),
            source_sha256: "aa".into(),
            rule: SEGMENTATION_RULE.into(),
            hash: Segmentation::units_hash(&units),
            units,
            warnings: vec![],
        },
        ACTOR,
    )
    .unwrap();
}

fn whole(id: &str, unit: &str) -> NewPassage {
    NewPassage {
        interview_id: id.into(),
        unit_id: unit.into(),
        start: None,
        end: None,
        offset_unit: OffsetUnit::Scalar,
        text: None,
    }
}

#[test]
fn the_index_is_cached_until_the_transcripts_change() {
    let (s, _d) = lex_ready();
    let a = s.lexical_index().unwrap();
    let b = s.lexical_index().unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&a, &b),
        "unchanged corpus, same index"
    );
    assert_eq!(a.stats().n_interviews, 1);
    assert_eq!(a.stats().n_respondent_units, 3);

    // A new interview changes the fingerprint.
    second(&s, "ECO-17", &["Le coût de la traçabilité."]);
    let c = s.lexical_index().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&a, &c));
    assert_eq!(c.stats().n_interviews, 2);

    // A correction of a unit text changes it too, and the index reads the
    // corrected text.
    s.correct_unit(
        ID,
        U2,
        "Le prix des étiquettes RFID reste le frein principal.",
        SpeakerRole::Respondent,
        "typo",
        ACTOR,
    )
    .unwrap();
    let d = s.lexical_index().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&c, &d));
    let r = d.kwic(&KwicQuery {
        query: "prix".into(),
        stem: false,
        width: 20,
        interviews: None,
        speakers: Speakers::Respondent,
        limit: 10,
        offset: 0,
    });
    assert_eq!(r.total, 1);

    // A new segmentation of an interview too.
    let mut seg = segmentation(&TEXTS);
    seg.units[0].role = SpeakerRole::Interviewer;
    seg.units[3].text = "Nos clients veulent savoir d'où viennent les lots.".into();
    seg.hash = Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
    let e = s.lexical_index().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&d, &e));
}

#[test]
fn the_language_setting_is_validated_audited_and_invalidates_the_index() {
    let (s, _d) = lex_ready();
    assert_eq!(s.lexicon_config().unwrap(), LexiconConfig::default());
    let before = s.lexical_index().unwrap();
    let bad = LexiconConfig {
        language: "xx".into(),
        interview_languages: Default::default(),
    };
    assert!(s.set_lexicon_config(&bad, ACTOR).is_err());
    let mut cfg = LexiconConfig::default();
    cfg.interview_languages.insert("NOPE".into(), "en".into());
    assert!(s.set_lexicon_config(&cfg, ACTOR).is_err());
    cfg.interview_languages.clear();
    cfg.interview_languages.insert(ID.into(), "EN".into());
    let stored = s.set_lexicon_config(&cfg, ACTOR).unwrap();
    assert_eq!(stored.interview_languages[ID], "en");
    let after = s.lexical_index().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&before, &after));
    assert_eq!(after.language_of(ID), cs_lex::Lang::En);
    assert!(s
        .audit_events(None)
        .unwrap()
        .iter()
        .any(|e| e.action == "lexicon.config.update"));
    // The project language itself equal to the interview's: no override kept.
    let same = LexiconConfig {
        language: "en".into(),
        interview_languages: [(ID.to_string(), "en".to_string())].into(),
    };
    assert!(s
        .set_lexicon_config(&same, ACTOR)
        .unwrap()
        .interview_languages
        .is_empty());
}

#[test]
fn keywords_cover_highlights_by_default_and_all_passages_on_request() {
    let (s, _d) = lex_ready();
    second(&s, "ECO-17", &["Le coût de la traçabilité reste élevé."]);
    let (h1, _) = s.create_passage(&whole(ID, U2), ACTOR).unwrap();
    let (h2, _) = s
        .create_passage(&whole("ECO-17", "ECO-17-U001"), ACTOR)
        .unwrap();
    let (coded, _) = s.create_passage(&whole(ID, U3), ACTOR).unwrap();
    s.code_passage(&coded.passage_id, "TECH-ADV", "", ACTOR)
        .unwrap();
    let opts = KeywordOptions {
        min_passages: 1,
        limit: 50,
    };
    let t = s.lexical_keywords("highlights", None, &opts).unwrap();
    assert_eq!(t.n_passages, 2, "the coded passage is not a highlight");
    let cout = t.rows.iter().find(|r| r.stem == "coût").unwrap();
    assert_eq!(cout.n_passages, 2);
    assert_eq!(cout.n_interviews, 2);
    assert!(cout.passage_ids.contains(&h1.passage_id));
    assert!(cout.passage_ids.contains(&h2.passage_id));
    let all = s.lexical_keywords("passages", None, &opts).unwrap();
    assert_eq!(all.n_passages, 3);
    let only: std::collections::BTreeSet<String> = [ID.to_string()].into();
    let one = s
        .lexical_keywords("highlights", Some(&only), &opts)
        .unwrap();
    assert_eq!(one.n_passages, 1);
    assert!(s.lexical_keywords("everything", None, &opts).is_err());
}

#[test]
fn similar_passages_are_annotated_with_the_codes_of_the_unit_and_can_skip_coded_ones() {
    let (s, _d) = lex_ready();
    second(
        &s,
        "ECO-17",
        &["Le coût de ces étiquettes pèse sur les fournisseurs."],
    );
    let (p, _) = s.create_passage(&whole(ID, U2), ACTOR).unwrap();
    let (other, _) = s
        .create_passage(&whole("ECO-17", "ECO-17-U001"), ACTOR)
        .unwrap();
    s.code_passage(&other.passage_id, "TECH-ADV", "", ACTOR)
        .unwrap();
    let opts = SimilarOptions {
        k: 5,
        ..Default::default()
    };
    let hits = s.similar_to_passage(&p.passage_id, &opts).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].hit.unit_id, "ECO-17-U001");
    let eco17 = hits
        .iter()
        .find(|h| h.hit.interview_id == "ECO-17")
        .unwrap();
    assert_eq!(eco17.coded_with.len(), 1);
    assert_eq!(eco17.coded_with[0].name, "TECH-ADV");
    // The unit of the passage itself is left out unless asked for.
    assert!(hits.iter().all(|h| h.hit.unit_id != U2));
    let with_own = s
        .similar_to_passage(
            &p.passage_id,
            &SimilarOptions {
                k: 5,
                include_own_unit: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(with_own[0].hit.unit_id, U2);
    // Coverage check: what is similar and does not carry the code yet.
    let missed = s
        .similar_to_passage(
            &p.passage_id,
            &SimilarOptions {
                k: 5,
                without_code: Some("TECH-ADV".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(missed.iter().all(|h| h.hit.interview_id != "ECO-17"));
    // The same from a unit.
    let from_unit = s.similar_to_unit(ID, U2, &opts).unwrap();
    assert_eq!(from_unit[0].hit.unit_id, "ECO-17-U001");
    assert_eq!(missed[0].hit.unit_id, U3);
    assert!(s.similar_to_unit(ID, "NOPE", &opts).is_err());
}

#[test]
fn a_passage_gets_keyphrases_stems_and_a_provisional_label() {
    let (s, _d) = lex_ready();
    let (p, _) = s.create_passage(&whole(ID, U2), ACTOR).unwrap();
    let panel = s.passage_lexical(&p.passage_id).unwrap();
    assert!(panel.provisional);
    assert!(!panel.keyphrases.is_empty());
    assert!(panel.label.is_some());
    assert!(panel.stems.iter().any(|x| x.stem == "coût"));
    assert!(s.passage_lexical("P-99999").is_err());
}

#[test]
fn attributes_are_filters_and_groups_of_term_frequencies() {
    let (s, _d) = lex_ready();
    second(&s, "ECO-17", &["Le coût de la traçabilité reste élevé."]);
    let mut cfg = s.project_config().unwrap();
    cfg.attributes = vec![AttributeDef {
        id: "sector".into(),
        label: "Secteur".into(),
        kind: AttributeKind::Choice,
        choices: vec!["public".into(), "privé".into()],
        shared_with_model: false,
        scope: Default::default(),
    }];
    s.set_project_config(&cfg, ACTOR).unwrap();
    let m = |v: &str| {
        [("sector".to_string(), Some(v.to_string()))]
            .into_iter()
            .collect()
    };
    s.set_interview_attributes(ID, &m("public"), ACTOR).unwrap();
    s.set_interview_attributes("ECO-17", &m("privé"), ACTOR)
        .unwrap();
    let rows = s
        .lexical_term_frequency("sector", &["coût".into()], true, Speakers::Respondent)
        .unwrap();
    let by: Vec<(Option<String>, usize)> = rows[0]
        .by_value
        .iter()
        .map(|v| (v.value.clone(), v.n_occurrences))
        .collect();
    assert_eq!(
        by,
        vec![(Some("privé".into()), 1), (Some("public".into()), 2)]
    );
    assert!(s
        .lexical_term_frequency("nope", &["coût".into()], true, Speakers::All)
        .is_err());
    assert_eq!(
        s.interview_filter(Some("sector"), Some("privé"))
            .unwrap()
            .unwrap()
            .len(),
        1
    );
    assert!(s.interview_filter(Some("sector"), None).is_err());
    assert!(s.interview_filter(None, None).unwrap().is_none());
    let top = s
        .lexical_top_terms("sector", Speakers::Respondent, 3)
        .unwrap();
    assert_eq!(top.len(), 2);
}
