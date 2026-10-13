//! V1 lot 4 (Review), store side: the unit coding written from the passages,
//! candidate concepts, the pseudonymisation list, the harmonisation worklist,
//! the review state.

use super::*;
use crate::codetree::{CodeUpdate, NewCode};
use crate::lot4::{ConceptDecisionRequest, SOURCE_PASSAGES};
use crate::passages::NewPassage;
use cs_model::passage::OffsetUnit;
use cs_model::pseudonym::{PseudonymConfig, PseudonymEntry};
use cs_model::{CandidateConcept, CandidateEvidence};

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

fn unit_codes(s: &Store, unit: &str) -> Option<CodeSet> {
    s.human_codings(ID, 2)
        .unwrap()
        .remove(unit)
        .map(|c| c.codes)
}

fn source_of(s: &Store, unit: &str) -> Option<String> {
    s.connection()
        .query_row(
            "SELECT source FROM human_codings WHERE interview_id = ?1 AND phase = 2 AND unit_id = ?2",
            params![ID, unit],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_unit_is_coded_with_x_when_any_of_its_passages_has_x() {
    let (s, _d) = ready();
    assert_eq!(unit_codes(&s, U2), None);

    // A passage inside the unit codes the unit.
    let (p1, _) = s.code_span(&span(U2, 4, 8), "TECH-ADV", "", ACTOR).unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["TECH-ADV"])));
    assert_eq!(source_of(&s, U2).as_deref(), Some(SOURCE_PASSAGES));

    // A second passage of the unit adds its code, a third with the same code adds nothing.
    let (p2, _) = s
        .code_span(&span(U2, 9, 13), "ORG-SKILLS", "", ACTOR)
        .unwrap();
    s.code_span(&span(U2, 14, 18), "TECH-ADV", "", ACTOR)
        .unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["ORG-SKILLS", "TECH-ADV"])));

    // A whole-unit passage writes the unit coding too.
    s.code_span(&whole(U3), "ENV-PARTNER", "", ACTOR).unwrap();
    assert_eq!(unit_codes(&s, U3), Some(set(&["ENV-PARTNER"])));

    // Removing a code from a passage takes it out of the unit when no other passage has it.
    s.uncode_passage(&p2.passage_id, "ORG-SKILLS", "misread", ACTOR)
        .unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["TECH-ADV"])));
    // The code stays while another passage of the unit carries it.
    s.uncode_passage(&p1.passage_id, "TECH-ADV", "misread", ACTOR)
        .unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["TECH-ADV"])));

    // Deleting the last passage that has the code clears the coding made from passages.
    let left = s
        .passages(&crate::passages::PassageFilter {
            unit_id: Some(U2.into()),
            code: Some("TECH-ADV".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(left.len(), 1);
    s.delete_passage(&left[0].passage_id, "wrong span", ACTOR)
        .unwrap();
    assert_eq!(unit_codes(&s, U2), None);

    // Every change is audited.
    assert!(actions(&s).iter().any(|a| a == "coding.sync_passages"));
}

#[test]
fn a_coding_by_hand_is_not_removed_by_the_passages() {
    let (s, _d) = ready();
    phase1(&s);
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    // No passage on U2: the hand coding stays when another unit is synced.
    s.code_span(&whole(U3), "ORG-SKILLS", "", ACTOR).unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["TECH-ADV"])));
    assert_eq!(source_of(&s, U2), None);

    // A passage that carries the same code leaves the hand coding as it is.
    let (p, _) = s.code_span(&whole(U2), "TECH-ADV", "", ACTOR).unwrap();
    assert_eq!(source_of(&s, U2), None);
    // Taking the code off the passage does not erase what the candidate wrote.
    s.uncode_passage(&p.passage_id, "TECH-ADV", "test", ACTOR)
        .unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["TECH-ADV"])));

    // The coding made from passages is replaced by a hand edit, which then owns it.
    s.set_human_coding(ID, 2, &hc(U3, &["NA"]), ACTOR).unwrap();
    assert_eq!(unit_codes(&s, U3), Some(set(&["NA"])));
    assert_eq!(source_of(&s, U3), None);
}

#[test]
fn the_unit_coding_made_from_passages_is_flagged_after_an_ai_answer() {
    let (s, _d) = ready();
    challenged(&s);
    assert!(!s.human_codings(ID, 2).unwrap()[U2].revised_after_ai);
    s.code_span(&whole(U2), "ORG-SKILLS", "", ACTOR).unwrap();
    let got = &s.human_codings(ID, 2).unwrap()[U2];
    assert_eq!(got.codes, set(&["ORG-SKILLS"]));
    assert!(got.revised_after_ai);
}

#[test]
fn a_merge_moves_the_passage_codings_and_asks_for_a_reread() {
    let (s, _d) = ready();
    s.code_span(&whole(U2), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&whole(U3), "ORG-SKILLS", "", ACTOR).unwrap();
    s.merge_code("TECH-ADV", "ORG-SKILLS", "same idea", ACTOR)
        .unwrap();
    assert_eq!(unit_codes(&s, U2), Some(set(&["ORG-SKILLS"])));
    assert_eq!(unit_codes(&s, U3), Some(set(&["ORG-SKILLS"])));
    let items = s.harmonisation(Some(ID)).unwrap();
    let units: Vec<&str> = items.iter().map(|i| i.unit_id.as_str()).collect();
    assert_eq!(units, vec![U2], "only the unit whose coding changed");
    assert_eq!(items[0].cause, "merge");
    assert_eq!(items[0].status, "open");
    assert_eq!(items[0].code_name, "ORG-SKILLS");
    assert_eq!(items[0].text, "The tags paid back within a year.");
    assert_eq!(items[0].codes[0].token, "ORG-SKILLS");
    assert_eq!(s.harmonisation_counts(ID).unwrap(), (1, 0));
}

#[test]
fn a_redefinition_and_a_new_sub_code_open_items_but_a_rename_does_not() {
    let (s, _d) = ready();
    s.code_span(&whole(U2), "TECH-ADV", "", ACTOR).unwrap();
    s.code_span(&whole(U3), "TECH-ADV", "", ACTOR).unwrap();
    let id = s.code_node("TECH-ADV").unwrap().code_id;

    s.update_code(
        &id,
        &CodeUpdate {
            name: Some("Avantage perçu".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    assert!(
        s.harmonisation(None).unwrap().is_empty(),
        "a rename is not a redefinition"
    );

    s.update_code(
        &id,
        &CodeUpdate {
            definition: Some("Un avantage mesuré, pas seulement perçu.".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    let items = s.harmonisation(Some(ID)).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items
        .iter()
        .all(|i| i.cause == "redefine" && i.code_id == id));

    // A second redefinition does not duplicate the open items.
    s.update_code(
        &id,
        &CodeUpdate {
            excludes: Some("le coût".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    assert_eq!(s.harmonisation(Some(ID)).unwrap().len(), 2);

    // Resolve one: confirmed, with a note; a second resolution is refused.
    let first = items[0].item_id;
    let done = s
        .resolve_harmonisation(ID, first, "confirmed", Some("relu"), ACTOR)
        .unwrap();
    assert_eq!(done.status, "done");
    assert_eq!(done.resolution.as_deref(), Some("confirmed"));
    let e = s
        .resolve_harmonisation(ID, first, "corrected", None, ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "guard");
    let e = s
        .resolve_harmonisation(ID, items[1].item_id, "later", None, ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    let e = s
        .resolve_harmonisation(ID, 9999, "confirmed", None, ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "not_found");
    assert_eq!(s.harmonisation_counts(ID).unwrap(), (1, 1));
    // Open items come first.
    assert_eq!(s.harmonisation(Some(ID)).unwrap()[0].status, "open");

    // A new sub-code asks for a reread of the units of its parent, once resolved items are closed.
    s.create_code_v1(
        &NewCode {
            name: "Avantage financier".into(),
            definition: "Un gain en euros.".into(),
            parent_id: Some(id.clone()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    let items = s.harmonisation(Some(ID)).unwrap();
    assert_eq!(
        items.len(),
        3,
        "the resolved unit is open again for the new cause"
    );
    assert!(items
        .iter()
        .any(|i| i.cause == "new_subcode" && i.status == "open"));
}

fn candidate(label: &str, units: &[&str]) -> CandidateConcept {
    CandidateConcept {
        label: label.into(),
        description: Some("Un tiers impose.".into()),
        unit_ids: units.iter().map(|u| u.to_string()).collect(),
        evidence: vec![CandidateEvidence {
            unit_id: units[0].into(),
            quote: "The tags paid back".into(),
        }],
    }
}

fn with_candidates(s: &Store) -> i64 {
    coded(s);
    batch(s);
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let mut out = output(s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["ORG-SKILLS"])]);
    out.candidate_concepts = vec![
        candidate("Pression du partenaire", &[U2]),
        candidate("Dette technique", &[U3]),
    ];
    s.record_ai_output(run, "raw answer", &out, ACTOR).unwrap();
    run
}

#[test]
fn candidate_concepts_are_stored_with_the_answer_and_change_no_coding() {
    let (s, _d) = ready();
    let before_codes = {
        coded(&s);
        s.human_codings(ID, 2).unwrap()
    };
    let n_before = s.code_tree().unwrap().codes.len();
    with_candidates(&s);
    assert_eq!(s.human_codings(ID, 2).unwrap(), before_codes);
    assert_eq!(s.code_tree().unwrap().codes.len(), n_before);

    let list = s.concept_candidates(ID).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].candidate_id, "CC-0001");
    assert_eq!(list[0].label, "Pression du partenaire");
    assert_eq!(list[0].run_label.as_deref(), Some("R1"));
    assert_eq!(list[0].unit_ids, vec![U2.to_string()]);
    assert!(list
        .iter()
        .all(|c| c.decision.is_none() && c.history.is_empty()));
    assert!(actions(&s).iter().any(|a| a == "concept.candidate.record"));
}

#[test]
fn a_candidate_is_decided_with_a_dated_reason_and_the_history_is_kept() {
    let (s, _d) = ready();
    with_candidates(&s);
    let req = |action: &str, rationale: &str| ConceptDecisionRequest {
        action: action.into(),
        rationale: rationale.into(),
        ..Default::default()
    };

    // No reason, an unknown action, an unknown candidate.
    for (cid, r) in [
        ("CC-0001", req("reject", "  ")),
        ("CC-0001", req("delete", "x")),
        ("CC-0001", req("create", "x")),
        ("CC-0001", req("map", "x")),
    ] {
        let e = s.decide_concept_candidate(ID, cid, &r, ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{r:?}");
    }
    let e = s
        .decide_concept_candidate(ID, "CC-0099", &req("reject", "x"), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "not_found");
    assert!(s.concept_candidates(ID).unwrap()[0].decision.is_none());

    // Reject, then change one's mind: attach to an existing code.
    let v = s
        .decide_concept_candidate(ID, "CC-0001", &req("reject", "trop étroit"), ACTOR)
        .unwrap();
    assert_eq!(v.decision.as_ref().unwrap().action, "reject");
    assert!(!v.decision.as_ref().unwrap().decided_at.is_empty());
    let id = s.code_node("ENV-PARTNER").unwrap().code_id;
    let v = s
        .decide_concept_candidate(
            ID,
            "CC-0001",
            &ConceptDecisionRequest {
                action: "map".into(),
                code_id: Some(id.clone()),
                rationale: "c'est le partenaire".into(),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(
        v.decision.as_ref().unwrap().code_id.as_deref(),
        Some(id.as_str())
    );
    assert_eq!(v.history.len(), 2, "the earlier decision stays on record");
    assert_eq!(v.history[0].action, "reject");

    // Attaching to the reserved code or to an inactive code is refused.
    let e = s
        .decide_concept_candidate(
            ID,
            "CC-0002",
            &ConceptDecisionRequest {
                action: "map".into(),
                code_id: Some("LATER".into()),
                rationale: "x".into(),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "validation");

    // Create: a new code with the label chosen, no coding written.
    let units_before = s.human_codings(ID, 2).unwrap();
    let v = s
        .decide_concept_candidate(
            ID,
            "CC-0002",
            &ConceptDecisionRequest {
                action: "create".into(),
                new_code_name: Some("Dette technique".into()),
                rationale: "absente du codebook".into(),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap();
    let created = v.decision.as_ref().unwrap().code_id.clone().unwrap();
    let node = s.code_node(&created).unwrap();
    assert_eq!(node.name, "Dette technique");
    assert_eq!(node.definition, "Un tiers impose.");
    assert_eq!(
        node.counts.passages, 0,
        "no passage is coded by the decision"
    );
    assert_eq!(s.human_codings(ID, 2).unwrap(), units_before);
    // A candidate creates one code at most.
    let e = s
        .decide_concept_candidate(
            ID,
            "CC-0002",
            &ConceptDecisionRequest {
                action: "create".into(),
                new_code_name: Some("Autre".into()),
                rationale: "encore".into(),
                ..Default::default()
            },
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "guard");
    // The decision log is audited and the tables are append-only.
    assert!(actions(&s).iter().any(|a| a == "concept.candidate.decide"));
    assert!(s
        .connection()
        .execute("UPDATE concept_decisions SET rationale = 'x'", [])
        .is_err());
    assert!(s
        .connection()
        .execute("DELETE FROM concept_candidates", [])
        .is_err());
}

fn list() -> PseudonymConfig {
    PseudonymConfig {
        enabled: true,
        entries: vec![
            PseudonymEntry {
                kind: "person".into(),
                original: "Anna".into(),
                pseudonym: "P1".into(),
            },
            PseudonymEntry {
                kind: "organisation".into(),
                original: "RFID".into(),
                pseudonym: "ORG1".into(),
            },
        ],
    }
}

#[test]
fn the_pseudonym_list_is_stored_and_the_audit_keeps_its_digest_not_its_names() {
    let (s, _d) = ready();
    assert_eq!(s.pseudonym_config().unwrap(), PseudonymConfig::default());
    assert!(s.pseudonym_usage(ID).unwrap().is_empty());

    let stored = s.set_pseudonym_config(&list(), ACTOR).unwrap();
    assert_eq!(s.pseudonym_config().unwrap(), stored);
    let ev = s
        .audit_events(None)
        .unwrap()
        .into_iter()
        .find(|e| e.action == "pseudonymisation.update")
        .unwrap();
    assert_eq!(ev.detail["digest_after"], stored.digest().as_str());
    assert_eq!(ev.detail["n_entries_after"], 2);
    let text = serde_json::to_string(&ev.detail).unwrap();
    assert!(!text.contains("Anna") && !text.contains("ORG1"));

    // Unchanged: no new event.
    let n = s.audit_events(None).unwrap().len();
    s.set_pseudonym_config(&list(), ACTOR).unwrap();
    assert_eq!(s.audit_events(None).unwrap().len(), n);

    // Usage in the interview: "RFID" occurs in U003.
    let usage = s.pseudonym_usage(ID).unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].original, "RFID");
    assert_eq!(usage[0].count, 1);
    assert_eq!(usage[0].pseudonym, "ORG1");

    // Off: nothing is replaced, and the digest is the empty list's.
    let mut off = list();
    off.enabled = false;
    s.set_pseudonym_config(&off, ACTOR).unwrap();
    assert!(s.pseudonym_usage(ID).unwrap().is_empty());

    // An invalid list is refused whole.
    let mut bad = list();
    bad.entries[1].original = "anna".into();
    let e = s.set_pseudonym_config(&bad, ACTOR).unwrap_err();
    assert_eq!(kind(&e), "validation");
}

#[test]
fn the_review_state_declares_what_was_covered() {
    let (s, _d) = ready();
    let st = s.review_state(ID).unwrap();
    assert_eq!(st.state, "not_done");
    assert_eq!(st.n_relevant, 3);
    assert!(st.declaration.contains("Pas de revue"));

    // A batch exists: in progress.
    coded(&s);
    batch(&s);
    let st = s.review_state(ID).unwrap();
    assert_eq!(st.state, "in_progress");
    assert_eq!(st.n_relevant, 2);
    assert_eq!(st.n_covered, 0);

    // A complete run with a disagreement and two candidates: still in progress.
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let mut out = output(&s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["ENV-PARTNER"])]);
    out.candidate_concepts = vec![candidate("Pression", &[U2])];
    s.record_ai_output(run, "raw", &out, ACTOR).unwrap();
    let st = s.review_state(ID).unwrap();
    assert_eq!(st.state, "in_progress");
    assert_eq!(st.n_covered, 2);
    assert_eq!(st.coverage, 1.0);
    assert_eq!(st.runs, vec!["R1".to_string()]);
    assert_eq!(st.n_disagreements, 1);
    assert_eq!(st.n_candidates, 1);
    assert!(st.declaration.contains("en cours"));

    // Decide everything: the disagreement, the sample, the candidate.
    s.record_decision(
        ID,
        &decision(
            U3,
            ArbitrationChoice::Human,
            &["ORG-SKILLS"],
            "ma lecture",
        ),
        ACTOR,
    )
    .unwrap();
    for item in s.review(ID).unwrap().items {
        if item.item.kind == ReviewKind::AgreementSample && item.item.decision.is_none() {
            s.record_decision(
                ID,
                &decision(
                    &item.item.unit_id,
                    ArbitrationChoice::Human,
                    &["TECH-ADV"],
                    "ok",
                ),
                ACTOR,
            )
            .unwrap();
        }
    }
    assert_eq!(s.review_state(ID).unwrap().state, "in_progress");
    s.decide_concept_candidate(
        ID,
        "CC-0001",
        &ConceptDecisionRequest {
            action: "reject".into(),
            rationale: "hors sujet".into(),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    let st = s.review_state(ID).unwrap();
    assert_eq!(st.state, "complete");
    assert!(st.declaration.contains("R1") && !st.declaration.contains('\u{2014}'));

    let corpus = s.corpus_review_state().unwrap();
    assert_eq!((corpus.n_interviews, corpus.n_complete), (1, 1));
    assert!(corpus.declaration.contains(ID));
}

#[test]
fn the_prompt_event_comes_after_the_frozen_reference_and_names_its_digests() {
    let (s, _d) = ready();
    coded(&s);
    let text = "interview_id: ECO-16\nconfiguration_digest: abc\npseudonymisation_digest: def\n\n## Units\n";
    let mut p = part(&s, &[U2, U3], &OFFERED, 1, 1);
    p.text = text.into();
    p.sha256 = sha256_hex(text.as_bytes());
    s.record_prompts(&[p], ACTOR).unwrap();
    let events = s.audit_events(None).unwrap();
    let take = events
        .iter()
        .position(|e| e.action == "human_snapshot.take")
        .unwrap();
    let gen = events
        .iter()
        .position(|e| e.action == "prompt.generate")
        .unwrap();
    assert!(
        take < gen,
        "the human coding is frozen before the prompt is recorded"
    );
    assert_eq!(events[gen].detail["configuration_digest"], "abc");
    assert_eq!(events[gen].detail["pseudonymisation_digest"], "def");
    assert_eq!(
        events[gen].detail["human_snapshot_sha256"],
        events[take].detail["sha256"]
    );
}
