//! Divergences and final coding, shared by the store (to enforce the phase
//! locks) and the AI layer (to report). One implementation, so the numbers
//! the database guards on and the numbers the reports print cannot differ.
//!
//! A unit diverges when the candidate's set differs from an AI run's set, or
//! when the candidate agrees with some runs but the runs disagree among
//! themselves. Only units coded by the candidate and by every run are compared.

use std::collections::BTreeMap;

use crate::{Arbitration, CodeSet, CodingMap, Divergence, DivergenceKind, FinalBasis, FinalCoding};

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

/// Divergent units, in `unit_order`. Empty when there is no run.
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

/// Final coding of every unit in `unit_order`: the common set when everyone
/// agrees, the arbitration otherwise. Err lists the units still pending (not
/// coded by the candidate, not coded by a run, or diverging without decision).
/// With no run at all, the candidate's coding is final.
pub fn final_coding(
    unit_order: &[String],
    human: &CodingMap,
    runs: &[(String, CodingMap)],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> Result<Vec<FinalCoding>, Vec<String>> {
    let mut finals = Vec::new();
    let mut pending = Vec::new();
    for id in unit_order {
        let Some(h) = coded(human, id) else {
            pending.push(id.clone());
            continue;
        };
        let Some(sets) = run_sets(runs, id) else {
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

    #[test]
    fn kinds_and_finals() {
        let order: Vec<String> = ["u1", "u2", "u3"].iter().map(|s| s.to_string()).collect();
        let human = map(&[("u1", &["TECH"]), ("u2", &["ORG"]), ("u3", &["NA"])]);
        let r1 = map(&[("u1", &["TECH"]), ("u2", &["ENV"]), ("u3", &["NA"])]);
        let r2 = map(&[("u1", &["TECH"]), ("u2", &["ORG"]), ("u3", &["ENV"])]);
        let runs = vec![("R1".to_string(), r1), ("R2".to_string(), r2)];
        let d = divergences(&order, &human, &runs);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].kind, DivergenceKind::AiRunsUnstable);
        assert_eq!(d[0].unit_id, "u2");

        let mut arbs = BTreeMap::new();
        assert_eq!(
            final_coding(&order, &human, &runs, &arbs).unwrap_err(),
            ["u2", "u3"]
        );
        for u in ["u2", "u3"] {
            arbs.insert(
                u.to_string(),
                Arbitration {
                    unit_id: u.into(),
                    phase: 1,
                    choice: ArbitrationChoice::Human,
                    final_codes: human[u].clone(),
                    rationale: "r".into(),
                    decided_by: "c".into(),
                    decided_at: "t".into(),
                },
            );
        }
        let f = final_coding(&order, &human, &runs, &arbs).unwrap();
        assert_eq!(f[0].basis, FinalBasis::Agreed);
        assert_eq!(f[1].basis, FinalBasis::Arbitrated);
    }

    #[test]
    fn no_run_means_human_is_final() {
        let order = vec!["u1".to_string()];
        let human = map(&[("u1", &["TECH"])]);
        let f = final_coding(&order, &human, &[], &BTreeMap::new()).unwrap();
        assert_eq!(f[0].codes, set(&["TECH"]));
    }
}
