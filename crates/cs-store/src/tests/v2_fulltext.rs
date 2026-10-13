//! V2, F-55: full-text search across interviews, accent-insensitive, with
//! filters and highlights.

use super::v2_common::*;
use super::*;
use crate::fulltext::{SearchMode, SearchRequest};
use cs_model::passage::{scalar_to_utf16, slice_scalars};

const F: &str = "FRA-01";
const F1: &str = "FRA-01-U001";
const F2: &str = "FRA-01-U002";

/// ECO-16 (3 respondent units, sector public), RET-22 (an interviewer turn,
/// then a respondent one; sector privé), FRA-01 (French, no sector).
fn fixture() -> (Store, TempDir) {
    let (s, d) = ready();
    add_interview(
        &s,
        R,
        &[
            "Why did you stop?",
            "The partner pulled out of the pilot, then we stopped the pilot.",
        ],
    );
    // R1 is the interviewer's.
    s.correct_unit(R, R1, "Why did you stop?", SpeakerRole::Interviewer, "speaker", ACTOR)
        .unwrap();
    add_interview(
        &s,
        F,
        &[
            "😀 Nous avons suivi une formation à la formation des étiquettes.",
            "La gestion des étiquettes était confiée à l'équipe.",
        ],
    );
    with_attributes(&s);
    set_attr(&s, ID, "sector", "public");
    set_attr(&s, R, "sector", "privé");
    (s, d)
}

fn req(q: &str) -> SearchRequest {
    SearchRequest {
        q: q.into(),
        ..Default::default()
    }
}

fn units(r: &crate::fulltext::SearchResult) -> Vec<String> {
    r.items.iter().map(|i| i.unit_id.clone()).collect()
}

#[test]
fn all_words_mode_needs_every_word_in_the_turn_and_ignores_accents() {
    let (s, _d) = fixture();
    let r = s.search_text(&req("ETIQUETTES formation")).unwrap();
    assert_eq!(units(&r), [F1]);
    let r = s.search_text(&req("étiquettes")).unwrap();
    assert_eq!(units(&r), [F1, F2]);
    // Whole words by default; the apostrophe separates words.
    assert!(s.search_text(&req("etiquet")).unwrap().items.is_empty());
    assert_eq!(units(&s.search_text(&req("equipe")).unwrap()), [F2]);
    // Prefix on request.
    let mut q = req("etiquet");
    q.prefix = true;
    assert_eq!(units(&s.search_text(&q).unwrap()), [F1, F2]);
}

#[test]
fn phrase_mode_needs_the_words_next_to_each_other() {
    let (s, _d) = fixture();
    let mut q = req("la formation");
    q.mode = SearchMode::Phrase;
    assert_eq!(units(&s.search_text(&q).unwrap()), [F1]);
    let mut q = req("formation une");
    q.mode = SearchMode::Phrase;
    assert!(s.search_text(&q).unwrap().items.is_empty());
    // The same words in the other mode match wherever they are.
    assert_eq!(units(&s.search_text(&req("formation une")).unwrap()), [F1]);
    // Punctuation and case do not break a phrase.
    let mut q = req("GESTION, des étiquettes");
    q.mode = SearchMode::Phrase;
    assert_eq!(units(&s.search_text(&q).unwrap()), [F2]);
}

#[test]
fn matches_are_positions_in_the_original_text_in_scalars_and_utf16() {
    let (s, _d) = fixture();
    let r = s.search_text(&req("formation")).unwrap();
    let hit = &r.items[0];
    assert_eq!(hit.unit_id, F1);
    assert_eq!(hit.matches.len(), 2);
    for m in &hit.matches {
        assert_eq!(slice_scalars(&hit.text, m.start, m.end).unwrap(), "formation");
        assert_eq!(m.start_utf16, scalar_to_utf16(&hit.text, m.start).unwrap());
        assert_eq!(m.end_utf16, scalar_to_utf16(&hit.text, m.end).unwrap());
    }
    // The emoji takes two UTF-16 units.
    assert_eq!(hit.matches[0].start_utf16, hit.matches[0].start + 1);
    // Phrase highlight covers the whole phrase, original accents kept.
    let mut q = req("des ÉTIQUETTES");
    q.mode = SearchMode::Phrase;
    let r = s.search_text(&q).unwrap();
    let m = &r.items[0].matches[0];
    assert_eq!(
        slice_scalars(&r.items[0].text, m.start, m.end).unwrap(),
        "des étiquettes"
    );
    // The excerpt carries the highlights relative to itself.
    let e = &r.items[0].excerpt;
    let h = &e.highlights[0];
    assert_eq!(slice_scalars(&e.text, h.start, h.end).unwrap(), "des étiquettes");
}

#[test]
fn interviewer_turns_are_searched_only_on_request() {
    let (s, _d) = fixture();
    assert!(s.search_text(&req("stop")).unwrap().items.is_empty());
    let r = s.search_text(&req("stopped")).unwrap();
    assert_eq!(units(&r), ["RET-22-U002"]);
    let mut q = req("why");
    q.include_interviewer = true;
    let r = s.search_text(&q).unwrap();
    assert_eq!(units(&r), [R1]);
    assert_eq!(r.items[0].role, "interviewer");
}

#[test]
fn filters_by_interview_and_by_attribute_value() {
    let (s, _d) = fixture();
    let mut q = req("pilot");
    q.interviews = vec![ID.into()];
    assert_eq!(units(&s.search_text(&q).unwrap()), [U1]);
    let mut q = req("pilot");
    q.attribute = Some("sector".into());
    q.value = Some("privé".into());
    assert_eq!(units(&s.search_text(&q).unwrap()), ["RET-22-U002"]);
    let mut q = req("pilot");
    q.attribute = Some("sector".into());
    assert!(s.search_text(&q).is_err(), "attribute and value go together");
    let mut q = req("pilot");
    q.interviews = vec!["NOPE".into()];
    assert!(s.search_text(&q).is_err());
}

#[test]
fn every_interview_is_listed_with_its_count_including_those_without_mention() {
    let (s, _d) = fixture();
    let r = s.search_text(&req("pilot")).unwrap();
    let by: Vec<(&str, usize, usize, bool)> = r
        .interviews
        .iter()
        .map(|i| (i.interview_id.as_str(), i.units, i.hits, i.mentioned))
        .collect();
    assert_eq!(
        by,
        [
            (ID, 1, 1, true),
            ("FRA-01", 0, 0, false),
            ("RET-22", 1, 2, true)
        ]
    );
    assert_eq!(r.total_units, 2);
    assert_eq!(r.total_hits, 3);
}

#[test]
fn coded_and_not_coded_look_at_the_passages_that_cover_the_match() {
    let (s, _d) = fixture();
    let a = code(&s, "Pilote", None);
    let b = code(&s, "Autre", None);
    // "pilot" in ECO-16-U1 ("So tell me about the pilot.") is coded; in RET-22
    // only the first half of the turn is coded, the second "pilot" is not.
    pcode(&s, ID, U1, Some((21, 26)), &[&a]);
    let p = pcode(&s, R, R2, Some((0, 40)), &[&b]);
    let mut q = req("pilot");
    q.coded = Some(true);
    let r = s.search_text(&q).unwrap();
    assert_eq!(units(&r), [U1, "RET-22-U002"]);
    assert!(r.items[0].coded);
    assert_eq!(r.items[0].code_ids, [a.clone()]);
    assert_eq!(r.items[1].passage_ids, [p.clone()]);
    // Only one of the two matches of RET-22-U002 is covered; the unit counts as
    // coded, and also as not entirely coded.
    assert_eq!(r.items[1].matches.len(), 2);
    assert_eq!(r.items[1].uncovered_matches, 1);
    q.coded = Some(false);
    let r = s.search_text(&q).unwrap();
    assert_eq!(units(&r), ["RET-22-U002"], "a not-coded match remains there");
    // A code filter, with its descendants.
    let child = code(&s, "Sous-pilote", Some(&a));
    pcode(&s, ID, U1, Some((0, 2)), &[&child]);
    let mut q = req("so pilot");
    q.code = Some(a.clone());
    assert_eq!(units(&s.search_text(&q).unwrap()), [U1]);
    let mut q = req("so");
    q.code = Some(a.clone());
    assert_eq!(units(&s.search_text(&q).unwrap()), [U1], "child code counts");
    q.descendants = false;
    assert!(s.search_text(&q).unwrap().items.is_empty());
}

#[test]
fn the_index_follows_the_transcript() {
    let (s, _d) = fixture();
    assert!(s.search_text(&req("zebra")).unwrap().items.is_empty());
    s.correct_unit(ID, U2, "A zebra crossed the road.", SpeakerRole::Respondent, "fix", ACTOR)
        .unwrap();
    assert_eq!(units(&s.search_text(&req("zebra")).unwrap()), [U2]);
    // The old text is gone.
    assert!(s.search_text(&req("tags")).unwrap().items.is_empty());
}

#[test]
fn context_limit_and_offset() {
    let (s, _d) = fixture();
    let r = s.search_text(&req("pilot")).unwrap();
    let hit = r.items.iter().find(|i| i.unit_id == U1).unwrap();
    assert!(hit.previous.is_none());
    assert_eq!(hit.next.as_ref().unwrap().unit_id, U2);
    let mut q = req("pilot");
    q.limit = Some(1);
    let r = s.search_text(&q).unwrap();
    assert_eq!(r.items.len(), 1);
    assert!(r.truncated);
    assert_eq!(r.total_units, 2);
    q.offset = 1;
    let r = s.search_text(&q).unwrap();
    assert_eq!(units(&r), ["RET-22-U002"]);
    assert!(!r.truncated);
}

#[test]
fn an_empty_query_is_refused() {
    let (s, _d) = fixture();
    assert!(s.search_text(&req("   ")).is_err());
    assert!(s.search_text(&req("?!")).is_err());
}
