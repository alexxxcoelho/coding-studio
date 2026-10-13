//! F-24: transcript versions and the projection of passages onto a new text.

use super::*;
use crate::passages::NewPassage;
use crate::transcripts::{Resolution, ReviewFilter};
use cs_model::passage::{text_digest, OffsetUnit};

const T2: &str = "The tags paid back within a year.";

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

fn correct(s: &Store, unit: &str, text: &str) {
    s.correct_unit(ID, unit, text, SpeakerRole::Respondent, "typo", ACTOR)
        .unwrap();
}

fn open_items(s: &Store) -> Vec<crate::ProjectionItem> {
    s.transcript_review(&ReviewFilter {
        interview_id: Some(ID.into()),
        status: None,
    })
    .unwrap()
}

/// The span "paid back" of U2, coded TECH-ADV.
fn coded_paid_back(s: &Store) -> String {
    let a = T2.find("paid back").unwrap();
    let (p, _) = s
        .code_span(&span(U2, a, a + 9), "TECH-ADV", "", ACTOR)
        .unwrap();
    p.passage_id
}

fn resegment(s: &Store, texts: &[&str]) {
    s.save_segmentation(&segmentation(texts), ACTOR).unwrap();
}

#[test]
fn the_first_segmentation_is_version_one_and_a_correction_makes_version_two() {
    let (s, _d) = ready();
    assert_eq!(s.current_transcript_version(ID).unwrap(), Some(1));
    correct(&s, U2, "The tags paid back within two years.");
    let v = s.transcript_versions(ID).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].cause, "segmentation");
    assert_eq!(v[1].cause, "correction");
    assert_eq!(v[1].target.as_deref(), Some(U2));
    assert_eq!(v[1].reason.as_deref(), Some("typo"));
    assert_ne!(v[0].text_sha256, v[1].text_sha256);
}

#[test]
fn the_previous_version_stays_readable() {
    let (s, _d) = ready();
    correct(&s, U2, "The tags paid back within two years.");
    let v1 = s.transcript_units_at(ID, 1).unwrap();
    let v2 = s.transcript_units_at(ID, 2).unwrap();
    assert_eq!(v1[1].text, T2);
    assert_eq!(v2[1].text, "The tags paid back within two years.");
    assert_eq!(v1[0].text, v2[0].text);
    assert!(s.transcript_units_at(ID, 9).is_err());
}

#[test]
fn a_version_is_not_made_when_nothing_changes() {
    let (s, _d) = ready();
    let seg = s.segmentation(ID).unwrap().unwrap();
    s.save_segmentation(&seg, ACTOR).unwrap();
    assert_eq!(s.transcript_versions(ID).unwrap().len(), 1);
}

#[test]
fn a_database_without_versions_gets_a_baseline_before_the_first_change() {
    let (s, _d) = ready();
    s.connection()
        .execute_batch(
            "DROP TRIGGER transcript_versions_no_delete; DELETE FROM transcript_versions;",
        )
        .unwrap();
    assert_eq!(s.current_transcript_version(ID).unwrap(), None);
    let pid = coded_paid_back(&s);
    correct(&s, U2, "Well, the tags paid back within a year.");
    let v = s.transcript_versions(ID).unwrap();
    assert_eq!(
        v.iter().map(|v| v.cause.as_str()).collect::<Vec<_>>(),
        ["baseline", "correction"]
    );
    assert!(!s.passage(&pid).unwrap().stale);
}

#[test]
fn an_edit_outside_the_passage_re_anchors_it_automatically() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    let before = s.passage(&pid).unwrap();
    correct(&s, U2, "Well, the tags paid back within a year.");
    let p = s.passage(&pid).unwrap();
    assert!(!p.stale);
    assert_eq!(p.text, "paid back");
    assert_eq!(p.start, before.start + 6);
    assert_eq!(p.end, before.end + 6);
    assert_eq!(
        p.anchor_digest,
        text_digest("Well, the tags paid back within a year.")
    );
    assert!(open_items(&s).is_empty());
    let done = s
        .transcript_review(&ReviewFilter {
            interview_id: Some(ID.into()),
            status: Some("reanchored".into()),
        })
        .unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].old.start, before.start);
    assert_eq!(done[0].final_span.as_ref().unwrap().start, p.start);
    assert_eq!(s.transcript_versions(ID).unwrap()[1].reanchored, 1);
    let events = s.audit_events(Some(ID)).unwrap();
    assert!(events.iter().any(|e| e.action == "passage.reanchor"));
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn a_passage_in_another_unit_is_left_alone() {
    let (s, _d) = ready();
    let (p3, _) = s
        .code_span(&span(U3, 0, 9), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    correct(&s, U2, "Well, the tags paid back within a year.");
    assert_eq!(s.passage(&p3.passage_id).unwrap(), p3);
    assert!(s
        .transcript_review(&ReviewFilter {
            status: Some("all".into()),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
}

#[test]
fn an_edit_inside_the_passage_flags_it_with_old_and_new_text() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    correct(&s, U2, "The tags paid off within a year.");
    let p = s.passage(&pid).unwrap();
    assert!(p.stale);
    assert_eq!(p.text, "paid back");
    let items = open_items(&s);
    assert_eq!(items.len(), 1);
    let it = &items[0];
    assert_eq!(it.passage_id, pid);
    assert_eq!(it.status, "needs_review");
    assert_eq!(it.reason, "text_changed");
    assert_eq!((it.from_version, it.to_version), (1, 2));
    assert_eq!(it.old.text, "paid back");
    assert_eq!(it.old.unit_text, T2);
    let prop = it.proposed.as_ref().expect("a proposal");
    assert!(prop.text.starts_with("paid "), "{:?}", prop.text);
    assert_eq!(prop.unit_text, "The tags paid off within a year.");
    assert_eq!(
        it.codes[0].code_id,
        s.code_node("TECH-ADV").unwrap().code_id
    );
    assert_eq!(
        it.current_unit_text.as_deref(),
        Some("The tags paid off within a year.")
    );
}

#[test]
fn accepting_the_proposal_moves_the_passage_to_the_new_text() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    correct(&s, U2, "The tags paid off within a year.");
    let it = open_items(&s).remove(0);
    let done = s
        .resolve_projection(it.projection_id, &Resolution::Accept, "ok", ACTOR)
        .unwrap();
    assert_eq!(done.status, "accepted");
    let p = s.passage(&pid).unwrap();
    assert!(!p.stale);
    assert_eq!(p.text, it.proposed.unwrap().text);
    assert_eq!(p.codings.len(), 1);
    assert!(open_items(&s).is_empty());
    // A second decision on the same item is refused.
    assert!(s
        .resolve_projection(it.projection_id, &Resolution::Detach, "", ACTOR)
        .is_err());
}

#[test]
fn adjusting_takes_offsets_from_the_researcher_in_utf16_too() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    let text = "The 😀 tags paid off within a year.";
    correct(&s, U2, text);
    let it = open_items(&s).remove(0);
    let a16 = text.encode_utf16().count() - "within a year.".encode_utf16().count();
    let done = s
        .resolve_projection(
            it.projection_id,
            &Resolution::Adjust {
                unit_id: None,
                start: a16,
                end: a16 + 6,
                offset_unit: OffsetUnit::Utf16,
            },
            "by hand",
            ACTOR,
        )
        .unwrap();
    assert_eq!(done.status, "adjusted");
    let p = s.passage(&pid).unwrap();
    assert_eq!(p.text, "within");
    assert!(!p.stale);
    assert_eq!(
        p.start,
        text.chars().count() - "within a year.".chars().count()
    );
}

#[test]
fn an_adjustment_outside_the_text_is_refused_and_changes_nothing() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    correct(&s, U2, "The tags paid off within a year.");
    let it = open_items(&s).remove(0);
    let err = s
        .resolve_projection(
            it.projection_id,
            &Resolution::Adjust {
                unit_id: None,
                start: 0,
                end: 500,
                offset_unit: OffsetUnit::Scalar,
            },
            "",
            ACTOR,
        )
        .unwrap_err();
    assert!(format!("{err:#}").contains("outside"));
    assert!(s.passage(&pid).unwrap().stale);
    assert_eq!(open_items(&s).len(), 1);
}

#[test]
fn detaching_removes_the_passage_from_the_lists_but_keeps_the_record() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    correct(&s, U2, "The tags paid off within a year.");
    let it = open_items(&s).remove(0);
    s.resolve_projection(
        it.projection_id,
        &Resolution::Detach,
        "no longer there",
        ACTOR,
    )
    .unwrap();
    assert!(s.passage(&pid).is_err());
    let item = s.transcript_review_item(it.projection_id).unwrap();
    assert_eq!(item.status, "detached");
    assert_eq!(item.resolution_note.as_deref(), Some("no longer there"));
    assert_eq!(item.old.text, "paid back");
    let ev = s.audit_events(Some(ID)).unwrap();
    let detach = ev.iter().find(|e| e.action == "passage.detach").unwrap();
    assert_eq!(detach.detail["old"]["text"], "paid back");
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn a_deleted_text_gets_no_proposal() {
    let (s, _d) = ready();
    coded_paid_back(&s);
    correct(&s, U2, "The tags.");
    let it = open_items(&s).remove(0);
    assert!(it.proposed.is_none());
    let err = s
        .resolve_projection(it.projection_id, &Resolution::Accept, "", ACTOR)
        .unwrap_err();
    assert!(format!("{err:#}").contains("no proposed span"));
}

#[test]
fn a_second_edit_replaces_the_open_item_and_projects_from_the_anchored_text() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    correct(&s, U2, "The tags paid off within a year.");
    correct(&s, U2, "The tags paid off within two years.");
    let all = s
        .transcript_review(&ReviewFilter {
            interview_id: Some(ID.into()),
            status: Some("all".into()),
        })
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].status, "superseded");
    let open = open_items(&s);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].passage_id, pid);
    assert_eq!((open[0].from_version, open[0].to_version), (1, 3));
    assert_eq!(open[0].old.unit_text, T2);
}

#[test]
fn reverting_the_edit_closes_the_open_item() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    correct(&s, U2, "The tags paid off within a year.");
    correct(&s, U2, T2);
    assert!(open_items(&s).is_empty());
    assert!(!s.passage(&pid).unwrap().stale);
}

#[test]
fn a_rescan_with_a_modified_file_projects_the_passages() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    let (other, _) = s
        .code_span(&span(U3, 4, 9), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    resegment(
        &s,
        &[
            "So tell me about the pilot.",
            "Well, the tags paid back within a year.",
            "Our staff had no RFID skills at all, honestly.",
        ],
    );
    let v = s.transcript_versions(ID).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[1].cause, "segmentation");
    let p = s.passage(&pid).unwrap();
    assert!(!p.stale);
    assert_eq!(p.text, "paid back");
    assert_eq!(p.start, 15);
    let o = s.passage(&other.passage_id).unwrap();
    assert!(!o.stale);
    assert_eq!(o.start, 4);
    assert!(open_items(&s).is_empty());
}

#[test]
fn a_rescan_that_touches_the_passage_flags_it() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    resegment(
        &s,
        &[
            "So tell me about the pilot.",
            "The tags paid off within a year.",
            "Our staff had no RFID skills at all.",
        ],
    );
    assert!(s.passage(&pid).unwrap().stale);
    assert_eq!(open_items(&s).len(), 1);
}

#[test]
fn a_unit_that_moved_with_its_exact_text_carries_its_passages() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    // A new first turn pushes every unit down by one id.
    resegment(
        &s,
        &[
            "A new opening remark.",
            "So tell me about the pilot.",
            T2,
            "Our staff had no RFID skills at all.",
        ],
    );
    let p = s.passage(&pid).unwrap();
    assert!(!p.stale);
    assert_eq!(p.unit_id, "ECO-16-U003");
    assert_eq!(p.text, "paid back");
    assert!(open_items(&s).is_empty());
}

#[test]
fn the_unit_level_coding_follows_a_move() {
    let (s, _d) = ready();
    coded_paid_back(&s);
    let tokens = |unit: &str| -> String {
        s.connection()
            .query_row(
                "SELECT COALESCE((SELECT codes FROM human_codings
                   WHERE interview_id = ?1 AND phase = 2 AND unit_id = ?2), '[]')",
                [ID, unit],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(tokens(U2), "[\"TECH-ADV\"]");
    resegment(
        &s,
        &[
            "A new opening remark.",
            "So tell me about the pilot.",
            T2,
            "Our staff had no RFID skills at all.",
        ],
    );
    assert_eq!(tokens(U2), "[]");
    assert_eq!(tokens("ECO-16-U003"), "[\"TECH-ADV\"]");
}

#[test]
fn words_found_once_elsewhere_are_proposed_not_moved() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    // Three units again, but the words now sit in U003 and U002 is gone from the ids.
    resegment(
        &s,
        &[
            "So tell me about the pilot.",
            "Our staff had no RFID skills at all.",
            "Roughly speaking, the tags paid back in one year.",
        ],
    );
    let items = open_items(&s);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].passage_id, pid);
    // U002 still exists with other words: that is a diff case with a proposal in U002 or none.
    assert!(s.passage(&pid).unwrap().stale);
}

/// A segmentation with chosen unit ids (a rescan that renumbered the turns).
fn resegment_ids(s: &Store, units: &[(&str, &str)]) {
    let mut seg = segmentation(&[]);
    seg.units = units
        .iter()
        .enumerate()
        .map(|(i, (id, text))| Unit {
            unit_id: (*id).into(),
            seq: i as u32 + 1,
            speaker_raw: "RESP".into(),
            role: SpeakerRole::Respondent,
            timestamp: format!("00:0{i}"),
            text: (*text).into(),
            corrected: false,
        })
        .collect();
    seg.hash = Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
}

#[test]
fn a_passage_whose_unit_is_gone_and_words_not_found_has_no_proposal() {
    let (s, _d) = ready();
    coded_paid_back(&s);
    resegment_ids(
        &s,
        &[
            (U1, "So tell me about the pilot."),
            (U3, "Our staff had no RFID skills at all."),
        ],
    );
    let items = open_items(&s);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].reason, "unit_removed");
    assert!(items[0].proposed.is_none());
}

#[test]
fn words_found_once_in_another_unit_are_proposed_and_can_be_accepted() {
    let (s, _d) = ready();
    let pid = coded_paid_back(&s);
    resegment_ids(
        &s,
        &[
            (U1, "So tell me about the pilot."),
            ("ECO-16-U010", "Roughly speaking, the tags paid back in one year."),
            (U3, "Our staff had no RFID skills at all."),
        ],
    );
    let items = open_items(&s);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].reason, "unit_moved");
    let prop = items[0].proposed.as_ref().expect("a proposal");
    assert_eq!(prop.unit_id, "ECO-16-U010");
    assert_eq!(prop.text, "paid back");
    // Nothing moved on its own.
    assert!(s.passage(&pid).unwrap().stale);
    let done = s
        .resolve_projection(items[0].projection_id, &Resolution::Accept, "", ACTOR)
        .unwrap();
    assert_eq!(done.status, "accepted");
    let p = s.passage(&pid).unwrap();
    assert_eq!(p.unit_id, "ECO-16-U010");
    assert!(!p.stale);
}

#[test]
fn words_found_twice_are_not_guessed() {
    let (s, _d) = ready();
    coded_paid_back(&s);
    resegment_ids(
        &s,
        &[
            (U1, "It paid back. It paid back."),
            (U3, "Our staff had no RFID skills at all."),
        ],
    );
    let items = open_items(&s);
    assert_eq!(items[0].reason, "unit_removed");
    assert!(items[0].proposed.is_none());
}
