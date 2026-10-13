//! The simple view (v5): automatic phase 1 at segmentation, interviewer units
//! kept out of every phase 2 path, derived TOE contexts and the three-step
//! progress. Red until implemented.
//!
//! Contract fixed here:
//! - `save_segmentation` fills the phase 1 coding of every unit that has none:
//!   interviewer units get `NA`, respondent and unknown units get `CONTEXT`.
//!   One audit event `coding.auto_phase1` per call that fills at least one
//!   unit, `target` = interview id, detail
//!   `{n_filled, n_na, n_context, n_kept}` (`n_kept` = units that already had a
//!   phase 1 coding). No per-unit `coding.set` event.
//! - Re-segmenting keeps every existing coding (phase 1 and phase 2, human or
//!   automatic) and fills only the units that have no phase 1 coding. The
//!   lock "refused once coding exists" no longer applies to codings made by
//!   the candidate; it still applies to AI outputs.
//! - A phase 1 change of category names right after segmentation (automatic
//!   codings only) is not refused.

use super::*;
use cs_model::{ArbitrationChoice, CodingMap, ReviewKind};

const U4: &str = "ECO-16-U004";
const U5: &str = "ECO-16-U005";
const U6: &str = "ECO-16-U006";

const BASE: [(SpeakerRole, &str); 5] = [
    (SpeakerRole::Interviewer, "So tell me about the pilot."),
    (SpeakerRole::Respondent, "The tags paid back within a year."),
    (
        SpeakerRole::Respondent,
        "Our staff had no RFID skills at all.",
    ),
    (SpeakerRole::Interviewer, "And the supplier?"),
    (
        SpeakerRole::Unknown,
        "The supplier refused to tag at source.",
    ),
];

fn seg_of(list: &[(SpeakerRole, &str)]) -> Segmentation {
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

fn base() -> Segmentation {
    seg_of(&BASE)
}

fn with_extra() -> Segmentation {
    let mut list = BASE.to_vec();
    list.push((SpeakerRole::Respondent, "We pay for the tags ourselves."));
    seg_of(&list)
}

fn simple_codes() -> Vec<Upstream> {
    vec![
        ("TECH-ADV", "TECH", "codebook", 1, "SQ1"),
        ("TECH-COST", "TECH", "codebook", 1, "SQ1"),
        ("ORG-SKILLS", "ORG", "codebook", 1, "SQ2"),
        ("ENV-PARTNER", "ENV", "codebook", 1, "SQ3"),
        ("OUT-REALISED", "OUTCOME", "codebook", 1, "SQ4"),
    ]
}

/// Codebook and interview, nothing segmented yet.
fn unsegmented() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    s.import_codebook(&coding_db(&dir, &simple_codes()), ACTOR)
        .unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    (s, dir)
}

/// Segmented with `BASE`: INT, RESP, RESP, INT, UNKNOWN.
fn segmented() -> (Store, TempDir) {
    let (s, d) = unsegmented();
    s.save_segmentation(&base(), ACTOR).unwrap();
    (s, d)
}

fn p1(s: &Store) -> CodingMap {
    s.human_coding_map(ID, 1).unwrap()
}

fn events(s: &Store, action: &str) -> Vec<AuditEvent> {
    s.audit_events(Some(ID))
        .unwrap()
        .into_iter()
        .filter(|e| e.action == action)
        .collect()
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Phase 2 coding of the three respondent-side units.
fn code_phase2(s: &Store) {
    for (u, c) in [(U2, "TECH-ADV"), (U3, "ORG-SKILLS"), (U5, "TECH-ADV")] {
        s.set_human_coding(ID, 2, &hc(u, &[c]), ACTOR).unwrap();
    }
}

fn part_of(s: &Store, units: &[&str]) -> PromptPart {
    part(s, units, &OFFERED, 1, 1)
}

/// Coded, one batch on U2, U3, U5, run R1: U3 disagrees, U2 and U5 agree.
fn challenged_simple(s: &Store) -> i64 {
    code_phase2(s);
    s.record_prompts(&[part_of(s, &[U2, U3, U5])], ACTOR)
        .unwrap();
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(
        s,
        1,
        1,
        &[
            (U2, &["TECH-ADV"]),
            (U3, &["ENV-PARTNER"]),
            (U5, &["TECH-ADV"]),
        ],
    );
    s.record_ai_output(run, "raw answer", &out, ACTOR).unwrap();
    run
}

fn decide(s: &Store, unit: &str, codes: &[&str], rationale: &str) {
    s.record_decision(
        ID,
        &decision(unit, ArbitrationChoice::Human, codes, rationale),
        ACTOR,
    )
    .unwrap();
}

// ------------------------------------------------- 1. automatic phase 1

#[test]
fn segmentation_codes_phase_1_by_speaker_role() {
    let (s, _d) = segmented();
    let map = p1(&s);
    assert_eq!(map.len(), 5);
    assert_eq!(map[U1], set(&["NA"]));
    assert_eq!(map[U2], set(&["CONTEXT"]));
    assert_eq!(map[U3], set(&["CONTEXT"]));
    assert_eq!(map[U4], set(&["NA"]));
    assert_eq!(
        map[U5],
        set(&["CONTEXT"]),
        "unknown role is kept as relevant"
    );
}

#[test]
fn phase_1_is_complete_right_after_segmentation() {
    let (s, _d) = segmented();
    let g = s.guard(ID, 1).unwrap();
    assert!(g.complete, "{g:?}");
    assert!(g.missing.is_empty());
    assert_eq!(g.n_units, 5);
}

#[test]
fn automatic_coding_is_audited_once_with_counts() {
    let (s, _d) = segmented();
    let ev = events(&s, "coding.auto_phase1");
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(ev[0].actor, ACTOR);
    assert_eq!(ev[0].target.as_deref(), Some(ID));
    assert_eq!(ev[0].detail["n_filled"], 5);
    assert_eq!(ev[0].detail["n_na"], 2);
    assert_eq!(ev[0].detail["n_context"], 3);
    assert_eq!(ev[0].detail["n_kept"], 0);
    assert!(
        events(&s, "coding.set").is_empty(),
        "no per-unit event for the automatic codings"
    );
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn saving_the_same_segmentation_again_adds_no_event() {
    let (s, _d) = segmented();
    s.save_segmentation(&base(), ACTOR).unwrap();
    assert_eq!(events(&s, "coding.auto_phase1").len(), 1);
    assert_eq!(p1(&s).len(), 5);
}

#[test]
fn an_interview_of_interviewer_turns_only_is_all_na() {
    let (s, _d) = unsegmented();
    s.save_segmentation(
        &seg_of(&[
            (SpeakerRole::Interviewer, "Hello."),
            (SpeakerRole::Interviewer, "Anyone there?"),
        ]),
        ACTOR,
    )
    .unwrap();
    assert_eq!(p1(&s).len(), 2);
    assert!(p1(&s).values().all(|c| *c == set(&["NA"])));
    let ev = events(&s, "coding.auto_phase1");
    assert_eq!(ev[0].detail["n_context"], 0);
    assert!(s.phase_units(ID, 2).unwrap().is_empty());
}

// ------------------------------------------- 1b. re-segmentation keeps work

#[test]
fn resegmenting_keeps_human_phase_1_and_fills_only_new_units() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 1, &hc(U2, &["TECH"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 1, &hc(U5, &["NA"]), ACTOR).unwrap();
    s.save_segmentation(&with_extra(), ACTOR)
        .expect("re-segmentation is allowed with candidate codings");
    let map = p1(&s);
    assert_eq!(map.len(), 6);
    assert_eq!(map[U2], set(&["TECH"]), "human category kept");
    assert_eq!(map[U5], set(&["NA"]), "human NA kept");
    assert_eq!(map[U3], set(&["CONTEXT"]));
    assert_eq!(map[U6], set(&["CONTEXT"]), "new unit filled");
    let ev = events(&s, "coding.auto_phase1");
    assert_eq!(ev.len(), 2, "{ev:?}");
    assert_eq!(ev[1].detail["n_filled"], 1);
    assert_eq!(ev[1].detail["n_context"], 1);
    assert_eq!(ev[1].detail["n_na"], 0);
    assert_eq!(ev[1].detail["n_kept"], 5);
}

#[test]
fn resegmenting_keeps_phase_2_codings() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    let mut rich = hc(U3, &["ORG-SKILLS"]);
    rich.memo = Some("skills gap".into());
    s.set_human_coding(ID, 2, &rich, ACTOR).unwrap();
    s.save_segmentation(&with_extra(), ACTOR).unwrap();
    let p2 = s.human_codings(ID, 2).unwrap();
    assert_eq!(p2[U2].codes, set(&["TECH-ADV"]));
    assert_eq!(p2[U3].codes, set(&["ORG-SKILLS"]));
    assert_eq!(p2[U3].memo.as_deref(), Some("skills gap"));
    assert!(!p2.contains_key(U6), "the new unit is not coded in phase 2");
    assert!(s.phase_units(ID, 2).unwrap().contains(&U6.to_string()));
}

#[test]
fn a_config_change_is_not_refused_when_only_automatic_codings_exist() {
    let (s, _d) = segmented();
    let cfg = cs_model::StudyConfig {
        study_name: "Renamed".into(),
        ..cs_model::StudyConfig::default()
    };
    s.set_study_config(&cfg, ACTOR).unwrap();
    let custom = cs_model::StudyConfig {
        framework: cs_model::FrameworkPreset::Custom,
        phase1_categories: vec![
            cs_model::Category {
                name: "ALPHA".into(),
                label: "Alpha".into(),
                definition: "Alpha.".into(),
                attachment: Some("T".into()),
            },
            cs_model::Category {
                name: "CONTEXT".into(),
                label: "Context".into(),
                definition: "Background.".into(),
                attachment: None,
            },
        ],
        ..cs_model::StudyConfig::default()
    };
    s.set_study_config(&custom, ACTOR)
        .expect("category names may still change before the candidate codes phase 1");
}

// ------------------------------------- 2. interviewer units stay out of AI

#[test]
fn interviewer_units_are_out_of_phase_2_scope() {
    let (s, _d) = segmented();
    assert_eq!(s.phase_units(ID, 2).unwrap(), ids(&[U2, U3, U5]));
    assert_eq!(s.phase_units(ID, 1).unwrap().len(), 5);
    let err = s
        .set_human_coding(ID, 2, &hc(U1, &["TECH-ADV"]), ACTOR)
        .unwrap_err();
    assert_eq!(kind(&err), "validation");
}

#[test]
fn an_interviewer_unit_cannot_enter_a_prompt_batch() {
    let (s, _d) = segmented();
    code_phase2(&s);
    let err = s
        .record_prompts(&[part_of(&s, &[U1, U2, U3, U5])], ACTOR)
        .unwrap_err();
    assert_eq!(kind(&err), "validation");
    assert!(err.to_string().contains(U1), "{err}");
}

#[test]
fn interviewer_units_are_never_verification_targets() {
    let (s, _d) = segmented();
    code_phase2(&s);
    let targets = s.verification_targets(ID).unwrap();
    let got: Vec<&str> = targets.iter().map(|t| t.unit_id.as_str()).collect();
    assert!(!got.contains(&U1), "{got:?}");
    assert!(!got.contains(&U4), "{got:?}");
    assert!(!got.is_empty());
}

#[test]
fn interviewer_units_are_never_in_the_review_or_the_sample() {
    let (s, _d) = segmented();
    challenged_simple(&s);
    let review = s.review(ID).unwrap();
    assert_eq!(review.n_disagreements, 1);
    assert_eq!(review.n_sampled, 2);
    for e in &review.items {
        assert!(
            e.item.unit_id != U1 && e.item.unit_id != U4,
            "{}",
            e.item.unit_id
        );
    }
    for (_, _, units) in s.review_samples(ID).unwrap() {
        assert!(!units.contains(&U1.to_string()) && !units.contains(&U4.to_string()));
    }
    let targets = s.verification_targets(ID).unwrap();
    assert!(targets.iter().all(|t| t.unit_id != U1 && t.unit_id != U4));
}

// ------------------------------------------------- 3. derived contexts

fn contexts(s: &Store, unit: &str) -> Vec<String> {
    s.phase2_context_for_unit(ID, unit).unwrap()
}

#[test]
fn one_attached_code_gives_its_context() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U5, &["ENV-PARTNER"]), ACTOR)
        .unwrap();
    assert_eq!(contexts(&s, U2), ids(&["TECH"]));
    assert_eq!(contexts(&s, U3), ids(&["ORG"]));
    assert_eq!(contexts(&s, U5), ids(&["ENV"]));
}

#[test]
fn several_codes_give_deduplicated_contexts_in_toe_order() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV", "TECH-COST"]), ACTOR)
        .unwrap();
    s.set_human_coding(
        ID,
        2,
        &hc(U3, &["ENV-PARTNER", "ORG-SKILLS", "TECH-ADV"]),
        ACTOR,
    )
    .unwrap();
    assert_eq!(contexts(&s, U2), ids(&["TECH"]), "two T codes, one context");
    assert_eq!(contexts(&s, U3), ids(&["TECH", "ORG", "ENV"]));
}

#[test]
fn a_code_without_attachment_gives_context() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 2, &hc(U2, &["OUT-REALISED"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U3, &["OUT-REALISED", "TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U5, &["NA"]), ACTOR).unwrap();
    assert_eq!(contexts(&s, U2), ids(&["CONTEXT"]));
    assert_eq!(
        contexts(&s, U3),
        ids(&["TECH"]),
        "CONTEXT only when nothing is attached"
    );
    assert_eq!(contexts(&s, U5), ids(&["CONTEXT"]), "NA in phase 2");
}

#[test]
fn a_relevant_unit_not_yet_coded_in_phase_2_is_context() {
    let (s, _d) = segmented();
    assert_eq!(contexts(&s, U2), ids(&["CONTEXT"]));
}

#[test]
fn a_unit_out_of_scope_has_no_derived_context() {
    let (s, _d) = segmented();
    assert!(contexts(&s, U1).is_empty(), "interviewer");
    s.set_human_coding(ID, 1, &hc(U2, &["NA"]), ACTOR).unwrap();
    assert!(contexts(&s, U2).is_empty(), "NA in phase 1");
}

#[test]
fn the_context_follows_the_current_codes() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U2, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    assert_eq!(contexts(&s, U2), ids(&["ORG"]));
}

#[test]
fn an_unknown_unit_is_refused() {
    let (s, _d) = segmented();
    let err = s.phase2_context_for_unit(ID, "ECO-16-U099").unwrap_err();
    assert_eq!(kind(&err), "validation");
}

// ----------------------------------------------------- 4. simple progress

#[test]
fn progress_after_segmentation() {
    let (s, _d) = segmented();
    assert_eq!(
        s.simple_progress(ID).unwrap(),
        SimpleProgress {
            n_respondent_units: 3,
            n_relevant: 3,
            n_coded: 0,
            n_batches: 0,
            n_runs_complete: 0,
            n_disagreements: 0,
            n_decided: 0,
            n_sample: 0,
            n_sample_reviewed: 0,
            done: false,
        }
    );
}

#[test]
fn progress_counts_phase_1_relevance_and_phase_2_coding() {
    let (s, _d) = segmented();
    s.set_human_coding(ID, 1, &hc(U5, &["NA"]), ACTOR).unwrap();
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    let p = s.simple_progress(ID).unwrap();
    assert_eq!(
        p.n_respondent_units, 3,
        "a respondent unit marked NA stays counted"
    );
    assert_eq!(p.n_relevant, 2);
    assert_eq!(p.n_coded, 1);
    assert!(!p.done);
    s.set_human_coding(ID, 2, &hc(U3, &["ORG-SKILLS"]), ACTOR)
        .unwrap();
    let p = s.simple_progress(ID).unwrap();
    assert_eq!((p.n_relevant, p.n_coded), (2, 2));
    assert!(!p.done, "no run yet");
}

#[test]
fn progress_follows_batch_run_and_review() {
    let (s, _d) = segmented();
    code_phase2(&s);
    s.record_prompts(&[part_of(&s, &[U2, U3, U5])], ACTOR)
        .unwrap();
    let p = s.simple_progress(ID).unwrap();
    assert_eq!((p.n_batches, p.n_runs_complete), (1, 0));
    assert_eq!(p.n_coded, 3);

    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(
        &s,
        1,
        1,
        &[
            (U2, &["TECH-ADV"]),
            (U3, &["ENV-PARTNER"]),
            (U5, &["TECH-ADV"]),
        ],
    );
    s.record_ai_output(run, "raw answer", &out, ACTOR).unwrap();
    let p = s.simple_progress(ID).unwrap();
    assert_eq!(p.n_runs_complete, 1);
    assert_eq!((p.n_disagreements, p.n_decided), (1, 0));
    assert_eq!((p.n_sample, p.n_sample_reviewed), (2, 0));
    assert!(!p.done);

    decide(&s, U3, &["ORG-SKILLS"], "The skills gap is stated.");
    let p = s.simple_progress(ID).unwrap();
    assert_eq!((p.n_decided, p.n_sample_reviewed), (1, 0));
    assert!(!p.done, "the sample is not reviewed yet");

    let review = s.review(ID).unwrap();
    let sampled: Vec<String> = review
        .items
        .iter()
        .filter(|e| e.item.kind == ReviewKind::AgreementSample)
        .map(|e| e.item.unit_id.clone())
        .collect();
    assert_eq!(sampled, ids(&[U2, U5]));
    decide(&s, U2, &["TECH-ADV"], "");
    let p = s.simple_progress(ID).unwrap();
    assert_eq!((p.n_decided, p.n_sample_reviewed), (1, 1));
    assert!(!p.done);
    decide(&s, U5, &["TECH-ADV"], "");
    let p = s.simple_progress(ID).unwrap();
    assert_eq!((p.n_decided, p.n_sample_reviewed), (1, 2));
    assert!(p.done, "{p:?}");
}

#[test]
fn not_done_when_a_relevant_unit_is_not_covered_by_a_complete_run() {
    let (s, _d) = segmented();
    code_phase2(&s);
    // The batch leaves U5 out.
    s.record_prompts(&[part_of(&s, &[U2, U3])], ACTOR).unwrap();
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let out = output(&s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["ORG-SKILLS"])]);
    s.record_ai_output(run, "raw answer", &out, ACTOR).unwrap();
    let review = s.review(ID).unwrap();
    for e in &review.items {
        decide(&s, &e.item.unit_id, &["TECH-ADV"], "ok");
    }
    let p = s.simple_progress(ID).unwrap();
    assert_eq!(p.n_runs_complete, 1);
    assert_eq!(p.n_decided + p.n_sample_reviewed, review.items.len());
    assert!(!p.done, "U5 is relevant but covered by no complete run");
}

#[test]
fn not_done_when_a_relevant_unit_is_uncoded() {
    let (s, _d) = segmented();
    challenged_simple(&s);
    for e in s.review(ID).unwrap().items {
        decide(&s, &e.item.unit_id, &["TECH-ADV"], "ok");
    }
    assert!(s.simple_progress(ID).unwrap().done);
    // A new respondent unit arrives with a re-segmentation: relevant, uncoded.
    s.save_segmentation(&with_extra(), ACTOR).unwrap();
    let p = s.simple_progress(ID).unwrap();
    assert_eq!((p.n_relevant, p.n_coded), (4, 3));
    assert!(!p.done);
}

#[test]
fn progress_of_an_unknown_interview_is_refused() {
    let (s, _d) = segmented();
    let err = s.simple_progress("NOPE-01").unwrap_err();
    assert_eq!(kind(&err), "not_found");
}
