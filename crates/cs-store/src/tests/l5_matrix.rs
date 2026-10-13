//! V1 lot 5: the analysis matrix, its stored cells and saved views.

use super::*;
use crate::codetree::NewCodeFromPassage;
use crate::passages::NewPassage;
use crate::{CellPatch, MatrixAxis, MatrixConfig, MatrixFilter, NewTheme};
use cs_model::passage::OffsetUnit;
use cs_model::project::*;
use std::collections::BTreeMap;

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

fn code(s: &Store, name: &str, unit: &str, start: usize, end: usize) -> (String, String) {
    let req = NewCodeFromPassage {
        name: name.into(),
        definition: "Une définition.".into(),
        includes: String::new(),
        excludes: String::new(),
        parent_id: None,
        passage_id: None,
        passage: Some(selection(unit, start, end)),
        research_questions: Vec::new(),
        reason: None,
    };
    let (c, p) = s.create_code_from_passage(&req, ACTOR).unwrap();
    (c.code_id, p.passage_id)
}

fn axis(kind: &str, ids: &[&str]) -> MatrixAxis {
    MatrixAxis {
        kind: kind.into(),
        ids: ids.iter().map(|s| s.to_string()).collect(),
        ..MatrixAxis::default()
    }
}

fn config(rows: MatrixAxis, cols: MatrixAxis) -> MatrixConfig {
    MatrixConfig {
        rows: vec![rows],
        cols: vec![cols],
        filters: Vec::new(),
    }
}

fn setup() -> (Store, TempDir, String, String, String) {
    let (s, d) = ready();
    let (a, pa) = code(&s, "Retour rapide", U2, 0, 8);
    let (b, _) = code(&s, "Peur du coût", U3, 0, 9);
    let t = s
        .create_theme(
            &NewTheme {
                title: "Le retour rapide rassure".into(),
                codes: vec![a.clone()],
                ..NewTheme::default()
            },
            ACTOR,
        )
        .unwrap();
    (s, d, t.theme_id, b, pa)
}

#[test]
fn the_passage_count_of_a_cell_is_computed_from_the_codings() {
    let (s, _d, theme, code_b, pa) = setup();
    let m = s
        .compute_matrix(&config(
            axis("themes", &[]),
            MatrixAxis {
                kind: "interviews".into(),
                ..MatrixAxis::default()
            },
        ))
        .unwrap();
    assert_eq!(m.rows.len(), 1);
    assert_eq!(m.rows[0].key, format!("theme:{theme}"));
    assert_eq!(m.cols[0].key, format!("interview:{ID}"));
    let cell = &m.cells[0][0];
    assert_eq!(cell.passages, 1);
    assert_eq!(cell.interviews, 1);
    assert_eq!(cell.passage_ids, vec![pa.clone()]);
    assert_eq!(cell.value, None, "the tool never fills a value");
    // theme x code: the passages carrying both ... none here
    let m = s
        .compute_matrix(&config(axis("themes", &[]), axis("codes", &[&code_b])))
        .unwrap();
    assert_eq!(m.cells[0][0].passages, 0);
    // the count follows the codings
    let a_code = s.theme(&theme).unwrap().codes[0].code_id.clone();
    s.uncode_passage(&pa, &a_code, "", ACTOR).unwrap();
    let m = s
        .compute_matrix(&config(
            axis("themes", &[]),
            MatrixAxis {
                kind: "interviews".into(),
                ..MatrixAxis::default()
            },
        ))
        .unwrap();
    assert_eq!(m.cells[0][0].passages, 0);
}

#[test]
fn what_the_researcher_says_of_a_cell_is_kept_under_stable_keys() {
    let (s, _d, theme, _b, _pa) = setup();
    let rk = format!("theme:{theme}");
    let ck = format!("interview:{ID}");
    s.set_matrix_cell(
        &CellPatch {
            row_key: rk.clone(),
            col_key: ck.clone(),
            value: Some(Some("present".into())),
            summary: Some("Rassure la direction.".into()),
        },
        ACTOR,
    )
    .unwrap();
    let cfg = config(axis("themes", &[]), axis("interviews", &[]));
    let m = s.compute_matrix(&cfg).unwrap();
    assert_eq!(m.cells[0][0].value.as_deref(), Some("present"));
    assert_eq!(m.cells[0][0].value_label.as_deref(), Some("présent"));
    assert_eq!(m.cells[0][0].summary, "Rassure la direction.");
    // transposed: the same cell, still found
    let m = s
        .compute_matrix(&config(axis("interviews", &[]), axis("themes", &[])))
        .unwrap();
    assert_eq!(m.cells[0][0].value.as_deref(), Some("present"));
    // closed values only
    let e = s
        .set_matrix_cell(
            &CellPatch {
                row_key: rk.clone(),
                col_key: ck.clone(),
                value: Some(Some("peut-être".into())),
                summary: None,
            },
            ACTOR,
        )
        .unwrap_err();
    assert_eq!(kind(&e), "validation");
    // one line only
    assert_eq!(
        kind(
            &s.set_matrix_cell(
                &CellPatch {
                    row_key: rk.clone(),
                    col_key: ck.clone(),
                    value: None,
                    summary: Some("a\nb".into()),
                },
                ACTOR,
            )
            .unwrap_err()
        ),
        "validation"
    );
    // unknown key
    assert!(s
        .set_matrix_cell(
            &CellPatch {
                row_key: "theme:T-0099".into(),
                col_key: ck.clone(),
                value: Some(Some("present".into())),
                summary: None,
            },
            ACTOR,
        )
        .is_err());
    // clearing both forgets the cell
    s.set_matrix_cell(
        &CellPatch {
            row_key: rk.clone(),
            col_key: ck.clone(),
            value: Some(None),
            summary: Some("".into()),
        },
        ACTOR,
    )
    .unwrap();
    assert!(s.matrix_cells_stored().unwrap().is_empty());
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn a_custom_dimension_is_filled_with_passages_linked_by_hand() {
    let (s, _d, theme, _b, pa) = setup();
    let mut cfg = s.project_config().unwrap();
    cfg.matrix_dimensions = vec![ListItem {
        id: "who_pays".into(),
        label: "Qui paie".into(),
    }];
    s.set_project_config(&cfg, ACTOR).unwrap();
    let c = config(axis("themes", &[]), axis("dimensions", &[]));
    let m = s.compute_matrix(&c).unwrap();
    assert_eq!(m.cols[0].key, "dim:who_pays");
    assert!(m.cells[0][0].manual);
    assert_eq!(m.cells[0][0].passages, 0);
    s.link_cell_passage(&format!("theme:{theme}"), "dim:who_pays", &pa, ACTOR)
        .unwrap();
    s.link_cell_passage(&format!("theme:{theme}"), "dim:who_pays", &pa, ACTOR)
        .unwrap();
    let m = s.compute_matrix(&c).unwrap();
    assert_eq!(m.cells[0][0].passages, 1);
    assert_eq!(m.cells[0][0].passage_ids, vec![pa.clone()]);
    s.unlink_cell_passage(&format!("theme:{theme}"), "dim:who_pays", &pa, ACTOR)
        .unwrap();
    assert_eq!(s.compute_matrix(&c).unwrap().cells[0][0].passages, 0);
    assert_eq!(
        kind(
            &s.unlink_cell_passage(&format!("theme:{theme}"), "dim:who_pays", &pa, ACTOR)
                .unwrap_err()
        ),
        "not_found"
    );
    // an unknown dimension is refused
    assert!(s
        .compute_matrix(&config(axis("themes", &[]), axis("dimensions", &["nope"])))
        .is_err());
}

#[test]
fn attribute_values_are_axes_and_filters_narrow_the_interviews() {
    let (s, _d, _theme, _b, _pa) = setup();
    let mut cfg = s.project_config().unwrap();
    cfg.attributes = vec![AttributeDef {
        id: "sector".into(),
        label: "Secteur".into(),
        kind: AttributeKind::Choice,
        choices: vec!["public".into(), "privé".into()],
        shared_with_model: false,
    }];
    s.set_project_config(&cfg, ACTOR).unwrap();
    s.set_interview_attributes(
        ID,
        &BTreeMap::from([("sector".to_string(), Some("privé".to_string()))]),
        ACTOR,
    )
    .unwrap();
    let by_sector = MatrixAxis {
        kind: "attribute".into(),
        attribute: Some("sector".into()),
        ..MatrixAxis::default()
    };
    let m = s
        .compute_matrix(&config(axis("themes", &[]), by_sector.clone()))
        .unwrap();
    assert_eq!(
        m.cols.iter().map(|c| c.key.clone()).collect::<Vec<_>>(),
        ["attr:sector=public", "attr:sector=privé"]
    );
    assert_eq!(m.cells[0][0].passages, 0);
    assert_eq!(m.cells[0][1].passages, 1);
    // filter: only public interviews
    let mut filtered = config(axis("themes", &[]), axis("interviews", &[]));
    filtered.filters = vec![MatrixFilter {
        attribute: "sector".into(),
        values: vec!["public".into()],
    }];
    let m = s.compute_matrix(&filtered).unwrap();
    assert!(m.interviews_in_scope.is_empty());
    assert!(m.cols.is_empty());
    filtered.filters[0].values = vec!["privé".into()];
    let m = s.compute_matrix(&filtered).unwrap();
    assert_eq!(m.interviews_in_scope, vec![ID.to_string()]);
    assert_eq!(m.cells[0][0].passages, 1);
    // axis without attribute id, unknown kind
    assert!(s
        .compute_matrix(&config(axis("themes", &[]), axis("attribute", &[])))
        .is_err());
    assert!(s
        .compute_matrix(&config(axis("themes", &[]), axis("planets", &[])))
        .is_err());
    assert!(s.compute_matrix(&MatrixConfig::default()).is_err());
}

#[test]
fn views_are_saved_renamed_and_deleted_and_the_options_list_the_choices() {
    let (s, _d, theme, _b, _pa) = setup();
    let cfg = config(axis("themes", &[&theme]), axis("interviews", &[]));
    let v = s
        .create_matrix_view("Thèmes par entretien", &cfg, ACTOR)
        .unwrap();
    assert_eq!(v.view_id, "M-0001");
    let (rec, m) = s.matrix_view("M-0001").unwrap();
    assert_eq!(rec.name, "Thèmes par entretien");
    assert_eq!(m.cells[0][0].passages, 1);
    let r = s
        .update_matrix_view("M-0001", Some("Autre nom"), None, ACTOR)
        .unwrap();
    assert_eq!(r.name, "Autre nom");
    assert!(s.create_matrix_view("", &cfg, ACTOR).is_err());
    assert!(s
        .create_matrix_view(
            "Mauvaise",
            &config(axis("themes", &["T-0099"]), axis("interviews", &[])),
            ACTOR
        )
        .is_err());
    let o = s.matrix_options().unwrap();
    assert_eq!(o.interviews, vec![ID.to_string()]);
    assert_eq!(o.themes.len(), 1);
    assert!(o.codes.len() >= 2);
    assert_eq!(o.cell_values.len(), 4);
    s.delete_matrix_view("M-0001", ACTOR).unwrap();
    assert!(s.matrix_views().unwrap().is_empty());
    assert_eq!(kind(&s.matrix_view("M-0001").unwrap_err()), "not_found");
}
