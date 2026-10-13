//! Fixtures shared by the V2 tests (codes and search).

use super::*;
use crate::codetree::NewCode;
use crate::passages::NewPassage;
use cs_model::passage::OffsetUnit;
use cs_model::project::*;
use std::collections::BTreeMap;

pub(super) const R: &str = "RET-22";
pub(super) const R1: &str = "RET-22-U001";
pub(super) const R2: &str = "RET-22-U002";
pub(super) const T: &str = "RET-23";
pub(super) const T1: &str = "RET-23-U001";

/// Registers an interview with the given respondent texts; the units are
/// `<id>-U001`, `<id>-U002`...
pub(super) fn add_interview(s: &Store, id: &str, texts: &[&str]) {
    let mut e = default_entry();
    e.interview_id = id.into();
    e.folder_name = format!("{id}_X");
    e.folder_path = format!("/tmp/{id}_X");
    s.upsert_interview(&e, ACTOR).unwrap();
    let mut seg = segmentation(texts);
    seg.interview_id = id.into();
    for (i, u) in seg.units.iter_mut().enumerate() {
        u.unit_id = format!("{id}-U{:03}", i + 1);
    }
    seg.hash = Segmentation::units_hash(&seg.units);
    s.save_segmentation(&seg, ACTOR).unwrap();
}

/// A code in the tree; returns its stable id.
pub(super) fn code(s: &Store, name: &str, parent: Option<&str>) -> String {
    s.create_code_v1(
        &NewCode {
            name: name.into(),
            definition: format!("definition of {name}"),
            includes: String::new(),
            excludes: String::new(),
            parent_id: parent.map(str::to_string),
            research_questions: vec![],
            memo: String::new(),
            reason: None,
        },
        ACTOR,
    )
    .unwrap()
    .code_id
}

/// A code with its own definition.
pub(super) fn code_def(s: &Store, name: &str, definition: &str) -> String {
    s.create_code_v1(
        &NewCode {
            name: name.into(),
            definition: definition.into(),
            includes: String::new(),
            excludes: String::new(),
            parent_id: None,
            research_questions: vec![],
            memo: String::new(),
            reason: None,
        },
        ACTOR,
    )
    .unwrap()
    .code_id
}

/// The passage (whole unit when `span` is None) coded with the given codes.
pub(super) fn pcode(
    s: &Store,
    interview: &str,
    unit: &str,
    span: Option<(usize, usize)>,
    codes: &[&str],
) -> String {
    let (p, _) = s
        .create_passage(
            &NewPassage {
                interview_id: interview.into(),
                unit_id: unit.into(),
                start: span.map(|x| x.0),
                end: span.map(|x| x.1),
                offset_unit: OffsetUnit::Scalar,
                text: None,
            },
            ACTOR,
        )
        .unwrap();
    for c in codes {
        s.code_passage(&p.passage_id, c, "", ACTOR).unwrap();
    }
    p.passage_id
}

/// Declares attributes `sector` (choice public / privé) and `size` (number).
pub(super) fn with_attributes(s: &Store) {
    let mut cfg = s.project_config().unwrap();
    cfg.attributes = vec![
        AttributeDef {
            id: "sector".into(),
            label: "Secteur".into(),
            kind: AttributeKind::Choice,
            choices: vec!["public".into(), "privé".into()],
            shared_with_model: false,
            scope: Default::default(),
        },
        AttributeDef {
            id: "size".into(),
            label: "Taille".into(),
            kind: AttributeKind::Number,
            choices: vec![],
            shared_with_model: false,
            scope: Default::default(),
        },
    ];
    s.set_project_config(&cfg, ACTOR).unwrap();
}

pub(super) fn set_attr(s: &Store, interview: &str, attr: &str, value: &str) {
    let mut m = BTreeMap::new();
    m.insert(attr.to_string(), Some(value.to_string()));
    s.set_interview_attributes(interview, &m, ACTOR).unwrap();
}

/// A case attributing the given units (all of the same interview `owner`).
pub(super) fn make_case(s: &Store, owner: &str, label: &str, units: &[&str]) -> String {
    let mut c = case(label);
    c.units = units.iter().map(|u| u.to_string()).collect();
    for f in [
        &mut c.trigger,
        &mut c.alternatives,
        &mut c.constraints,
        &mut c.choice,
        &mut c.result,
    ] {
        f.evidence.clear();
    }
    s.create_case(owner, &c, ACTOR).unwrap().case_id
}
