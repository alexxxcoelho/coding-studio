//! Disagreements, agreement samples and final coding, shared by the store and
//! the AI layer so that the figures the database works on and the figures the
//! reports print cannot differ.
//!
//! A unit is a disagreement when the candidate's set differs from the set of
//! at least one complete AI run (`HumanVsAi` when it matches no run,
//! `AiRunsUnstable` when it matches some runs only). Only units coded by the
//! candidate and by every run are compared.

use std::collections::BTreeMap;

use crate::{
    sha256_hex, Arbitration, CodeSet, CodingMap, Divergence, DivergenceKind, FinalBasis,
    FinalCoding,
};

fn coded<'a>(map: &'a CodingMap, unit_id: &str) -> Option<&'a CodeSet> {
    map.get(unit_id).filter(|s| !s.is_empty())
}

/// `None` when the candidate and every run agree.
fn classify(human: &CodeSet, runs: &[(&String, &CodeSet)]) -> Option<DivergenceKind> {
    if runs.iter().all(|(_, s)| *s == human) {
        None
    } else if runs.iter().any(|(_, s)| *s == human) {
        Some(DivergenceKind::AiRunsUnstable)
    } else {
        Some(DivergenceKind::HumanVsAi)
    }
}

fn run_sets<'a>(
    runs: &'a [(String, CodingMap)],
    unit_id: &str,
) -> Option<Vec<(&'a String, &'a CodeSet)>> {
    runs.iter()
        .map(|(label, map)| coded(map, unit_id).map(|s| (label, s)))
        .collect()
}

/// Disagreements, in `unit_order`. Empty when there is no run.
pub fn divergences(
    unit_order: &[String],
    human: &CodingMap,
    runs: &[(String, CodingMap)],
) -> Vec<Divergence> {
    if runs.is_empty() {
        return Vec::new();
    }
    unit_order
        .iter()
        .filter_map(|id| {
            let h = coded(human, id)?;
            let sets = run_sets(runs, id)?;
            classify(h, &sets).map(|kind| Divergence {
                unit_id: id.clone(),
                kind,
                human: h.clone(),
                ai: sets
                    .into_iter()
                    .map(|(l, s)| (l.clone(), s.clone()))
                    .collect(),
            })
        })
        .collect()
}

/// Units, in `unit_order`, on which the candidate and one run agree.
pub fn agreements(unit_order: &[String], human: &CodingMap, run: &CodingMap) -> Vec<String> {
    unit_order
        .iter()
        .filter(|id| matches!((coded(human, id), coded(run, id)), (Some(h), Some(r)) if h == r))
        .cloned()
        .collect()
}

/// Seed of the agreement sample of a run: first 16 hex characters of
/// sha256(run_id followed by the segmentation hash).
pub fn sample_seed(run_id: i64, segmentation_hash: &str) -> String {
    sha256_hex(format!("{run_id}{segmentation_hash}").as_bytes())[..16].to_string()
}

/// Size of an agreement sample: max(5, ceil(10 %)) capped at the number of agreements.
pub fn sample_size(n_agreements: usize) -> usize {
    n_agreements.min(5.max(n_agreements.div_ceil(10)))
}

/// Deterministic sample: agreements ordered by sha256(seed + unit_id), the
/// first `sample_size` kept, returned in their original order.
pub fn sample_agreements(seed: &str, agreeing: &[String]) -> Vec<String> {
    let mut keyed: Vec<(String, &String)> = agreeing
        .iter()
        .map(|u| (sha256_hex(format!("{seed}{u}").as_bytes()), u))
        .collect();
    keyed.sort();
    let chosen: Vec<&String> = keyed
        .into_iter()
        .take(sample_size(agreeing.len()))
        .map(|(_, u)| u)
        .collect();
    agreeing
        .iter()
        .filter(|u| chosen.contains(u))
        .cloned()
        .collect()
}

/// Final coding of `unit_order`: the review decision where one exists,
/// otherwise the candidate's current codes. The second vector lists the units
/// that have neither.
pub fn final_coding(
    unit_order: &[String],
    human: &CodingMap,
    decisions: &BTreeMap<String, Arbitration>,
) -> (Vec<FinalCoding>, Vec<String>) {
    let mut finals = Vec::new();
    let mut uncoded = Vec::new();
    for id in unit_order {
        if let Some(d) = decisions.get(id) {
            finals.push(FinalCoding {
                unit_id: id.clone(),
                codes: d.final_codes.clone(),
                basis: FinalBasis::Reviewed,
            });
        } else if let Some(h) = coded(human, id) {
            finals.push(FinalCoding {
                unit_id: id.clone(),
                codes: h.clone(),
                basis: FinalBasis::Candidate,
            });
        } else {
            uncoded.push(id.clone());
        }
    }
    (finals, uncoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArbitrationChoice;

    fn set(codes: &[&str]) -> CodeSet {
        codes.iter().map(|s| s.to_string()).collect()
    }
    fn map(pairs: &[(&str, &[&str])]) -> CodingMap {
        pairs.iter().map(|(u, c)| (u.to_string(), set(c))).collect()
    }
    fn ids(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("u{i:03}")).collect()
    }

    #[test]
    fn kinds_and_finals() {
        let order: Vec<String> = ["u1", "u2", "u3", "u4"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let human = map(&[("u1", &["A"]), ("u2", &["B"]), ("u3", &["NA"])]);
        let r1 = map(&[("u1", &["A"]), ("u2", &["C"]), ("u3", &["NA"])]);
        let r2 = map(&[("u1", &["A"]), ("u2", &["B"]), ("u3", &["C"])]);
        let runs = vec![("R1".to_string(), r1.clone()), ("R2".to_string(), r2)];
        let d = divergences(&order, &human, &runs);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].kind, DivergenceKind::AiRunsUnstable);
        assert_eq!(agreements(&order, &human, &r1), ["u1", "u3"]);

        let mut decisions = BTreeMap::new();
        decisions.insert(
            "u2".to_string(),
            Arbitration {
                unit_id: "u2".into(),
                phase: 2,
                choice: ArbitrationChoice::Custom,
                final_codes: set(&["D"]),
                rationale: "r".into(),
                decided_by: "c".into(),
                decided_at: "t".into(),
            },
        );
        let (finals, uncoded) = final_coding(&order, &human, &decisions);
        assert_eq!(uncoded, ["u4"]);
        assert_eq!(finals[0].basis, FinalBasis::Candidate);
        assert_eq!(finals[1].codes, set(&["D"]));
        assert_eq!(finals[1].basis, FinalBasis::Reviewed);
    }

    #[test]
    fn sample_size_rule() {
        assert_eq!(sample_size(0), 0);
        assert_eq!(sample_size(3), 3);
        assert_eq!(sample_size(5), 5);
        assert_eq!(sample_size(49), 5);
        assert_eq!(sample_size(51), 6);
        assert_eq!(sample_size(120), 12);
    }

    #[test]
    fn sample_is_deterministic_and_in_order() {
        let agreeing = ids(80);
        let seed = sample_seed(7, "abc");
        assert_eq!(seed.len(), 16);
        let a = sample_agreements(&seed, &agreeing);
        assert_eq!(a, sample_agreements(&seed, &agreeing));
        assert_eq!(a.len(), 8);
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(a, sorted);
        assert_ne!(a, sample_agreements(&sample_seed(8, "abc"), &agreeing));
    }
}
