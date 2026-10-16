//! Deterministic analysis report. Copies saved interpretation, never generates findings.
use anyhow::Result;
use cs_model::analysis::LinkTarget;
use cs_store::{MatrixAxis, MatrixConfig};
use cs_store::{Store, WriteupBlock, WriteupCite, WriteupDoc};

fn heading(doc: &mut WriteupDoc, level: u8, text: impl Into<String>) {
    doc.blocks.push(WriteupBlock::Heading {
        level,
        text: text.into(),
    });
}
fn para(doc: &mut WriteupDoc, text: impl Into<String>) {
    let text = text.into();
    for line in text.split("\n\n") {
        doc.blocks.push(WriteupBlock::Para {
            text: if line.trim().is_empty() {
                "Not documented.".into()
            } else {
                line.into()
            },
            italic: false,
            cites: vec![],
        });
    }
}
fn source(
    doc: &mut WriteupDoc,
    passage_id: &str,
    interview_id: &str,
    unit_id: &str,
    quote: &str,
    note: String,
) -> usize {
    let prefix = format!("passage {passage_id};");
    if let Some(c) = doc.cites.iter_mut().find(|c| c.note.starts_with(&prefix)) {
        return c.n;
    }
    let n = doc.cites.len() + 1;
    doc.cites.push(WriteupCite {
        n,
        interview_id: interview_id.into(),
        unit_id: unit_id.into(),
        quote: quote.into(),
        note: format!("{prefix} {note}"),
    });
    n
}
fn evidence(doc: &mut WriteupDoc, links: &[cs_store::LinkView]) {
    let mut roles: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for l in links {
        let n = source(
            doc,
            &l.passage.passage_id,
            &l.passage.interview_id,
            &l.passage.unit_id,
            &l.passage.text,
            format!(
                "verification: {}; stale: {}",
                l.passage.verification.as_deref().unwrap_or("unverified"),
                l.passage.stale
            ),
        );
        let group = roles.entry(l.link_type.clone()).or_default();
        if !group.contains(&n) {
            group.push(n);
        }
    }
    if !roles.is_empty() {
        para(
            doc,
            format!(
                "Evidence: {}.",
                roles
                    .iter()
                    .map(|(role, numbers)| format!(
                        "{role} {}",
                        numbers
                            .iter()
                            .map(|n| format!("[{n}]"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    }
}

pub(crate) fn document(store: &Store, actor: &str) -> Result<WriteupDoc> {
    let mut doc = WriteupDoc {
        blocks: vec![],
        cites: vec![],
    };
    heading(&mut doc, 1, "Analysis report and evidence register");
    para(&mut doc, "This report assembles the analysis and writing currently saved in Coding Studio. It introduces no new findings and uses no generative model at export time. Saved text may have been prepared with AI assistance. Exporting it does not establish independent researcher validation, audio verification, saturation or causal inference. The recorded provenance and verification states remain authoritative.");
    para(&mut doc, "Interview and unit identifiers provide pseudonymous references. Configured pseudonymisation rules are applied to prose and quotations. Masked quotations are not verbatim originals; consult the source before citation. Unlisted identifying details require a confidentiality review before circulation.");
    heading(&mut doc, 2, "Corpus and analytical scope");
    let progress = store.library_progress()?;
    let totals = &progress.totals;
    para(&mut doc, format!("The corpus contains {} interviews and {} respondent units. {} units are recorded as in scope, {} as outside scope and {} as coded in the unit grid. {} units are marked for review. These are workflow counts, not evidence of theoretical saturation or independent validation.", totals.interviews, totals.respondent_units, totals.relevant, totals.not_relevant, totals.coded, totals.to_review));
    doc.blocks.push(WriteupBlock::Table {
        header: vec![
            "Interview".into(),
            "Turns".into(),
            "In scope / coded".into(),
            "Review".into(),
        ],
        rows: progress
            .interviews
            .iter()
            .map(|i| {
                vec![
                    i.interview_id.clone(),
                    i.respondent_units.to_string(),
                    format!("{} / {}", i.relevant, i.coded),
                    format!("{}; {} marked", i.review.state, i.to_review),
                ]
            })
            .collect(),
    });
    let themes = store.themes(None)?;
    let links = store.links()?;
    heading(&mut doc, 2, "Findings from retained themes");
    let part_titles = [
        ("analytical_statement", "Analytical statement"),
        ("mechanism_evidence", "Mechanism and evidence"),
        ("variations_divergent", "Variation and divergent cases"),
        ("competing_limits", "Competing explanations and limitations"),
        ("mini_synthesis", "Synthesis"),
    ];
    for t in themes.iter().filter(|t| t.status == "retained") {
        heading(&mut doc, 2, format!("{}: {}", t.theme_id, t.title));
        para(&mut doc, format!("Retained theme. Checks recorded: {}/{}. Code coverage: {} passages in {} interviews. Coverage indicates retrieval through associated codes; it does not establish that every passage supports the complete proposition.", t.checks_done, t.checks_total, t.counts.passages, t.counts.interviews));
        let writing = store.theme_writing(&t.theme_id)?;
        for (key, title) in part_titles {
            heading(&mut doc, 3, title);
            let part = &writing.parts[key];
            para(&mut doc, &part.text);
            if let Some(n) = &part.nature {
                para(&mut doc, format!("Recorded evidence nature: {n}."));
            }
            evidence(&mut doc, &part.links);
        }
        let saved: Vec<_> = writing.parts.values().map(|p| p.text.as_str()).collect();
        for (name, text) in [
            ("Conditions", &t.conditions),
            ("Alternative interpretation", &t.alternative_interpretation),
            ("Limits", &t.limits),
        ] {
            if !text.is_empty() && !saved.iter().any(|part| part.contains(text.as_str())) {
                para(&mut doc, format!("{name}: {text}"));
            }
        }
        para(
            &mut doc,
            format!(
                "Supporting cases: {}. Divergent cases: {}. Checks: {}.",
                t.supporting_cases.join(", "),
                t.divergent_cases.join(", "),
                t.checks
                    .iter()
                    .map(|c| format!("{}={}", c.id, if c.done { "done" } else { "pending" }))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    }
    if !themes.iter().any(|t| t.status == "retained") {
        para(&mut doc, "No retained themes have been recorded.");
    }
    heading(&mut doc, 2, "Answers to the research questions");
    for q in store.research_questions()? {
        let s = store.question_synthesis(&q.id)?;
        heading(&mut doc, 3, format!("{}: {}", q.id, q.text));
        para(&mut doc, s.answer);
        para(&mut doc, format!("Limits: {}", s.limits));
        evidence(&mut doc, &s.links);
    }
    heading(&mut doc, 2, "Cross-case comparison");
    para(&mut doc, "Counts describe coded passage coverage, not prevalence in a population, causal effect, importance or an independent test of a theme. Overlapping codes and themes mean columns must not be summed. A zero or a non-mentioned judgement is not evidence of absence. Manual judgements (present, not mentioned, contradicted, uncertain) are distinct from calculated coverage. Missing judgements are shown as unassessed. The unit of comparison is an interview, not necessarily an independent organisation or decision.");
    let matrix = store.compute_matrix(&MatrixConfig {
        rows: vec![MatrixAxis {
            kind: "interviews".into(),
            ..Default::default()
        }],
        cols: vec![MatrixAxis {
            kind: "themes".into(),
            status: Some("retained".into()),
            ..Default::default()
        }],
        ..Default::default()
    })?;
    for (j, col) in matrix.cols.iter().enumerate() {
        heading(&mut doc, 3, &col.label);
        doc.blocks.push(WriteupBlock::Table {
            header: vec![
                "Interview".into(),
                "Count".into(),
                "Judgement".into(),
                "Recorded comparison".into(),
            ],
            rows: matrix
                .rows
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let c = &matrix.cells[i][j];
                    vec![
                        r.id.clone(),
                        c.passages.to_string(),
                        c.value.as_deref().unwrap_or("unassessed").replace('_', " "),
                        c.summary.clone(),
                    ]
                })
                .collect(),
        });
    }
    heading(&mut doc, 2, "Additional comparison views");
    para(&mut doc, "Full saved-view matrices, configuration, filters, typed-link notes and analytical decisions are retained in the accompanying analysis-evidence.json. They are not repeated here. Counts describe coverage only and overlapping views must not be summed.");
    let mut view_rows = vec![];
    for view in store.matrix_views()? {
        let state = match store.compute_matrix(&view.config) {
            Ok(m) => format!(
                "{} rows; {} columns; {} interviews in scope",
                m.rows.len(),
                m.cols.len(),
                m.interviews_in_scope.len()
            ),
            Err(_) => "Unable to recompute; review the saved configuration".into(),
        };
        view_rows.push(vec![view.view_id, view.name, state]);
    }
    if !view_rows.is_empty() {
        doc.blocks.push(WriteupBlock::Table {
            header: vec!["View".into(), "Name".into(), "Scope".into()],
            rows: view_rows,
        });
    }
    // Register unique matrix passages once; full cell-to-passage mapping is in JSON.
    for (_row, _col, pid) in store.matrix_cell_links()? {
        let p = store.passage(&pid)?;
        let verification = store
            .verification_status(&p.interview_id)?
            .into_iter()
            .find(|v| v.unit_id == p.unit_id)
            .map(|v| {
                if v.stale {
                    "unverified".into()
                } else {
                    v.state
                }
            })
            .unwrap_or_else(|| "unverified".into());
        let n = source(
            &mut doc,
            &pid,
            &p.interview_id,
            &p.unit_id,
            &p.text,
            format!("verification: {verification}; stale: {}", p.stale),
        );
        let _ = n;
    }
    if let Ok(framework) = store.framework_table() {
        heading(&mut doc, 2, "Interpretive framework");
        para(&mut doc, "The framework is a later interpretive reading of the themes. Its dimensions are not descriptive codes and do not establish a mechanism by themselves.");
        for row in framework.rows {
            heading(&mut doc, 3, row.title);
            for d in &framework.dimensions {
                let c = row.reading.cells.get(&d.id).cloned().unwrap_or_default();
                para(
                    &mut doc,
                    format!("{}: explains = {}; {}", d.label, c.explains, c.note),
                );
            }
            para(
                &mut doc,
                format!(
                    "Outside framework: {}; {}. Complementary dimension: {}. Evidence limit: {}.",
                    row.reading.out_of_frame,
                    row.reading.out_of_frame_note,
                    row.reading.complementary_dimension,
                    row.reading.evidence_limit
                ),
            );
        }
    }
    heading(&mut doc, 2, "Documented decision episodes");
    for c in store.cases(None)? {
        heading(&mut doc, 3, format!("{} ({})", c.case_id, c.interview_id));
        para(
            &mut doc,
            format!(
                "Recorded outcome: {}. Decision: {}",
                c.outcome.as_deref().unwrap_or("undetermined"),
                c.decision.as_deref().unwrap_or("Not documented.")
            ),
        );
        para(
            &mut doc,
            format!(
                "Recorded characteristics: {}",
                c.characteristics
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
        for (key, field) in [
            ("Trigger", &c.trigger),
            ("Alternatives", &c.alternatives),
            ("Constraints", &c.constraints),
            ("Choice", &c.choice),
            ("Result", &c.result),
        ] {
            para(
                &mut doc,
                format!(
                    "{key}: {}. Contradictions: {}. Missing: {}. Source units: {}.",
                    field.text,
                    field.contradictions,
                    field.missing,
                    field.evidence.join(", ")
                ),
            );
        }
        para(
            &mut doc,
            format!(
                "Source units: {}. Passage IDs: {}.",
                c.units.join(", "),
                c.passages.join(", ")
            ),
        );
    }
    heading(&mut doc, 2, "Codebook and descriptive coverage");
    para(&mut doc, "Only active, non-system codes are included. Parent codes organise navigation; direct counts differ from counts including descendants. Descriptive dimensions may co-occur on the same passage. Code frequency does not measure explanatory strength.");
    for c in store
        .code_tree()?
        .codes
        .into_iter()
        .filter(|c| c.active && !c.system)
    {
        heading(&mut doc, 3, format!("{}: {}", c.code_id, c.name));
        para(&mut doc, &c.definition);
        para(&mut doc, format!("Include: {}. Exclude: {}. Parent: {}. Direct coverage: {} passages, {} interviews. Including descendants: {} passages, {} interviews.", c.includes.trim().trim_end_matches('.'), c.excludes.trim().trim_end_matches('.'), c.parent_id.as_deref().unwrap_or("none"), c.counts.passages, c.counts.interviews, c.counts.passages_rollup, c.counts.interviews_rollup));
    }
    if themes.iter().any(|t| t.status != "retained") {
        heading(&mut doc, 2, "Analytical work not retained");
    }
    for t in themes.iter().filter(|t| t.status != "retained") {
        para(
            &mut doc,
            format!(
                "{}: {}. Status: {}. Proposition: {}. Decision memo: {}",
                t.theme_id, t.title, t.status, t.proposition, t.decision_memo
            ),
        );
    }
    heading(&mut doc, 2, "Audit and traceability");
    para(&mut doc, format!("{} active typed evidence links were recorded. References retain interview, unit, passage and link identifiers, link role, current source verification and stale status. Supporting, nuancing and contradicting roles are not interchangeable. Audio verification must be checked separately before thesis quotation.", links.len()));
    if let Some(a) = store.latest_final_audit()? {
        para(&mut doc, format!("Latest recorded audit: {} sampled statements; seed {}. This sample is not validation of every statement or of the whole corpus.", a.n, a.seed));
        for c in a.claims {
            para(
                &mut doc,
                format!(
                    "{}: {}. Verdict: {}",
                    c.part_title,
                    c.sentence,
                    c.verdict
                        .map(|v| format!("{}: {}", v.verdict, v.note))
                        .unwrap_or_else(|| "not assessed".into())
                ),
            );
        }
    }
    // Preserve links outside retained writing in a separate register; do not
    // silently interpret a candidate theme as an established finding.

    let remaining: Vec<_> = links.into_iter().filter(|l| matches!(&l.target, LinkTarget::ThemePart { theme_id, .. } if !themes.iter().any(|t| t.theme_id == *theme_id && t.status == "retained"))).collect();
    if !remaining.is_empty() {
        heading(&mut doc, 2, "Evidence register outside retained findings");
        evidence(&mut doc, &remaining);
    }
    let mut timestamps = std::collections::BTreeMap::new();
    for cite in &mut doc.cites {
        if !timestamps.contains_key(&cite.interview_id) {
            let units = store
                .segmentation(&cite.interview_id)?
                .map(|s| s.units)
                .unwrap_or_default();
            timestamps.insert(
                cite.interview_id.clone(),
                units
                    .into_iter()
                    .map(|u| (u.unit_id, u.timestamp))
                    .collect::<std::collections::BTreeMap<_, _>>(),
            );
        }
        let time = timestamps
            .get(&cite.interview_id)
            .and_then(|u| u.get(&cite.unit_id))
            .filter(|t| !t.trim().is_empty())
            .map(|s| s.as_str())
            .unwrap_or("unavailable");
        cite.note.push_str(&format!(
            "; source timestamp: {time} (not an audio verification)"
        ));
        if let Some(pid) = cite
            .note
            .strip_prefix("passage ")
            .and_then(|s| s.split(';').next())
        {
            let passage = store.passage(pid)?;
            cite.note.push_str(&format!("; source: unit excerpt, scalar offsets {}..{}; recorded origin: {}; decision role: {}", passage.start, passage.end, passage.origin.as_deref().unwrap_or("unspecified"), passage.role.as_deref().unwrap_or("unspecified")));
        }
    }
    crate::export_v2::mask_document(store, &doc, actor)
}

/// Complete machine-readable companion; detailed audit/provenance stays out of prose.
pub(crate) fn evidence_value(store: &Store, actor: &str) -> Result<serde_json::Value> {
    let mut views = vec![];
    for view in store.matrix_views()? {
        let matrix = store.compute_matrix(&view.config).ok();
        views.push(serde_json::json!({"view": view, "matrix": matrix}));
    }
    let value = serde_json::json!({
        "kind": "analysis_evidence", "runtime_ai": false,
        "analysis": crate::export_analysis::state_parts(store)?,
        "matrix_views_computed": views,
        "cases": store.cases(None)?,
        "active_codebook": store.code_tree()?.codes.into_iter().filter(|c| c.active && !c.system).collect::<Vec<_>>()
    });
    crate::export_v2::mask_value(store, &value, actor)
}
