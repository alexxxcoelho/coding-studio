//! C2: the human reference is frozen when the challenger package is generated.

use super::*;

fn codings_of(pairs: &[(&str, &[&str])]) -> cs_model::CodingMap {
    pairs.iter().map(|(u, c)| (u.to_string(), set(c))).collect()
}

fn uid(n: usize) -> String {
    format!("{ID}-U{n:03}")
}

#[test]
fn prompt_generation_freezes_the_human_coding() {
    let (s, _d) = ready();
    coded(&s);
    batch(&s);
    let snaps = s.human_snapshots(ID).unwrap();
    assert_eq!(snaps.len(), 1);
    let snap = &snaps[0];
    assert_eq!(snap.batch_id, 1);
    assert_eq!(snap.phase, 2);
    assert_eq!(
        snap.codings,
        codings_of(&[(U2, &["TECH-ADV"]), (U3, &["ORG-SKILLS"])])
    );
    assert_eq!(
        snap.segmentation_hash,
        s.segmentation(ID).unwrap().unwrap().hash
    );
    assert_eq!(snap.codebook_hash, "c");
    assert_eq!(
        snap.sha256,
        sha256_hex(serde_json::to_string(&snap.codings).unwrap().as_bytes())
    );
    assert_eq!(*snap, s.human_snapshot(ID, 1).unwrap());

    let events = s.audit_events(None).unwrap();
    let gen = events
        .iter()
        .find(|e| e.action == "prompt.generate")
        .unwrap();
    let take = events
        .iter()
        .find(|e| e.action == "human_snapshot.take")
        .expect("human_snapshot.take is audited");
    assert_eq!(gen.detail["batch_id"], 1);
    assert_eq!(take.detail["batch_id"], 1);
    assert_eq!(take.detail["sha256"], snap.sha256.as_str());
    assert_eq!(take.detail["n_units"], 2);
}

#[test]
fn prompt_generation_is_refused_while_a_unit_is_uncoded() {
    let (s, _d) = ready();
    phase1(&s);
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    let e = s
        .record_prompts(&[part(&s, &[U2, U3], &OFFERED, 1, 1)], ACTOR)
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    assert!(e.to_string().contains(U3), "{e}");
    assert!(s.human_snapshots(ID).unwrap().is_empty());
    assert!(s.prompts(ID).unwrap().is_empty());
}

#[test]
fn a_unit_with_only_an_open_label_is_frozen_with_no_codes() {
    let (s, _d) = ready();
    phase1(&s);
    s.set_human_coding(ID, 2, &hc(U2, &["TECH-ADV"]), ACTOR)
        .unwrap();
    s.create_to_clarify(ID, U3, "skills gap at the integrator", "", ACTOR)
        .unwrap();
    s.set_human_coding(ID, 2, &hc(U3, &[]), ACTOR).unwrap();
    batch(&s);
    let snap = s.human_snapshot(ID, 1).unwrap();
    assert_eq!(snap.codings, codings_of(&[(U2, &["TECH-ADV"]), (U3, &[])]));
}

#[test]
fn snapshots_are_append_only() {
    let (s, _d) = ready();
    coded(&s);
    batch(&s);
    let c = s.connection();
    for sql in [
        "UPDATE human_snapshots SET sha256 = 'x'",
        "DELETE FROM human_snapshots",
    ] {
        let e = c.execute(sql, []).unwrap_err();
        assert!(e.to_string().contains("append-only"), "{sql}: {e}");
    }
    assert_eq!(s.human_snapshots(ID).unwrap().len(), 1);
}

#[test]
fn review_compares_the_snapshot_not_the_current_coding() {
    let (s, _d) = ready();
    challenged(&s);
    s.set_human_coding(ID, 2, &hc(U3, &["ENV-PARTNER"]), ACTOR)
        .unwrap();
    assert!(s.human_codings(ID, 2).unwrap()[U3].revised_after_ai);
    let review = s.review(ID).unwrap();
    assert_eq!(review.n_disagreements, 1);
    let u3 = &review
        .items
        .iter()
        .find(|e| e.item.unit_id == U3)
        .unwrap()
        .item;
    assert_eq!(u3.kind, ReviewKind::Disagreement);
    assert_eq!(u3.human, set(&["ORG-SKILLS"]));
    assert_eq!(u3.human_current, set(&["ENV-PARTNER"]));
    assert!(u3.revised_since_snapshot);
    let u2 = &review
        .items
        .iter()
        .find(|e| e.item.unit_id == U2)
        .unwrap()
        .item;
    assert_eq!(u2.human, set(&["TECH-ADV"]));
    assert_eq!(u2.human_current, set(&["TECH-ADV"]));
    assert!(!u2.revised_since_snapshot);
    let snap = s.human_snapshot(ID, 1).unwrap();
    assert_eq!(review.snapshots.len(), 1);
    assert_eq!(review.snapshots[0].run_label, "R1");
    assert_eq!(review.snapshots[0].snapshot_id, snap.snapshot_id);
    assert_eq!(review.snapshots[0].sha256, snap.sha256);
}

#[test]
fn review_sample_is_drawn_on_the_snapshot() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    s.import_codebook(&coding_db(&dir, &default_codes()), ACTOR)
        .unwrap();
    s.upsert_interview(&default_entry(), ACTOR).unwrap();
    s.save_segmentation(
        &segmentation(&[
            "a one.", "a two.", "a three.", "a four.", "a five.", "a six.",
        ]),
        ACTOR,
    )
    .unwrap();
    let ids: Vec<String> = (1..=6).map(uid).collect();
    for u in &ids {
        s.set_human_coding(ID, 1, &hc(u, &["TECH"]), ACTOR).unwrap();
        s.set_human_coding(ID, 2, &hc(u, &["TECH-ADV"]), ACTOR)
            .unwrap();
    }
    let refs: Vec<&str> = ids.iter().map(|u| u.as_str()).collect();
    s.record_prompts(&[part(&s, &refs, &OFFERED, 1, 1)], ACTOR)
        .unwrap();
    let run = s.create_ai_run(ID, &meta("R1"), ACTOR).unwrap();
    let all: Vec<(&str, &[&str])> = refs.iter().map(|u| (*u, &["TECH-ADV"][..])).collect();
    s.record_ai_output(run, "raw", &output(&s, 1, 1, &all), ACTOR)
        .unwrap();

    let before = s.review_samples(ID).unwrap();
    assert_eq!(before.len(), 1);
    let sampled = before[0].2.clone();
    assert!(!sampled.is_empty());
    let target = sampled[0].clone();

    s.set_human_coding(ID, 2, &hc(&target, &["ORG-SKILLS"]), ACTOR)
        .unwrap();

    assert_eq!(s.review_samples(ID).unwrap(), before, "sample is immutable");
    let review = s.review(ID).unwrap();
    assert_eq!(
        review.n_disagreements, 0,
        "the snapshot still agrees with the AI"
    );
    let item = &review
        .items
        .iter()
        .find(|e| e.item.unit_id == target)
        .expect("sampled unit still listed")
        .item;
    assert_eq!(item.kind, ReviewKind::AgreementSample);
    assert_eq!(item.human, set(&["TECH-ADV"]));
    assert_eq!(item.human_current, set(&["ORG-SKILLS"]));
    assert!(item.revised_since_snapshot);
}

#[test]
fn final_coding_still_follows_decisions_then_current_coding() {
    let (s, _d) = ready();
    challenged(&s);
    s.set_human_coding(ID, 2, &hc(U3, &["ENV-PARTNER"]), ACTOR)
        .unwrap();
    let (finals, uncoded) = s.final_codings(ID).unwrap();
    assert!(uncoded.is_empty());
    let u3 = finals.iter().find(|f| f.unit_id == U3).unwrap();
    assert_eq!(
        u3.codes,
        set(&["ENV-PARTNER"]),
        "current coding, not the snapshot"
    );

    s.record_decision(
        ID,
        &decision(
            U3,
            ArbitrationChoice::Human,
            &["ORG-SKILLS"],
            "Snapshot value kept.",
        ),
        ACTOR,
    )
    .unwrap();
    let (finals, _) = s.final_codings(ID).unwrap();
    let u3 = finals.iter().find(|f| f.unit_id == U3).unwrap();
    assert_eq!(u3.codes, set(&["ORG-SKILLS"]));
}

#[test]
fn a_second_batch_takes_a_second_snapshot() {
    let (s, _d) = ready();
    challenged(&s);
    s.set_human_coding(ID, 2, &hc(U3, &["ENV-PARTNER"]), ACTOR)
        .unwrap();
    s.record_prompts(&[part(&s, &[U2, U3], &OFFERED, 1, 1)], ACTOR)
        .unwrap();
    let snaps = s.human_snapshots(ID).unwrap();
    assert_eq!(snaps.len(), 2);
    assert_eq!((snaps[0].batch_id, snaps[1].batch_id), (1, 2));
    assert_ne!(snaps[0].sha256, snaps[1].sha256);
    assert_eq!(snaps[0].codings[U3], set(&["ORG-SKILLS"]));
    assert_eq!(snaps[1].codings[U3], set(&["ENV-PARTNER"]));

    // R2 answers batch 2 (the latest) and disagrees with the new reference on U3.
    let run = s.create_ai_run(ID, &meta("R2"), ACTOR).unwrap();
    let out = output(&s, 1, 1, &[(U2, &["TECH-ADV"]), (U3, &["ORG-SKILLS"])]);
    s.record_ai_output(run, "raw 2", &out, ACTOR).unwrap();
    let review = s.review(ID).unwrap();
    let refs: Vec<(&str, i64, &str)> = review
        .snapshots
        .iter()
        .map(|r| (r.run_label.as_str(), r.snapshot_id, r.sha256.as_str()))
        .collect();
    assert!(refs.contains(&("R1", snaps[0].snapshot_id, snaps[0].sha256.as_str())));
    assert!(refs.contains(&("R2", snaps[1].snapshot_id, snaps[1].sha256.as_str())));
    let u3 = &review
        .items
        .iter()
        .find(|e| e.item.unit_id == U3)
        .unwrap()
        .item;
    assert_eq!(u3.kind, ReviewKind::Disagreement);
    assert_eq!(u3.ai["R2"], set(&["ORG-SKILLS"]));
    // Ambiguity to settle in the implementation: with runs on two batches the
    // single `human` field is taken from the latest snapshot (R2's batch).
    assert_eq!(u3.human, set(&["ENV-PARTNER"]));
    assert!(!u3.revised_since_snapshot);
}
