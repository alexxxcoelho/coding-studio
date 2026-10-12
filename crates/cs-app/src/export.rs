//! Audit package: everything needed to reconstruct the coding of one
//! interview, written to a new folder that is never overwritten, with a
//! manifest of SHA-256 hashes.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cs_model::phase::{PHASE_1, PHASE_2};
use cs_model::{sha256_hex, CsError, HumanCoding};
use cs_store::Store;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct ExportedFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// Status of one coding phase in the package.
#[derive(Debug, Serialize)]
pub struct PhaseExport {
    pub phase: u8,
    pub n_units: usize,
    pub n_runs: usize,
    pub n_complete_runs: usize,
    pub n_divergences: usize,
    pub reconciled: bool,
    pub pending_units: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub manifest_sha256: String,
    pub phases: Vec<PhaseExport>,
}

struct Writer {
    dir: PathBuf,
    files: Vec<ExportedFile>,
}

impl Writer {
    fn write(&mut self, name: &str, content: &[u8]) -> Result<()> {
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

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn human_full_csv(
    order: &[String],
    codings: &std::collections::BTreeMap<String, HumanCoding>,
) -> String {
    let mut out = String::from("unit_id,codes,subcode,modifier,ev_level,provenance,memo,excerpt\n");
    for unit_id in order {
        let Some(c) = codings.get(unit_id) else {
            continue;
        };
        let codes = c.codes.iter().cloned().collect::<Vec<_>>().join(";");
        let opt = |v: &Option<String>| csv_field(v.as_deref().unwrap_or(""));
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_field(unit_id),
            csv_field(&codes),
            opt(&c.subcode),
            opt(&c.modifier),
            opt(&c.ev_level),
            opt(&c.provenance),
            opt(&c.memo),
            opt(&c.excerpt),
        ));
    }
    out
}

/// Picks `<base>/<YYYY-MM-DD>_<ID>`, or a time-suffixed sibling when it exists.
fn target_dir(base: &Path, interview_id: &str) -> PathBuf {
    let now = chrono::Local::now();
    let first = base.join(format!("{}_{}", now.format("%Y-%m-%d"), interview_id));
    if !first.exists() {
        return first;
    }
    let stamped = base.join(format!(
        "{}_{}_{}",
        now.format("%Y-%m-%d"),
        interview_id,
        now.format("%H%M%S")
    ));
    if !stamped.exists() {
        return stamped;
    }
    let mut n = 2;
    loop {
        let candidate = base.join(format!(
            "{}_{}_{}_{n}",
            now.format("%Y-%m-%d"),
            interview_id,
            now.format("%H%M%S")
        ));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Writes the files of one phase; returns its status.
fn write_phase(
    w: &mut Writer,
    store: &Store,
    interview_id: &str,
    seg: &cs_model::Segmentation,
    phase: u8,
) -> Result<PhaseExport> {
    let order = store.phase_units(interview_id, phase)?;
    let human_full = store.human_codings(interview_id, phase)?;
    let human = store.human_coding_map(interview_id, phase)?;
    let runs = store.ai_runs(interview_id, phase)?;
    let prompts = store.prompts(interview_id, phase)?;
    let pre = format!("phase{phase}_");

    if !human_full.is_empty() || !runs.is_empty() {
        w.write(
            &format!("{pre}coding_human.csv"),
            cs_ai::coding_csv(&human, &order).as_bytes(),
        )?;
        w.write(
            &format!("{pre}human_full.csv"),
            human_full_csv(&order, &human_full).as_bytes(),
        )?;
    }

    let mut seen_prompts = BTreeSet::new();
    for p in &prompts {
        if !seen_prompts.insert(p.sha256.clone()) {
            continue;
        }
        let name = format!(
            "{pre}prompt_part{}_of{}_{}.txt",
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
            "phase": run.phase,
            "meta": run.meta,
            "parts_received": run.parts_received,
            "parts_expected": run.parts_expected,
            "complete": run.complete,
        });
        w.write(
            &format!("{pre}{tag}.json"),
            serde_json::to_string_pretty(&meta)?.as_bytes(),
        )?;
        w.write(
            &format!("{pre}coding_ai_{tag}.csv"),
            cs_ai::coding_csv(&run.codings, &order).as_bytes(),
        )?;
        for (part, raw, _sha, _at) in store.ai_raw_outputs(run.run_id)? {
            w.write(&format!("{pre}ai_{tag}_part{part}_raw.txt"), raw.as_bytes())?;
        }
    }

    let complete: Vec<(String, cs_model::CodingMap)> = runs
        .iter()
        .filter(|r| r.complete)
        .map(|r| (r.meta.run_label.clone(), r.codings.clone()))
        .collect();
    let arbitrations = store.arbitrations(interview_id, phase)?;
    let divergences = cs_ai::divergences(&order, &human, &complete);
    if !complete.is_empty() {
        let reports = crate::agreement_reports(&human, &complete, &order);
        w.write(
            &format!("{pre}agreement.md"),
            cs_ai::render_agreement_md(seg, phase, &order, &reports).as_bytes(),
        )?;
        w.write(
            &format!("{pre}adjudication_log.md"),
            cs_ai::render_adjudication_md(seg, phase, &divergences, &arbitrations).as_bytes(),
        )?;
    }

    let (reconciled, pending) = match store.final_codings(interview_id, phase)? {
        Ok(finals) => {
            w.write(
                &format!("{pre}coding_final.csv"),
                cs_ai::final_csv(&finals).as_bytes(),
            )?;
            (true, Vec::new())
        }
        Err(pending) => (false, pending),
    };

    Ok(PhaseExport {
        phase,
        n_units: order.len(),
        n_runs: runs.len(),
        n_complete_runs: complete.len(),
        n_divergences: divergences.len(),
        reconciled,
        pending_units: pending,
    })
}

fn inductive_log_csv(codebook: &cs_model::Codebook) -> String {
    let mut out = String::from("code,label,definition,example,reason,created_at\n");
    for c in codebook.codes.iter().filter(|c| c.origin == "inductive") {
        let opt = |v: &Option<String>| csv_field(v.as_deref().unwrap_or(""));
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_field(&c.code),
            csv_field(&c.label),
            csv_field(&c.definition),
            opt(&c.example),
            opt(&c.reason),
            opt(&c.created_at),
        ));
    }
    out
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
    let codebook = store.codebook()?;
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
    let phases = vec![
        write_phase(&mut w, store, interview_id, &seg, PHASE_1)?,
        write_phase(&mut w, store, interview_id, &seg, PHASE_2)?,
    ];
    w.write("inductive_log.csv", inductive_log_csv(&codebook).as_bytes())?;

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
    w.write(
        "README.md",
        readme(interview_id, &entry.folder_name, &seg, &phases).as_bytes(),
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
        "phases": phases,
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
            "phases": phases,
        }),
    )?;

    Ok(ExportResult {
        dir: dir.display().to_string(),
        files: w.files,
        manifest_sha256: manifest_sha,
        phases,
    })
}

fn phase_status(p: &PhaseExport) -> String {
    let title = cs_ai::phase_title(p.phase);
    if p.n_units == 0 && p.n_runs == 0 {
        return format!("{title}: not started (phase 1 must be reconciled first).\n\n");
    }
    let mut s = format!(
        "{title}: {} unit(s) in scope, {} AI run(s) recorded, {} complete, {} divergence(s). ",
        p.n_units, p.n_runs, p.n_complete_runs, p.n_divergences
    );
    if p.reconciled {
        s.push_str(
            "Reconciled: every divergence is arbitrated and the final coding is included.\n\n",
        );
    } else if p.n_complete_runs == 0 {
        s.push_str("No complete AI run yet, so no agreement, adjudication or final coding is included.\n\n");
    } else {
        s.push_str(&format!(
            "Not reconciled: {} unit(s) still pending: {}.\n\n",
            p.pending_units.len(),
            p.pending_units.join(", ")
        ));
    }
    s
}

fn readme(id: &str, folder: &str, seg: &cs_model::Segmentation, phases: &[PhaseExport]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Coding audit package, {id}\n\n"));
    s.push_str(&format!(
        "Interview folder: {folder}. Segmented source: {} (SHA-256 {}). Segmentation hash: {}. Rule: {}. Units: {}.\n\n",
        seg.source_rel_path, seg.source_sha256, seg.hash, seg.rule, seg.units.len()
    ));
    s.push_str("Coding runs in two phases. Phase 1 codes every unit against the three contexts of the TOE framework (TECH, ORG, ENV), plus CONTEXT for descriptive material about the respondent, the organisation or the scope of the account, INDUCTIVE-PENDING for relevant material outside the three contexts, and NA for the rest. Phase 2 codes only the units found relevant in the reconciled phase 1, with the study-specific codes of their contexts and the cross-cutting families. In each phase the candidate codes first, an AI coder then codes blind from a generated prompt, and every divergence is arbitrated with a written rationale.\n\n");
    for p in phases {
        s.push_str(&phase_status(p));
    }
    s.push_str("## Files\n\n");
    s.push_str("Files starting with `phase1_` or `phase2_` belong to that phase.\n\n");
    for (name, what) in [
        ("units.csv", "The units of analysis, `unit_id,speaker,timestamp,text`, as produced by the deterministic segmentation."),
        ("phase<p>_coding_human.csv", "The candidate's coding, `unit_id,codes`, codes separated by `;`, NA when no code applies."),
        ("phase<p>_human_full.csv", "The same coding with every field: sub-code, modifier, evidence level, provenance, memo and verbatim excerpt."),
        ("phase<p>_prompt_part<n>_of<m>_<hash>.txt", "Each prompt generated for the AI coder, byte for byte; the hash prefix is the start of its SHA-256."),
        ("phase<p>_run<id>_<label>.json", "Parameters of each AI run: model, version, temperature, seed, context length, batch size, prompt version, execution time."),
        ("phase<p>_coding_ai_run<id>_<label>.csv", "Codes per unit returned by that run, after validation, in the format of the human coding."),
        ("phase<p>_ai_run<id>_<label>_part<n>_raw.txt", "The AI answer exactly as pasted, before any parsing."),
        ("phase<p>_agreement.md", "Exact agreement and Cohen's kappa per code, human against each run and runs against each other."),
        ("phase<p>_adjudication_log.md", "Every divergence with the unit text, the competing codes, the decision and its rationale."),
        ("phase<p>_coding_final.csv", "Final coding of the phase: agreed units keep the common codes, divergent units take the arbitrated codes (`basis`)."),
        ("inductive_log.csv", "The candidate's inductive codes: definition, example, reason for creation and creation time."),
        ("audit_log.jsonl", "Every recorded action on this interview, one JSON event per line, each chained to the previous one by SHA-256."),
        ("audit_chain.json", "Result of the verification of the whole audit chain at export time."),
        ("manifest.json", "SHA-256 and size of every file above, with the tool version and the export time."),
    ] {
        s.push_str(&format!("- `{name}`: {what}\n"));
    }
    s.push_str("\nTo check the package, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}
