//! Audit package: everything needed to reconstruct the coding of one
//! interview, written to a new folder that is never overwritten, with a
//! manifest of SHA-256 hashes.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cs_model::phase::{PHASE_1, PHASE_2};
use cs_model::{
    sha256_hex, ArbitrationChoice, Case, CodeSet, CsError, HumanCoding, HumanSnapshot, ToClarify,
};
use cs_store::Store;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct ExportedFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// Status of the detailed coding in the package.
#[derive(Debug, Serialize)]
pub struct CodingStatus {
    pub n_units_phase1: usize,
    pub n_coded_phase1: usize,
    pub n_units_phase2: usize,
    pub n_coded_phase2: usize,
    pub n_runs: usize,
    pub n_complete_runs: usize,
    pub n_disagreements: usize,
    pub n_sampled: usize,
    pub n_decided: usize,
    pub n_revised_after_ai: usize,
    pub n_to_clarify_open: usize,
    pub n_cases: usize,
    pub n_snapshots: usize,
}

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub manifest_sha256: String,
    pub status: CodingStatus,
}

pub(crate) struct Writer {
    pub dir: PathBuf,
    pub files: Vec<ExportedFile>,
}

impl Writer {
    pub fn write(&mut self, name: &str, content: &[u8]) -> Result<()> {
        let path = self.dir.join(name);
        if path.exists() {
            anyhow::bail!("refusing to overwrite {}", path.display());
        }
        fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        self.files.push(ExportedFile {
            name: name.to_string(),
            sha256: sha256_hex(content),
            size: content.len() as u64,
        });
        Ok(())
    }
}

/// Keeps a run label safe as a file name part.
fn safe(label: &str) -> String {
    let s: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "run".to_string()
    } else {
        s
    }
}

pub(crate) fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Codes of a set, sorted, joined with `;`.
fn codes_list(codes: &CodeSet) -> String {
    codes.iter().cloned().collect::<Vec<_>>().join(";")
}

/// Codes of a set, or `(none)` when empty.
fn codes_or_none(codes: &CodeSet) -> String {
    if codes.is_empty() {
        "(none)".to_string()
    } else {
        codes_list(codes)
    }
}

fn codes_text(c: &HumanCoding) -> String {
    codes_list(&c.codes)
}

fn opt_text(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// One row per case, lists joined with `;`.
fn cases_csv(cases: &[Case]) -> String {
    let mut out = String::from(
        "case_id,label,interview_id,interviews,decision,granularity,carrier_chosen,carriers_rejected,account_type,swiss_link,stage,outcome_status,n_units\n",
    );
    for c in cases {
        let row = [
            csv_field(&c.case_id),
            csv_field(&c.label),
            csv_field(&c.interview_id),
            csv_field(&c.interviews.join(";")),
            csv_field(opt_text(&c.decision)),
            csv_field(opt_text(&c.granularity)),
            csv_field(opt_text(&c.carrier_chosen)),
            csv_field(&c.carriers_rejected.join(";")),
            csv_field(opt_text(&c.account_type)),
            csv_field(opt_text(&c.swiss_link)),
            csv_field(opt_text(&c.stage)),
            csv_field(opt_text(&c.outcome_status)),
            c.units.len().to_string(),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// One row per attributed unit. The interview is the prefix before `-U`, as in the store.
fn case_units_csv(cases: &[Case]) -> String {
    let mut out = String::from("case_id,unit_id,interview_id\n");
    for c in cases {
        for unit in &c.units {
            let interview = unit.rsplit_once("-U").map_or("", |(i, _)| i);
            out.push_str(&format!(
                "{},{},{}\n",
                csv_field(&c.case_id),
                csv_field(unit),
                csv_field(interview)
            ));
        }
    }
    out
}

fn snapshot_csv(snap: &HumanSnapshot) -> String {
    let mut out = String::from("unit_id,codes\n");
    for (unit_id, codes) in &snap.codings {
        out.push_str(&format!(
            "{},{}\n",
            csv_field(unit_id),
            csv_field(&codes_list(codes))
        ));
    }
    out
}

fn decision_text(choice: &ArbitrationChoice) -> String {
    match choice {
        ArbitrationChoice::Human => "candidate's coding kept".to_string(),
        ArbitrationChoice::Ai { run_label } => format!("AI run {run_label} adopted"),
        ArbitrationChoice::Custom => "new code set decided by the candidate".to_string(),
    }
}

/// Units whose current coding differs from their snapshot, appended after the review log.
fn revisions_md(items: &[cs_model::ReviewItem]) -> String {
    let mut s = String::from("\n## Revisions since the snapshot\n\n");
    s.push_str(
        "Units whose current coding differs from the human reference frozen when their AI batch was generated. The AI answers were compared with the snapshot, not with these current codes.\n\n",
    );
    let revised: Vec<&cs_model::ReviewItem> =
        items.iter().filter(|i| i.revised_since_snapshot).collect();
    if revised.is_empty() {
        s.push_str("None.\n");
        return s;
    }
    for item in revised {
        let decision = match &item.decision {
            Some(a) => format!(
                "{}, final codes {}",
                decision_text(&a.choice),
                codes_or_none(&a.final_codes)
            ),
            None => "pending".to_string(),
        };
        s.push_str(&format!(
            "- `{}`: snapshot codes {}; current codes {}; decision: {}.\n",
            item.unit_id,
            codes_or_none(&item.human),
            codes_or_none(&item.human_current),
            decision
        ));
    }
    s
}

/// The snapshot references of the complete runs, appended after the review log.
fn snapshots_md(refs: &[cs_model::ReviewSnapshotRef]) -> String {
    let mut s = String::from("\n## Snapshots\n\n");
    s.push_str(
        "The human reference of each complete run, frozen when its prompt batch was generated. Each SHA-256 is that of the snapshot codings in the matching human_snapshot_batch<id>.json.\n\n",
    );
    if refs.is_empty() {
        s.push_str("None.\n");
        return s;
    }
    s.push_str("| Run | Snapshot id | SHA-256 |\n|---|---|---|\n");
    for r in refs {
        s.push_str(&format!(
            "| {} | {} | `{}` |\n",
            r.run_label.replace('|', "\\|"),
            r.snapshot_id,
            r.sha256
        ));
    }
    s
}

fn human_csv(order: &[String], codings: &BTreeMap<String, HumanCoding>) -> String {
    let mut out = String::from("unit_id,codes,revised_after_ai\n");
    for unit_id in order {
        if let Some(c) = codings.get(unit_id) {
            out.push_str(&format!(
                "{},{},{}\n",
                csv_field(unit_id),
                csv_field(&codes_text(c)),
                c.revised_after_ai as u8
            ));
        }
    }
    out
}

fn human_full_csv(order: &[String], codings: &BTreeMap<String, HumanCoding>) -> String {
    let mut out = String::from(
        "unit_id,codes,subcode,modifier,ev_level,provenance,memo,excerpt,revised_after_ai\n",
    );
    for unit_id in order {
        let Some(c) = codings.get(unit_id) else {
            continue;
        };
        let opt = |v: &Option<String>| csv_field(v.as_deref().unwrap_or(""));
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            csv_field(unit_id),
            csv_field(&codes_text(c)),
            opt(&c.subcode),
            opt(&c.modifier),
            opt(&c.ev_level),
            opt(&c.provenance),
            opt(&c.memo),
            opt(&c.excerpt),
            c.revised_after_ai as u8,
        ));
    }
    out
}

fn to_clarify_csv(items: &[ToClarify]) -> String {
    let mut out = String::from(
        "id,unit_id,label,note,created_at,created_by,resolution,code,rationale,resolved_at,resolved_by\n",
    );
    for t in items {
        let (kind, code, rationale, at, by) = match &t.resolved {
            Some(r) => (
                match r.kind {
                    cs_model::ResolutionKind::NewCode => "new_code",
                    cs_model::ResolutionKind::Mapped => "mapped",
                },
                r.code.as_str(),
                r.rationale.as_deref().unwrap_or(""),
                r.at.as_str(),
                r.by.as_str(),
            ),
            None => ("open", "", "", "", ""),
        };
        out.push_str(
            &[
                t.id.to_string(),
                csv_field(&t.unit_id),
                csv_field(&t.label),
                csv_field(&t.note),
                csv_field(&t.created_at),
                csv_field(&t.created_by),
                kind.to_string(),
                csv_field(code),
                csv_field(rationale),
                csv_field(at),
                csv_field(by),
            ]
            .join(","),
        );
        out.push('\n');
    }
    out
}

/// Markdown synthesis of the cases of one interview.
pub(crate) fn cases_md(interview_id: &str, cases: &[Case]) -> String {
    let mut s = format!("# Case syntheses, {interview_id}\n\n");
    if cases.is_empty() {
        s.push_str("No case recorded.\n");
        return s;
    }
    for c in cases {
        s.push_str(&format!("## {}, {}\n\n", c.case_id, c.label));
        for (name, value) in [
            ("Decision", &c.decision),
            ("Granularity", &c.granularity),
            ("Carrier chosen", &c.carrier_chosen),
            ("Account type", &c.account_type),
            ("Swiss link", &c.swiss_link),
        ] {
            if let Some(v) = value {
                s.push_str(&format!("{name}: {v}\n"));
            }
        }
        if !c.carriers_rejected.is_empty() {
            s.push_str(&format!(
                "Carriers rejected: {}\n",
                c.carriers_rejected.join("; ")
            ));
        }
        if c.units.is_empty() {
            s.push_str("Attributed units: 0.\n");
        } else {
            s.push_str(&format!(
                "Attributed units: {} ({}).\n",
                c.units.len(),
                c.units.join(", ")
            ));
        }
        s.push('\n');
        if let Some(stage) = &c.stage {
            s.push_str(&format!("Stage: {stage}. "));
        }
        if let Some(status) = &c.outcome_status {
            s.push_str(&format!("Outcome status: {status}."));
        }
        if c.stage.is_some() || c.outcome_status.is_some() {
            s.push_str("\n\n");
        }
        for (name, f) in c.fields() {
            let title = match name {
                "trigger" => "Trigger",
                "alternatives" => "Alternatives",
                "constraints" => "Constraints",
                "choice" => "Choice",
                _ => "Result",
            };
            s.push_str(&format!("### {title}\n\n"));
            s.push_str(if f.text.is_empty() {
                "(not filled)"
            } else {
                f.text.as_str()
            });
            s.push_str("\n\n");
            if !f.contradictions.is_empty() {
                s.push_str(&format!("Contradictions: {}\n\n", f.contradictions));
            }
            if !f.missing.is_empty() {
                s.push_str(&format!("Missing: {}\n\n", f.missing));
            }
            if !f.evidence.is_empty() {
                s.push_str(&format!("Evidence: {}.\n\n", f.evidence.join(", ")));
            }
        }
        if let Some(n) = &c.notes {
            s.push_str(&format!("Notes: {n}\n\n"));
        }
    }
    s
}

/// Picks `<base>/<YYYY-MM-DD>_<name>`, or a time-suffixed sibling when it exists.
pub(crate) fn target_dir(base: &Path, name: &str) -> PathBuf {
    let now = chrono::Local::now();
    let first = base.join(format!("{}_{}", now.format("%Y-%m-%d"), name));
    if !first.exists() {
        return first;
    }
    let mut n = 1;
    loop {
        let candidate = base.join(format!(
            "{}_{}_{}{}",
            now.format("%Y-%m-%d"),
            name,
            now.format("%H%M%S"),
            if n == 1 {
                String::new()
            } else {
                format!("_{n}")
            }
        ));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

pub fn write_package(
    store: &Store,
    interview_id: &str,
    base: &Path,
    db_path: &Path,
    actor: &str,
) -> Result<ExportResult> {
    let entry = store
        .interview(interview_id)?
        .ok_or_else(|| CsError::NotFound(format!("interview {interview_id}")))?;
    let seg = store.segmentation(interview_id)?.ok_or_else(|| {
        CsError::Validation(format!(
            "{interview_id} has no segmentation; nothing to export yet"
        ))
    })?;
    let all_units: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
    let p1 = store.human_codings(interview_id, PHASE_1)?;
    let p2 = store.human_codings(interview_id, PHASE_2)?;
    let scope2 = store.phase_units(interview_id, PHASE_2)?;
    let runs = store.ai_runs(interview_id)?;
    let prompts = store.prompts(interview_id)?;
    let review = store.review(interview_id)?;
    let history = store.decision_history(interview_id)?;
    let (finals, _uncoded) = store.final_codings(interview_id)?;
    let to_clarify = store.to_clarify_items(Some(interview_id), false)?;
    let cases = store.cases(Some(interview_id))?;
    let human_snaps = store.human_snapshots(interview_id)?;
    let (codebook_csv, _) = store.codebook_export()?;
    let events = store.audit_events(Some(interview_id))?;
    let chain = store.verify_chain()?;

    fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
    let dir = target_dir(base, interview_id);
    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut w = Writer {
        dir: dir.clone(),
        files: Vec::new(),
    };

    w.write("units.csv", cs_ai::units_csv(&seg).as_bytes())?;
    let p1_map = store.human_coding_map(interview_id, PHASE_1)?;
    w.write(
        "phase1_contexts.csv",
        cs_ai::coding_csv(&p1_map, &all_units).as_bytes(),
    )?;
    w.write(
        "phase2_coding_human.csv",
        human_csv(&all_units, &p2).as_bytes(),
    )?;
    w.write(
        "phase2_human_full.csv",
        human_full_csv(&all_units, &p2).as_bytes(),
    )?;

    for p in &prompts {
        let name = format!(
            "phase2_batch{}_prompt_part{}_of{}_{}.txt",
            p.batch_id,
            p.part,
            p.of,
            &p.sha256[..8]
        );
        w.write(&name, p.text.as_bytes())?;
    }

    for run in &runs {
        let tag = format!("run{}_{}", run.run_id, safe(&run.meta.run_label));
        let meta = json!({
            "run_id": run.run_id,
            "batch_id": run.batch_id,
            "meta": run.meta,
            "unit_ids": run.unit_ids,
            "parts_received": run.parts_received,
            "parts_expected": run.parts_expected,
            "complete": run.complete,
        });
        w.write(
            &format!("phase2_{tag}.json"),
            serde_json::to_string_pretty(&meta)?.as_bytes(),
        )?;
        w.write(
            &format!("phase2_coding_ai_{tag}.csv"),
            cs_ai::coding_csv(&run.codings, &run.unit_ids).as_bytes(),
        )?;
        for (part, raw, _sha, _at) in store.ai_raw_outputs(run.run_id)? {
            w.write(
                &format!("phase2_ai_{tag}_part{part}_raw.txt"),
                raw.as_bytes(),
            )?;
        }
    }

    let (snapshots, complete) = crate::candidate_runs(store, interview_id)?;
    if !complete.is_empty() {
        let order = crate::compared_units(store, interview_id)?;
        let reports = crate::agreement_reports(&snapshots, &complete, &order)?;
        w.write(
            "phase2_agreement.md",
            cs_ai::render_agreement_md(&seg, &order, &reports).as_bytes(),
        )?;
        let items: Vec<cs_model::ReviewItem> =
            review.items.iter().map(|e| e.item.clone()).collect();
        let mut review_md = cs_ai::render_review_md(&seg, &items, &history, &review.seeds);
        review_md.push_str(&revisions_md(&items));
        review_md.push_str(&snapshots_md(&review.snapshots));
        w.write("phase2_review.md", review_md.as_bytes())?;
    }
    for snap in &human_snaps {
        w.write(
            &format!("human_snapshot_batch{}.csv", snap.batch_id),
            snapshot_csv(snap).as_bytes(),
        )?;
        w.write(
            &format!("human_snapshot_batch{}.json", snap.batch_id),
            serde_json::to_string_pretty(snap)?.as_bytes(),
        )?;
    }
    w.write(
        "phase2_coding_final.csv",
        cs_ai::final_csv(&finals).as_bytes(),
    )?;
    w.write("to_clarify.csv", to_clarify_csv(&to_clarify).as_bytes())?;
    w.write(
        "cases.json",
        serde_json::to_string_pretty(&cases)?.as_bytes(),
    )?;
    w.write("cases.md", cases_md(interview_id, &cases).as_bytes())?;
    w.write("cases.csv", cases_csv(&cases).as_bytes())?;
    w.write("case_units.csv", case_units_csv(&cases).as_bytes())?;
    w.write("codebook.csv", codebook_csv.as_bytes())?;

    let mut jsonl = String::new();
    for e in &events {
        jsonl.push_str(&serde_json::to_string(e)?);
        jsonl.push('\n');
    }
    w.write("audit_log.jsonl", jsonl.as_bytes())?;
    w.write(
        "audit_chain.json",
        serde_json::to_string_pretty(&chain)?.as_bytes(),
    )?;

    let status = CodingStatus {
        n_units_phase1: all_units.len(),
        n_coded_phase1: p1.len(),
        n_units_phase2: scope2.len(),
        n_coded_phase2: store.guard(interview_id, PHASE_2)?.n_units
            - store.guard(interview_id, PHASE_2)?.missing.len(),
        n_runs: runs.len(),
        n_complete_runs: complete.len(),
        n_disagreements: review.n_disagreements,
        n_sampled: review.n_sampled,
        n_decided: review.n_decided,
        n_revised_after_ai: p2.values().filter(|c| c.revised_after_ai).count(),
        n_to_clarify_open: to_clarify.iter().filter(|t| t.resolved.is_none()).count(),
        n_cases: cases.len(),
        n_snapshots: human_snaps.len(),
    };
    w.write(
        "README.md",
        readme(interview_id, &entry.folder_name, &seg, &status).as_bytes(),
    )?;

    let manifest = json!({
        "tool": "Coding Studio",
        "version": crate::VERSION,
        "prompt_version": cs_ai::PROMPT_VERSION,
        "phase1_codebook": cs_model::phase::PHASE1_CODEBOOK_VERSION,
        "generated_at": crate::now(),
        "generated_by": actor,
        "interview_id": interview_id,
        "db_path": db_path,
        "segmentation_hash": seg.hash,
        "source": { "path": seg.source_rel_path, "sha256": seg.source_sha256 },
        "audit_chain_ok": chain.ok,
        "status": status,
        "files": w.files,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_sha = sha256_hex(&manifest_bytes);
    w.write("manifest.json", &manifest_bytes)?;

    store.log(
        actor,
        "export.package",
        Some(interview_id),
        Some(&dir.display().to_string()),
        json!({
            "dir": dir.display().to_string(),
            "manifest_sha256": manifest_sha,
            "n_files": w.files.len(),
            "status": status,
        }),
    )?;

    Ok(ExportResult {
        dir: dir.display().to_string(),
        files: w.files,
        manifest_sha256: manifest_sha,
        status,
    })
}

fn readme(id: &str, folder: &str, seg: &cs_model::Segmentation, st: &CodingStatus) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Coding audit package, {id}\n\n"));
    s.push_str(&format!(
        "Interview folder: {folder}. Segmented source: {} (SHA-256 {}). Segmentation hash: {}. Rule: {}. Units: {}.\n\n",
        seg.source_rel_path, seg.source_sha256, seg.hash, seg.rule, seg.units.len()
    ));
    s.push_str("Phase 1 is the candidate's provisional pre-classification of every unit against the three contexts of the TOE framework (TECH, ORG, ENV), plus CONTEXT for descriptive material and NA for the rest; it stays revisable and is not challenged by the AI. Phase 2 codes the units whose phase 1 category is not NA with concrete codes, each attached to a TOE context where relevant and marked as coming from the literature or from the field. A unit may carry several codes, or a provisional label to clarify that is later resolved into a new code or mapped to an existing one. An AI coder then challenges the detailed codes blind, from a generated prompt; the candidate reviews every disagreement and a random sample of agreements and keeps the final decision.\n\n");
    s.push_str(&format!(
        "Status at export: phase 1, {} of {} units coded; phase 2, {} of {} units coded, {} AI run(s) of which {} complete, {} disagreement(s) and {} sampled agreement(s), {} decision(s); {} coding(s) revised after the AI answer; {} open label(s) to clarify; {} case(s); {} human snapshot(s), one per prompt batch.\n\n",
        st.n_coded_phase1, st.n_units_phase1, st.n_coded_phase2, st.n_units_phase2, st.n_runs,
        st.n_complete_runs, st.n_disagreements, st.n_sampled, st.n_decided, st.n_revised_after_ai,
        st.n_to_clarify_open, st.n_cases, st.n_snapshots
    ));
    s.push_str("## Files\n\n");
    for (name, what) in [
        ("units.csv", "The units of analysis, `unit_id,speaker,timestamp,text`, as produced by the deterministic segmentation."),
        ("phase1_contexts.csv", "The provisional TOE pre-classification, `unit_id,codes`."),
        ("phase2_coding_human.csv", "The candidate's detailed coding, `unit_id,codes,revised_after_ai`; the last column is 1 when the coding was changed after an AI answer covering the unit."),
        ("phase2_human_full.csv", "The same coding with every field: sub-code, modifier, evidence level, provenance, memo and verbatim excerpt."),
        ("phase2_batch<b>_prompt_part<n>_of<m>_<hash>.txt", "Each prompt generated for the AI coder, byte for byte, by batch; the hash prefix is the start of its SHA-256."),
        ("phase2_run<id>_<label>.json", "Parameters of each AI run: batch answered, model, version, temperature, seed, context length, prompt version, execution time."),
        ("phase2_coding_ai_run<id>_<label>.csv", "Codes per unit returned by that run, after validation."),
        ("phase2_ai_run<id>_<label>_part<n>_raw.txt", "The AI answer exactly as pasted, before any parsing."),
        ("phase2_agreement.md", "Exact agreement and Cohen's kappa per code, candidate against each run and runs against each other; reported, not optimised."),
        ("phase2_review.md", "Every disagreement and every sampled agreement with the unit text, the competing codes, the decision and its rationale, and the sample seeds; then the units revised since their snapshot and the snapshot references."),
        ("phase2_coding_final.csv", "Final detailed coding: the review decision where one exists, otherwise the candidate's codes (`basis`)."),
        ("to_clarify.csv", "Every provisional label, its note, and how it was resolved (new code or mapped code)."),
        ("human_snapshot_batch<id>.csv", "The candidate's phase 2 coding frozen when prompt batch <id> was generated, `unit_id,codes`: the reference the AI answers of that batch were compared with."),
        ("human_snapshot_batch<id>.json", "The same snapshot with its SHA-256, the segmentation and codebook hashes, and its creation time."),
        ("cases.json, cases.md", "The case syntheses: decision, granularity, carriers chosen and rejected, account type, Swiss link, stage and outcome status; for trigger, alternatives, constraints, choice and result, the text, the contradictions, what is missing and the evidence units; and the units attributed to the case, from any interview."),
        ("cases.csv", "One row per case: identifiers, decision, granularity, carriers, account type, Swiss link, stage, outcome status and number of attributed units; lists joined with `;`."),
        ("case_units.csv", "The units attributed to each case, `case_id,unit_id,interview_id`."),
        ("codebook.csv", "The codebook at export time: TOE attachment, origin, research questions, version."),
        ("audit_log.jsonl", "Every recorded action on this interview, one JSON event per line, each chained to the previous one by SHA-256."),
        ("audit_chain.json", "Result of the verification of the whole audit chain at export time."),
        ("manifest.json", "SHA-256 and size of every file above, with the tool version and the export time."),
    ] {
        s.push_str(&format!("- `{name}`: {what}\n"));
    }
    s.push_str("\nTo check the package, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}
