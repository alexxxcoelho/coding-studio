//! CSV exports (formats of `00_Tools/Scripts`) and Markdown reports.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cs_model::{
    AgreementReport, Arbitration, ArbitrationChoice, CodeSet, CodingMap, Divergence,
    DivergenceKind, FinalBasis, FinalCoding, Segmentation, Unit,
};

use crate::prompt::speaker_label;

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
            FinalBasis::Agreed => "agreed",
            FinalBasis::Arbitrated => "arbitrated",
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

pub(crate) fn render_agreement_md(seg: &Segmentation, reports: &[AgreementReport]) -> String {
    let units: BTreeMap<&str, &Unit> = seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let mut t = String::new();
    let _ = writeln!(t, "# Inter-coder agreement report, {}", seg.interview_id);
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "Primary human coding against blind secondary AI coding. Deterministic calculation, \
no model involved. Kappa is Cohen's kappa per code on presence or absence, the form suited to \
multi-label coding (O'Connor and Joffe, 2020); undefined when a single level is observed."
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "| Item | Value |");
    let _ = writeln!(t, "|---|---|");
    let _ = writeln!(t, "| Interview | {} |", cell(&seg.interview_id));
    let _ = writeln!(t, "| Source | {} |", cell(&seg.source_rel_path));
    let _ = writeln!(t, "| Segmentation hash | `{}` |", seg.hash);
    let _ = writeln!(t, "| Units in the segmentation | {} |", seg.units.len());
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
            seg.units.len().saturating_sub(r.n_units)
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
        let _ = writeln!(t, "### Divergences: {}", r.divergent_unit_ids.len());
        let _ = writeln!(t);
        if r.divergent_unit_ids.is_empty() {
            let _ = writeln!(t, "None.");
        } else {
            let _ = writeln!(t, "Every divergence is arbitrated by hand. No sampling.");
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
        ArbitrationChoice::Human => "human coding kept".to_string(),
        ArbitrationChoice::Ai { run_label } => format!("AI run {run_label} adopted"),
        ArbitrationChoice::Custom => "new code set decided by the candidate".to_string(),
    }
}

pub(crate) fn render_adjudication_md(
    seg: &Segmentation,
    divergences: &[Divergence],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> String {
    let units: BTreeMap<&str, &Unit> = seg.units.iter().map(|u| (u.unit_id.as_str(), u)).collect();
    let n_arbitrated = divergences
        .iter()
        .filter(|d| arbitrations.contains_key(&d.unit_id))
        .count();
    let mut t = String::new();
    let _ = writeln!(t, "# Adjudication log, {}", seg.interview_id);
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "Every unit where the human coding and the AI coding differ, or where AI runs differ \
among themselves, with the decision of the candidate and its rationale."
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "| Item | Value |");
    let _ = writeln!(t, "|---|---|");
    let _ = writeln!(t, "| Segmentation hash | `{}` |", seg.hash);
    let _ = writeln!(t, "| Divergences | {} |", divergences.len());
    let _ = writeln!(t, "| Arbitrated | {n_arbitrated} |");
    let _ = writeln!(t, "| Pending | {} |", divergences.len() - n_arbitrated);
    for d in divergences {
        let _ = writeln!(t);
        let (ts, speaker, text) = match units.get(d.unit_id.as_str()) {
            Some(u) => (u.timestamp.as_str(), speaker_label(u), u.text.as_str()),
            None => ("", "", "(text unavailable)"),
        };
        let _ = writeln!(t, "## {} ({ts}, {speaker})", d.unit_id);
        let _ = writeln!(t);
        let _ = writeln!(t, "> {}", text.replace('\n', " "));
        let _ = writeln!(t);
        let kind = match d.kind {
            DivergenceKind::HumanVsAi => "human coding differs from the AI coding",
            DivergenceKind::AiRunsUnstable => {
                "AI runs differ among themselves; the human coding matches at least one run"
            }
        };
        let _ = writeln!(t, "Divergence: {kind}.");
        let _ = writeln!(t);
        let _ = writeln!(t, "| Coder | Codes |");
        let _ = writeln!(t, "|---|---|");
        let _ = writeln!(t, "| Human | {} |", cell(&codes_or_none(&d.human)));
        for (label, codes) in &d.ai {
            let _ = writeln!(
                t,
                "| AI {} | {} |",
                cell(label),
                cell(&codes_or_none(codes))
            );
        }
        let _ = writeln!(t);
        match arbitrations.get(&d.unit_id) {
            Some(a) => {
                let _ = writeln!(t, "Decision: {}.", describe_choice(&a.choice));
                let _ = writeln!(t);
                let _ = writeln!(t, "Final codes: {}", codes_or_none(&a.final_codes));
                let _ = writeln!(t);
                let _ = writeln!(t, "Rationale: {}", a.rationale.replace('\n', " "));
                let _ = writeln!(t);
                let _ = writeln!(t, "Decided by {} at {}.", a.decided_by, a.decided_at);
            }
            None => {
                let _ = writeln!(t, "Pending: no decision recorded.");
            }
        }
    }
    t
}
