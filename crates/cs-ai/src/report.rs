//! CSV exports (formats of `00_Tools/Scripts`) and Markdown reports.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cs_model::{
    AgreementReport, Arbitration, ArbitrationChoice, CodeSet, CodingMap, FinalBasis, FinalCoding,
    ReviewItem, ReviewKind, Segmentation, Unit, PHASE_2,
};

use crate::prompt::{phase_title, speaker_label};

/// RFC 4180 field: quoted when it holds a comma, a quote or a line break.
pub(crate) fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn join_codes(codes: &CodeSet) -> String {
    codes
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(";")
}

fn codes_or_none(codes: &CodeSet) -> String {
    if codes.is_empty() {
        "(none)".to_string()
    } else {
        join_codes(codes)
    }
}

/// Markdown table cell: pipes escaped, line breaks flattened.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

pub(crate) fn units_csv(seg: &Segmentation) -> String {
    let mut out = String::from("unit_id,speaker,timestamp,text\n");
    for u in &seg.units {
        let _ = writeln!(
            out,
            "{},{},{},{}",
            csv_field(&u.unit_id),
            csv_field(speaker_label(u)),
            csv_field(&u.timestamp),
            csv_field(&u.text)
        );
    }
    out
}

pub(crate) fn coding_csv(coding: &CodingMap, unit_order: &[String]) -> String {
    let mut out = String::from("unit_id,codes\n");
    for id in unit_order {
        if let Some(codes) = coding.get(id) {
            let _ = writeln!(out, "{},{}", csv_field(id), csv_field(&join_codes(codes)));
        }
    }
    out
}

pub(crate) fn final_csv(finals: &[FinalCoding]) -> String {
    let mut out = String::from("unit_id,codes,basis\n");
    for f in finals {
        let basis = match f.basis {
            FinalBasis::Candidate => "candidate",
            FinalBasis::Reviewed => "reviewed",
        };
        let _ = writeln!(
            out,
            "{},{},{basis}",
            csv_field(&f.unit_id),
            csv_field(&join_codes(&f.codes))
        );
    }
    out
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "n/a".to_string()
    } else {
        format!("{:.1} %", 100.0 * n as f64 / d as f64)
    }
}

pub(crate) fn render_agreement_md(
    seg: &Segmentation,
    unit_order: &[String],
    reports: &[AgreementReport],
) -> String {
    let phase = PHASE_2;
    let units: BTreeMap<&str, &Unit> = seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# Inter-coder agreement report, {}, {}",
        seg.interview_id,
        phase_title(phase)
    );
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "The candidate's detailed coding against the blind AI coding. The figures describe how \
far the two readings coincide; they are reported, not optimised, and every disagreement is \
reviewed by the candidate. Deterministic calculation, no model involved. Kappa is Cohen's kappa \
per code on presence or absence, the form suited to multi-label coding (O'Connor and Joffe, \
2020); undefined when a single level is observed."
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "| Item | Value |");
    let _ = writeln!(t, "|---|---|");
    let _ = writeln!(t, "| Interview | {} |", cell(&seg.interview_id));
    let _ = writeln!(t, "| Source | {} |", cell(&seg.source_rel_path));
    let _ = writeln!(t, "| Phase | {} |", phase_title(phase));
    let _ = writeln!(t, "| Segmentation hash | `{}` |", seg.hash);
    let _ = writeln!(t, "| Units in the segmentation | {} |", seg.units.len());
    let _ = writeln!(t, "| Units compared with the AI | {} |", unit_order.len());
    if reports.is_empty() {
        let _ = writeln!(t);
        let _ = writeln!(t, "No comparison available yet.");
    }
    for r in reports {
        let _ = writeln!(t);
        let _ = writeln!(t, "## {} against {}", cell(&r.label_a), cell(&r.label_b));
        let _ = writeln!(t);
        let _ = writeln!(t, "| Indicator | Value |");
        let _ = writeln!(t, "|---|---|");
        let _ = writeln!(t, "| Units compared | {} |", r.n_units);
        let _ = writeln!(
            t,
            "| Units not compared | {} |",
            unit_order.len().saturating_sub(r.n_units)
        );
        let _ = writeln!(
            t,
            "| Exact agreement on the full code set | {} ({}) |",
            r.n_exact,
            pct(r.n_exact, r.n_units)
        );
        let _ = writeln!(t);
        let _ = writeln!(t, "### Agreement and kappa per code");
        let _ = writeln!(t);
        let _ = writeln!(
            t,
            "| Code | n {} | n {} | Raw agreement | Cohen's kappa |",
            cell(&r.label_a),
            cell(&r.label_b)
        );
        let _ = writeln!(t, "|---|---|---|---|---|");
        for c in &r.per_code {
            let kappa = c
                .kappa
                .map_or("undefined".to_string(), |k| format!("{k:.3}"));
            let _ = writeln!(
                t,
                "| {} | {} | {} | {:.1} % | {kappa} |",
                cell(&c.code),
                c.n_a,
                c.n_b,
                100.0 * c.raw
            );
        }
        let _ = writeln!(t);
        let _ = writeln!(t, "### Disagreements: {}", r.divergent_unit_ids.len());
        let _ = writeln!(t);
        if r.divergent_unit_ids.is_empty() {
            let _ = writeln!(t, "None.");
        } else {
            let _ = writeln!(t, "Every disagreement is reviewed by the candidate.");
            let _ = writeln!(t);
            let _ = writeln!(t, "| unit_id | Timestamp | Speaker | Text |");
            let _ = writeln!(t, "|---|---|---|---|");
            for id in &r.divergent_unit_ids {
                match units.get(id.as_str()) {
                    Some(u) => {
                        let _ = writeln!(
                            t,
                            "| {} | {} | {} | {} |",
                            cell(id),
                            cell(&u.timestamp),
                            cell(speaker_label(u)),
                            cell(&u.text)
                        );
                    }
                    None => {
                        let _ = writeln!(t, "| {} |  |  | (text unavailable) |", cell(id));
                    }
                }
            }
        }
    }
    t
}

fn describe_choice(choice: &ArbitrationChoice) -> String {
    match choice {
        ArbitrationChoice::Human => "the candidate's coding kept".to_string(),
        ArbitrationChoice::Ai { run_label } => format!("AI run {run_label} adopted"),
        ArbitrationChoice::Custom => "new code set decided by the candidate".to_string(),
    }
}

pub(crate) fn render_review_md(
    seg: &Segmentation,
    items: &[ReviewItem],
    history: &[Arbitration],
    seeds: &[(String, String)],
) -> String {
    let units: BTreeMap<&str, &Unit> = seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let n_dis = items
        .iter()
        .filter(|i| i.kind == ReviewKind::Disagreement)
        .count();
    let n_decided = items.iter().filter(|i| i.decision.is_some()).count();
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# Review log, {}, {}",
        seg.interview_id,
        phase_title(PHASE_2)
    );
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "Every unit where the candidate's detailed coding and a complete AI run differ, then a \
random sample of the units where they agree, each with the candidate's decision. The aim is a \
better reading of the units, not a higher agreement figure."
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "| Item | Value |");
    let _ = writeln!(t, "|---|---|");
    let _ = writeln!(t, "| Segmentation hash | `{}` |", seg.hash);
    let _ = writeln!(t, "| Disagreements | {n_dis} |");
    let _ = writeln!(t, "| Sampled agreements | {} |", items.len() - n_dis);
    let _ = writeln!(t, "| Decided | {n_decided} |");
    for (label, seed) in seeds {
        let _ = writeln!(
            t,
            "| Sample seed, run {} | `{seed}` (max(5, ceil(10 %)) of the agreeing units, ordered by sha256(seed + unit_id)) |",
            cell(label)
        );
    }
    for item in items {
        let _ = writeln!(t);
        let (ts, speaker, text) = match units.get(item.unit_id.as_str()) {
            Some(u) => (u.timestamp.as_str(), speaker_label(u), u.text.as_str()),
            None => ("", "", "(text unavailable)"),
        };
        let kind = match item.kind {
            ReviewKind::Disagreement => "disagreement",
            ReviewKind::AgreementSample => "sampled agreement",
        };
        let _ = writeln!(t, "## {} ({ts}, {speaker}), {kind}", item.unit_id);
        let _ = writeln!(t);
        let _ = writeln!(t, "> {}", text.replace('\n', " "));
        let _ = writeln!(t);
        let _ = writeln!(t, "| Coder | Codes |");
        let _ = writeln!(t, "|---|---|");
        let _ = writeln!(t, "| Candidate | {} |", cell(&codes_or_none(&item.human)));
        for (label, codes) in &item.ai {
            let _ = writeln!(
                t,
                "| AI {} | {} |",
                cell(label),
                cell(&codes_or_none(codes))
            );
        }
        let _ = writeln!(t);
        match &item.decision {
            Some(a) => {
                let _ = writeln!(t, "Decision: {}.", describe_choice(&a.choice));
                let _ = writeln!(t);
                let _ = writeln!(t, "Final codes: {}", codes_or_none(&a.final_codes));
                let _ = writeln!(t);
                if !a.rationale.is_empty() {
                    let _ = writeln!(t, "Rationale: {}", a.rationale.replace('\n', " "));
                    let _ = writeln!(t);
                }
                let _ = writeln!(t, "Decided by {} at {}.", a.decided_by, a.decided_at);
                let earlier = history
                    .iter()
                    .filter(|h| h.unit_id == item.unit_id)
                    .count()
                    .saturating_sub(1);
                if earlier > 0 {
                    let _ = writeln!(t);
                    let _ = writeln!(t, "Earlier decisions on this unit: {earlier}.");
                }
            }
            None => {
                let _ = writeln!(t, "Pending: no decision recorded.");
            }
        }
    }
    t
}
