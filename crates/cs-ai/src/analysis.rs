//! Agreement, divergences and final coding. Deterministic arithmetic only.

use std::collections::{BTreeMap, BTreeSet};

use cs_model::{
    AgreementReport, Arbitration, CodeAgreement, CodeSet, CodingMap, Divergence, DivergenceKind,
    FinalBasis, FinalCoding, Segmentation,
};

/// Cohen's kappa on two boolean vectors of equal length; `None` when undefined
/// (a single level observed, `pe == 1`). Same formula as
/// `00_Tools/Scripts/compute_agreement.py`.
pub fn cohen_kappa(a: &[bool], b: &[bool]) -> Option<f64> {
    let n = a.len();
    if n == 0 {
        return None;
    }
    let nf = n as f64;
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count() as f64;
    let neither = a.iter().zip(b).filter(|(x, y)| !**x && !**y).count() as f64;
    let po = (both + neither) / nf;
    let pa = a.iter().filter(|x| **x).count() as f64 / nf;
    let pb = b.iter().filter(|x| **x).count() as f64 / nf;
    let pe = pa * pb + (1.0 - pa) * (1.0 - pb);
    if pe == 1.0 {
        return None;
    }
    Some((po - pe) / (1.0 - pe))
}

pub(crate) fn agreement(
    label_a: &str,
    a: &CodingMap,
    label_b: &str,
    b: &CodingMap,
    unit_order: &[String],
) -> AgreementReport {
    let shared: Vec<&String> = unit_order
        .iter()
        .filter(|u| a.contains_key(*u) && b.contains_key(*u))
        .collect();
    let codes: BTreeSet<&String> = shared
        .iter()
        .flat_map(|u| a[*u].iter().chain(b[*u].iter()))
        .collect();
    let n = shared.len();
    let n_exact = shared.iter().filter(|u| a[**u] == b[**u]).count();
    let per_code = codes
        .into_iter()
        .map(|code| {
            let fa: Vec<bool> = shared.iter().map(|u| a[*u].contains(code)).collect();
            let fb: Vec<bool> = shared.iter().map(|u| b[*u].contains(code)).collect();
            let same = fa.iter().zip(&fb).filter(|(x, y)| x == y).count();
            CodeAgreement {
                code: code.clone(),
                n_a: fa.iter().filter(|x| **x).count(),
                n_b: fb.iter().filter(|x| **x).count(),
                raw: if n == 0 { 0.0 } else { same as f64 / n as f64 },
                kappa: cohen_kappa(&fa, &fb),
            }
        })
        .collect();
    AgreementReport {
        label_a: label_a.to_string(),
        label_b: label_b.to_string(),
        n_units: n,
        n_exact,
        per_code,
        divergent_unit_ids: shared
            .iter()
            .filter(|u| a[**u] != b[**u])
            .map(|u| (*u).clone())
            .collect(),
    }
}

fn coded<'a>(map: &'a CodingMap, unit_id: &str) -> Option<&'a CodeSet> {
    map.get(unit_id).filter(|s| !s.is_empty())
}

/// Classifies one unit; `None` when human and every run agree. Callers make
/// sure the unit is coded on every side.
fn classify(human: &CodeSet, runs: &[(&String, &CodeSet)]) -> Option<DivergenceKind> {
    let human_matches_some = runs.iter().any(|(_, s)| *s == human);
    let human_matches_all = runs.iter().all(|(_, s)| *s == human);
    if human_matches_all {
        None
    } else if human_matches_some {
        // Human sides with at least one run, so the runs disagree among themselves.
        Some(DivergenceKind::AiRunsUnstable)
    } else {
        Some(DivergenceKind::HumanVsAi)
    }
}

pub(crate) fn divergences(
    seg: &Segmentation,
    human: &CodingMap,
    runs: &[(String, CodingMap)],
) -> Vec<Divergence> {
    if runs.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for unit in &seg.units {
        let Some(h) = coded(human, &unit.unit_id) else {
            continue;
        };
        let sets: Option<Vec<(&String, &CodeSet)>> = runs
            .iter()
            .map(|(label, map)| coded(map, &unit.unit_id).map(|s| (label, s)))
            .collect();
        let Some(sets) = sets else {
            continue;
        };
        if let Some(kind) = classify(h, &sets) {
            out.push(Divergence {
                unit_id: unit.unit_id.clone(),
                kind,
                human: h.clone(),
                ai: sets
                    .into_iter()
                    .map(|(l, s)| (l.clone(), s.clone()))
                    .collect(),
            });
        }
    }
    out
}

pub(crate) fn final_coding(
    seg: &Segmentation,
    human: &CodingMap,
    runs: &[(String, CodingMap)],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> Result<Vec<FinalCoding>, Vec<String>> {
    let mut finals = Vec::new();
    let mut pending = Vec::new();
    for unit in &seg.units {
        let id = &unit.unit_id;
        let Some(h) = coded(human, id) else {
            pending.push(id.clone());
            continue;
        };
        let sets: Option<Vec<(&String, &CodeSet)>> = runs
            .iter()
            .map(|(label, map)| coded(map, id).map(|s| (label, s)))
            .collect();
        let Some(sets) = sets else {
            // A run has not coded this unit yet: the comparison is incomplete.
            pending.push(id.clone());
            continue;
        };
        match classify(h, &sets) {
            None => finals.push(FinalCoding {
                unit_id: id.clone(),
                codes: h.clone(),
                basis: FinalBasis::Agreed,
            }),
            Some(_) => match arbitrations.get(id) {
                Some(arb) => finals.push(FinalCoding {
                    unit_id: id.clone(),
                    codes: arb.final_codes.clone(),
                    basis: FinalBasis::Arbitrated,
                }),
                None => pending.push(id.clone()),
            },
        }
    }
    if pending.is_empty() {
        Ok(finals)
    } else {
        Err(pending)
    }
}
