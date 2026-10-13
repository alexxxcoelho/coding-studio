//! V1 lot 1: passages, codings of passages, highlights, the LATER marker,
//! stale anchors and roll-up counts. Acceptance tests A1, A2, A3 (store
//! side), A5 (counts) and A11.

use super::*;
use crate::passages::{NewPassage, PassageFilter};
use cs_model::passage::{scalar_len, text_digest, OffsetUnit};

fn span(unit: &str, start: usize, end: usize) -> NewPassage {
    NewPassage {
        interview_id: ID.into(),
        unit_id: unit.into(),
        start: Some(start),
        end: Some(end),
        offset_unit: OffsetUnit::Scalar,
        text: None,
    }
}

fn whole(unit: &str) -> NewPassage {
    NewPassage {
        interview_id: ID.into(),
        unit_id: unit.into(),
        start: None,
        end: None,
        offset_unit: OffsetUnit::Scalar,
        text: None,
    }
}

fn code_id(s: &Store, token: &str) -> String {
    s.code_node(token).unwrap().code_id
}

fn validation_message(err: anyhow::Error) -> String {
    match err.downcast_ref::<CsError>() {
        Some(CsError::Validation(m)) => m.clone(),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

/// A second interview with `n` respondent units "text of <id> <i>".
fn add_interview(s: &Store, id: &str, n: usize) {
    let mut e = default_entry();
    e.interview_id = id.into();
    e.folder_name = format!("{id}_x");
    e.folder_path = format!("/tmp/{id}_x");
    s.upsert_interview(&e, ACTOR).unwrap();
    let units: Vec<Unit> = (1..=n)
        .map(|i| Unit {
            unit_id: format!("{id}-U{i:03}"),
            seq: i as u32,
            speaker_raw: "RESP".into(),
            role: SpeakerRole::Respondent,
            timestamp: format!("00:0{i}"),
            text: format!("text of {id} number {i}, long enough to select a part"),
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

#[test]
fn a_whole_unit_passage_covers_the_unit_text() {
    let (s, _d) = ready();
    let (p, created) = s.create_passage(&whole(U2), ACTOR).unwrap();
    assert!(created);
    assert!(p.whole_unit);
    assert_eq!(p.start, 0);
    assert_eq!(p.end, scalar_len("The tags paid back within a year."));
    assert_eq!(p.text, "The tags paid back within a year.");
    assert_eq!(p.anchor_digest, text_digest(&p.text));
    assert_eq!(
        p.segmentation_hash,
        s.segmentation(ID).unwrap().unwrap().hash
    );
    assert_eq!(p.kind, "highlight");
    assert!(!p.stale);
    assert!(p.passage_id.starts_with("P-"));
}

#[test]
fn a1_a_selected_span_is_stored_with_its_exact_text_and_context() {
    let (s, _d) = ready();
    // "The tags paid back within a year.": chars 4..8 is "tags".
    let (p, _) = s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    assert_eq!(p.text, "tags");
    assert!(!p.whole_unit);
    assert_eq!((p.start, p.end), (4, 8));
    // Reopen.
    let again = s.passage(&p.passage_id).unwrap();
    assert_eq!(again.text, "tags");
    assert_eq!(again.before, "The ");
    assert_eq!(again.after, " paid back within a year.");
    assert_eq!(again.kind, "coded");
    assert_eq!(again.codings.len(), 1);
    assert_eq!(again.codings[0].code_name, "TECH-ADV");
}

#[test]
fn a1_a_passage_whose_offsets_do_not_return_the_expected_text_is_refused() {
    let (s, _d) = ready();
    let mut bad = span(U2, 4, 8);
    bad.text = Some("tugs".into());
    let m = validation_message(s.create_passage(&bad, ACTOR).unwrap_err());
    assert!(m.contains("tags"), "{m}");
    let mut good = span(U2, 4, 8);
    good.text = Some("tags".into());
    assert!(s.create_passage(&good, ACTOR).is_ok());
    // Nothing was stored for the refused one.
    assert_eq!(s.passages(&PassageFilter::default()).unwrap().len(), 1);
}

#[test]
fn offsets_outside_the_unit_or_empty_are_refused() {
    let (s, _d) = ready();
    let len = scalar_len("The tags paid back within a year.");
    for (a, b) in [(5, 5), (8, 4), (0, len + 1), (len + 3, len + 5)] {
        assert!(
            s.create_passage(&span(U2, a, b), ACTOR).is_err(),
            "{a}..{b}"
        );
    }
    // Only one end given.
    let mut half = whole(U2);
    half.start = Some(3);
    assert!(s.create_passage(&half, ACTOR).is_err());
    // Unknown unit and unknown interview.
    assert!(s.create_passage(&span("ECO-16-U099", 0, 1), ACTOR).is_err());
    let mut other = span(U2, 0, 3);
    other.interview_id = "NOPE".into();
    assert!(s.create_passage(&other, ACTOR).is_err());
}

#[test]
fn a_selection_of_blanks_only_is_refused() {
    let (s, _d) = ready();
    // Position 3 is the space between "The" and "tags".
    assert!(s.create_passage(&span(U2, 3, 4), ACTOR).is_err());
}

#[test]
fn a2_two_codes_on_one_passage_and_overlapping_passages_all_stay() {
    let (s, _d) = ready();
    let (p1, _) = s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    let (p1b, created) = s
        .code_span(&span(U2, 4, 8), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    assert!(!created, "the same span is the same passage");
    assert_eq!(p1b.passage_id, p1.passage_id);
    assert_eq!(p1b.codings.len(), 2);
    // An overlapping passage is another passage.
    let (p2, created) = s
        .code_span(&span(U2, 4, 18), "ENV-PARTNER", "", ACTOR)
        .unwrap();
    assert!(created);
    assert_ne!(p2.passage_id, p1.passage_id);
    let all = s
        .passages(&PassageFilter {
            interview_id: Some(ID.into()),
            unit_id: Some(U2.into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(all.len(), 2);
    let first = all.iter().find(|p| p.passage_id == p1.passage_id).unwrap();
    let names: Vec<&str> = first.codings.iter().map(|c| c.code_name.as_str()).collect();
    assert_eq!(names, vec!["TECH-ADV", "ORG-SKILLS"]);
    // Coding twice with the same code does not duplicate.
    let (again, _) = s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    assert_eq!(again.codings.len(), 2);
}

#[test]
fn coding_a_whole_unit_stays_possible_and_leaves_the_unit_level_coding_alone() {
    let (s, _d) = ready();
    coded(&s);
    let before = s.human_codings(ID, 2).unwrap();
    let (p, _) = s.code_span(&whole(U3), "ORG-SKILLS", "", ACTOR).unwrap();
    assert!(p.whole_unit);
    assert_eq!(s.human_codings(ID, 2).unwrap(), before);
}

#[test]
fn a11_unicode_offsets_round_trip_between_utf16_and_scalars() {
    let (s, _d) = ready();
    // Replace the unit text through a correction to get a rich text.
    let text = "Éric dit: « c'est l'été » 😀 fin\nligne deux";
    s.correct_unit(ID, U2, text, SpeakerRole::Respondent, "test", ACTOR)
        .unwrap();
    // Select "l'été" with UTF-16 offsets: scalars 21..26 (no astral before it).
    let start_scalar = text.chars().position(|c| c == 'l').unwrap();
    let sel: String = text.chars().skip(start_scalar).take(5).collect();
    assert_eq!(sel, "l'été");
    let (p, _) = s
        .code_span(
            &NewPassage {
                interview_id: ID.into(),
                unit_id: U2.into(),
                start: Some(start_scalar),
                end: Some(start_scalar + 5),
                offset_unit: OffsetUnit::Utf16,
                text: Some("l'été".into()),
            },
            "TECH-ADV",
            "",
            ACTOR,
        )
        .unwrap();
    assert_eq!(p.text, "l'été");
    // The emoji and what follows: UTF-16 offsets are shifted by one after it.
    let emoji_scalar = text.chars().position(|c| c == '😀').unwrap();
    let emoji_utf16 = text
        .chars()
        .take(emoji_scalar)
        .map(char::len_utf16)
        .sum::<usize>();
    let (e, _) = s
        .code_span(
            &NewPassage {
                interview_id: ID.into(),
                unit_id: U2.into(),
                start: Some(emoji_utf16),
                end: Some(emoji_utf16 + 2),
                offset_unit: OffsetUnit::Utf16,
                text: Some("😀".into()),
            },
            "TECH-ADV",
            "",
            ACTOR,
        )
        .unwrap();
    assert_eq!(e.text, "😀");
    assert_eq!((e.start, e.end), (emoji_scalar, emoji_scalar + 1));
    assert_eq!(e.start_utf16, Some(emoji_utf16));
    assert_eq!(e.end_utf16, Some(emoji_utf16 + 2));
    // After the emoji, the line break and the second line.
    let nl_scalar = text.chars().position(|c| c == '\n').unwrap();
    let nl_utf16 = text
        .chars()
        .take(nl_scalar)
        .map(char::len_utf16)
        .sum::<usize>();
    let (l, _) = s
        .code_span(
            &NewPassage {
                interview_id: ID.into(),
                unit_id: U2.into(),
                start: Some(nl_utf16),
                end: Some(nl_utf16 + 1 + "ligne".len()),
                offset_unit: OffsetUnit::Utf16,
                text: Some("\nligne".into()),
            },
            "TECH-ADV",
            "",
            ACTOR,
        )
        .unwrap();
    assert_eq!(l.text, "\nligne");
    assert_eq!(l.start, nl_scalar);
    // A UTF-16 position in the middle of the emoji is refused.
    let err = s
        .create_passage(
            &NewPassage {
                interview_id: ID.into(),
                unit_id: U2.into(),
                start: Some(emoji_utf16 + 1),
                end: Some(emoji_utf16 + 3),
                offset_unit: OffsetUnit::Utf16,
                text: None,
            },
            ACTOR,
        )
        .unwrap_err();
    assert!(validation_message(err).contains("character"));
}

#[test]
fn highlights_are_passages_without_code_and_become_coded_by_adding_one() {
    let (s, _d) = ready();
    let (h, _) = s.create_passage(&span(U2, 4, 8), ACTOR).unwrap();
    let hl = s.highlights(Some(ID)).unwrap();
    assert_eq!(hl.len(), 1);
    assert_eq!(hl[0].passage_id, h.passage_id);
    let coded_now = s
        .code_passage(&h.passage_id, "TECH-ADV", "", ACTOR)
        .unwrap();
    assert_eq!(coded_now.kind, "coded");
    assert!(s.highlights(Some(ID)).unwrap().is_empty());
    // Removing the last code makes it a highlight again; the history stays.
    let back = s
        .uncode_passage(&h.passage_id, "TECH-ADV", "erreur", ACTOR)
        .unwrap();
    assert_eq!(back.kind, "highlight");
    assert_eq!(s.highlights(Some(ID)).unwrap().len(), 1);
    let n_rows: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM passage_codings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n_rows, 1, "the removed coding is kept");
}

#[test]
fn an_interviewer_unit_can_be_neither_coded_nor_highlighted_nor_parked() {
    let (s, _d) = ready();
    s.save_segmentation(&simple_seg(), ACTOR).unwrap();
    let interviewer = format!("{ID}-U001");
    for result in [
        s.create_passage(&whole(&interviewer), ACTOR).map(|_| ()),
        s.code_span(&whole(&interviewer), "TECH-ADV", "", ACTOR)
            .map(|_| ()),
        s.code_span(&whole(&interviewer), "LATER", "", ACTOR)
            .map(|_| ()),
    ] {
        let m = validation_message(result.unwrap_err());
        assert!(m.to_lowercase().contains("interviewer"), "{m}");
    }
    assert!(s.passages(&PassageFilter::default()).unwrap().is_empty());
}

fn simple_seg() -> Segmentation {
    let list = [
        (SpeakerRole::Interviewer, "So tell me about the pilot."),
        (SpeakerRole::Respondent, "The tags paid back within a year."),
        (
            SpeakerRole::Respondent,
            "Our staff had no RFID skills at all.",
        ),
    ];
    let units: Vec<Unit> = list
        .iter()
        .enumerate()
        .map(|(i, (role, text))| Unit {
            unit_id: format!("{ID}-U{:03}", i + 1),
            seq: i as u32 + 1,
            speaker_raw: role.short().to_string(),
            role: *role,
            timestamp: format!("00:0{i}"),
            text: text.to_string(),
            corrected: false,
        })
        .collect();
    Segmentation {
        interview_id: ID.into(),
        source_rel_path: "02_transcript_cleaned_diarised.md".into(),
        source_sha256: "aa".into(),
        rule: SEGMENTATION_RULE.into(),
        hash: Segmentation::units_hash(&units),
        units,
        warnings: vec![],
    }
}

#[test]
fn a_unit_corrected_to_an_interviewer_turn_can_no_longer_be_coded() {
    let (s, _d) = ready();
    s.correct_unit(
        ID,
        U2,
        "The tags paid back within a year.",
        SpeakerRole::Interviewer,
        "mauvais locuteur",
        ACTOR,
    )
    .unwrap();
    assert!(s.create_passage(&whole(U2), ACTOR).is_err());
}

#[test]
fn later_parks_a_passage_with_a_note_and_is_listed_per_interview() {
    let (s, _d) = ready();
    let (p, _) = s
        .code_span(&span(U2, 4, 8), "LATER", "hésitation sur le sens", ACTOR)
        .unwrap();
    assert_eq!(p.codings[0].code_id, "LATER");
    assert!(p.codings[0].system);
    assert_eq!(p.codings[0].note, "hésitation sur le sens");
    // By its display name too.
    s.code_span(&whole(U3), "plus tard", "", ACTOR).unwrap();
    let items = s.later_items(Some(ID)).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(s.later_summary().unwrap(), vec![(ID.to_string(), 2)]);
    // Resolved: the marker is removed, the passage stays.
    s.uncode_passage(&p.passage_id, "LATER", "traité", ACTOR)
        .unwrap();
    assert_eq!(s.later_items(Some(ID)).unwrap().len(), 1);
    assert_eq!(s.later_summary().unwrap(), vec![(ID.to_string(), 1)]);
}

#[test]
fn later_is_excluded_from_every_count() {
    let (s, _d) = ready();
    s.code_span(&span(U2, 4, 8), "LATER", "", ACTOR).unwrap();
    s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    let nodes = s.code_nodes().unwrap();
    let later = nodes.iter().find(|n| n.system).unwrap();
    assert_eq!(later.counts.passages, 0);
    assert_eq!(later.counts.passages_rollup, 0);
    let adv = nodes.iter().find(|n| n.token == "TECH-ADV").unwrap();
    assert_eq!(adv.counts.passages, 1);
    let total: usize = nodes.iter().map(|n| n.counts.passages).sum();
    assert_eq!(total, 1);
}

#[test]
fn counts_separate_passages_from_interviews() {
    let (s, _d) = ready();
    add_interview(&s, "ECO-17", 3);
    s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&span(U3, 0, 3), "TECH-ADV", "", ACTOR).unwrap();
    for u in 1..=2 {
        let mut np = whole(&format!("ECO-17-U{u:03}"));
        np.interview_id = "ECO-17".into();
        s.code_span(&np, "TECH-ADV", "", ACTOR).unwrap();
    }
    let adv = s.code_node("TECH-ADV").unwrap();
    assert_eq!(adv.counts.passages, 4);
    assert_eq!(adv.counts.interviews, 2);
}

#[test]
fn a5_a_child_counts_for_its_parent_and_moving_changes_no_passage() {
    let (s, _d) = ready();
    add_interview(&s, "ECO-17", 2);
    let a = code_id(&s, "TECH-ADV");
    let b = code_id(&s, "ORG-SKILLS");
    s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&span(U3, 0, 3), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    let mut np = whole("ECO-17-U001");
    np.interview_id = "ECO-17".into();
    s.code_span(&np, "ORG-SKILLS", "", ACTOR).unwrap();
    let before: Vec<_> = s
        .passages(&PassageFilter {
            code: Some("ORG-SKILLS".into()),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .map(|p| p.passage_id)
        .collect();
    assert_eq!(before.len(), 2);
    s.move_code(&b, Some(&a), "regroupement", ACTOR).unwrap();
    let after: Vec<_> = s
        .passages(&PassageFilter {
            code: Some("ORG-SKILLS".into()),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .map(|p| p.passage_id)
        .collect();
    assert_eq!(
        before, after,
        "number and ids of the passages of B are unchanged"
    );
    let nb = s.code_node(&b).unwrap();
    assert_eq!(nb.counts.passages, 2);
    let na = s.code_node(&a).unwrap();
    assert_eq!(na.counts.passages, 1, "direct");
    assert_eq!(na.counts.passages_rollup, 3, "includes the passages of B");
    assert_eq!(na.counts.interviews_rollup, 2);
    assert_eq!(na.counts.interviews, 1);
}

#[test]
fn a_passage_carrying_a_parent_and_its_child_is_counted_once_in_the_rollup() {
    let (s, _d) = ready();
    let a = code_id(&s, "TECH-ADV");
    let b = code_id(&s, "ORG-SKILLS");
    s.move_code(&b, Some(&a), "r", ACTOR).unwrap();
    s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&span(U2, 4, 8), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    assert_eq!(s.code_node(&a).unwrap().counts.passages_rollup, 1);
}

#[test]
fn a3_merge_keeps_every_source_and_deduplicates_identical_passages() {
    let (s, _d) = ready();
    add_interview(&s, "ECO-17", 2);
    add_interview(&s, "ECO-18", 2);
    // A: 2 passages in ECO-16 and 1 in ECO-17. B: 1 in ECO-16 (shared with A),
    // 1 in ECO-17, 2 in ECO-18.
    let np = |i: &str, u: usize| {
        let mut n = whole(&format!("{i}-U{u:03}"));
        n.interview_id = i.into();
        n
    };
    s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&span(U3, 0, 3), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&np("ECO-17", 1), "TECH-ADV", "", ACTOR)
        .unwrap();
    s.code_span(&span(U2, 4, 8), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    s.code_span(&np("ECO-17", 2), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    s.code_span(&np("ECO-18", 1), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    s.code_span(&np("ECO-18", 2), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    let a = s.code_node("TECH-ADV").unwrap();
    let b = s.code_node("ORG-SKILLS").unwrap();
    assert_eq!((a.counts.passages, a.counts.interviews), (3, 2));
    assert_eq!((b.counts.passages, b.counts.interviews), (4, 3));
    let out = s
        .merge_code("ORG-SKILLS", "TECH-ADV", "synonymes", ACTOR)
        .unwrap();
    assert_eq!(out.deduplicated_passage_codings, 1);
    assert_eq!(out.moved_passage_codings, 3);
    let merged = s.code_node("TECH-ADV").unwrap();
    assert_eq!(merged.counts.passages, 6);
    assert_eq!(merged.counts.interviews, 3, "recomputed from distinct ids");
    // The 7 sources are traceable: the removed duplicate keeps its reason.
    let reason: String = s
        .connection()
        .query_row(
            "SELECT remove_reason FROM passage_codings WHERE removed_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "merged");
    let n_rows: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM passage_codings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n_rows, 7);
    // And the old code name finds the merged code.
    let found = s
        .passages(&PassageFilter {
            code: Some("ORG-SKILLS".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(found.len(), 6);
}

#[test]
fn a_changed_unit_text_flags_the_passage_stale_and_never_moves_it() {
    let (s, _d) = ready();
    let (p, _) = s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    assert!(!s.passage(&p.passage_id).unwrap().stale);
    s.correct_unit(
        ID,
        U2,
        "Then the tags paid back within a year.",
        SpeakerRole::Respondent,
        "mot manquant",
        ACTOR,
    )
    .unwrap();
    let after = s.passage(&p.passage_id).unwrap();
    assert!(after.stale);
    assert_eq!(after.stale_reason.as_deref(), Some("text_changed"));
    assert_eq!((after.start, after.end), (4, 8));
    assert_eq!(after.text, "tags", "the stored text stays readable");
    assert_eq!(after.before, "");
    assert!(after.start_utf16.is_none());
    // The stale passage is still listed and still counted.
    assert_eq!(s.code_node("TECH-ADV").unwrap().counts.passages, 1);
}

#[test]
fn a_new_passage_on_a_corrected_unit_is_anchored_on_the_corrected_text() {
    let (s, _d) = ready();
    s.correct_unit(
        ID,
        U2,
        "Then the tags paid back.",
        SpeakerRole::Respondent,
        "r",
        ACTOR,
    )
    .unwrap();
    let (p, _) = s.create_passage(&span(U2, 9, 13), ACTOR).unwrap();
    assert_eq!(p.text, "tags");
    assert!(!p.stale);
}

#[test]
fn a_unit_that_left_the_segmentation_flags_the_passage() {
    let (s, _d) = ready();
    let (p, _) = s.create_passage(&whole(U3), ACTOR).unwrap();
    let mut seg = segmentation(&[
        "So tell me about the pilot.",
        "The tags paid back within a year.",
        "Our staff had no RFID skills at all.",
        "Added later.",
    ]);
    seg.units.remove(2);
    seg.hash = Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
    let after = s.passage(&p.passage_id).unwrap();
    assert!(after.stale);
    assert_eq!(after.stale_reason.as_deref(), Some("unit_removed"));
}

#[test]
fn re_segmenting_without_changing_a_unit_keeps_its_passage_valid() {
    let (s, _d) = ready();
    let (p, _) = s.create_passage(&span(U2, 4, 8), ACTOR).unwrap();
    let seg = segmentation(&[
        "So tell me about the pilot.",
        "The tags paid back within a year.",
        "Our staff had no RFID skills at all.",
        "Added later.",
    ]);
    s.save_segmentation(&seg, ACTOR).unwrap();
    let after = s.passage(&p.passage_id).unwrap();
    assert!(!after.stale);
    assert_eq!(after.segmentation_hash.len(), 64);
}

#[test]
fn deleting_a_passage_is_logical_and_audited() {
    let (s, _d) = ready();
    let (p, _) = s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    s.delete_passage(&p.passage_id, "sélection erronée", ACTOR)
        .unwrap();
    assert!(s.passages(&PassageFilter::default()).unwrap().is_empty());
    assert_eq!(s.code_node("TECH-ADV").unwrap().counts.passages, 0);
    assert!(s.passage(&p.passage_id).is_err());
    let n: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM passages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    assert!(s
        .audit_events(None)
        .unwrap()
        .iter()
        .any(|e| e.action == "passage.delete"));
    assert!(s.delete_passage(&p.passage_id, "x", ACTOR).is_err());
    // The row cannot be deleted at the database level.
    assert!(s.connection().execute("DELETE FROM passages", []).is_err());
}

#[test]
fn a_deactivated_code_cannot_be_applied_and_a_merged_name_resolves_to_its_target() {
    let (s, _d) = ready();
    s.set_code_active("ENV-PARTNER", false, ACTOR).unwrap();
    let m = validation_message(
        s.code_span(&span(U2, 4, 8), "ENV-PARTNER", "", ACTOR)
            .unwrap_err(),
    );
    assert!(m.contains("inactive"), "{m}");
    s.merge_code("ORG-SKILLS", "TECH-ADV", "r", ACTOR).unwrap();
    let (p, _) = s
        .code_span(&span(U2, 4, 8), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    assert_eq!(p.codings[0].code_name, "TECH-ADV");
    assert!(s.code_span(&span(U2, 4, 8), "NOPE", "", ACTOR).is_err());
}

#[test]
fn every_passage_write_is_audited_and_the_chain_holds() {
    let (s, _d) = ready();
    let (p, _) = s.create_passage(&span(U2, 4, 8), ACTOR).unwrap();
    s.code_passage(&p.passage_id, "TECH-ADV", "", ACTOR)
        .unwrap();
    s.uncode_passage(&p.passage_id, "TECH-ADV", "r", ACTOR)
        .unwrap();
    s.delete_passage(&p.passage_id, "r", ACTOR).unwrap();
    let actions: Vec<String> = s
        .audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    for a in [
        "passage.create",
        "passage.code",
        "passage.uncode",
        "passage.delete",
    ] {
        assert!(actions.contains(&a.to_string()), "{a}");
    }
    assert!(s.verify_chain().unwrap().ok);
}
