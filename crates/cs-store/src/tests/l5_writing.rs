//! V1 lot 5: typed links, the five parts, syntheses, framework reading, the
//! Markdown export and the final audit.

use super::*;
use crate::codetree::NewCodeFromPassage;
use crate::passages::NewPassage;
use crate::{FrameworkCellReading, FrameworkReading, NewTheme, PartEdit};
use cs_model::analysis::LinkTarget;
use cs_model::passage::OffsetUnit;
use cs_model::project::*;

fn selection(unit: &str, start: usize, end: usize) -> NewPassage {
    NewPassage {
        interview_id: ID.into(),
        unit_id: unit.into(),
        start: Some(start),
        end: Some(end),
        offset_unit: OffsetUnit::Scalar,
        text: None,
    }
}

/// A retained theme "T-0001" with one code, one passage "tags" (P-00001).
fn setup() -> (Store, TempDir, String, String) {
    let (s, d) = ready();
    let (code, passage) = s
        .create_code_from_passage(
            &NewCodeFromPassage {
                name: "Retour rapide".into(),
                definition: "Une définition.".into(),
                includes: String::new(),
                excludes: String::new(),
                parent_id: None,
                passage_id: None,
                passage: Some(selection(U2, 4, 8)),
                research_questions: Vec::new(),
                reason: None,
            },
            ACTOR,
        )
        .unwrap();
    s.create_research_question("RQ1", "Pourquoi ?", ACTOR)
        .unwrap();
    let t = s
        .create_theme(
            &NewTheme {
                title: "Le retour rapide rassure".into(),
                codes: vec![code.code_id],
                research_questions: vec!["RQ1".into()],
                ..NewTheme::default()
            },
            ACTOR,
        )
        .unwrap();
    s.set_theme_status(&t.theme_id, "retained", "", Some("test"), ACTOR)
        .unwrap();
    (s, d, t.theme_id, passage.passage_id)
}

fn part(theme: &str, p: &str) -> LinkTarget {
    LinkTarget::ThemePart {
        theme_id: theme.into(),
        part: p.into(),
    }
}

#[test]
fn a_passage_is_linked_with_a_type_from_the_project_list() {
    let (s, _d, theme, pid) = setup();
    let target = part(&theme, "mechanism_evidence");
    let l = s
        .create_link(&pid, &target, "supports", "montre le délai", ACTOR)
        .unwrap();
    assert_eq!(l.link_id, "L-0001");
    assert_eq!(l.passage.text, "tags");
    assert_eq!(l.passage.verification.as_deref(), Some("unverified"));
    // refused: unknown type, long note, unknown part, duplicate
    assert_eq!(
        kind(
            &s.create_link(&pid, &part(&theme, "mini_synthesis"), "adores", "", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(
            &s.create_link(
                &pid,
                &part(&theme, "mini_synthesis"),
                "supports",
                "a\nb",
                ACTOR
            )
            .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(
            &s.create_link(&pid, &part(&theme, "nope"), "supports", "", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(
            &s.create_link(&pid, &target, "nuances", "", ACTOR)
                .unwrap_err()
        ),
        "guard"
    );
    // update and remove
    let l = s
        .update_link("L-0001", Some("nuances"), None, ACTOR)
        .unwrap();
    assert_eq!(l.link_type, "nuances");
    assert_eq!(l.note, "montre le délai");
    assert_eq!(s.links_of(&target).unwrap().len(), 1);
    s.remove_link("L-0001", "doublon", ACTOR).unwrap();
    assert!(s.links().unwrap().is_empty());
    assert_eq!(kind(&s.link("L-0001").unwrap_err()), "not_found");
    // linked again after removal
    let again = s.create_link(&pid, &target, "supports", "", ACTOR).unwrap();
    assert_eq!(again.link_id, "L-0002");
    // a theme with links cannot be deleted
    assert_eq!(
        kind(&s.delete_theme(&theme, "", ACTOR).unwrap_err()),
        "guard"
    );
}

#[test]
fn parts_are_saved_as_versions_with_a_nature_from_the_list() {
    let (s, _d, theme, _pid) = setup();
    let w = s.theme_writing(&theme).unwrap();
    assert_eq!(w.parts.len(), 5);
    assert_eq!(w.parts["analytical_statement"].text, "");
    let p = s
        .set_theme_part(
            &theme,
            "analytical_statement",
            &PartEdit {
                text: Some("Le retour rapide rassure.".into()),
                nature: Some(Some("first_hand".into())),
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(p.text, "Le retour rapide rassure.");
    assert_eq!(p.nature.as_deref(), Some("first_hand"));
    assert_eq!(p.nature_label.as_deref(), Some("Récit de première main"));
    assert_eq!(p.n_versions, 1);
    // partial save: the nature stays
    let p = s
        .set_theme_part(
            &theme,
            "analytical_statement",
            &PartEdit {
                text: Some("Version deux.".into()),
                nature: None,
            },
            ACTOR,
        )
        .unwrap();
    assert_eq!(p.nature.as_deref(), Some("first_hand"));
    assert_eq!(p.n_versions, 2);
    // same text: no new version
    s.set_theme_part(
        &theme,
        "analytical_statement",
        &PartEdit {
            text: Some("Version deux.".into()),
            nature: None,
        },
        ACTOR,
    )
    .unwrap();
    assert_eq!(
        s.writing_versions("theme_part", &theme, "analytical_statement")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        kind(
            &s.set_theme_part(
                &theme,
                "analytical_statement",
                &PartEdit {
                    text: None,
                    nature: Some(Some("rumeur".into()))
                },
                ACTOR
            )
            .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(
            &s.set_theme_part(&theme, "nope", &PartEdit::default(), ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(
            &s.set_theme_part("T-0099", "mini_synthesis", &PartEdit::default(), ACTOR)
                .unwrap_err()
        ),
        "not_found"
    );
}

#[test]
fn a_research_question_has_a_synthesis_with_its_themes() {
    let (s, _d, theme, _pid) = setup();
    s.set_theme_part(
        &theme,
        "mini_synthesis",
        &PartEdit {
            text: Some("En bref.".into()),
            nature: None,
        },
        ACTOR,
    )
    .unwrap();
    let q = s.question_synthesis("RQ1").unwrap();
    assert_eq!(q.themes.len(), 1);
    assert_eq!(q.themes[0].mini_synthesis, "En bref.");
    assert_eq!(q.n_versions, 0);
    let q = s
        .set_question_synthesis("RQ1", "Parce que.", "Un site.", ACTOR)
        .unwrap();
    assert_eq!(
        (q.answer.as_str(), q.limits.as_str()),
        ("Parce que.", "Un site.")
    );
    assert_eq!(q.n_versions, 1);
    s.set_question_synthesis("RQ1", "Parce que.", "Un site.", ACTOR)
        .unwrap();
    assert_eq!(s.question_synthesis("RQ1").unwrap().n_versions, 1);
    s.set_question_synthesis("RQ1", "Autre.", "Un site.", ACTOR)
        .unwrap();
    assert_eq!(s.question_synthesis("RQ1").unwrap().n_versions, 2);
    assert_eq!(kind(&s.question_synthesis("RQ9").unwrap_err()), "not_found");
}

#[test]
fn the_framework_reading_exists_only_with_a_framework() {
    let (s, _d, theme, _pid) = setup();
    assert_eq!(kind(&s.framework_reading(&theme).unwrap_err()), "guard");
    let mut cfg = s.project_config().unwrap();
    cfg.interpretive_framework = Some(Framework {
        preset: "custom".into(),
        dimensions: vec![
            Dimension {
                id: "tech".into(),
                label: "Technologie".into(),
                definition: String::new(),
            },
            Dimension {
                id: "org".into(),
                label: "Organisation".into(),
                definition: String::new(),
            },
        ],
    });
    s.set_project_config(&cfg, ACTOR).unwrap();
    let r = s.framework_reading(&theme).unwrap();
    assert_eq!(r.cells.len(), 2);
    let mut reading = FrameworkReading {
        out_of_frame: true,
        out_of_frame_note: "Le coût symbolique.".into(),
        evidence_limit: "Un site.".into(),
        ..FrameworkReading::default()
    };
    reading.cells.insert(
        "tech".into(),
        FrameworkCellReading {
            explains: true,
            note: "Le délai".into(),
        },
    );
    let saved = s.set_framework_reading(&theme, &reading, ACTOR).unwrap();
    assert!(saved.cells["tech"].explains);
    assert!(!saved.cells["org"].explains);
    assert!(saved.out_of_frame);
    let t = s.framework_table().unwrap();
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.dimensions.len(), 2);
    reading
        .cells
        .insert("nope".into(), FrameworkCellReading::default());
    assert_eq!(
        kind(
            &s.set_framework_reading(&theme, &reading, ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
}

#[test]
fn the_markdown_cites_each_passage_as_a_footnote() {
    let (s, _d, theme, pid) = setup();
    s.set_theme_part(
        &theme,
        "mechanism_evidence",
        &PartEdit {
            text: Some("Les puces se remboursent en un an.".into()),
            nature: Some(Some("first_hand".into())),
        },
        ACTOR,
    )
    .unwrap();
    s.create_link(
        &pid,
        &part(&theme, "mechanism_evidence"),
        "supports",
        "le délai",
        ACTOR,
    )
    .unwrap();
    let md = s.writeup_markdown().unwrap();
    assert!(md.contains("## Le retour rapide rassure"), "{md}");
    assert!(md.contains("### Mécanisme et preuves"));
    assert!(md.contains("Les puces se remboursent en un an.[^1]"));
    assert!(md.contains("[^1]: ECO-16, unité ECO-16-U002 : « tags » (le délai)"));
    assert!(md.contains("Nature de la preuve : Récit de première main."));
    assert!(md.contains("_Partie non rédigée._"));
    assert!(!md.contains('\u{2014}'));
    // a candidate theme is not exported
    let c = s
        .create_theme(
            &NewTheme {
                title: "Brouillon".into(),
                ..NewTheme::default()
            },
            ACTOR,
        )
        .unwrap();
    assert!(!s.writeup_markdown().unwrap().contains(&c.title));
}

#[test]
fn the_final_audit_draws_with_a_recorded_seed_and_keeps_the_verdicts() {
    let (s, _d, theme, pid) = setup();
    assert_eq!(
        kind(&s.draw_final_audit(3, None, ACTOR).unwrap_err()),
        "guard"
    );
    for (p, text) in [
        ("analytical_statement", "Une idée. Deux idées."),
        ("mechanism_evidence", "Le mécanisme est décrit ici."),
        ("mini_synthesis", "Pour finir."),
    ] {
        s.set_theme_part(
            &theme,
            p,
            &PartEdit {
                text: Some(text.into()),
                nature: None,
            },
            ACTOR,
        )
        .unwrap();
    }
    s.create_link(
        &pid,
        &part(&theme, "mechanism_evidence"),
        "supports",
        "",
        ACTOR,
    )
    .unwrap();
    let a = s.draw_final_audit(3, Some("graine-1"), ACTOR).unwrap();
    assert_eq!(a.seed, "graine-1");
    assert_eq!(a.claims.len(), 3);
    assert_eq!(a.eligible, 3);
    // the linked part comes first in the pool; with 3 eligible all are drawn
    let linked = a
        .claims
        .iter()
        .find(|c| c.claim_key == format!("theme_part:{theme}:mechanism_evidence"))
        .unwrap();
    assert_eq!(linked.links.len(), 1);
    assert_eq!(linked.links[0].passage.text, "tags");
    assert_eq!(linked.theme.as_ref().unwrap().theme_id, theme);
    assert_eq!(linked.codes.len(), 1);
    assert!(linked.gaps.iter().any(|g| g.kind == "unverified_passage"));
    assert!(!linked.gaps.iter().any(|g| g.kind == "no_link"));
    let unlinked = a
        .claims
        .iter()
        .find(|c| c.claim_key.ends_with("mini_synthesis"))
        .unwrap();
    assert!(unlinked.gaps.iter().any(|g| g.kind == "no_link"));
    // same seed, same draw
    let b = s.draw_final_audit(2, Some("graine-1"), ACTOR).unwrap();
    assert_eq!(
        b.claims
            .iter()
            .map(|c| c.claim_key.clone())
            .collect::<Vec<_>>()[..1],
        a.claims
            .iter()
            .map(|c| c.claim_key.clone())
            .collect::<Vec<_>>()[..1]
    );
    // a fresh seed is generated and recorded
    let c = s.draw_final_audit(1, None, ACTOR).unwrap();
    assert_eq!(c.seed.len(), 16);
    assert_eq!(
        s.latest_final_audit().unwrap().unwrap().audit_id,
        c.audit_id
    );
    // verdicts
    let key = linked.claim_key.clone();
    let a2 = s
        .set_audit_verdict(a.audit_id, &key, "to_nuance", "trop fort", ACTOR)
        .unwrap();
    let v = a2
        .claims
        .iter()
        .find(|x| x.claim_key == key)
        .unwrap()
        .verdict
        .clone()
        .unwrap();
    assert_eq!(
        (v.verdict.as_str(), v.verdict_label.as_str()),
        ("to_nuance", "à nuancer")
    );
    s.set_audit_verdict(a.audit_id, &key, "supported", "", ACTOR)
        .unwrap();
    assert_eq!(s.audit_verdict_log().unwrap().len(), 2, "both stay");
    assert_eq!(
        s.final_audit(a.audit_id)
            .unwrap()
            .claims
            .iter()
            .find(|x| x.claim_key == key)
            .unwrap()
            .verdict
            .as_ref()
            .unwrap()
            .verdict,
        "supported"
    );
    assert_eq!(
        kind(
            &s.set_audit_verdict(a.audit_id, &key, "fine", "", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(
            &s.set_audit_verdict(a.audit_id, "theme_part:T-0001:nope", "supported", "", ACTOR)
                .unwrap_err()
        ),
        "validation"
    );
    assert_eq!(
        kind(&s.draw_final_audit(0, None, ACTOR).unwrap_err()),
        "validation"
    );
    // the audit shows in the Markdown with its seed
    let md = s.writeup_markdown().unwrap();
    assert!(md.contains("## Audit final"));
    assert!(md.contains(&c.seed));
    assert!(s.verify_chain().unwrap().ok);
}
