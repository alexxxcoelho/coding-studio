//! F-104: the write-up as format-independent blocks.

use super::l5_writing::{part, setup};
use super::*;
use crate::{FrameworkCellReading, FrameworkReading, PartEdit, WriteupBlock};
use cs_model::analysis::LinkTarget;
use cs_model::project::*;

fn rich() -> (Store, TempDir, String, String) {
    let (s, d, theme, pid) = setup();
    for (p, text) in [
        ("analytical_statement", "Une idée | avec barre."),
        ("mechanism_evidence", "Les puces se remboursent."),
    ] {
        s.set_theme_part(
            &theme,
            p,
            &PartEdit {
                text: Some(text.into()),
                nature: Some(Some("first_hand".into())),
            },
            ACTOR,
        )
        .unwrap();
    }
    s.create_link(
        &pid,
        &part(&theme, "mechanism_evidence"),
        "supports",
        "le délai",
        ACTOR,
    )
    .unwrap();
    s.create_link(
        &pid,
        &LinkTarget::RqPart {
            rq_id: "RQ1".into(),
        },
        "supports",
        "",
        ACTOR,
    )
    .unwrap();
    s.set_question_synthesis("RQ1", "Parce que.", "Un site.", ACTOR)
        .unwrap();
    let mut cfg = s.project_config().unwrap();
    cfg.interpretive_framework = Some(Framework {
        preset: "custom".into(),
        dimensions: vec![Dimension {
            id: "tech".into(),
            label: "Technologie".into(),
            definition: String::new(),
        }],
    });
    s.set_project_config(&cfg, ACTOR).unwrap();
    let mut reading = FrameworkReading {
        out_of_frame: true,
        out_of_frame_note: "Hors cadre".into(),
        ..FrameworkReading::default()
    };
    reading.cells.insert(
        "tech".into(),
        FrameworkCellReading {
            explains: true,
            note: "Le délai".into(),
        },
    );
    s.set_framework_reading(&theme, &reading, ACTOR).unwrap();
    s.draw_final_audit(2, Some("graine"), ACTOR).unwrap();
    (s, d, theme, pid)
}

#[test]
fn the_markdown_renders_the_blocks_as_before() {
    let (s, _d, _t, _p) = rich();
    let md = s.writeup_markdown().unwrap();
    assert!(
        md.starts_with("# Rédaction\n\n## Le retour rapide rassure\n\n### "),
        "{md}"
    );
    assert!(md.contains("Les puces se remboursent.[^1]\n\n"), "{md}");
    assert!(md.contains("Parce que.[^1]\n\n"), "{md}");
    assert!(md.contains("| Thème | Technologie | Hors cadre |\n|---|---|---|\n| Le retour rapide rassure | explique : Le délai | oui : Hors cadre |\n\n"), "{md}");
    assert!(md.contains("- "), "{md}");
    assert!(
        md.ends_with("---\n\n[^1]: ECO-16, unité ECO-16-U002 : « tags » (le délai)\n"),
        "{md}"
    );
}

#[test]
fn the_blocks_carry_headings_citations_a_table_and_the_audit() {
    let (s, _d, _t, _p) = rich();
    let doc = s.writeup_doc().unwrap();
    assert!(matches!(
        &doc.blocks[0],
        WriteupBlock::Heading { level: 1, .. }
    ));
    assert_eq!(
        doc.cites.len(),
        1,
        "one passage, cited twice, numbered once"
    );
    assert_eq!(doc.cites[0].reference(), "ECO-16, unité ECO-16-U002");
    assert_eq!(doc.cites[0].quote, "tags");
    assert!(doc
        .blocks
        .iter()
        .any(|b| matches!(b, WriteupBlock::Table { rows, .. } if rows.len() == 1)));
    assert!(doc
        .blocks
        .iter()
        .any(|b| matches!(b, WriteupBlock::Bullet { .. })));
}
