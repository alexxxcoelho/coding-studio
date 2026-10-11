//! Agreement and Cohen's kappa. Deterministic arithmetic only. Divergences
//! and final coding live in `cs_model::reconcile`, shared with the store.

use std::collections::BTreeSet;

use cs_model::{AgreementReport, CodeAgreement, CodingMap};

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
